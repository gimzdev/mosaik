//! Watching windows: the frames, tags and tab strips windows wear in their zones, the zones
//! shown while a window is dragged, the pull into a zone when it is dropped, and dropping a
//! window on another's tag to make them tabs.
//!
//! Most events come from global hooks. Moves (`EVENT_OBJECT_LOCATIONCHANGE`) are hooked per
//! process, only for the apps whose windows wear a frame or are being dragged, or whose window
//! has just taken the mouse: a global hook would also report every movement of the pointer.
//!
//! A drag is followed through Windows' move loop (`EVENT_SYSTEM_MOVESIZESTART`), which nearly
//! every app uses. An app that drags its window itself (it takes the mouse and moves the window
//! as the pointer moves) is followed too: while one of its windows holds the mouse, a move of
//! that window at the same size, with the button down, is a drag, which ends when the app lets
//! the mouse go.

use std::collections::{HashMap, HashSet};

use mosaik_core::classify::{Refuse, Verdict, classify};
use mosaik_core::geometry::{Insets, Point, Rect, scale_for_dpi};
use mosaik_core::layout::{Place, TabGroupId};
use mosaik_core::snap::{self, DragKind, LiveZone as SnapZone};
use mosaik_core::window::{WindowHandle, covers_monitor};
use mosaik_core::zone::band_above;
use mosaik_overlay::canvas::Canvas;
use mosaik_overlay::frame::{Frame, FrameLook, Preview, TagPlace, ZoneView};
use mosaik_overlay::layered::Stack;
use mosaik_overlay::tabbar::{self, BarLook, BarPlace, TabBar, TabView};
use mosaik_win::events::{self, Hooks, WinEvent};
use mosaik_win::{arrange, input, window};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_CAPTUREEND,
    EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, OBJID_WINDOW,
};

use crate::mover::{self, To};
use crate::tabs::{AsIsize, BarDrag, Spot};
use crate::thread::{Engine, WM_EVENTS, timer};
use crate::titles::TitleSpot;

/// Events hooked for every window.
const GLOBAL: [(u32, u32); 6] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    // Taking and letting go of the mouse (a press), then Windows' move loop: one range.
    (EVENT_SYSTEM_CAPTURESTART, EVENT_SYSTEM_MOVESIZEEND),
    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
    (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
    (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];

/// How often the drag preview follows the pointer and the Shift key while the window itself
/// is not moving (held at a screen edge, Shift pressed without moving), and Esc is checked
/// during a drag on a tab strip.
pub(crate) const DRAG_TICK_MS: u32 = 30;

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
    /// The app moves the window itself (no move loop of Windows'): the drag ends when the app
    /// lets the mouse go, or a moment after the button is up.
    pub self_moved: bool,
    /// When the button was first seen up during a drag the app makes itself.
    pub released: Option<std::time::Instant>,
    /// The zones a resize of the window would move, when it fills a zone (`resize.rs`).
    pub zone_resize: Option<crate::resize::ZoneResize>,
}

/// A window that took the mouse: if it moves at the same size while the button is down, without
/// Windows' move loop, its app is dragging it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pressed {
    window: WindowHandle,
    pid: u32,
    start: Rect,
}

/// A press on the band Mosaïk draws above a window (or on its group's strip, off the chips):
/// the band works as the window's title bar, so the window follows the pointer, moved by Mosaïk.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BandPress {
    pub window: WindowHandle,
    /// Where the button went down, and where the window's rectangle (invisible borders
    /// included) started then.
    pub start: Point,
    pub origin: Point,
    /// Where its visible frame was then.
    pub frame: Rect,
    /// Past a few pixels, the window follows the pointer.
    pub moving: bool,
}

/// How long a drag the app makes itself waits, once the button is up, for the app to let the
/// mouse go before Mosaïk ends it anyway.
const SELF_MOVE_RELEASE_MS: u128 = 400;

struct Framed {
    pid: u32,
    frame: Frame,
    /// Where dropping a window on this one makes them tabs: the stretch of its title bar that
    /// holds its tag, or the band above it.
    drop_area: Option<(Rect, BarPlace)>,
}

