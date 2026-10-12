//! Watching windows: the frames, tags and tab strips windows wear in their zones, the zones
//! shown while a window is dragged, the pull into a zone when it is dropped, and dropping a
//! window on another's tag to make them tabs.
//!
//! Most events come from global hooks. Moves (`EVENT_OBJECT_LOCATIONCHANGE`) are hooked per
//! process, only for the apps whose windows wear a frame or are being dragged: a global hook
//! would also report every movement of the mouse pointer.

use std::collections::{HashMap, HashSet};

use mosaik_core::geometry::{Insets, Point, Rect, scale_for_dpi};
use mosaik_core::layout::{Place, TabGroupId};
use mosaik_core::snap::{self, LiveZone as SnapZone};
use mosaik_core::window::WindowHandle;
use mosaik_overlay::canvas::Canvas;
use mosaik_overlay::frame::{Frame, FrameLook, Preview, ZoneView};
use mosaik_overlay::layered::Stack;
use mosaik_overlay::tabbar::{self, BarLook, TabBar, TabView};
use mosaik_win::events::{self, Hooks, WinEvent};
use mosaik_win::{input, window};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, OBJID_WINDOW,
};

use crate::mover::{self, To};
use crate::tabs::{AsIsize, BarDrag};
use crate::thread::{Engine, WM_EVENTS, timer};

/// Events hooked for every window.
const GLOBAL: [(u32, u32); 6] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
    (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];

/// How often the drag preview follows the pointer and the Shift key while the window itself
/// is not moving (held at a screen edge, Shift pressed without moving), and Esc is checked
/// during a drag on a tab strip.
pub(crate) const DRAG_TICK_MS: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragKind {
    /// Grabbed away from the edges (a move), not moved yet.
    Undecided,
    Moving,
    /// Grabbed by an edge or a corner, or only resized: never snaps.
    Resizing,
}

pub(crate) struct Drag {
    pub window: WindowHandle,
    pub pid: u32,
    pub start: Rect,
    pub kind: DragKind,
    /// The zones shown, in the preview's order.
    pub zones: Vec<SnapZone>,
    pub target: Option<usize>,
    /// The window whose tag or tab strip is under the pointer, and the tab the dropped window
    /// would go before (the end when `None`).
    pub tab_target: Option<(WindowHandle, Option<WindowHandle>)>,
}

struct Framed {
    pid: u32,
    frame: Frame,
}

/// What a framed window wears.
struct FrameInfo {
    rect: Rect,
    look: FrameLook,
    pid: u32,
    zone: ZoneView,
}

#[derive(Default)]
pub(crate) struct Watch {
    hooks: Option<Hooks>,
    /// Move hooks, by process id.
    location: HashMap<u32, Hooks>,
    canvas: Option<Canvas>,
    /// Drawing is not possible (Direct2D unavailable): frames are off, said once.
    canvas_failed: bool,
    frames: HashMap<WindowHandle, Framed>,
    pub(crate) preview: Preview,
    /// Each tab group's strip.
    pub(crate) bars: HashMap<TabGroupId, TabBar>,
    /// Groups whose strip could not be drawn: they show all their tabs until it can.
    pub(crate) strip_broken: HashSet<TabGroupId>,
    /// A press on a tab chip.
    pub(crate) bar_drag: Option<BarDrag>,
    /// The strip shown over a single window's tag while another window is dragged onto it.
    drop_bar: Option<TabBar>,
    pub(crate) drag: Option<Drag>,
    /// The window the user is moving or resizing right now (any window, zones or not), so
    /// nothing moves it from under the pointer.
    pub(crate) moving: Option<WindowHandle>,
    /// The drag preview is on screen (it steps aside while Shift is held).
    pub(crate) preview_visible: bool,
    rounded: bool,
}

impl Watch {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The canvas, lent out while something else of the watch is borrowed; put it back with
    /// [`Watch::put_canvas`].
    pub(crate) fn take_canvas(&mut self) -> Option<Canvas> {
        self.canvas.take()
    }

    pub(crate) fn put_canvas(&mut self, canvas: Canvas) {
        self.canvas = Some(canvas);
    }
}

