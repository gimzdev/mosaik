//! Watching windows: the frames and tags windows wear in their zones, the zones shown while a
//! window is dragged, and the pull into a zone when it is dropped.
//!
//! Most events come from global hooks. Moves (`EVENT_OBJECT_LOCATIONCHANGE`) are hooked per
//! process, only for the apps whose windows wear a frame or are being dragged: a global hook
//! would also report every movement of the mouse pointer.

use std::collections::{HashMap, HashSet};

use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Place, Slot, SlotId};
use mosaik_core::snap::{self, LiveZone as SnapZone};
use mosaik_core::window::WindowHandle;
use mosaik_core::workspace::app_match;
use mosaik_overlay::canvas::Canvas;
use mosaik_overlay::frame::{Frame, FrameLook, Preview, ZoneView};
use mosaik_overlay::layered::Stack;
use mosaik_win::events::{self, Hooks, WinEvent};
use mosaik_win::{apps, input, place, window};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND,
    EVENT_SYSTEM_MOVESIZESTART, OBJID_WINDOW,
};

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
/// is not moving (held at a screen edge, Shift pressed without moving).
const DRAG_TICK_MS: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragKind {
    /// Pressed, not moved yet.
    Undecided,
    Moving,
    /// The size changed: a resize, which never snaps.
    Resizing,
}

pub(crate) struct Drag {
    pub window: WindowHandle,
    pub pid: u32,
    pub start: Rect,
    /// Dragging a maximised window restores it first: its size changes, yet it is a move.
    pub was_maximized: bool,
    pub kind: DragKind,
    /// The zones shown, in the preview's order.
    pub zones: Vec<SnapZone>,
    pub target: Option<usize>,
}

struct Framed {
    pid: u32,
    frame: Frame,
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
    preview: Preview,
    pub(crate) drag: Option<Drag>,
    /// The drag preview is on screen (it steps aside while Shift is held).
    preview_visible: bool,
    rounded: bool,
}

impl Watch {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

fn top_level_window(e: &WinEvent) -> bool {
    e.object == OBJID_WINDOW.0 && e.child == i32::try_from(CHILDID_SELF).unwrap_or(0) && e.window.0 != 0
}

fn shift_down() -> bool {
    // SAFETY: reads the key state; no preconditions.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_SHIFT.0)) };
    state.cast_unsigned() & 0x8000 != 0
}

impl Engine {
    pub(crate) fn start_watching(&mut self) {
        events::wake_on_event(self.window, WM_EVENTS);
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
        self.watch.preview.hide();
        self.watch.frames.clear();
        self.watch.location.clear();
        self.watch.hooks = None;
    }

    fn canvas(&mut self) -> Option<&mut Canvas> {
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
        self.watch.canvas.as_mut()
    }

    /// Handles every window event recorded since the last call. Moves are merged per window:
    /// only where a window ended up matters.
    pub(crate) fn on_events(&mut self) {
        let batch = events::drain();
        let mut moved: Vec<WindowHandle> = Vec::new();
        for e in batch {
            if !top_level_window(&e) {
                continue;
            }
            let h = e.window;
            match e.event {
                EVENT_OBJECT_LOCATIONCHANGE => {
                    if !moved.contains(&h) {
                        moved.push(h);
                    }
                }
                EVENT_SYSTEM_MOVESIZESTART => self.drag_start(h),
                EVENT_SYSTEM_MOVESIZEEND => self.drag_end(h),
                EVENT_SYSTEM_FOREGROUND => {
                    if let Some(f) = self.watch.frames.get(&h) {
                        f.frame.restack(h.0.cast_signed_isize());
                    }
                }
                EVENT_SYSTEM_MINIMIZESTART | EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED => {
                    if let Some(f) = self.watch.frames.get_mut(&h) {
                        f.frame.hide();
                    }
                }
                EVENT_SYSTEM_MINIMIZEEND | EVENT_OBJECT_UNCLOAKED => self.refresh_frame(h),
                EVENT_OBJECT_SHOW => {
                    self.adopt(h);
                    if self.watch.frames.contains_key(&h) {
                        self.refresh_frame(h);
                    }
                }
                // Some apps show their window first and name it a moment later.
                EVENT_OBJECT_NAMECHANGE => self.adopt(h),
                EVENT_OBJECT_DESTROY => self.forget(h),
                _ => {}
            }
        }
        for h in moved {
            if self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
                self.drag_update();
            } else if self.watch.frames.contains_key(&h) {
                self.refresh_frame(h);
            }
        }
    }

