//! Reads a keyboard layout's tables straight from its DLL (`KbdLayerDescriptor`), without
//! installing or activating the layout: what AltGr types, what the right Ctrl key is.

use std::ffi::c_void;

use mosaik_win::text::wide;
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{PCSTR, PCWSTR};

#[repr(C)]
struct KbdTables {
    char_modifiers: *const Modifiers,
    vk_to_wchar_table: *const VkToWcharTable,
    dead_key: *const c_void,
    key_names: *const c_void,
    key_names_ext: *const c_void,
    key_names_dead: *const c_void,
    vsc_to_vk: *const u16,
    max_vsc_to_vk: u8,
    vsc_to_vk_e0: *const VscVk,
    vsc_to_vk_e1: *const VscVk,
    locale_flags: u32,
    lg_max: u8,
    cb_lg_entry: u8,
    ligature: *const c_void,
    kind: u32,
    sub_kind: u32,
}

#[repr(C)]
struct Modifiers {
    vk_to_bit: *const c_void,
    max_mod_bits: u16,
    mod_number: [u8; 1],
}

#[repr(C)]
struct VkToWcharTable {
    rows: *const u8,
    modifications: u8,
    size: u8,
}

#[repr(C)]
struct VscVk {
    vsc: u8,
    vk: u16,
}

const KLLF_ALTGR: u32 = 0x1;
const SHIFT: u8 = 0x1;
const CTRL: u8 = 0x2;
const ALT: u8 = 0x4;
const WCH_NONE: u16 = 0xF000;
const WCH_DEAD: u16 = 0xF001;
const WCH_LGTR: u16 = 0xF002;

pub struct LayoutInfo {
    pub has_altgr: bool,
    /// The virtual key the right Ctrl key sends (0xA3 normally; 0xDF, VK_OEM_8, on Canadian Multilingual).
    pub right_ctrl_vk: Option<u16>,
    /// The virtual key of scan code 0x39 (Space).
    pub space_vk: Option<u16>,
    /// What a key types with the given modifier bits.
    chars: Vec<(u8, u8, char)>,
}

impl LayoutInfo {
    pub fn typed(&self, vk: u8, mods: u8) -> Option<char> {
        self.chars
            .iter()
            .find(|(v, m, _)| *v == vk && *m == mods)
            .map(|(_, _, c)| *c)
    }

    /// Letters and digits that type a character with AltGr (Ctrl+Alt), with and without Shift.
    pub fn altgr_letters_and_digits(&self) -> Vec<(char, char)> {
        self.chars
            .iter()
            .filter(|(vk, m, _)| {
                (*m == CTRL | ALT || *m == CTRL | ALT | SHIFT)
                    && (vk.is_ascii_uppercase() || vk.is_ascii_digit())
            })
            .map(|(vk, _, c)| (char::from(*vk), *c))
            .collect()
    }

    pub fn altgr_space(&self) -> Option<char> {
        self.typed(0x20, CTRL | ALT)
    }
}

/// Loads `dll` (a layout DLL in System32, e.g. "kbdcan.dll") and reads its tables.
pub fn read(dll: &str) -> Result<LayoutInfo, String> {
    let name = wide(dll);
    // SAFETY: loads a Windows keyboard layout DLL from System32 (its DllMain does nothing) and
    // frees it before returning; the tables are copied out first.
    unsafe {
        let module = LoadLibraryExW(PCWSTR(name.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
            .map_err(|e| format!("{dll}: {e}"))?;
        let result = (|| {
            let proc = GetProcAddress(module, PCSTR(c"KbdLayerDescriptor".as_ptr().cast()))
                .ok_or("no KbdLayerDescriptor export")?;
            let descriptor: unsafe extern "system" fn() -> *const KbdTables = std::mem::transmute(proc);
            let t = descriptor();
            if t.is_null() {
                return Err("KbdLayerDescriptor returned nothing".to_owned());
            }
            Ok(parse(&*t))
        })();
        let _ = FreeLibrary(module);
        result
    }
}

/// # Safety
/// `t` must be the tables returned by a loaded layout DLL.
unsafe fn parse(t: &KbdTables) -> LayoutInfo {
    // SAFETY: walks the null-terminated tables the layout DLL defines, within their declared sizes.
    unsafe {
        let mut chars = Vec::new();
        let m = &*t.char_modifiers;
        let mod_numbers = std::slice::from_raw_parts(m.mod_number.as_ptr(), usize::from(m.max_mod_bits) + 1);
        let mut table = t.vk_to_wchar_table;
        while !(*table).rows.is_null() {
            let n = usize::from((*table).modifications);
            let size = usize::from((*table).size);
            let mut row = (*table).rows;
            while *row != 0 {
                let vk = *row;
                if vk != 0xFF {
                    for (bits, &col) in mod_numbers.iter().enumerate() {
                        let col = usize::from(col);
                        if col >= n || col == 0x0F {
                            continue;
                        }
                        let wch = std::ptr::read_unaligned(row.add(2 + 2 * col).cast::<u16>());
                        if wch != WCH_NONE
                            && wch != WCH_DEAD
                            && wch != WCH_LGTR
                            && wch >= 0x20
                            && let Some(c) = char::from_u32(u32::from(wch))
                        {
                            chars.push((vk, u8::try_from(bits).unwrap_or(0), c));
                        }
                    }
                }
                row = row.add(size);
            }
            table = table.add(1);
        }
        let mut right_ctrl_vk = None;
        let mut e0 = t.vsc_to_vk_e0;
        while !e0.is_null() && (*e0).vsc != 0 {
            if (*e0).vsc == 0x1D {
                right_ctrl_vk = Some((*e0).vk & 0xFF);
            }
            e0 = e0.add(1);
        }
        let space_vk = (0x39 <= t.max_vsc_to_vk).then(|| *t.vsc_to_vk.add(0x39) & 0xFF);
        LayoutInfo {
            has_altgr: t.locale_flags & KLLF_ALTGR != 0,
            right_ctrl_vk,
            space_vk,
            chars,
        }
    }
}
