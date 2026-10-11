//! Zones: named, coloured areas of a monitor that belong to a workspace.
//!
//! A zone is stored as a fraction of its monitor's work area, so it keeps its shape when the
//! resolution, the scale or the taskbar changes. Inside the zone, the window sits below a band
//! that carries the zone's tag, with a small gap between neighbouring zones.

use serde::{Deserialize, Serialize};

use crate::geometry::{Insets, RatioRect, Rect, scale_for_dpi};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ZoneId(pub String);

impl ZoneId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// The zone colours: the Mosaïk palette. Each has a label colour with at least 4.5:1 contrast.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneColor {
    Lapis,
    Verdigris,
    Saffron,
    Madder,
    Slate,
}

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl ZoneColor {
    pub const ALL: [ZoneColor; 5] = [
        ZoneColor::Lapis,
        ZoneColor::Verdigris,
        ZoneColor::Saffron,
        ZoneColor::Madder,
        ZoneColor::Slate,
    ];

    pub const fn fill(self) -> Rgb {
        match self {
            ZoneColor::Lapis => Rgb(0x2B, 0x44, 0xD9),
            ZoneColor::Verdigris => Rgb(0x12, 0x78, 0x6A),
            ZoneColor::Saffron => Rgb(0xF4, 0xB1, 0x3A),
            ZoneColor::Madder => Rgb(0xC9, 0x34, 0x50),
            ZoneColor::Slate => Rgb(0x55, 0x60, 0x70),
        }
    }

    /// Text drawn on the fill: ink on saffron, white on the darker colours.
    pub const fn label(self) -> Rgb {
        match self {
            ZoneColor::Saffron => Rgb(0x16, 0x1B, 0x25),
            _ => Rgb(0xFF, 0xFF, 0xFF),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            ZoneColor::Lapis => "Lapis",
            ZoneColor::Verdigris => "Verdigris",
            ZoneColor::Saffron => "Saffron",
            ZoneColor::Madder => "Madder",
            ZoneColor::Slate => "Slate",
        }
    }

    /// The colour a new zone gets: the palette in order, so neighbours differ.
    pub fn nth(n: usize) -> ZoneColor {
        Self::ALL[n % Self::ALL.len()]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: ZoneId,
    /// Shown as the tag on every window in the zone, e.g. "Code".
    pub name: String,
    pub color: ZoneColor,
    /// Index into the layout's remembered monitors.
    pub monitor: usize,
    /// The zone as a fraction of that monitor's work area.
    pub area: RatioRect,
}

/// How a window sits inside its zone, in device-independent pixels (scaled by the monitor's DPI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameStyle {
    /// Space between neighbouring zones.
    pub gap: i32,
    /// Height of the band above the window that carries the tag. 0 shows no tags.
    pub tag_band: i32,
    /// Width of the coloured ring around the window.
    pub border: i32,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            gap: 8,
            tag_band: 28,
            border: 2,
        }
    }
}

impl FrameStyle {
    pub const fn without_tags(self) -> Self {
        Self { tag_band: 0, ..self }
    }

    /// Where the window's visible frame goes inside a zone on a monitor at `dpi`.
    pub fn window_area(self, zone: Rect, dpi: u32) -> Rect {
        let half_gap = scale_for_dpi(self.gap, dpi) / 2;
        let band = scale_for_dpi(self.tag_band, dpi);
        let area = zone.inset(Insets::uniform(half_gap));
        let area = area.inset(Insets::new(0, band, 0, 0));
        if area.is_empty() { zone } else { area }
    }

    /// The tag band of a zone: where the tag (or the tab strip) is drawn.
    pub fn tag_area(self, zone: Rect, dpi: u32) -> Option<Rect> {
        if self.tag_band <= 0 {
            return None;
        }
        let window = self.window_area(zone, dpi);
        let band = Rect::new(
            window.left,
            zone.top + scale_for_dpi(self.gap, dpi) / 2,
            window.right,
            window.top,
        );
        (!band.is_empty()).then_some(band)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_area_leaves_room_for_the_tag() {
        let zone = Rect::new(0, 0, 960, 1032);
        let style = FrameStyle::default();
        assert_eq!(style.window_area(zone, 96), Rect::new(4, 32, 956, 1028));
        assert_eq!(style.tag_area(zone, 96), Some(Rect::new(4, 4, 956, 32)));
        // At 200% everything doubles.
        assert_eq!(
            style.window_area(Rect::new(0, 0, 1920, 2064), 192),
            Rect::new(8, 64, 1912, 2056)
        );
        assert_eq!(style.without_tags().tag_area(zone, 96), None);
    }

    #[test]
    fn neighbours_get_a_full_gap() {
        let style = FrameStyle::default();
        let a = style.window_area(Rect::new(0, 0, 960, 1032), 96);
        let b = style.window_area(Rect::new(960, 0, 1920, 1032), 96);
        assert_eq!(b.left - a.right, 8);
    }

    #[test]
    fn tiny_zone_keeps_its_rect() {
        let tiny = Rect::new(0, 0, 20, 20);
        assert_eq!(FrameStyle::default().window_area(tiny, 96), tiny);
    }

    #[test]
    fn colours_have_readable_labels() {
        // WCAG 2.1 contrast ratio between fill and label must be at least 4.5:1 (AA, small text).
        fn lum(Rgb(r, g, b): Rgb) -> f64 {
            let ch = |c: u8| {
                let c = f64::from(c) / 255.0;
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
        }
        for c in ZoneColor::ALL {
            let (a, b) = (lum(c.fill()), lum(c.label()));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(ratio >= 4.5, "{} label contrast {ratio:.2}", c.name());
        }
    }
}
