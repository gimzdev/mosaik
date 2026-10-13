//! The tab strip of a tab group: the zone's tag, then one chip per tab, where a single window
//! shows its tag: in the visible tab's own title bar, against its buttons, or in the band above
//! it. Unlike the other overlays it takes the mouse: a click shows a tab, a drag reorders the
//! tabs or pulls one out. It never takes the keyboard focus. Transparent pixels let clicks
//! through, as with every layered window, so the title bar between chips still moves the window.

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

use mosaik_core::strip;

use crate::canvas::{Align, Area, Canvas, Rgba, Weight};
use crate::layered::{LayeredWindow, Stack, restack};

/// What the mouse did on a tab strip, or on the band above a window (`bar` is the window that
/// took it), in screen coordinates.
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

/// The mouse on an overlay that takes it (a tab strip, a window's band): pressed, it keeps the
/// mouse until the button goes up, and the engine reads what happened ([`take_inputs`]). `None`
/// for messages that are not the mouse's.
pub(crate) fn mouse(hwnd: HWND, msg: u32, lparam: LPARAM) -> Option<LRESULT> {
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
        }
        WM_MOUSEMOVE => {
            if CAPTURED.with(Cell::get) == Some(bar) {
                push(BarInput::Move {
                    bar,
                    at: screen_point(hwnd, lparam),
                });
            }
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
        }
        WM_CAPTURECHANGED => {
            if CAPTURED.with(Cell::take) == Some(bar) {
                push(BarInput::Cancel { bar });
            }
        }
        // Windows asks every window to end what it is doing (a dialog, Ctrl+Alt+Del): the press
        // ends, and the mouse goes back.
        WM_CANCELMODE => {
            if CAPTURED.with(Cell::take) == Some(bar) {
                push(BarInput::Cancel { bar });
            }
            // SAFETY: gives the mouse back.
            let _ = unsafe { ReleaseCapture() };
        }
        WM_MOUSEACTIVATE => return Some(LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3))),
        _ => return None,
    }
    Some(LRESULT(0))
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = mouse(hwnd, msg, lparam) {
        return r;
    }
    if msg == WM_CLOSE {
        return LRESULT(0);
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Ends a press on any tab strip or band (Esc during a drag): the mouse goes back and no
/// `Cancel` is queued, since the caller is the one cancelling.
pub fn cancel_press() {
    if CAPTURED.with(Cell::take).is_some() {
        // SAFETY: gives the mouse back; the strip's WM_CAPTURECHANGED then finds no press.
        let _ = unsafe { ReleaseCapture() };
    }
}

/// One tab as the strip shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabView {
    /// Who the tab is (its window's handle), so a click names the tab it landed on even if the
    /// tabs changed since the strip was drawn.
    pub id: u64,
    pub title: String,
    pub active: bool,
}

/// What a press on a strip landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Tab(u64),
    /// The "+N" chip of tabs that do not fit.
    More,
}

/// Where a strip's chips sit in the rectangle it is given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPlace {
    /// A band above the window: chips from the left, at the band's bottom.
    Band,
    /// A free stretch of the window's title bar: chips against its right end, next to the
    /// window's buttons, centred.
    TitleBar,
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
    /// Windows 11 rounds window corners: a band's top corners follow.
    pub rounded: bool,
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
    place: BarPlace,
    look: BarLook,
    tabs: Vec<TabView>,
    drop_at: Option<usize>,
}

/// Where the chips are, as last drawn, in the strip's own pixels.
#[derive(Clone, Debug, Default, PartialEq)]
struct Placed {
    layout: strip::Layout,
    /// The tabs' ids, in tab order (the layout names tabs by index).
    ids: Vec<u64>,
    /// The chips' top and bottom.
    top: f32,
    bottom: f32,
}

/// A group's tab strip.
pub struct TabBar {
    window: LayeredWindow,
    placed: Placed,
    drawn: Option<Drawn>,
}

