//! Moving windows so their visible frame lands exactly on a target rectangle.
//!
//! The rectangle passed to Windows includes the invisible resize borders, measured for each
//! window at the moment of the move (they scale with DPI). When a window crosses to a monitor
//! with another DPI it rescales itself, so the move is repeated once Windows has settled.

use mosaik_core::geometry::{Insets, Rect};
use mosaik_core::place::window_rect_for;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowPlacement, IsIconic, IsZoomed, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SWP_ASYNCWINDOWPOS,
    SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPlacement, SetWindowPos, WINDOWPLACEMENT,
    WPF_ASYNCWINDOWPLACEMENT,
};

use crate::error::{Context, Error, Result};
use crate::window::{exists, frame_rect, window_rect};

/// What a move achieved, for `mosaikctl state` and the probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moved {
    pub target: Rect,
    /// The visible frame after the move.
    pub frame: Rect,
    /// Largest distance between a frame edge and the target edge, in pixels.
    pub error: i32,
    /// Moves issued (2 when the window changed DPI on the way).
    pub passes: u32,
    /// The window was maximised or minimised and had to be restored first.
    pub restored: bool,
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

/// Converts a screen rectangle to the "workspace" coordinates WINDOWPLACEMENT uses: relative to
/// the work area's offset inside its monitor (non-zero when the taskbar is on the left or top).
fn to_workspace(r: Rect) -> Rect {
    let rc = win_rect(r);
    // SAFETY: MonitorFromRect accepts any rectangle; GetMonitorInfoW gets a sized struct.
    let offset = unsafe {
        let m = MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).unwrap_or(0),
            ..Default::default()
        };
        if GetMonitorInfoW(m, &mut info).as_bool() {
            (
                info.rcWork.left - info.rcMonitor.left,
                info.rcWork.top - info.rcMonitor.top,
            )
        } else {
            (0, 0)
        }
    };
    r.offset(-offset.0, -offset.1)
}

fn set_pos(hwnd: HWND, r: Rect, asynchronous: bool) -> Result<()> {
    let mut flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    if asynchronous {
        // Never block on an app that is not answering.
        flags |= SWP_ASYNCWINDOWPOS;
    }
    // SAFETY: plain SetWindowPos on a top-level window; fails cleanly if it is gone or protected.
    unsafe { SetWindowPos(hwnd, None, r.left, r.top, r.width(), r.height(), flags) }.ctx("SetWindowPos")
}

/// Restores a maximised or minimised window straight to a normal rectangle, without activating
/// it and without a restore animation to its old place first.
fn restore_to(hwnd: HWND, window: Rect) -> Result<()> {
    let mut wp = WINDOWPLACEMENT {
        length: u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: sized struct and a valid window.
    unsafe { GetWindowPlacement(hwnd, &mut wp) }.ctx("GetWindowPlacement")?;
    let ws = to_workspace(window);
    wp.rcNormalPosition = win_rect(ws);
    wp.showCmd = u32::try_from(SW_SHOWNOACTIVATE.0).unwrap_or(4);
    wp.flags |= WPF_ASYNCWINDOWPLACEMENT;
    // SAFETY: sized struct filled by GetWindowPlacement and edited above.
    unsafe { SetWindowPlacement(hwnd, &wp) }.ctx("SetWindowPlacement")
}

/// Moves `hwnd` so its visible frame covers `target`. Synchronous: when it returns, the window
/// is where it will stay (used by tests and probes, and for single moves).
pub fn move_frame(hwnd: HWND, target: Rect) -> Result<Moved> {
    move_frame_with(hwnd, target, false)
}

/// Like [`move_frame`], optionally without waiting for the app (for switching many windows: a
/// hung app must not hold up the others). The result is then measured right away and may lag.
pub fn move_frame_with(hwnd: HWND, target: Rect, asynchronous: bool) -> Result<Moved> {
    if !exists(hwnd) {
        return Err(Error::WindowGone);
    }
    let insets_now = || -> Option<Insets> { Some(Insets::between(window_rect(hwnd)?, frame_rect(hwnd)?)) };
    // SAFETY: read-only state queries.
    let (zoomed, iconic) = unsafe { (IsZoomed(hwnd).as_bool(), IsIconic(hwnd).as_bool()) };
    let restored = zoomed || iconic;
    let mut passes = 0;
    if restored {
        // A maximised window's insets are not those of its normal state; restore first with the
        // target as its normal rectangle, then measure and correct below.
        restore_to(hwnd, target)?;
        passes += 1;
    }
    // SAFETY: read-only.
    let dpi_before = unsafe { GetDpiForWindow(hwnd) };
    let insets = insets_now().ok_or(Error::WindowGone)?;
    set_pos(hwnd, window_rect_for(target, insets), asynchronous)?;
    passes += 1;
    // SAFETY: read-only.
    let dpi_after = unsafe { GetDpiForWindow(hwnd) };
    let mut frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    // The window changed DPI (crossed monitors) or its borders changed: its insets are different
    // now, so place it again with the new ones.
    if dpi_after != dpi_before || edge_error(frame, target) > 0 {
        let insets = insets_now().ok_or(Error::WindowGone)?;
        set_pos(hwnd, window_rect_for(target, insets), asynchronous)?;
        passes += 1;
        frame = frame_rect(hwnd).ok_or(Error::WindowGone)?;
    }
    Ok(Moved {
        target,
        frame,
        error: edge_error(frame, target),
        passes,
        restored,
    })
}

/// Maximises a window on the monitor containing `on` (a point or small rectangle on it),
/// without activating it.
pub fn maximize_on(hwnd: HWND, on: Rect) -> Result<()> {
    let mut wp = WINDOWPLACEMENT {
        length: u32::try_from(std::mem::size_of::<WINDOWPLACEMENT>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: sized struct and a valid window.
    unsafe { GetWindowPlacement(hwnd, &mut wp) }.ctx("GetWindowPlacement")?;
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
