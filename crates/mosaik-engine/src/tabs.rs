//! Tabs for any app: windows that share one place, one visible at a time, with a tab strip in
//! the band above them (ADR 0003 for how background tabs are hidden).
//!
//! A window is hidden only after the journal says how to bring it back, and only while the
//! watchdog runs (ADR 0006): whatever happens to Mosaïk, hidden windows come back.

use std::collections::HashMap;
use std::time::Duration;

use mosaik_core::geometry::{Point, RatioRect, Rect, scale_for_dpi};
use mosaik_core::journal::{HiddenWindow, HideMethod, Journal};
use mosaik_core::layout::{Place, Slot, SlotId, TabGroupId};
use mosaik_core::place::{Resolver, Target};
use mosaik_core::snap::{self, LiveZone as SnapZone};
use mosaik_core::window::WindowHandle;
use mosaik_core::workspace::app_match;
use mosaik_core::zone::ZoneId;
use mosaik_overlay::frame::ZoneView;
use mosaik_overlay::layered::Stack;
use mosaik_overlay::tabbar::{BarInput, BarLook, TabBar, TabView};
use mosaik_win::hide::{Hidden, Hider};
use mosaik_win::{apps, arrange, window};

use crate::mover::{self, To};
use crate::thread::Engine;

/// Windows Mosaïk has hidden, how, and the journal the watchdog reads.
pub(crate) struct Hiding {
    hider: Hider,
    pub journal: Journal,
    pub hidden: HashMap<WindowHandle, Hidden>,
}

impl Hiding {
    pub(crate) fn new(owner_pid: u32) -> Self {
        Self {
            hider: Hider::new(),
            journal: Journal::new(owner_pid),
            hidden: HashMap::new(),
        }
    }
}

/// A tab group of the open workspace as it is on screen: its tabs' windows (in tab order) and
/// the one that shows.
#[derive(Clone, Debug)]
pub(crate) struct GroupNow {
    pub id: TabGroupId,
    pub tabs: Vec<(SlotId, WindowHandle)>,
    pub visible: Option<(SlotId, WindowHandle)>,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarMode {
    Pressed,
    Reordering,
    PullingOut,
}

impl Engine {
    /// Hides a window as a background tab. Refused (with the reason) when the watchdog is not
    /// running, when the journal cannot be written, or when Windows will not hide it.
    pub(crate) fn hide_window(&mut self, h: WindowHandle) -> Result<(), String> {
        if !self.config.watchdog {
            return Err("Mosaïk's safety net (its watchdog) is not running, so it hides no window".into());
        }
        if self.hiding.hidden.contains_key(&h) {
            return Ok(());
        }
        let hwnd = window::hwnd(h);
        let facts = self.scanner.facts(hwnd).ok_or("the window closed")?;
        let preferred = self.settings.tab_hiding.unwrap_or(HideMethod::ShellCloak);
        let mut methods = vec![preferred];
        if preferred != HideMethod::Minimize {
            methods.push(HideMethod::Minimize);
        }
        let mut last = String::new();
        for method in methods {
            // The journal first: if Mosaïk stops between these two steps, the watchdog still
            // knows the window.
            self.hiding.journal.record(HiddenWindow {
                handle: h,
                pid: facts.pid,
                method,
                restore_rect: facts.window_rect,
                was_maximized: facts.maximized,
                exe: facts.exe.clone(),
                title: facts.title.clone(),
            });
            if let Err(e) = self.store.save(&self.hiding.journal) {
                self.hiding.journal.forget(h);
                return Err(format!("the list of hidden windows could not be saved ({e})"));
            }
            match self.hiding.hider.hide(hwnd, method) {
                Ok(hidden) => {
                    self.hiding.hidden.insert(h, hidden);
                    return Ok(());
                }
                Err(e) => {
                    last = e.to_string();
                    self.hiding.journal.forget(h);
                    let _ = self.store.save(&self.hiding.journal);
                }
            }
        }
        Err(last)
    }

