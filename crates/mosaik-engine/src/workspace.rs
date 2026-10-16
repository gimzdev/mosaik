//! Workspaces at run time: saving the windows on screen as a workspace, opening one, starting
//! the apps it needs, and remembering a window the user moves by hand.
//!
//! A workspace's layout changes only when the user changes it: by saving, by dropping a window
//! into a zone, or by moving one of its windows by hand. Windows that apps move on their own, or
//! that the user maximises for a moment, go back to their places the next time the workspace
//! opens. Arrange changes none (ADR 0030): what it puts on screen is [`Arranged`], which the
//! same changes go to, until a workspace is opened or saved.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Pending, Verdict, classify};
use mosaik_core::geometry::{Rect, scale_for_dpi};
use mosaik_core::layout::{Place, SlotId, TabGroupId, Workspace, WorkspaceId};
use mosaik_core::matching::AppMatch;
use mosaik_core::place::{Resolver, Target};
use mosaik_core::store::StoreError;
use mosaik_core::topology::Topology;
use mosaik_core::window::{Cloak, WindowFacts, WindowHandle};
use mosaik_core::workspace::{
    GroupSeen, Move, Present, Seen, TabSeen, capture, edge_distance, holding, holds_window, plan_refit,
    plan_switch, regroup, remember, same_name, saved_over, workspace_id,
};
use mosaik_core::zone::{FrameStyle, ZoneColor, ZoneId};
use mosaik_ipc::protocol::{ErrorCode, Reply};
use mosaik_win::{apps, arrange, window};

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
    /// Tabs shown for now rather than by the user's choice (an app brought one forward): the
    /// layout keeps the user's choice.
    pub shown: HashMap<TabGroupId, SlotId>,
}

/// What Arrange put on screen, not saved (ADR 0030): its zones, and the windows in them, which
/// tags, snapping and tabs use as they use an open workspace's, until a workspace is opened or
/// saved.
#[derive(Clone, Debug)]
pub(crate) struct Arranged {
    /// Its layout, under `WorkspaceId::arranged()`.
    pub ws: Workspace,
    /// The saved workspace it was arranged from: still the one in use, which Save updates.
    pub from: Option<WorkspaceId>,
}

impl Active {
    pub fn slot_of(&self, h: WindowHandle) -> Option<&SlotId> {
        self.windows.iter().find(|(_, w)| **w == h).map(|(s, _)| s)
    }

    /// The zone a window whose visible frame is `frame` sits in (the rule is
    /// `mosaik_core::workspace::holds_window`: filling it, or at its own size for a window that
    /// cannot be resized); when several hold it, the one whose middle is nearest.
    pub fn zone_for(&self, frame: Rect, resizable: bool, style: FrameStyle) -> Option<&LiveZone> {
        holding(
            frame,
            resizable,
            self.zones.iter().map(|z| (z, z.rect, z.dpi)),
            style,
        )
    }
}

