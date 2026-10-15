//! The workspace switcher (ADR 0002): a native window on the engine thread. It opens in the
//! next frame after the hotkey and costs nothing while closed, where a web view kept ready
//! for it cost about 150 MB.
//!
//! It is drawn with Direct2D around a real Windows list box, so screen readers read each
//! workspace (name, counts, its key) as the selection moves, and the keyboard works as in any
//! list. Keys are read before the list box sees them: the switcher's own keys (numbers, S,
//! typing to filter) must not turn into the list box's type-ahead.

mod paint;

use std::cell::RefCell;
use std::time::Instant;

use mosaik_core::layout::WorkspaceId;
use mosaik_core::settings::Theme as ThemeSetting;
use mosaik_core::switcher::{Action, Entry, Key, Model};
use mosaik_win::own::{WindowClass, destroy};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, GetMonitorInfoW, GetSysColor, HBRUSH, HGDIOBJ,
    InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, PAINTSTRUCT, SYS_COLOR_INDEX,
    UpdateWindow,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_SELECTED};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, VIRTUAL_KEY, VK_BACK, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F24, VK_HOME, VK_RETURN, VK_TAB,
    VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, GetClientRect, GetCursorPos, HMENU, HWND_TOPMOST,
    LB_ADDSTRING, LB_RESETCONTENT, LB_SETCURSEL, LB_SETITEMHEIGHT, LBN_SELCHANGE, LBS_HASSTRINGS,
    LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, LBS_OWNERDRAWFIXED, MSG, PostMessageW, SPI_GETHIGHCONTRAST, SW_HIDE,
    SW_SHOW, SWP_NOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SetForegroundWindow,
    SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, TranslateMessage, WA_INACTIVE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CTLCOLORLISTBOX,
    WM_DRAWITEM, WM_ERASEBKGND, WM_KEYDOWN, WM_PAINT, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

use paint::{Painter, Rgba, Row, Theme, size};

thread_local! {
    static SWITCHER: RefCell<Option<Switcher>> = const { RefCell::new(None) };
    /// What the user chose, for the engine to act on when it next wakes.
    static CHOSEN: RefCell<Vec<Choice>> = const { RefCell::new(Vec::new()) };
}

/// What the switcher asks the engine to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Open(WorkspaceId),
    /// Save the windows on screen under this name.
    Save(String),
}

struct Switcher {
    window: HWND,
    list: HWND,
    /// The engine's window, woken when the user chooses something.
    engine: HWND,
    painter: Painter,
    theme: Theme,
    theme_setting: ThemeSetting,
    panel_brush: HBRUSH,
    model: Model,
    shortcut: String,
    dpi: u32,
    /// The first half of a character outside the basic plane, waiting for its second half.
    high_surrogate: Option<u16>,
}

/// Creates the (hidden) switcher. Called once on the engine thread.
pub(crate) fn create(engine: HWND, theme_setting: ThemeSetting) -> Result<(), String> {
    let class = WindowClass::register_with("MosaikSwitcher", Some(switcher_proc), CS_DROPSHADOW)
        .map_err(|e| e.to_string())?;
    let window = class
        .create(
            "Mosaïk switcher",
            WS_POPUP | WS_CLIPCHILDREN,
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            mosaik_core::geometry::Rect::from_origin_size(0, 0, 480, 400),
            None,
        )
        .map_err(|e| e.to_string())?;
    // SAFETY: creates a standard list box as a child of our own window.
    let list = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("LISTBOX"),
            w!("Workspaces"),
            WS_CHILD
                | WS_VISIBLE
                | WINDOW_STYLE(
                    (LBS_OWNERDRAWFIXED | LBS_HASSTRINGS | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT).cast_unsigned(),
                ),
            8,
            52,
            464,
            300,
            Some(window),
            Some(HMENU(std::ptr::dangling_mut())),
            None,
            None,
        )
    }
    .map_err(|e| format!("the switcher's list could not be created: {e}"))?;
    let painter = Painter::new(96).map_err(|e| format!("the switcher cannot draw: {e}"))?;
    // SAFETY: DWM attributes on our own window; failure (Windows 10) only loses round corners.
    unsafe {
        let round = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const round).cast(),
            4,
        );
    }
    let theme = theme_for(theme_setting);
    let switcher = Switcher {
        window,
        list,
        engine,
        painter,
        theme,
        theme_setting,
        panel_brush: brush(theme.panel),
        model: Model::default(),
        shortcut: String::new(),
        dpi: 96,
        high_surrogate: None,
    };
    SWITCHER.with(|s| *s.borrow_mut() = Some(switcher));
    Ok(())
}

