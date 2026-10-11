//! The switcher shortcut on real keyboard layouts.
//!
//! 1. Registers Win+Alt+Space and presses it with synthetic input: does it fire, how fast?
//! 2. For each installed layout, switched on for this probe's window only: Win+Left Alt+Space
//!    must fire; Win+Right Alt+Space must fire only where Right Alt is plain Alt (on AltGr
//!    layouts it adds Ctrl); AltGr+Space alone must not fire and must still type.
//! 3. For US, Canadian Multilingual, Canadian French, French and German, reads the layout tables
//!    (without installing anything): AltGr, what AltGr+Space types, what the right Ctrl key is.

use std::cell::RefCell;

use mosaik_core::geometry::Rect;
use mosaik_core::hotkey::Hotkey;
use mosaik_win::input::Chord;
use mosaik_win::own::{WindowClass, plain_wndproc};
use mosaik_win::time;
use serde_json::{Value, json};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ACTIVATE_KEYBOARD_LAYOUT_FLAGS, ActivateKeyboardLayout, GetKeyboardLayout, GetKeyboardLayoutList, HKL,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, SC_KEYMENU, SW_SHOWNOACTIVATE, SetForegroundWindow, ShowWindow, WM_CHAR, WM_HOTKEY,
    WM_SYSCOMMAND, WS_CAPTION, WS_EX_TOPMOST,
};

use crate::kbd;

const LWIN: u16 = 0x5B;
const LMENU: u16 = 0xA4;
const RMENU: u16 = 0xA5;
const SPACE: u16 = 0x20;

thread_local! {
    static HOTKEYS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
    static CHARS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

unsafe extern "system" fn probe_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => HOTKEYS.with(|h| h.borrow_mut().push(time::now())),
        WM_CHAR => CHARS.with(|c| c.borrow_mut().push(u32::try_from(wparam.0).unwrap_or(0))),
        // Alt+Space (Right Alt on US layouts) would open a window menu and its modal loop.
        WM_SYSCOMMAND if wparam.0 & 0xFFF0 == SC_KEYMENU as usize => return LRESULT(0),
        _ => {}
    }
    // SAFETY: default handling for every message.
    unsafe { plain_wndproc(hwnd, msg, wparam, lparam) }
}

fn clear() {
    HOTKEYS.with(|h| h.borrow_mut().clear());
    CHARS.with(|c| c.borrow_mut().clear());
}

/// Presses `keys` together (in order), releases them, and reports whether the hotkey fired and
/// after how long.
fn press(keys: &[u16]) -> (bool, Option<f64>, Vec<u32>) {
    clear();
    let sent = time::now();
    {
        let mut chord = Chord::new();
        for k in &keys[..keys.len() - 1] {
            chord.press(*k);
        }
        chord.tap(keys[keys.len() - 1]);
    }
    mosaik_win::events::pump_until(400, || HOTKEYS.with(|h| !h.borrow().is_empty()));
    mosaik_win::events::pump_until(100, || false);
    let fired = HOTKEYS.with(|h| h.borrow().first().copied());
    let chars = CHARS.with(|c| c.borrow().clone());
    (
        fired.is_some(),
        fired.map(|t| time::micros(sent, t) / 1000.0),
        chars,
    )
}

/// The language half of a layout handle names the language, not the key arrangement (English
/// (Canada) can be the US layout or Canadian Multilingual Standard), so the whole handle is kept.
fn layout_name(hkl: HKL) -> String {
    let raw = hkl.0 as usize & 0xFFFF_FFFF;
    let language = match raw & 0xFFFF {
        0x0409 => "English (US)",
        0x1009 => "English (Canada)",
        0x0C0C => "French (Canada)",
        0x040C => "French (France)",
        0x0407 => "German (Germany)",
        _ => "other language",
    };
    format!("{language} [HKL {raw:08X}]")
}

