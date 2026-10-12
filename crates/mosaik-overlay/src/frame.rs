//! What Mosaïk draws on screen: the coloured ring and tag a window wears in its zone, and the
//! zones shown while a window is dragged.

use mosaik_core::geometry::{Insets, Point, Rect};
use mosaik_core::zone::{Rgb, ZoneColor};
use mosaik_win::error::Result;

use crate::canvas::{Canvas, Rgba};
use crate::layered::{LayeredWindow, Stack, restack_windows};
use crate::shapes::{Ring, RingLook, Tag, TagLook};
use crate::tint::{Corners, Tint};
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
    pub tag_at: TagPlace,
    /// The band above the window that is its own (its tag, or its group's strip, sits in it):
    /// inside its ring with the window, so the two read as one. `None` when it wears its tag in
    /// its own title bar.
    pub band: Option<Rect>,
    /// The band is filled here (a single window's), not by its group's strip.
    pub fill_band: bool,
    /// Mosaïk's dark theme, or Windows' dark mode.
    pub dark: bool,
}

/// The colour of a band above a window: solid, as a title bar is (job 0076: the owner asked for
/// no see-through gap above a window), close to Windows' own title bars.
pub fn band_color(dark: bool) -> Rgb {
    if dark {
        Rgb(0x11, 0x14, 0x1B)
    } else {
        Rgb(0xED, 0xEF, 0xEA)
    }
}

/// Where a window's tag goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagPlace {
    /// At the right end of this free stretch of the window's own title bar.
    TitleBar(Rect),
    /// Above the window, in this band at the top of its zone: also when the window keeps a
    /// height of its own that runs into the band (an app's minimum size), the tag stays in its
    /// zone.
    Band(Rect),
    /// Right above the window.
    Above,
}

/// The width a zone's tag needs to show all of `label` on a monitor at `dpi`.
pub fn tag_width(canvas: &mut Canvas, label: &str, dpi: u32) -> i32 {
    crate::shapes::tag_width(canvas, label, scale(dpi))
}

/// The ring around a window in a zone and its tag, in its title bar or above it. Both sit right
/// above the window in the z-order (and below anything above it), so they are covered by whatever
/// covers the window and are not darkened by its shadow. The tag lets clicks through: pressing
/// on it grabs the title bar under it.
pub struct Frame {
    ring: Ring,
    tag: Tag,
    /// The band's fill, made on first use and kept.
    band: Option<Tint>,
}

impl Frame {
    pub fn new() -> Result<Self> {
        Ok(Self {
            ring: Ring::new("MosaikFrame", false)?,
            tag: Tag::new("MosaikTag", false)?,
            band: None,
        })
    }

    /// Shows the frame around a window whose visible frame is `frame`.
    pub fn show(&mut self, canvas: &mut Canvas, frame: Rect, look: &FrameLook, window: isize) -> Result<()> {
        let s = scale(look.dpi);
        let width = px(2.0 * s).max(1);
        let corner = if look.rounded { 8.0 * s } else { 0.0 };
        let stack = Stack::Above(window);
        // The band, over the window's width and down to its top edge (a window whose app keeps
        // a height that runs into the band keeps all of its own title bar).
        let band = look.band.and_then(|b| {
            Rect::new(
                frame.left.max(b.left),
                b.top,
                frame.right.min(b.right),
                frame.top.min(b.bottom),
            )
            .intersection(b)
        });
        match band.filter(|_| look.fill_band) {
            Some(b) => {
                if self.band.is_none() {
                    self.band = Some(Tint::band()?);
                }
                if let Some(t) = &mut self.band {
                    t.show(b, band_color(look.dark), 255, (px(corner), Corners::Top), stack)?;
                }
            }
            None => {
                if let Some(t) = &mut self.band {
                    t.hide();
                }
            }
        }
        if look.ring {
            let ring = RingLook {
                color: Rgba::of(look.color.fill(), 1.0),
                width,
                radius: corner + f(width),
            };
            let around = band.map_or(frame, |b| frame.union(b));
            self.ring
                .show(canvas, around.outset(Insets::uniform(width)), ring, stack)?;
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
            // 25 px up from the window's top edge, or the band's bottom: 3 px below the band's
            // top, 4 px above the window.
            let above = px(25.0 * s);
            match look.tag_at {
                TagPlace::TitleBar(room) => {
                    let width = self.tag.width(canvas, &tag, room.width());
                    let at = Point::new(room.right - width, room.top + (room.height() - tag.height()) / 2);
                    self.tag.show(canvas, at, &tag, room.width(), stack)?;
                }
                TagPlace::Band(band) => {
                    // Right above the window, as when it sits below the band; never higher than
                    // the band, so a window whose app keeps a height that runs into the band has
                    // its tag in the band, over its top edge. A little in from the band's left
                    // end, as a strip's tag is.
                    let left = frame.left.max(band.left) + px(4.0 * s);
                    let at = Point::new(left, frame.top.max(band.bottom) - above);
                    let room = (band.right - left).min(frame.width());
                    self.tag.show(canvas, at, &tag, room, stack)?;
                }
                TagPlace::Above => {
                    let at = Point::new(frame.left, frame.top - above);
                    self.tag.show(canvas, at, &tag, frame.width(), stack)?;
                }
            }
        } else {
            self.tag.hide();
        }
        // Each piece stacked itself as it showed; in one step now, the tag in front of the band.
        self.restack(window);
        Ok(())
    }

    /// Where the tag is on screen, when it shows.
    pub fn tag_rect(&self) -> Option<Rect> {
        self.tag.window().shown_at()
    }

    /// Puts the frame back right above its window (after the window came to the front): the
    /// tag in front, then the ring, then the band.
    pub fn restack(&self, window: isize) {
        let mut all: Vec<HWND> = std::iter::once(self.tag.window())
            .chain(self.ring.windows())
            .filter(|w| w.shown_at().is_some())
            .map(LayeredWindow::hwnd)
            .collect();
        all.extend(self.band.as_ref().filter(|t| t.is_shown()).map(Tint::hwnd));
        restack_windows(&all, Stack::Above(window));
    }

    pub fn hide(&mut self) {
        self.ring.hide();
        self.tag.hide();
        if let Some(t) = &mut self.band {
            t.hide();
        }
    }

    /// Whether `hwnd` is this frame's band (the mouse on it is named by its window).
    pub fn band_is(&self, hwnd: isize) -> bool {
        self.band.as_ref().is_some_and(|t| t.is(hwnd))
    }

    pub fn is_shown(&self) -> bool {
        self.ring.is_shown()
            || self.tag.window().shown_at().is_some()
            || self.band.as_ref().is_some_and(Tint::is_shown)
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
            t.show(
                outline,
                shown.view.color.fill(),
                TINT_ALPHA,
                (px(8.0 * s), Corners::All),
                stack,
            )?;
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