/// Pixels from a scaled length: window sizes, far inside `i32`.
#[allow(clippy::cast_possible_truncation)]
fn px(v: f32) -> i32 {
    v.round() as i32
}

/// The low and high 16 bits of a message parameter.
fn loword(v: usize) -> u32 {
    u32::try_from(v & 0xFFFF).unwrap_or(0)
}

fn hiword(v: usize) -> u32 {
    u32::try_from((v >> 16) & 0xFFFF).unwrap_or(0)
}

fn brush(c: Rgba) -> HBRUSH {
    // SAFETY: a GDI brush owned by the switcher, deleted when replaced or destroyed.
    unsafe { CreateSolidBrush(COLORREF(c.colorref())) }
}

/// Light or dark from the setting (or from Windows), or Windows' own colours in high contrast.
fn theme_for(setting: ThemeSetting) -> Theme {
    let mut hc = HIGHCONTRASTW {
        cbSize: u32::try_from(std::mem::size_of::<HIGHCONTRASTW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: SPI_GETHIGHCONTRAST fills a sized HIGHCONTRASTW.
    let high_contrast = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some((&raw mut hc).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok()
        && hc.dwFlags.contains(HCF_HIGHCONTRASTON);
    if high_contrast {
        let sys = |i: i32| {
            // SAFETY: GetSysColor has no preconditions.
            let c = unsafe { GetSysColor(SYS_COLOR_INDEX(i)) };
            Rgba(
                (c & 0xFF) as f32 / 255.0,
                ((c >> 8) & 0xFF) as f32 / 255.0,
                ((c >> 16) & 0xFF) as f32 / 255.0,
                1.0,
            )
        };
        // COLOR_WINDOW 5, COLOR_WINDOWTEXT 8, COLOR_HIGHLIGHT 13, COLOR_HIGHLIGHTTEXT 14,
        // COLOR_GRAYTEXT 17.
        return Theme {
            panel: sys(5),
            text: sys(8),
            muted: sys(8),
            line: sys(8),
            selection: sys(13),
            selection_text: sys(14),
            accent: sys(14),
            thumb: sys(5),
            high_contrast: true,
        };
    }
    let light = match setting {
        ThemeSetting::Light => true,
        ThemeSetting::Dark => false,
        ThemeSetting::System => mosaik_win::os::apps_use_light_theme(),
    };
    if light { Theme::LIGHT } else { Theme::DARK }
}

/// Opens the switcher on the monitor with the pointer, in the current theme (read on every open,
/// so a change of light, dark or high contrast is picked up). `asked` is when the hotkey fired,
/// for the log line that says how long the switcher took to be on screen.
pub(crate) fn open(entries: Vec<Entry>, active: Option<WorkspaceId>, shortcut: String, asked: Instant) {
    let window = SWITCHER.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        let s = slot.as_mut()?;
        s.model = Model::new(entries, active);
        s.shortcut = shortcut;
        s.high_surrogate = None;
        let theme = theme_for(s.theme_setting);
        if theme != s.theme {
            s.theme = theme;
            // SAFETY: the old brush is ours and no longer selected anywhere.
            let _ = unsafe { DeleteObject(HGDIOBJ(s.panel_brush.0)) };
            s.panel_brush = brush(theme.panel);
        }
        Some(s.window)
    });
    let Some(window) = window else { return };
    place(window);
    refill();
    // SAFETY: shows our own window and gives it the keyboard; this process just received the
    // hotkey, so Windows lets it come to the front.
    unsafe {
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
        let list = SWITCHER.with(|c| c.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.list)));
        if let Some(list) = list {
            let _ = SetFocus(Some(list));
        }
        let _ = UpdateWindow(window);
    }
    // Not waiting for the screen to show it (`DwmFlush`): that held the engine a frame (13-17
    // ms) after each opening, with the user's first key waiting behind it, only for this line.
    tracing::info!(
        painted_ms = format!("{:.1}", asked.elapsed().as_secs_f64() * 1000.0),
        "switcher open"
    );
}

