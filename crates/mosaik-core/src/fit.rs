//! Zones that make room for their windows.
//!
//! A window that will not get as small as its zone (an app's minimum size: Battle.net keeps about
//! 1280 x 720; or a window of a fixed size) gets a zone just large enough for it, and the zones
//! beside it give up the room, as little of it as they can. The window does not take a larger
//! part of the layout than it needs: the lines between zones move only as far as the window
//! needs them to.
//!
//! Zones that tile a monitor in rows and columns (every layout Mosaïk makes, and most drawn by
//! hand) are seen as a tree of cuts: a monitor cut into columns, each column cut into rows, and
//! so on. Each part keeps its share of the room it is cut from, unless one inside it needs more:
//! then it takes what it needs from the parts beside it, in proportion to what each can spare.
//! When a layout can be cut either way first (four quarters: two columns of two, or two rows of
//! two), the way that moves the zones least is kept.

use crate::geometry::{RatioRect, Size};

/// A zone gives up room down to this size in pixels (or keeps all of its room, when it has
/// less), unless its window needs more: a window whose app sets no minimum stays usable.
pub const FLOOR: Size = Size::new(320, 200);

/// Pixels a window may be larger than its zone by, for rounding.
const SLACK: f64 = 2.0;

/// Positions closer than this are the same line, in pixels.
const SAME: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Px {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    /// Cut into columns.
    X,
    /// Cut into rows.
    Y,
}

impl Px {
    fn start(self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.left,
            Axis::Y => self.top,
        }
    }

    fn end(self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.right,
            Axis::Y => self.bottom,
        }
    }

    fn length(self, axis: Axis) -> f64 {
        self.end(axis) - self.start(axis)
    }

    /// The same rectangle from `start` to `end` along `axis`.
    fn along(self, axis: Axis, start: f64, end: f64) -> Px {
        match axis {
            Axis::X => Px {
                left: start,
                right: end,
                ..self
            },
            Axis::Y => Px {
                top: start,
                bottom: end,
                ..self
            },
        }
    }

    /// How far this rectangle's edges are from `other`'s, added up.
    fn distance(self, other: Px) -> f64 {
        (self.left - other.left).abs()
            + (self.top - other.top).abs()
            + (self.right - other.right).abs()
            + (self.bottom - other.bottom).abs()
    }
}

struct Room<'a> {
    /// The zones as they are, in pixels.
    rects: Vec<Px>,
    /// The least each zone may be, in pixels.
    least: &'a [(f64, f64)],
}

