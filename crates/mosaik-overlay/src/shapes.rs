//! The two shapes every overlay is made of: a ring (a rounded outline drawn as four thin
//! windows, so a frame around a large window costs a few kilobytes instead of a full-size bitmap)
//! and a tag (a pill with a name in it).

use mosaik_core::geometry::{Point, Rect};
use mosaik_win::error::Result;

use crate::canvas::{Align, Area, Canvas, Rgba, Weight};
use crate::layered::{LayeredWindow, Stack, restack};

/// Pixel lengths are whole; fractional DIPs are rounded to the nearest pixel.
fn px(v: f32) -> i32 {
    // Overlay sizes are small and positive.
    #[allow(clippy::cast_possible_truncation)]
    let r = v.round() as i32;
    r
}

fn size(v: i32) -> usize {
    usize::try_from(v.max(1)).unwrap_or(1)
}

#[allow(clippy::cast_precision_loss)]
fn f(v: i32) -> f32 {
    v as f32
}

/// How a ring looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingLook {
    pub color: Rgba,
    /// Line width in pixels.
    pub width: i32,
    /// Radius of the outer corners in pixels.
    pub radius: f32,
}

/// A rounded outline drawn inside `outer`, as four windows: top and bottom carry the corners,
/// left and right are straight.
pub struct Ring {
    parts: [LayeredWindow; 4],
    drawn: Option<(i32, i32, RingLook)>,
    at: Option<Rect>,
}

impl Ring {
    pub fn new(class: &str, topmost: bool) -> Result<Self> {
        Ok(Self {
            parts: [
                LayeredWindow::create(class, true, topmost)?,
                LayeredWindow::create(class, true, topmost)?,
                LayeredWindow::create(class, true, topmost)?,
                LayeredWindow::create(class, true, topmost)?,
            ],
            drawn: None,
            at: None,
        })
    }

    /// The four parts' rectangles for a ring inside `outer`: top, bottom, left, right.
    fn layout(outer: Rect, look: RingLook) -> [Rect; 4] {
        let corner = px(look.radius.max(0.0)).max(look.width).min(outer.height() / 2);
        [
            Rect::new(outer.left, outer.top, outer.right, outer.top + corner),
            Rect::new(outer.left, outer.bottom - corner, outer.right, outer.bottom),
            Rect::new(
                outer.left,
                outer.top + corner,
                outer.left + look.width,
                outer.bottom - corner,
            ),
            Rect::new(
                outer.right - look.width,
                outer.top + corner,
                outer.right,
                outer.bottom - corner,
            ),
        ]
    }

    pub fn show(&mut self, canvas: &mut Canvas, outer: Rect, look: RingLook, stack: Stack) -> Result<()> {
        if outer.width() <= 2 * look.width || outer.height() <= 2 * look.width {
            self.hide();
            return Ok(());
        }
        let rects = Self::layout(outer, look);
        let redraw = self.drawn != Some((outer.width(), outer.height(), look));
        if redraw {
            for (part, r) in self.parts.iter_mut().zip(rects) {
                if r.is_empty() {
                    part.hide();
                    continue;
                }
                let surface = part.surface(size(r.width()), size(r.height()))?;
                // The whole ring, positioned so this part's window shows its own slice of it.
                let whole = Area::new(
                    f(outer.left - r.left),
                    f(outer.top - r.top),
                    f(outer.right - r.left),
                    f(outer.bottom - r.top),
                );
                canvas.render(surface, |pen| {
                    pen.stroke_round(whole, look.radius, f(look.width), look.color);
                    Ok(())
                })?;
                part.present(r.origin(), 255)?;
            }
            self.drawn = Some((outer.width(), outer.height(), look));
        } else {
            for (part, r) in self.parts.iter_mut().zip(rects) {
                if r.is_empty() {
                    continue;
                }
                // Hidden since it was drawn: show the bitmap it still has.
                if part.shown_at().is_none() {
                    part.present(r.origin(), 255)?;
                } else {
                    part.move_to(r.origin());
                }
            }
        }
        self.at = Some(outer);
        self.restack(stack);
        Ok(())
    }

