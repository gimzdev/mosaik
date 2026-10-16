//! Arrange: every screen laid out for what its windows say the user is doing, and Undo.
//!
//! The plan is `mosaik_core::arrange::plan`; this applies it. Every monitor with windows gets
//! the layout of its task, in place of the zones on screen there (a layout picked on the home
//! page does the same on the main monitor); no window gets a zone smaller than its app lets it
//! go. The other windows are minimised. Arrange arranges and Save saves (ADR 0030): no workspace
//! is made or changed. The zones it puts on screen, with the windows in them, are kept apart
//! (`Arranged`) for tags, snapping and tabs, until a workspace is opened or saved; the workspace
//! in use stays the one Save updates. What Arrange did can be undone for ten minutes, until a
//! workspace is saved or edited: every window it moved or minimised goes back where it was, and
//! what was on screen before is again.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mosaik_core::arrange::{Chosen, Goes, KeptZone, Kind, Move, Unit, plan_moved};
use mosaik_core::editing::{stretch_zones, without_zones};
use mosaik_core::geometry::{RatioRect, Rect};
use mosaik_core::layout::{Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use mosaik_core::place::{Resolver, Target, fit_size};
use mosaik_core::presets::Preset;
use mosaik_core::task::Task;
use mosaik_core::topology::Topology;
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::workspace::app_match;
use mosaik_core::zone::{Zone, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ArrangeMove, ArrangePreview, ErrorCode, PreviewWindow, PreviewZone, Reply};
use mosaik_win::{apps, window};

use crate::mover::{self, To};
use crate::thread::{Engine, timer};
use crate::workspace::{Arranged, live_zones};

/// How long Arrange, or opening a workspace, can be undone.
const UNDO_FOR: Duration = Duration::from_secs(10 * 60);

/// What Arrange, or opening a workspace, changed, to put it back.
pub(crate) struct Undo {
    pub(crate) at: Instant,
    /// The workspace opened, as it was (opening it can make room in its zones); Arrange changes
    /// none.
    pub(crate) workspace: Option<Workspace>,
    /// What Arrange had put on screen before.
    pub(crate) arranged: Option<Arranged>,
    /// What was open before (a workspace, or `arranged`), with the windows in its slots.
    pub(crate) active: Option<(WorkspaceId, HashMap<SlotId, WindowHandle>)>,
    /// Where each window moved or minimised was, and whether it was maximised.
    pub(crate) windows: Vec<(WindowHandle, Rect, bool)>,
    /// Windows that were minimised before (opening a workspace brings its own back): minimised
    /// again.
    pub(crate) minimized: Vec<WindowHandle>,
    /// The windows Arrange stacked in a zone, each zone's as they were, front to back: stacked
    /// so again (ADR 0032).
    pub(crate) stacked: Vec<Vec<WindowHandle>>,
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
    /// The windows front to back as Mosaïk stacks them in their zones (indexes into `windows`):
    /// each zone's own window, then those stacked behind it, as the user's trades left them
    /// (ADR 0032).
    order: Vec<usize>,
    /// What the main monitor's windows say the user is doing, when it is laid out for that.
    task: Option<Task>,
    /// What will be on screen, and the saved workspace in use (which stays so).
    ws: Workspace,
    from: Option<WorkspaceId>,
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

/// The windows of each zone that has more than one, in two orders: front to back as Arrange
/// stacks them (`order`), and as they are on screen now (`windows`, front to back), for Undo.
fn zone_stacks(p: &Prepared) -> Vec<(Vec<WindowHandle>, Vec<WindowHandle>)> {
    let zone = |i: usize| match p.goes.get(i) {
        Some(Goes::Zone(r)) => Some(*r),
        _ => None,
    };
    let mut zones: Vec<usize> = (0..p.windows.len()).filter_map(zone).collect();
    zones.sort_unstable();
    zones.dedup();
    zones
        .into_iter()
        .map(|r| {
            let handles = |list: &mut dyn Iterator<Item = usize>| -> Vec<WindowHandle> {
                list.filter(|&i| zone(i) == Some(r))
                    .filter_map(|i| p.windows.get(i).map(|w| w.handle))
                    .collect()
            };
            (
                handles(&mut p.order.iter().copied()),
                handles(&mut (0..p.windows.len())),
            )
        })
        .filter(|(stacked, _)| stacked.len() >= 2)
        .collect()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Engine {
    /// A workspace was saved or edited: what Arrange, or opening a workspace, did stays done.
    pub(crate) fn forget_undo(&mut self) {
        self.undo = None;
    }

    /// Works out what Arrange would do now, changing nothing: the plan, the zones as they would
    /// be, and where each window would go. Arrange applies it; the home page shows it.
    /// `moves` are the windows the user moved on the home page's picture of the main screen,
    /// `areas` the main screen's zones as the user stretched them there.
    fn prepare_arrange(
        &mut self,
        preset: Option<&str>,
        moves: &[ArrangeMove],
        areas: &[RatioRect],
    ) -> Result<Prepared, Reply> {
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
                Some("Restart Mosaïk normally to move windows."),
            ));
        }
        // Arrange saves nothing (ADR 0030): a workspaces file that cannot be written does not
        // stop it.
        let Some(topology) = self.read_topology() else {
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
        let spots: Vec<KeptZone> = live
            .iter()
            .map(|z| KeptZone {
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
                    self.least(w.handle, w.window_rect, w.frame_rect, dpi)
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
        // of a window that has gone since is dropped, and one dropped onto a window gone since
        // goes onto its zone (ADR 0032).
        let index_of = |h: WindowHandle| windows.iter().position(|w| w.handle == h);
        let moves: Vec<Move> = moves
            .iter()
            .filter_map(|m| {
                Some(Move {
                    unit: index_of(m.window)?,
                    monitor: main,
                    zone: usize::try_from(m.zone).ok()?,
                    onto: m.onto.and_then(index_of),
                })
            })
            .collect();
        let plan = plan_moved(&units, &spots, &monitors, chosen, &moves);

        // What will be on screen: the zones there now, or none; no saved workspace changes.
        let on_screen = self.on_screen();
        let mut ws = Workspace {
            id: WorkspaceId::arranged(),
            name: "Arranged".to_owned(),
            hotkey: None,
            layout: on_screen.map_or_else(
                || {
                    let mut layout = Layout::default();
                    for m in topology.monitors() {
                        layout.monitor_index(m, &topology);
                    }
                    layout
                },
                |w| w.layout.clone(),
            ),
        };
        let from = self.in_use().cloned();

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
        // The main monitor's zones as the user stretched them on the picture (ADR 0031): taken
        // with one area for each, none too small and none over another.
        if !areas.is_empty()
            && let Some(m) = topology.monitors().get(main)
        {
            let index = ws.layout.monitor_index(m, &topology);
            if !stretch_zones(&mut ws.layout, index, areas) {
                tracing::debug!(areas = areas.len(), "stretched zones not taken");
            }
        }
        // Numbered with the zones kept, in reading order, where they are once stretched.
        ws.layout.number_zones();
        let zone_id = |i: usize| made.get(i).cloned().flatten();

        // Each window's slot: the one it fills, or a new one for a window going into a zone (a
        // window minimised does not join the workspace).
        let mut assigned: HashMap<SlotId, WindowHandle> = self
            .active
            .as_ref()
            .filter(|_| on_screen.is_some())
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
            order: plan.order,
            task,
            ws,
            from,
            assigned,
            moves,
            minimize,
            zoned,
            made: made.iter().flatten().count(),
        })
    }

    /// Puts every window on screen in the zone that suits it; with `preset` (a layout's id),
    /// the main monitor takes that layout first; `moves` are the windows the user moved on the
    /// home page's picture, `areas` its zones as the user stretched them.
    pub(crate) fn arrange(
        &mut self,
        preset: Option<&str>,
        moves: &[ArrangeMove],
        areas: &[RatioRect],
    ) -> Reply {
        let p = match self.prepare_arrange(preset, moves, areas) {
            Ok(p) => p,
            Err(reply) => return reply,
        };
        if p.windows.is_empty() {
            return error(
                ErrorCode::Failed,
                "There are no windows on screen to move.",
                Some("Open the apps you work with, then arrange them."),
            );
        }
        let stacks = zone_stacks(&p);
        self.undo = Some(Undo {
            at: Instant::now(),
            workspace: None,
            arranged: self.arranged.clone(),
            active: self.active.as_ref().map(|a| (a.id.clone(), a.windows.clone())),
            windows: p
                .windows
                .iter()
                .map(|w| (w.handle, w.frame_rect, w.maximized))
                .collect(),
            minimized: Vec::new(),
            stacked: stacks.iter().map(|(_, before)| before.clone()).collect(),
        });
        let id = p.ws.id.clone();
        self.arranged = Some(Arranged {
            ws: p.ws,
            from: p.from,
        });
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
        // The windows of a zone, each zone's own window in front, as the picture showed them
        // (ADR 0032): each just below the one before, so none comes up over Mosaïk's window.
        for (order, _) in &stacks {
            if !mover::stack(order, mover::WAIT_ONE) {
                tracing::warn!(windows = order.len(), "windows not stacked in time");
            }
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
        tracing::info!(
            windows = p.windows.len(),
            zoned = p.zoned,
            minimized = p.minimize.len(),
            new_zones = p.made,
            preset = p.preset.map(Preset::id),
            "windows arranged"
        );
        // In short sentences: what moved, and what went to the taskbar. No workspace changed.
        let placed_n = p.windows.len().saturating_sub(p.minimize.len());
        let mut message = match p.preset {
            Some(preset) => format!(
                "Moved {} into {}.",
                plural(placed_n, "window", "windows"),
                preset.label()
            ),
            None => format!("Moved {}.", plural(placed_n, "window", "windows")),
        };
        match p.minimize.len() {
            0 => {}
            1 => message.push_str(" 1 window minimized."),
            n => message.push_str(&format!(" {n} windows minimized.")),
        }
        Reply::Done { message }
    }

    /// What Arrange would do now on the main monitor, without doing it: where its windows are,
    /// where they would go, and the zones they would fill (the home page shows it, with the
    /// windows the user moved on it).
    pub(crate) fn preview_arrange(
        &mut self,
        preset: Option<&str>,
        moves: &[ArrangeMove],
        areas: &[RatioRect],
    ) -> Reply {
        let p = match self.prepare_arrange(preset, moves, areas) {
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
                        id: Some(z.id.clone()),
                        name: z.name.clone(),
                        color: z.color,
                        rect,
                        area: Some(z.area),
                    })
                })
                .collect();
        // The zone each window goes in, by its slot there.
        let zone_of = |h: WindowHandle| {
            let (slot, _) = p.assigned.iter().find(|(_, w)| **w == h)?;
            match p.ws.layout.effective_place(p.ws.layout.slot(slot)?)? {
                Place::Zone { zone } => Some(zone.clone()),
                _ => None,
            }
        };
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
        // Front to back as Mosaïk stacks them: each zone's own window first (ADR 0032).
        for &i in &p.order {
            let w = &p.windows[i];
            if p.on_monitor[i] != p.main {
                elsewhere += 1;
                continue;
            }
            let (then, minimized, zone) = match p.goes[i] {
                Goes::Zone(_) => (target(i), false, zone_of(w.handle)),
                Goes::Minimize => (None, true, None),
                Goes::Stay => (None, false, None),
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
                zone,
                app: None,
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

    /// Puts back what Arrange, or opening a workspace, changed: the workspace opened as it was,
    /// what was on screen before, and every window it moved or minimised where it was.
    pub(crate) fn undo_arrange(&mut self) -> Reply {
        let Some(undo) = self.undo.take().filter(|u| u.at.elapsed() < UNDO_FOR) else {
            return error(
                ErrorCode::NotFound,
                "There is nothing to undo.",
                Some(
                    "Arrange, and opening a workspace, can be undone for ten minutes, until a workspace is saved or edited.",
                ),
            );
        };
        // A workspace deleted since is not brought back (deleting one forgets the undo anyway).
        let mut put_back = false;
        if let Some(before) = undo.workspace
            && let Some(w) = self.file.workspaces.iter_mut().find(|w| w.id == before.id)
        {
            *w = before;
            put_back = true;
        }
        self.arranged = undo.arranged;
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
        let topology = self.read_topology();
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
        for h in &undo.minimized {
            let hwnd = window::hwnd(*h);
            if window::exists(hwnd) {
                mosaik_win::arrange::minimize(hwnd);
            }
        }
        // The windows Arrange stacked in a zone, in the order they had (ADR 0032).
        for before in &undo.stacked {
            let open: Vec<WindowHandle> = before
                .iter()
                .copied()
                .filter(|h| window::exists(window::hwnd(*h)))
                .collect();
            if !mover::stack(&open, mover::WAIT_ONE) {
                tracing::debug!(windows = open.len(), "windows not stacked back in time");
            }
        }
        if let (Some(t), Some(ws)) = (&topology, self.on_screen()) {
            let zones = live_zones(ws, t, self.style());
            if let Some(a) = &mut self.active {
                a.zones = zones;
            }
        }
        self.refresh_frames();
        if put_back {
            self.persist();
        }
        tracing::info!(windows = moves.len(), "arrange undone");
        Reply::Done {
            message: "Windows are back where they were.".into(),
        }
    }
}