    /// A window closed: its frame goes; its slot stays (the workspace starts it again next time).
    fn forget(&mut self, h: WindowHandle) {
        if self.watch.frames.remove(&h).is_some() {
            self.sync_location_hooks();
        }
        if let Some(a) = &mut self.active {
            a.windows.retain(|_, w| *w != h);
        }
        if self.watch.drag.as_ref().is_some_and(|d| d.window == h) {
            self.kill_timer(timer::DRAG);
            self.watch.drag = None;
            self.watch.preview.hide();
        }
    }

    pub(crate) fn hide_frames(&mut self) {
        for f in self.watch.frames.values_mut() {
            f.frame.hide();
        }
    }

    /// Shows, moves or hides the frames of every window of the open workspace.
    pub(crate) fn refresh_frames(&mut self) {
        let windows: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let keep: HashSet<WindowHandle> = windows.iter().copied().collect();
        self.watch.frames.retain(|h, _| keep.contains(h));
        for h in windows {
            self.refresh_frame(h);
        }
        self.sync_location_hooks();
    }

    /// The frame of one window: shown around it when it sits in a zone of the open workspace,
    /// hidden otherwise.
    pub(crate) fn refresh_frame(&mut self, h: WindowHandle) {
        let wanted = self.frame_for(h);
        let Some((frame_rect, look, pid)) = wanted else {
            if let Some(f) = self.watch.frames.get_mut(&h) {
                f.frame.hide();
            }
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(slot) = self.watch.frames.entry(h) {
            match Frame::new() {
                Ok(frame) => {
                    slot.insert(Framed { pid, frame });
                    self.sync_location_hooks();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "frame not created");
                    return;
                }
            }
        }
        if self.canvas().is_none() {
            return;
        }
        let (Some(canvas), Some(framed)) = (self.watch.canvas.as_mut(), self.watch.frames.get_mut(&h)) else {
            return;
        };
        if let Err(e) = framed
            .frame
            .show(canvas, frame_rect, &look, h.0.cast_signed_isize())
        {
            tracing::debug!(error = %e, "frame not drawn");
        }
    }

