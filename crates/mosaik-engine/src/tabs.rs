//! Tabs for any app: windows that share one place, one visible at a time, with a tab strip in
//! the band above the visible one (ADR 0003 for how background tabs are hidden, ADR 0027 for
//! where strips go).
//!
//! Which window shows is decided in one place, [`Engine::sync_tabs`], with the rule of
//! `mosaik_core::tabs`: a window stays hidden only while it is a background tab of a group of the
//! open workspace whose other tabs can be reached (its strip is on screen, or comes back with
//! its minimised or maximised visible tab). A window is hidden only after the journal says how
//! to bring it back, and only while the watchdog runs (ADR 0006): whatever happens to Mosaïk,
//! hidden windows come back.

use std::collections::{HashMap, HashSet};

use mosaik_core::geometry::{Point, RatioRect, Rect, scale_for_dpi};
use mosaik_core::journal::{HiddenWindow, HideMethod, Journal};
use mosaik_core::layout::{Layout, Place, Slot, SlotId, TabGroupId};
use mosaik_core::place::{Resolver, Target};
use mosaik_core::snap::{self, LiveZone as SnapZone};
use mosaik_core::store::Saved;
use mosaik_core::tabs::{GroupNow, Reach, plan_tabs};
use mosaik_core::window::{Cloak, WindowHandle};
use mosaik_core::workspace::{app_match, edge_distance};
use mosaik_core::zone::{ZoneColor, ZoneId, band_above};
use mosaik_overlay::layered::Stack;
use mosaik_overlay::tabbar::{self, BarInput, BarLook, BarPlace, Hit, TabBar, TabView};
use mosaik_win::hide::Hidden;
use mosaik_win::process::Process;
use mosaik_win::{apps, arrange, window};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};

use crate::hider::{self, Answer, Kind, Request, Worker};
use crate::mover::{self, To};
use crate::thread::{Engine, timer};
use crate::watch::{DRAG_TICK_MS, shift_down};

/// After a tab is shown, strips go back above their windows this long after, then again.
const STACK_FIRST_MS: u32 = 120;
const STACK_AGAIN_MS: u32 = 400;

/// Windows Mosaïk has hidden, how, and the journal the watchdog reads.
pub(crate) struct Hiding {
    worker: Option<Worker>,
    pub journal: Journal,
    /// Windows Mosaïk keeps hidden, and how to bring each back. A window whose hide is still
    /// running is listed too: the journal has it, so it cannot be lost.
    pub hidden: HashMap<WindowHandle, Hidden>,
    /// Windows that would not hide: their groups show every tab instead.
    refused: HashSet<WindowHandle>,
    /// The watchdog, checked before anything hides and started again if it stopped.
    watchdog: Option<Process>,
    watchdog_restarts: u32,
    /// After a rescue nothing hides, until the user next picks a tab or a workspace.
    paused: bool,
    /// The journal could not be written, or the hiding thread could not start: nothing hides
    /// for the rest of the session.
    broken: bool,
}

impl Hiding {
    /// `left` are entries an earlier run could not restore: kept, so writing the journal again
    /// does not drop them.
    pub(crate) fn new(owner_pid: u32, watchdog: Option<Process>, left: Vec<HiddenWindow>) -> Self {
        let mut journal = Journal::new(owner_pid);
        journal.hidden = left;
        Self {
            worker: None,
            journal,
            hidden: HashMap::new(),
            refused: HashSet::new(),
            watchdog,
            watchdog_restarts: 0,
            paused: false,
            broken: false,
        }
    }

    /// After a rescue brought every window back: only what it could not restore stays in the
    /// journal, and nothing hides until the user next picks a tab or a workspace.
    pub(crate) fn rescued(&mut self, left: Vec<HiddenWindow>) {
        self.journal.hidden = left;
        self.hidden.clear();
        self.refused.clear();
        self.paused = true;
    }

    /// The user picked a tab or a workspace: tabs hide again after a rescue.
    pub(crate) fn resume(&mut self) {
        self.paused = false;
    }

    /// The watchdog is running now.
    pub(crate) fn watchdog_running(&self) -> bool {
        self.watchdog.as_ref().is_some_and(|p| p.exit_code().is_none())
    }
}

/// A press on a tab chip, and what it turned into.
#[derive(Clone, Debug)]
pub(crate) struct BarDrag {
    pub group: TabGroupId,
    pub slot: SlotId,
    pub start: Point,
    pub mode: BarMode,
    /// Zones shown while a tab is pulled out, and the one under the pointer.
    pub zones: Vec<SnapZone>,
    pub target: Option<usize>,
    /// The tab order changed during the drag: saved when it ends.
    pub reordered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarMode {
    Pressed,
    Reordering,
    PullingOut,
}

/// Where a group's strip goes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Spot {
    At {
        band: Rect,
        /// In the visible tab's title bar, or in a band above it.
        place: BarPlace,
        color: ZoneColor,
        /// The zone's name; empty outside zones.
        label: String,
        dpi: u32,
    },
    /// The visible tab is minimised or on another desktop, maximised with no room in its title
    /// bar, or its title bar is being read: the strip comes back with it.
    Later,
    /// No room for a strip, in the visible tab's title bar or above it.
    Nowhere,
}

/// What a tab chip says: the window's title, or "Window" when it has none.
fn tab_title(h: WindowHandle) -> String {
    let title = window::title(window::hwnd(h));
    let t = title.trim();
    if t.is_empty() {
        "Window".to_owned()
    } else {
        t.to_owned()
    }
}

/// Whether a window Mosaïk hid is still hidden (an app may have brought it back by itself).
fn still_hidden(w: WindowHandle, how: &Hidden) -> bool {
    let hwnd = window::hwnd(w);
    if !window::exists(hwnd) {
        return true;
    }
    match how.method {
        HideMethod::ShellCloak => window::cloak(hwnd) != Cloak::None,
        HideMethod::Minimize => window::geometry(hwnd).is_some_and(|g| g.minimized),
        HideMethod::Park => true,
    }
}