/// The theme the switcher follows from now on (the user changed it in Settings).
pub(crate) fn set_theme(setting: ThemeSetting) {
    SWITCHER.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut()
            && let Some(s) = slot.as_mut()
        {
            s.theme_setting = setting;
        }
    });
}

pub(crate) fn close() {
    let window = SWITCHER.with(|c| c.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.window)));
    if let Some(w) = window {
        // SAFETY: hides our own window.
        let _ = unsafe { ShowWindow(w, SW_HIDE) };
    }
}

pub(crate) fn take_choices() -> Vec<Choice> {
    CHOSEN.with(|c| c.borrow_mut().drain(..).collect())
}

pub(crate) fn destroy_window() {
    if let Some(s) = SWITCHER.with(|c| c.borrow_mut().take()) {
        // SAFETY: the brush is ours; the windows were created on this thread.
        let _ = unsafe { DeleteObject(HGDIOBJ(s.panel_brush.0)) };
        destroy(s.window);
    }
}

/// Size and place the window for the number of rows, on the pointer's monitor, at its DPI.
fn place(window: HWND) {
    let mut cursor = POINT::default();
    // SAFETY: reads the pointer position into a local.
    let _ = unsafe { GetCursorPos(&mut cursor) };
    // SAFETY: monitor queries with sized structs and locals.
    let (work, dpi) = unsafe {
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).unwrap_or(0),
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let (mut x, mut y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        (info.rcWork, x)
    };
    let geometry = SWITCHER.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        let s = slot.as_mut()?;
        if s.painter.set_dpi(dpi).is_err() {
            tracing::warn!(dpi, "switcher text not rescaled");
        }
        s.dpi = dpi;
        let k = dpi as f32 / 96.0;
        let rows = if s.model.naming().is_some() {
            1
        } else {
            s.model.shown().len()
        };
        let list_h = if rows == 0 {
            size::EMPTY * k
        } else {
            rows.min(size::MAX_ROWS) as f32 * size::ROW * k + 2.0 * size::PAD * k
        };
        let w = px(size::WIDTH * k);
        let h = px(size::HEADER * k + list_h + size::FOOTER * k);
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.top + (work.bottom - work.top) * 22 / 100;
        let pad = px(size::PAD * k);
        let list_rect = (
            pad,
            px(size::HEADER * k) + pad,
            w - 2 * pad,
            px(list_h) - 2 * pad,
            rows > 0,
        );
        Some((s.list, (x, y, w, h), list_rect, px(size::ROW * k)))
    });
    let Some((list, (x, y, w, h), (lx, ly, lw, lh, visible), row)) = geometry else {
        return;
    };
    // SAFETY: moves and sizes our own windows; messages they send are handled with the
    // switcher state not borrowed.
    unsafe {
        let _ = SetWindowPos(window, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        let _ = SetWindowPos(list, None, lx, ly, lw, lh, SWP_NOACTIVATE);
        let _ = ShowWindow(list, if visible { SW_SHOW } else { SW_HIDE });
        SendMessageW(
            list,
            LB_SETITEMHEIGHT,
            Some(WPARAM(0)),
            Some(LPARAM(row as isize)),
        );
    }
}