impl Engine {
    /// Whether what is on screen is Arrange's, not a saved workspace.
    pub(crate) fn arranged_on_screen(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.id.is_arranged()) && self.arranged.is_some()
    }

    /// The layout whose zones are on screen: Arrange's, or the open workspace's.
    pub(crate) fn on_screen(&self) -> Option<&Workspace> {
        let active = self.active.as_ref()?;
        match &self.arranged {
            Some(a) if active.id.is_arranged() => Some(&a.ws),
            _ => self.file.workspaces.iter().find(|w| w.id == active.id),
        }
    }

    /// [`Engine::on_screen`], to change it.
    pub(crate) fn on_screen_mut(&mut self) -> Option<&mut Workspace> {
        let active = self.active.as_ref()?;
        match &mut self.arranged {
            Some(a) if active.id.is_arranged() => Some(&mut a.ws),
            _ => self.file.workspaces.iter_mut().find(|w| w.id == active.id),
        }
    }

    /// The saved workspace in use, as lists mark it "On screen" and Save updates it: the open
    /// one, or the one Arrange started from.
    pub(crate) fn in_use(&self) -> Option<&WorkspaceId> {
        let active = self.active.as_ref()?;
        if active.id.is_arranged() {
            self.arranged.as_ref().and_then(|a| a.from.as_ref())
        } else {
            Some(&active.id)
        }
    }

    /// The saved workspace whose zones are on screen (none after Arrange): the one whose zones
    /// the editor shows as they are.
    pub(crate) fn open_saved(&self) -> Option<&Workspace> {
        self.on_screen().filter(|w| !w.id.is_arranged())
    }

    /// Writes `workspaces.json` after a change to what is on screen, unless that is Arrange's,
    /// which is not saved.
    pub(crate) fn persist_on_screen(&mut self) {
        if !self.arranged_on_screen() {
            self.persist();
        }
    }

    /// The zone of the open workspace that window `h`, whose visible frame is `frame`, sits in:
    /// the zone its slot (or its tab group) is placed in, when that zone holds it, since a
    /// window larger than its zone can cover a neighbouring zone just as well; else whichever
    /// zone holds it.
    pub(crate) fn zone_of(
        &self,
        h: WindowHandle,
        frame: Rect,
        resizable: bool,
        style: FrameStyle,
    ) -> Option<&LiveZone> {
        let active = self.active.as_ref()?;
        let placed_in = active.slot_of(h).and_then(|slot| {
            let layout = &self.on_screen()?.layout;
            match layout.effective_place(layout.slot(slot)?)? {
                Place::Zone { zone } => Some(zone),
                _ => None,
            }
        });
        placed_in
            .and_then(|id| active.zones.iter().find(|z| &z.id == id))
            .filter(|z| holds_window(z.rect, z.dpi, frame, style, resizable))
            .or_else(|| active.zone_for(frame, resizable, style))
    }
}

