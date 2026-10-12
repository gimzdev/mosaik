//! The quick menu's handle: a thin line at an edge of the main monitor that opens, when the
//! pointer rests on it, onto a short menu (Arrange, the switcher, saving the layout...).
//!
//! It is a layered window that takes the mouse but never the keyboard focus, so whatever the
//! user was typing in keeps it. Every pixel of it is at least faintly opaque, so the pointer
//! anywhere over it counts as over it (fully transparent pixels of a layered window let the
//! mouse through). What the mouse does is queued for the engine ([`take_inputs`]), which decides
//! what happens and draws the handle or the menu again; Windows' own hover tracking says when
//! the pointer has rested on the handle.

use std::cell::{Cell, RefCell};

use mosaik_core::geometry::{Point, Rect};
use mosaik_win::error::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, GetSysColor, SYS_COLOR_INDEX};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, MA_NOACTIVATE, PostMessageW, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
};

use crate::canvas::{Align, Area, Canvas, Rgba, Weight};
use crate::layered::LayeredWindow;

/// `WM_MOUSEHOVER` and `WM_MOUSELEAVE` (`TrackMouseEvent`'s messages; the `windows` crate keeps
/// them with the common controls).
const WM_MOUSEHOVER: u32 = 0x02A1;
const WM_MOUSELEAVE: u32 = 0x02A3;

/// How long the pointer rests on the handle before the menu opens, in milliseconds: long enough
/// that passing by (on the way to a scroll bar at the screen's edge) does not open it.
pub const HOVER_MS: u32 = 300;

/// What the mouse did on the handle or the menu, in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeInput {
    /// The pointer is over it (sent on every move).
    Move(Point),
    /// The pointer rested on it.
    Hover,
    /// The pointer left it.
    Leave,
    Down(Point),
    Up(Point),
    /// The mouse was taken away during a press (a dialog, Esc): the press is off.
    Cancel,
}

thread_local! {
    static INPUTS: RefCell<Vec<EdgeInput>> = const { RefCell::new(Vec::new()) };
    static WAKE: Cell<Option<(isize, u32)>> = const { Cell::new(None) };
    /// Windows is tracking the pointer over the handle (for the hover and the leave).
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static PRESSED: Cell<bool> = const { Cell::new(false) };
}

/// The handle posts `message` to `window` when the mouse did something, for the engine to read
/// with [`take_inputs`].
pub fn wake_on_input(window: HWND, message: u32) {
    WAKE.with(|w| w.set(Some((window.0 as isize, message))));
}

pub fn take_inputs() -> Vec<EdgeInput> {
    INPUTS.with(|i| {
        i.try_borrow_mut()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    })
}

fn push(input: EdgeInput) {
    INPUTS.with(|i| {
        if let Ok(mut v) = i.try_borrow_mut() {
            // Moves only matter where the pointer is now.
            if let (Some(EdgeInput::Move(_)), EdgeInput::Move(_)) = (v.last(), input) {
                v.pop();
            }
            v.push(input);
        }
    });
    if let Some((window, message)) = WAKE.with(Cell::get) {
        // SAFETY: posts to the engine's window on this thread.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(window as *mut core::ffi::c_void)),
                message,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }
}

fn screen_point(hwnd: HWND, lparam: LPARAM) -> Point {
    // The low and high words are signed 16-bit client coordinates.
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (i32::from(lparam.0 as i16), i32::from((lparam.0 >> 16) as i16));
    let mut p = POINT { x, y };
    // SAFETY: converts a point of our own window's client area.
    let _ = unsafe { ClientToScreen(hwnd, &mut p) };
    Point::new(p.x, p.y)
}

fn track(hwnd: HWND) {
    if TRACKING.with(Cell::get) {
        return;
    }
    let mut t = TRACKMOUSEEVENT {
        cbSize: u32::try_from(std::mem::size_of::<TRACKMOUSEEVENT>()).unwrap_or(0),
        dwFlags: TME_HOVER | TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: HOVER_MS,
    };
    // SAFETY: a sized TRACKMOUSEEVENT for our own window.
    if unsafe { TrackMouseEvent(&mut t) }.is_ok() {
        TRACKING.with(|c| c.set(true));
    }
}

