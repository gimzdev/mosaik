//! Moving windows so their visible frame lands exactly on a target rectangle.
//!
//! The rectangle passed to Windows includes the invisible resize borders, measured for each
//! window at the moment of the move (they scale with DPI). When a window crosses to a monitor
//! with another DPI it rescales itself, so the move is repeated once Windows has settled.
//!
//! Measured on real apps (docs/STATUS.md, step 1): Chrome and Edge will not go below about
//! 500 px wide and Windows Terminal below about 466 px, so a window may keep a size other than
//! the target's; it is then fitted on screen with [`fit_size`]. A window without a frame to
//! resize it by (a game, a dialog) is never resized: it keeps its size in the middle of the
//! target. Some apps also move themselves after a restore (VS Code went back to its previous
//! position 300 ms after being restored into a zone), so callers check the result again once
//! the app has settled ([`settle`]).

use mosaik_core::geometry::{Insets, Rect, Size};
use mosaik_core::place::{fit_size, window_rect_for};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, GetWindowPlacement, IsIconic, IsZoomed, SM_CXPADDEDBORDER, SM_CXSIZEFRAME,
    SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT, WPF_ASYNCWINDOWPLACEMENT, WS_THICKFRAME,
};

use crate::error::{Context, Error, Result};
use crate::window::{exists, frame_rect, window_rect};

/// What a move achieved, for `mosaikctl state` and the probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moved {
    pub target: Rect,
    /// Where the frame was meant to end up: the target, or, when the app kept another size, that
    /// size fitted on screen (see `constrained`).
    pub wanted: Rect,
    /// The visible frame after the move.
    pub frame: Rect,
    /// Largest distance between a frame edge and `wanted`, in pixels.
    pub error: i32,
    /// Moves issued (more when the window changed DPI on the way or kept its own size).
    pub passes: u32,
    /// The window was maximised or minimised and had to be restored first.
    pub restored: bool,
    /// The app would not take the target's size (a minimum or fixed size).
    pub constrained: bool,
}

pub fn edge_error(a: Rect, b: Rect) -> i32 {
    [
        a.left - b.left,
        a.top - b.top,
        a.right - b.right,
        a.bottom - b.bottom,
    ]
    .into_iter()
    .map(i32::abs)
    .max()
    .unwrap_or(0)
}

fn win_rect(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// The bounds and work area of the monitor nearest to `r`.
fn monitor_of(r: Rect) -> Option<(Rect, Rect)> {
    let rc = win_rect(r);
    // SAFETY: MonitorFromRect accepts any rectangle; GetMonitorInfoW gets a sized struct.
    unsafe {
        let m = MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).unwrap_or(0),
            ..Default::default()
        };
        GetMonitorInfoW(m, &mut info).as_bool().then(|| {
            (
                crate::monitors::rect(info.rcMonitor),
                crate::monitors::rect(info.rcWork),
            )
        })
    }
}

/// Converts a screen rectangle to the "workspace" coordinates WINDOWPLACEMENT uses: relative to
/// the work area's offset inside its monitor (non-zero when the taskbar is on the left or top).
fn to_workspace(r: Rect) -> Rect {
    let (dx, dy) = monitor_of(r).map_or((0, 0), |(monitor, work)| {
        (work.left - monitor.left, work.top - monitor.top)
    });
    r.offset(-dx, -dy)
}

/// Whether the user can resize `hwnd` by its frame (`WS_THICKFRAME`). A window without one (a
/// game, a dialog) was not made to change size.
fn has_resize_frame(hwnd: HWND) -> bool {
    // SAFETY: read-only query; 0 for a window that is gone.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
    style & isize::try_from(WS_THICKFRAME.0).unwrap_or(0) != 0
}

/// The invisible borders a resizable window will most likely have once restored, for its DPI
/// (7 px at 100% on the left, right and bottom). Only a first guess: the move is measured and
/// corrected right after.
fn likely_insets(hwnd: HWND) -> Insets {
    // SAFETY: read-only query on a valid window.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if !has_resize_frame(hwnd) || dpi == 0 {
        return Insets::default();
    }
    // SAFETY: GetSystemMetricsForDpi has no preconditions.
    let border = unsafe {
        GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    };
    // The outermost pixel of the border is the visible one.
    let b = (border - 1).max(0);
    Insets::new(b, 0, b, b)
}

