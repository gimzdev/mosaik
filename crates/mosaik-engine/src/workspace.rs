//! Workspaces at run time: saving the windows on screen as a workspace, opening one, starting
//! the apps it needs, and remembering where its windows are as the user moves them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Pending, Verdict, classify};
use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Place, SlotId, Workspace, WorkspaceId};
use mosaik_core::matching::AppMatch;
use mosaik_core::place::{Resolver, Target};
use mosaik_core::settings::SwitchSettings;
use mosaik_core::store::StoreError;
use mosaik_core::topology::Topology;
use mosaik_core::window::{Cloak, WindowFacts, WindowHandle};
use mosaik_core::workspace::{Present, Seen, capture, edge_distance, plan_switch, remember, workspace_id};
use mosaik_core::zone::{FrameStyle, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ErrorCode, Reply};
use mosaik_win::{apps, arrange, place, window};

use crate::thread::{Engine, timer};

/// How long an app started for a workspace has to show its window.
const LAUNCH_WAIT: Duration = Duration::from_secs(30);

/// A zone of the open workspace, where it is on screen now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveZone {
    pub id: ZoneId,
    pub rect: Rect,
    pub dpi: u32,
    pub color: ZoneColor,
    pub name: String,
}

/// An app started for the open workspace, waiting for its window.
#[derive(Clone, Debug)]
pub(crate) struct Launching {
    pub slot: SlotId,
    pub app: AppMatch,
    pub until: Instant,
}

/// The workspace in use.
pub(crate) struct Active {
    pub id: WorkspaceId,
    /// The window filling each slot.
    pub windows: HashMap<SlotId, WindowHandle>,
    pub zones: Vec<LiveZone>,
    /// Windows just moved, with where they were meant to land, checked again by `settle`.
    pub placed: Vec<(WindowHandle, Rect)>,
    pub launching: Vec<Launching>,
}

impl Active {
    pub fn slot_of(&self, h: WindowHandle) -> Option<&SlotId> {
        self.windows.iter().find(|(_, w)| **w == h).map(|(s, _)| s)
    }

    /// The zone a window at `frame` sits in: its centre is in the zone, and the window and the
    /// zone's window area mostly cover each other. That holds for a window Mosaïk put there,
    /// including an app that kept a size of its own a little larger than the zone, but not for
    /// a small window floating over the zone or a large one spanning two zones.
    pub fn zone_for(&self, frame: Rect, style: FrameStyle) -> Option<&LiveZone> {
        self.zones
            .iter()
            .filter(|z| z.rect.contains(frame.center()))
            .filter(|z| {
                let area = style.window_area(z.rect, z.dpi);
                let shared = frame.overlap_area(area);
                let larger = frame.area().max(area.area()).max(1);
                // Pixel areas are far below 2^52.
                #[allow(clippy::cast_precision_loss)]
                let fraction = shared as f64 / larger as f64;
                fraction >= 0.7
            })
            .min_by_key(|z| z.rect.area())
    }
}

/// The zones of `ws` on the monitors connected now.
fn live_zones(ws: &Workspace, topology: &Topology, style: FrameStyle) -> Vec<LiveZone> {
    let resolver = Resolver::new(&ws.layout, topology, style);
    ws.layout
        .zones
        .iter()
        .filter_map(|z| {
            resolver.zone_rect(&z.id).map(|(rect, dpi)| LiveZone {
                id: z.id.clone(),
                rect,
                dpi,
                color: z.color,
                name: z.name.clone(),
            })
        })
        .collect()
}