unsafe extern "system" fn edge_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            track(hwnd);
            push(EdgeInput::Move(screen_point(hwnd, lparam)));
        }
        WM_MOUSEHOVER => push(EdgeInput::Hover),
        WM_MOUSELEAVE => {
            TRACKING.with(|c| c.set(false));
            push(EdgeInput::Leave);
        }
        WM_LBUTTONDOWN => {
            // SAFETY: our own window keeps the mouse until the button goes up.
            unsafe { SetCapture(hwnd) };
            PRESSED.with(|p| p.set(true));
            push(EdgeInput::Down(screen_point(hwnd, lparam)));
        }
        WM_LBUTTONUP => {
            if PRESSED.with(Cell::take) {
                push(EdgeInput::Up(screen_point(hwnd, lparam)));
            }
            // SAFETY: gives the mouse back.
            let _ = unsafe { ReleaseCapture() };
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            if PRESSED.with(Cell::take) {
                push(EdgeInput::Cancel);
            }
        }
        // It never takes the keyboard from the app the user is in.
        WM_MOUSEACTIVATE => return LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        // It closes with Mosaïk, not when another program asks every window to close.
        WM_CLOSE => return LRESULT(0),
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}

/// The edge of the monitor the handle is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// Where the handle is: an edge of a monitor's work area, its middle at `middle` (a screen y).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub work: Rect,
    pub side: Side,
    pub middle: i32,
    pub dpi: u32,
    /// The pointer goes on past that edge (another monitor, or the taskbar, is there), so it
    /// does not stop on the handle by itself: the handle takes a wider strip.
    pub through: bool,
}

/// The colours of the handle and the menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub panel: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub line: Rgba,
    pub hover: Rgba,
    pub hover_text: Rgba,
    pub accent: Rgba,
    /// The handle's line, and the hairline around it that keeps it visible on any background.
    pub handle: Rgba,
    pub handle_edge: Rgba,
    pub shadow: bool,
}

fn hex(rgb: u32, alpha: f32) -> Rgba {
    // Each channel is 0..=255, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let c = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    Rgba(c(16), c(8), c(0), alpha)
}

impl Palette {
    /// Paper and ink, as the switcher.
    pub fn light() -> Self {
        Self {
            panel: hex(0xF7F8F4, 1.0),
            text: hex(0x161B25, 1.0),
            muted: hex(0x556070, 1.0),
            line: hex(0x161B25, 0.12),
            hover: hex(0xE4E7E1, 1.0),
            hover_text: hex(0x161B25, 1.0),
            accent: hex(0x2B44D9, 1.0),
            handle: hex(0x2B44D9, 0.9),
            handle_edge: hex(0xFFFFFF, 0.75),
            shadow: true,
        }
    }

    pub fn dark() -> Self {
        Self {
            panel: hex(0x1B1E26, 1.0),
            text: hex(0xE6E9F0, 1.0),
            muted: hex(0x8E97AA, 1.0),
            line: hex(0xFFFFFF, 0.12),
            hover: hex(0x272B37, 1.0),
            hover_text: hex(0xFFFFFF, 1.0),
            accent: hex(0x7C93FF, 1.0),
            handle: hex(0x7C93FF, 0.95),
            handle_edge: hex(0x000000, 0.55),
            shadow: true,
        }
    }

    /// Windows' own colours, in a high-contrast theme.
    pub fn high_contrast() -> Self {
        let sys = |i: i32| {
            // SAFETY: GetSysColor has no preconditions.
            let c = unsafe { GetSysColor(SYS_COLOR_INDEX(i)) };
            // GDI's 0x00BBGGRR.
            hex(((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF), 1.0)
        };
        // COLOR_WINDOW 5, COLOR_WINDOWTEXT 8, COLOR_HIGHLIGHT 13, COLOR_HIGHLIGHTTEXT 14,
        // COLOR_HOTLIGHT 26.
        Self {
            panel: sys(5),
            text: sys(8),
            muted: sys(8),
            line: sys(8),
            hover: sys(13),
            hover_text: sys(14),
            accent: sys(26),
            handle: sys(13),
            handle_edge: sys(8),
            shadow: false,
        }
    }
}

/// One line of the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    /// A short note at the right end: a shortcut, "On" or "Off".
    pub detail: Option<String>,
    pub enabled: bool,
    /// The menu's main action, in the accent colour.
    pub primary: bool,
    /// A thin line above it, to group the rows.
    pub rule_before: bool,
}

/// Sizes at 96 DPI.
mod size {
    /// The line, and the strip around it that takes the pointer: narrow where the pointer
    /// stops at the screen's edge anyway (a window in a zone there is only 2 px from it),
    /// wider where it goes on past the edge.
    pub const LINE: f32 = 4.0;
    pub const LINE_WIDE: f32 = 6.0;
    pub const LINE_LENGTH: f32 = 56.0;
    pub const STRIP: f32 = 8.0;
    pub const STRIP_THROUGH: f32 = 12.0;
    pub const STRIP_LENGTH: f32 = 76.0;
    pub const PANEL: f32 = 256.0;
    pub const HEADER: f32 = 50.0;
    pub const ROW: f32 = 34.0;
    pub const RULE: f32 = 9.0;
    pub const PAD: f32 = 6.0;
    pub const RADIUS: f32 = 10.0;
    pub const SHADOW: f32 = 10.0;
}