/// Puts the shown entries in the list box (as what a screen reader says for each) and selects
/// the model's selection.
fn refill() {
    let data = SWITCHER.with(|cell| {
        let slot = cell.try_borrow().ok()?;
        let s = slot.as_ref()?;
        if let Some(n) = s.model.naming() {
            let what = match s.model.replaces(&n.name) {
                Some(e) => format!("updates {}", e.name),
                None => "new workspace".to_owned(),
            };
            let spoken = vec![format!(
                "Save as workspace: {}. {what}. Type a name, Enter to save, Escape to go back.",
                if n.name.is_empty() {
                    "no name yet"
                } else {
                    n.name.as_str()
                }
            )];
            return Some((s.list, s.window, spoken, "Save as workspace".to_owned(), Some(0)));
        }
        let spoken: Vec<String> = (0..s.model.shown().len())
            .filter_map(|i| s.model.spoken(i))
            .collect();
        let name = if s.model.is_empty() {
            "No workspaces yet. Arrange your windows, then press S to save them as a workspace.".to_owned()
        } else if s.model.shown().is_empty() {
            format!(
                "No workspace matches \u{201c}{}\u{201d}. Press Escape to clear the filter.",
                s.model.filter()
            )
        } else if s.model.filter().is_empty() {
            "Workspaces".to_owned()
        } else {
            format!("Workspaces matching \u{201c}{}\u{201d}", s.model.filter())
        };
        Some((s.list, s.window, spoken, name, s.model.selected()))
    });
    let Some((list, window, spoken, name, selected)) = data else {
        return;
    };
    let name = mosaik_win::text::wide(&name);
    // SAFETY: list box messages to our own child; strings outlive each call.
    unsafe {
        SendMessageW(list, LB_RESETCONTENT, None, None);
        for s in &spoken {
            let text = mosaik_win::text::wide(s);
            SendMessageW(list, LB_ADDSTRING, None, Some(LPARAM(text.as_ptr() as isize)));
        }
        let index = selected.map_or(-1isize, |i| i as isize);
        SendMessageW(list, LB_SETCURSEL, Some(WPARAM(index.cast_unsigned())), None);
        let _ = SetWindowTextW(list, PCWSTR(name.as_ptr()));
        let _ = InvalidateRect(Some(window), None, true);
    }
}

fn key_for(vk: VIRTUAL_KEY) -> Option<Key> {
    Some(match vk {
        VK_UP => Key::Up,
        VK_DOWN => Key::Down,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_RETURN => Key::Enter,
        VK_ESCAPE => Key::Escape,
        VK_BACK => Key::Backspace,
        k if (VK_F1.0..=VK_F24.0).contains(&k.0) => Key::Function(u8::try_from(k.0 - VK_F1.0 + 1).ok()?),
        _ => return None,
    })
}

/// Reads keys meant for the switcher before Windows hands them to the list box. Returns true
/// when the message was used here and must not be dispatched.
pub(crate) fn translate(msg: &MSG) -> bool {
    let ours = SWITCHER.with(|c| {
        c.try_borrow()
            .ok()
            .and_then(|s| s.as_ref().map(|s| msg.hwnd == s.list || msg.hwnd == s.window))
            .unwrap_or(false)
    });
    if !ours {
        return false;
    }
    match msg.message {
        WM_KEYDOWN => {
            let vk = VIRTUAL_KEY(u16::try_from(msg.wParam.0).unwrap_or(0));
            if vk == VK_TAB {
                return true;
            }
            match key_for(vk) {
                Some(key) => act(key),
                None => {
                    // Let Windows turn the key into a character (WM_CHAR, handled below), but
                    // keep it from the list box's own type-ahead.
                    // SAFETY: translates a message from this thread's queue.
                    let _ = unsafe { TranslateMessage(msg) };
                }
            }
            true
        }
        WM_CHAR => {
            let unit = u16::try_from(msg.wParam.0).unwrap_or(0);
            let ch = SWITCHER.with(|c| {
                let mut slot = c.try_borrow_mut().ok()?;
                let s = slot.as_mut()?;
                if (0xD800..0xDC00).contains(&unit) {
                    s.high_surrogate = Some(unit);
                    return None;
                }
                let units = match s.high_surrogate.take() {
                    Some(high) => vec![high, unit],
                    None => vec![unit],
                };
                char::decode_utf16(units).next()?.ok()
            });
            if let Some(c) = ch.filter(|c| !c.is_control()) {
                act(Key::Char(c));
            }
            true
        }
        _ => false,
    }
}

fn act(key: Key) {
    let action = SWITCHER.with(|c| {
        c.try_borrow_mut()
            .ok()
            .and_then(|mut s| s.as_mut().map(|s| s.model.key(key)))
    });
    match action {
        Some(Action::Changed) => {
            let window = SWITCHER.with(|c| c.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.window)));
            if let Some(w) = window {
                place(w);
            }
            refill();
        }
        Some(Action::Close) => close(),
        Some(Action::Open(id)) => choose(Choice::Open(id)),
        Some(Action::Save(name)) => choose(Choice::Save(name)),
        Some(Action::None) | None => {}
    }
}

