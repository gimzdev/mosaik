//! Geometry in physical pixels and as ratios of a reference rectangle.
//!
//! Pixel coordinates are virtual-screen coordinates, the space Win32 uses for a Per-Monitor-V2
//! DPI-aware process: the primary monitor starts at (0, 0) and other monitors may sit at
//! negative coordinates. Rectangles follow the Win32 convention: left and top are inside,
//! right and bottom are one past the last pixel.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Windows' reference DPI: 96 dots per inch is 100% scale.
pub const BASE_DPI: u32 = 96;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

/// Distances from each edge of a rectangle, for example a window's invisible resize borders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const ZERO: Self = Self::uniform(0);

    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn uniform(n: i32) -> Self {
        Self::new(n, n, n, n)
    }

    /// The insets that take `outer` to `inner`, edge by edge. Negative when `inner` sticks out.
    pub const fn between(outer: Rect, inner: Rect) -> Self {
        Self {
            left: inner.left.saturating_sub(outer.left),
            top: inner.top.saturating_sub(outer.top),
            right: outer.right.saturating_sub(inner.right),
            bottom: outer.bottom.saturating_sub(inner.bottom),
        }
    }
}

/// A rectangle in physical pixels: left and top inclusive, right and bottom exclusive (Win32 `RECT`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn from_origin_size(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self::new(x, y, x.saturating_add(width), y.saturating_add(height))
    }

    pub const fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub const fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    pub const fn size(self) -> Size {
        Size::new(self.width(), self.height())
    }

    pub const fn origin(self) -> Point {
        Point::new(self.left, self.top)
    }

    pub const fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    pub const fn area(self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.width() as i64 * self.height() as i64
        }
    }

    pub const fn contains(self, p: Point) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }

    /// True when `other` lies entirely inside this rectangle. An empty `other` is never contained.
    pub const fn contains_rect(self, other: Rect) -> bool {
        !other.is_empty()
            && other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn intersection(self, other: Rect) -> Option<Rect> {
        let r = Rect::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        (!r.is_empty()).then_some(r)
    }

    pub fn overlap_area(self, other: Rect) -> i64 {
        self.intersection(other).map_or(0, Rect::area)
    }

    /// The smallest rectangle containing both. Empty rectangles are ignored.
    pub fn union(self, other: Rect) -> Rect {
        match (self.is_empty(), other.is_empty()) {
            (true, _) => other,
            (_, true) => self,
            _ => Rect::new(
                self.left.min(other.left),
                self.top.min(other.top),
                self.right.max(other.right),
                self.bottom.max(other.bottom),
            ),
        }
    }

    /// The centre pixel, rounded toward the top-left.
    pub const fn center(self) -> Point {
        Point::new(
            self.left + self.width().div_euclid(2),
            self.top + self.height().div_euclid(2),
        )
    }

    pub const fn offset(self, dx: i32, dy: i32) -> Rect {
        Rect::new(
            self.left.saturating_add(dx),
            self.top.saturating_add(dy),
            self.right.saturating_add(dx),
            self.bottom.saturating_add(dy),
        )
    }

    /// Shrinks the rectangle by `insets` (grows it for negative insets).
    pub const fn inset(self, insets: Insets) -> Rect {
        Rect::new(
            self.left.saturating_add(insets.left),
            self.top.saturating_add(insets.top),
            self.right.saturating_sub(insets.right),
            self.bottom.saturating_sub(insets.bottom),
        )
    }

    /// Grows the rectangle by `insets`.
    pub const fn outset(self, insets: Insets) -> Rect {
        Rect::new(
            self.left.saturating_sub(insets.left),
            self.top.saturating_sub(insets.top),
            self.right.saturating_add(insets.right),
            self.bottom.saturating_add(insets.bottom),
        )
    }

    /// Squared distance from `p` to the nearest pixel of the rectangle; 0 inside.
    pub fn distance_squared_to(self, p: Point) -> i64 {
        let dx = if p.x < self.left {
            i64::from(self.left) - i64::from(p.x)
        } else if p.x >= self.right {
            i64::from(p.x) - i64::from(self.right) + 1
        } else {
            0
        };
        let dy = if p.y < self.top {
            i64::from(self.top) - i64::from(p.y)
        } else if p.y >= self.bottom {
            i64::from(p.y) - i64::from(self.bottom) + 1
        } else {
            0
        };
        dx * dx + dy * dy
    }

    /// Moves the rectangle the least distance needed to lie inside `bounds`, shrinking it first
    /// if it is wider or taller than `bounds`. The result is always inside `bounds` when
    /// `bounds` is not empty.
    pub fn fit_inside(self, bounds: Rect) -> Rect {
        if bounds.is_empty() {
            return self;
        }
        let width = self.width().clamp(1, bounds.width());
        let height = self.height().clamp(1, bounds.height());
        let left = self.left.clamp(bounds.left, bounds.right - width);
        let top = self.top.clamp(bounds.top, bounds.bottom - height);
        Rect::from_origin_size(left, top, width, height)
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GeometryError {
    #[error("a ratio must be a finite number between 0 and 1, got {0}")]
    RatioOutOfRange(f64),
    #[error("a ratio rectangle needs left < right and top < bottom")]
    RatioInverted,
    #[error("the reference rectangle is empty")]
    EmptyReference,
}

/// A rectangle as fractions of a reference rectangle (usually a monitor's work area), edge by edge.
///
/// Storing edges rather than origin and size means two zones that share an edge fraction land
/// on exactly the same pixel on every monitor, so tiled zones never gain gaps or overlaps when
/// the resolution or scale changes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RatioRectFields", into = "RatioRectFields")]
pub struct RatioRect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

#[derive(Serialize, Deserialize)]
struct RatioRectFields {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl TryFrom<RatioRectFields> for RatioRect {
    type Error = GeometryError;
    fn try_from(f: RatioRectFields) -> Result<Self, Self::Error> {
        RatioRect::new(f.left, f.top, f.right, f.bottom)
    }
}

impl From<RatioRect> for RatioRectFields {
    fn from(r: RatioRect) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

impl RatioRect {
    /// The whole reference rectangle.
    pub const FULL: RatioRect = RatioRect {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
    };

    pub fn new(left: f64, top: f64, right: f64, bottom: f64) -> Result<Self, GeometryError> {
        for v in [left, top, right, bottom] {
            if !v.is_finite() || !(0.0..=1.0).contains(&v) {
                return Err(GeometryError::RatioOutOfRange(v));
            }
        }
        if left >= right || top >= bottom {
            return Err(GeometryError::RatioInverted);
        }
        Ok(Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub const fn left(self) -> f64 {
        self.left
    }
    pub const fn top(self) -> f64 {
        self.top
    }
    pub const fn right(self) -> f64 {
        self.right
    }
    pub const fn bottom(self) -> f64 {
        self.bottom
    }

    /// Expresses `rect` as fractions of `reference`. The part of `rect` outside `reference` is
    /// cut off first, so the result always describes an area inside the reference.
    pub fn from_rect(rect: Rect, reference: Rect) -> Result<Self, GeometryError> {
        if reference.is_empty() {
            return Err(GeometryError::EmptyReference);
        }
        let inside = rect.intersection(reference).ok_or(GeometryError::RatioInverted)?;
        let w = f64::from(reference.width());
        let h = f64::from(reference.height());
        Self::new(
            f64::from(inside.left - reference.left) / w,
            f64::from(inside.top - reference.top) / h,
            f64::from(inside.right - reference.left) / w,
            f64::from(inside.bottom - reference.top) / h,
        )
    }

    /// The pixel rectangle these fractions describe inside `reference`. Each edge is rounded on
    /// its own, so shared edges stay shared. Can be empty when `reference` is tiny.
    pub fn to_rect(self, reference: Rect) -> Rect {
        let w = f64::from(reference.width());
        let h = f64::from(reference.height());
        Rect::new(
            reference.left.saturating_add(round_to_i32(self.left * w)),
            reference.top.saturating_add(round_to_i32(self.top * h)),
            reference.left.saturating_add(round_to_i32(self.right * w)),
            reference.top.saturating_add(round_to_i32(self.bottom * h)),
        )
    }

    pub fn width(self) -> f64 {
        self.right - self.left
    }

    pub fn height(self) -> f64 {
        self.bottom - self.top
    }

    pub fn contains_point(self, x: f64, y: f64) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Rounds half away from zero and saturates at the `i32` range.
pub fn round_to_i32(v: f64) -> i32 {
    if v.is_nan() {
        return 0;
    }
    let r = v.round();
    if r >= f64::from(i32::MAX) {
        i32::MAX
    } else if r <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        // The value is a whole number inside the i32 range, so the cast is exact.
        #[allow(clippy::cast_possible_truncation)]
        let n = r as i32;
        n
    }
}

/// Converts a length in device-independent pixels (96 DPI) to physical pixels at `dpi`,
/// rounding like Win32 `MulDiv`.
pub fn scale_for_dpi(logical: i32, dpi: u32) -> i32 {
    mul_div(logical, dpi, BASE_DPI)
}

/// Converts a length in physical pixels at `dpi` to device-independent pixels (96 DPI).
pub fn unscale_for_dpi(physical: i32, dpi: u32) -> i32 {
    mul_div(physical, BASE_DPI, dpi)
}

fn mul_div(value: i32, numerator: u32, denominator: u32) -> i32 {
    if denominator == 0 {
        return value;
    }
    let n = i64::from(value) * i64::from(numerator);
    let d = i64::from(denominator);
    // Round half away from zero, as MulDiv does.
    let q = if n >= 0 { (n + d / 2) / d } else { (n - d / 2) / d };
    i32::try_from(q).unwrap_or(if q > 0 { i32::MAX } else { i32::MIN })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rect_basics() {
        let r = Rect::from_origin_size(-1920, 0, 1920, 1080);
        assert_eq!(r.width(), 1920);
        assert_eq!(r.height(), 1080);
        assert!(r.contains(Point::new(-1920, 0)));
        assert!(!r.contains(Point::new(0, 0)), "right edge is exclusive");
        assert_eq!(r.center(), Point::new(-960, 540));
        assert_eq!(r.area(), 1920 * 1080);
        assert!(Rect::new(5, 5, 5, 10).is_empty());
    }

    #[test]
    fn intersection_and_union() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 150, 150);
        assert_eq!(a.intersection(b), Some(Rect::new(50, 50, 100, 100)));
        assert_eq!(
            a.intersection(Rect::new(100, 0, 200, 100)),
            None,
            "touching edges do not overlap"
        );
        assert_eq!(a.union(b), Rect::new(0, 0, 150, 150));
        assert_eq!(Rect::default().union(b), b);
    }

    #[test]
    fn insets_between_invisible_borders() {
        // A window rect with 7 px invisible borders on the left, right and bottom (Windows 10/11 at 100%).
        let window = Rect::new(93, 100, 1007, 707);
        let frame = Rect::new(100, 100, 1000, 700);
        let insets = Insets::between(window, frame);
        assert_eq!(insets, Insets::new(7, 0, 7, 7));
        assert_eq!(frame.outset(insets), window);
        assert_eq!(window.inset(insets), frame);
    }

    #[test]
    fn fit_inside_moves_and_shrinks() {
        let bounds = Rect::new(0, 0, 1920, 1040);
        assert_eq!(
            Rect::new(1800, 900, 2100, 1200).fit_inside(bounds),
            Rect::new(1620, 740, 1920, 1040)
        );
        assert_eq!(Rect::new(-500, -500, 3000, 3000).fit_inside(bounds), bounds);
        let neg = Rect::new(-2560, -300, 0, 1140);
        assert_eq!(
            Rect::new(-3000, 1000, -2000, 1500).fit_inside(neg),
            Rect::new(-2560, 640, -1560, 1140)
        );
    }

    #[test]
    fn distance_to_rect() {
        let r = Rect::new(0, 0, 10, 10);
        assert_eq!(r.distance_squared_to(Point::new(5, 5)), 0);
        assert_eq!(r.distance_squared_to(Point::new(-3, 5)), 9);
        assert_eq!(
            r.distance_squared_to(Point::new(10, 10)),
            2,
            "the bottom-right corner pixel is (9, 9)"
        );
    }

    #[test]
    fn ratio_rect_rejects_bad_values() {
        assert!(RatioRect::new(0.5, 0.0, 0.5, 1.0).is_err());
        assert!(RatioRect::new(-0.1, 0.0, 0.5, 1.0).is_err());
        assert!(RatioRect::new(0.0, 0.0, f64::NAN, 1.0).is_err());
        assert!(serde_json::from_str::<RatioRect>(r#"{"left":0.6,"top":0,"right":0.4,"bottom":1}"#).is_err());
    }

    #[test]
    fn shared_edges_stay_shared() {
        let left = RatioRect::new(0.0, 0.0, 0.6, 1.0).unwrap();
        let right = RatioRect::new(0.6, 0.0, 1.0, 1.0).unwrap();
        for work in [
            Rect::new(0, 0, 1920, 1032),
            Rect::new(-1366, 0, 0, 728),
            Rect::new(3840, -217, 6400, 1223),
            Rect::new(0, 0, 1081, 1913),
        ] {
            let a = left.to_rect(work);
            let b = right.to_rect(work);
            assert_eq!(a.right, b.left, "no gap or overlap on {work:?}");
            assert_eq!(a.left, work.left);
            assert_eq!(b.right, work.right);
        }
    }

    #[test]
    fn dpi_scaling_matches_muldiv() {
        assert_eq!(scale_for_dpi(48, 96), 48);
        assert_eq!(scale_for_dpi(48, 120), 60);
        assert_eq!(scale_for_dpi(48, 144), 72);
        assert_eq!(scale_for_dpi(7, 120), 9, "8.75 rounds to 9");
        assert_eq!(scale_for_dpi(-7, 120), -9);
        assert_eq!(unscale_for_dpi(72, 144), 48);
    }

    fn arb_reference() -> impl Strategy<Value = Rect> {
        (-8000i32..8000, -8000i32..8000, 1i32..8000, 1i32..8000)
            .prop_map(|(x, y, w, h)| Rect::from_origin_size(x, y, w, h))
    }

    proptest! {
        #[test]
        fn ratio_round_trip_is_exact(reference in arb_reference(), a in 0.0f64..1.0, b in 0.0f64..1.0, c in 0.0f64..1.0, d in 0.0f64..1.0) {
            let (l, r) = if a < b { (a, b) } else { (b, a) };
            let (t, bt) = if c < d { (c, d) } else { (d, c) };
            prop_assume!(r - l > 1e-9 && bt - t > 1e-9);
            let rect = RatioRect::new(l, t, r, bt).unwrap().to_rect(reference);
            prop_assume!(!rect.is_empty());
            // Pixels -> ratios -> pixels on the same reference gives back the same pixels.
            let again = RatioRect::from_rect(rect, reference).unwrap().to_rect(reference);
            prop_assert_eq!(again, rect);
        }

        #[test]
        fn to_rect_stays_inside_reference(reference in arb_reference(), a in 0.0f64..=1.0, b in 0.0f64..=1.0) {
            let (l, r) = if a < b { (a, b) } else { (b, a) };
            prop_assume!(r > l);
            let rect = RatioRect::new(l, 0.0, r, 1.0).unwrap().to_rect(reference);
            prop_assert!(rect.left >= reference.left && rect.right <= reference.right);
            prop_assert!(rect.top == reference.top && rect.bottom == reference.bottom);
        }

        #[test]
        fn fit_inside_always_inside(bounds in arb_reference(), x in -20000i32..20000, y in -20000i32..20000, w in 1i32..20000, h in 1i32..20000) {
            let r = Rect::from_origin_size(x, y, w, h).fit_inside(bounds);
            prop_assert!(bounds.contains_rect(r), "{:?} not inside {:?}", r, bounds);
        }

        #[test]
        fn ratio_survives_json(l in 0.0f64..0.5, r in 0.5f64..=1.0) {
            prop_assume!(r > l);
            let rr = RatioRect::new(l, l, r, r).unwrap();
            let json = serde_json::to_string(&rr).unwrap();
            let back: RatioRect = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, rr);
        }
    }
}