fn px(v: f32) -> i32 {
    // Overlay sizes are small and positive.
    #[allow(clippy::cast_possible_truncation)]
    let r = v.round() as i32;
    r
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i32) -> f32 {
    v as f32
}

fn scale(dpi: u32) -> f32 {
    // DPI values are small.
    #[allow(clippy::cast_precision_loss)]
    let s = dpi.max(96) as f32 / 96.0;
    s
}

/// What shows, drawn again only when it changes.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Handle {
        place: Place,
        wide: bool,
        palette: Palette,
    },
    Menu {
        place: Place,
        title: String,
        subtitle: String,
        rows: Vec<Row>,
        hot: Option<usize>,
        palette: Palette,
    },
}

pub struct Edge {
    window: LayeredWindow,
    shown: Option<Shown>,
    /// The menu's rows on screen, for [`Edge::row_at`].
    rows: Vec<Rect>,
}

/// Where the handle's strip goes on screen.
pub fn handle_rect(place: Place) -> Rect {
    let s = scale(place.dpi);
    let strip = if place.through {
        size::STRIP_THROUGH
    } else {
        size::STRIP
    };
    let (w, h) = (px(strip * s), px(size::STRIP_LENGTH * s));
    let top = (place.middle - h / 2).clamp(place.work.top, (place.work.bottom - h).max(place.work.top));
    let left = match place.side {
        Side::Left => place.work.left,
        Side::Right => place.work.right - w,
    };
    Rect::from_origin_size(left, top, w, h)
}

/// The middle of a handle placed so it stays on its edge, for a pointer dragging it to `y`.
pub fn clamp_middle(place: Place, y: i32) -> i32 {
    let half = px(size::STRIP_LENGTH * scale(place.dpi)) / 2;
    y.clamp(
        place.work.top + half,
        (place.work.bottom - half).max(place.work.top + half),
    )
}

impl Edge {
    pub fn new() -> Result<Self> {
        Ok(Self {
            window: LayeredWindow::with_proc("MosaikEdge", false, true, edge_proc)?,
            shown: None,
            rows: Vec::new(),
        })
    }

    pub fn is(&self, hwnd: isize) -> bool {
        self.window.hwnd().0 as isize == hwnd
    }

    pub fn hwnd(&self) -> HWND {
        self.window.hwnd()
    }

    /// Where it is on screen, while it shows.
    pub fn shown_at(&self) -> Option<Rect> {
        self.window.shown_at()
    }

    /// The menu shows (rather than the handle).
    pub fn menu_open(&self) -> bool {
        matches!(self.shown, Some(Shown::Menu { .. }))
    }

    pub fn hide(&mut self) {
        self.window.hide();
        self.shown = None;
        self.rows.clear();
    }

    /// Shows the handle: the line at `place`, wider while the pointer is on it (`wide`).
    pub fn show_handle(
        &mut self,
        canvas: &mut Canvas,
        place: Place,
        wide: bool,
        palette: Palette,
    ) -> Result<()> {
        let want = Shown::Handle { place, wide, palette };
        if self.shown.as_ref() == Some(&want) && self.window.shown_at().is_some() {
            return Ok(());
        }
        // Only a handle that appears or moves goes back on top: one drawn again where it is
        // (wider under the pointer) stays as it is.
        let appears = !matches!(&self.shown, Some(Shown::Handle { place: p, .. }) if *p == place)
            || self.window.shown_at().is_none();
        let s = scale(place.dpi);
        let r = handle_rect(place);
        let (w, h) = (f(r.width()), f(r.height()));
        let line = if wide { size::LINE_WIDE } else { size::LINE } * s;
        let length = size::LINE_LENGTH * s;
        let inset = 2.0 * s;
        let left = match place.side {
            Side::Left => inset,
            Side::Right => w - inset - line,
        };
        let top = (h - length) / 2.0;
        let surface = self.window.surface(
            usize::try_from(r.width()).unwrap_or(1),
            usize::try_from(r.height()).unwrap_or(1),
        )?;
        canvas.render(surface, |pen| {
            // Faintly there everywhere, so the whole strip takes the pointer.
            pen.fill_round(Area::new(0.0, 0.0, w, h), 0.0, Rgba(0.0, 0.0, 0.0, 1.0 / 255.0));
            let pill = Area::new(left, top, left + line, top + length);
            pen.fill_round(pill.inset(-s), line / 2.0 + s, palette.handle_edge);
            pen.fill_round(pill, line / 2.0, palette.handle);
            Ok(())
        })?;
        self.window.present(r.origin(), 255)?;
        if appears {
            // Above windows kept on top that came after it (a video player, Task Manager).
            crate::layered::restack(&[&self.window], crate::layered::Stack::Topmost);
        }
        self.shown = Some(want);
        self.rows.clear();
        Ok(())
    }