fn esc_down() -> bool {
    // SAFETY: reads the key state; no preconditions.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) };
    state.cast_unsigned() & 0x8000 != 0
}

impl Engine {
    // ----- The groups and where they go -----

    /// The open workspace's tab groups as they are now, their reach not decided yet.
    pub(crate) fn groups_now(&self) -> Vec<GroupNow> {
        let Some(active) = &self.active else {
            return Vec::new();
        };
        let Some(ws) = self.file.workspaces.iter().find(|w| w.id == active.id) else {
            return Vec::new();
        };
        ws.layout
            .tab_groups
            .iter()
            .map(|g| GroupNow {
                id: g.id.clone(),
                order: g.tabs.clone(),
                open: g
                    .tabs
                    .iter()
                    .filter_map(|s| {
                        let w = *active.windows.get(s)?;
                        self.present(w).then(|| (s.clone(), w))
                    })
                    .collect(),
                wanted: Some(active.shown.get(&g.id).unwrap_or(&g.active).clone()),
                reach: Reach::Strip,
            })
            .collect()
    }

    /// A tab's window is there to be shown: open, and not hidden by its own app (closed to the
    /// tray), which leaves its place in the group to the next tab.
    fn present(&self, w: WindowHandle) -> bool {
        let hwnd = window::hwnd(w);
        window::exists(hwnd) && (self.hiding.hidden.contains_key(&w) || window::is_visible(hwnd))
    }

    /// The group a window is an open tab of, in the open workspace.
    pub(crate) fn group_of_window(&self, h: WindowHandle) -> Option<GroupNow> {
        self.groups_now()
            .into_iter()
            .find(|g| g.open.iter().any(|(_, w)| *w == h))
    }

    /// Where a window at `place` goes, from the layout.
    fn resolve(&self, place: &Place) -> Option<To> {
        let active = self.active.as_ref()?;
        let ws = self.file.workspaces.iter().find(|w| w.id == active.id)?;
        let topology = self.topology.as_ref()?;
        let resolver = Resolver::new(&ws.layout, topology, self.style());
        match resolver.target(place)? {
            Target::Frame(r) => Some(To::Frame(r)),
            Target::Maximized(m) => topology.get(&m).map(|mon| To::Maximized(mon.work_area)),
        }
    }

    /// Where the tabs of `group` go: the group's place, from the layout, below a band with room
    /// for its strip.
    pub(crate) fn group_target(&self, group: &TabGroupId) -> Option<To> {
        self.resolve(&Place::Tab { group: group.clone() })
    }

    /// Where the window of a slot goes, from the layout.
    pub(crate) fn slot_target(&self, slot: &SlotId) -> Option<To> {
        let active = self.active.as_ref()?;
        let ws = self.file.workspaces.iter().find(|w| w.id == active.id)?;
        let place = ws.layout.slot(slot)?.place.clone();
        self.resolve(&place)
    }

    /// Where a group's strip goes, given its visible tab: in the band above it, in the zone
    /// holding it, or right above it outside zones when there is room on its monitor. Never in
    /// its title bar (ADR 0027): a group sits below a band for its strip, tags in title bars or
    /// not, so every tab has room to be dragged out.
    pub(crate) fn strip_spot(&mut self, group: &TabGroupId, visible: WindowHandle) -> Spot {
        let (frame, maximized) = if self.hiding.hidden.contains_key(&visible) {
            // Still hidden: it shows in the group's place.
            match self.group_target(group) {
                Some(To::Frame(r)) => (r, false),
                Some(To::Maximized(_)) => return Spot::Later,
                None => return Spot::Nowhere,
            }
        } else {
            let Some(g) = window::geometry(window::hwnd(visible)) else {
                return Spot::Nowhere;
            };
            if g.minimized || g.cloaked || g.full_screen || g.maximized {
                return Spot::Later;
            }
            (g.frame, g.maximized)
        };
        let banded = self.style().for_tabs();
        let resizable = window::geometry(window::hwnd(visible)).is_none_or(|g| g.resizable);
        let zone = self
            .zone_of(visible, frame, resizable, banded)
            .filter(|_| !maximized)
            .cloned();
        let monitor = self.topology.as_ref().map(|t| t.monitor_for_rect(frame).clone());
        let dpi = zone
            .as_ref()
            .map(|z| z.dpi)
            .or_else(|| monitor.as_ref().map(|m| m.dpi))
            .unwrap_or(96);
        let (color, label) = zone
            .as_ref()
            .map_or((ZoneColor::Slate, String::new()), |z| (z.color, z.name.clone()));
        if let Some(z) = &zone
            && let Some(band) = banded.tag_area(z.rect, z.dpi)
        {
            return Spot::At {
                // Right above the visible tab (one that cannot be resized sits lower in the zone).
                band: band_above(frame, band),
                place: BarPlace::Band,
                color,
                label,
                dpi,
            };
        }
        let Some(monitor) = monitor else {
            return Spot::Nowhere;
        };
        let band = Rect::new(
            frame.left,
            frame.top - scale_for_dpi(banded.tag_band, monitor.dpi),
            frame.right,
            frame.top,
        );
        if band.top < monitor.work_area.top || band.is_empty() {
            return Spot::Nowhere;
        }
        Spot::At {
            band,
            place: BarPlace::Band,
            color,
            label,
            dpi,
        }
    }

    // ----- Showing what should show -----

    /// Makes the screen match the open workspace's tab groups: each group shows one tab, with
    /// its strip above it, and keeps its other tabs hidden; every other window Mosaïk hid shows
    /// again. Called after anything that changes groups, the tab that shows, or whether a strip
    /// can be drawn.
    pub(crate) fn sync_tabs(&mut self) {
        self.sync_tabs_pass(true);
    }