fn top_level_window(e: &WinEvent) -> bool {
    e.object == OBJID_WINDOW.0 && e.child == i32::try_from(CHILDID_SELF).unwrap_or(0) && e.window.0 != 0
}

pub(crate) fn shift_down() -> bool {
    // SAFETY: reads the key state; no preconditions.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) };
    state.cast_unsigned() & 0x8000 != 0
}

impl Engine {
    pub(crate) fn start_watching(&mut self) {
        events::wake_on_event(self.window, WM_EVENTS);
        tabbar::wake_on_input(self.window, WM_EVENTS);
        match events::hook(&GLOBAL, 0) {
            Ok(h) => self.watch.hooks = Some(h),
            Err(e) => {
                tracing::error!(error = %e, "window events not hooked");
                self.notice(
                    "Mosaïk cannot follow windows",
                    format!("{e}. Switching workspaces still works; zones will not appear while dragging."),
                );
            }
        }
        self.watch.rounded = mosaik_win::os::is_windows_11();
    }

    pub(crate) fn stop_watching(&mut self) {
        self.kill_timer(timer::DRAG);
        self.watch.drag = None;
        self.watch.bar_drag = None;
        self.watch.moving = None;
        self.watch.preview.hide();
        self.watch.frames.clear();
        self.watch.bars.clear();
        self.watch.drop_bar = None;
        self.watch.location.clear();
        self.watch.hooks = None;
    }

    /// Makes sure the canvas exists (said once if it cannot be made).
    pub(crate) fn canvas_ready(&mut self) -> Option<()> {
        if self.watch.canvas.is_none() && !self.watch.canvas_failed {
            match Canvas::new() {
                Ok(c) => self.watch.canvas = Some(c),
                Err(e) => {
                    self.watch.canvas_failed = true;
                    tracing::error!(error = %e, "overlays cannot draw");
                    self.notice(
                        "Zone frames are off",
                        format!("Mosaïk could not start drawing ({e}). Workspaces and zones still work, without frames and tags."),
                    );
                }
            }
        }
        self.watch.canvas.as_ref().map(|_| ())
    }

    /// Dark chips on tab strips: the user's choice, or Windows' app mode.
    pub(crate) fn dark_theme(&self) -> bool {
        match self.settings.theme {
            mosaik_core::settings::Theme::Dark => true,
            mosaik_core::settings::Theme::Light => false,
            mosaik_core::settings::Theme::System => !mosaik_win::os::apps_use_light_theme(),
        }
    }