/// What a framed window wears.
struct FrameInfo {
    rect: Rect,
    look: FrameLook,
    pid: u32,
    zone: ZoneView,
    drop_area: Option<(Rect, BarPlace)>,
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
    /// A press on a window's band, and the thread moving its window with the pointer.
    pub(crate) band_press: Option<BandPress>,
    pub(crate) follower: Option<mover::Follower>,
    /// Esc, held by Mosaïk while a press on a strip or a band lasts, so it cancels the drag and
    /// does not reach the app in front, as with Windows' own move.
    pub(crate) drag_esc: Option<mosaik_win::hotkey::Registration>,
    /// The strip shown over a single window's tag while another window is dragged onto it.
    drop_bar: Option<TabBar>,
    pub(crate) drag: Option<Drag>,
    /// The window holding the mouse, watched for a drag its app makes itself.
    pressed: Option<Pressed>,
    /// A window Mosaïk must not move, being dragged while zones are on: why is told when the
    /// drag ends, not in the middle of it.
    refused: Option<(WindowHandle, Refuse)>,
    /// The window the user is moving or resizing right now (any window, zones or not), so
    /// nothing moves it from under the pointer.
    pub(crate) moving: Option<WindowHandle>,
    /// The drag preview is on screen (it steps aside while Shift is held).
    pub(crate) preview_visible: bool,
    /// Windows 11 rounds window corners; frames and bands follow.
    pub(crate) rounded: bool,
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

    /// Where `h`'s zone tag is drawn, when it shows.
    pub(crate) fn tag_rect(&self, h: WindowHandle) -> Option<Rect> {
        self.frames.get(&h).and_then(|f| f.frame.tag_rect())
    }

