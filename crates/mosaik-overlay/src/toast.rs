//! A toast: what Mosaïk has to say while it works (a window it cannot move, a shortcut taken by
//! another app), in a small panel by the corner of the screen the pointer is on, above the
//! taskbar. It fades away by itself; a click opens Mosaïk's window, where the message stays with
//! its notes. It never takes the keyboard, and never opens a window over what the user is doing.

use std::cell::Cell;

use mosaik_core::geometry::{Point, Rect};
use mosaik_win::error::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, MA_NOACTIVATE, PostMessageW, WM_CLOSE, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_SETCURSOR,
};

use crate::canvas::{Align, Area, Canvas, Rgba, Weight};
use crate::edge::Palette;
use crate::layered::LayeredWindow;

/// How long it takes to fade in, and away, in milliseconds.
pub const FADE_IN_MS: u32 = 140;
pub const FADE_OUT_MS: u32 = 260;

/// Sizes at 96 DPI.
mod size {
    pub const WIDTH: f32 = 360.0;
    pub const PAD: f32 = 14.0;
    pub const MARK: f32 = 18.0;
    pub const TITLE: f32 = 13.5;
    pub const BODY: f32 = 12.5;
    pub const LINE: f32 = 18.0;
    pub const RADIUS: f32 = 12.0;
    pub const SHADOW: f32 = 12.0;
    /// From the work area's corner.
    pub const MARGIN: f32 = 14.0;
    pub const MOST_LINES: usize = 3;
}

thread_local! {
    static CLICKED: Cell<bool> = const { Cell::new(false) };
    static WAKE: Cell<Option<(isize, u32)>> = const { Cell::new(None) };
}

/// The toast posts `message` to `window` when it is clicked, for the engine to read with
/// [`take_clicked`].
pub fn wake_on_click(window: HWND, message: u32) {
    WAKE.with(|w| w.set(Some((window.0 as isize, message))));
}

/// Whether the toast was clicked since this was last asked.
pub fn take_clicked() -> bool {
    CLICKED.with(Cell::take)
}

unsafe extern "system" fn toast_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            CLICKED.with(|c| c.set(true));
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
            LRESULT(0)
        }
        // A hand over it: it opens something.
        WM_SETCURSOR => {
            // SAFETY: loads a system cursor and sets it for this message.
            unsafe {
                if let Ok(c) = windows::Win32::UI::WindowsAndMessaging::LoadCursorW(
                    None,
                    windows::Win32::UI::WindowsAndMessaging::IDC_HAND,
                ) {
                    windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(c));
                }
            }
            LRESULT(1)
        }
        WM_MOUSEACTIVATE => LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        WM_CLOSE => LRESULT(0),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn scale(dpi: u32) -> f32 {
    // DPI values are small.
    #[allow(clippy::cast_precision_loss)]
    let s = dpi.max(96) as f32 / 96.0;
    s
}

fn px(v: f32) -> i32 {
    // Overlay sizes are small and positive.
    #[allow(clippy::cast_possible_truncation)]
    let r = v.round() as i32;
    r
}

/// `text` in lines of at most `width` (measured with `measure`), at most `most` of them, the last
/// cut short with an ellipsis when more would be needed.
pub fn wrap(text: &str, width: f32, most: usize, measure: &mut dyn FnMut(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if line.is_empty() || measure(&candidate) <= width {
            line = candidate;
        } else {
            lines.push(std::mem::replace(&mut line, word.to_owned()));
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > most {
        lines.truncate(most);
        if let Some(last) = lines.last_mut() {
            // The canvas cuts a line too long for its box with an ellipsis; one is added here
            // so a line that fits still says more was left out.
            last.push('\u{2026}');
        }
    }
    lines
}

/// The screen a toast shows on: its work area and DPI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub work: Rect,
    pub dpi: u32,
}

/// What shows: where, and its words, so the same toast is not drawn again.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    at: Rect,
}

pub struct Toast {
    window: LayeredWindow,
    shown: Option<Shown>,
}