pub fn run() -> Result<Value, String> {
    let class = WindowClass::register("MosaikProbeHotkey", Some(probe_proc)).map_err(|e| e.to_string())?;
    let hwnd = class
        .create(
            "Mosaïk hotkey probe",
            WS_CAPTION,
            WS_EX_TOPMOST,
            Rect::from_origin_size(60, 60, 360, 120),
            None,
        )
        .map_err(|e| e.to_string())?;
    // SAFETY: our own window.
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    let registration =
        mosaik_win::hotkey::register(Some(hwnd), 1, Hotkey::default_switcher()).map_err(|e| e.to_string())?;

    // 1. Fire it from whatever window is in front.
    let (fired, latency, _) = press(&[LWIN, LMENU, SPACE]);
    println!("Win+Alt+Space fired: {fired} after {latency:?} ms");

    // A process that receives its hotkey may take the foreground: use it to own the keyboard
    // for the layout tests, so no keystroke reaches another app.
    // SAFETY: our own window; read-only check afterwards.
    let foreground = unsafe { SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd };
    let mut layouts = Vec::new();
    if foreground {
        // SAFETY: reads the layout list and this thread's layout; restores it at the end.
        let (original, list) = unsafe {
            let original = GetKeyboardLayout(0);
            let n = usize::try_from(GetKeyboardLayoutList(None)).unwrap_or(0);
            let mut list = vec![HKL::default(); n];
            GetKeyboardLayoutList(Some(&mut list));
            (original, list)
        };
        for hkl in list {
            // SAFETY: activates an installed layout for this thread; undone below.
            let _ = unsafe { ActivateKeyboardLayout(hkl, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)) };
            mosaik_win::events::pump_until(150, || false);
            let left = press(&[LWIN, LMENU, SPACE]);
            let right = press(&[LWIN, RMENU, SPACE]);
            let altgr_space = press(&[RMENU, SPACE]);
            println!(
                "  {:<32} Win+LeftAlt+Space {}  Win+RightAlt+Space {}  RightAlt+Space fired {} typed {:?}",
                layout_name(hkl),
                left.0,
                right.0,
                altgr_space.0,
                altgr_space.2
            );
            layouts.push(json!({
                "layout": layout_name(hkl),
                "win_left_alt_space_fires": left.0,
                "win_right_alt_space_fires": right.0,
                "right_alt_space_fires": altgr_space.0,
                "right_alt_space_typed": altgr_space.2,
            }));
        }
        // SAFETY: restores the layout this thread had before.
        let _ = unsafe { ActivateKeyboardLayout(original, ACTIVATE_KEYBOARD_LAYOUT_FLAGS(0)) };
    } else {
        println!("  could not take the foreground; layout tests skipped so no key reaches another app");
    }
    drop(registration);
    mosaik_win::own::destroy(hwnd);

    // 3. Layout tables, read without installing anything.
    let mut tables = Vec::new();
    for (name, dll) in [
        ("English (US)", "kbdus.dll"),
        ("Canadian Multilingual Standard", "kbdcan.dll"),
        ("Canadian French", "kbdca.dll"),
        ("French (AZERTY)", "kbdfr.dll"),
        ("German (QWERTZ)", "kbdgr.dll"),
    ] {
        match kbd::read(dll) {
            Ok(info) => {
                let letters = info.altgr_letters_and_digits();
                println!(
                    "  {name:<32} AltGr {} | AltGr+Space {:?} | right Ctrl VK {:?} | Space VK {:?} | AltGr letters/digits typing: {}",
                    info.has_altgr,
                    info.altgr_space(),
                    info.right_ctrl_vk.map(|v| format!("{v:#04X}")),
                    info.space_vk.map(|v| format!("{v:#04X}")),
                    letters.len()
                );
                tables.push(json!({
                    "layout": name,
                    "dll": dll,
                    "altgr": info.has_altgr,
                    "altgr_space_types": info.altgr_space().map(|c| format!("U+{:04X}", u32::from(c))),
                    "right_ctrl_vk": info.right_ctrl_vk,
                    "space_vk": info.space_vk,
                    "altgr_letter_digit_keys_that_type": letters.iter().map(|(k, c)| format!("{k}={c}")).collect::<Vec<_>>(),
                }));
            }
            Err(e) => tables.push(json!({ "layout": name, "error": e })),
        }
    }
    Ok(json!({
        "default": Hotkey::default_switcher().to_string(),
        "fires": fired,
        "latency_ms": latency,
        "took_foreground": foreground,
        "installed_layouts": layouts,
        "layout_tables": tables,
    }))
}