    pub fn restack(&self, stack: Stack) {
        let parts: Vec<&LayeredWindow> = self.parts.iter().collect();
        restack(&parts, stack);
    }

    pub fn windows(&self) -> impl Iterator<Item = &LayeredWindow> {
        self.parts.iter()
    }

    pub fn hide(&mut self) {
        for p in &mut self.parts {
            p.hide();
        }
        self.at = None;
    }

    pub fn is_shown(&self) -> bool {
        self.at.is_some()
    }
}

/// How a tag looks, in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct TagLook {
    pub text: String,
    pub fill: Rgba,
    pub ink: Rgba,
    /// Scale factor of the monitor (1.0 at 100%).
    pub scale: f32,
}

impl TagLook {
    /// Text size and side padding at 100%, as on mosaïk.com.
    const TEXT: f32 = 11.5;
    const PAD: f32 = 11.0;

    pub fn height(&self) -> i32 {
        px(f(mosaik_core::zone::CHIP_HEIGHT) * self.scale)
    }
}

/// The width a tag needs to show all of `text` at `scale` (1.0 at 100%).
pub fn tag_width(canvas: &mut Canvas, text: &str, scale: f32) -> i32 {
    px(canvas.text_width(text, TagLook::TEXT * scale, Weight::SemiBold) + 2.0 * TagLook::PAD * scale)
}

/// A pill-shaped name tag.
pub struct Tag {
    window: LayeredWindow,
    drawn: Option<(TagLook, i32)>,
    /// The width the text needs, for the look it was measured for: measuring lays the text out,
    /// which is not worth doing every time the window moves.
    natural: Option<(String, u32, i32)>,
}

impl Tag {
    pub fn new(class: &str, topmost: bool) -> Result<Self> {
        Ok(Self {
            window: LayeredWindow::create(class, true, topmost)?,
            drawn: None,
            natural: None,
        })
    }

    /// How wide the tag is drawn for `look`, at most `max_width` pixels.
    pub fn width(&mut self, canvas: &mut Canvas, look: &TagLook, max_width: i32) -> i32 {
        // Scales are a few hundred percent at most.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let scale_key = (look.scale * 100.0).round() as u32;
        let natural = match &self.natural {
            Some((text, key, width)) if *text == look.text && *key == scale_key => *width,
            _ => {
                let width = tag_width(canvas, &look.text, look.scale);
                self.natural = Some((look.text.clone(), scale_key, width));
                width
            }
        };
        natural.min(max_width)
    }

    /// Shows the tag with its top-left corner at `at`, at most `max_width` pixels wide.
    pub fn show(
        &mut self,
        canvas: &mut Canvas,
        at: Point,
        look: &TagLook,
        max_width: i32,
        stack: Stack,
    ) -> Result<()> {
        let text_size = TagLook::TEXT * look.scale;
        let pad = TagLook::PAD * look.scale;
        let width = self.width(canvas, look, max_width);
        let height = look.height();
        if width < height || look.text.trim().is_empty() {
            self.hide();
            return Ok(());
        }
        if self.drawn.as_ref() != Some(&(look.clone(), width)) {
            let surface = self.window.surface(size(width), size(height))?;
            canvas.render(surface, |pen| {
                let all = Area::new(0.0, 0.0, f(width), f(height));
                pen.fill_round(all, f(height) / 2.0, look.fill);
                pen.text(
                    &look.text,
                    Area::new(pad, 0.0, f(width) - pad, f(height)),
                    text_size,
                    Weight::SemiBold,
                    Align::Center,
                    look.ink,
                )
            })?;
            self.window.present(at, 255)?;
            self.drawn = Some((look.clone(), width));
        } else if self.window.shown_at().is_none() {
            self.window.present(at, 255)?;
        } else {
            self.window.move_to(at);
        }
        restack(&[&self.window], stack);
        Ok(())
    }

    pub fn window(&self) -> &LayeredWindow {
        &self.window
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }
}