    fn frame_for(&mut self, h: WindowHandle) -> Option<(Rect, FrameLook, u32)> {
        let frames = self.settings.frames;
        if !(frames.show_frames || frames.show_tags)
            || self.watch.drag.as_ref().is_some_and(|d| d.window == h)
        {
            return None;
        }
        let active = self.active.as_ref()?;
        active.slot_of(h)?;
        let hwnd = window::hwnd(h);
        let g = window::geometry(hwnd)?;
        if !g.framable() {
            return None;
        }
        let zone = active.zone_for(g.frame, self.style())?;
        let look = FrameLook {
            color: zone.color,
            label: zone.name.clone(),
            dpi: zone.dpi,
            rounded: self.watch.rounded,
            ring: frames.show_frames,
            tag: frames.show_tags,
        };
        Some((g.frame, look, window::pid(hwnd)))
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
        if let Some(f) = self.watch.frames.get_mut(&h) {
            f.frame.hide();
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
        self.watch.drag = Some(Drag {
            window: h,
            pid: facts.pid,
            start: facts.frame_rect,
            was_maximized: facts.maximized,
            kind: DragKind::Undecided,
            zones,
            target: None,
        });
        self.sync_location_hooks();
        self.set_timer(timer::DRAG, DRAG_TICK_MS);
    }

    pub(crate) fn drag_tick(&mut self) {
        if self.watch.drag.is_none() {
            self.kill_timer(timer::DRAG);
            return;
        }
        self.drag_update();
    }

    /// Follows the dragged window: decides whether it is a move or a resize, shows the zones
    /// on a move, and lights the one it would drop into.
    fn drag_update(&mut self) {
        let Some(d) = &self.watch.drag else { return };
        let (h, start, kind, was_maximized) = (d.window, d.start, d.kind, d.was_maximized);
        let Some(g) = window::geometry(window::hwnd(h)) else {
            return;
        };
        if kind == DragKind::Undecided {
            if g.frame.size() != start.size() && !was_maximized {
                if let Some(d) = &mut self.watch.drag {
                    d.kind = DragKind::Resizing;
                }
                return;
            }
            if g.frame == start {
                return;
            }
            if let Some(d) = &mut self.watch.drag {
                d.kind = DragKind::Moving;
            }
            self.show_preview(h);
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
        let target = self.watch.drag.as_ref().and_then(|d| {
            if !snapping {
                return None;
            }
            let id = snap::target(cursor, &d.zones, self.settings.snap)?;
            d.zones.iter().position(|z| &z.id == id)
        });
        if let Some(d) = &mut self.watch.drag {
            d.target = target;
        }
        if snapping {
            if !self.watch.preview_visible {
                self.show_preview(h);
            }
            if self.canvas().is_none() {
                return;
            }
            if let Some(canvas) = self.watch.canvas.as_mut()
                && let Err(e) = self.watch.preview.set_hot(canvas, target)
            {
                tracing::debug!(error = %e, "drop target not drawn");
            }
        } else {
            // Shift held: the window moves freely and the zones step aside.
            self.watch.preview.hide();
            self.watch.preview_visible = false;
        }
    }

    fn show_preview(&mut self, h: WindowHandle) {
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
        if self.canvas().is_none() {
            return;
        }
        if let Some(canvas) = self.watch.canvas.as_mut() {
            match self
                .watch
                .preview
                .show(canvas, &views, Stack::Below(h.0.cast_signed_isize()))
            {
                Ok(()) => self.watch.preview_visible = true,
                Err(e) => tracing::debug!(error = %e, "zones not shown"),
            }
        }
    }

    fn drag_end(&mut self, h: WindowHandle) {
        let Some(d) = self.watch.drag.take().filter(|d| d.window == h) else {
            // Not a drag Mosaïk follows: the window may have moved in or out of a zone.
            self.after_user_move(h);
            return;
        };
        self.kill_timer(timer::DRAG);
        self.watch.preview.hide();
        self.watch.preview_visible = false;
        if d.kind == DragKind::Moving
            && let Some(i) = d.target
            && let Some(zone) = d.zones.get(i)
        {
            self.snap_into(h, zone.id.clone());
        }
        self.sync_location_hooks();
        self.after_user_move(h);
    }

    /// The user moved or resized a window by hand: the open workspace remembers it.
    fn after_user_move(&mut self, h: WindowHandle) {
        if self.active.as_ref().is_some_and(|a| a.slot_of(h).is_some()) {
            self.remember_active();
        }
        self.refresh_frame(h);
    }

    /// Pulls a dropped window into a zone of the open workspace; a window that was not part of
    /// the workspace joins it.
    fn snap_into(&mut self, h: WindowHandle, zone: mosaik_core::zone::ZoneId) {
        let Some(active) = &self.active else { return };
        let Some(live) = active.zones.iter().find(|z| z.id == zone).cloned() else {
            return;
        };
        let hwnd = window::hwnd(h);
        let target = self.style().window_area(live.rect, live.dpi);
        if let Err(e) = place::move_frame(hwnd, target) {
            tracing::warn!(error = %e, "window not pulled into its zone");
            return;
        }
        let Some(index) = self.file.workspaces.iter().position(|w| w.id == active.id) else {
            return;
        };
        let slot_id = active.slot_of(h).cloned();
        let ws = &mut self.file.workspaces[index];
        match slot_id.and_then(|id| ws.layout.slots.iter_mut().find(|s| s.id == id)) {
            Some(slot) => {
                if !matches!(slot.place, Place::Tab { .. }) {
                    slot.place = Place::Zone { zone };
                }
            }
            None => {
                let Some(facts) = self.scanner.facts(hwnd) else {
                    return;
                };
                let path = self.scanner.process_path(facts.pid);
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
                        app: app_match(&facts),
                        launch: apps::launch_for(&facts, path.as_deref()),
                        place: Place::Zone { zone },
                    },
                );
                if let Some(a) = &mut self.active {
                    a.windows.insert(id, h);
                }
            }
        }
        self.persist();
        self.refresh_frame(h);
        self.sync_location_hooks();
    }
}

/// Window handles as the overlay crate takes them (a pointer-sized integer).
trait AsIsize {
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
