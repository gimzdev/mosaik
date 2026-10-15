//! Arrange: every screen laid out for what its windows say the user is doing, and Undo.
//!
//! The plan is `mosaik_core::arrange::plan`; this applies it. Every monitor with windows gets
//! the layout of its task, in place of the open workspace's zones there (a layout picked on the
//! home page does the same on the main monitor); no window gets a zone smaller than its app lets
//! it go. The open workspace takes the windows put in zones (they join it if they were not part
//! of it); with no workspace open, a new one is made from them. The other windows are minimised.
//! What Arrange changed can be undone for ten minutes, until a workspace is opened, saved or
//! edited: the workspace goes back to what it was, and every window it moved or minimised goes
//! back where it was.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mosaik_core::arrange::{Chosen, Goes, Kind, Move, Spot, Unit, plan_moved};
use mosaik_core::editing::without_zones;
use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use mosaik_core::place::{Resolver, Target, fit_size};
use mosaik_core::presets::Preset;
use mosaik_core::task::Task;
use mosaik_core::topology::Topology;
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::workspace::{app_match, suggested_name, workspace_id};
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ArrangeMove, ArrangePreview, ErrorCode, PreviewWindow, PreviewZone, Reply};
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
    /// What the main monitor's windows say the user is doing, when it is laid out for that.
    task: Option<Task>,
    /// The workspace as it will be, where it goes in the list (`None`: a new one), and as it was.
    ws: Workspace,
    index: Option<usize>,
    before: Option<Workspace>,
    /// Which window fills which slot.
    assigned: HashMap<SlotId, WindowHandle>,
    /// Where the windows that fill zones go.
    moves: Vec<(WindowHandle, To)>,
    /// The windows left over, minimised.
    minimize: Vec<WindowHandle>,
    zoned: usize,
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
    /// A workspace was opened, saved or edited: what Arrange did is that workspace's now.
    pub(crate) fn forget_arrange(&mut self) {
        self.undo = None;
    }

    /// Works out what Arrange would do now, changing nothing: the plan, the workspace as it
    /// would be, and where each window would go. Arrange applies it; the home page shows it.
    /// `moves` are the windows the user moved on the home page's picture of the main screen.
    fn prepare_arrange(&mut self, preset: Option<&str>, moves: &[ArrangeMove]) -> Result<Prepared, Reply> {
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
            })
            .collect();
        let units: Vec<Unit> = windows
            .iter()
            .map(|w| {
                let path = self.scanner.process_path(w.pid);
                // How small it goes: what its app tells Windows, or its size when it keeps one.
                let least = if w.resizable {
                    let dpi = topology.monitor_for_rect(w.frame_rect).dpi;
                    window::least_frame(window::hwnd(w.handle), w.window_rect, w.frame_rect, dpi)
                        .unwrap_or_default()
                } else {
                    w.frame_rect.size()
                };
                Unit {
                    kind: Kind::of(w.exe.as_deref(), &w.class, w.aumid.as_deref()),
                    label: self.apps.label(w, path.as_deref()),
                    frame: w.frame_rect,
                    resizable: w.resizable,
                    least,
                    monitor: monitor_of(w.frame_rect),
                }
            })
            .collect();
        let monitors: Vec<Rect> = topology.monitors().iter().map(|m| m.work_area).collect();
        // A picked layout goes on the main monitor.
        let chosen = preset.map(|preset| Chosen {
            monitor: main,
            preset,
        });
        // The picture shows the main monitor's zones, so a move's zone is one of those; a move
        // of a window that has gone since is dropped.
        let moves: Vec<Move> = moves
            .iter()
            .filter_map(|m| {
                Some(Move {
                    unit: windows.iter().position(|w| w.handle == m.window)?,
                    monitor: main,
                    zone: usize::try_from(m.zone).ok()?,
                })
            })
            .collect();
        let plan = plan_moved(&units, &spots, &monitors, chosen, &moves);

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

        // The open workspace's zones on the monitors laid out anew make way; windows in them
        // keep their places until the plan moves them.
        let gone: Vec<ZoneId> = plan
            .gone
            .iter()
            .filter_map(|&i| live.get(i).map(|z| z.id.clone()))
            .collect();
        if !gone.is_empty() {
            ws = without_zones(&ws, &gone, &topology, style);
        }

        // The plan's zones, in its order, in the colours that follow on from the zones kept (a
        // window the user moved keeps its colour: `NewZone::color`).
        let mut zone_ids: HashSet<String> = ws.layout.zones.iter().map(|z| z.id.0.clone()).collect();
        let first_color = ws.layout.zones.len();
        let mut made: Vec<Option<ZoneId>> = Vec::with_capacity(plan.new_zones.len());
        for z in &plan.new_zones {
            let Some(monitor) = topology.monitors().get(z.monitor) else {
                made.push(None);
                continue;
            };
            let id = ZoneId(fresh("z", &mut zone_ids));
            let monitor = ws.layout.monitor_index(monitor, &topology);
            ws.layout.zones.push(Zone {
                id: id.clone(),
                name: z.name.clone(),
                color: ZoneColor::nth(first_color + z.color),
                monitor,
                area: z.area,
            });
            made.push(Some(id));
        }
        // Numbered with the zones kept, in reading order.
        ws.layout.number_zones();
        let zone_id = |i: usize| made.get(i).cloned().flatten();

        // Each window's slot: the one it fills, or a new one for a window going into a zone (a
        // window minimised does not join the workspace).
        let mut assigned: HashMap<SlotId, WindowHandle> = self
            .active
            .as_ref()
            .filter(|_| index.is_some())
            .map(|a| a.windows.clone())
            .unwrap_or_default();
        let mut slot_ids: HashSet<String> = ws.layout.slots.iter().map(|s| s.id.0.clone()).collect();
        let mut slots: Vec<Option<SlotId>> = Vec::with_capacity(windows.len());
        for (i, w) in windows.iter().enumerate() {
            if let Some((s, _)) = assigned.iter().find(|(_, h)| **h == w.handle) {
                slots.push(Some(s.clone()));
                continue;
            }
            if !matches!(plan.goes.get(i), Some(Goes::Zone(_))) {
                slots.push(None);
                continue;
            }
            let id = SlotId(fresh("s", &mut slot_ids));
            let path = self.scanner.process_path(w.pid);
            ws.layout.slots.push(Slot {
                id: id.clone(),
                label: units.get(i).map(|u| u.label.clone()),
                app: app_match(w),
                launch: apps::launch_for(w, path.as_deref()),
                place: Place::Maximized { monitor: 0 },
            });
            assigned.insert(id.clone(), w.handle);
            slots.push(Some(id));
        }

        // Each window into its zone (a tab group as one); the others minimised.
        let mut zoned = 0;
        let mut minimize = Vec::new();
        for (i, goes) in plan.goes.iter().enumerate() {
            match goes {
                Goes::Zone(r) => {
                    let (Some(zone), Some(slot)) = (zone_id(*r), slots[i].as_ref()) else {
                        continue;
                    };
                    let place = Place::Zone { zone };
                    match ws.layout.group_of(slot).map(|g| g.id.clone()) {
                        Some(group) => ws.layout.move_group(&group, place),
                        None => {
                            if let Some(s) = ws.layout.slots.iter_mut().find(|s| &s.id == slot) {
                                s.place = place;
                            }
                        }
                    }
                    zoned += 1;
                }
                Goes::Minimize => minimize.push(windows[i].handle),
                Goes::Stay => {}
            }
        }

        // Where the windows that fill zones go.
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let mut moves: Vec<(WindowHandle, To)> = Vec::new();
        for (i, goes) in plan.goes.iter().enumerate() {
            if !matches!(goes, Goes::Zone(_)) {
                continue;
            }
            let Some(place) = slots[i]
                .as_ref()
                .and_then(|slot| ws.layout.slot(slot))
                .map(|s| s.place.clone())
            else {
                continue;
            };
            if let Some(Target::Frame(r)) = resolver.target(&place) {
                moves.push((windows[i].handle, To::Frame(r)));
            }
        }
        let on_monitor: Vec<usize> = units.iter().map(|u| u.monitor).collect();
        let labels: Vec<String> = units.into_iter().map(|u| u.label).collect();
        let task = plan.tasks.get(main).copied().flatten();
        Ok(Prepared {
            preset,
            topology,
            main,
            windows,
            labels,
            on_monitor,
            goes: plan.goes,
            task,
            ws,
            index,
            before,
            assigned,
            moves,
            minimize,
            zoned,
            made: made.iter().flatten().count(),
        })
    }

    /// Puts every window on screen in the zone that suits it; with `preset` (a layout's id),
    /// the main monitor takes that layout first; `moves` are the windows the user moved on the
    /// home page's picture.
    pub(crate) fn arrange(&mut self, preset: Option<&str>, moves: &[ArrangeMove]) -> Reply {
        let p = match self.prepare_arrange(preset, moves) {
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
        for h in &p.minimize {
            mosaik_win::arrange::minimize(window::hwnd(*h));
        }
        let kept: Vec<(WindowHandle, Rect, Rect)> = placed
            .iter()
            .filter_map(|&(h, kept)| {
                p.moves
                    .iter()
                    .find(|(w, _)| *w == h)
                    .and_then(|(_, to)| match to {
                        To::Frame(target) => Some((h, *target, kept)),
                        To::Maximized(_) => None,
                    })
            })
            .collect();
        self.activate(id, p.assigned, placed);
        self.hiding.resume();
        self.sync_tabs();
        // An app that kept more than it said it would: the zones make room for it.
        self.make_room(&p.topology, &kept);
        self.refresh_frames();
        self.set_timer(timer::SETTLE, 400);
        self.persist();
        tracing::info!(
            windows = p.windows.len(),
            zoned = p.zoned,
            minimized = p.minimize.len(),
            new_zones = p.made,
            preset = p.preset.map(Preset::id),
            "windows arranged"
        );
        // The windows put in zones; the ones minimised are said after.
        let placed_n = p.windows.len().saturating_sub(p.minimize.len());
        let mut message = match p.preset {
            Some(preset) => format!(
                "Arranged {} in the \u{201c}{}\u{201d} layout",
                plural(placed_n, "window", "windows"),
                preset.label()
            ),
            None => match p.task.and_then(Task::words) {
                Some(words) => format!("Arranged {} for {words}", plural(placed_n, "window", "windows")),
                None => format!(
                    "Arranged {} in {}",
                    plural(placed_n, "window", "windows"),
                    plural(zones_n, "zone", "zones")
                ),
            },
        };
        if p.index.is_none() {
            message.push_str(&format!(" of \u{201c}{name}\u{201d}, a new workspace"));
        }
        match p.minimize.len() {
            0 => {}
            1 => message.push_str("; 1 window minimized"),
            n => message.push_str(&format!("; {n} windows minimized")),
        }
        message.push('.');
        Reply::Done { message }
    }

    /// What Arrange would do now on the main monitor, without doing it: where its windows are,
    /// where they would go, and the zones they would fill (the home page shows it, with the
    /// windows the user moved on it).
    pub(crate) fn preview_arrange(&mut self, preset: Option<&str>, moves: &[ArrangeMove]) -> Reply {
        let p = match self.prepare_arrange(preset, moves) {
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
            let (then, minimized) = match p.goes[i] {
                Goes::Zone(_) => (target(i), false),
                Goes::Minimize => (None, true),
                Goes::Stay => (None, false),
            };
            let path = self.scanner.process_path(w.pid);
            windows.push(PreviewWindow {
                window: w.handle,
                label: p.labels[i].clone(),
                icon: mosaik_core::icons::key(w.aumid.as_deref(), path.as_deref()),
                now: w.frame_rect,
                then: then.map(|r| lands(w, r)),
                minimized,
                starting: false,
                tabs: Vec::new(),
            });
        }
        Reply::Preview(Box::new(ArrangePreview {
            work_area,
            zones,
            windows,
            elsewhere,
            task: p.task,
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
        // Every window back where it was, the ones minimised too.
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
