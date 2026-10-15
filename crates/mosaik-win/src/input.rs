//! Synthetic keyboard and mouse input, for the probes and integration tests that act like a
//! user. Every key pressed through a [`Chord`] is released when it drops, even on panic, so a
//! failed test can never leave Win or Alt stuck down.

use mosaik_core::geometry::Point;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

fn send(inputs: &[INPUT]) -> bool {
    let size = i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(0);
    // SAFETY: a slice of fully initialised INPUT structures with the right size.
    let sent = unsafe { SendInput(inputs, size) };
    usize::try_from(sent).is_ok_and(|n| n == inputs.len())
}

/// Presses or releases one key by its scan code, as a real keyboard does, so the active layout
/// translates it (this is what makes Right Alt act as AltGr on AltGr layouts).
pub fn key(vk: u16, down: bool) -> bool {
    // SAFETY: pure translation with the current keyboard layout.
    let ex = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC_EX) };
    let scan = u16::try_from(ex & 0xFF).unwrap_or(0);
    let mut flags = KEYEVENTF_SCANCODE;
    if ex & 0xFF00 == 0xE000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    send(&[INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: KEYBD_EVENT_FLAGS(flags.0),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }])
}

/// Keys held down together; released in reverse order when dropped.
#[derive(Default)]
pub struct Chord {
    held: Vec<u16>,
}

impl Chord {
    pub fn new() -> Self {
        Self::default()
    }

    /// Presses `vk` and keeps it held.
    pub fn press(&mut self, vk: u16) -> &mut Self {
        if key(vk, true) {
            self.held.push(vk);
        }
        self
    }

    /// Presses and releases `vk` while the held keys stay down.
    pub fn tap(&mut self, vk: u16) -> &mut Self {
        key(vk, true);
        key(vk, false);
        self
    }

    pub fn release_all(&mut self) {
        while let Some(vk) = self.held.pop() {
            key(vk, false);
        }
    }
}

impl Drop for Chord {
    fn drop(&mut self) {
        self.release_all();
    }
}

pub fn cursor() -> Point {
    let mut p = POINT::default();
    // SAFETY: valid out pointer.
    let _ = unsafe { GetCursorPos(&mut p) };
    Point::new(p.x, p.y)
}

/// The pointer as Windows shows it now, if it can be read.
fn cursor_info() -> Option<windows::Win32::UI::WindowsAndMessaging::CURSORINFO> {
    use windows::Win32::UI::WindowsAndMessaging::{CURSORINFO, GetCursorInfo};
    let mut info = CURSORINFO {
        cbSize: u32::try_from(std::mem::size_of::<CURSORINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: fills a sized structure about the pointer.
    unsafe { GetCursorInfo(&raw mut info) }.ok().map(|()| info)
}

/// The pointer is hidden now, over the window it is on: the app hid it (`ShowCursor`), or gave
/// it no shape (`SetCursor(NULL)`), as games do while the mouse turns the view.
pub fn pointer_hidden() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::CURSOR_SHOWING;
    cursor_info().is_some_and(|i| i.flags.0 & CURSOR_SHOWING.0 == 0 || i.hCursor.is_invalid())
}

/// An app keeps the pointer inside part of the screens, as a game does while the mouse turns
/// the view.
pub fn pointer_confined() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClipCursor;
    let mut clip = RECT::default();
    // SAFETY: reads the rectangle the pointer is confined to (the whole desktop when none).
    unsafe { GetClipCursor(&raw mut clip) }.is_ok() && {
        // SAFETY: reads system metrics.
        let (x, y, w, h) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        clip.left > x || clip.top > y || clip.right < x + w || clip.bottom < y + h
    }
}

/// The primary mouse button is held down now: the left one, or the right one when the user
/// swapped them (the state is read from the physical buttons).
pub fn primary_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
    // SAFETY: reads a system setting.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let vk = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    // SAFETY: reads the state of one button; the top bit is set while it is down.
    let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
    state < 0
}

/// Shift is held down now (either one): it inverts snapping while a window is dragged.
pub fn shift_down() -> bool {
    key_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT)
}

/// Escape is held down now: it cancels a press on a tab strip or a window's band.
pub fn escape_down() -> bool {
    key_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE)
}

fn key_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // SAFETY: reads the state of one key; the top bit is set while it is down.
    let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
    state < 0
}

/// The pointer's shape, as far as it tells a move from a resize: Windows shows one of its
/// resize arrows for the whole of a resize, whichever app the window belongs to.
pub fn pointer() -> mosaik_core::snap::Pointer {
    use mosaik_core::snap::Pointer;
    use windows::Win32::UI::WindowsAndMessaging::{
        CURSOR_SHOWING, CURSORINFO, GetCursorInfo, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
        LoadCursorW,
    };
    let mut info = CURSORINFO {
        cbSize: u32::try_from(std::mem::size_of::<CURSORINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: fills a sized CURSORINFO.
    if unsafe { GetCursorInfo(&raw mut info) }.is_err() || info.flags.0 & CURSOR_SHOWING.0 == 0 {
        return Pointer::Hidden;
    }
    let arrow = [IDC_SIZEWE, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZENESW]
        .into_iter()
        .any(|id| {
            // SAFETY: returns Windows' shared cursor for `id`, which is never destroyed.
            unsafe { LoadCursorW(None, id) }.is_ok_and(|c| c == info.hCursor)
        });
    if arrow {
        Pointer::ResizeArrow
    } else {
        Pointer::Other
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32) -> bool {
    send(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }])
}

/// Moves the cursor to `p` (virtual-screen pixels).
pub fn move_to(p: Point) -> bool {
    // SAFETY: GetSystemMetrics has no preconditions.
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    // Absolute coordinates are 0..65535 across the virtual desktop.
    let nx = (i64::from(p.x - x) * 65535 + i64::from(w.max(2) - 1) / 2) / i64::from(w.max(2) - 1);
    let ny = (i64::from(p.y - y) * 65535 + i64::from(h.max(2) - 1) / 2) / i64::from(h.max(2) - 1);
    mouse(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        i32::try_from(nx).unwrap_or(0),
        i32::try_from(ny).unwrap_or(0),
    )
}

pub fn left_button(down: bool) -> bool {
    mouse(
        if down {
            MOUSEEVENTF_LEFTDOWN
        } else {
            MOUSEEVENTF_LEFTUP
        },
        0,
        0,
    )
}

/// Holds the left button down; releases it when dropped.
pub struct LeftButton(());

impl LeftButton {
    pub fn press() -> Self {
        left_button(true);
        Self(())
    }
}

impl Drop for LeftButton {
    fn drop(&mut self) {
        left_button(false);
    }
}