impl TabBar {
    pub fn new() -> Result<Self> {
        Ok(Self {
            window: LayeredWindow::with_proc("MosaikTabBar", false, false, bar_proc)?,
            placed: Placed::default(),
            drawn: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.window.hwnd()
    }

    pub fn is(&self, bar: isize) -> bool {
        self.window.hwnd().0 as isize == bar
    }

    /// `p` (on screen) in the strip's own pixels, when it is shown.
    fn local(&self, p: Point) -> Option<(f32, f32)> {
        let at = self.window.shown_at()?;
        Some((f(p.x - at.left), f(p.y - at.top)))
    }

    /// What is under `p` (on screen): a tab's chip or the "+N" chip.
    pub fn hit(&self, p: Point) -> Option<Hit> {
        let (x, y) = self.local(p)?;
        if y < self.placed.top || y >= self.placed.bottom {
            return None;
        }
        if let Some(i) = self.placed.layout.tab_at(x) {
            return self.placed.ids.get(i).map(|id| Hit::Tab(*id));
        }
        self.placed
            .layout
            .more
            .filter(|(span, _)| span.contains(x))
            .map(|_| Hit::More)
    }

    /// Where a tab dropped (or dragged) at `p` (on screen) goes: before the tab with this id,
    /// or at the end (`None`, also when the strip is hidden).
    pub fn insert_before(&self, p: Point) -> Option<u64> {
        let (x, _) = self.local(p)?;
        let i = self.placed.layout.insert_before(x)?;
        self.placed.ids.get(i).copied()
    }

    /// The strip on screen, when shown.
    pub fn rect(&self) -> Option<Rect> {
        self.window.shown_at()
    }

    /// Each drawn chip on screen with its tab's id, in tab order (empty when hidden).
    pub fn chips(&self) -> Vec<(u64, Rect)> {
        let Some(at) = self.window.shown_at() else {
            return Vec::new();
        };
        self.placed
            .layout
            .chips
            .iter()
            .filter_map(|(i, span)| {
                let id = self.placed.ids.get(*i)?;
                Some((
                    *id,
                    Rect::new(
                        px(span.x),
                        px(self.placed.top),
                        px(span.end()),
                        px(self.placed.bottom),
                    )
                    .offset(at.left, at.top),
                ))
            })
            .collect()
    }

    /// Shows the strip in `band`, placed as `place` says and stacked as `stack` says. `drop_at`
    /// shows the "Drop to add" slot before that tab (after the last one when it is the number of
    /// tabs).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        canvas: &mut Canvas,
        band: Rect,
        place: BarPlace,
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
            place,
            look: look.clone(),
            tabs: tabs.to_vec(),
            drop_at,
        };
        if self.drawn.as_ref() != Some(&want) {
            let h = f(mosaik_core::zone::CHIP_HEIGHT) * s;
            let top = match place {
                BarPlace::Band => (f(band.height()) - h - 3.0 * s).max(0.0),
                BarPlace::TitleBar => ((f(band.height()) - h) / 2.0).max(0.0),
            };
            let text = 11.5 * s;
            let pad = 10.0 * s;
            let drop_label = "+ Drop to add";
            let tag_w = if look.label.trim().is_empty() {
                0.0
            } else {
                canvas.text_width(&look.label, text, Weight::SemiBold) + 22.0 * s
            };
            let naturals: Vec<f32> = tabs
                .iter()
                .map(|t| {
                    let weight = if t.active {
                        Weight::SemiBold
                    } else {
                        Weight::Normal
                    };
                    canvas.text_width(&t.title, text, weight) + 2.0 * pad
                })
                .collect();
            let drop_w = canvas.text_width(drop_label, text, Weight::SemiBold) + 2.0 * pad;
            let layout = strip::layout(
                &strip::Input {
                    width: f(band.width()),
                    tag: tag_w,
                    tabs: &naturals,
                    active: tabs.iter().position(|t| t.active),
                    drop: drop_at.map(|at| (at, drop_w)),
                },
                strip::Metrics::at(s),
            );
            // Against the right end, in a title bar or a band: in the same place on every
            // window, next to its buttons.
            let layout = layout.align_right(f(band.width()));
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
            let (w, h_band) = (f(band.width()), f(band.height()));
            let band_fill = Rgba::of(crate::frame::band_color(look.dark), 1.0);
            let corner = if look.rounded { 8.0 * s } else { 0.0 };
            canvas.render(surface, |pen| {
                // Above the window, the strip is a solid band (no gap to see through), its top
                // corners following the window's; in a title bar, only its chips are drawn.
                if place == BarPlace::Band {
                    pen.fill_round(Area::new(0.0, 0.0, w, h_band), corner, band_fill);
                    pen.fill_round(Area::new(0.0, corner.min(h_band), w, h_band), 0.0, band_fill);
                }
                if let Some(t) = layout.tag {
                    let tag = Area::new(t.x, top, t.end(), top + h);
                    pen.fill_round(tag, h / 2.0, zone);
                    pen.text(
                        &look.label,
                        Area::new(t.x + 8.0 * s, top, t.end() - 8.0 * s, top + h),
                        text,
                        Weight::SemiBold,
                        Align::Center,
                        label,
                    )?;
                }
                for (i, span) in &layout.chips {
                    let Some(t) = tabs.get(*i) else { continue };
                    let chip = Area::new(span.x, top, span.end(), top + h);
                    // Chips are opaque, so their text keeps its contrast whatever is behind
                    // the strip; the visible one is lighter and carries the zone's colour.
                    if t.active {
                        pen.fill_round(chip, 6.0 * s, panel);
                        pen.fill_round(
                            Area::new(span.x + 4.0 * s, top, span.end() - 4.0 * s, top + 2.5 * s),
                            1.25 * s,
                            zone,
                        );
                    } else {
                        pen.fill_round(chip, 6.0 * s, panel);
                        pen.stroke_round(chip, 6.0 * s, 1.0 * s, Rgba(muted.0, muted.1, muted.2, 0.35));
                    }
                    let weight = if t.active {
                        Weight::SemiBold
                    } else {
                        Weight::Normal
                    };
                    pen.text(
                        &t.title,
                        Area::new(span.x + pad, top, span.end() - pad, top + h),
                        text,
                        weight,
                        Align::Leading,
                        if t.active { ink } else { muted },
                    )?;
                }
                if let Some((span, hidden)) = layout.more {
                    let chip = Area::new(span.x, top, span.end(), top + h);
                    pen.fill_round(chip, 6.0 * s, panel);
                    pen.stroke_round(chip, 6.0 * s, 1.0 * s, Rgba(muted.0, muted.1, muted.2, 0.35));
                    pen.text(
                        &format!("+{hidden}"),
                        chip,
                        text,
                        Weight::SemiBold,
                        Align::Center,
                        ink,
                    )?;
                }
                if let Some(span) = layout.drop {
                    let slot = Area::new(span.x, top, span.end(), top + h);
                    pen.fill_round(slot, 6.0 * s, panel);
                    pen.dash_round(slot, 6.0 * s, 1.6 * s, saffron);
                    pen.text(
                        drop_label,
                        Area::new(span.x + 4.0 * s, top, span.end() - 4.0 * s, top + h),
                        text,
                        Weight::SemiBold,
                        Align::Center,
                        saffron,
                    )?;
                }
                Ok(())
            })?;
            self.window.present(band.origin(), 255)?;
            // Clicks map to what is on screen: the new places count once the new picture is up.
            self.placed = Placed {
                layout,
                ids: tabs.iter().map(|t| t.id).collect(),
                top,
                bottom: top + h,
            };
            self.drawn = Some(want);
        } else if self.window.shown_at().is_none() {
            self.window.present(band.origin(), 255)?;
        } else {
            self.window.move_to(band.origin());
        }
        restack(&[&self.window], stack);
        Ok(())
    }

    /// Hides the strip. A press on it ends (the engine gets `Cancel`): a strip that is not on
    /// screen cannot be dragged.
    pub fn hide(&mut self) {
        self.release_press();
        self.window.hide();
    }

    /// Hides the strip but keeps a press on it: its group's visible tab is being dragged by the
    /// strip's band, and the strip still takes the mouse until the button goes up.
    pub fn hide_keeping_press(&mut self) {
        self.window.hide();
    }

    fn release_press(&self) {
        let bar = self.window.hwnd().0 as isize;
        if CAPTURED.with(Cell::get) == Some(bar) {
            CAPTURED.with(|c| c.set(None));
            push(BarInput::Cancel { bar });
            // SAFETY: gives the mouse back.
            let _ = unsafe { ReleaseCapture() };
        }
    }

    pub fn is_shown(&self) -> bool {
        self.window.shown_at().is_some()
    }
}

impl Drop for TabBar {
    fn drop(&mut self) {
        self.release_press();
    }
}