/// The zones of `ws` on the monitors connected now.
pub(crate) fn live_zones(ws: &Workspace, topology: &Topology, style: FrameStyle) -> Vec<LiveZone> {
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

fn error(code: ErrorCode, message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

/// How long a save the user asked for waits for the disk.
const SAVE_WAIT: Duration = Duration::from_secs(5);

impl Engine {
    pub(crate) fn style(&self) -> FrameStyle {
        self.settings.frames.effective_style()
    }

    /// The moves of a plan, in the form the mover takes.
    fn to_moves(&self, moves: &[Move], topology: &Topology) -> Vec<(WindowHandle, To)> {
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

    pub(crate) fn allowed(&self, f: &WindowFacts) -> bool {
        self.config
            .manage_only
            .as_ref()
            .is_none_or(|scope| scope.allows(f))
    }

    /// A window Mosaïk manages now: on screen, an app window, not elevated.
    pub(crate) fn manageable(&self, f: &WindowFacts) -> bool {
        self.allowed(f) && classify(f, &self.settings.rules) == Verdict::Manage
    }

    /// A window that can be added to a workspace: one Mosaïk manages, or one minimised, which
    /// it shows again when it goes to its place. The owner: "i should be able to add claude".
    pub(crate) fn addable(&self, f: &WindowFacts) -> bool {
        self.allowed(f)
            && matches!(
                classify(f, &self.settings.rules),
                Verdict::Manage | Verdict::Pending(Pending::Minimized)
            )
    }

    /// A window that can fill a slot when a workspace opens: a managed one, a minimised one it
    /// restores, or one in full screen, which it leaves as it is (`plan_switch`).
    pub(crate) fn movable(&self, f: &WindowFacts) -> bool {
        self.allowed(f)
            && matches!(
                classify(f, &self.settings.rules),
                Verdict::Manage | Verdict::Pending(Pending::Minimized | Pending::FullScreen)
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

    /// The monitors as they are now, without taking them as the last known arrangement:
    /// only `displays_changed` does that, so it never misses a change.
    pub(crate) fn read_topology(&self) -> Option<Topology> {
        mosaik_win::monitors::topology()
            .map_err(|e| tracing::warn!(error = %e, "monitors not read"))
            .ok()
            .or_else(|| self.topology.clone())
    }

    /// Writes `workspaces.json` for a change the user asked for, and waits for it; the error
    /// says why it could not be saved (the caller puts things back as they were).
    pub(crate) fn save_file(&mut self) -> Result<(), String> {
        if let Some(problem) = &self.file_problem {
            return Err(format!("Workspaces cannot be saved: {problem}"));
        }
        match &self.saver {
            Some(saver) => saver.now(self.file.clone(), SAVE_WAIT),
            None => self.store.save(&self.file).map_err(|e| e.to_string()),
        }
        .map_err(|e| format!("Your workspaces could not be saved: {e}"))
    }

    /// Writes `workspaces.json` after a change Mosaïk made itself (a window moved by hand, a tab
    /// clicked): on the saver's thread, once changes stop for a moment, so the change shows
    /// without waiting for the disk. A failure is told once and logged.
    pub(crate) fn persist(&mut self) {
        if self.file_problem.is_some() {
            return;
        }
        let failed = match &self.saver {
            // A write before this one may have failed meanwhile.
            Some(saver) if saver.later(self.file.clone()) => saver.take_failure(),
            _ => self.store.save(&self.file).err().map(|e| e.to_string()),
        };
        if let Some(e) = failed {
            tracing::error!(error = %e, "workspaces not saved");
            if !self.save_failed_told {
                self.save_failed_told = true;
                self.notice("Your workspaces could not be saved", e);
            }
        }
    }

    /// A workspace by id (what the interface sends), then by name (`mosaikctl` turns names
    /// into ids first, so a name never reaches a workspace whose id it happens to be).
    pub(crate) fn find_workspace(&self, which: &str) -> Option<usize> {
        let w = which.trim();
        self.file
            .workspaces
            .iter()
            .position(|ws| ws.id.0 == w)
            .or_else(|| self.find_by_name(w))
    }

    /// A workspace by name only (the same comparison the switcher uses).
    pub(crate) fn find_by_name(&self, name: &str) -> Option<usize> {
        self.file
            .workspaces
            .iter()
            .position(|ws| same_name(&ws.name, name))
    }

    /// Makes `id` the open workspace with these windows in its slots, and shows its frames. A
    /// saved workspace opened leaves what Arrange put on screen behind; `WorkspaceId::arranged()`
    /// is that, set in `arranged` first.
    pub(crate) fn activate(
        &mut self,
        id: WorkspaceId,
        windows: HashMap<SlotId, WindowHandle>,
        placed: Vec<(WindowHandle, Rect)>,
    ) {
        if !id.is_arranged() {
            self.arranged = None;
        }
        self.active = Some(Active {
            id,
            windows,
            zones: Vec::new(),
            placed,
            launching: Vec::new(),
            shown: HashMap::new(),
        });
        let zones = match (&self.topology, self.on_screen()) {
            (Some(t), Some(ws)) => live_zones(ws, t, self.style()),
            _ => Vec::new(),
        };
        if let Some(a) = &mut self.active {
            a.zones = zones;
        }
        self.file.active = self.in_use().cloned();
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
        let Some(facts) = self.scanner.facts(window::hwnd(h)) else {
            return;
        };
        if facts.cloak == Cloak::ByShell {
            return;
        }
        let style = self.style();
        let Some(ws) = self.on_screen() else { return };
        let present = [Present { slot, window: &facts }];
        let updated = remember(ws, &present, &topology, style);
        if &updated != ws {
            if let Some(ws) = self.on_screen_mut() {
                *ws = updated;
            }
            self.persist_on_screen();
        }
    }

    /// Saves the windows on screen as the workspace `name` (updating the one with that name, if
    /// any; without a name, the one in use), makes it the open one and puts its windows in their
    /// zones. Arrange's zones on screen are saved with them (`saved_over`).
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
        let Some(topology) = self.read_topology() else {
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
                self.in_use()
                    .and_then(|id| self.file.workspaces.iter().find(|w| &w.id == id))
                    .map(|w| w.name.clone())
            })
            .unwrap_or_else(|| mosaik_core::workspace::suggested_name(&self.file.workspaces));
        let name: String = name.chars().take(mosaik_core::editing::MAX_NAME).collect();

        // The windows on screen; background tabs Mosaïk hid are saved with their group below.
        let mut windows: Vec<WindowFacts> = self
            .scan_marked()
            .into_iter()
            .filter(|f| self.manageable(f) && !self.hiding.hidden.contains_key(&f.handle))
            .collect();
        // The tab that shows of a group sits below its strip's band: the zone made for it
        // includes the band.
        let style = self.style();
        let extra = style.for_tabs().tag_band - style.tag_band;
        let shown_tabs: std::collections::HashSet<WindowHandle> = self
            .groups_now()
            .iter()
            .filter_map(|g| g.visible().map(|(_, w)| *w))
            .collect();
        for f in &mut windows {
            if extra > 0 && shown_tabs.contains(&f.handle) {
                f.frame_rect.top -= scale_for_dpi(extra, f.dpi.max(96));
            }
        }
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
        // Saving under an existing name updates that workspace. Only names count: a name typed
        // in the switcher is never taken for the id of a workspace renamed since.
        let previous_index = self.find_by_name(&name);
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
        // Arrange's zones on screen are saved with the windows (ADR 0030); else, as always, the
        // zones of the workspace saved over are kept.
        let arranged = self
            .arranged
            .as_ref()
            .map(|a| &a.ws)
            .filter(|_| self.arranged_on_screen());
        let base = saved_over(previous.as_ref(), arranged, &windows, &topology, style);
        let mut ws = capture(id.clone(), name.clone(), &seen, &topology, style, base.as_ref());
        // Tab groups on screen stay tab groups, where their visible tab is. Their hidden tabs
        // (minimised ones too, when tabs are minimised) come from Mosaïk's own list.
        let groups = self.groups_now();
        let hidden_facts: Vec<WindowFacts> = groups
            .iter()
            .flat_map(|g| g.open.iter().map(|(_, w)| *w))
            .filter(|w| self.hiding.hidden.contains_key(w))
            .filter_map(|w| self.scanner.facts(window::hwnd(w)))
            .collect();
        let hidden_seen: Vec<Seen> = hidden_facts
            .iter()
            .map(|f| {
                let path = self.scanner.process_path(f.pid);
                Seen {
                    facts: f,
                    label: self.apps.label(f, path.as_deref()),
                    launch: apps::launch_for(f, path.as_deref()),
                }
            })
            .collect();
        let at = |h: WindowHandle| windows.iter().position(|w| w.handle == h);
        let group_seen: Vec<GroupSeen> = groups
            .iter()
            .filter_map(|g| {
                let (_, visible) = g.visible()?;
                let visible_at = at(*visible)?;
                let tabs: Vec<TabSeen> = g
                    .open
                    .iter()
                    .filter_map(|(_, w)| match at(*w) {
                        Some(i) => Some(TabSeen::Captured(i)),
                        None => hidden_seen
                            .iter()
                            .find(|s| s.facts.handle == *w)
                            .map(|s| TabSeen::Hidden(s.clone())),
                    })
                    .collect();
                let visible = tabs
                    .iter()
                    .position(|t| matches!(t, TabSeen::Captured(i) if *i == visible_at))?;
                Some(GroupSeen { tabs, visible })
            })
            .collect();
        let tab_slots = regroup(&mut ws, &group_seen);
        let mut assigned: HashMap<SlotId, WindowHandle> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .map(|(s, w)| (s.id.clone(), w.handle))
            .collect();
        assigned.extend(tab_slots);
        // Windows in zones move inside them, below their tags; a group's visible tab below the
        // band of its strip.
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let moves: Vec<(WindowHandle, To)> = ws
            .layout
            .slots
            .iter()
            .zip(&windows)
            .filter(|(s, _)| matches!(s.place, Place::Zone { .. } | Place::Tab { .. }))
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
        self.refresh_frames();
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
        let Some(topology) = self.read_topology() else {
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
        // Apps started a moment ago whose window has not shown yet: started again they would
        // open twice (the owner, opening a set twice).
        let now = Instant::now();
        let mut earlier: Vec<Launching> = self
            .active
            .as_ref()
            .map(|a| a.launching.iter().filter(|l| l.until > now).cloned().collect())
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
        // What undoes it (ADR 0029): opening a workspace minimises the other windows on its
        // screens, which a person new to Mosaïk would otherwise bring back one by one. Each
        // window it moves or minimises goes back where it was; one it brought back from the
        // taskbar goes back there.
        let touched: Vec<&WindowFacts> = plan
            .moves
            .iter()
            .map(|m| m.window)
            .chain(plan.minimize.iter().copied())
            .filter_map(|h| all.iter().find(|f| f.handle == h))
            .collect();
        let undo = crate::arranging::Undo {
            at: Instant::now(),
            workspace: Some(ws.clone()),
            arranged: self.arranged.clone(),
            active: self.active.as_ref().map(|a| (a.id.clone(), a.windows.clone())),
            windows: touched
                .iter()
                .filter(|f| !f.minimized)
                .map(|f| (f.handle, f.frame_rect, f.maximized))
                .collect(),
            minimized: touched.iter().filter(|f| f.minimized).map(|f| f.handle).collect(),
        };

        self.hide_frames();
        let moves = self.to_moves(&plan.moves, &topology);
        let outcomes = mover::move_all(&moves, mover::WAIT);
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
        // Every window the plan picked belongs to the workspace, moved or not, background tabs
        // included.
        let assigned: HashMap<SlotId, WindowHandle> = plan.assigned.iter().cloned().collect();
        for h in &plan.minimize {
            arrange::minimize(window::hwnd(*h));
        }
        // Tab groups: the visible tab shows, the others hide; windows hidden for the workspace
        // being left come back (minimised, unless this one uses them).
        self.hiding.resume();
        let kept: Vec<(WindowHandle, Rect, Rect)> = placed
            .iter()
            .filter_map(|&(h, kept)| {
                moves.iter().find(|(w, _)| *w == h).and_then(|(_, to)| match to {
                    To::Frame(target) => Some((h, *target, kept)),
                    To::Maximized(_) => None,
                })
            })
            .collect();
        self.activate(ws.id.clone(), assigned, placed);
        self.name_open_apps();
        self.sync_tabs();
        // Windows their apps keep larger than their zones: the zones make room for them.
        self.make_room(&topology, &kept);
        for h in &plan.raise {
            let _ = arrange::raise(window::hwnd(*h));
        }
        if let Some(f) = plan.focus {
            arrange::focus(window::hwnd(f));
        }
        // Strips and frames back above their windows once the apps have raised them.
        self.restack_soon();

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
            // Already starting: its window takes this slot when it shows. Each app started
            // stands for one slot.
            if let Some(pos) = earlier.iter().position(|l| l.app.same_app(&app)) {
                let started = earlier.remove(pos);
                launching.push(Launching {
                    slot: slot.clone(),
                    app,
                    until: started.until,
                });
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
        self.undo = Some(undo);
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
        let spawned = apps::start_on_its_own_thread(launch, move |outcome| {
            if let Err(e) = outcome {
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
        let placed = mosaik_core::workspace::last_places(std::mem::take(&mut active.placed));
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
        let Some(topology) = self.read_topology() else {
            return;
        };
        // The one place the arrangement read is kept as the last known: a preview or a switch
        // read in the 400 ms before this runs must not hide the change from it.
        self.topology = Some(topology.clone());
        if before.as_deref() == Some(topology.fingerprint().as_str()) {
            self.refresh_frames();
            return;
        }
        tracing::info!(monitors = topology.monitors().len(), "displays changed");
        self.refit_open_workspace(&topology);
        self.place_quick();
    }

    /// Puts the open workspace's windows that are on screen where its layout says on
    /// `topology` (after a display change, or when its zones were edited), and shows its zones
    /// there. Hidden, minimised and other-desktop windows are left alone.
    pub(crate) fn refit_open_workspace(&mut self, topology: &Topology) {
        self.hide_frames();
        let placed = self.refit(topology, None);
        // Zones that came out smaller than a window needs make room for it.
        self.make_room(topology, &placed);
    }

    /// [`Engine::refit_open_workspace`] for the windows in some zones only (`only`), or all of
    /// them. Returns each window moved, with the frame it was meant to have and the one it
    /// kept: they differ when its app would not take the zone's size.
    pub(crate) fn refit(
        &mut self,
        topology: &Topology,
        only: Option<&std::collections::HashSet<ZoneId>>,
    ) -> Vec<(WindowHandle, Rect, Rect)> {
        self.refit_with(topology, only, None)
    }

    /// [`Engine::refit`], with `shown` placed even when it is minimised: a window just added to
    /// the workspace, which its move shows again. Others the user minimised stay so.
    pub(crate) fn refit_with(
        &mut self,
        topology: &Topology,
        only: Option<&std::collections::HashSet<ZoneId>>,
        shown: Option<WindowHandle>,
    ) -> Vec<(WindowHandle, Rect, Rect)> {
        let Some(ws) = self.on_screen().cloned() else {
            return Vec::new();
        };
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let in_zones = |slot: &SlotId| {
            only.is_none_or(|zones| {
                ws.layout
                    .slot(slot)
                    .and_then(|s| ws.layout.effective_place(s))
                    .is_some_and(|p| matches!(p, Place::Zone { zone } if zones.contains(zone)))
            })
        };
        let assigned: HashMap<WindowHandle, SlotId> = active
            .windows
            .iter()
            .filter(|(s, _)| in_zones(s))
            .map(|(s, w)| (*w, s.clone()))
            .collect();
        let all = self.scan_marked();
        let on_screen: Vec<(SlotId, &WindowFacts)> = all
            .iter()
            .filter(|f| self.manageable(f) || (Some(f.handle) == shown && self.addable(f)))
            .filter_map(|f| assigned.get(&f.handle).map(|s| (s.clone(), f)))
            .collect();
        let moves = self.to_moves(
            &plan_refit(&ws, &on_screen, topology, self.style(), shown),
            topology,
        );
        let targets: HashMap<WindowHandle, Rect> = moves
            .iter()
            .filter_map(|(w, to)| match to {
                To::Frame(r) => Some((*w, *r)),
                To::Maximized(_) => None,
            })
            .collect();
        let placed: Vec<(WindowHandle, Rect)> = mover::move_all(&moves, mover::WAIT)
            .into_iter()
            .filter_map(|o| o.result.ok().map(|r| (o.window, r)))
            .collect();
        let zones = live_zones(&ws, topology, self.style());
        if let Some(a) = &mut self.active {
            a.zones = zones;
            a.placed.extend(placed.iter().copied());
        }
        self.set_timer(timer::SETTLE, 500);
        self.refresh_frames();
        placed
            .into_iter()
            .filter_map(|(w, wanted)| targets.get(&w).map(|t| (w, *t, wanted)))
            .collect()
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
        let Some(ws) = self.on_screen().cloned() else {
            return;
        };
        let style = self.style();
        let Some(active) = &mut self.active else { return };
        let launched = active.launching.remove(pos);
        active.windows.insert(launched.slot.clone(), h);
        let moves = plan_refit(&ws, &[(launched.slot, &facts)], &topology, style, None);
        let placed: Vec<(WindowHandle, Rect)> =
            mover::move_all(&self.to_moves(&moves, &topology), Duration::from_millis(1500))
                .into_iter()
                .filter_map(|o| o.result.ok().map(|r| (o.window, r)))
                .collect();
        if let Some(a) = &mut self.active {
            a.placed.extend(placed);
        }
        self.set_timer(timer::SETTLE, 600);
        // A tab of a group shows only if it is the group's tab to show.
        self.refresh_frame(h);
        self.sync_tabs();
    }
}