    /// Opens the menu by the handle at `place`: `title` and `subtitle` at its top (Mosaïk, the
    /// open workspace), then `rows`, the one at `hot` lit.
    #[allow(clippy::too_many_arguments)]
    pub fn show_menu(
        &mut self,
        canvas: &mut Canvas,
        place: Place,
        title: &str,
        subtitle: &str,
        rows: &[Row],
        hot: Option<usize>,
        palette: Palette,
    ) -> Result<()> {
        let want = Shown::Menu {
            place,
            title: title.to_owned(),
            subtitle: subtitle.to_owned(),
            rows: rows.to_vec(),
            hot,
            palette,
        };
        if self.shown.as_ref() == Some(&want) && self.window.shown_at().is_some() {
            return Ok(());
        }
        let s = scale(place.dpi);
        let shadow = size::SHADOW * s;
        let panel_w = size::PANEL * s;
        let mut panel_h = size::HEADER * s + 2.0 * size::PAD * s;
        for r in rows {
            if r.rule_before {
                panel_h += size::RULE * s;
            }
            panel_h += size::ROW * s;
        }
        let (w, h) = (px(panel_w + 2.0 * shadow), px(panel_h + 2.0 * shadow));
        let handle = handle_rect(place);
        let middle = handle.top + handle.height() / 2;
        let top = (middle - h / 2).clamp(place.work.top, (place.work.bottom - h).max(place.work.top));
        let left = match place.side {
            Side::Left => place.work.left,
            Side::Right => place.work.right - w,
        };
        let at = Rect::from_origin_size(left, top, w, h);
        // Rows, and where each lands on screen.
        let mut placed: Vec<(Area, &Row, bool)> = Vec::new();
        let mut y = shadow + size::HEADER * s + size::PAD * s;
        for (i, r) in rows.iter().enumerate() {
            if r.rule_before {
                y += size::RULE * s;
            }
            let a = Area::new(
                shadow + size::PAD * s,
                y,
                shadow + panel_w - size::PAD * s,
                y + size::ROW * s,
            );
            placed.push((a, r, hot == Some(i) && r.enabled));
            y += size::ROW * s;
        }
        // Details are right-aligned: measured first.
        let detail_size = 12.0 * s;
        let widths: Vec<f32> = rows
            .iter()
            .map(|r| {
                r.detail
                    .as_deref()
                    .map_or(0.0, |d| canvas.text_width(d, detail_size, Weight::Normal))
            })
            .collect();
        let surface = self
            .window
            .surface(usize::try_from(w).unwrap_or(1), usize::try_from(h).unwrap_or(1))?;
        let panel = Area::new(shadow, shadow, shadow + panel_w, shadow + panel_h);
        let radius = size::RADIUS * s;
        canvas.render(surface, |pen| {
            pen.fill_round(
                Area::new(0.0, 0.0, f(w), f(h)),
                0.0,
                Rgba(0.0, 0.0, 0.0, 1.0 / 255.0),
            );
            if palette.shadow {
                // A soft shadow: rings of growing size and fading darkness.
                let steps = 8;
                for k in (1..=steps).rev() {
                    let grow = shadow * f(k) / f(steps);
                    let alpha = 0.10 * (1.0 - f(k) / f(steps + 1));
                    let a = Area::new(
                        panel.left - grow,
                        panel.top - grow + 2.0 * s,
                        panel.right + grow,
                        panel.bottom + grow + 2.0 * s,
                    );
                    pen.fill_round(a, radius + grow, Rgba(0.0, 0.0, 0.0, alpha / f(steps) * 4.0));
                }
            }
            pen.fill_round(panel, radius, palette.panel);
            pen.stroke_round(panel, radius, s.max(1.0), palette.line);
            let text_left = panel.left + 16.0 * s;
            let text_right = panel.right - 16.0 * s;
            pen.text(
                title,
                Area::new(text_left, panel.top + 8.0 * s, text_right, panel.top + 24.0 * s),
                11.5 * s,
                Weight::SemiBold,
                Align::Leading,
                palette.muted,
            )?;
            pen.text(
                subtitle,
                Area::new(text_left, panel.top + 23.0 * s, text_right, panel.top + 43.0 * s),
                14.5 * s,
                Weight::SemiBold,
                Align::Leading,
                palette.text,
            )?;
            for (i, (a, r, lit)) in placed.iter().enumerate() {
                if r.rule_before {
                    let y = a.top - size::RULE * s / 2.0;
                    pen.fill_round(
                        Area::new(
                            panel.left + 12.0 * s,
                            y - s / 2.0,
                            panel.right - 12.0 * s,
                            y + s / 2.0,
                        ),
                        0.0,
                        palette.line,
                    );
                }
                if *lit {
                    pen.fill_round(*a, 6.0 * s, palette.hover);
                }
                let color = match (r.enabled, *lit, r.primary) {
                    (false, _, _) => Rgba(palette.muted.0, palette.muted.1, palette.muted.2, 0.6),
                    (true, true, _) => palette.hover_text,
                    (true, false, true) => palette.accent,
                    (true, false, false) => palette.text,
                };
                let detail_w = widths.get(i).copied().unwrap_or(0.0);
                let label_right = a.right - 10.0 * s - if detail_w > 0.0 { detail_w + 8.0 * s } else { 0.0 };
                pen.text(
                    &r.label,
                    Area::new(a.left + 10.0 * s, a.top, label_right, a.bottom),
                    13.5 * s,
                    if r.primary {
                        Weight::SemiBold
                    } else {
                        Weight::Normal
                    },
                    Align::Leading,
                    color,
                )?;
                if let Some(d) = &r.detail {
                    pen.text(
                        d,
                        Area::new(a.right - 10.0 * s - detail_w, a.top, a.right - 10.0 * s, a.bottom),
                        detail_size,
                        Weight::Normal,
                        Align::Leading,
                        if *lit { palette.hover_text } else { palette.muted },
                    )?;
                }
            }
            Ok(())
        })?;
        self.window.present(at.origin(), 255)?;
        crate::layered::restack(&[&self.window], crate::layered::Stack::Topmost);
        self.rows = placed
            .iter()
            .map(|(a, _, _)| {
                Rect::new(
                    at.left + px(a.left),
                    at.top + px(a.top),
                    at.left + px(a.right),
                    at.top + px(a.bottom),
                )
            })
            .collect();
        self.shown = Some(want);
        Ok(())
    }