impl Room<'_> {
    fn least(&self, zone: usize, axis: Axis) -> f64 {
        let (w, h) = self.least[zone];
        match axis {
            Axis::X => w,
            Axis::Y => h,
        }
    }

    /// The groups the zones `ids` (inside `bounds`) fall into when cut along `axis` at every
    /// line that runs right across them, in order, each with where it starts and ends. Fewer
    /// than two when no line runs right across.
    fn cut(&self, ids: &[usize], bounds: Px, axis: Axis) -> Vec<(Vec<usize>, f64, f64)> {
        let mut lines: Vec<f64> = ids
            .iter()
            .map(|&i| self.rects[i].end(axis))
            .filter(|&p| p > bounds.start(axis) + SAME && p < bounds.end(axis) - SAME)
            .filter(|&p| {
                ids.iter().all(|&i| {
                    let r = self.rects[i];
                    r.end(axis) <= p + SAME || r.start(axis) >= p - SAME
                })
            })
            .collect();
        lines.sort_by(f64::total_cmp);
        lines.dedup_by(|a, b| (*a - *b).abs() <= SAME);
        if lines.is_empty() {
            return Vec::new();
        }
        let mut edges = vec![bounds.start(axis)];
        edges.extend(lines);
        edges.push(bounds.end(axis));
        edges
            .windows(2)
            .map(|w| {
                let (from, to) = (w[0], w[1]);
                let group: Vec<usize> = ids
                    .iter()
                    .copied()
                    .filter(|&i| {
                        let r = self.rects[i];
                        r.start(axis) >= from - SAME && r.end(axis) <= to + SAME
                    })
                    .collect();
                (group, from, to)
            })
            .collect()
    }

    /// The least length along `axis` the zones `ids` (inside `bounds`) can be given.
    fn least_length(&self, ids: &[usize], bounds: Px, axis: Axis) -> Option<f64> {
        if let [one] = ids {
            return Some(self.least(*one, axis));
        }
        let mut best: Option<f64> = None;
        for by in [Axis::X, Axis::Y] {
            let groups = self.cut(ids, bounds, by);
            if groups.len() < 2 {
                continue;
            }
            let mut parts = Vec::with_capacity(groups.len());
            for (group, from, to) in &groups {
                parts.push(self.least_length(group, bounds.along(by, *from, *to), axis)?);
            }
            // Side by side along `axis`, the parts add up; across it, the largest decides.
            let length = if by == axis {
                parts.iter().sum()
            } else {
                parts.iter().copied().fold(0.0, f64::max)
            };
            best = Some(best.map_or(length, |b: f64| b.min(length)));
        }
        best
    }

    /// The zones `ids`, which fill `was`, laid out in `now`: each its share, the ones that need
    /// more taking it from their neighbours. Returns each zone's new place and how far the zones
    /// moved in all, or `None` when they cannot all fit.
    fn place(&self, ids: &[usize], was: Px, now: Px) -> Option<(Vec<(usize, Px)>, f64)> {
        if let [one] = ids {
            let fits = self.least(*one, Axis::X) <= now.length(Axis::X) + SLACK
                && self.least(*one, Axis::Y) <= now.length(Axis::Y) + SLACK;
            return fits.then(|| (vec![(*one, now)], now.distance(self.rects[*one])));
        }
        let mut best: Option<(Vec<(usize, Px)>, f64)> = None;
        for axis in [Axis::X, Axis::Y] {
            let groups = self.cut(ids, was, axis);
            if groups.len() < 2 {
                continue;
            }
            let Some(placed) = self.place_cut(&groups, was, now, axis) else {
                continue;
            };
            if best.as_ref().is_none_or(|(_, moved)| placed.1 < *moved) {
                best = Some(placed);
            }
        }
        best
    }

    /// [`Room::place`] for zones cut into `groups` along `axis`.
    fn place_cut(
        &self,
        groups: &[(Vec<usize>, f64, f64)],
        was: Px,
        now: Px,
        axis: Axis,
    ) -> Option<(Vec<(usize, Px)>, f64)> {
        let scale = now.length(axis) / was.length(axis).max(SAME);
        let wanted: Vec<f64> = groups.iter().map(|(_, from, to)| (to - from) * scale).collect();
        let mut least = Vec::with_capacity(groups.len());
        for (group, from, to) in groups {
            least.push(self.least_length(group, was.along(axis, *from, *to), axis)?);
        }
        let lengths = share_out(&wanted, &least, now.length(axis))?;
        let mut out = Vec::new();
        let mut moved = 0.0;
        let mut at = now.start(axis);
        for (k, (group, from, to)) in groups.iter().enumerate() {
            let end = if k + 1 == groups.len() {
                now.end(axis)
            } else {
                at + lengths[k]
            };
            let (placed, m) = self.place(group, was.along(axis, *from, *to), now.along(axis, at, end))?;
            out.extend(placed);
            moved += m;
            at = end;
        }
        Some((out, moved))
    }
}

/// Lengths for parts that want `wanted` and need at least `least`, adding up to `total`: each
/// part short of its least gets it, and the others give up the difference in proportion to what
/// each can spare. `None` when the leasts add up to more than `total`.
fn share_out(wanted: &[f64], least: &[f64], total: f64) -> Option<Vec<f64>> {
    let needed: f64 = least.iter().sum();
    if needed > total + SLACK {
        return None;
    }
    let mut lengths: Vec<f64> = wanted.iter().zip(least).map(|(w, l)| w.max(*l)).collect();
    let over: f64 = lengths.iter().sum::<f64>() - total;
    if over > 0.0 {
        let spare: Vec<f64> = lengths.iter().zip(least).map(|(l, m)| (l - m).max(0.0)).collect();
        let all: f64 = spare.iter().sum();
        if all <= 0.0 {
            // Every part at its least, and they fill the room within the slack.
            return Some(lengths);
        }
        let taken = over.min(all);
        for (length, s) in lengths.iter_mut().zip(&spare) {
            *length -= taken * s / all;
        }
    }
    Some(lengths)
}

