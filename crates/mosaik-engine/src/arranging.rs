//! Arrange: every window on screen in the zone that suits it, and Undo.
//!
//! The plan is `mosaik_core::arrange::plan`; this applies it. The open workspace takes the
//! windows on screen (they join it if they were not part of it); with no workspace open, a new
//! one is made from them. A monitor without zones gets a preset. Windows left over become tabs
//! where they belong. What Arrange changed can be undone for ten minutes, until a workspace is
//! opened, saved or edited: the workspace goes back to what it was, and every window it moved
//! goes back where it was.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mosaik_core::arrange::{Goes, Kind, Spot, Unit, ZoneRef, plan};
use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use mosaik_core::place::{Resolver, Target};
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::workspace::{app_match, suggested_name, workspace_id};
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ErrorCode, Reply};
use mosaik_win::{apps, window};

use crate::mover::{self, To};
use crate::thread::{Engine, timer};
use crate::workspace::live_zones;

/// How long Arrange can be undone.
const UNDO_FOR: Duration = Duration::from_secs(10 * 60);

/// What Arrange changed, to put it back.
pub(crate) struct Undo {
    at: Instant,
    /// The workspace as it was (`None`: Arrange made it).
    workspace: Option<Workspace>,
    workspace_id: WorkspaceId,
    /// The workspace open before, with the windows in its slots.
    active: Option<(WorkspaceId, HashMap<SlotId, WindowHandle>)>,
    /// Where each window Arrange moved was, and whether it was maximised.
    windows: Vec<(WindowHandle, Rect, bool)>,
}

