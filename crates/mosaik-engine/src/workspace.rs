//! Workspaces at run time: saving the windows on screen as a workspace, opening one, starting
//! the apps it needs, and remembering a window the user moves by hand.
//!
//! A workspace's layout changes only when the user changes it: by saving, by dropping a window
//! into a zone, or by moving one of its windows by hand. Windows that apps move on their own, or
//! that the user maximises for a moment, go back to their places the next time the workspace
//! opens.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Pending, Verdict, classify};
use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Place, SlotId, Workspace, WorkspaceId};
use mosaik_core::matching::AppMatch;
use mosaik_core::place::{Resolver, Target};
use mosaik_core::store::StoreError;
use mosaik_core::topology::Topology;
use mosaik_core::window::{Cloak, WindowFacts, WindowHandle};
use mosaik_core::workspace::{
    Move, Present, Seen, capture, edge_distance, holds, plan_refit, plan_switch, regroup, remember,
    same_name, workspace_id,
};
use mosaik_core::zone::{FrameStyle, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ErrorCode, Reply};
use mosaik_win::{apps, arrange, window};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

use crate::UiEvent;
use crate::mover::{self, To};
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

    /// The zone a window whose visible frame is `frame` sits in (the rule is
    /// `mosaik_core::workspace::holds`); the smallest one when zones overlap.
    pub fn zone_for(&self, frame: Rect, style: FrameStyle) -> Option<&LiveZone> {
        self.zones
            .iter()
            .filter(|z| holds(z.rect, z.dpi, frame, style))
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

/// A short name for a slot in messages: its zone's name, else its program.
fn slot_label(ws: &Workspace, slot: &SlotId) -> String {
    let Some(s) = ws.layout.slot(slot) else {
        return "a window".to_owned();
    };
    if let Some(Place::Zone { zone }) = ws.layout.effective_place(s)
        && let Some(z) = ws.layout.zone(zone)
    {
        return z.name.clone();
    }
    s.app
        .exe
        .as_deref()
        .map(|e| e.trim_end_matches(".exe").to_owned())
        .or_else(|| {
            s.app
                .aumid
                .as_deref()
                .map(|a| a.split('!').next().unwrap_or(a).to_owned())
        })
        .unwrap_or_else(|| "a window".to_owned())
}

/// The moves of a plan, in the form the mover takes.
fn to_moves(moves: &[Move], topology: &Topology) -> Vec<(WindowHandle, To)> {
    moves
        .iter()
        .filter_map(|m| match &m.target {
            Target::Frame(r) => Some((m.window, To::Frame(*r))),
            Target::Maximized(monitor) => topology
                .get(monitor)
                .map(|mon| (m.window, To::Maximized(mon.work_area))),
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
        self.config.manage_only.as_ref().is_none_or(|exes| {
            f.exe
                .as_deref()
                .is_some_and(|e| exes.iter().any(|x| x.eq_ignore_ascii_case(e)))
        })
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

    /// Every top-level window, with the ones Mosaïk hid (background tabs) marked as such, so
    /// they count as the workspace's windows rather than as windows on another desktop.
    pub(crate) fn scan_marked(&mut self) -> Vec<WindowFacts> {
        let mut all = self.scanner.scan();
        for f in &mut all {
            if self.hiding.hidden.contains_key(&f.handle) {
                f.cloak = Cloak::ByMosaik;
            }
        }
        all
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

    /// A workspace by id, then by name (the same comparison the switcher uses).
    fn find_workspace(&self, which: &str) -> Option<usize> {
        let w = which.trim();
        self.file
            .workspaces
            .iter()
            .position(|ws| ws.id.0 == w)
            .or_else(|| self.file.workspaces.iter().position(|ws| same_name(&ws.name, w)))
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

    /// The user moved or resized one of the open workspace's windows by hand: the workspace
    /// remembers where it is now.
    pub(crate) fn remember_window(&mut self, h: WindowHandle) {
        let Some(active) = &self.active else { return };
        let Some(slot) = active.slot_of(h).cloned() else {
            return;
        };
        let Some(topology) = self.topology.clone() else {
            return;
        };
        let Some(index) = self.file.workspaces.iter().position(|w| w.id == active.id) else {
            return;
        };
        let Some(facts) = self.scanner.facts(window::hwnd(h)) else {
            return;
        };
        if facts.cloak == Cloak::ByShell {
            return;
        }
        let present = [Present { slot, window: &facts }];
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

        let windows: Vec<WindowFacts> = self
            .scan_marked()
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
        // Saving under an existing name (or id) updates that workspace.
        let previous_index = self.find_workspace(&name);
        let previous = previous_index.map(|i| self.file.workspaces[i].clone());
        let id = previous.as_ref().map_or_else(
            || {
                let ids: Vec<WorkspaceId> = self.file.workspaces.iter().map(|w| w.id.clone()).collect();
                workspace_id(&name, &ids)
            },
            |p| p.id.clone(),
        );
        let name = previous.as_ref().map_or(name, |p| p.name.clone());
        let style = self.style();
        let mut ws = capture(
            id.clone(),
            name.clone(),
            &seen,
            &topology,
            style,
            previous.as_ref(),
        );
        // Tab groups on screen stay tab groups.
        let groups: Vec<(Vec<usize>, usize)> = self
            .groups_now()
            .into_iter()
            .filter_map(|g| {
                let at = |h: WindowHandle| windows.iter().position(|w| w.handle == h);
                let tabs: Vec<usize> = g.tabs.iter().filter_map(|(_, w)| at(*w)).collect();
                let visible = g.visible.and_then(|(_, w)| at(w))?;
                (tabs.len() > 1).then_some((tabs, visible))
            })
            .collect();
        regroup(&mut ws, &groups);
        let assigned: HashMap<SlotId, WindowHandle> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .map(|(s, w)| (s.id.clone(), w.handle))
            .collect();
        // Windows in zones move inside them, below their tags.
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let moves: Vec<(WindowHandle, To)> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .filter(|(s, _)| matches!(s.place, Place::Zone { .. }))
            .filter_map(|(s, w)| match resolver.target(&s.place) {
                Some(Target::Frame(r)) => Some((w.handle, To::Frame(r))),
                _ => None,
            })
            .collect();
        let (windows_n, zones_n) = (ws.layout.slots.len(), ws.layout.zones.len());
        match previous_index {
            Some(i) => self.file.workspaces[i] = ws,
            None => self.file.workspaces.push(ws),
        }
        self.hide_frames();
        let placed = mover::move_all(&moves, mover::WAIT)
            .into_iter()
            .filter_map(|o| match o.result {
                Ok(wanted) => Some((o.window, wanted)),
                Err(e) => {
                    tracing::warn!(error = %e, "window not put in its zone");
                    None
                }
            })
            .collect();
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
        let previous: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let ws = self.file.workspaces[index].clone();
        let all = self.scan_marked();
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
        let outcomes = mover::move_all(&to_moves(&plan.moves, &topology), mover::WAIT);
        let mut placed = Vec::new();
        let mut trouble: Vec<String> = Vec::new();
        for o in outcomes {
            match o.result {
                Ok(wanted) => placed.push((o.window, wanted)),
                Err(e) => {
                    let slot = plan
                        .moves
                        .iter()
                        .find(|m| m.window == o.window)
                        .map(|m| m.slot.clone());
                    let who = slot.map_or_else(|| "a window".to_owned(), |s| slot_label(&ws, &s));
                    trouble.push(format!("{who}: {e}"));
                }
            }
        }
        // Every window the plan picked belongs to the workspace, moved or not.
        let assigned: HashMap<SlotId, WindowHandle> =
            plan.moves.iter().map(|m| (m.slot.clone(), m.window)).collect();
        for h in &plan.minimize {
            arrange::minimize(window::hwnd(*h));
        }
        // Tab groups: the visible tab shows, the others hide.
        self.activate(ws.id.clone(), assigned, placed);
        self.apply_groups();
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
            if all
                .iter()
                .any(|f| f.cloak == Cloak::ByShell && app.score(f).is_some())
            {
                continue;
            }
            self.start_app(launch.clone(), slot_label(&ws, slot));
            launching.push(Launching {
                slot: slot.clone(),
                app,
                until: Instant::now() + LAUNCH_WAIT,
            });
        }
        let starting = launching.len();
        if let Some(a) = &mut self.active {
            a.launching = launching;
        }
        self.refresh_frames();
        if starting > 0 {
            self.set_timer(timer::LAUNCH, 1000);
        }
        self.set_timer(timer::SETTLE, 400);
        self.persist();
        tracing::info!(
            ms = format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
            moved = plan.moves.len() - trouble.len().min(plan.moves.len()),
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
                    "{message} Some windows stayed where they were: {}.",
                    trouble.join("; ")
                ),
                Some("Apps that are not responding, and apps running as administrator, are left alone."),
            )
        }
    }

    /// Starts an app on a thread of its own: Windows can take seconds to start one (a packaged
    /// app waits for it to be ready), and the engine must not wait. A failure is told to the
    /// user in plain words.
    fn start_app(&self, launch: mosaik_core::layout::Launch, label: String) {
        let ui = Arc::clone(&self.ui);
        let spawned = std::thread::Builder::new()
            .name("mosaik-launch".into())
            .spawn(move || {
                // SAFETY: COM for this short-lived thread (packaged apps are started through COM).
                let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                if let Err(e) = apps::start(&launch) {
                    tracing::warn!(error = %e, "app not started");
                    ui(UiEvent::Notice {
                        title: "An app did not start".into(),
                        body: format!("{label} did not start: {e}"),
                    });
                }
            });
        if let Err(e) = spawned {
            tracing::error!(error = %e, "launch thread not started");
        }
    }

    /// A moment after windows were moved: those an app moved again (some restore their own
    /// position after being moved) are put back once. A window the user is moving is left alone.
    pub(crate) fn settle(&mut self) {
        let Some(active) = &mut self.active else { return };
        let placed = std::mem::take(&mut active.placed);
        let moving = self.watch.moving;
        let again: Vec<(WindowHandle, To)> = placed
            .into_iter()
            .filter(|(h, _)| moving != Some(*h))
            .filter_map(|(h, wanted)| {
                let hwnd = window::hwnd(h);
                let g = window::geometry(hwnd)?;
                (g.framable() && edge_distance(g.frame, wanted) > 2 && !arrange::is_hung(hwnd))
                    .then_some((h, To::Frame(wanted)))
            })
            .collect();
        if !again.is_empty() {
            for o in mover::move_all(&again, Duration::from_millis(1000)) {
                if let Err(e) = o.result {
                    tracing::debug!(error = %e, "window not put back");
                }
            }
        }
        self.refresh_frames();
    }

    /// Monitors or work areas changed (a monitor plugged in or out, a scale or taskbar change,
    /// back from sleep): the open workspace's windows are fitted to the new arrangement. Only
    /// windows on screen move; hidden, minimised and other-desktop windows are left alone.
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
        let assigned: HashMap<WindowHandle, SlotId> =
            active.windows.iter().map(|(s, w)| (*w, s.clone())).collect();
        let all = self.scan_marked();
        let on_screen: Vec<(SlotId, &WindowFacts)> = all
            .iter()
            .filter(|f| self.manageable(f))
            .filter_map(|f| assigned.get(&f.handle).map(|s| (s.clone(), f)))
            .collect();
        let moves = plan_refit(&ws, &on_screen, &topology, self.style());
        self.hide_frames();
        let placed: Vec<(WindowHandle, Rect)> = mover::move_all(&to_moves(&moves, &topology), mover::WAIT)
            .into_iter()
            .filter_map(|o| o.result.ok().map(|r| (o.window, r)))
            .collect();
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
        let Some(facts) = self.scanner.facts(window::hwnd(h)) else {
            return;
        };
        if !self.manageable(&facts) {
            return;
        }
        let Some(active) = &self.active else { return };
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
        let moves = plan_refit(&ws, &[(launched.slot, &facts)], &topology, style);
        let placed: Vec<(WindowHandle, Rect)> =
            mover::move_all(&to_moves(&moves, &topology), Duration::from_millis(1500))
                .into_iter()
                .filter_map(|o| o.result.ok().map(|r| (o.window, r)))
                .collect();
        if let Some(a) = &mut self.active {
            a.placed.extend(placed);
        }
        self.set_timer(timer::SETTLE, 600);
        self.refresh_frame(h);
    }
}