/// The zones `areas` of a `width` x `height` work area, each at least `least[i]` pixels (its
/// window's least size; zero for none), moved as little as can be: a zone too small for its
/// window grows to fit it, and the zones beside it give up the room, down to [`FLOOR`]. Zones
/// that already fit are returned as they are. `None` when the windows cannot all fit, or the
/// zones do not tile the work area in rows and columns.
pub fn make_room(areas: &[RatioRect], least: &[Size], width: i32, height: i32) -> Option<Vec<RatioRect>> {
    let (w, h) = (f64::from(width), f64::from(height));
    if areas.is_empty() || w <= 0.0 || h <= 0.0 {
        return Some(areas.to_vec());
    }
    let rects: Vec<Px> = areas
        .iter()
        .map(|a| Px {
            left: a.left() * w,
            top: a.top() * h,
            right: a.right() * w,
            bottom: a.bottom() * h,
        })
        .collect();
    let need = |i: usize| least.get(i).copied().unwrap_or_default();
    let fits = |i: usize| {
        let (n, r) = (need(i), rects[i]);
        f64::from(n.width) <= r.length(Axis::X) + SLACK && f64::from(n.height) <= r.length(Axis::Y) + SLACK
    };
    if (0..rects.len()).all(fits) {
        return Some(areas.to_vec());
    }
    // Each zone's least: its window's, and otherwise as much of its room as the floor keeps.
    let floors: Vec<(f64, f64)> = rects
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let n = need(i);
            (
                f64::from(n.width).max(r.length(Axis::X).min(f64::from(FLOOR.width))),
                f64::from(n.height).max(r.length(Axis::Y).min(f64::from(FLOOR.height))),
            )
        })
        .collect();
    let room = Room {
        rects,
        least: &floors,
    };
    let screen = Px {
        left: 0.0,
        top: 0.0,
        right: w,
        bottom: h,
    };
    let ids: Vec<usize> = (0..areas.len()).collect();
    let (placed, _) = room.place(&ids, screen, screen)?;
    let mut out = areas.to_vec();
    for (i, p) in placed {
        out[i] = RatioRect::new(
            (p.left / w).clamp(0.0, 1.0),
            (p.top / h).clamp(0.0, 1.0),
            (p.right / w).clamp(0.0, 1.0),
            (p.bottom / h).clamp(0.0, 1.0),
        )
        .ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::presets::Preset;
    use proptest::prelude::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1032);

    fn px(areas: &[RatioRect]) -> Vec<Rect> {
        areas.iter().map(|a| a.to_rect(WORK)).collect()
    }

    /// The areas cover the work area once, without overlapping (within rounding).
    fn tiles(areas: &[RatioRect]) -> bool {
        let total: f64 = areas.iter().map(|a| a.width() * a.height()).sum();
        let overlap = areas.iter().enumerate().any(|(i, a)| {
            areas.iter().skip(i + 1).any(|b| {
                let w = a.right().min(b.right()) - a.left().max(b.left());
                let h = a.bottom().min(b.bottom()) - a.top().max(b.top());
                w > 1e-9 && h > 1e-9
            })
        });
        (total - 1.0).abs() < 1e-9 && !overlap
    }

    fn fits(r: Rect, s: Size) -> bool {
        r.width() + 2 >= s.width && r.height() + 2 >= s.height
    }

    #[test]
    fn a_quarter_grows_just_enough_for_battle_net_and_the_others_make_room() {
        // The owner: "instead of letting it overlap or minimising it, reorganise the size of the
        // zones; it's the whole point of the app. Not becoming the main zone, just taking the
        // minimal space." Battle.net keeps 1280 x 720; a quarter of a 1080p screen is 960 x 516.
        let quarters = Preset::Quarters.zones();
        let mut least = vec![Size::default(); 4];
        least[0] = Size::new(1280, 720);
        let fitted = make_room(&quarters, &least, 1920, 1032).unwrap();
        let r = px(&fitted);
        assert!(tiles(&fitted), "{r:?}");
        assert!(fits(r[0], least[0]), "{r:?}");
        // Just enough: 1280 x 720, not the main part of the screen.
        assert!(
            (r[0].width() - 1280).abs() <= 2 && (r[0].height() - 720).abs() <= 2,
            "{r:?}"
        );
        // The others keep at least the floor.
        for z in &r[1..] {
            assert!(z.width() >= 320 && z.height() >= 200, "{r:?}");
        }
        // The way that moves the zones least: the top row grows to 720 px, the bottom row keeps
        // both of its quarters as wide as they were.
        assert_eq!((r[2].width(), r[3].width()), (960, 960), "{r:?}");
    }

    #[test]
    fn zones_that_fit_are_left_as_they_are() {
        let thirds = Preset::Thirds.zones();
        let least = [Size::new(500, 400), Size::new(600, 900), Size::default()];
        assert_eq!(make_room(&thirds, &least, 1920, 1032), Some(thirds.clone()));
        assert_eq!(make_room(&thirds, &[], 1920, 1032), Some(thirds));
    }

    #[test]
    fn a_third_grows_and_the_other_two_share_what_is_left() {
        let thirds = Preset::Thirds.zones();
        let least = [Size::default(), Size::new(1100, 600), Size::default()];
        let fitted = make_room(&thirds, &least, 1920, 1032).unwrap();
        let r = px(&fitted);
        assert!(tiles(&fitted));
        assert!((r[1].width() - 1100).abs() <= 2, "{r:?}");
        // The other two give up the same room: 410 px each.
        assert!((r[0].width() - r[2].width()).abs() <= 1, "{r:?}");
    }

    #[test]
    fn windows_that_cannot_all_fit_are_said_so() {
        // Two windows of 1280 px side by side on 1920 px.
        let halves = Preset::Halves.zones();
        let least = [Size::new(1280, 0), Size::new(1280, 0)];
        assert_eq!(make_room(&halves, &least, 1920, 1032), None);
        // A window taller than the screen.
        let least = [Size::new(0, 1200), Size::default()];
        assert_eq!(make_room(&halves, &least, 1920, 1032), None);
        // A neighbour keeps its floor: 1700 px leaves it 220, under 320.
        let least = [Size::new(1700, 0), Size::default()];
        assert_eq!(make_room(&halves, &least, 1920, 1032), None);
    }

    #[test]
    fn a_side_column_makes_room_for_a_tall_window_and_the_main_zone_does_not_move() {
        // The main zone and three stacked beside it: the first beside it needs 600 px of height.
        let areas = vec![
            RatioRect::new(0.0, 0.0, 0.55, 1.0).unwrap(),
            RatioRect::new(0.55, 0.0, 1.0, 1.0 / 3.0).unwrap(),
            RatioRect::new(0.55, 1.0 / 3.0, 1.0, 2.0 / 3.0).unwrap(),
            RatioRect::new(0.55, 2.0 / 3.0, 1.0, 1.0).unwrap(),
        ];
        let least = [
            Size::default(),
            Size::new(0, 600),
            Size::default(),
            Size::default(),
        ];
        let fitted = make_room(&areas, &least, 1920, 1032).unwrap();
        let r = px(&fitted);
        assert!(tiles(&fitted));
        assert_eq!(fitted[0], areas[0], "the main zone stays");
        assert!((r[1].height() - 600).abs() <= 2, "{r:?}");
        assert!((r[2].height() - r[3].height()).abs() <= 1, "{r:?}");
    }

    #[test]
    fn zones_drawn_as_a_pinwheel_are_left_alone() {
        // Four zones turning around a middle one: no line runs right across them.
        let areas = vec![
            RatioRect::new(0.0, 0.0, 0.6, 0.4).unwrap(),
            RatioRect::new(0.6, 0.0, 1.0, 0.6).unwrap(),
            RatioRect::new(0.4, 0.6, 1.0, 1.0).unwrap(),
            RatioRect::new(0.0, 0.4, 0.4, 1.0).unwrap(),
            RatioRect::new(0.4, 0.4, 0.6, 0.6).unwrap(),
        ];
        let least = [
            Size::new(1500, 0),
            Size::default(),
            Size::default(),
            Size::default(),
            Size::default(),
        ];
        assert_eq!(make_room(&areas, &least, 1920, 1032), None);
    }

    proptest! {
        /// Whatever window needs room in whatever layout, the zones still tile the screen, and
        /// when they fit, every zone fits its window.
        #[test]
        fn zones_that_make_room_still_tile_the_screen(
            preset in 0usize..10,
            zone in 0usize..4,
            w in 0i32..1900,
            h in 0i32..1100,
        ) {
            let p = Preset::ALL[preset % Preset::ALL.len()];
            let areas = p.zones();
            let mut least = vec![Size::default(); areas.len()];
            let z = zone % areas.len();
            least[z] = Size::new(w, h);
            if let Some(fitted) = make_room(&areas, &least, 1920, 1032) {
                prop_assert!(tiles(&fitted), "{:?}", fitted);
                let r = px(&fitted);
                prop_assert!(fits(r[z], least[z]), "{:?} for {:?}", r, least[z]);
                for (i, a) in r.iter().enumerate() {
                    if i != z {
                        let was = areas[i].to_rect(WORK);
                        prop_assert!(a.width() + 2 >= was.width().min(320) && a.height() + 2 >= was.height().min(200), "{:?}", r);
                    }
                }
            } else {
                // Every layout leaves a zone at least 60% of the screen once its neighbours give
                // up their room: only a window larger than that is refused.
                prop_assert!(w > 1152 || h > 619, "{w} x {h} in {:?}", p);
            }
        }
    }
}