    /// The open menu's rows, and where each is on screen.
    pub fn menu_rows(&self) -> Vec<(&Row, Rect)> {
        match &self.shown {
            Some(Shown::Menu { rows, .. }) => rows.iter().zip(self.rows.iter().copied()).collect(),
            _ => Vec::new(),
        }
    }

    /// The row of the open menu under `p` (screen coordinates).
    pub fn row_at(&self, p: Point) -> Option<usize> {
        if !self.menu_open() {
            return None;
        }
        self.rows.iter().position(|r| r.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(side: Side, middle: i32) -> Place {
        Place {
            through: false,
            work: Rect::new(0, 0, 1920, 1032),
            side,
            middle,
            dpi: 96,
        }
    }

    #[test]
    fn the_handle_sits_on_its_edge_and_stays_on_screen() {
        let r = handle_rect(place(Side::Right, 310));
        assert_eq!((r.right, r.width(), r.height()), (1920, 8, 76));
        assert_eq!(r.top + r.height() / 2, 310);
        assert_eq!(handle_rect(place(Side::Left, 310)).left, 0);
        // Dragged past the top or the bottom, it stops at the end of the edge.
        assert_eq!(handle_rect(place(Side::Right, 0)).top, 0);
        assert_eq!(handle_rect(place(Side::Right, 5000)).bottom, 1032);
        assert_eq!(clamp_middle(place(Side::Right, 0), -40), 38);
        assert_eq!(clamp_middle(place(Side::Right, 0), 2000), 1032 - 38);
    }

    #[test]
    fn it_takes_a_wider_strip_where_the_pointer_goes_on_past_the_edge() {
        let mut p = place(Side::Right, 310);
        p.through = true;
        let r = handle_rect(p);
        assert_eq!((r.right, r.width()), (1920, 12));
    }

    #[test]
    fn it_scales_with_the_monitor() {
        let mut p = place(Side::Right, 500);
        p.dpi = 192;
        let r = handle_rect(p);
        assert_eq!((r.width(), r.height()), (16, 152));
    }
}
