//! Is a window hidden, or showing? Checked the way each hiding method hides.

use mosaik_core::journal::HideMethod;
use mosaik_core::window::Cloak;
use mosaik_win::window::{cloak, frame_rect};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

fn on_some_monitor(hwnd: HWND) -> bool {
    frame_rect(hwnd).is_some_and(|f| {
        mosaik_win::monitors::topology()
            .is_ok_and(|t| t.monitors().iter().any(|m| m.bounds.overlap_area(f) > 0))
    })
}

/// Hidden the way `method` hides: cloaked, minimised, or parked off every monitor.
pub fn is_hidden(hwnd: HWND, method: HideMethod) -> bool {
    match method {
        HideMethod::ShellCloak => cloak(hwnd) != Cloak::None,
        // SAFETY: read-only.
        HideMethod::Minimize => unsafe { IsIconic(hwnd) }.as_bool(),
        HideMethod::Park => !on_some_monitor(hwnd),
    }
}

/// Visible, not minimised, not cloaked and on a monitor.
pub fn is_shown(hwnd: HWND) -> bool {
    // SAFETY: read-only.
    let shown = unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() };
    shown && cloak(hwnd) == Cloak::None && on_some_monitor(hwnd)
}