/// Closes the switcher and wakes the engine to act on the choice.
fn choose(choice: Choice) {
    close();
    CHOSEN.with(|c| c.borrow_mut().push(choice));
    let engine = SWITCHER.with(|c| c.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.engine)));
    if let Some(e) = engine {
        // SAFETY: posts to the engine's own window on this thread.
        let _ = unsafe { PostMessageW(Some(e), crate::WM_WAKE, WPARAM(0), LPARAM(0)) };
    }
}

unsafe extern "system" fn switcher_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: standard paint cycle on our own window.
            let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
            let mut client = RECT::default();
            // SAFETY: reads our own window's client area.
            let _ = unsafe { GetClientRect(hwnd, &mut client) };
            SWITCHER.with(|cell| {
                if let Ok(mut slot) = cell.try_borrow_mut()
                    && let Some(s) = slot.as_mut()
                {
                    let r = s.painter.window(hdc, client, &s.theme, &s.model, &s.shortcut);
                    if r.is_err() && s.painter.recreate().is_ok() {
                        let _ = s.painter.window(hdc, client, &s.theme, &s.model, &s.shortcut);
                    }
                }
            });
            // SAFETY: ends the paint cycle begun above.
            let _ = unsafe { EndPaint(hwnd, &ps) };
            LRESULT(0)
        }
        WM_DRAWITEM => {
            // SAFETY: for WM_DRAWITEM, lParam points to a DRAWITEMSTRUCT valid for this call.
            let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
            SWITCHER.with(|cell| {
                if let Ok(mut slot) = cell.try_borrow_mut()
                    && let Some(s) = slot.as_mut()
                    && let Some(n) = s.model.naming().cloned()
                {
                    let replaces = s.model.replaces(&n.name).map(|e| e.name.clone());
                    let r = s.painter.naming_row(
                        item.hDC,
                        item.rcItem,
                        &s.theme,
                        &n.name,
                        n.selected,
                        replaces.as_deref(),
                    );
                    if r.is_err() && s.painter.recreate().is_ok() {
                        let _ = s.painter.naming_row(
                            item.hDC,
                            item.rcItem,
                            &s.theme,
                            &n.name,
                            n.selected,
                            replaces.as_deref(),
                        );
                    }
                    return;
                }
                if let Ok(mut slot) = cell.try_borrow_mut()
                    && let Some(s) = slot.as_mut()
                    && let Some(entry) = s.model.shown().get(item.itemID as usize).map(|e| (*e).clone())
                {
                    let index = item.itemID as usize;
                    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
                    let current = s.model.active() == Some(&entry.id);
                    let key = s.model.key_for(index);
                    let row = Row {
                        entry: &entry,
                        current,
                        selected,
                        key: key.as_deref(),
                    };
                    let r = s.painter.row(item.hDC, item.rcItem, &s.theme, &row);
                    if r.is_err() && s.painter.recreate().is_ok() {
                        let _ = s.painter.row(item.hDC, item.rcItem, &s.theme, &row);
                    }
                }
            });
            LRESULT(1)
        }
        WM_CTLCOLORLISTBOX => {
            let brush = SWITCHER.with(|c| {
                c.try_borrow()
                    .ok()
                    .and_then(|s| s.as_ref().map(|s| s.panel_brush))
            });
            LRESULT(brush.map_or(0, |b| b.0 as isize))
        }
        WM_COMMAND => {
            // A click in the list (the keyboard never changes its selection itself).
            if hiword(wparam.0) == LBN_SELCHANGE {
                let chosen = SWITCHER.with(|cell| {
                    let slot = cell.try_borrow().ok()?;
                    let s = slot.as_ref()?;
                    if s.model.naming().is_some() {
                        return None;
                    }
                    // SAFETY: asks our own list box for its selection.
                    let index = unsafe {
                        SendMessageW(
                            s.list,
                            windows::Win32::UI::WindowsAndMessaging::LB_GETCURSEL,
                            None,
                            None,
                        )
                    };
                    let index = usize::try_from(index.0).ok()?;
                    s.model.shown().get(index).map(|e| e.id.clone())
                });
                if let Some(id) = chosen {
                    choose(Choice::Open(id));
                }
            }
            LRESULT(0)
        }
        // Another window took the focus: the switcher goes, as with Esc.
        WM_ACTIVATE if loword(wparam.0) == WA_INACTIVE => {
            close();
            LRESULT(0)
        }
        WM_CLOSE => {
            close();
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
