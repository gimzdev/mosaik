//! What the keyboard layouts installed on this PC type with AltGr, so a shortcut that would
//! stop someone typing a character is refused (`mosaik_core::hotkey::check`).

use mosaik_core::hotkey::LayoutTyping;
use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SLOCALIZEDDISPLAYNAME};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayoutList, HKL, MAPVK_VK_TO_VSC, MapVirtualKeyExW, ToUnicodeEx, VK_CONTROL, VK_LCONTROL,
    VK_MENU, VK_RMENU, VK_SHIFT,
};
use windows::core::PCWSTR;

use crate::text::from_wide;

/// Keys that type characters: digits, letters, Space and the punctuation keys.
fn typing_keys() -> impl Iterator<Item = u16> {
    (0x30..=0x39)
        .chain(0x41..=0x5A)
        .chain([0x20])
        .chain(0xBA..=0xC0)
        .chain(0xDB..=0xDF)
        .chain([0xE2])
}

/// Don't change the keyboard state (dead keys stay untouched); Windows 10 1607 and later.
const NO_STATE_CHANGE: u32 = 0x4;

fn layout_name(hkl: HKL) -> String {
    // The language is the low word of the layout handle.
    let lang = u32::try_from(hkl.0 as usize & 0xFFFF).unwrap_or(0);
    let mut locale = [0u16; 85];
    // SAFETY: writes at most the buffer's length.
    let n = unsafe { LCIDToLocaleName(lang, Some(&mut locale), 0) };
    if n <= 0 {
        return format!("layout {lang:04X}");
    }
    let mut name = [0u16; 128];
    // SAFETY: the locale name is NUL-terminated (LCIDToLocaleName wrote it); writes at most the
    // buffer's length.
    let m = unsafe {
        GetLocaleInfoEx(
            PCWSTR(locale.as_ptr()),
            LOCALE_SLOCALIZEDDISPLAYNAME,
            Some(&mut name),
        )
    };
    if m <= 0 {
        from_wide(&locale)
    } else {
        from_wide(&name)
    }
}

/// Every installed layout with what Ctrl+Alt (AltGr), with and without Shift, types on it.
pub fn layouts() -> Vec<LayoutTyping> {
    // SAFETY: asks for the count first, then fills a buffer of that size.
    let count = unsafe { GetKeyboardLayoutList(None) };
    let mut list = vec![HKL::default(); usize::try_from(count).unwrap_or(0)];
    // SAFETY: the buffer holds `count` handles.
    let got = unsafe { GetKeyboardLayoutList(Some(&mut list)) };
    list.truncate(usize::try_from(got).unwrap_or(0));
    list.into_iter()
        .map(|hkl| {
            let mut altgr = Vec::new();
            for shift in [false, true] {
                let mut state = [0u8; 256];
                for vk in [VK_CONTROL, VK_LCONTROL, VK_MENU, VK_RMENU] {
                    state[usize::from(vk.0)] = 0x80;
                }
                if shift {
                    state[usize::from(VK_SHIFT.0)] = 0x80;
                }
                for vk in typing_keys() {
                    // SAFETY: pure lookups in the given layout; the output buffer is local.
                    let typed = unsafe {
                        let scan = MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_VSC, Some(hkl));
                        let mut out = [0u16; 4];
                        let n =
                            ToUnicodeEx(u32::from(vk), scan, &state, &mut out, NO_STATE_CHANGE, Some(hkl));
                        (n == 1).then(|| char::from_u32(u32::from(out[0]))).flatten()
                    };
                    if let Some(ch) = typed.filter(|c| !c.is_control()) {
                        altgr.push((vk, shift, ch));
                    }
                }
            }
            LayoutTyping {
                name: layout_name(hkl),
                altgr,
            }
        })
        .collect()
}