fn placement(hwnd: HWND) -> Result<WINDOWPLACEMENT> {
    let mut wp = WINDOWPLACEMENT {
        length: u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: sized struct and a valid window.
    unsafe { GetWindowPlacement(hwnd, &mut wp) }.ctx("GetWindowPlacement")?;
    Ok(wp)
}

/// Where a window of `size` goes for `target`: in its middle, kept on screen.
fn fitted(target: Rect, size: Size) -> Rect {
    monitor_of(target).map_or_else(
        || Rect::from_origin_size(target.left, target.top, size.width, size.height),
        |(_, work)| fit_size(target, size, work),
    )
}

fn set_pos(hwnd: HWND, r: Rect) -> Result<()> {
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    // SAFETY: plain SetWindowPos on a top-level window; fails cleanly if it is gone or protected.
    unsafe { SetWindowPos(hwnd, None, r.left, r.top, r.width(), r.height(), flags) }.ctx("SetWindowPos")
}

/// Restores a maximised or minimised window straight to a normal rectangle, without activating
/// it and without a restore animation to its old place first. Synchronous: a restore that
/// landed after the corrective move that follows it left Chrome's frame 7 px inside its zone.
fn restore_to(hwnd: HWND, window: Rect) -> Result<()> {
    let mut wp = placement(hwnd)?;
    wp.rcNormalPosition = win_rect(to_workspace(window));
    wp.showCmd = u32::try_from(SW_SHOWNOACTIVATE.0).unwrap_or(4);
    wp.flags &= !WPF_ASYNCWINDOWPLACEMENT;
    // SAFETY: sized struct filled by GetWindowPlacement and edited above.
    unsafe { SetWindowPlacement(hwnd, &wp) }.ctx("SetWindowPlacement")
}

/// Moves a window without resizing it.
fn set_origin(hwnd: HWND, x: i32, y: i32) -> Result<()> {
    let flags = SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    // SAFETY: plain SetWindowPos on a top-level window; fails cleanly if it is gone or protected.
    unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, flags) }.ctx("SetWindowPos")
}

/// Moves `hwnd` so its visible frame covers `target`. Synchronous: when it returns, the window
/// is where it will stay (the engine moves each window on a thread of its own, `mover.rs`).
///
/// A window without a frame to resize it by keeps its own size, in the middle of `target`: a
/// game or a dialog stretched by someone else draws blurred, cut off or not at all.
pub fn move_frame(hwnd: HWND, target: Rect) -> Result<Moved> {
    if !exists(hwnd) {
        return Err(Error::WindowGone);
    }
    let insets_now = || -> Option<Insets> { Some(Insets::between(window_rect(hwnd)?, frame_rect(hwnd)?)) };
    // SAFETY: read-only state queries.
    let (zoomed, iconic) = unsafe { (IsZoomed(hwnd).as_bool(), IsIconic(hwnd).as_bool()) };
    let restored = zoomed || iconic;
    let fixed = !has_resize_frame(hwnd);
    let mut passes = 0;
    if restored {
        // A maximised window's insets are not those of its normal state: restore it into a
        // first guess, then measure and correct below. One of a fixed size keeps that size.
        let guess = if fixed {
            let r = placement(hwnd)?.rcNormalPosition;
            fitted(target, Size::new(r.right - r.left, r.bottom - r.top))
        } else {
            window_rect_for(target, likely_insets(hwnd))
        };
        restore_to(hwnd, guess)?;
        passes += 1;
    }
    let goal = if fixed {
        fitted(target, frame_rect(hwnd).ok_or(Error::WindowGone)?.size())
    } else {
        target
    };
    // SAFETY: read-only.
    let dpi_before = unsafe { GetDpiForWindow(hwnd) };
    let place = |goal: Rect| -> Result<()> {
        let w = window_rect_for(goal, insets_now().ok_or(Error::WindowGone)?);
        if fixed {
            set_origin(hwnd, w.left, w.top)
        } else {
            set_pos(hwnd, w)
        }
    };
    place(goal)?;
    passes += 1;
    // SAFETY: read-only.
    let dpi_after = unsafe { GetDpiForWindow(hwnd) };
    let mut frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    // The window changed DPI (crossed monitors) or its borders changed: its insets are different
    // now, so place it again with the new ones.
    if dpi_after != dpi_before || edge_error(frame, goal) > 0 {
        place(goal)?;
        passes += 1;
        frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    }
    // The app kept a size of its own (a minimum, or a new one for the monitor's DPI): fit that
    // size in the target instead of letting it hang off the edge.
    let mut wanted = goal;
    if frame.size() != goal.size() {
        wanted = fitted(target, frame.size());
        if wanted != frame {
            let w = window_rect_for(wanted, insets_now().ok_or(Error::WindowGone)?);
            set_origin(hwnd, w.left, w.top)?;
            passes += 1;
            frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
        }
    }
    Ok(Moved {
        target,
        wanted,
        frame,
        error: edge_error(frame, wanted),
        passes,
        restored,
        constrained: frame.size() != target.size(),
    })
}

