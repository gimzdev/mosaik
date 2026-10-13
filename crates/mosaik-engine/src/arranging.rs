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

use mosaik_core::arrange::{Chosen, Goes, Kind, Spot, Unit, ZoneRef, plan_with};
use mosaik_core::editing::without_zones;
use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use mosaik_core::place::{Resolver, Target, fit_size};
use mosaik_core::presets::Preset;
use mosaik_core::topology::Topology;
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::workspace::{app_match, suggested_name, workspace_id};
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ArrangePreview, ErrorCode, PreviewWindow, PreviewZone, Reply};
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

/// What Arrange is about to do, worked out without changing anything.
struct Prepared {
    preset: Option<Preset>,
    topology: Topology,
    /// The main monitor's index among the topology's.
    main: usize,
    /// The windows on screen, front to back, with their apps' names and the monitors they are on.
    windows: Vec<WindowFacts>,
    labels: Vec<String>,
    on_monitor: Vec<usize>,
    /// Where each window goes, in the order of `windows`.
    goes: Vec<Goes>,
    /// The workspace as it will be, where it goes in the list (`None`: a new one), and as it was.
    ws: Workspace,
    index: Option<usize>,
    before: Option<Workspace>,
    /// Which window fills which slot.
    assigned: HashMap<SlotId, WindowHandle>,
    /// Where the windows that fill zones go.
    moves: Vec<(WindowHandle, To)>,
    zoned: usize,
    tabbed: usize,
    made: usize,
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

    /// Works out what Arrange would do now, changing nothing: the plan, the workspace as it
    /// would be, and where each window would go. Arrange applies it; the home page shows it.
    fn prepare_arrange(&mut self, preset: Option<&str>) -> Result<Prepared, Reply> {
        let preset = match preset.map(|id| Preset::from_id(id).ok_or(id)) {
            None => None,
            Some(Ok(p)) => Some(p),
            Some(Err(id)) => {
                let known: Vec<&str> = Preset::ALL.iter().map(|p| p.id()).collect();
                return Err(error(
                    ErrorCode::BadRequest,
                    format!("There is no layout called \u{201c}{id}\u{201d}."),
                    Some(&format!("The layouts are {}.", known.join(", "))),
                ));
            }
        };
        if self.config.safe_mode {
            return Err(error(
                ErrorCode::Failed,
                "Mosaïk is in safe mode: it does not move windows.",
                Some("Restart Mosaïk normally to arrange windows."),
            ));
        }
        if let Some(problem) = &self.file_problem {
            return Err(error(
                ErrorCode::Failed,
                format!("Workspaces cannot be saved: {problem}"),
                None,
            ));
        }
        let Some(topology) = self.refresh_topology() else {
            return Err(error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            ));
        };
        let style = self.style();

        // The windows on screen, front to back; a tab group goes as its visible tab.
        let windows: Vec<WindowFacts> = self
            .scan_marked()
            .into_iter()
            .filter(|f| self.manageable(f) && !self.hiding.hidden.contains_key(&f.handle))
            .collect();
        let monitor_of = |r: Rect| {
            let m = topology.monitor_for_rect(r);
            topology.monitors().iter().position(|o| o.id == m.id).unwrap_or(0)
        };
        let main = topology
            .monitors()
            .iter()
            .position(|m| m.id == topology.primary().id)
            .unwrap_or(0);
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
        // A picked layout goes on the main monitor.
        let chosen = preset.map(|preset| Chosen {
            monitor: main,
            preset,
        });
        let plan = plan_with(&units, &spots, &monitors, chosen);

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

        // The zones that make way for a picked layout go; windows in them keep their places
        // until the plan moves them.
        if let Some(c) = chosen {
            let gone: Vec<ZoneId> = live
                .iter()
                .filter(|z| monitor_of(z.rect) == c.monitor)
                .map(|z| z.id.clone())
                .collect();
            if !gone.is_empty() {
                ws = without_zones(&ws, &gone, &topology, style);
            }
        }

