//! What Mosaïk draws on screen: the coloured ring and tag a window wears in its zone, and the
//! zones shown while a window is dragged.

use mosaik_core::geometry::{Insets, Point, Rect};
use mosaik_core::zone::ZoneColor;
use mosaik_win::error::Result;

use crate::canvas::{Canvas, Rgba};
use crate::layered::{LayeredWindow, Stack, restack, restack_windows};
use crate::shapes::{Ring, RingLook, Tag, TagLook};
use crate::tint::Tint;
use windows::Win32::Foundation::HWND;

fn scale(dpi: u32) -> f32 {
    // DPI values are small (96 to 480).
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

#[allow(clippy::cast_precision_loss)]
fn f(v: i32) -> f32 {
    v as f32
}

/// How a window's frame looks.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameLook {
    pub color: ZoneColor,
    /// The zone's name, shown in the tag.
    pub label: String,
    /// DPI of the monitor the window is on.
    pub dpi: u32,
    /// Windows 11 rounds window corners (8 px at 100%); Windows 10 does not.
    pub rounded: bool,
    pub ring: bool,
    pub tag: bool,
}

/// The ring around a window in a zone and the tag above it. Both sit right above the window in
/// the z-order (and below anything above it), so they are covered by whatever covers the window
/// and are not darkened by its shadow.
pub struct Frame {
    ring: Ring,
    tag: Tag,
}

impl Frame {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ring: Ring::new("MosaikFrame", false)?,
            tag: Tag::new("MosaikTag", false)?,
        })
    }

    /// Shows the frame around a window whose visible frame is `frame`.
    pub fn show(&mut self, canvas: &mut Canvas, frame: Rect, look: &FrameLook, window: isize) -> Result<()> {
        let s = scale(look.dpi);
        let width = px(2.0 * s).max(1);
        let corner = if look.rounded { 8.0 * s } else { 0.0 };
        let stack = Stack::Above(window);
        if look.ring {
            let ring = RingLook {
                color: Rgba::of(look.color.fill(), 1.0),
                width,
                radius: corner + f(width),
            };
            self.ring
                .show(canvas, frame.outset(Insets::uniform(width)), ring, stack)?;
        } else {
            self.ring.hide();
        }
        if look.tag {
            let tag = TagLook {
                text: look.label.clone(),
                fill: Rgba::of(look.color.fill(), 1.0),
                ink: Rgba::of(look.color.label(), 1.0),
                scale: s,
            };
            let at = Point::new(frame.left, frame.top - px(25.0 * s));
            self.tag.show(canvas, at, &tag, frame.width(), stack)?;
        } else {
            self.tag.hide();
        }
        Ok(())
    }

    /// Puts the frame back right above its window (after the window came to the front).
    pub fn restack(&self, window: isize) {
        let mut all: Vec<&LayeredWindow> = vec![self.tag.window()];
        all.extend(self.ring.windows());
        restack(&all, Stack::Above(window));
    }

    pub fn hide(&mut self) {
        self.ring.hide();
        self.tag.hide();
    }

    pub fn is_shown(&self) -> bool {
        self.ring.is_shown() || self.tag.window().shown_at().is_some()
    }
}

/// A zone as the drag preview shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneView {
    pub rect: Rect,
    pub color: ZoneColor,
    pub name: String,
    pub dpi: u32,
}

struct Shown {
    view: ZoneView,
    ring: Ring,
    tag: Tag,
}

/// The zones of the open workspace while a window is dragged: an outline and name for each,
/// and the zone the window would go to tinted with its colour and outlined more strongly.
#[derive(Default)]
pub struct Preview {
    zones: Vec<Shown>,
    /// The drop target's tint, made on first use and kept.
    tint: Option<Tint>,
    hot: Option<usize>,
    stack: Option<Stack>,
}

/// Opacity of the drop target's tint: 22%, as on mosaïk.com.
const TINT_ALPHA: u8 = 56;

impl Preview {
    pub fn new() -> Self {
        Self::default()
    }

    fn outline(v: &ZoneView) -> (Rect, f32) {
        let s = scale(v.dpi);
        (v.rect.inset(Insets::uniform(px(4.0 * s))), s)
    }