impl Toast {
    pub fn new() -> Result<Self> {
        Ok(Self {
            window: LayeredWindow::with_proc("MosaikToast", false, true, toast_proc)?,
            shown: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.window.hwnd()
    }

    /// Where it is on screen, while it shows.
    pub fn shown_at(&self) -> Option<Rect> {
        self.window.shown_at()
    }

    /// Draws `title` and `body` by the bottom corner of the screen's work area (on the side the
    /// taskbar's clock is), at `alpha` (0 to 255; a fade starts low).
    pub fn show(
        &mut self,
        canvas: &mut Canvas,
        screen: Screen,
        (title, body): (&str, &str),
        palette: Palette,
        alpha: u8,
    ) -> Result<()> {
        let Screen { work, dpi } = screen;
        let s = scale(dpi);
        let shadow = size::SHADOW * s;
        let panel_w = size::WIDTH * s;
        let text_left = size::PAD * s + size::MARK * s + 12.0 * s;
        let text_w = panel_w - text_left - size::PAD * s;
        let body_size = size::BODY * s;
        let mut measure = |t: &str| canvas.text_width(t, body_size, Weight::Normal);
        let lines = wrap(body, text_w, size::MOST_LINES, &mut measure);
        let line_count = f32::from(u8::try_from(lines.len()).unwrap_or(3));
        let panel_h = size::PAD * s * 2.0 + 20.0 * s + line_count * size::LINE * s;
        let (w, h) = (px(panel_w + 2.0 * shadow), px(panel_h + 2.0 * shadow));
        let margin = px(size::MARGIN * s);
        let at = Rect::from_origin_size(
            work.right - w - margin + px(shadow),
            work.bottom - h - margin + px(shadow),
            w,
            h,
        );
        let surface = self
            .window
            .surface(usize::try_from(w).unwrap_or(1), usize::try_from(h).unwrap_or(1))?;
        let panel = Area::new(shadow, shadow, shadow + panel_w, shadow + panel_h);
        let radius = size::RADIUS * s;
        let f = |v: i32| {
            // Pixel counts are small.
            #[allow(clippy::cast_precision_loss)]
            let r = v as f32;
            r
        };
        canvas.render(surface, |pen| {
            if palette.shadow {
                let steps = 8;
                for k in (1..=steps).rev() {
                    let grow = shadow * f(k) / f(steps);
                    let a = 0.12 * (1.0 - f(k) / f(steps + 1));
                    pen.fill_round(
                        Area::new(
                            panel.left - grow,
                            panel.top - grow + 3.0 * s,
                            panel.right + grow,
                            panel.bottom + grow + 3.0 * s,
                        ),
                        radius + grow,
                        Rgba(0.0, 0.0, 0.0, a / f(steps) * 4.0),
                    );
                }
            }
            pen.fill_round(panel, radius, palette.panel);
            pen.stroke_round(panel, radius, s.max(1.0), palette.line);
            // Mosaïk's mark: four tiles of the palette.
            let m = Area::new(
                panel.left + size::PAD * s,
                panel.top + size::PAD * s + 1.0 * s,
                panel.left + size::PAD * s + size::MARK * s,
                panel.top + size::PAD * s + 1.0 * s + size::MARK * s,
            );
            let u = size::MARK * s / 32.0;
            let tile = |x: f32, y: f32, tw: f32, th: f32| {
                Area::new(
                    m.left + x * u,
                    m.top + y * u,
                    m.left + (x + tw) * u,
                    m.top + (y + th) * u,
                )
            };
            pen.fill_round(
                tile(2.0, 2.0, 16.0, 16.0),
                3.0 * u,
                Rgba(0.169, 0.267, 0.851, 1.0),
            );
            pen.fill_round(tile(21.0, 2.0, 9.0, 9.0), 3.0 * u, Rgba(0.957, 0.694, 0.227, 1.0));
            pen.fill_round(
                tile(21.0, 14.0, 9.0, 16.0),
                3.0 * u,
                Rgba(0.071, 0.471, 0.416, 1.0),
            );
            pen.fill_round(
                tile(2.0, 21.0, 16.0, 9.0),
                3.0 * u,
                Rgba(0.788, 0.204, 0.314, 1.0),
            );
            let left = panel.left + text_left;
            let right = panel.right - size::PAD * s;
            let mut y = panel.top + size::PAD * s;
            pen.text(
                title,
                Area::new(left, y, right, y + 20.0 * s),
                size::TITLE * s,
                Weight::SemiBold,
                Align::Leading,
                palette.text,
            )?;
            y += 20.0 * s;
            for line in &lines {
                pen.text(
                    line,
                    Area::new(left, y, right, y + size::LINE * s),
                    body_size,
                    Weight::Normal,
                    Align::Leading,
                    palette.muted,
                )?;
                y += size::LINE * s;
            }
            Ok(())
        })?;
        self.window.present(Point::new(at.left, at.top), alpha.max(1))?;
        crate::layered::restack(&[&self.window], crate::layered::Stack::Topmost);
        self.shown = Some(Shown { at });
        Ok(())
    }

    /// Changes how opaque it is while it fades.
    pub fn fade(&mut self, alpha: u8) -> Result<()> {
        match &self.shown {
            Some(_) => self.window.set_alpha(alpha.max(1)),
            None => Ok(()),
        }
    }

    pub fn hide(&mut self) {
        self.window.hide();
        self.shown = None;
    }
}

#[cfg(test)]
mod tests {
    use super::wrap;

    /// Every character is 7 pixels wide.
    fn width(t: &str) -> f32 {
        // Short test strings.
        #[allow(clippy::cast_precision_loss)]
        let w = t.chars().count() as f32 * 7.0;
        w
    }

    #[test]
    fn words_wrap_into_lines_that_fit() {
        let lines = wrap(
            "Task Manager runs as administrator, so Windows keeps it",
            140.0,
            3,
            &mut width,
        );
        // 20 characters fit (140 px): "Task Manager runs as" exactly.
        assert_eq!(
            lines,
            ["Task Manager runs as", "administrator, so", "Windows keeps it"]
        );
        assert!(lines.iter().all(|l| width(l) <= 140.0));
    }

    #[test]
    fn too_many_lines_end_with_an_ellipsis() {
        let lines = wrap("one two three four five six seven eight", 35.0, 2, &mut width);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('\u{2026}'), "{lines:?}");
    }

    #[test]
    fn a_word_longer_than_a_line_still_shows() {
        assert_eq!(
            wrap("Supercalifragilistic", 35.0, 3, &mut width),
            ["Supercalifragilistic"]
        );
        assert!(wrap("   ", 35.0, 3, &mut width).is_empty());
    }
}
