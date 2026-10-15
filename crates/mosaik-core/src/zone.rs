//! Zones: numbered, coloured areas of a monitor that belong to a workspace.
//!
//! A zone is stored as a fraction of its monitor's work area, so it keeps its shape when the
//! resolution, the scale or the taskbar changes. The window fills its zone, edge to edge with its
//! neighbours, and wears the zone's tag (its number) in its own title bar, or sits below a band
//! that carries the tag when tags are set to go above windows. A tab group sits below a band
//! with its strip either way (ADR 0027).

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
/// A value missing from a settings file takes its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrameStyle {
    /// Space between neighbouring windows (half of it at the screen's edges). None by default:
    /// windows meet edge to edge, and no desktop shows between them.
    pub gap: i32,
    /// Height of the band above the window that carries the tag. 0 shows no tags.
    pub tag_band: i32,
    /// Width of the coloured ring around the window, when rings are shown.
    pub border: i32,
    /// Tags go in each window's own title bar (`crate::titlebar`): windows alone fill their
    /// zones; a tab group sits below a band for its strip (`for_tabs`). Set from the settings
    /// for the session; not saved with the style.
    #[serde(skip)]
    pub title_bars: bool,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            gap: 0,
            tag_band: 28,
            border: 2,
            title_bars: false,
        }
    }
}

/// The height of a tag and of a tab's chip at 100%.
pub const CHIP_HEIGHT: i32 = 21;

/// The room at each end of a tag's number, at 100%.
pub const TAG_PAD: i32 = 11;

/// The room at each end of a tag's number drawn compact, at 100%.
pub const TAG_PAD_COMPACT: i32 = 3;

/// How a zone's tag fits the room it has. A tag shows its zone's number, all of it or nothing:
/// zone 12 cut to "1" would name another zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagCut {
    /// With room at each end, this wide.
    Whole(i32),
    /// Without that room, in a chip at least as wide as it is tall, this wide: a title bar
    /// full of a browser's tabs keeps a short stretch to be grabbed by.
    Compact(i32),
    /// No room: no tag.
    Hidden,
}

impl TagCut {
    /// How wide the tag is drawn (0 when it is not).
    pub const fn width(self) -> i32 {
        match self {
            TagCut::Whole(w) | TagCut::Compact(w) => w,
            TagCut::Hidden => 0,
        }
    }
}

/// How wide a tag `natural` pixels wide whole is drawn compact, on a monitor at `dpi`: the least
/// room it is drawn in.
pub fn compact_width(natural: i32, dpi: u32) -> i32 {
    let px = |v: i32| scale_for_dpi(v, dpi);
    (natural - 2 * px(TAG_PAD) + 2 * px(TAG_PAD_COMPACT)).max(px(CHIP_HEIGHT))
}

/// How a tag `natural` pixels wide whole fits in `room` pixels, on a monitor at `dpi`: whole,
/// compact, or not at all.
pub fn tag_cut(natural: i32, room: i32, dpi: u32) -> TagCut {
    if natural <= 0 {
        return TagCut::Hidden;
    }
    if natural <= room {
        return TagCut::Whole(natural);
    }
    let compact = compact_width(natural, dpi);
    if compact <= room {
        TagCut::Compact(compact)
    } else {
        TagCut::Hidden
    }
}

/// The least band height a tab strip needs.
pub const TAB_STRIP_BAND: i32 = 24;

impl FrameStyle {
    pub const fn without_tags(self) -> Self {
        Self { tag_band: 0, ..self }
    }