    /// Shows a window Mosaïk hid.
    pub(crate) fn show_window(&mut self, h: WindowHandle) {
        let Some(hidden) = self.hiding.hidden.remove(&h) else {
            return;
        };
        if let Err(e) = self.hiding.hider.show(window::hwnd(h), &hidden) {
            tracing::warn!(error = %e, "hidden window not shown");
        }
        self.hiding.journal.forget(h);
        if let Err(e) = self.store.save(&self.hiding.journal) {
            tracing::warn!(error = %e, "journal not saved");
        }
    }

    /// A hidden window closed: nothing to bring back any more.
    pub(crate) fn forget_hidden(&mut self, h: WindowHandle) {
        if self.hiding.hidden.remove(&h).is_some() {
            self.hiding.journal.forget(h);
            let _ = self.store.save(&self.hiding.journal);
        }
    }

    /// Every window Mosaïk hid comes back (rescue, safe exit); the journal is emptied.
    pub(crate) fn show_all_hidden(&mut self) {
        let all: Vec<WindowHandle> = self.hiding.hidden.keys().copied().collect();
        for h in all {
            self.show_window(h);
        }
    }

    /// The open workspace's tab groups, with the windows filling their tabs.
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
            .map(|g| {
                let tabs: Vec<(SlotId, WindowHandle)> = g
                    .tabs
                    .iter()
                    .filter_map(|s| active.windows.get(s).map(|w| (s.clone(), *w)))
                    .collect();
                let visible = tabs
                    .iter()
                    .find(|(s, _)| *s == g.active)
                    .or_else(|| tabs.first())
                    .cloned();
                GroupNow {
                    id: g.id.clone(),
                    tabs,
                    visible,
                }
            })
            .collect()
    }

    /// The group a window is a tab of, in the open workspace.
    pub(crate) fn group_of_window(&self, h: WindowHandle) -> Option<GroupNow> {
        self.groups_now()
            .into_iter()
            .find(|g| g.tabs.iter().any(|(_, w)| *w == h))
    }

    /// Makes the hidden and shown windows of every group match the layout: the visible tab
    /// shows, the others hide. Used after a switch and after changes to groups.
    pub(crate) fn apply_groups(&mut self) {
        for g in self.groups_now() {
            let Some((_, visible)) = g.visible else { continue };
            if self.hiding.hidden.contains_key(&visible) {
                self.show_window(visible);
            }
            for (_, w) in &g.tabs {
                if *w != visible
                    && let Err(e) = self.hide_window(*w)
                {
                    tracing::warn!(error = %e, "background tab not hidden");
                    self.tabs_unavailable(&e);
                }
            }
        }
    }

    /// Says once why tabs cannot hide windows.
    fn tabs_unavailable(&mut self, why: &str) {
        let key = "tabs-unavailable".to_owned();
        if self.settings.notices_shown.contains(&key) || self.told_tabs {
            return;
        }
        self.told_tabs = true;
        self.notice(
            "Tabs could not hide a window",
            format!("{why}. The windows of this tab group stay stacked in their place instead."),
        );
    }

    /// Where a group's windows go: where its visible window is, if it is on screen, else the
    /// group's place in the layout.
    fn group_rect(&self, g: &GroupNow) -> Option<Rect> {
        if let Some((_, w)) = &g.visible
            && !self.hiding.hidden.contains_key(w)
            && let Some(geo) = window::geometry(window::hwnd(*w))
            && geo.framable()
        {
            return Some(geo.frame);
        }
        let active = self.active.as_ref()?;
        let ws = self.file.workspaces.iter().find(|w| w.id == active.id)?;
        let topology = self.topology.as_ref()?;
        let group = ws.layout.tab_group(&g.id)?;
        match Resolver::new(&ws.layout, topology, self.style()).target(&group.place)? {
            Target::Frame(r) => Some(r),
            Target::Maximized(_) => None,
        }
    }