    fn sync_tabs_pass(&mut self, again: bool) {
        let late = self.hiding.worker.as_mut().map(Worker::late).unwrap_or_default();
        self.take_answers(late);
        self.reclaim_shown_by_apps();
        let can_hide = self.can_hide();
        let mut groups = self.groups_now();
        let mut spots: HashMap<TabGroupId, Spot> = HashMap::new();
        for g in &mut groups {
            let spot = g
                .visible()
                .map_or(Spot::Nowhere, |(_, w)| self.strip_spot(&g.id, *w));
            let refused = g.open.iter().any(|(_, w)| self.hiding.refused.contains(w));
            g.reach = match &spot {
                _ if !can_hide || refused => Reach::None,
                Spot::At { .. } if self.watch.strip_broken.contains(&g.id) => Reach::None,
                Spot::At { .. } => Reach::Strip,
                Spot::Later => Reach::Later,
                Spot::Nowhere => Reach::None,
            };
            spots.insert(g.id.clone(), spot);
        }
        let hidden: Vec<WindowHandle> = self.hiding.hidden.keys().copied().collect();
        let plan = plan_tabs(&groups, &hidden);
        let refused_before = self.hiding.refused.len();
        let member: HashMap<WindowHandle, TabGroupId> = groups
            .iter()
            .flat_map(|g| g.open.iter().map(|(_, w)| (*w, g.id.clone())))
            .collect();

        // 1. Windows coming back go to their place while still hidden, then show. A window
        //    that is no longer the workspace's comes back minimised, in the taskbar.
        let mut moves = Vec::new();
        let mut shows = Vec::new();
        for w in &plan.show {
            let slot = self.active.as_ref().and_then(|a| a.slot_of(*w)).cloned();
            let target = match (member.get(w), &slot) {
                (Some(g), _) => self.group_target(g),
                (None, Some(s)) => self.slot_target(s),
                (None, None) => None,
            };
            if let Some(to) = target
                && needs_move(*w, to)
            {
                moves.push((*w, to));
            }
            shows.push((*w, !member.contains_key(w) && slot.is_none()));
        }
        if !moves.is_empty() {
            for o in mover::move_all(&moves, mover::WAIT_ONE) {
                if let Err(e) = o.result {
                    tracing::debug!(error = %e, "window not put in place before showing");
                }
            }
        }
        let still_hidden = self.show_windows(&shows);

        // 2. Strips, before anything hides: a group whose strip cannot be drawn keeps its tabs
        //    on screen (the next pass sees it broken).
        let mut broken_changed = false;
        for g in &groups {
            let drop_at = self.drop_at(g);
            match self.draw_strip(g, spots.get(&g.id), drop_at) {
                Some(true) => broken_changed |= self.watch.strip_broken.remove(&g.id),
                Some(false) => broken_changed |= self.watch.strip_broken.insert(g.id.clone()),
                None => {}
            }
        }
        let ids: HashSet<&TabGroupId> = groups.iter().map(|g| &g.id).collect();
        self.watch.bars.retain(|g, _| ids.contains(g));

        // 3. Background tabs hide, except in a group whose tab to show did not come back.
        let stuck: HashSet<&TabGroupId> = plan
            .visible
            .iter()
            .filter(|(_, _, w)| still_hidden.contains(w))
            .map(|(g, _, _)| g)
            .collect();
        let hide: Vec<WindowHandle> = plan
            .hide
            .iter()
            .copied()
            .filter(|w| !self.hiding.hidden.contains_key(w))
            .filter(|w| !member.get(w).is_some_and(|g| stuck.contains(g)))
            .collect();
        if can_hide && !hide.is_empty() {
            self.hide_windows(&hide);
        }
        // A strip that could not be drawn, or a window that would not hide: its group shows all
        // its tabs, which takes one more pass.
        if again && (broken_changed || self.hiding.refused.len() != refused_before) {
            self.sync_tabs_pass(false);
        }
    }

