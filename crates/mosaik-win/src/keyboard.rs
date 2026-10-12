//! What the keyboard layouts installed on this PC type with AltGr, so a shortcut that would
//! stop someone typing a character is refused (`mosaik_core::hotkey::check`).

use mosaik_core::hotkey::LayoutTyping;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SLOCALIZEDDISPLAYNAME};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayoutList, HKL, MAPVK_VK_TO_VSC, MapVirtualKeyExW, ToUnicodeEx, VK_CONTROL, VK_LCONTROL,
    VK_MENU, VK_RMENU, VK_SHIFT,
};
use windows::Win32::UI::Shell::SHLoadIndirectString;
use windows::core::{PCWSTR, PWSTR};

use crate::text::{from_wide, wide};

/// Where Windows describes its keyboard layouts.
const LAYOUTS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Keyboard Layouts";

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

/// A string value under `HKEY_LOCAL_MACHINE`.
fn machine_string(key: &str, value: &str) -> Option<String> {
    let key = wide(key);
    let value = wide(value);
    let mut buf = [0u16; 260];
    let mut bytes = u32::try_from(std::mem::size_of_val(&buf)).ok()?;
    // SAFETY: reads one string value into the local buffer; `bytes` is its size in bytes.
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    (r == ERROR_SUCCESS).then(|| from_wide(&buf))
}

/// The registry name of a layout ("00001009" for Canadian French), from its handle: the high
/// word of the handle, or for a variant of a layout (device `0xFnnn`) the key whose "Layout Id"
/// is `nnn`.
fn layout_key(hkl: HKL) -> Option<String> {
    let device = (hkl.0 as usize >> 16) & 0xFFFF;
    if device & 0xF000 != 0xF000 {
        return Some(format!("{device:08X}"));
    }
    let wanted = device & 0x0FFF;
    let root = wide(LAYOUTS_KEY);
    let mut key = HKEY::default();
    // SAFETY: opens a read-only handle, closed below.
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(root.as_ptr()),
            None,
            KEY_READ,
            &raw mut key,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    let mut found = None;
    for index in 0.. {
        let mut name = [0u16; 16];
        let mut len = 16u32;
        // SAFETY: writes one subkey name into the local buffer of `len` characters.
        let r = unsafe {
            RegEnumKeyExW(
                key,
                index,
                Some(PWSTR(name.as_mut_ptr())),
                &raw mut len,
                None,
                None,
                None,
                None,
            )
        };
        if r != ERROR_SUCCESS {
            break;
        }
        let klid = from_wide(&name);
        let id = machine_string(&format!(r"{LAYOUTS_KEY}\{klid}"), "Layout Id")
            .and_then(|v| usize::from_str_radix(v.trim(), 16).ok());
        if id == Some(wanted) {
            found = Some(klid);
            break;
        }
    }
    // SAFETY: closes the handle opened above.
    let _ = unsafe { RegCloseKey(key) };
    found
}

/// What Windows calls a layout ("Canadian Multilingual Standard"), in the user's language
/// when it can say; else its language ("French (Canada)").
fn layout_name(hkl: HKL) -> String {
    let named = layout_key(hkl).and_then(|klid| {
        let key = format!(r"{LAYOUTS_KEY}\{klid}");
        machine_string(&key, "Layout Display Name")
            .and_then(|source| {
                let source = wide(&source);
                let mut out = [0u16; 260];
                // SAFETY: resolves an "@file,-id" resource string into the local buffer.
                unsafe { SHLoadIndirectString(PCWSTR(source.as_ptr()), &mut out, None) }.ok()?;
                Some(from_wide(&out))
            })
            .or_else(|| machine_string(&key, "Layout Text"))
            .filter(|n| !n.trim().is_empty())
    });
    named.unwrap_or_else(|| language_name(hkl))
}

fn language_name(hkl: HKL) -> String {
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
                        // 1 or more: what the key types; -1: a dead key (an accent waiting for
                        // the next key), whose own character is in `out` too. Either way the
                        // key types with AltGr.
                        (n != 0).then(|| char::decode_utf16(out).next()?.ok()).flatten()
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