fn error(code: ErrorCode, message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

impl Engine {
    pub(crate) fn style(&self) -> FrameStyle {
        self.settings.frames.effective_style()
    }

    fn allowed(&self, f: &WindowFacts) -> bool {
        self.config
            .manage_only
            .as_ref()
            .is_none_or(|pids| pids.contains(&f.pid))
    }

    /// A window Mosaïk manages now: on screen, an app window, not elevated.
    pub(crate) fn manageable(&self, f: &WindowFacts) -> bool {
        self.allowed(f) && classify(f, &self.settings.rules) == Verdict::Manage
    }

    /// A window a workspace may move: a managed one, or a minimised one it restores.
    fn movable(&self, f: &WindowFacts) -> bool {
        self.allowed(f)
            && matches!(
                classify(f, &self.settings.rules),
                Verdict::Manage | Verdict::Pending(Pending::Minimized)
            )
    }

    /// Reads the monitors again; keeps the last known arrangement if Windows cannot say.
    pub(crate) fn refresh_topology(&mut self) -> Option<Topology> {
        match mosaik_win::monitors::topology() {
            Ok(t) => {
                self.topology = Some(t.clone());
                Some(t)
            }
            Err(e) => {
                tracing::warn!(error = %e, "monitors not read");
                self.topology.clone()
            }
        }
    }

    /// `workspaces.json` could not be read at start.
    pub(crate) fn file_locked(&mut self, problem: &StoreError) {
        let message = problem.to_string();
        match problem {
            // The damaged file was kept aside under another name: saving a new one is safe.
            StoreError::Damaged { .. } => self.notice("Your workspaces could not be read", message),
            // A newer format or a file Windows would not let Mosaïk read: saving would replace
            // it, so nothing is saved this session.
            _ => {
                self.notice(
                    "Your workspaces could not be read",
                    format!("{message} Mosaïk will not save workspaces until this is fixed, so nothing is overwritten."),
                );
                self.file_problem = Some(message);
            }
        }
    }

    /// Writes `workspaces.json`.
    pub(crate) fn persist(&mut self) {
        if self.file_problem.is_some() {
            return;
        }
        if let Err(e) = self.store.save(&self.file) {
            tracing::error!(error = %e, "workspaces not saved");
            if !self.save_failed_told {
                self.save_failed_told = true;
                self.notice("Your workspaces could not be saved", e.to_string());
            }
        }
    }

    fn find_workspace(&self, which: &str) -> Option<usize> {
        let w = which.trim();
        self.file
            .workspaces
            .iter()
            .position(|ws| ws.id.0 == w)
            .or_else(|| {
                let lower = w.to_lowercase();
                self.file
                    .workspaces
                    .iter()
                    .position(|ws| ws.name.trim().to_lowercase() == lower)
            })
    }

    /// Makes `id` the open workspace with these windows in its slots, and shows its frames.
    fn activate(
        &mut self,
        id: WorkspaceId,
        windows: HashMap<SlotId, WindowHandle>,
        placed: Vec<(WindowHandle, Rect)>,
    ) {
        let zones = match (&self.topology, self.file.workspaces.iter().find(|w| w.id == id)) {
            (Some(t), Some(ws)) => live_zones(ws, t, self.style()),
            _ => Vec::new(),
        };
        self.active = Some(Active {
            id: id.clone(),
            windows,
            zones,
            placed,
            launching: Vec::new(),
        });
        self.file.active = Some(id);
        self.refresh_frames();
    }

    /// The open workspace remembers where its windows are now (when it is left, after the user
    /// moves one of its windows, and before Mosaïk exits).
    pub(crate) fn remember_active(&mut self) {
        let Some(active) = &self.active else { return };
        let Some(topology) = self.topology.clone() else {
            return;
        };
        let Some(index) = self.file.workspaces.iter().position(|w| w.id == active.id) else {
            return;
        };
        let by_window: HashMap<WindowHandle, SlotId> =
            active.windows.iter().map(|(s, w)| (*w, s.clone())).collect();
        // Front to back, as Windows stacks them.
        let mut facts: Vec<(SlotId, WindowFacts)> = Vec::new();
        for hwnd in window::top_level() {
            let h = window::handle(hwnd);
            if let Some(slot) = by_window.get(&h)
                && let Some(f) = self.scanner.facts(hwnd)
                && f.cloak != Cloak::ByShell
            {
                facts.push((slot.clone(), f));
            }
        }
        let present: Vec<Present<'_>> = facts
            .iter()
            .map(|(slot, f)| Present {
                slot: slot.clone(),
                window: f,
            })
            .collect();
        let updated = remember(&self.file.workspaces[index], &present, &topology, self.style());
        if updated != self.file.workspaces[index] {
            self.file.workspaces[index] = updated;
            self.persist();
        }
    }

    /// Saves the windows on screen as the workspace `name` (updating the one with that name, if
    /// any), makes it the open one and puts its windows in their zones.
    pub(crate) fn save(&mut self, name: Option<String>) -> Reply {
        if self.config.safe_mode {
            return error(
                ErrorCode::Failed,
                "Mosaïk is in safe mode: it does not move windows, so it does not save workspaces either.",
                Some("Restart Mosaïk normally to save workspaces."),
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
        let name = name
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .or_else(|| {
                self.active
                    .as_ref()
                    .and_then(|a| self.file.workspaces.iter().find(|w| w.id == a.id))
                    .map(|w| w.name.clone())
            })
            .unwrap_or_else(|| mosaik_core::workspace::suggested_name(&self.file.workspaces));
        let name: String = name.chars().take(mosaik_core::switcher::MAX_NAME).collect();

        // The workspace being left keeps its own windows' places.
        let previous_index = self.find_workspace(&name).filter(|i| {
            // Only an exact name match updates; an id typed by hand does too.
            let ws = &self.file.workspaces[*i];
            ws.name.trim().eq_ignore_ascii_case(name.trim()) || ws.id.0 == name.trim()
        });
        if self
            .active
            .as_ref()
            .is_some_and(|a| previous_index.is_none_or(|i| self.file.workspaces[i].id != a.id))
        {
            self.remember_active();
        }

        let windows: Vec<WindowFacts> = self
            .scanner
            .scan()
            .into_iter()
            .filter(|f| self.manageable(f))
            .collect();
        if windows.is_empty() {
            return error(
                ErrorCode::Failed,
                "There are no windows on screen to save.",
                Some("Open and arrange the apps for this project, then save again."),
            );
        }
        let mut seen = Vec::with_capacity(windows.len());
        for w in &windows {
            let path = self.scanner.process_path(w.pid);
            seen.push(Seen {
                facts: w,
                label: self.apps.label(w, path.as_deref()),
                launch: apps::launch_for(w, path.as_deref()),
            });
        }
        let previous = previous_index.map(|i| self.file.workspaces[i].clone());
        let id = previous.as_ref().map_or_else(
            || {
                let ids: Vec<WorkspaceId> = self.file.workspaces.iter().map(|w| w.id.clone()).collect();
                workspace_id(&name, &ids)
            },
            |p| p.id.clone(),
        );
        let style = self.style();
        let ws = capture(
            id.clone(),
            name.clone(),
            &seen,
            &topology,
            style,
            previous.as_ref(),
        );
        let assigned: HashMap<SlotId, WindowHandle> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .map(|(s, w)| (s.id.clone(), w.handle))
            .collect();
        // Windows in zones move inside them, below their tags.
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let targets: Vec<(WindowHandle, Rect)> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .filter(|(s, _)| matches!(s.place, Place::Zone { .. }))
            .filter_map(|(s, w)| match resolver.target(&s.place) {
                Some(Target::Frame(r)) => Some((w.handle, r)),
                _ => None,
            })
            .collect();
        let (windows_n, zones_n) = (ws.layout.slots.len(), ws.layout.zones.len());
        match previous_index {
            Some(i) => self.file.workspaces[i] = ws,
            None => self.file.workspaces.push(ws),
        }
        self.hide_frames();
        let mut placed = Vec::new();
        for (h, r) in targets {
            let hwnd = window::hwnd(h);
            if arrange::is_hung(hwnd) {
                continue;
            }
            match place::move_frame(hwnd, r) {
                Ok(moved) => placed.push((h, moved.wanted)),
                Err(e) => tracing::warn!(error = %e, "window not put in its zone"),
            }
        }
        self.activate(id, assigned, placed);
        self.persist();
        self.set_timer(timer::SETTLE, 400);
        tracing::info!(
            windows = windows_n,
            zones = zones_n,
            updated = previous_index.is_some(),
            "workspace saved"
        );
        Reply::Done {
            message: format!(
                "Saved \u{201c}{name}\u{201d}: {windows_n} {}, {zones_n} {}.",
                if windows_n == 1 { "window" } else { "windows" },
                if zones_n == 1 { "zone" } else { "zones" }
            ),
        }
    }

    /// Opens a workspace (by id or name): its windows go to their places, missing apps start,
    /// the previous workspace's other windows are minimised.
    pub(crate) fn switch(&mut self, which: &str) -> Reply {
        if self.config.safe_mode {
            return error(
                ErrorCode::Failed,
                "Mosaïk is in safe mode: it does not move windows.",
                Some("Restart Mosaïk normally to switch workspaces."),
            );
        }
        let Some(index) = self.find_workspace(which) else {
            let names: Vec<String> = self
                .file
                .workspaces
                .iter()
                .map(|w| format!("\u{201c}{}\u{201d}", w.name))
                .collect();
            return error(
                ErrorCode::NotFound,
                format!("There is no workspace called \u{201c}{}\u{201d}.", which.trim()),
                Some(&if names.is_empty() {
                    "No workspace is saved yet: arrange your windows, open the switcher and press S."
                        .to_owned()
                } else {
                    format!("Saved workspaces: {}.", names.join(", "))
                }),
            );
        };
        let started = Instant::now();
        let Some(topology) = self.refresh_topology() else {
            return error(
                ErrorCode::Failed,
                "Mosaïk could not read the monitor layout.",
                None,
            );
        };
        self.remember_active();
        let previous: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let ws = self.file.workspaces[index].clone();
        let all = self.scanner.scan();
        let candidates: Vec<&WindowFacts> = all.iter().filter(|f| self.movable(f)).collect();
        let plan = plan_switch(
            &ws,
            &candidates,
            &previous,
            &topology,
            self.style(),
            self.settings.switching,
        );

        self.hide_frames();
        let mut assigned: HashMap<SlotId, WindowHandle> = HashMap::new();
        let mut placed = Vec::new();
        let mut trouble: Vec<String> = Vec::new();
        let label = |slot: &SlotId| -> String {
            ws.layout
                .slot(slot)
                .and_then(|s| s.app.exe.clone().or_else(|| s.app.aumid.clone()))
                .unwrap_or_else(|| "a window".to_owned())
        };
        for m in &plan.moves {
            let hwnd = window::hwnd(m.window);
            if arrange::is_hung(hwnd) {
                trouble.push(format!("{} is not responding", label(&m.slot)));
                continue;
            }
            let result = match &m.target {
                Target::Frame(r) => place::move_frame(hwnd, *r).map(|moved| {
                    placed.push((m.window, moved.wanted));
                }),
                Target::Maximized(monitor) => match topology.get(monitor) {
                    Some(mon) => place::maximize_on(hwnd, mon.work_area),
                    None => Ok(()),
                },
            };
            match result {
                Ok(()) => {
                    assigned.insert(m.slot.clone(), m.window);
                }
                Err(e) => trouble.push(format!("{}: {e}", label(&m.slot))),
            }
        }
        for h in &plan.minimize {
            arrange::minimize(window::hwnd(*h));
        }
        for h in &plan.raise {
            let _ = arrange::raise(window::hwnd(*h));
        }
        if let Some(f) = plan.focus {
            arrange::focus(window::hwnd(f));
        }

        // Start what is missing, unless it is open on another desktop.
        let mut launching = Vec::new();
        for (slot, launch) in &plan.launch {
            let Some(app) = ws.layout.slot(slot).map(|s| s.app.clone()) else {
                continue;
            };
            let elsewhere = all
                .iter()
                .any(|f| f.cloak == Cloak::ByShell && app.score(f).is_some());
            if elsewhere {
                continue;
            }
            match apps::start(launch) {
                Ok(_) => launching.push(Launching {
                    slot: slot.clone(),
                    app,
                    until: Instant::now() + LAUNCH_WAIT,
                }),
                Err(e) => trouble.push(format!("{} did not start: {e}", label(slot))),
            }
        }
        let starting = launching.len();
        self.activate(ws.id.clone(), assigned, placed);
        if let Some(a) = &mut self.active {
            a.launching = launching;
        }
        if starting > 0 {
            self.set_timer(timer::LAUNCH, 1000);
        }
        self.set_timer(timer::SETTLE, 400);
        self.persist();
        let moved = plan.moves.len() - trouble.len().min(plan.moves.len());
        tracing::info!(
            ms = format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
            moved,
            starting,
            problems = trouble.len(),
            minimized = plan.minimize.len(),
            "workspace opened"
        );
        let mut message = format!("Opened \u{201c}{}\u{201d}", ws.name);
        if starting > 0 {
            message.push_str(&format!(
                "; starting {starting} {}",
                if starting == 1 { "app" } else { "apps" }
            ));
        }
        message.push('.');
        if trouble.is_empty() {
            Reply::Done { message }
        } else {
            error(
                ErrorCode::Failed,
                format!(
                    "{message} Some windows could not be placed: {}.",
                    trouble.join("; ")
                ),
                Some(
                    "Windows of apps running as administrator, and apps that are not responding, are left where they are.",
                ),
            )
        }
    }

    /// A moment after a switch or save: windows that apps moved again (some restore their own
    /// position after being moved) are put back once.
    pub(crate) fn settle(&mut self) {
        let Some(active) = &mut self.active else { return };
        let placed = std::mem::take(&mut active.placed);
        for (h, wanted) in placed {
            let hwnd = window::hwnd(h);
            let Some(g) = window::geometry(hwnd) else { continue };
            if g.framable()
                && edge_distance(g.frame, wanted) > 2
                && !arrange::is_hung(hwnd)
                && let Err(e) = place::move_frame(hwnd, wanted)
            {
                tracing::debug!(error = %e, "window not put back");
            }
        }
        self.refresh_frames();
    }

    /// Monitors or work areas changed (a monitor plugged in or out, a scale or taskbar change,
    /// back from sleep): the open workspace's windows are fitted to the new arrangement.
    pub(crate) fn displays_changed(&mut self) {
        let before = self.topology.as_ref().map(Topology::fingerprint);
        let Some(topology) = self.refresh_topology() else {
            return;
        };
        if before.as_deref() == Some(topology.fingerprint().as_str()) {
            self.refresh_frames();
            return;
        }
        tracing::info!(monitors = topology.monitors().len(), "displays changed");
        let Some(active) = &self.active else {
            return;
        };
        let Some(ws) = self.file.workspaces.iter().find(|w| w.id == active.id).cloned() else {
            return;
        };
        let assigned: Vec<WindowHandle> = active.windows.values().copied().collect();
        let all = self.scanner.scan();
        let candidates: Vec<&WindowFacts> = all
            .iter()
            .filter(|f| assigned.contains(&f.handle) && !f.minimized)
            .collect();
        let quiet = SwitchSettings {
            minimize_others: false,
            launch_missing: false,
        };
        let plan = plan_switch(&ws, &candidates, &[], &topology, self.style(), quiet);
        self.hide_frames();
        let mut placed = Vec::new();
        for m in &plan.moves {
            let hwnd = window::hwnd(m.window);
            if arrange::is_hung(hwnd) {
                continue;
            }
            match &m.target {
                Target::Frame(r) => {
                    if let Ok(moved) = place::move_frame(hwnd, *r) {
                        placed.push((m.window, moved.wanted));
                    }
                }
                Target::Maximized(monitor) => {
                    if let Some(mon) = topology.get(monitor) {
                        let _ = place::maximize_on(hwnd, mon.work_area);
                    }
                }
            }
        }
        let zones = live_zones(&ws, &topology, self.style());
        if let Some(a) = &mut self.active {
            a.zones = zones;
            a.placed = placed;
        }
        self.set_timer(timer::SETTLE, 500);
        self.refresh_frames();
    }

    /// Drops apps that never showed a window.
    pub(crate) fn launch_tick(&mut self) {
        let now = Instant::now();
        let left = self.active.as_mut().map_or(0, |a| {
            a.launching.retain(|l| l.until > now);
            a.launching.len()
        });
        if left == 0 {
            self.kill_timer(timer::LAUNCH);
        }
    }

    /// A window appeared or got its title: if it belongs to an app started for the open
    /// workspace, it takes its slot and goes to its place.
    pub(crate) fn adopt(&mut self, h: WindowHandle) {
        let Some(active) = &self.active else { return };
        if active.launching.is_empty() || active.slot_of(h).is_some() {
            return;
        }
        let hwnd = window::hwnd(h);
        let Some(facts) = self.scanner.facts(hwnd) else {
            return;
        };
        if !self.manageable(&facts) {
            return;
        }
        let Some(pos) = active
            .launching
            .iter()
            .position(|l| l.app.score(&facts).is_some())
        else {
            return;
        };
        let Some(topology) = self.topology.clone() else {
            return;
        };
        let Some(ws) = self.file.workspaces.iter().find(|w| w.id == active.id).cloned() else {
            return;
        };
        let style = self.style();
        let Some(active) = &mut self.active else { return };
        let launched = active.launching.remove(pos);
        active.windows.insert(launched.slot.clone(), h);
        let target = ws
            .layout
            .slot(&launched.slot)
            .and_then(|s| ws.layout.effective_place(s))
            .and_then(|p| Resolver::new(&ws.layout, &topology, style).target(p));
        match target {
            Some(Target::Frame(r)) => match place::move_frame(hwnd, r) {
                Ok(moved) => active.placed.push((h, moved.wanted)),
                Err(e) => tracing::warn!(error = %e, "started window not placed"),
            },
            Some(Target::Maximized(monitor)) => {
                if let Some(mon) = topology.get(&monitor) {
                    let _ = place::maximize_on(hwnd, mon.work_area);
                }
            }
            None => {}
        }
        self.set_timer(timer::SETTLE, 600);
        self.refresh_frame(h);
    }
}