        // New zones: on monitors that had none, and the picked layout's.
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
        let on_monitor: Vec<usize> = units.iter().map(|u| u.monitor).collect();
        let labels: Vec<String> = units.into_iter().map(|u| u.label).collect();
        Ok(Prepared {
            preset,
            topology,
            main,
            windows,
            labels,
            on_monitor,
            goes: plan.goes,
            ws,
            index,
            before,
            assigned,
            moves,
            zoned,
            tabbed,
            made: made.len(),
        })
    }

    /// Puts every window on screen in the zone that suits it; with `preset` (a layout's id),
    /// the main monitor takes that layout first.
    pub(crate) fn arrange(&mut self, preset: Option<&str>) -> Reply {
        let p = match self.prepare_arrange(preset) {
            Ok(p) => p,
            Err(reply) => return reply,
        };
        if p.windows.is_empty() {
            return error(
                ErrorCode::Failed,
                "There are no windows on screen to arrange.",
                Some("Open the apps you work with, then arrange them."),
            );
        }
        self.undo = Some(Undo {
            at: Instant::now(),
            workspace: p.before,
            workspace_id: p.ws.id.clone(),
            active: self.active.as_ref().map(|a| (a.id.clone(), a.windows.clone())),
            windows: p
                .windows
                .iter()
                .map(|w| (w.handle, w.frame_rect, w.maximized))
                .collect(),
        });
        let (id, name, zones_n) = (p.ws.id.clone(), p.ws.name.clone(), p.ws.layout.zones.len());
        match p.index {
            Some(i) => self.file.workspaces[i] = p.ws,
            None => self.file.workspaces.push(p.ws),
        }
        self.hide_frames();
        let placed: Vec<(WindowHandle, Rect)> = mover::move_all(&p.moves, mover::WAIT)
            .into_iter()
            .filter_map(|o| match o.result {
                Ok(wanted) => Some((o.window, wanted)),
                Err(e) => {
                    tracing::warn!(error = %e, "window not arranged");
                    None
                }
            })
            .collect();
        self.activate(id, p.assigned, placed);
        self.hiding.resume();
        self.sync_tabs();
        self.refresh_frames();
        self.set_timer(timer::SETTLE, 400);
        self.persist();
        tracing::info!(
            windows = p.windows.len(),
            zoned = p.zoned,
            tabbed = p.tabbed,
            new_zones = p.made,
            preset = p.preset.map(Preset::id),
            "windows arranged"
        );
        let mut message = match p.preset {
            Some(preset) => format!(
                "Arranged {} in the \u{201c}{}\u{201d} layout",
                plural(p.windows.len(), "window", "windows"),
                preset.label()
            ),
            None => format!(
                "Arranged {} in {}",
                plural(p.windows.len(), "window", "windows"),
                plural(zones_n, "zone", "zones")
            ),
        };
        if p.index.is_none() {
            message.push_str(&format!(" of \u{201c}{name}\u{201d}, a new workspace"));
        }
        match p.tabbed {
            0 => {}
            1 => message.push_str("; 1 window as a tab"),
            n => message.push_str(&format!("; {n} windows as tabs")),
        }
        message.push('.');
        Reply::Done { message }
    }

    /// What Arrange would do now on the main monitor, without doing it: where its windows are,
    /// where they would go, and the zones they would fill (the home page shows it while the
    /// pointer is on a layout).
    pub(crate) fn preview_arrange(&mut self, preset: Option<&str>) -> Reply {
        let p = match self.prepare_arrange(preset) {
            Ok(p) => p,
            Err(reply) => return reply,
        };
        let style = self.style();
        let Some(work_area) = p.topology.monitors().get(p.main).map(|m| m.work_area) else {
            return error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            );
        };
        let on_main = |r: Rect| p.topology.monitor_for_rect(r).id == p.topology.primary().id;
        let resolver = Resolver::new(&p.ws.layout, &p.topology, style);
        let zones: Vec<PreviewZone> =
            p.ws.layout
                .zones
                .iter()
                .filter_map(|z| {
                    let (rect, _) = resolver.zone_rect(&z.id)?;
                    on_main(rect).then(|| PreviewZone {
                        name: z.name.clone(),
                        color: z.color,
                        rect,
                    })
                })
                .collect();
        let target = |i: usize| {
            p.moves
                .iter()
                .find(|(h, _)| *h == p.windows[i].handle)
                .map(|(_, to)| match to {
                    To::Frame(r) | To::Maximized(r) => *r,
                })
        };
        // Where window `w` lands in `r`: a window of a fixed size keeps it, centred across its
        // place and at its top, as `move_frame` puts it.
        let lands = |w: &WindowFacts, r: Rect| {
            if w.resizable {
                r
            } else {
                fit_size(r, w.frame_rect.size(), work_area)
            }
        };
        let mut windows = Vec::new();
        let mut elsewhere = 0u32;
        for (i, w) in p.windows.iter().enumerate() {
            if p.on_monitor[i] != p.main {
                elsewhere += 1;
                continue;
            }
            let (then, tab) = match p.goes[i] {
                Goes::Zone(_) => (target(i), false),
                Goes::TabOf(owner) => (target(owner), true),
                Goes::Stay => (None, false),
            };
            windows.push(PreviewWindow {
                label: p.labels[i].clone(),
                now: w.frame_rect,
                then: then.map(|r| lands(w, r)),
                tab,
            });
        }
        Reply::Preview(Box::new(ArrangePreview {
            work_area,
            zones,
            windows,
            elsewhere,
        }))
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
        let at = self
            .file
            .workspaces
            .iter()
            .position(|w| w.id == undo.workspace_id);
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
        // Tabs Arrange made show again, as the windows they were (a window no longer the
        // workspace's would come back minimised); then every window goes back where it was.
        // Arrange only took windows on screen, so any of them hidden now is one of its tabs.
        let tabs_made: Vec<(WindowHandle, bool)> = undo
            .windows
            .iter()
            .filter(|(h, _, _)| self.hiding.hidden.contains_key(h))
            .map(|(h, _, _)| (*h, false))
            .collect();
        self.show_windows(&tabs_made);
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
