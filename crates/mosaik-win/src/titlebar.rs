//! Reading a window's title bar: what its app answers, point by point, to the question Windows
//! asks wherever the pointer goes (`WM_NCHITTEST`). Where tags and strips then go is decided by
//! `mosaik_core::titlebar`.
//!
//! Each point is asked of the window real input would reach there: the deepest visible, enabled
//! child of the window under that point, passing on to its parent when it answers "transparent",
//! as Windows does. Other windows covering the title bar do not matter. Asking sends a message to
//! the app, which may be busy: it is given up after a moment, and at once for an app that is not
//! responding. This runs on a thread of its own, never the engine's.

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::titlebar::{self, Hit, TitleRoom};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::PhysicalToLogicalPointForPerMonitorDPI;
use windows::Win32::UI::WindowsAndMessaging::{
    CWP_SKIPDISABLED, CWP_SKIPINVISIBLE, CWP_SKIPTRANSPARENT, ChildWindowFromPointEx, GA_PARENT, GetAncestor,
    HTCAPTION, HTTRANSPARENT, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NCHITTEST,
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
}

/// Reads the title bar of `hwnd`. `None` when the window is gone.
pub fn read(hwnd: HWND) -> Option<Reading> {
    let g = window::geometry(hwnd)?;
    let title = window::title(hwnd);
    let room = titlebar::scan(g.frame, g.dpi.max(96), |p| hit(hwnd, p));
    Some(Reading {
        frame: g.frame,
        title,
        room,
    })
}

/// What `root` (a top-level window) answers for the screen point `p`; `None` when it does not
/// answer in time.
fn hit(root: HWND, p: Point) -> Option<Hit> {
    let mut h = deepest_child(root, p);
    loop {
        let code = ask(h, p)?;
        if code == HTTRANSPARENT && h != root {
            // SAFETY: read-only; any handle value is accepted.
            let parent = unsafe { GetAncestor(h, GA_PARENT) };
            if parent.is_invalid() {
                return Some(Hit::Other);
            }
            h = parent;
            continue;
        }
        return Some(if u32::try_from(code) == Ok(HTCAPTION) {
            Hit::Caption
        } else {
            Hit::Other
        });
    }
}

/// The deepest visible, enabled, not see-through child of `root` under `p` (`root` itself
/// outside its client area, where its title bar may be).
fn deepest_child(root: HWND, p: Point) -> HWND {
    let mut h = root;
    for _ in 0..DEPTH {
        let mut pt = POINT { x: p.x, y: p.y };
        // SAFETY: converts a point of our own into the window's client coordinates.
        if !unsafe { ScreenToClient(h, &raw mut pt) }.as_bool() {
            break;
        }
        // SAFETY: read-only lookup among the window's children.
        let child = unsafe {
            ChildWindowFromPointEx(h, pt, CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT)
        };
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