    /// Where dropping a window on `h` makes them tabs, while its tag shows.
    pub(crate) fn drop_area(&self, h: WindowHandle) -> Option<Rect> {
        self.frames
            .get(&h)
            .filter(|f| f.frame.tag_rect().is_some())
            .and_then(|f| f.drop_area.map(|(r, _)| r))
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
        mosaik_overlay::edge::wake_on_input(self.window, WM_EVENTS);
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
        self.watch.pressed = None;
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
        self.take_titles();
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
                EVENT_SYSTEM_CAPTURESTART => self.capture_start(h),
                EVENT_SYSTEM_CAPTUREEND => self.capture_end(h),
                EVENT_SYSTEM_MOVESIZESTART => self.drag_start(h),
                EVENT_SYSTEM_MOVESIZEEND => self.drag_end(h),
                EVENT_SYSTEM_FOREGROUND => {
                    self.quick_follow_foreground(h);
                    sync |= self.came_forward(h);
                }
                EVENT_SYSTEM_MINIMIZESTART | EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED => {
                    if let Some(f) = self.watch.frames.get_mut(&h) {
                        f.frame.hide();
                    }
                    self.quick_window_changed(h);
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
                // shows its window's title; a new title may come with a new title bar.
                EVENT_OBJECT_NAMECHANGE => {
                    self.adopt(h);
                    if self.watch.frames.contains_key(&h) {
                        self.refresh_ring(h);
                    }
                    if let Some(g) = self.group_of_window(h) {
                        self.refresh_strip(&g.id);
                    }
                }
                EVENT_OBJECT_DESTROY => {
                    self.quick_window_changed(h);
                    sync |= self.forget(h);
                }
                _ => {}
            }
        }
        for h in moved {
            self.quick_window_changed(h);
            if self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
                self.drag_update();
            } else if self.watch.pressed.is_some_and(|p| p.window == h) && self.self_move_begins(h) {
                // Followed as a drag from here on.
            } else if self.watch.frames.contains_key(&h) || self.group_of_window(h).is_some() {
                self.refresh_frame(h);
            }
        }
        let inputs = tabbar::take_inputs();
        if !inputs.is_empty() {
            self.on_bar_inputs(inputs);
        }
        let quick = mosaik_overlay::edge::take_inputs();
        if !quick.is_empty() {
            self.on_quick_inputs(quick);
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
        let group = self.group_of_window(h);
        if let Some(g) = &group
            && let Some(bar) = self.watch.bars.get(&g.id)
            && bar.is_shown()
        {
            mosaik_overlay::layered::restack_windows(&[bar.hwnd()], Stack::Above(h.0.cast_signed_isize()));
        }
        // Its app may have changed its title bar while it was behind (a browser opened a tab):
        // read again, and the tag or strip moves if it must.
        if (self.watch.frames.contains_key(&h) || group.is_some())
            && let Some(g) = window::geometry(window::hwnd(h))
        {
            self.titles.stale(h);
            let _ = self.title_bar(h, g.frame);
        }
        false
    }

    /// A window closed: its frame goes; its slot stays (the workspace starts it again next
    /// time). Returns whether tabs must be synced (it was a tab: the next one shows).
    fn forget(&mut self, h: WindowHandle) -> bool {
        let tab = self.hiding.hidden.contains_key(&h) || self.group_of_window(h).is_some();
        self.titles.forget(h);
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
        if let Some(d) = self.watch.drag.take_if(|d| d.window == h) {
            self.kill_timer(timer::DRAG);
            self.watch.preview.hide();
            self.watch.preview_visible = false;
            self.hide_drop_bar();
            if let Some((onto, _)) = d.tab_target {
                self.refresh_frame(onto);
            }
            self.sync_location_hooks();
        }
        if self.watch.refused.is_some_and(|(w, _)| w == h) {
            self.watch.refused = None;
        }
        if self.watch.pressed.is_some_and(|p| p.window == h) {
            self.watch.pressed = None;
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

    /// Shows, moves or hides the frames and strips of every window of the open workspace, each
    /// window first in or out of a band as its title bar's room says.
    pub(crate) fn refresh_frames(&mut self) {
        let windows: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let keep: HashSet<WindowHandle> = windows.iter().copied().collect();
        self.watch.frames.retain(|h, _| keep.contains(h));
        self.titles.keep_only(&keep);
        self.update_bands(&windows);
        for h in windows {
            self.refresh_ring(h);
        }
        self.sync_location_hooks();
        self.sync_tabs();
    }

    /// The frame of one window, and its group's strip when it is the tab that shows; the window
    /// first in or out of a band if its title bar's room, or what it shows, says so.
    pub(crate) fn refresh_frame(&mut self, h: WindowHandle) {
        if self.style().title_bars && self.active.as_ref().is_some_and(|a| a.slot_of(h).is_some()) {
            self.update_bands(&[h]);
        }
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
                    slot.insert(Framed {
                        pid: info.pid,
                        frame,
                        drop_area: None,
                    });
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
        framed.drop_area = info.drop_area;
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
        let zone = self.zone_of(h, g.frame, g.resizable, style)?.clone();
        // The tag goes in the window's title bar, or above it, in the band at the top of its
        // zone when tags go above windows (or while it still sits below one); not at all for the
        // moment its title bar is being read.
        let (tag, tag_at, drop_area) = if tag {
            match self.tag_spot(h, g.frame, &zone.name, zone.dpi) {
                TitleSpot::In(r) => (true, TagPlace::TitleBar(r), Some((r, BarPlace::TitleBar))),
                // A window over its whole monitor is never moved below a band (`wants_band`): a
                // tag above it would lie on its own title bar, so it wears its ring alone.
                TitleSpot::Above
                    if self
                        .topology
                        .as_ref()
                        .is_some_and(|t| covers_monitor(g.frame, t.monitor_for_rect(g.frame).bounds)) =>
                {
                    (false, TagPlace::Above, None)
                }
                // With tags in title bars, a window alone whose title bar has no room even for
                // its zone's initial wears its ring alone, at the top of its zone like its
                // neighbours: it is never moved below a band for its tag (`wants_band`).
                TitleSpot::Above if self.style().title_bars && !self.titles.banded.contains(&h) => {
                    (false, TagPlace::Above, None)
                }
                TitleSpot::Above => {
                    // Right above the window (one that cannot be resized sits lower in its zone).
                    let band = self
                        .style_of(h)
                        .tag_area(zone.rect, zone.dpi)
                        .map(|b| band_above(g.frame, b));
                    (
                        true,
                        band.map_or(TagPlace::Above, TagPlace::Band),
                        band.map(|b| (b, BarPlace::Band)),
                    )
                }
                TitleSpot::NotYet | TitleSpot::NoAnswer => (false, TagPlace::Above, None),
            }
        } else {
            (false, TagPlace::Above, None)
        };
        if !(ring || tag) {
            return None;
        }
        // The band above the window that is its own: its tag's, or, for the tab that shows, its
        // group's strip's when that goes above it. The strip fills its band itself.
        let band = match (&tag_at, self.group_of_window(h).filter(|g| g.open.len() > 1)) {
            (_, Some(g)) if g.visible().is_some_and(|(_, w)| *w == h) => match self.strip_spot(&g.id, h) {
                Spot::At {
                    band,
                    place: BarPlace::Band,
                    ..
                } => Some(band),
                _ => None,
            },
            (_, Some(_)) => None,
            (TagPlace::Band(b), None) if tag => Some(*b),
            _ => None,
        };
        let look = FrameLook {
            color: zone.color,
            label: zone.name.clone(),
            dpi: zone.dpi,
            rounded: self.watch.rounded,
            ring,
            tag,
            tag_at,
            band,
            fill_band: !in_group,
            dark: self.dark_theme(),
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
            drop_area,
        })
    }

    /// Move hooks for exactly the processes whose windows wear a frame or are being dragged.
    fn sync_location_hooks(&mut self) {
        let mut wanted: HashSet<u32> = self.watch.frames.values().map(|f| f.pid).collect();
        if let Some(d) = &self.watch.drag {
            wanted.insert(d.pid);
        }
        if let Some(p) = &self.watch.pressed {
            wanted.insert(p.pid);
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
        // Windows' own move loop: no need to watch for one the app makes itself.
        self.watch.pressed = None;
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
            // Dragged while zones are on, a window Mosaïk must not move would get no zones and
            // no tag without a word: why is told when the drag ends.
            if self.allowed(&facts)
                && let Verdict::Refuse(why) = classify(&facts, &self.settings.rules)
            {
                self.watch.refused = Some((h, why));
            }
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
        // A resize shows Windows' resize arrows from the start (mouse or keyboard); anything else
        // is decided by what the window does (DragKind). Without a pointer on screen (touch), a
        // grab away from the edges (Windows' resize borders reach a few pixels into the frame)
        // is taken for a move.
        let margin = scale_for_dpi(4, facts.dpi.max(96));
        let inner = facts.frame_rect.inset(Insets::uniform(margin));
        let pointer = input::pointer();
        let kind = DragKind::at_start(pointer, inner.contains(input::cursor()));
        tracing::debug!(?pointer, ?kind, frame = ?facts.frame_rect, "drag started");
        let zone_resize = self.zone_resize_for(h, facts.frame_rect, facts.resizable);
        self.watch.drag = Some(Drag {
            window: h,
            pid: facts.pid,
            start: facts.frame_rect,
            kind,
            zones,
            target: None,
            tab_target: None,
            self_moved: false,
            released: None,
            zone_resize,
        });
        self.sync_location_hooks();
        self.set_timer(timer::DRAG, DRAG_TICK_MS);
    }

    /// A window took the mouse (a press in it). Watched while zones are on, in case its app drags
    /// it itself rather than through Windows' move loop.
    fn capture_start(&mut self, h: WindowHandle) {
        if self.watch.drag.is_some()
            || self.watch.moving.is_some()
            || self.config.safe_mode
            || !self.settings.snap.enabled
            || !self.active.as_ref().is_some_and(|a| !a.zones.is_empty())
        {
            return;
        }
        let root = window::root(window::hwnd(h));
        let Some(facts) = self.scanner.facts(root) else {
            return;
        };
        if !self.manageable(&facts) {
            return;
        }
        self.watch.pressed = Some(Pressed {
            window: facts.handle,
            pid: facts.pid,
            start: facts.frame_rect,
        });
        self.sync_location_hooks();
    }

    /// A window let the mouse go: a drag its app made itself ends here (unless the button is
    /// still down: the app may take the mouse again).
    fn capture_end(&mut self, h: WindowHandle) {
        let root = window::handle(window::root(window::hwnd(h)));
        let ends = self
            .watch
            .drag
            .as_ref()
            .is_some_and(|d| d.self_moved && d.window == root);
        if ends && !input::primary_button_down() {
            self.drag_end(root);
        }
        if self.watch.pressed.is_some_and(|p| p.window == root) {
            self.watch.pressed = None;
            self.sync_location_hooks();
        }
    }

    /// The window holding the mouse moved: at the same size, with the button down and no move
    /// loop of Windows', its app is dragging it, and Mosaïk follows it as any drag. Returns
    /// whether it did.
    fn self_move_begins(&mut self, h: WindowHandle) -> bool {
        let Some(p) = self.watch.pressed else { return false };
        let Some(g) = window::geometry(window::hwnd(h)) else {
            return false;
        };
        if g.frame == p.start || g.frame.size() != p.start.size() || !input::primary_button_down() {
            return false;
        }
        self.drag_start(h);
        let Some(d) = self.watch.drag.as_mut().filter(|d| d.window == h) else {
            return false;
        };
        d.start = p.start;
        d.kind = DragKind::Moving;
        d.self_moved = true;
        tracing::debug!(frame = ?g.frame, "the app drags its window itself");
        self.show_zone_preview(Stack::Below(h.0.cast_signed_isize()));
        self.drag_update();
        true
    }

    pub(crate) fn drag_tick(&mut self) {
        self.bar_tick();
        // A drag the app makes itself ends when the app lets the mouse go; if it keeps it a
        // moment after the button is up, Mosaïk ends the drag anyway.
        let up = input::primary_button_down();
        if let Some(d) = self.watch.drag.as_mut().filter(|d| d.self_moved) {
            if up {
                d.released = None;
            } else {
                let since = *d.released.get_or_insert_with(std::time::Instant::now);
                if since.elapsed().as_millis() >= SELF_MOVE_RELEASE_MS {
                    let w = d.window;
                    self.drag_end(w);
                    return;
                }
            }
        }
        if self.watch.drag.is_some() {
            self.drag_update();
        } else if self.watch.bar_drag.is_none() && self.watch.band_press.is_none() {
            self.kill_timer(timer::DRAG);
        }
        self.hold_esc();
    }

    /// Holds Esc while a press on a strip or a band lasts, and lets it go after.
    pub(crate) fn hold_esc(&mut self) {
        let pressed = self.watch.bar_drag.is_some() || self.watch.band_press.is_some();
        if !pressed {
            self.watch.drag_esc = None;
            return;
        }
        if self.watch.drag_esc.is_none() {
            let esc = mosaik_core::hotkey::Key::from_vk(0x1B)
                .map(|k| mosaik_core::hotkey::Hotkey::new(mosaik_core::hotkey::Modifiers::NONE, k));
            // Taken by another app: Esc is still read from the keyboard (`bar_tick`).
            self.watch.drag_esc = esc.and_then(|h| {
                mosaik_win::hotkey::register(Some(self.window), crate::hotkeys::DRAG_ESC, h).ok()
            });
        }
    }

    /// The window whose band is the overlay window `bar`, if it is a band.
    pub(crate) fn band_owner(&self, bar: isize) -> Option<WindowHandle> {
        self.watch
            .frames
            .iter()
            .find(|(_, f)| f.frame.band_is(bar))
            .map(|(h, _)| *h)
    }

    /// The button went down on the band above `h`: the window comes to the front, as from its
    /// title bar, and follows the pointer once it moves.
    pub(crate) fn band_down(&mut self, h: WindowHandle, at: Point) {
        let hwnd = window::hwnd(h);
        let (Some(rect), Some(frame)) = (window::window_rect(hwnd), window::frame_rect(hwnd)) else {
            return;
        };
        arrange::focus(hwnd);
        self.watch.band_press = Some(BandPress {
            window: h,
            start: at,
            origin: Point::new(rect.left, rect.top),
            frame,
            moving: false,
        });
        self.set_timer(timer::DRAG, DRAG_TICK_MS);
    }

    /// Where the window's rectangle goes with the pointer at `at`: moved as the pointer moved,
    /// but never off every screen. It is held above itself, so the pointer alone could take all
    /// of it below the bottom of a screen, where nothing would be left to grab.
    fn band_place(&self, p: &BandPress, at: Point) -> Point {
        let frame = p.frame.offset(at.x - p.start.x, at.y - p.start.y);
        let kept = self
            .topology
            .as_ref()
            .map_or(frame, |t| mosaik_core::place::reachable(frame, t));
        Point::new(
            p.origin.x + kept.left - p.frame.left,
            p.origin.y + kept.top - p.frame.top,
        )
    }

    /// The pointer moved with the button down on a band: past a few pixels, the window is
    /// dragged as by its title bar (zones show, tags and strips take it as for any drag), moved
    /// by a thread of its own so a slow app never holds up Mosaïk.
    pub(crate) fn band_move(&mut self, at: Point) {
        let Some(mut p) = self.watch.band_press else {
            return;
        };
        if !p.moving {
            let dpi = self.topology.as_ref().map_or(96, |t| t.nearest_monitor(at).dpi);
            if (at.x - p.start.x).abs().max((at.y - p.start.y).abs()) <= scale_for_dpi(4, dpi) {
                return;
            }
            p.moving = true;
            self.watch.band_press = Some(p);
            self.watch.follower = mover::Follower::start(p.window);
            self.drag_start(p.window);
            let followed = self
                .watch
                .drag
                .as_mut()
                .filter(|d| d.window == p.window)
                .map(|d| {
                    d.start = p.frame;
                    d.kind = DragKind::Moving;
                    // Ended by the button going up, as a drag an app makes itself.
                    d.self_moved = true;
                });
            if followed.is_some() {
                self.show_zone_preview(Stack::Below(p.window.0.cast_signed_isize()));
            }
        }
        let to = self.band_place(&p, at);
        if let Some(f) = &self.watch.follower {
            f.to(to);
        }
        self.drag_update();
    }

    /// The button went up on a band: a dragged window drops where it is, into a zone or onto a
    /// tag as from any drag, once its app has taken the last place.
    pub(crate) fn band_up(&mut self, at: Point) {
        let Some(p) = self.watch.band_press.take() else {
            return;
        };
        if p.moving {
            let to = self.band_place(&p, at);
            if let Some(f) = self.watch.follower.take() {
                f.to(to);
                f.finish(mover::WAIT_ONE);
            }
            self.drag_end(p.window);
        }
        if self.watch.drag.is_none() && self.watch.bar_drag.is_none() {
            self.kill_timer(timer::DRAG);
        }
    }

    /// A press on a band ended without a drop (Esc, or the mouse taken away): a dragged window
    /// goes back where it was.
    pub(crate) fn band_cancel(&mut self) {
        let Some(p) = self.watch.band_press.take() else {
            return;
        };
        if !p.moving {
            return;
        }
        if let Some(f) = self.watch.follower.take() {
            f.to(p.origin);
            f.finish(mover::WAIT_ONE);
        }
        // The strip it was over loses its "Drop to add" slot.
        let over = self.watch.drag.take().and_then(|d| d.tab_target);
        self.watch.preview.hide();
        self.watch.preview_visible = false;
        self.hide_drop_bar();
        if let Some((onto, _)) = over {
            self.refresh_frame(onto);
        }
        if self.watch.moving == Some(p.window) {
            self.watch.moving = None;
        }
        if self.watch.bar_drag.is_none() {
            self.kill_timer(timer::DRAG);
        }
        self.sync_location_hooks();
        self.refresh_frame(p.window);
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
            let pointer = input::pointer();
            let decided = DragKind::once_changed(start, g.frame, pointer);
            if decided != DragKind::Undecided {
                tracing::debug!(?pointer, ?decided, ?start, now = ?g.frame, "drag decided");
            }
            if let Some(d) = &mut self.watch.drag {
                d.kind = decided;
            }
            match decided {
                DragKind::Moving => self.show_zone_preview(Stack::Below(h.0.cast_signed_isize())),
                DragKind::Resizing => {
                    self.resize_update(h);
                    return;
                }
                DragKind::Undecided => return,
            }
        }
        match self.watch.drag.as_ref().map(|d| d.kind) {
            Some(DragKind::Moving) => {}
            Some(DragKind::Resizing) => {
                // A window filling a zone takes its zone's lines along (`resize.rs`).
                self.resize_update(h);
                return;
            }
            _ => return,
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
        // A single window's tag (the stretch of its title bar holding it, or the band above it):
        // the window goes before it or after it.
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
            let Some((area, _)) = framed.drop_area else {
                continue;
            };
            if !area.outset(Insets::new(0, 4, 0, 2)).contains(p) {
                continue;
            }
            return Some((w, (p.x < area.center().x).then_some(w)));
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
        let Some((band, place)) = info.drop_area else {
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
            rounded: self.watch.rounded,
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
                place,
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

    /// Shows `views` (the zones of a monitor as a resize would leave them), stacked per `stack`,
    /// with the one at `hot` lit. Returns whether they show.
    pub(crate) fn show_zone_views(&mut self, views: &[ZoneView], hot: Option<usize>, stack: Stack) -> bool {
        if self.canvas_ready().is_none() {
            return false;
        }
        let Some(canvas) = self.watch.canvas.as_mut() else {
            return false;
        };
        match self.watch.preview.show(canvas, views, stack) {
            Ok(()) => {
                if let Err(e) = self.watch.preview.set_hot(canvas, hot) {
                    tracing::debug!(error = %e, "zone not lit");
                }
                self.watch.preview_visible = true;
                true
            }
            Err(e) => {
                tracing::debug!(error = %e, "zones not shown");
                false
            }
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
        // A drag from a band ended another way (its button seen up by the drag timer).
        if self.watch.band_press.is_some_and(|p| p.window == h) {
            self.watch.band_press = None;
            self.watch.follower = None;
        }
        if let Some((_, why)) = self.watch.refused.take_if(|(w, _)| *w == h) {
            self.tell_once("refused-drag", "Mosaïk cannot move this window", why.explain());
        }
        if !self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
            // Not a drag Mosaïk follows: the window may have moved in or out of a zone.
            self.after_user_move(h);
            return;
        }
        let Some(d) = self.watch.drag.take() else { return };
        tracing::debug!(
            kind = ?d.kind,
            onto_tab = d.tab_target.is_some(),
            zone = ?d.target.and_then(|i| d.zones.get(i)).map(|z| &z.id),
            pointer = ?input::cursor(),
            "drag ended"
        );
        self.kill_timer(timer::DRAG);
        self.watch.preview.hide();
        self.watch.preview_visible = false;
        self.hide_drop_bar();
        // A window that is gone at the end of its drag (a browser tab dragged back into its
        // window: the browser closes the window it had made for it) has nowhere to go.
        if !window::exists(window::hwnd(h)) {
            // The strip it was over loses its "Drop to add" slot.
            if let Some((onto, _)) = d.tab_target {
                self.refresh_frame(onto);
            }
            self.sync_location_hooks();
            return;
        }
        if d.kind == DragKind::Moving {
            let pointer = input::cursor();
            if let Some((onto, before)) = d.tab_target {
                if let Some(g) = self.group_of_window(onto) {
                    self.hide_bar(&g.id);
                }
                self.join_group(onto, h, before);
                let joined = self
                    .group_of_window(h)
                    .is_some_and(|g| g.open.iter().any(|(_, w)| *w == onto));
                if joined {
                    self.note_windows_snapping(pointer);
                }
                self.sync_location_hooks();
                return;
            }
            if let Some(i) = d.target
                && let Some(zone) = d.zones.get(i)
                && self.snap_into(h, zone.id.clone())
            {
                self.note_windows_snapping(pointer);
            }
        }
        if d.kind == DragKind::Resizing
            && let Some(zr) = d.zone_resize
            && self.resize_end(h, d.start, zr)
        {
            // The window stays in its zone, which followed it; its group's other tabs follow
            // when shown.
            self.sync_location_hooks();
            self.refresh_frame(h);
            if self.group_of_window(h).is_some() {
                self.sync_tabs();
            }
            return;
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
        let band = scale_for_dpi(self.style_of(h).for_tabs().tag_band, geo.dpi.max(96));
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
    /// Returns whether the window went into the zone.
    fn snap_into(&mut self, h: WindowHandle, zone: mosaik_core::zone::ZoneId) -> bool {
        let Some(live) = self
            .active
            .as_ref()
            .and_then(|a| a.zones.iter().find(|z| z.id == zone).cloned())
        else {
            return false;
        };
        let group = self.group_of_window(h).map(|g| g.id);
        // A tab group sits below a band with room for its strip, tags or not.
        let style = match group {
            Some(_) => self.style().for_tabs(),
            None => self.style(),
        };
        let target = self
            .banded_area(h, live.rect, live.dpi)
            .unwrap_or_else(|| style.window_area(live.rect, live.dpi));
        let moved = mover::move_all(&[(h, To::Frame(target))], mover::WAIT_ONE);
        if let Some(Err(e)) = moved.into_iter().next().map(|o| o.result) {
            if !window::exists(window::hwnd(h)) {
                // Closed while it was being moved: nothing to tell.
                return false;
            }
            tracing::warn!(error = %e, "window not pulled into its zone");
            self.tell_once(
                "snap",
                "A window did not move into its zone",
                format!("{e}. It stays where it was dropped."),
            );
            return false;
        }
        let place = Place::Zone { zone };
        match group {
            Some(id) => {
                let p = place.clone();
                self.edit_layout(|layout| layout.move_group(&id, p));
            }
            None => {
                let Some(slot) = self.slot_for_window(h, place.clone()) else {
                    return true;
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
        true
    }
}
