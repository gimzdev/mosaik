//! Reading a window's title bar: what its app answers, point by point, to the question Windows
//! asks wherever the pointer goes (`WM_NCHITTEST`). Where tags then go is decided by
//! `mosaik_core::titlebar`.
//!
//! Each point is asked of the window real input would reach there: the deepest visible, enabled
//! child of the window under that point, passing on to its parent when it answers "transparent",
//! as Windows does (`mosaik-probe titlebar`, job 0064: the same answers as `WindowFromPoint`'s
//! window, at every point of eight apps' title bars). Other windows covering the title bar do not
//! matter. Asking sends a message to
//! the app, which may be busy: it is given up after a moment, and at once for an app that is not
//! responding. This runs on a thread of its own, never the engine's.
//!
//! The buttons Windows draws on a classic title bar are not asked about: they are where Windows
//! says they are (`DWMWA_CAPTION_BUTTON_BOUNDS`). Measured on Windows 11 (job 0062): asked, a
//! classic window answers "caption" over most of its minimise button, which Windows 11 draws
//! wider than the window's own answers assume.

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::titlebar::{self, Hit, TitleRoom};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CAPTION_BUTTON_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::PhysicalToLogicalPointForPerMonitorDPI;
use windows::Win32::UI::WindowsAndMessaging::{
    CWP_SKIPDISABLED, CWP_SKIPINVISIBLE, ChildWindowFromPointEx, GA_PARENT, GetAncestor, HTCAPTION,
    HTTRANSPARENT, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NCHITTEST,
};

use crate::window;

/// How long one point may take before the app is taken for busy and the reading given up.
const ANSWER_MS: u32 = 150;

/// Child windows deeper than this are not looked into (a guard against a loop in a broken tree).
const DEPTH: usize = 16;

/// A window's title bar as read now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The window's visible frame when it was read.
    pub frame: Rect,
    pub title: String,
    /// `None`: the app did not answer in time.
    pub room: Option<TitleRoom>,
    /// When reading it began, and how long it took.
    pub at: std::time::Instant,
    pub took: std::time::Duration,
}

/// Reads the title bar of `hwnd`. `None` when the window is gone.
pub fn read(hwnd: HWND) -> Option<Reading> {
    let start = std::time::Instant::now();
    let g = window::geometry(hwnd)?;
    let title = window::title(hwnd);
    let buttons = caption_buttons(hwnd);
    // Pixels on screen are the monitor's, also for a window that is not DPI aware.
    let dpi = crate::monitors::dpi_of_monitor_of(hwnd);
    let room = titlebar::scan(g.frame, dpi, |p| {
        if buttons.is_some_and(|b| b.contains(p)) {
            return Some(Hit::Other);
        }
        hit(hwnd, p)
    });
    Some(Reading {
        frame: g.frame,
        title,
        room,
        at: start,
        took: start.elapsed(),
    })
}

/// Where the buttons Windows draws in `hwnd`'s title bar are, on screen; `None` when Windows
/// draws none there (apps that draw their own title bar).
pub fn caption_buttons(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    // SAFETY: the attribute is a RECT and the buffer is exactly one RECT.
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_BUTTON_BOUNDS,
            (&raw mut r).cast::<std::ffi::c_void>(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap_or(0),
        )
    }
    .ok()?;
    // Relative to the window's top-left corner; meaningless while it is minimised or hidden.
    let window = window::window_rect(hwnd)?;
    let buttons = Rect::new(r.left, r.top, r.right, r.bottom).offset(window.left, window.top);
    (!buttons.is_empty() && window.contains_rect(buttons)).then_some(buttons)
}

/// What `root` (a top-level window) answers for the screen point `p`; `None` when it does not
/// answer in time.
fn hit(root: HWND, p: Point) -> Option<Hit> {
    let code = hit_code(root, p)?;
    Some(if u32::try_from(code) == Ok(HTCAPTION) {
        Hit::Caption
    } else {
        Hit::Other
    })
}

/// The `WM_NCHITTEST` code `root` (a top-level window) answers for the screen point `p`, asked
/// as Mosaïk asks it when reading title bars; `None` when it does not answer in time.
pub fn hit_code(root: HWND, p: Point) -> Option<i32> {
    let mut h = deepest_child(root, p);
    loop {
        let code = ask(h, p)?;
        if code == HTTRANSPARENT && h != root {
            // SAFETY: read-only; any handle value is accepted.
            let parent = unsafe { GetAncestor(h, GA_PARENT) };
            if parent.is_invalid() {
                return Some(code);
            }
            h = parent;
            continue;
        }
        return Some(code);
    }
}

/// The deepest visible, enabled child of `root` under `p` (`root` itself outside its client
/// area, where its title bar may be): the window a point is asked of first.
pub fn deepest_child(root: HWND, p: Point) -> HWND {
    let mut h = root;
    for _ in 0..DEPTH {
        let mut pt = POINT { x: p.x, y: p.y };
        // SAFETY: converts a point of our own into the window's client coordinates.
        if !unsafe { ScreenToClient(h, &raw mut pt) }.as_bool() {
            break;
        }
        // SAFETY: read-only lookup among the window's children.
        let child = unsafe { ChildWindowFromPointEx(h, pt, CWP_SKIPINVISIBLE | CWP_SKIPDISABLED) };
        if child.is_invalid() || child == h {
            break;
        }
        h = child;
    }
    h
}

/// Sends `WM_NCHITTEST` for the screen point `p` to `h`. A window that is not per-monitor DPI
/// aware gets the point in its own scaled coordinates, as Windows gives it real input.
fn ask(h: HWND, p: Point) -> Option<i32> {
    let mut pt = POINT { x: p.x, y: p.y };
    // SAFETY: converts a point of our own; it is left as it is for a per-monitor aware window,
    // and when the conversion fails.
    let _ = unsafe { PhysicalToLogicalPointForPerMonitorDPI(Some(h), &raw mut pt) };
    // Screen coordinates as MAKELPARAM packs them: x in the low word, y in the high word, each
    // read back as a signed 16-bit value (so points left of or above the primary monitor work).
    let word = |v: i32| isize::try_from(v & 0xFFFF).unwrap_or_default();
    let lparam = LPARAM((word(pt.y) << 16) | word(pt.x));
    let mut answer = 0usize;
    // SAFETY: WM_NCHITTEST carries no pointer; the answer is written to a local.
    let sent = unsafe {
        SendMessageTimeoutW(
            h,
            WM_NCHITTEST,
            WPARAM(0),
            lparam,
            SMTO_ABORTIFHUNG,
            ANSWER_MS,
            Some(&raw mut answer),
        )
    };
    if sent.0 == 0 {
        return None;
    }
    // The answer is a small signed code (HTTRANSPARENT is -1) in a pointer-sized result.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let code = answer as isize as i32;
    Some(code)
}