    /// The style a tab group's windows are placed with: below a band with room for the tab
    /// strip, always, since the strip is the way to the group's other tabs: when tags are
    /// hidden, and when tags go in title bars (ADR 0027: a crowded title bar left the strip room
    /// for the zone's tag and "+1" only, with no tab to drag out).
    pub fn for_tabs(self) -> Self {
        let tag_band = if self.tag_band >= TAB_STRIP_BAND {
            self.tag_band
        } else {
            Self::default().tag_band
        };
        Self {
            tag_band,
            title_bars: false,
            ..self
        }
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

/// The band a window wears its tag (or its group's strip) in, above it, given its zone's band
/// `zone_band`: the zone's band for a window right below it, which is every window Mosaïk put in
/// its zone (`place::fit_size` sets windows of their own size at the zone's top too); right
/// above a window that sits lower in its zone (one the user or its app moved down, or one placed
/// by an earlier version), as tall and as wide as the window. A window that runs into the zone's
/// band (an app that keeps a height of its own) keeps the zone's band, over its top edge, so the
/// tag stays in its zone.
pub fn band_above(frame: Rect, zone_band: Rect) -> Rect {
    if frame.top <= zone_band.bottom {
        return zone_band;
    }
    Rect::new(
        frame.left.max(zone_band.left),
        frame.top - zone_band.height(),
        frame.right.min(zone_band.right),
        frame.top,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_area_leaves_room_for_the_tag() {
        let zone = Rect::new(0, 0, 960, 1032);
        let style = FrameStyle::default();
        assert_eq!(style.window_area(zone, 96), Rect::new(0, 28, 960, 1032));
        assert_eq!(style.tag_area(zone, 96), Some(Rect::new(0, 0, 960, 28)));
        // At 200% everything doubles.
        assert_eq!(
            style.window_area(Rect::new(0, 0, 1920, 2064), 192),
            Rect::new(0, 56, 1920, 2064)
        );
        assert_eq!(style.without_tags().tag_area(zone, 96), None);
        // A tab group keeps its strip's band when tags are hidden.
        assert_eq!(style.without_tags().for_tabs(), style);
        assert_eq!(style.for_tabs(), style);
        // With tags in title bars, windows fill their zones; a tab group sits below a band for
        // its strip all the same (ADR 0027): in a crowded title bar the strip had room for the
        // zone's tag and "+1" only, and no tab to drag out (the owner: "we cannot drag only 1
        // tab it drags the whole window").
        let title_bars = FrameStyle {
            title_bars: true,
            ..style.without_tags()
        };
        assert_eq!(title_bars.for_tabs(), style);
        assert_eq!(
            title_bars.for_tabs().window_area(zone, 96),
            Rect::new(0, 28, 960, 1032)
        );
        assert_eq!(title_bars.window_area(zone, 96), zone);
        assert_eq!(title_bars.tag_area(zone, 96), None);
        // A space between zones: half of it on each side of a window.
        let apart = FrameStyle { gap: 8, ..title_bars };
        assert_eq!(apart.window_area(zone, 96), Rect::new(4, 4, 956, 1028));
    }

    #[test]
    fn a_band_sits_right_above_its_window() {
        let zone_band = Rect::new(4, 4, 956, 32);
        // A window below the band, filling the zone: the zone's band.
        let filling = Rect::new(4, 32, 956, 1028);
        assert_eq!(band_above(filling, zone_band), zone_band);
        // A game of a fixed size in the middle of the zone: right above it, as tall.
        let game = Rect::new(160, 300, 800, 780);
        assert_eq!(band_above(game, zone_band), Rect::new(160, 272, 800, 300));
        // Calculator, keeping a height that runs 22 px into the band: the zone's band, over its
        // top edge.
        let tall = Rect::new(4, 10, 956, 1028);
        assert_eq!(band_above(tall, zone_band), zone_band);
    }

    #[test]
    fn neighbouring_windows_meet_edge_to_edge() {
        // The owner, again: "get rid of coloured borders, we lose space; just window to window".
        // By default two windows side by side touch, and a window at the screen's edge reaches
        // it: no desktop shows anywhere.
        let style = FrameStyle {
            title_bars: true,
            ..FrameStyle::default().without_tags()
        };
        let a = style.window_area(Rect::new(0, 0, 960, 1032), 96);
        let b = style.window_area(Rect::new(960, 0, 1920, 1032), 96);
        assert_eq!((a.right, b.left), (960, 960));
        assert_eq!((a.left, a.top, b.right, b.bottom), (0, 0, 1920, 1032));
        // At 200% too.
        let a = style.window_area(Rect::new(0, 0, 1920, 2064), 192);
        let b = style.window_area(Rect::new(1920, 0, 3840, 2064), 192);
        assert_eq!(b.left - a.right, 0);
    }

    #[test]
    fn a_tag_with_little_room_shows_its_whole_number_compact_then_nothing() {
        // Zone 12's tag, 36 px wide whole, in title bars with less and less room.
        assert_eq!(tag_cut(36, 300, 96), TagCut::Whole(36));
        assert_eq!(tag_cut(36, 36, 96), TagCut::Whole(36));
        // Less: the whole number without the room at its ends, in a chip at least as wide as it
        // is tall (a browser full of tabs keeps a short stretch to be grabbed by). Never its
        // first digit alone, as tags cut to a name's initial were: "1" names another zone.
        assert_eq!(tag_cut(36, 35, 96), TagCut::Compact(21));
        assert_eq!(tag_cut(36, 21, 96), TagCut::Compact(21));
        // A wider number keeps all its digits: zone 123, 43 px wide whole, 27 compact.
        assert_eq!(tag_cut(43, 30, 96), TagCut::Compact(27));
        // Less than that: no tag.
        assert_eq!(tag_cut(36, 20, 96), TagCut::Hidden);
        assert_eq!(tag_cut(36, 0, 96), TagCut::Hidden);
        // Everything scales with the monitor.
        assert_eq!(tag_cut(72, 50, 192), TagCut::Compact(42));
        assert_eq!(tag_cut(72, 41, 192), TagCut::Hidden);
        // Nothing measured yet (no canvas): nothing to draw.
        assert_eq!(tag_cut(0, 300, 96), TagCut::Hidden);
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
