//! Bringing windows forward, focusing and minimising them. Calls on other apps' windows are
//! asynchronous where Windows allows it, so an app that is not responding never holds up Mosaïk.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, HTCAPTION, HWND_TOP, IsHungAppWindow, IsIconic, PostMessageW, SC_MOVE,
    SW_SHOWMINNOACTIVE, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SetForegroundWindow, SetWindowPos, ShowWindowAsync, WM_SYSCOMMAND,
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

/// A press on the band Mosaïk draws above `hwnd` is a press on its title bar: the window comes
/// to the front and Windows moves it with the pointer, in its own move loop, as from the title
/// bar (zones, tags and Windows' snapping follow as for any drag). Called on the button going
/// down; the request is posted, so an app that is not responding holds nothing up, and a press
/// already let go when the app gets to it moves nothing.
pub fn start_move(hwnd: HWND) {
    let mut p = POINT::default();
    // SAFETY: a valid out pointer.
    let _ = unsafe { GetCursorPos(&raw mut p) };
    // Screen coordinates packed as for any mouse message: the low 16 bits of each (negative
    // ones too, as Windows packs them).
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let packed = ((p.y as u16 as usize) << 16) | (p.x as u16 as usize);
    // The low four bits of SC_MOVE say how the move was asked for: from the title bar, with the
    // mouse.
    let command = SC_MOVE as usize | HTCAPTION as usize;
    // SAFETY: Windows allows the app that just received the click to bring a window forward;
    // the command is posted to the window's own thread.
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let _ = PostMessageW(
            Some(hwnd),
            WM_SYSCOMMAND,
            WPARAM(command),
            LPARAM(packed.cast_signed()),
        );
    }
}
