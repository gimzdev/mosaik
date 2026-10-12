//! The tab strip of a tab group: the zone's tag, then one chip per tab, in the band above the
//! group's window (where a single window shows its tag). Unlike the other overlays it takes the
//! mouse: a click shows a tab, a drag reorders the tabs or pulls one out. It never takes the
//! keyboard focus. Transparent pixels let clicks through, as with every layered window.

use std::cell::{Cell, RefCell};

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::zone::ZoneColor;
use mosaik_win::error::Result;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, MA_NOACTIVATE, PostMessageW, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE,
};

use crate::canvas::{Align, Area, Canvas, Rgba, Weight};
use crate::layered::{LayeredWindow, Stack, restack};

/// What the mouse did on a tab strip, in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarInput {
    Down {
        bar: isize,
        at: Point,
    },
    Move {
        bar: isize,
        at: Point,
    },
    Up {
        bar: isize,
        at: Point,
    },
    /// The mouse was taken away mid-drag (Esc, another window): the drag is off.
    Cancel {
        bar: isize,
    },
}

thread_local! {
    static INPUTS: RefCell<Vec<BarInput>> = const { RefCell::new(Vec::new()) };
    static WAKE: Cell<Option<(isize, u32)>> = const { Cell::new(None) };
    /// The strip holding the mouse, while the button is down on it.
    static CAPTURED: Cell<Option<isize>> = const { Cell::new(None) };
}

/// Tab strips post `message` to `window` when the mouse did something, for the engine to read
/// with [`take_inputs`].
pub fn wake_on_input(window: HWND, message: u32) {
    WAKE.with(|w| w.set(Some((window.0 as isize, message))));
}

pub fn take_inputs() -> Vec<BarInput> {
    INPUTS.with(|i| {
        i.try_borrow_mut()
            .map(|mut v| std::mem::take(&mut *v))
            .unwrap_or_default()
    })
}