fn error(code: ErrorCode, message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

/// The first of `prefix1`, `prefix2`... not in `taken`, which it joins.
fn fresh(prefix: &str, taken: &mut HashSet<String>) -> String {
    let id = (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id))
        .unwrap_or_else(|| format!("{prefix}0"));
    taken.insert(id.clone());
    id
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Engine {
    /// Whether Arrange can be undone now (the quick menu offers it).
    pub(crate) fn can_undo_arrange(&self) -> bool {
        self.undo.as_ref().is_some_and(|u| u.at.elapsed() < UNDO_FOR)
    }

    /// A workspace was opened, saved or edited: what Arrange did is that workspace's now.
    pub(crate) fn forget_arrange(&mut self) {
        self.undo = None;
    }

    /// Puts every window on screen in the zone that suits it.
    pub(crate) fn arrange(&mut self) -> Reply {
        if self.config.safe_mode {
            return error(
                ErrorCode::Failed,
                "Mosaïk is in safe mode: it does not move windows.",
                Some("Restart Mosaïk normally to arrange windows."),
            );
        }
        if let Some(problem) = &self.file_problem {
            return error(
                ErrorCode::Failed,
                format!("Workspaces cannot be saved: {problem}"),
                None,
            );
        }
        let Some(topology) = self.refresh_topology() else {
            return error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            );
        };
        let style = self.style();

        // The windows on screen, front to back; a tab group goes as its visible tab.
        let all = self.scan_marked();
        let windows: Vec<&WindowFacts> = all
            .iter()
            .filter(|f| self.manageable(f) && !self.hiding.hidden.contains_key(&f.handle))
            .collect();
        if windows.is_empty() {
            return error(
                ErrorCode::Failed,
                "There are no windows on screen to arrange.",
                Some("Open the apps you work with, then arrange them."),
            );
        }
        let monitor_of = |r: Rect| {
            let m = topology.monitor_for_rect(r);
            topology.monitors().iter().position(|o| o.id == m.id).unwrap_or(0)
        };
        let live = self.active.as_ref().map(|a| a.zones.clone()).unwrap_or_default();
        let spots: Vec<Spot> = live
            .iter()
            .map(|z| Spot {
                name: z.name.clone(),
                monitor: monitor_of(z.rect),
                rect: z.rect,
            })
            .collect();
        let units: Vec<Unit> = windows
            .iter()
            .map(|w| {
                let path = self.scanner.process_path(w.pid);
                Unit {
                    kind: Kind::of(w.exe.as_deref(), &w.class, w.aumid.as_deref()),
                    label: self.apps.label(w, path.as_deref()),
                    exe: w.exe.clone(),
                    frame: w.frame_rect,
                    resizable: w.resizable,
                    monitor: monitor_of(w.frame_rect),
                    zone: self
                        .zone_of(w.handle, w.frame_rect, w.resizable, style)
                        .and_then(|z| live.iter().position(|l| l.id == z.id)),
                }
            })
            .collect();
        let monitors: Vec<Rect> = topology.monitors().iter().map(|m| m.work_area).collect();
        let plan = plan(&units, &spots, &monitors);

        // The workspace that takes them: the open one, or a new one.
        let index = self
            .active
            .as_ref()
            .and_then(|a| self.file.workspaces.iter().position(|w| w.id == a.id));
        let before = index.map(|i| self.file.workspaces[i].clone());
        let mut ws = before.clone().unwrap_or_else(|| {
            let name = suggested_name(&self.file.workspaces);
            let ids: Vec<WorkspaceId> = self.file.workspaces.iter().map(|w| w.id.clone()).collect();
            let mut layout = Layout::default();
            for m in topology.monitors() {
                layout.monitor_index(m, &topology);
            }
            Workspace {
                id: workspace_id(&name, &ids),
                name,
                hotkey: None,
                layout,
            }
        });

        // New zones on monitors that had none.
        let mut zone_ids: HashSet<String> = ws.layout.zones.iter().map(|z| z.id.0.clone()).collect();
        let mut made: Vec<ZoneId> = Vec::with_capacity(plan.new_zones.len());
        for z in &plan.new_zones {
            let id = ZoneId(fresh("z", &mut zone_ids));
            let Some(monitor) = topology.monitors().get(z.monitor) else {
                continue;
            };
            let monitor = ws.layout.monitor_index(monitor, &topology);
            ws.layout.zones.push(Zone {
                id: id.clone(),
                name: z.name.clone(),
                color: ZoneColor::nth(ws.layout.zones.len()),
                monitor,
                area: z.area,
            });
            made.push(id);
        }
        let zone_id = |r: ZoneRef| match r {
            ZoneRef::Existing(i) => live.get(i).map(|z| z.id.clone()),
            ZoneRef::New(i) => made.get(i).cloned(),
        };

        // Each window's slot: the one it fills, or a new one.
        let mut assigned: HashMap<SlotId, WindowHandle> = self
            .active
            .as_ref()
            .filter(|_| index.is_some())
            .map(|a| a.windows.clone())
            .unwrap_or_default();
        let mut slot_ids: HashSet<String> = ws.layout.slots.iter().map(|s| s.id.0.clone()).collect();
        let mut slots: Vec<SlotId> = Vec::with_capacity(windows.len());
        for w in &windows {
            if let Some((s, _)) = assigned.iter().find(|(_, h)| **h == w.handle) {
                slots.push(s.clone());
                continue;
            }
            let id = SlotId(fresh("s", &mut slot_ids));
            let path = self.scanner.process_path(w.pid);
            ws.layout.slots.push(Slot {
                id: id.clone(),
                app: app_match(w),
                launch: apps::launch_for(w, path.as_deref()),
                place: Place::Maximized { monitor: 0 },
            });
            assigned.insert(id.clone(), w.handle);
            slots.push(id);
        }

        // Zones first, then tabs (a tab joins the window filling its zone, wherever that is in
        // the order).
        let mut zoned = 0;
        for (i, goes) in plan.goes.iter().enumerate() {
            let Goes::Zone(r) = goes else { continue };
            let Some(zone) = zone_id(*r) else { continue };
            let place = Place::Zone { zone };
            match ws.layout.group_of(&slots[i]).map(|g| g.id.clone()) {
                Some(group) => ws.layout.move_group(&group, place),
                None => {
                    if let Some(s) = ws.layout.slots.iter_mut().find(|s| s.id == slots[i]) {
                        s.place = place;
                    }
                }
            }
            zoned += 1;
        }
        let mut tabbed = 0;
        for (i, goes) in plan.goes.iter().enumerate() {
            let Goes::TabOf(owner) = goes else { continue };
            let Some(onto) = slots.get(*owner).cloned() else {
                continue;
            };
            // A tab group joins as all of its tabs, in their order.
            let tabs = ws
                .layout
                .group_of(&slots[i])
                .map_or_else(|| vec![slots[i].clone()], |g| g.tabs.clone());
            for t in tabs {
                if ws.layout.join_tab(&onto, &t, None).is_some() {
                    tabbed += 1;
                }
            }
            // The window that fills the zone stays the one showing.
            ws.layout.show_tab(&onto);
        }

        // Where the windows that fill zones go.
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let mut moves: Vec<(WindowHandle, To)> = Vec::new();
        for (i, goes) in plan.goes.iter().enumerate() {
            if !matches!(goes, Goes::Zone(_)) {
                continue;
            }
            let Some(place) = ws.layout.slot(&slots[i]).map(|s| s.place.clone()) else {
                continue;
            };
            let h = windows[i].handle;
            let banded = resolver
                .zone_of(&place)
                .and_then(|(zone, dpi)| self.banded_area(h, zone, dpi));
            match (banded, resolver.target(&place)) {
                (Some(area), _) => moves.push((h, To::Frame(area))),
                (None, Some(Target::Frame(r))) => moves.push((h, To::Frame(r))),
                _ => {}
            }
        }

        self.undo = Some(Undo {
            at: Instant::now(),
            workspace: before,
            workspace_id: ws.id.clone(),
            active: self.active.as_ref().map(|a| (a.id.clone(), a.windows.clone())),
            windows: windows
                .iter()
                .map(|w| (w.handle, w.frame_rect, w.maximized))
                .collect(),
        });
        let (id, name, zones_n) = (ws.id.clone(), ws.name.clone(), ws.layout.zones.len());
        match index {
            Some(i) => self.file.workspaces[i] = ws,
            None => self.file.workspaces.push(ws),
        }
        self.hide_frames();
        let placed: Vec<(WindowHandle, Rect)> = mover::move_all(&moves, mover::WAIT)
            .into_iter()
            .filter_map(|o| match o.result {
                Ok(wanted) => Some((o.window, wanted)),
                Err(e) => {
                    tracing::warn!(error = %e, "window not arranged");
                    None
                }
            })
            .collect();
        self.activate(id, assigned, placed);
        self.hiding.resume();
        self.sync_tabs();
        self.refresh_frames();
        self.set_timer(timer::SETTLE, 400);
        self.persist();
        tracing::info!(
            windows = windows.len(),
            zoned,
            tabbed,
            new_zones = made.len(),
            "windows arranged"
        );
        let mut message = format!(
            "Arranged {} in {}",
            plural(windows.len(), "window", "windows"),
            plural(zones_n, "zone", "zones")
        );
        if index.is_none() {
            message.push_str(&format!(" of \u{201c}{name}\u{201d}, a new workspace"));
        }
        if tabbed > 0 {
            message.push_str(&format!("; {} as tabs", plural(tabbed, "window", "windows")));
        }
        message.push('.');
        Reply::Done { message }
    }

    /// Puts back what Arrange changed: the workspace as it was, the one open before, and every
    /// window it moved where it was.
    pub(crate) fn undo_arrange(&mut self) -> Reply {
        let Some(undo) = self.undo.take().filter(|u| u.at.elapsed() < UNDO_FOR) else {
            return error(
                ErrorCode::NotFound,
                "There is no Arrange to undo.",
                Some("Arrange can be undone for ten minutes, until a workspace is opened, saved or edited."),
            );
        };
        let at = self.file.workspaces.iter().position(|w| w.id == undo.workspace_id);
        match (undo.workspace, at) {
            (Some(before), Some(i)) => self.file.workspaces[i] = before,
            (Some(before), None) => self.file.workspaces.push(before),
            (None, Some(i)) => {
                self.file.workspaces.remove(i);
            }
            (None, None) => {}
        }
        self.hide_frames();
        match undo.active {
            Some((id, windows)) => self.activate(id, windows, Vec::new()),
            None => {
                self.active = None;
                self.file.active = None;
            }
        }
        // Tabs Arrange made show again; then every window goes back where it was.
        self.hiding.resume();
        self.sync_tabs();
        let topology = self.refresh_topology();
        let moves: Vec<(WindowHandle, To)> = undo
            .windows
            .iter()
            .filter(|(h, _, _)| window::exists(window::hwnd(*h)))
            .map(|&(h, frame, maximized)| match (&topology, maximized) {
                (Some(t), true) => (h, To::Maximized(t.monitor_for_rect(frame).work_area)),
                _ => (h, To::Frame(frame)),
            })
            .collect();
        for o in mover::move_all(&moves, mover::WAIT) {
            if let Err(e) = o.result {
                tracing::debug!(error = %e, "window not put back");
            }
        }
        if let (Some(t), Some(a)) = (&topology, &self.active)
            && let Some(ws) = self.file.workspaces.iter().find(|w| w.id == a.id)
        {
            let zones = live_zones(ws, t, self.style());
            if let Some(a) = &mut self.active {
                a.zones = zones;
            }
        }
        self.refresh_frames();
        self.persist();
        tracing::info!(windows = moves.len(), "arrange undone");
        Reply::Done {
            message: "Windows are back where they were.".into(),
        }
    }
}
