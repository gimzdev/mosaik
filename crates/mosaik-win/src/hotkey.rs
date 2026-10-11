//! Registering a global shortcut with Windows.

use mosaik_core::hotkey::Hotkey;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};

use crate::error::{Error, Result};

/// A registered shortcut; unregistered when dropped. Must be dropped on the thread that
/// registered it.
pub struct Registration {
    hwnd: Option<HWND>,
    id: i32,
}

/// Why registering failed, in words for the user.
pub fn explain(code: u32) -> &'static str {
    match code {
        1409 => "Another app (or Windows) already uses this shortcut.",
        1408 => "Internal error: the shortcut was registered from the wrong thread.",
        _ => "Windows did not accept this shortcut.",
    }
}

/// Registers `hotkey`, delivering `WM_HOTKEY` with `id` to `hwnd` (or the thread when `None`).
/// Auto-repeat is off, so holding the keys fires once.
pub fn register(hwnd: Option<HWND>, id: i32, hotkey: Hotkey) -> Result<Registration> {
    let mods = HOT_KEY_MODIFIERS(u32::from(hotkey.mods.bits())) | MOD_NOREPEAT;
    // SAFETY: registers a hotkey for this thread's window (or thread); undone in Drop.
    unsafe { RegisterHotKey(hwnd, id, mods, u32::from(hotkey.key.vk())) }.map_err(|e| {
        let code = u32::try_from(e.code().0 & 0xFFFF).unwrap_or(0);
        Error::Other(format!("{hotkey}: {} (error {code})", explain(code)))
    })?;
    Ok(Registration { hwnd, id })
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: undoes the registration made in `register` on the same thread.
        let _ = unsafe { UnregisterHotKey(self.hwnd, self.id) };
    }
}
