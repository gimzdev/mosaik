//! Rasterising rounded rectangles with anti-aliased edges into premultiplied BGRA pixels.
//! Portable and tested everywhere; the Windows side only copies the pixels to the screen.

use mosaik_core::zone::Rgb;

/// A premultiplied BGRA pixel as `UpdateLayeredWindow` expects (0xAARRGGBB in memory order
/// B, G, R, A).
pub fn premultiply(Rgb(r, g, b): Rgb, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let ch = |c: u8| -> u32 {
        // In 0..=255 by construction.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (f32::from(c) * a).round() as u32;
        v.min(255)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a8 = (a * 255.0).round() as u32;
    (a8 << 24) | (ch(r) << 16) | (ch(g) << 8) | ch(b)
}

/// A bitmap of premultiplied BGRA pixels, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u32>,
}

impl Pixels {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> u32 {
        self.data[y * self.width + x]
    }

    /// Composites `color` (premultiplied) over the pixel with the given coverage.
    fn blend(&mut self, x: usize, y: usize, color: u32, coverage: f32) {
        if coverage <= 0.0 {
            return;
        }
        let i = y * self.width + x;
        let src = scale(color, coverage);
        let dst = self.data[i];
        let sa = src >> 24;
        // Porter-Duff "over" on premultiplied channels: out = src + dst * (1 - src_alpha).
        let inv = 255 - sa;
        let mix = |shift: u32| -> u32 {
            let s = (src >> shift) & 0xFF;
            let d = (dst >> shift) & 0xFF;
            (s + (d * inv + 127) / 255).min(255)
        };
        self.data[i] = (mix(24) << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0);
    }

    /// Fills a rounded rectangle with `color`.
    pub fn fill(&mut self, shape: RoundRect, color: u32) {
        self.shape(shape, None, color);
    }

    /// Strokes the inside edge of a rounded rectangle with a band `width` pixels wide.
    pub fn stroke(&mut self, shape: RoundRect, width: f32, color: u32) {
        self.shape(shape, Some(width), color);
    }

    fn shape(&mut self, s: RoundRect, stroke: Option<f32>, color: u32) {
        let r = s
            .radius
            .max(0.0)
            .min((s.right - s.left) / 2.0)
            .min((s.bottom - s.top) / 2.0);
        let shape = RoundRect { radius: r, ..s };
        // Pixel rows and columns the shape can touch.
        let clamp = |v: f32, max: usize| -> usize {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let i = v.max(0.0) as usize;
            i.min(max)
        };
        let (x0, x1) = (
            clamp(s.left.floor(), self.width),
            clamp(s.right.ceil(), self.width),
        );
        let (y0, y1) = (
            clamp(s.top.floor(), self.height),
            clamp(s.bottom.ceil(), self.height),
        );
        for y in y0..y1 {
            #[allow(clippy::cast_precision_loss)]
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                #[allow(clippy::cast_precision_loss)]
                let px = x as f32 + 0.5;
                let d = shape.distance(px, py);
                let coverage = match stroke {
                    None => (0.5 - d).clamp(0.0, 1.0),
                    // Inside band: covered between the outer edge (d = 0) and d = -width.
                    Some(w) => (0.5 - d).clamp(0.0, 1.0) - (0.5 - (d + w)).clamp(0.0, 1.0),
                };
                self.blend(x, y, color, coverage);
            }
        }
    }
}

/// A rectangle with rounded corners, in pixel units (may be fractional).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub radius: f32,
}

impl RoundRect {
    /// Signed distance from a point to the edge: negative inside.
    fn distance(self, px: f32, py: f32) -> f32 {
        let cx = (self.left + self.right) / 2.0;
        let cy = (self.top + self.bottom) / 2.0;
        let hx = (self.right - self.left) / 2.0 - self.radius;
        let hy = (self.bottom - self.top) / 2.0 - self.radius;
        let qx = (px - cx).abs() - hx;
        let qy = (py - cy).abs() - hy;
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        outside + qx.max(qy).min(0.0) - self.radius
    }
}

fn scale(color: u32, coverage: f32) -> u32 {
    if coverage >= 1.0 {
        return color;
    }
    let ch = |shift: u32| -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (((color >> shift) & 0xFF) as f32 * coverage).round() as u32;
        v.min(255)
    };
    (ch(24) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// The zone preview shown while dragging: a translucent fill of the zone colour with a solid
/// border, corners matching Windows 11 (8 px at 100%).
pub fn zone_highlight(width: usize, height: usize, color: Rgb, scale: f32) -> Pixels {
    let mut p = Pixels::new(width, height);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    let radius = 8.0 * scale;
    let shape = RoundRect {
        left: 0.0,
        top: 0.0,
        right: w,
        bottom: h,
        radius,
    };
    p.fill(shape, premultiply(color, 0.22));
    p.stroke(shape, (2.0 * scale).max(1.0), premultiply(color, 1.0));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_values() {
        assert_eq!(premultiply(Rgb(255, 255, 255), 1.0), 0xFFFF_FFFF);
        assert_eq!(premultiply(Rgb(0x2B, 0x44, 0xD9), 0.0), 0);
        let half = premultiply(Rgb(200, 100, 50), 0.5);
        assert_eq!(half >> 24, 128);
        assert_eq!((half >> 16) & 0xFF, 100);
    }

    #[test]
    fn highlight_has_solid_border_translucent_inside_and_clear_corners() {
        let p = zone_highlight(100, 60, Rgb(0x2B, 0x44, 0xD9), 1.0);
        assert_eq!(
            p.get(0, 0) >> 24,
            0,
            "the corner outside the radius is transparent"
        );
        assert_eq!(p.get(50, 0) >> 24, 255, "the border is opaque");
        let inside = p.get(50, 30) >> 24;
        assert!(
            (50..=60).contains(&inside),
            "the fill is about 22% opaque, got {inside}"
        );
        // Premultiplied: no channel exceeds alpha.
        for &px in &p.data {
            let a = px >> 24;
            assert!((px >> 16) & 0xFF <= a && (px >> 8) & 0xFF <= a && px & 0xFF <= a);
        }
    }
}
