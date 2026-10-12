//! Bringing windows forward, focusing and minimising them. Calls on other apps' windows are
//! asynchronous where Windows allows it, so an app that is not responding never holds up Mosaïk.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, HWND_TOP, IsHungAppWindow, IsIconic, SW_SHOWMINNOACTIVE, SWP_ASYNCWINDOWPOS,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow,
    SetWindowPos, ShowWindowAsync,
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

pub fn is_minimized(hwnd: HWND) -> bool {
    // SAFETY: a read-only query on any handle.
    unsafe { IsIconic(hwnd) }.as_bool()
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
