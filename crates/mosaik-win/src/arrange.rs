//! Bringing windows forward, focusing and minimising them. Calls on other apps' windows are
//! asynchronous where Windows allows it, so an app that is not responding never holds up Mosaïk.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOP, IsHungAppWindow, SW_SHOWMINNOACTIVE,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, ShowWindowAsync, WS_EX_TOPMOST,
};

use crate::error::{Context, Result};

/// The window the user is working in, if any.
pub fn foreground() -> Option<HWND> {
    // SAFETY: no preconditions.
    let h = unsafe { GetForegroundWindow() };
    (!h.is_invalid()).then_some(h)
}

/// True when Windows considers the window's app not responding (no messages handled for 5 s).
pub fn is_hung(hwnd: HWND) -> bool {
    // SAFETY: a read-only query on any handle.
    unsafe { IsHungAppWindow(hwnd) }.as_bool()
}

/// Puts a window on top of the other normal windows without activating it.
pub fn raise(hwnd: HWND) -> Result<()> {
    // SAFETY: z-order only; asynchronous, so a hung app does not block.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS,
        )
    }
    .ctx("SetWindowPos")
}

/// Whether `hwnd` stays above every window that is not (always on top).
fn topmost(hwnd: HWND) -> bool {
    // SAFETY: reads the extended style of any window handle; 0 for one that has gone.
    let style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    style & WS_EX_TOPMOST.0 as isize != 0
}

/// Puts `hwnd` just below `above`, without activating it: windows stacked in one zone put in
/// the order the user chose (ADR 0032), under the front one rather than over every window,
/// Mosaïk's own among them. Synchronous, unlike [`raise`], so a chain of them lands in order:
/// call it off the engine thread (the mover does). Only between two windows alike, always on top
/// or not: under one that is, a window would become so; one that is would stop being so under
/// one that is not. Returns whether it was done.
pub fn put_below(hwnd: HWND, above: HWND) -> Result<bool> {
    if topmost(hwnd) != topmost(above) {
        return Ok(false);
    }
    // SAFETY: z-order only (no move, no size, no activation) of another app's window.
    unsafe {
        SetWindowPos(
            hwnd,
            Some(above),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
    }
    .ctx("SetWindowPos")?;
    Ok(true)
}

/// Minimises a window without activating another one in its place.
pub fn minimize(hwnd: HWND) -> bool {
    // SAFETY: asynchronous show command on another app's window.
    unsafe { ShowWindowAsync(hwnd, SW_SHOWMINNOACTIVE) }.as_bool()
}

/// Gives a window the keyboard focus. Windows only lets the app the user is working with (or
/// that just received a hotkey or a click) move the focus, which is the case when a workspace is
/// opened from the switcher or the tray; from `mosaikctl` the window comes forward without the
/// focus. Returns whether the window is now in front.
pub fn focus(hwnd: HWND) -> bool {
    // SAFETY: asks Windows to activate the window; it decides whether that is allowed.
    unsafe { SetForegroundWindow(hwnd) }.as_bool() && foreground() == Some(hwnd)
}

/// Moves a window so its window rectangle starts at (`x`, `y`), keeping its size, without
/// activating it: the window following the pointer while the user drags it by the band Mosaïk
/// draws above it. Synchronous, so moves land in order and before the drop that follows: call
/// it from a thread of its own, never Mosaïk's engine thread (`mover::Follower`). An app Windows
/// reports as not responding is not asked.
pub fn move_to(hwnd: HWND, x: i32, y: i32) {
    if is_hung(hwnd) {
        return;
    }
    // SAFETY: a move of another app's window; it fails cleanly if the window is gone or
    // protected.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )
    };
}