/// How a window settled after a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settled {
    pub moved: Moved,
    /// The frame once the app stopped moving it.
    pub frame: Rect,
    /// Distance from `moved.wanted` before any correction (the app's own drift).
    pub drift: i32,
    /// Moves repeated because the app moved itself away.
    pub corrections: u32,
    /// Distance from the wanted rectangle at the end.
    pub error: i32,
}

/// Moves a window, waits (pumping messages) until the app stops moving it, and puts it back up
/// to twice if it wandered off. Blocking: for single moves, tests and probes; the engine does
/// the same from window events without blocking.
pub fn settle(hwnd: HWND, target: Rect, budget_ms: u32) -> Result<Settled> {
    let moved = move_frame(hwnd, target)?;
    let mut wanted = moved.wanted;
    let start = std::time::Instant::now();
    let mut last = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    let mut still_since = std::time::Instant::now();
    let mut drift = None;
    let mut corrections = 0;
    while start.elapsed().as_millis() < u128::from(budget_ms) {
        crate::events::pump_until(40, || false);
        let now = frame_rect(hwnd).ok_or(Error::WindowGone)?;
        if now != last {
            last = now;
            still_since = std::time::Instant::now();
            continue;
        }
        // Unchanged for 150 ms: the app is done with it.
        if still_since.elapsed().as_millis() < 150 {
            continue;
        }
        let error = edge_error(now, wanted);
        drift.get_or_insert(error);
        if error == 0 || corrections == 2 {
            break;
        }
        wanted = move_frame(hwnd, target)?.wanted;
        corrections += 1;
        last = frame_rect(hwnd).ok_or(Error::WindowGone)?;
        still_since = std::time::Instant::now();
    }
    let frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    Ok(Settled {
        moved,
        frame,
        drift: drift.unwrap_or_else(|| edge_error(frame, moved.wanted)),
        corrections,
        error: edge_error(frame, wanted),
    })
}

/// Maximises a window on the monitor containing `on` (a point or small rectangle on it),
/// without activating it.
pub fn maximize_on(hwnd: HWND, on: Rect) -> Result<()> {
    let mut wp = placement(hwnd)?;
    // Moving the normal rectangle onto the target monitor makes Windows maximise it there.
    let normal = mosaik_core::geometry::Rect::new(
        wp.rcNormalPosition.left,
        wp.rcNormalPosition.top,
        wp.rcNormalPosition.right,
        wp.rcNormalPosition.bottom,
    );
    let moved = Rect::from_origin_size(
        on.left,
        on.top,
        normal.width().min(on.width().max(200)),
        normal.height().min(on.height().max(150)),
    );
    wp.rcNormalPosition = win_rect(to_workspace(moved));
    wp.showCmd = u32::try_from(SW_SHOWMAXIMIZED.0).unwrap_or(3);
    wp.ptMaxPosition = POINT { x: 0, y: 0 };
    wp.flags |= WPF_ASYNCWINDOWPLACEMENT;
    // SAFETY: sized struct filled by GetWindowPlacement and edited above.
    unsafe { SetWindowPlacement(hwnd, &wp) }.ctx("SetWindowPlacement")
}