    /// Draws a group's strip where `spot` says, with the "Drop to add" slot at `drop_at`, or
    /// hides it. `None` when no strip is wanted now, else whether it is on screen.
    pub(crate) fn draw_strip(
        &mut self,
        g: &GroupNow,
        spot: Option<&Spot>,
        drop_at: Option<usize>,
    ) -> Option<bool> {
        let visible = g.visible().map(|(_, w)| *w);
        let (
            Some(visible),
            Some(Spot::At {
                band,
                place,
                color,
                label,
                dpi,
            }),
        ) = (visible, spot)
        else {
            self.hide_bar(&g.id);
            return None;
        };
        let dragged = self.watch.drag.as_ref().map(|d| d.window);
        if (g.open.len() < 2 && drop_at.is_none())
            || dragged == Some(visible)
            || self.hiding.hidden.contains_key(&visible)
        {
            // One tab, its window being dragged, or not back yet: no strip for now.
            self.hide_bar(&g.id);
            return None;
        }
        let tabs: Vec<TabView> = g
            .open
            .iter()
            .map(|(_, w)| TabView {
                id: w.0,
                title: tab_title(*w),
                active: *w == visible,
            })
            .collect();
        let look = BarLook {
            color: *color,
            label: label.clone(),
            dpi: *dpi,
            dark: self.dark_theme(),
            rounded: self.watch.rounded,
        };
        // While a window is dragged over the strip, the strip shows above it.
        let above = match (drop_at, dragged) {
            (Some(_), Some(d)) => d,
            _ => visible,
        };
        let (band, place) = (*band, *place);
        if self.canvas_ready().is_none() {
            return Some(false);
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return Some(false);
        };
        let shown = match self.bar(&g.id) {
            Some(bar) => match bar.show(
                &mut canvas,
                band,
                place,
                &look,
                &tabs,
                drop_at,
                Stack::Above(above.0.cast_signed_isize()),
            ) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(error = %e, "tab strip not drawn");
                    false
                }
            },
            None => false,
        };
        self.watch.put_canvas(canvas);
        Some(shown)
    }

    /// Redraws one group's strip where it belongs now (its window moved, a title changed, a
    /// window is dragged over it).
    pub(crate) fn refresh_strip(&mut self, group: &TabGroupId) {
        let Some(g) = self.groups_now().into_iter().find(|g| &g.id == group) else {
            self.hide_bar(group);
            return;
        };
        let spot = g.visible().map(|(_, w)| self.strip_spot(&g.id, *w));
        let drop_at = self.drop_at(&g);
        if self.draw_strip(&g, spot.as_ref(), drop_at) == Some(false)
            && self.watch.strip_broken.insert(g.id.clone())
        {
            self.sync_tabs();
        }
    }

    /// Where the "Drop to add" slot goes on a group's strip while a window is dragged over it.
    fn drop_at(&self, g: &GroupNow) -> Option<usize> {
        let (onto, before) = self.watch.drag.as_ref()?.tab_target?;
        if !g.open.iter().any(|(_, w)| *w == onto) {
            return None;
        }
        Some(
            before
                .and_then(|b| g.open.iter().position(|(_, w)| *w == b))
                .unwrap_or(g.open.len()),
        )
    }

    /// The tab strip of a group, made on first use.
    fn bar(&mut self, group: &TabGroupId) -> Option<&mut TabBar> {
        if !self.watch.bars.contains_key(group) {
            match TabBar::new() {
                Ok(b) => {
                    self.watch.bars.insert(group.clone(), b);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "tab strip not created");
                    return None;
                }
            }
        }
        self.watch.bars.get_mut(group)
    }

    pub(crate) fn hide_bar(&mut self, group: &TabGroupId) {
        // Its visible tab dragged by the strip's band: the strip keeps the press it started
        // with, hidden or not, until the button goes up.
        let dragged_by_it = self
            .watch
            .band_press
            .is_some_and(|p| p.moving && self.group_of_window(p.window).is_some_and(|g| &g.id == group));
        if let Some(b) = self.watch.bars.get_mut(group) {
            if dragged_by_it {
                b.hide_keeping_press();
            } else {
                b.hide();
            }
        }
    }

    // ----- Hiding and showing -----

    /// Whether Mosaïk may hide windows now: not in safe mode, not paused after a rescue, the
    /// journal can be written, and the watchdog runs.
    fn can_hide(&mut self) -> bool {
        if self.config.safe_mode || self.hiding.paused || self.hiding.broken {
            return false;
        }
        if self.hiding.watchdog_running() {
            return true;
        }
        if self.config.watchdog.is_none() {
            // There never was one: the app said so when it started.
            return false;
        }
        // It stopped (ended from Task Manager, say): start it again, a few times at most.
        while self.hiding.watchdog_restarts < 3 {
            self.hiding.watchdog_restarts += 1;
            match crate::watchdog::start(&self.config.data_dir.join(Journal::FILE)) {
                Ok(p) => {
                    tracing::warn!(pid = p.pid, "watchdog started again");
                    self.scanner.own_pids.push(p.pid);
                    self.hiding.watchdog = Some(p);
                    return true;
                }
                Err(e) => tracing::error!(error = %e, "watchdog not started again"),
            }
        }
        self.hiding.watchdog = None;
        self.tell_once(
            "watchdog",
            "Mosaïk's safety net stopped",
            "It could not be started again, so Mosaïk hides no window: tab groups show all their windows, stacked. Quit Mosaïk from the tray and start it again to bring tabs back.",
        );
        false
    }

    fn worker(&mut self) -> Option<&mut Worker> {
        if self.hiding.worker.is_none() {
            if self.hiding.broken {
                return None;
            }
            match Worker::start() {
                Ok(w) => self.hiding.worker = Some(w),
                Err(e) => {
                    self.hiding.broken = true;
                    tracing::error!(error = %e, "hiding thread not started");
                    self.tell_once(
                        "hider",
                        "Tabs cannot hide windows",
                        format!("Mosaïk could not start hiding windows ({e}). Tab groups show all their windows, stacked."),
                    );
                    return None;
                }
            }
        }
        self.hiding.worker.as_mut()
    }

    /// Writes the journal; a failure is logged (the watchdog then works from the older journal
    /// and the windows' own markers).
    fn save_journal(&mut self) {
        if let Err(e) = self.store.save(&self.hiding.journal) {
            tracing::error!(error = %e, "journal not saved");
        }
    }

    /// Applies what the hiding thread did.
    fn take_answers(&mut self, answers: Vec<Answer>) {
        if answers.is_empty() {
            return;
        }
        let preferred = self.settings.tab_hiding.unwrap_or(HideMethod::ShellCloak);
        let mut dirty = false;
        let mut not_back = Vec::new();
        for a in answers {
            match (a.kind, a.result) {
                (Kind::Show, Ok(_)) => {
                    self.hiding.hidden.remove(&a.window);
                    dirty |= self.hiding.journal.forget(a.window).is_some();
                }
                (Kind::Show, Err(f)) if f.gone => {
                    self.hiding.hidden.remove(&a.window);
                    dirty |= self.hiding.journal.forget(a.window).is_some();
                }
                // Still hidden, still in the journal: a later pass or a rescue brings it back.
                (Kind::Show, Err(f)) => not_back.push(f.message),
                (Kind::Hide, Ok(Some(how))) => {
                    if how.method != preferred {
                        if let Some(e) = self
                            .hiding
                            .journal
                            .hidden
                            .iter_mut()
                            .find(|e| e.handle == a.window)
                        {
                            e.method = how.method;
                            dirty = true;
                        }
                        self.tell_once(
                            "minimise",
                            "Background tabs are minimised",
                            "Windows did not hide a tab the usual way, so Mosaïk minimises background tabs instead: they show in the taskbar while hidden.",
                        );
                    }
                    self.hiding.hidden.insert(a.window, how);
                }
                (Kind::Hide, Ok(None)) => {}
                (Kind::Hide, Err(f)) => {
                    self.hiding.hidden.remove(&a.window);
                    dirty |= self.hiding.journal.forget(a.window).is_some();
                    if !f.gone {
                        self.hiding.refused.insert(a.window);
                        self.tell_once(
                            "refused",
                            "A tab could not be hidden",
                            format!("Windows would not hide it ({}). Its tab group shows all its windows, stacked in their place.", f.message),
                        );
                    }
                }
            }
        }
        if dirty {
            self.save_journal();
        }
        if let Some(first) = not_back.first() {
            tracing::warn!(error = %first, count = not_back.len(), "hidden windows not shown");
            self.tell_once(
                "not-back",
                "A hidden window did not come back",
                format!("{first}. Mosaïk keeps trying; \u{201c}Rescue windows\u{201d} in the tray menu brings back every window it hid."),
            );
        }
    }

    /// Brings back windows Mosaïk hid (`true`: minimised, in the taskbar). Returns the ones
    /// still hidden.
    pub(crate) fn show_windows(&mut self, list: &[(WindowHandle, bool)]) -> HashSet<WindowHandle> {
        let batch: Vec<(WindowHandle, Request)> = list
            .iter()
            .filter_map(|(w, minimised)| {
                let how = *self.hiding.hidden.get(w)?;
                Some((
                    *w,
                    if *minimised {
                        Request::ShowMinimized(how)
                    } else {
                        Request::Show(how)
                    },
                ))
            })
            .collect();
        if !batch.is_empty() {
            let answers = match self.worker() {
                Some(worker) => worker.run(batch, hider::WAIT),
                None => Vec::new(),
            };
            self.take_answers(answers);
        }
        list.iter()
            .map(|(w, _)| *w)
            .filter(|w| self.hiding.hidden.contains_key(w))
            .collect()
    }

    /// Hides background tabs: all are written to the journal first, in one write, then hidden
    /// on the hiding thread.
    fn hide_windows(&mut self, list: &[WindowHandle]) {
        let method = self.settings.tab_hiding.unwrap_or(HideMethod::ShellCloak);
        let mut batch = Vec::new();
        for w in list {
            let Some(facts) = self.scanner.facts(window::hwnd(*w)) else {
                continue;
            };
            self.hiding.journal.record(HiddenWindow {
                handle: *w,
                pid: facts.pid,
                method,
                restore_rect: facts.window_rect,
                was_maximized: facts.maximized,
                exe: facts.exe.clone(),
                title: facts.title.clone(),
            });
            self.hiding.hidden.insert(
                *w,
                Hidden {
                    method,
                    restore_rect: facts.window_rect,
                    was_maximized: facts.maximized,
                },
            );
            batch.push((*w, Request::Hide(method)));
        }
        if batch.is_empty() {
            return;
        }
        let undo = |hiding: &mut Hiding, batch: &[(WindowHandle, Request)]| {
            for (w, _) in batch {
                hiding.journal.forget(*w);
                hiding.hidden.remove(w);
            }
        };
        if let Err(e) = self.store.save(&self.hiding.journal) {
            undo(&mut self.hiding, &batch);
            self.hiding.broken = true;
            self.tell_once(
                "journal",
                "Tabs cannot hide windows",
                format!("The list of hidden windows could not be saved ({e}), and Mosaïk hides nothing it could not bring back. Tab groups show all their windows, stacked."),
            );
            return;
        }
        let answers = match self.worker() {
            Some(worker) => worker.run(batch, hider::WAIT),
            None => {
                undo(&mut self.hiding, &batch);
                self.save_journal();
                return;
            }
        };
        self.take_answers(answers);
    }

    /// Windows an app brought back by itself while Mosaïk kept them hidden (restored from
    /// minimised, or uncloaked): they are on screen, so they show in their group, for this
    /// session; the layout keeps the user's choice.
    fn reclaim_shown_by_apps(&mut self) {
        if self.hiding.worker.as_ref().is_some_and(Worker::busy) {
            // A hide may not have happened yet.
            return;
        }
        let back: Vec<WindowHandle> = self
            .hiding
            .hidden
            .iter()
            .filter(|(w, how)| !still_hidden(**w, how))
            .map(|(w, _)| *w)
            .collect();
        if back.is_empty() {
            return;
        }
        for w in back {
            self.hiding.hidden.remove(&w);
            self.hiding.journal.forget(w);
            if let Some(g) = self.group_of_window(w)
                && let Some((slot, _)) = g.open.iter().find(|(_, x)| *x == w)
                && let Some(a) = &mut self.active
            {
                a.shown.insert(g.id.clone(), slot.clone());
            }
        }
        self.save_journal();
    }

    /// A hidden window closed: nothing to bring back any more.
    pub(crate) fn forget_hidden(&mut self, h: WindowHandle) {
        if self.hiding.hidden.remove(&h).is_some() | self.hiding.journal.forget(h).is_some() {
            self.save_journal();
        }
    }

    /// Every window Mosaïk hid comes back where it is (rescue, exit).
    pub(crate) fn show_all_hidden(&mut self) {
        let all: Vec<(WindowHandle, bool)> = self.hiding.hidden.keys().map(|w| (*w, false)).collect();
        let _ = self.show_windows(&all);
    }

    /// Says something once per session.
    pub(crate) fn tell_once(&mut self, key: &'static str, title: &str, body: impl Into<String>) {
        if self.told.insert(key) {
            self.notice(title, body);
        }
    }

    // ----- What the user does with tabs -----

    /// Changes the open workspace's layout, and saves it when it changed.
    pub(crate) fn edit_layout<R>(&mut self, change: impl FnOnce(&mut Layout) -> R) -> Option<R> {
        let changed = self.edit_layout_quietly(change)?;
        if changed.1 {
            self.persist();
        }
        Some(changed.0)
    }

    /// Changes the open workspace's layout without saving it (a drag in progress, saved when it
    /// ends). Returns whether it changed.
    fn edit_layout_quietly<R>(&mut self, change: impl FnOnce(&mut Layout) -> R) -> Option<(R, bool)> {
        let id = self.active.as_ref()?.id.clone();
        let ws = self.file.workspaces.iter_mut().find(|w| w.id == id)?;
        let before = ws.layout.clone();
        let r = change(&mut ws.layout);
        let changed = ws.layout != before;
        Some((r, changed))
    }

    /// Shows the tab `slot` of `group`, as the user picked it: it is remembered, takes the
    /// group's place and the keyboard; the tab that was showing hides.
    pub(crate) fn show_tab(&mut self, group: &TabGroupId, slot: &SlotId) {
        self.hiding.resume();
        if let Some(a) = &mut self.active {
            a.shown.remove(group);
        }
        self.edit_layout(|l| l.show_tab(slot));
        self.sync_tabs();
        if let Some(w) = self.active.as_ref().and_then(|a| a.windows.get(slot)).copied()
            && !self.hiding.hidden.contains_key(&w)
        {
            self.bring_forward(w);
        }
    }

    /// Brings `w` to the front with the keyboard, its strip and frame above it. The raise lands
    /// when the window's app gets round to it, often after the strip was put above the window:
    /// they go back above it once it is up (the owner: "the tabs go behind the new window" when a
    /// tab was clicked; a window dropped on a tag "overlaps the tabs and we only see the window").
    pub(crate) fn bring_forward(&mut self, w: WindowHandle) {
        let hwnd = window::hwnd(w);
        let _ = arrange::raise(hwnd);
        arrange::focus(hwnd);
        self.restack_soon();
    }

    /// Puts strips and frames back above their windows in a moment, twice: soon after a window
    /// was raised, and again in case its app was slow.
    pub(crate) fn restack_soon(&mut self) {
        self.watch.stack_passes = 2;
        self.set_timer(timer::STACK, STACK_FIRST_MS);
    }

    /// One of the passes of [`Engine::restack_soon`].
    pub(crate) fn stack_tick(&mut self) {
        self.kill_timer(timer::STACK);
        self.restack_overlays();
        self.watch.stack_passes = self.watch.stack_passes.saturating_sub(1);
        if self.watch.stack_passes > 0 {
            self.set_timer(timer::STACK, STACK_AGAIN_MS);
        }
    }

    /// Every shown tab strip and frame right above its window, where a window raised over them
    /// may have left them behind.
    pub(crate) fn restack_overlays(&mut self) {
        for g in self.groups_now() {
            if let Some((_, w)) = g.visible()
                && let Some(bar) = self.watch.bars.get(&g.id)
                && bar.is_shown()
            {
                mosaik_overlay::layered::restack_windows(
                    &[bar.hwnd()],
                    Stack::Above(w.0.cast_signed_isize()),
                );
            }
        }
        self.restack_frames();
    }

    /// The tab after the one showing (the "+N" chip of a crowded strip).
    fn show_next_tab(&mut self, group: &TabGroupId) {
        let Some(g) = self.groups_now().into_iter().find(|g| &g.id == group) else {
            return;
        };
        let Some((visible, _)) = g.visible().cloned() else {
            return;
        };
        let i = g.open.iter().position(|(s, _)| *s == visible).unwrap_or(0);
        if let Some((next, _)) = g.open.get((i + 1) % g.open.len().max(1)).cloned() {
            self.show_tab(group, &next);
        }
    }

    /// The slot of a window in the open workspace, adding the window to the workspace (as a
    /// window of its own, at `place`) when it is not part of it.
    pub(crate) fn slot_for_window(&mut self, h: WindowHandle, place: Place) -> Option<SlotId> {
        if let Some(s) = self.active.as_ref()?.slot_of(h) {
            return Some(s.clone());
        }
        let facts = self.scanner.facts(window::hwnd(h))?;
        let path = self.scanner.process_path(facts.pid);
        let label = self.apps.label(&facts, path.as_deref());
        let id = {
            let active = self.active.as_ref()?;
            let ws = self.file.workspaces.iter_mut().find(|w| w.id == active.id)?;
            let taken: HashSet<String> = ws.layout.slots.iter().map(|s| s.id.0.clone()).collect();
            let id = SlotId(
                (1..)
                    .map(|n| format!("s{n}"))
                    .find(|s| !taken.contains(s))
                    .unwrap_or_else(|| "s0".into()),
            );
            ws.layout.slots.insert(
                0,
                Slot {
                    id: id.clone(),
                    label: Some(label),
                    app: app_match(&facts),
                    launch: apps::launch_for(&facts, path.as_deref()),
                    place,
                },
            );
            id
        };
        if let Some(a) = &mut self.active {
            a.windows.insert(id.clone(), h);
        }
        Some(id)
    }

    /// A window dropped on another window's tag or tab strip becomes a tab with it, just before
    /// the tab `before` (at the end when `None`), and shows in the group's place.
    pub(crate) fn join_group(
        &mut self,
        onto: WindowHandle,
        dropped: WindowHandle,
        before: Option<WindowHandle>,
    ) {
        self.hiding.resume();
        // Each way this can fail says so: the window stays where it was dropped.
        let not_joined = |engine: &mut Self, why: &str| {
            tracing::warn!(why, "window not made a tab");
            engine.tell_once(
                "tab-join",
                "A window did not become a tab",
                format!("{why} It stays where it was dropped."),
            );
        };
        let Some(onto_slot) = self.active.as_ref().and_then(|a| a.slot_of(onto)).cloned() else {
            not_joined(
                self,
                "The window it was dropped on is not part of the open workspace any more.",
            );
            return;
        };
        let left = self.group_of_window(dropped).map(|g| g.id);
        let Some(dropped_slot) = self.slot_for_window(dropped, Place::Maximized { monitor: 0 }) else {
            not_joined(
                self,
                "Mosaïk could not read the dropped window (it may have closed).",
            );
            return;
        };
        let before = before.and_then(|b| self.active.as_ref().and_then(|a| a.slot_of(b)).cloned());
        let Some(Some(group)) = self.edit_layout(|l| l.join_tab(&onto_slot, &dropped_slot, before.as_ref()))
        else {
            not_joined(self, "These two windows cannot share a tab group.");
            return;
        };
        if let Some(a) = &mut self.active {
            a.shown.remove(&group);
            if let Some(l) = &left {
                a.shown.remove(l);
            }
        }
        if let Some(to) = self.group_target(&group) {
            for o in mover::move_all(&[(dropped, to)], mover::WAIT_ONE) {
                if let Err(e) = o.result {
                    self.tell_once(
                        "tab-move",
                        "A window did not move into its tab group",
                        format!("{e}. It stays where it is, as the group's visible tab."),
                    );
                }
            }
        }
        self.sync_tabs();
        self.bring_forward(dropped);
        self.refresh_frames();
    }

    /// A tab pulled out of its strip becomes a window again: into the zone `zone`, or where it
    /// was dropped. The group's next tab shows in its place.
    pub(crate) fn pull_out(&mut self, group: &TabGroupId, slot: &SlotId, drop: Point, zone: Option<ZoneId>) {
        self.hiding.resume();
        let Some(g) = self.groups_now().into_iter().find(|g| &g.id == group) else {
            return;
        };
        let Some(window) = g.open.iter().find(|(s, _)| s == slot).map(|(_, w)| *w) else {
            return;
        };
        let Some(topology) = self.topology.clone() else {
            return;
        };
        let style = self.style();
        let live = zone.and_then(|z| {
            self.active
                .as_ref()
                .and_then(|a| a.zones.iter().find(|lz| lz.id == z).cloned())
        });
        // Its zone's window area, or its own size centred under the pointer.
        let target = match &live {
            Some(z) => style.window_area(z.rect, z.dpi),
            None => {
                let size = match self.group_target(group) {
                    Some(To::Frame(r)) => (r.width(), r.height()),
                    _ => (800, 600),
                };
                let monitor = topology.nearest_monitor(drop);
                Rect::from_origin_size(drop.x - size.0 / 2, drop.y - 12, size.0, size.1)
                    .fit_inside(monitor.work_area)
            }
        };
        let monitor = topology.monitor_for_rect(target).clone();
        if let Some(a) = &mut self.active {
            a.shown.remove(group);
        }
        self.edit_layout(|l| {
            let place = match &live {
                Some(z) => Place::Zone { zone: z.id.clone() },
                None => Place::Free {
                    monitor: l.monitor_index(&monitor, &topology),
                    area: RatioRect::from_rect(target, monitor.work_area).unwrap_or(RatioRect::FULL),
                },
            };
            l.leave_tab(slot, place)
        });
        for o in mover::move_all(&[(window, To::Frame(target))], mover::WAIT_ONE) {
            if let Err(e) = o.result {
                self.tell_once(
                    "tab-move",
                    "A window did not move out of its tab group",
                    format!("{e}. It stays where it is."),
                );
            }
        }
        self.sync_tabs();
        self.bring_forward(window);
        self.refresh_frames();
    }

    // ----- The mouse on tab strips -----

    /// Mouse input on tab strips: a click shows a tab, a drag along the strip reorders it, a
    /// drag away from the strip pulls it out (zones show, as when dragging a window).
    pub(crate) fn on_bar_inputs(&mut self, inputs: Vec<BarInput>) {
        for input in inputs {
            match input {
                BarInput::Down { bar, at } => {
                    // A press whose release never came ends now.
                    self.end_bar_drag();
                    self.watch.band_press = None;
                    if let Some(h) = self.band_owner(bar) {
                        self.band_down(h, at);
                        continue;
                    }
                    let Some((group, hit)) = self
                        .watch
                        .bars
                        .iter()
                        .find(|(_, b)| b.is(bar))
                        .map(|(g, b)| (g.clone(), b.hit(at)))
                    else {
                        continue;
                    };
                    match hit {
                        Some(Hit::Tab(id)) => {
                            let w = WindowHandle(id);
                            let Some(g) = self.groups_now().into_iter().find(|n| n.id == group) else {
                                continue;
                            };
                            let Some((slot, _)) = g.open.iter().find(|(_, x)| *x == w).cloned() else {
                                continue;
                            };
                            self.watch.bar_drag = Some(BarDrag {
                                group,
                                slot,
                                start: at,
                                mode: BarMode::Pressed,
                                zones: Vec::new(),
                                target: None,
                                reordered: false,
                            });
                            self.set_timer(timer::DRAG, DRAG_TICK_MS);
                        }
                        Some(Hit::More) => self.show_next_tab(&group),
                        // The strip's band, or the zone's tag at its start: the title bar of the
                        // tab that shows, as a window's band is.
                        None => {
                            let visible = self
                                .groups_now()
                                .into_iter()
                                .find(|n| n.id == group)
                                .and_then(|g| g.visible().map(|(_, w)| *w));
                            if let Some(w) = visible {
                                self.band_down(w, at);
                            }
                        }
                    }
                }
                BarInput::Move { at, .. } if self.watch.band_press.is_some() => self.band_move(at),
                BarInput::Move { at, .. } => self.bar_drag_to(at),
                BarInput::Up { at, .. } if self.watch.band_press.is_some() => self.band_up(at),
                BarInput::Up { at, .. } => self.bar_drop(at),
                BarInput::Cancel { .. } => {
                    self.end_bar_drag();
                    self.band_cancel();
                }
            }
        }
        self.hold_esc();
    }

    /// On the drag timer: Esc ends a press on a strip, or on a band (its window goes back).
    /// Esc is held by Mosaïk meanwhile (`hold_esc`); this reads it when another app holds it.
    pub(crate) fn bar_tick(&mut self) {
        if (self.watch.bar_drag.is_some() || self.watch.band_press.is_some()) && esc_down() {
            self.cancel_presses();
        }
    }

    /// Esc during a press on a strip or a band: the press ends, and a window dragged by its band
    /// goes back.
    pub(crate) fn cancel_presses(&mut self) {
        tabbar::cancel_press();
        self.end_bar_drag();
        self.band_cancel();
        self.hold_esc();
    }

    /// Ends a press on a strip without doing anything more (a reorder made so far is kept).
    fn end_bar_drag(&mut self) {
        if let Some(d) = self.watch.bar_drag.take() {
            self.after_bar_drag(&d);
        }
    }

    fn after_bar_drag(&mut self, d: &BarDrag) {
        if d.mode == BarMode::PullingOut {
            self.watch.preview.hide();
            self.watch.preview_visible = false;
        }
        if self.watch.drag.is_none() {
            self.kill_timer(timer::DRAG);
        }
        if d.reordered {
            self.persist();
        }
    }

    fn bar_drag_to(&mut self, at: Point) {
        let Some(d) = self.watch.bar_drag.clone() else {
            return;
        };
        let dpi = self.topology.as_ref().map_or(96, |t| t.nearest_monitor(at).dpi);
        let threshold = scale_for_dpi(6, dpi);
        let away = scale_for_dpi(28, dpi);
        let strip = self.watch.bars.get(&d.group).and_then(TabBar::rect);
        let moved = (at.x - d.start.x).abs().max((at.y - d.start.y).abs());
        let outside = strip.is_none_or(|s| at.y < s.top - away || at.y > s.bottom + away);
        let mut mode = d.mode;
        if mode == BarMode::Pressed && moved > threshold {
            mode = BarMode::Reordering;
        }
        if mode == BarMode::Reordering && outside {
            mode = BarMode::PullingOut;
            let zones: Vec<SnapZone> = self
                .active
                .as_ref()
                .map(|a| {
                    a.zones
                        .iter()
                        .map(|z| SnapZone {
                            id: z.id.clone(),
                            rect: z.rect,
                            dpi: z.dpi,
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(bd) = &mut self.watch.bar_drag {
                bd.zones = zones;
            }
        } else if mode == BarMode::PullingOut && !outside {
            // Back on the strip: reordering again, and a release there pulls nothing out.
            mode = BarMode::Reordering;
            self.watch.preview.hide();
            self.watch.preview_visible = false;
        }
        if let Some(bd) = &mut self.watch.bar_drag {
            bd.mode = mode;
        }
        match mode {
            BarMode::Reordering => {
                let before = self
                    .watch
                    .bars
                    .get(&d.group)
                    .and_then(|b| b.insert_before(at))
                    .map(WindowHandle);
                let before = before.and_then(|b| self.active.as_ref().and_then(|a| a.slot_of(b)).cloned());
                let slot = d.slot.clone();
                let changed = self
                    .edit_layout_quietly(|l| l.move_tab_before(&slot, before.as_ref()))
                    .is_some_and(|(moved, _)| moved);
                if changed {
                    if let Some(bd) = &mut self.watch.bar_drag {
                        bd.reordered = true;
                    }
                    self.refresh_strip(&d.group);
                }
            }
            BarMode::PullingOut => {
                // Zones show as for a window dragged by its title bar: Shift (or the setting)
                // moves it freely.
                let snapping = self.settings.snap.active(shift_down());
                let target = if snapping {
                    self.watch.bar_drag.as_ref().and_then(|bd| {
                        let id = snap::target(at, &bd.zones, self.settings.snap)?;
                        bd.zones.iter().position(|z| &z.id == id)
                    })
                } else {
                    None
                };
                if let Some(bd) = &mut self.watch.bar_drag {
                    bd.target = target;
                }
                if snapping {
                    if !self.watch.preview_visible {
                        self.show_zone_preview(Stack::Topmost);
                    }
                    self.set_preview_target(target);
                } else {
                    self.watch.preview.hide();
                    self.watch.preview_visible = false;
                }
            }
            BarMode::Pressed => {}
        }
    }

    fn bar_drop(&mut self, at: Point) {
        let Some(d) = self.watch.bar_drag.take() else {
            return;
        };
        self.after_bar_drag(&d);
        match d.mode {
            BarMode::Pressed => self.show_tab(&d.group, &d.slot),
            BarMode::Reordering => {}
            BarMode::PullingOut => {
                let zone = d.target.and_then(|i| d.zones.get(i)).map(|z| z.id.clone());
                self.pull_out(&d.group, &d.slot, at, zone);
            }
        }
    }
}

/// Whether a window must move to reach `to` (a window already there is left alone, and one in
/// full screen too).
fn needs_move(w: WindowHandle, to: To) -> bool {
    let Some(g) = window::geometry(window::hwnd(w)).filter(|g| !g.full_screen) else {
        return false;
    };
    match to {
        To::Frame(r) => g.maximized || edge_distance(g.frame, r) > 2,
        To::Maximized(_) => !g.maximized,
    }
}

/// Window handles as the overlay crate takes them (a pointer-sized integer).
pub(crate) trait AsIsize {
    fn cast_signed_isize(self) -> isize;
}

impl AsIsize for u64 {
    fn cast_signed_isize(self) -> isize {
        // Handles are 32-bit values sign-extended to pointer size (see `mosaik_win::window`).
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let v = self as isize;
        v
    }
}