    /// Handles every window event recorded since the last call, and the mouse on tab strips.
    /// Moves are merged per window: only where a window ended up matters. Tab groups are made to
    /// match the screen once, after the whole batch.
    pub(crate) fn on_events(&mut self) {
        let batch = events::drain();
        let mut moved: Vec<WindowHandle> = Vec::new();
        let mut sync = false;
        for e in batch {
            if !top_level_window(&e) {
                continue;
            }
            let h = e.window;
            // A tab of a group, or a window Mosaïk hid: what shows may change with it.
            let tab =
                |engine: &Self| engine.hiding.hidden.contains_key(&h) || engine.group_of_window(h).is_some();
            match e.event {
                EVENT_OBJECT_LOCATIONCHANGE => {
                    if !moved.contains(&h) {
                        moved.push(h);
                    }
                }
                EVENT_SYSTEM_MOVESIZESTART => self.drag_start(h),
                EVENT_SYSTEM_MOVESIZEEND => self.drag_end(h),
                EVENT_SYSTEM_FOREGROUND => sync |= self.came_forward(h),
                EVENT_SYSTEM_MINIMIZESTART | EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED => {
                    if let Some(f) = self.watch.frames.get_mut(&h) {
                        f.frame.hide();
                    }
                    sync |= tab(self);
                }
                EVENT_SYSTEM_MINIMIZEEND | EVENT_OBJECT_UNCLOAKED => {
                    self.refresh_frame(h);
                    sync |= tab(self);
                }
                EVENT_OBJECT_SHOW => {
                    self.adopt(h);
                    if self.watch.frames.contains_key(&h) {
                        self.refresh_frame(h);
                    }
                    sync |= tab(self);
                }
                // Some apps show their window first and name it a moment later; a tab's chip
                // shows its window's title.
                EVENT_OBJECT_NAMECHANGE => {
                    self.adopt(h);
                    if let Some(g) = self.group_of_window(h) {
                        self.refresh_strip(&g.id);
                    }
                }
                EVENT_OBJECT_DESTROY => sync |= self.forget(h),
                _ => {}
            }
        }
        for h in moved {
            if self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
                self.drag_update();
            } else if self.watch.frames.contains_key(&h) || self.group_of_window(h).is_some() {
                self.refresh_frame(h);
            }
        }
        let inputs = tabbar::take_inputs();
        if !inputs.is_empty() {
            self.on_bar_inputs(inputs);
        }
        if sync {
            self.sync_tabs();
        }
    }

    /// A window came to the front: its frame and strip follow it in the stacking order. A
    /// background tab brought forward (an app's notification, a link, its taskbar button when
    /// tabs are minimised) shows in its group, for now. Returns whether tabs must be synced.
    fn came_forward(&mut self, h: WindowHandle) -> bool {
        // Only if it is still in front: Windows may pass the focus through a window for a moment
        // while tabs change.
        let in_front = mosaik_win::arrange::foreground().is_some_and(|f| window::handle(f) == h);
        if in_front
            && let Some(g) = self.group_of_window(h)
            && g.visible().is_some_and(|(_, w)| *w != h)
            && let Some((slot, _)) = g.open.iter().find(|(_, w)| *w == h)
            && let Some(a) = &mut self.active
        {
            a.shown.insert(g.id.clone(), slot.clone());
            return true;
        }
        if let Some(f) = self.watch.frames.get(&h) {
            f.frame.restack(h.0.cast_signed_isize());
        }
        if let Some(g) = self.group_of_window(h)
            && let Some(bar) = self.watch.bars.get(&g.id)
            && bar.is_shown()
        {
            mosaik_overlay::layered::restack_windows(&[bar.hwnd()], Stack::Above(h.0.cast_signed_isize()));
        }
        false
    }

    /// A window closed: its frame goes; its slot stays (the workspace starts it again next
    /// time). Returns whether tabs must be synced (it was a tab: the next one shows).
    fn forget(&mut self, h: WindowHandle) -> bool {
        let tab = self.hiding.hidden.contains_key(&h) || self.group_of_window(h).is_some();
        if self.watch.frames.remove(&h).is_some() {
            self.sync_location_hooks();
        }
        self.forget_hidden(h);
        if let Some(a) = &mut self.active {
            a.windows.retain(|_, w| *w != h);
        }
        if self.watch.moving == Some(h) {
            self.watch.moving = None;
        }
        if self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
            self.kill_timer(timer::DRAG);
            self.watch.drag = None;
            self.watch.preview.hide();
            self.watch.preview_visible = false;
            self.hide_drop_bar();
            self.sync_location_hooks();
        }
        tab
    }

    pub(crate) fn hide_frames(&mut self) {
        for f in self.watch.frames.values_mut() {
            f.frame.hide();
        }
        for b in self.watch.bars.values_mut() {
            b.hide();
        }
    }

    /// Shows, moves or hides the frames and strips of every window of the open workspace.
    pub(crate) fn refresh_frames(&mut self) {
        let windows: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let keep: HashSet<WindowHandle> = windows.iter().copied().collect();
        self.watch.frames.retain(|h, _| keep.contains(h));
        for h in windows {
            self.refresh_ring(h);
        }
        self.sync_location_hooks();
        self.sync_tabs();
    }

    /// The frame of one window, and its group's strip when it is the tab that shows.
    pub(crate) fn refresh_frame(&mut self, h: WindowHandle) {
        self.refresh_ring(h);
        if let Some(g) = self.group_of_window(h)
            && g.visible().is_some_and(|(_, w)| *w == h)
        {
            self.refresh_strip(&g.id);
        }
    }

    /// The ring and tag of one window: shown around it when it sits in a zone of the open
    /// workspace, hidden otherwise. A tab of a group wears its group's strip instead of a tag.
    fn refresh_ring(&mut self, h: WindowHandle) {
        let Some(info) = self.frame_for(h) else {
            if let Some(f) = self.watch.frames.get_mut(&h) {
                f.frame.hide();
            }
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(slot) = self.watch.frames.entry(h) {
            match Frame::new() {
                Ok(frame) => {
                    slot.insert(Framed { pid: info.pid, frame });
                    self.sync_location_hooks();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "frame not created");
                    return;
                }
            }
        }
        if self.canvas_ready().is_none() {
            return;
        }
        let (Some(canvas), Some(framed)) = (self.watch.canvas.as_mut(), self.watch.frames.get_mut(&h)) else {
            return;
        };
        if let Err(e) = framed
            .frame
            .show(canvas, info.rect, &info.look, h.0.cast_signed_isize())
        {
            tracing::debug!(error = %e, "frame not drawn");
        }
    }

    fn frame_for(&mut self, h: WindowHandle) -> Option<FrameInfo> {
        let frames = self.settings.frames;
        if self.watch.drag.as_ref().is_some_and(|d| d.window == h) || self.hiding.hidden.contains_key(&h) {
            return None;
        }
        let in_group = self.group_of_window(h).is_some_and(|g| g.open.len() > 1);
        let (ring, tag) = (frames.show_frames, frames.show_tags && !in_group);
        if !(ring || tag) {
            return None;
        }
        let style = self.style();
        let active = self.active.as_ref()?;
        active.slot_of(h)?;
        let hwnd = window::hwnd(h);
        let g = window::geometry(hwnd)?;
        if !g.framable() {
            return None;
        }
        let zone = active.zone_for(g.frame, style)?.clone();
        let look = FrameLook {
            color: zone.color,
            label: zone.name.clone(),
            dpi: zone.dpi,
            rounded: self.watch.rounded,
            ring,
            tag,
        };
        Some(FrameInfo {
            rect: g.frame,
            look,
            pid: window::pid(hwnd),
            zone: ZoneView {
                rect: zone.rect,
                color: zone.color,
                name: zone.name,
                dpi: zone.dpi,
            },
        })
    }

    /// Move hooks for exactly the processes whose windows wear a frame or are being dragged.
    fn sync_location_hooks(&mut self) {
        let mut wanted: HashSet<u32> = self.watch.frames.values().map(|f| f.pid).collect();
        if let Some(d) = &self.watch.drag {
            wanted.insert(d.pid);
        }
        self.watch.location.retain(|pid, _| wanted.contains(pid));
        for pid in wanted {
            if pid == 0 || self.watch.location.contains_key(&pid) {
                continue;
            }
            match events::hook(&[(EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE)], pid) {
                Ok(h) => {
                    self.watch.location.insert(pid, h);
                }
                Err(e) => tracing::debug!(error = %e, pid, "move hook not installed"),
            }
        }
    }

    fn drag_start(&mut self, h: WindowHandle) {
        self.watch.moving = Some(h);
        if let Some(f) = self.watch.frames.get_mut(&h) {
            f.frame.hide();
        }
        if let Some(g) = self.group_of_window(h) {
            self.hide_bar(&g.id);
        }
        if self.config.safe_mode || !self.settings.snap.enabled {
            return;
        }
        let Some(active) = &self.active else { return };
        if active.zones.is_empty() {
            return;
        }
        let hwnd = window::hwnd(h);
        let Some(facts) = self.scanner.facts(hwnd) else {
            return;
        };
        if !self.manageable(&facts) {
            return;
        }
        let zones = active
            .zones
            .iter()
            .map(|z| SnapZone {
                id: z.id.clone(),
                rect: z.rect,
                dpi: z.dpi,
            })
            .collect();
        // Grabbed by an edge or a corner (Windows' resize borders reach a few pixels into the
        // frame), it is a resize; anywhere else (a title bar, a tab strip, a keyboard move), a
        // move. A snapped or maximised window changes size as it is dragged, so the size alone
        // cannot tell.
        let margin = scale_for_dpi(4, facts.dpi.max(96));
        let inner = facts.frame_rect.inset(Insets::uniform(margin));
        let kind = if inner.contains(input::cursor()) {
            DragKind::Undecided
        } else {
            DragKind::Resizing
        };
        self.watch.drag = Some(Drag {
            window: h,
            pid: facts.pid,
            start: facts.frame_rect,
            kind,
            zones,
            target: None,
            tab_target: None,
        });
        self.sync_location_hooks();
        self.set_timer(timer::DRAG, DRAG_TICK_MS);
    }

    pub(crate) fn drag_tick(&mut self) {
        self.bar_tick();
        if self.watch.drag.is_some() {
            self.drag_update();
        } else if self.watch.bar_drag.is_none() {
            self.kill_timer(timer::DRAG);
        }
    }

    /// Follows the dragged window: decides whether it is a move or a resize, shows the zones
    /// on a move, and lights the zone it would drop into, or the tab strip it would join.
    fn drag_update(&mut self) {
        let Some(d) = &self.watch.drag else { return };
        let (h, start, kind) = (d.window, d.start, d.kind);
        let Some(g) = window::geometry(window::hwnd(h)) else {
            return;
        };
        if kind == DragKind::Undecided {
            if g.frame == start {
                return;
            }
            // Sized from the keyboard: the size changes, the window does not travel.
            if g.frame.origin() == start.origin() {
                if let Some(d) = &mut self.watch.drag {
                    d.kind = DragKind::Resizing;
                }
                return;
            }
            if let Some(d) = &mut self.watch.drag {
                d.kind = DragKind::Moving;
            }
            self.show_zone_preview(Stack::Below(h.0.cast_signed_isize()));
        }
        if self
            .watch
            .drag
            .as_ref()
            .is_some_and(|d| d.kind != DragKind::Moving)
        {
            return;
        }
        let snapping = self.settings.snap.active(shift_down());
        let cursor = input::cursor();
        // Over a window's tag or tab strip: the dragged window would become one of its tabs.
        let tab_target = if snapping {
            self.tab_target_at(cursor, h)
        } else {
            None
        };
        let previous_tab_target = self.watch.drag.as_ref().and_then(|d| d.tab_target);
        if let Some(d) = &mut self.watch.drag {
            d.tab_target = tab_target;
        }
        if previous_tab_target != tab_target {
            self.hide_drop_bar();
            if let Some((w, _)) = previous_tab_target {
                self.refresh_frame(w);
            }
            if let Some((w, before)) = tab_target {
                self.show_drop_strip(w, before);
            }
        }
        let target = if tab_target.is_some() || !snapping {
            None
        } else {
            self.watch.drag.as_ref().and_then(|d| {
                let id = snap::target(cursor, &d.zones, self.settings.snap)?;
                d.zones.iter().position(|z| &z.id == id)
            })
        };
        if let Some(d) = &mut self.watch.drag {
            d.target = target;
        }
        if snapping {
            if !self.watch.preview_visible {
                self.show_zone_preview(Stack::Below(h.0.cast_signed_isize()));
            }
            self.set_preview_target(target);
        } else {
            // Shift held: the window moves freely and the zones step aside.
            self.watch.preview.hide();
            self.watch.preview_visible = false;
        }
    }

    /// The window whose tag (or tab strip) is under `p`, other than `dragged` and its own group,
    /// with the tab the dragged window would go before (the end when `None`).
    fn tab_target_at(&self, p: Point, dragged: WindowHandle) -> Option<(WindowHandle, Option<WindowHandle>)> {
        let groups = self.groups_now();
        let own_group = groups
            .iter()
            .find(|g| g.open.iter().any(|(_, w)| *w == dragged))
            .map(|g| g.id.clone());
        // A group's strip: the window joins the group where the pointer is.
        for g in &groups {
            if Some(&g.id) == own_group.as_ref() {
                continue;
            }
            let Some(bar) = self.watch.bars.get(&g.id) else {
                continue;
            };
            let Some(r) = bar.rect() else { continue };
            if !r.outset(Insets::new(0, 4, 0, 2)).contains(p) {
                continue;
            }
            let Some((_, onto)) = g.visible() else { continue };
            return Some((*onto, bar.insert_before(p).map(WindowHandle)));
        }
        // A single window's tag: the window goes before it or after it.
        let style = self.style();
        if style.tag_band <= 0 {
            return None;
        }
        let active = self.active.as_ref()?;
        for (&w, framed) in &self.watch.frames {
            if w == dragged || !framed.frame.is_shown() {
                continue;
            }
            if groups
                .iter()
                .any(|g| g.open.len() > 1 && g.open.iter().any(|(_, x)| *x == w))
            {
                continue;
            }
            let Some(geo) = window::geometry(window::hwnd(w)) else {
                continue;
            };
            let Some(zone) = active.zone_for(geo.frame, style) else {
                continue;
            };
            let Some(band) = style.tag_area(zone.rect, zone.dpi) else {
                continue;
            };
            if !band.outset(Insets::new(0, 4, 0, 2)).contains(p) {
                continue;
            }
            return Some((w, (p.x < band.center().x).then_some(w)));
        }
        None
    }

    /// The "Drop to add" slot on the strip of `onto`, or on a strip shown over its tag when it
    /// is a single window.
    fn show_drop_strip(&mut self, onto: WindowHandle, before: Option<WindowHandle>) {
        if let Some(g) = self.group_of_window(onto).filter(|g| g.open.len() > 1) {
            self.refresh_strip(&g.id);
            return;
        }
        let Some(info) = self.frame_for(onto) else { return };
        let Some(band) = self.style().tag_area(info.zone.rect, info.zone.dpi) else {
            return;
        };
        let tabs = [TabView {
            id: onto.0,
            title: window::title(window::hwnd(onto)).trim().to_owned(),
            active: true,
        }];
        let look = BarLook {
            color: info.zone.color,
            label: info.zone.name.clone(),
            dpi: info.zone.dpi,
            dark: self.dark_theme(),
        };
        // Above the dragged window, which would otherwise cover it around the pointer.
        let above = self.watch.drag.as_ref().map_or(onto, |d| d.window);
        if self.canvas_ready().is_none() {
            return;
        }
        if self.watch.drop_bar.is_none() {
            self.watch.drop_bar = TabBar::new().ok();
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        if let Some(bar) = &mut self.watch.drop_bar
            && let Err(e) = bar.show(
                &mut canvas,
                band,
                &look,
                &tabs,
                Some(usize::from(before != Some(onto))),
                Stack::Above(above.0.cast_signed_isize()),
            )
        {
            tracing::debug!(error = %e, "drop strip not drawn");
        }
        self.watch.put_canvas(canvas);
    }

    fn hide_drop_bar(&mut self) {
        if let Some(b) = &mut self.watch.drop_bar {
            b.hide();
        }
    }

    /// Shows the open workspace's zones, stacked per `stack`.
    pub(crate) fn show_zone_preview(&mut self, stack: Stack) {
        let Some(active) = &self.active else { return };
        let views: Vec<ZoneView> = active
            .zones
            .iter()
            .map(|z| ZoneView {
                rect: z.rect,
                color: z.color,
                name: z.name.clone(),
                dpi: z.dpi,
            })
            .collect();
        if self.canvas_ready().is_none() {
            return;
        }
        if let Some(canvas) = self.watch.canvas.as_mut() {
            match self.watch.preview.show(canvas, &views, stack) {
                Ok(()) => self.watch.preview_visible = true,
                Err(e) => tracing::debug!(error = %e, "zones not shown"),
            }
        }
    }

    /// Lights the zone at `target` in the preview (or none).
    pub(crate) fn set_preview_target(&mut self, target: Option<usize>) {
        if self.canvas_ready().is_none() {
            return;
        }
        if let Some(canvas) = self.watch.canvas.as_mut()
            && let Err(e) = self.watch.preview.set_hot(canvas, target)
        {
            tracing::debug!(error = %e, "drop target not drawn");
        }
    }

    fn drag_end(&mut self, h: WindowHandle) {
        if self.watch.moving == Some(h) {
            self.watch.moving = None;
        }
        if !self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
            // Not a drag Mosaïk follows: the window may have moved in or out of a zone.
            self.after_user_move(h);
            return;
        }
        let Some(d) = self.watch.drag.take() else { return };
        self.kill_timer(timer::DRAG);
        self.watch.preview.hide();
        self.watch.preview_visible = false;
        self.hide_drop_bar();
        if d.kind == DragKind::Moving {
            if let Some((onto, before)) = d.tab_target {
                if let Some(g) = self.group_of_window(onto) {
                    self.hide_bar(&g.id);
                }
                self.join_group(onto, h, before);
                self.sync_location_hooks();
                return;
            }
            if let Some(i) = d.target
                && let Some(zone) = d.zones.get(i)
            {
                self.snap_into(h, zone.id.clone());
            }
        }
        self.sync_location_hooks();
        self.after_user_move(h);
    }

    /// The user moved or resized a window by hand: the open workspace remembers it. A tab
    /// group's visible tab brings its group along, and keeps room above it for its strip.
    fn after_user_move(&mut self, h: WindowHandle) {
        let group = self.group_of_window(h).map(|g| g.id);
        if let Some(g) = &group {
            self.make_room_for_strip(g, h);
        }
        self.remember_window(h);
        self.refresh_frame(h);
        if group.is_some() {
            self.sync_tabs();
        }
    }

    /// A group dropped at the top of a monitor, outside zones, has no room above it for its
    /// strip: it moves down by the strip's height (shrinking if it must, to stay on screen).
    fn make_room_for_strip(&mut self, group: &TabGroupId, h: WindowHandle) {
        let Some(geo) = window::geometry(window::hwnd(h)) else {
            return;
        };
        if !geo.framable() || self.strip_spot(group, h) != crate::tabs::Spot::Nowhere {
            return;
        }
        let Some(topology) = &self.topology else { return };
        let work = topology.monitor_for_rect(geo.frame).work_area;
        let band = scale_for_dpi(self.style().for_tabs().tag_band, geo.dpi.max(96));
        let top = work.top + band;
        if geo.frame.top >= top {
            return;
        }
        let bottom = (geo.frame.bottom + (top - geo.frame.top)).min(work.bottom);
        let to = Rect::new(geo.frame.left, top, geo.frame.right, bottom);
        if to.height() < band * 3 {
            return;
        }
        for o in mover::move_all(&[(h, To::Frame(to))], mover::WAIT_ONE) {
            if let Err(e) = o.result {
                tracing::debug!(error = %e, "tab group not moved below its strip");
            }
        }
    }

    /// Pulls a dropped window into a zone of the open workspace; a window that was not part of
    /// the workspace joins it. The visible tab of a group brings its group along.
    fn snap_into(&mut self, h: WindowHandle, zone: mosaik_core::zone::ZoneId) {
        let Some(live) = self
            .active
            .as_ref()
            .and_then(|a| a.zones.iter().find(|z| z.id == zone).cloned())
        else {
            return;
        };
        let group = self.group_of_window(h).map(|g| g.id);
        // A tab group sits below a band with room for its strip, tags or not.
        let style = match group {
            Some(_) => self.style().for_tabs(),
            None => self.style(),
        };
        let target = style.window_area(live.rect, live.dpi);
        let moved = mover::move_all(&[(h, To::Frame(target))], mover::WAIT_ONE);
        if let Some(Err(e)) = moved.into_iter().next().map(|o| o.result) {
            tracing::warn!(error = %e, "window not pulled into its zone");
            self.tell_once(
                "snap",
                "A window did not move into its zone",
                format!("{e}. It stays where it was dropped."),
            );
            return;
        }
        let place = Place::Zone { zone };
        match group {
            Some(id) => {
                let p = place.clone();
                self.edit_layout(|layout| layout.move_group(&id, p));
            }
            None => {
                let Some(slot) = self.slot_for_window(h, place.clone()) else {
                    return;
                };
                self.edit_layout(|layout| {
                    if let Some(s) = layout.slots.iter_mut().find(|s| s.id == slot) {
                        s.place = place;
                    }
                });
            }
        }
        self.refresh_frame(h);
        self.sync_location_hooks();
    }
}