    fn ring_look(v: &ZoneView, hot: bool) -> RingLook {
        let s = scale(v.dpi);
        RingLook {
            color: Rgba::of(v.color.fill(), if hot { 1.0 } else { 0.9 }),
            width: px(if hot { 3.0 } else { 2.0 } * s).max(1),
            radius: 8.0 * s,
        }
    }

    /// Shows every zone, stacked per `stack` (right below the dragged window).
    pub fn show(&mut self, canvas: &mut Canvas, zones: &[ZoneView], stack: Stack) -> Result<()> {
        while self.zones.len() < zones.len() {
            self.zones.push(Shown {
                view: zones[self.zones.len()].clone(),
                ring: Ring::new("MosaikZoneRing", true)?,
                tag: Tag::new("MosaikZoneTag", true)?,
            });
        }
        if let Some(t) = &mut self.tint {
            t.hide();
        }
        for (i, shown) in self.zones.iter_mut().enumerate() {
            let Some(v) = zones.get(i) else {
                shown.ring.hide();
                shown.tag.hide();
                continue;
            };
            shown.view = v.clone();
            let (outline, s) = Self::outline(v);
            shown
                .ring
                .show(canvas, outline, Self::ring_look(v, false), stack)?;
            let tag = TagLook {
                text: v.name.clone(),
                fill: Rgba::of(v.color.fill(), 1.0),
                ink: Rgba::of(v.color.label(), 1.0),
                scale: s,
            };
            let at = Point::new(outline.left + px(10.0 * s), outline.top + px(10.0 * s));
            shown
                .tag
                .show(canvas, at, &tag, outline.width() - px(20.0 * s), stack)?;
        }
        self.stack = Some(stack);
        self.hot = None;
        self.restack();
        Ok(())
    }

    /// Marks the zone at `index` as the drop target (or none).
    pub fn set_hot(&mut self, canvas: &mut Canvas, index: Option<usize>) -> Result<()> {
        if index == self.hot {
            return Ok(());
        }
        let Some(stack) = self.stack else { return Ok(()) };
        if let Some(old) = self.hot.and_then(|i| self.zones.get_mut(i)) {
            let (outline, _) = Self::outline(&old.view);
            let look = Self::ring_look(&old.view, false);
            old.ring.show(canvas, outline, look, stack)?;
        }
        self.hot = index;
        let Some(shown) = index.and_then(|i| self.zones.get_mut(i)) else {
            if let Some(t) = &mut self.tint {
                t.hide();
            }
            return Ok(());
        };
        let (outline, s) = Self::outline(&shown.view);
        let look = Self::ring_look(&shown.view, true);
        shown.ring.show(canvas, outline, look, stack)?;
        if self.tint.is_none() {
            self.tint = Some(Tint::new()?);
        }
        if let Some(t) = &mut self.tint {
            t.show(outline, shown.view.color.fill(), TINT_ALPHA, px(8.0 * s), stack)?;
        }
        self.restack();
        Ok(())
    }

    /// Tags on top, then outlines, then the tint, all right below the dragged window.
    fn restack(&self) {
        let Some(stack) = self.stack else { return };
        let mut all: Vec<HWND> = Vec::new();
        for z in &self.zones {
            if let Some(w) = Some(z.tag.window()).filter(|w| w.shown_at().is_some()) {
                all.push(w.hwnd());
            }
        }
        for z in &self.zones {
            all.extend(
                z.ring
                    .windows()
                    .filter(|w| w.shown_at().is_some())
                    .map(LayeredWindow::hwnd),
            );
        }
        if let Some(t) = self.tint.as_ref().filter(|t| t.is_shown()) {
            all.push(t.hwnd());
        }
        restack_windows(&all, stack);
    }

    /// Hides everything.
    pub fn hide(&mut self) {
        for z in &mut self.zones {
            z.ring.hide();
            z.tag.hide();
        }
        if let Some(t) = &mut self.tint {
            t.hide();
        }
        self.hot = None;
        self.stack = None;
    }

    pub fn hot(&self) -> Option<usize> {
        self.hot
    }
}