    /// Shows the tab `slot` of `group`: its window takes the group's place and the keyboard, the
    /// tab that was showing hides.
    pub(crate) fn show_tab(&mut self, group: &TabGroupId, slot: &SlotId) {
        let Some(g) = self.groups_now().into_iter().find(|g| &g.id == group) else {
            return;
        };
        let Some((_, target)) = g.tabs.iter().find(|(s, _)| s == slot).cloned() else {
            return;
        };
        let previous = g.visible.clone();
        if previous.as_ref().is_some_and(|(_, w)| *w == target) && !self.hiding.hidden.contains_key(&target) {
            arrange::focus(window::hwnd(target));
            return;
        }
        if let Some(rect) = self.group_rect(&g) {
            for o in mover::move_all(&[(target, To::Frame(rect))], Duration::from_millis(1500)) {
                if let Err(e) = o.result {
                    tracing::debug!(error = %e, "tab not moved into place");
                }
            }
        }
        self.show_window(target);
        let hwnd = window::hwnd(target);
        let _ = arrange::raise(hwnd);
        arrange::focus(hwnd);
        if let Some((_, old)) = previous
            && old != target
            && let Err(e) = self.hide_window(old)
        {
            self.tabs_unavailable(&e);
        }
        self.edit_layout(|layout| {
            layout.show_tab(slot);
        });
        self.refresh_frames();
    }

    /// Changes the open workspace's layout and saves it.
    pub(crate) fn edit_layout(&mut self, change: impl FnOnce(&mut mosaik_core::layout::Layout)) {
        let Some(active) = &self.active else { return };
        let Some(ws) = self.file.workspaces.iter_mut().find(|w| w.id == active.id) else {
            return;
        };
        change(&mut ws.layout);
        self.persist();
    }