fn push(input: BarInput) {
    INPUTS.with(|i| {
        if let Ok(mut v) = i.try_borrow_mut() {
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

/// The pointer in screen coordinates, from a mouse message's client coordinates.
fn screen_point(hwnd: HWND, lparam: LPARAM) -> Point {
    // The low and high words are signed 16-bit client coordinates.
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (i32::from(lparam.0 as i16), i32::from((lparam.0 >> 16) as i16));
    let mut p = POINT { x, y };
    // SAFETY: converts a point of our own window's client area.
    let _ = unsafe { ClientToScreen(hwnd, &mut p) };
    Point::new(p.x, p.y)
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let bar = hwnd.0 as isize;
    match msg {
        WM_LBUTTONDOWN => {
            // SAFETY: our own window keeps the mouse until the button goes up.
            unsafe { SetCapture(hwnd) };
            CAPTURED.with(|c| c.set(Some(bar)));
            push(BarInput::Down {
                bar,
                at: screen_point(hwnd, lparam),
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if CAPTURED.with(Cell::get) == Some(bar) {
                push(BarInput::Move {
                    bar,
                    at: screen_point(hwnd, lparam),
                });
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if CAPTURED.with(Cell::take) == Some(bar) {
                push(BarInput::Up {
                    bar,
                    at: screen_point(hwnd, lparam),
                });
            }
            // SAFETY: gives the mouse back.
            let _ = unsafe { ReleaseCapture() };
            LRESULT(0)
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            if CAPTURED.with(Cell::take) == Some(bar) {
                push(BarInput::Cancel { bar });
            }
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// One tab as the strip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView {
    pub title: String,
    pub active: bool,
}

/// How a strip looks.
#[derive(Clone, Debug, PartialEq)]
pub struct BarLook {
    pub color: ZoneColor,
    /// The zone's name, in the tag at the start of the strip.
    pub label: String,
    pub dpi: u32,
    /// Chips drawn for a dark theme (else light).
    pub dark: bool,
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

fn hex(rgb: u32, alpha: f32) -> Rgba {
    // Each channel is 0..=255, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let c = |shift: u32| ((rgb >> shift) & 0xFF) as f32 / 255.0;
    Rgba(c(16), c(8), c(0), alpha)
}

/// What a strip shows; drawn again only when this changes.
#[derive(Clone, Debug, PartialEq)]
struct Drawn {
    width: i32,
    height: i32,
    look: BarLook,
    tabs: Vec<TabView>,
    drop_at: Option<usize>,
}

/// A group's tab strip.
pub struct TabBar {
    window: LayeredWindow,
    /// Where each chip is inside the strip, in tab order, as last drawn.
    chips: Vec<Rect>,
    drawn: Option<Drawn>,
}

impl TabBar {
    pub fn new() -> Result<Self> {
        Ok(Self {
            window: LayeredWindow::with_proc("MosaikTabBar", false, false, bar_proc)?,
            chips: Vec::new(),
            drawn: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.window.hwnd()
    }

    pub fn is(&self, bar: isize) -> bool {
        self.window.hwnd().0 as isize == bar
    }

    /// `p` (on screen) inside the strip.
    fn local(&self, p: Point) -> Option<Point> {
        let at = self.window.shown_at()?;
        Some(Point::new(p.x - at.left, p.y - at.top))
    }

    /// The tab whose chip is under `p` (on screen).
    pub fn hit(&self, p: Point) -> Option<usize> {
        let p = self.local(p)?;
        self.chips.iter().position(|c| c.contains(p))
    }

    /// Where a tab dropped (or dragged) at `p` (on screen) goes: before the first chip whose
    /// middle is to the right of `p`.
    pub fn slot_for(&self, p: Point) -> usize {
        let Some(p) = self.local(p) else {
            return self.chips.len();
        };
        self.chips
            .iter()
            .position(|c| p.x < c.center().x)
            .unwrap_or(self.chips.len())
    }

    /// The strip on screen, when shown.
    pub fn rect(&self) -> Option<Rect> {
        self.window.shown_at()
    }

    /// Each chip on screen, in tab order (empty when hidden).
    pub fn chips(&self) -> Vec<Rect> {
        let Some(at) = self.window.shown_at() else {
            return Vec::new();
        };
        self.chips.iter().map(|c| c.offset(at.left, at.top)).collect()
    }

    /// Shows the strip across `band` (the band above the group's window), right above `window`
    /// in the stacking order. `drop_at` shows the "Drop to add" slot at that position.
    pub fn show(
        &mut self,
        canvas: &mut Canvas,
        band: Rect,
        look: &BarLook,
        tabs: &[TabView],
        drop_at: Option<usize>,
        stack: Stack,
    ) -> Result<()> {
        if band.width() <= 0 || band.height() <= 0 {
            self.hide();
            return Ok(());
        }
        // DPI values are small (96 to 480).
        #[allow(clippy::cast_precision_loss)]
        let s = look.dpi.max(96) as f32 / 96.0;
        let want = Drawn {
            width: band.width(),
            height: band.height(),
            look: look.clone(),
            tabs: tabs.to_vec(),
            drop_at,
        };
        if self.drawn.as_ref() != Some(&want) {
            // Layout, left to right: the zone's tag, then the chips (shrunk to fit), then the
            // drop slot.
            let h = 21.0 * s;
            let top = f(band.height()) - h - 3.0 * s;
            let text = 11.5 * s;
            let pad = 10.0 * s;
            let gap = 4.0 * s;
            let tag_w = canvas.text_width(&look.label, text, Weight::SemiBold) + 22.0 * s;
            let drop_label = "Drop to add";
            let drop_w = if drop_at.is_some() {
                canvas.text_width(drop_label, text, Weight::SemiBold) + 2.0 * pad + 14.0 * s
            } else {
                0.0
            };
            let natural: Vec<f32> = tabs
                .iter()
                .map(|t| {
                    let weight = if t.active {
                        Weight::SemiBold
                    } else {
                        Weight::Normal
                    };
                    (canvas.text_width(&t.title, text, weight) + 2.0 * pad).min(220.0 * s)
                })
                .collect();
            let room = f(band.width()) - tag_w - gap * 2.0 - drop_w;
            let total: f32 = natural.iter().sum::<f32>() + gap * f(i32::try_from(tabs.len()).unwrap_or(0));
            let shrink = if total > room && total > 0.0 {
                (room / total).max(0.2)
            } else {
                1.0
            };
            let mut chips = Vec::with_capacity(tabs.len());
            let mut x = tag_w + gap * 2.0;
            let mut drop_x = None;
            for (i, w) in natural.iter().enumerate() {
                if drop_at == Some(i) {
                    drop_x = Some(x);
                    x += drop_w + gap;
                }
                let w = (w * shrink).max(36.0 * s);
                chips.push((x, w));
                x += w + gap;
            }
            if drop_at.is_some() && drop_x.is_none() {
                drop_x = Some(x);
            }
            let panel = if look.dark {
                hex(0x1B1E26, 0.96)
            } else {
                hex(0xF7F8F4, 0.96)
            };
            let ink = if look.dark {
                hex(0xE6E9F0, 1.0)
            } else {
                hex(0x161B25, 1.0)
            };
            let muted = if look.dark {
                hex(0xA3ACBA, 1.0)
            } else {
                hex(0x556070, 1.0)
            };
            let zone = Rgba::of(look.color.fill(), 1.0);
            let label = Rgba::of(look.color.label(), 1.0);
            let saffron = Rgba::of(ZoneColor::Saffron.fill(), 1.0);
            let surface = self.window.surface(
                usize::try_from(band.width()).unwrap_or(1),
                usize::try_from(band.height()).unwrap_or(1),
            )?;
            canvas.render(surface, |pen| {
                let tag = Area::new(0.0, top, tag_w, top + h);
                pen.fill_round(tag, h / 2.0, zone);
                pen.text(
                    &look.label,
                    tag.inset(0.0),
                    text,
                    Weight::SemiBold,
                    Align::Center,
                    label,
                )?;
                for (t, (x, w)) in tabs.iter().zip(&chips) {
                    let chip = Area::new(*x, top, x + w, top + h);
                    if t.active {
                        pen.fill_round(chip, 6.0 * s, panel);
                        // The visible tab carries the zone's colour along its top, as on the site.
                        pen.fill_round(
                            Area::new(x + 4.0 * s, top, x + w - 4.0 * s, top + 2.5 * s),
                            1.25 * s,
                            zone,
                        );
                    } else {
                        pen.fill_round(chip, 6.0 * s, Rgba(panel.0, panel.1, panel.2, 0.72));
                    }
                    let weight = if t.active {
                        Weight::SemiBold
                    } else {
                        Weight::Normal
                    };
                    pen.text(
                        &t.title,
                        Area::new(x + pad, top, x + w - pad, top + h),
                        text,
                        weight,
                        Align::Leading,
                        if t.active { ink } else { muted },
                    )?;
                }
                if let Some(x) = drop_x {
                    let slot = Area::new(x, top, x + drop_w, top + h);
                    pen.fill_round(slot, 6.0 * s, Rgba(panel.0, panel.1, panel.2, 0.85));
                    pen.dash_round(slot, 6.0 * s, 1.6 * s, saffron);
                    pen.text(
                        &format!("+ {drop_label}"),
                        slot,
                        text,
                        Weight::SemiBold,
                        Align::Center,
                        saffron,
                    )?;
                }
                Ok(())
            })?;
            self.chips = chips
                .iter()
                .map(|(x, w)| Rect::new(px(*x), px(top), px(x + w), px(top + h)))
                .collect();
            self.window.present(band.origin(), 255)?;
            self.drawn = Some(want);
        } else if self.window.shown_at().is_none() {
            self.window.present(band.origin(), 255)?;
        } else {
            self.window.move_to(band.origin());
        }
        restack(&[&self.window], stack);
        Ok(())
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }

    pub fn is_shown(&self) -> bool {
        self.window.shown_at().is_some()
    }
}