    /// The slot of a window in the open workspace, adding the window to the workspace (as a
    /// window of its own, at `place`) when it is not part of it.
    pub(crate) fn slot_for_window(&mut self, h: WindowHandle, place: Place) -> Option<SlotId> {
        if let Some(s) = self.active.as_ref()?.slot_of(h) {
            return Some(s.clone());
        }
        let facts = self.scanner.facts(window::hwnd(h))?;
        let path = self.scanner.process_path(facts.pid);
        let id = {
            let active = self.active.as_ref()?;
            let ws = self.file.workspaces.iter_mut().find(|w| w.id == active.id)?;
            let taken: std::collections::HashSet<String> =
                ws.layout.slots.iter().map(|s| s.id.0.clone()).collect();
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

    /// A window dropped on another window's tag or tab strip becomes a tab with it, at position
    /// `at`, and shows.
    pub(crate) fn join_group(&mut self, onto: WindowHandle, dropped: WindowHandle, at: Option<usize>) {
        let Some(onto_slot) = self.active.as_ref().and_then(|a| a.slot_of(onto)).cloned() else {
            return;
        };
        let Some(rect) = window::geometry(window::hwnd(onto)).map(|g| g.frame) else {
            return;
        };
        // The window that shows now in that place, to hide once the dropped one is there.
        let showing = self
            .group_of_window(onto)
            .and_then(|g| g.visible.map(|(_, w)| w))
            .unwrap_or(onto);
        let Some(dropped_slot) = self.slot_for_window(dropped, Place::Maximized { monitor: 0 }) else {
            return;
        };
        let mut joined = None;
        self.edit_layout(|layout| {
            joined = layout.join_tab(&onto_slot, &dropped_slot, at);
        });
        if joined.is_none() {
            return;
        }
        for o in mover::move_all(&[(dropped, To::Frame(rect))], Duration::from_millis(1500)) {
            if let Err(e) = o.result {
                tracing::debug!(error = %e, "tab not moved into place");
            }
        }
        self.show_window(dropped);
        arrange::focus(window::hwnd(dropped));
        if showing != dropped
            && let Err(e) = self.hide_window(showing)
        {
            self.tabs_unavailable(&e);
        }
        self.refresh_frames();
    }

    /// A tab pulled out of its strip becomes a window again: into the zone `zone`, or where it
    /// was dropped. The group's next tab shows in its place.
    pub(crate) fn pull_out(&mut self, group: &TabGroupId, slot: &SlotId, drop: Point, zone: Option<ZoneId>) {
        let Some(g) = self.groups_now().into_iter().find(|g| &g.id == group) else {
            return;
        };
        let Some((_, window)) = g.tabs.iter().find(|(s, _)| s == slot).cloned() else {
            return;
        };
        let Some(topology) = self.topology.clone() else {
            return;
        };
        let group_rect = self.group_rect(&g);
        let style = self.style();
        let live = zone.as_ref().and_then(|z| {
            self.active
                .as_ref()
                .and_then(|a| a.zones.iter().find(|lz| &lz.id == z).cloned())
        });
        // Where it goes: its zone's window area, or its own size centred under the pointer.
        let (place, target) = match &live {
            Some(z) => (
                Place::Zone { zone: z.id.clone() },
                style.window_area(z.rect, z.dpi),
            ),
            None => {
                let size = group_rect.map_or((800, 600), |r| (r.width(), r.height()));
                let monitor = topology.nearest_monitor(drop);
                let r = Rect::from_origin_size(drop.x - size.0 / 2, drop.y - 12, size.0, size.1)
                    .fit_inside(monitor.work_area);
                let index = self
                    .active
                    .as_ref()
                    .and_then(|a| self.file.workspaces.iter().find(|w| w.id == a.id))
                    .and_then(|ws| ws.layout.monitors.iter().position(|m| m.id == monitor.id))
                    .unwrap_or(0);
                let area = RatioRect::from_rect(r, monitor.work_area).unwrap_or(RatioRect::FULL);
                (Place::Free { monitor: index, area }, r)
            }
        };
        self.edit_layout(|layout| {
            layout.leave_tab(slot, place);
        });
        for o in mover::move_all(&[(window, To::Frame(target))], Duration::from_millis(1500)) {
            if let Err(e) = o.result {
                tracing::debug!(error = %e, "pulled-out tab not placed");
            }
        }
        self.show_window(window);
        let hwnd = window::hwnd(window);
        let _ = arrange::raise(hwnd);
        arrange::focus(hwnd);
        // The group's new visible tab (or the last window left) takes the group's place.
        if let Some(rest) = self.groups_now().into_iter().find(|n| &n.id == group)
            && let Some((_, next)) = rest.visible
        {
            if let Some(rect) = group_rect {
                for o in mover::move_all(&[(next, To::Frame(rect))], Duration::from_millis(1500)) {
                    let _ = o.result;
                }
            }
            self.show_window(next);
        } else if let Some((_, last)) = g.tabs.iter().find(|(s, _)| s != slot).cloned() {
            // The group ended: its last window is a window of its own again.
            self.show_window(last);
        }
        self.refresh_frames();
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

    /// Shows a group's strip above its visible window (`frame`), in the zone's colours.
    pub(crate) fn show_bar(
        &mut self,
        g: &GroupNow,
        frame: Rect,
        view: &ZoneView,
        window: WindowHandle,
        drop_at: Option<usize>,
    ) {
        let band_h = scale_for_dpi(self.style().tag_band.max(24), view.dpi);
        let band = Rect::new(frame.left, frame.top - band_h, frame.right, frame.top);
        let tabs: Vec<TabView> = g
            .tabs
            .iter()
            .map(|(_, w)| TabView {
                title: tab_title(*w),
                active: g.visible.as_ref().is_some_and(|(_, v)| v == w),
            })
            .collect();
        let look = BarLook {
            color: view.color,
            label: view.name.clone(),
            dpi: view.dpi,
            dark: self.dark_theme(),
        };
        if self.canvas_ready().is_none() {
            return;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        if let Some(bar) = self.bar(&g.id)
            && let Err(e) = bar.show(
                &mut canvas,
                band,
                &look,
                &tabs,
                drop_at,
                Stack::Above(window.0.cast_signed_isize()),
            )
        {
            tracing::debug!(error = %e, "tab strip not drawn");
        }
        self.watch.put_canvas(canvas);
    }

    pub(crate) fn hide_bar(&mut self, group: &TabGroupId) {
        if let Some(b) = self.watch.bars.get_mut(group) {
            b.hide();
        }
    }

    /// Mouse input on tab strips: a click shows a tab, a drag along the strip reorders it, a
    /// drag away from the strip pulls it out (zones show, as when dragging a window).
    pub(crate) fn on_bar_inputs(&mut self, inputs: Vec<BarInput>) {
        for input in inputs {
            match input {
                BarInput::Down { bar, at } => {
                    let Some((group, index)) = self
                        .watch
                        .bars
                        .iter()
                        .find(|(_, b)| b.is(bar))
                        .map(|(g, b)| (g.clone(), b.hit(at)))
                    else {
                        continue;
                    };
                    let Some(index) = index else { continue };
                    let Some(g) = self.groups_now().into_iter().find(|n| n.id == group) else {
                        continue;
                    };
                    let Some((slot, _)) = g.tabs.get(index).cloned() else {
                        continue;
                    };
                    self.watch.bar_drag = Some(BarDrag {
                        group,
                        slot,
                        start: at,
                        mode: BarMode::Pressed,
                        zones: Vec::new(),
                        target: None,
                    });
                }
                BarInput::Move { at, .. } => self.bar_drag_to(at),
                BarInput::Up { at, .. } => self.bar_drop(at),
                BarInput::Cancel { .. } => {
                    if self.watch.bar_drag.take().is_some() {
                        self.watch.preview.hide();
                    }
                }
            }
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
        let mut mode = d.mode;
        if mode == BarMode::Pressed && moved > threshold {
            mode = BarMode::Reordering;
        }
        let outside = strip.is_none_or(|s| at.y < s.top - away || at.y > s.bottom + away);
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
            self.show_zone_preview(Stack::Topmost);
        }
        if let Some(bd) = &mut self.watch.bar_drag {
            bd.mode = mode;
        }
        match mode {
            BarMode::Reordering => {
                let Some(to) = self.watch.bars.get(&d.group).map(|b| b.slot_for(at)) else {
                    return;
                };
                let Some(g) = self.groups_now().into_iter().find(|n| n.id == d.group) else {
                    return;
                };
                let Some(from) = g.tabs.iter().position(|(s, _)| *s == d.slot) else {
                    return;
                };
                // Dropping a tab on its own right half keeps it where it is.
                let to = if to > from { to - 1 } else { to };
                if to != from {
                    let slot = d.slot.clone();
                    self.edit_layout(|layout| layout.move_tab(&slot, to));
                    self.refresh_frames();
                }
            }
            BarMode::PullingOut => {
                let target = self.watch.bar_drag.as_ref().and_then(|bd| {
                    let id = snap::target(at, &bd.zones, self.settings.snap)?;
                    bd.zones.iter().position(|z| &z.id == id)
                });
                if let Some(bd) = &mut self.watch.bar_drag {
                    bd.target = target;
                }
                self.set_preview_target(target);
            }
            BarMode::Pressed => {}
        }
    }

    fn bar_drop(&mut self, at: Point) {
        let Some(d) = self.watch.bar_drag.take() else {
            return;
        };
        match d.mode {
            BarMode::Pressed => self.show_tab(&d.group, &d.slot),
            BarMode::Reordering => {}
            BarMode::PullingOut => {
                self.watch.preview.hide();
                let zone = d.target.and_then(|i| d.zones.get(i)).map(|z| z.id.clone());
                self.pull_out(&d.group, &d.slot, at, zone);
            }
        }
    }
}

/// What a tab chip says: the window's title, or its program when it has none.
fn tab_title(h: WindowHandle) -> String {
    let hwnd = window::hwnd(h);
    let title = window::title(hwnd);
    let t = title.trim();
    if t.is_empty() {
        "Window".to_owned()
    } else {
        t.to_owned()
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
