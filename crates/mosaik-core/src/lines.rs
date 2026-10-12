//! The lines between the zones of one monitor, and moving them.
//!
//! A line is every zone edge at one place on the monitor (not on its border) whose extents
//! overlap, on either side: the edge between two side-by-side zones, or between a full-height
//! zone and the two zones stacked beside it. Moving a line moves all of those edges, so zones
//! that tile stay tiled, with no gap and no overlap. The zone editor moves lines when one is
//! dragged (`apps/desktop/src/editor/model.ts` works the same way); Mosaïk moves them when the
//! user resizes a window in a zone by an edge it shares with neighbouring zones, so the zone and
//! its neighbours follow the window.

use crate::geometry::{RatioRect, Rect};

const EPSILON: f64 = 1e-6;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < EPSILON
}

/// Which way a line runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// A vertical line, moved left and right (the edge between zones side by side).
    X,
    /// A horizontal line, moved up and down (the edge between zones one above the other).
    Y,
}

/// A side of a zone, or of a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Left, Side::Top, Side::Right, Side::Bottom];

    pub const fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::X,
            Side::Top | Side::Bottom => Axis::Y,
        }
    }

    /// The side a zone's line is on the far side of: right and bottom edges are before their
    /// line, left and top edges after it.
    const fn is_end(self) -> bool {
        matches!(self, Side::Right | Side::Bottom)
    }

    /// Where this side of `r` is, in pixels.
    pub const fn of(self, r: Rect) -> i32 {
        match self {
            Side::Left => r.left,
            Side::Top => r.top,
            Side::Right => r.right,
            Side::Bottom => r.bottom,
        }
    }

    /// Where this side of `r` is, as a fraction.
    pub fn of_ratio(self, r: RatioRect) -> f64 {
        match self {
            Side::Left => r.left(),
            Side::Top => r.top(),
            Side::Right => r.right(),
            Side::Bottom => r.bottom(),
        }
    }
}

fn start(r: RatioRect, axis: Axis) -> f64 {
    match axis {
        Axis::X => r.left(),
        Axis::Y => r.top(),
    }
}

fn end(r: RatioRect, axis: Axis) -> f64 {
    match axis {
        Axis::X => r.right(),
        Axis::Y => r.bottom(),
    }
}

fn across_start(r: RatioRect, axis: Axis) -> f64 {
    match axis {
        Axis::X => r.top(),
        Axis::Y => r.left(),
    }
}

fn across_end(r: RatioRect, axis: Axis) -> f64 {
    match axis {
        Axis::X => r.bottom(),
        Axis::Y => r.right(),
    }
}

/// `r` with its start (left or top) or end (right or bottom) along `axis` at `at`; unchanged
/// when that would not be a rectangle.
fn with_edge(r: RatioRect, axis: Axis, is_end: bool, at: f64) -> RatioRect {
    let (mut l, mut t, mut rt, mut b) = (r.left(), r.top(), r.right(), r.bottom());
    match (axis, is_end) {
        (Axis::X, false) => l = at,
        (Axis::X, true) => rt = at,
        (Axis::Y, false) => t = at,
        (Axis::Y, true) => b = at,
    }
    RatioRect::new(l, t, rt, b).unwrap_or(r)
}

/// A line between zones (or between a zone and empty space) that can be moved.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub axis: Axis,
    /// Where it is, as a fraction of the work area.
    pub at: f64,
    /// Its extent along the other axis.
    pub from: f64,
    pub to: f64,
    /// The zones before it (left or above), as indices into the areas given.
    pub before: Vec<usize>,
    /// The zones after it (right or below).
    pub after: Vec<usize>,
}

struct Edge {
    zone: usize,
    before: bool,
    from: f64,
    to: f64,
}

/// The lines of one monitor's zones (`areas`): every zone edge not on the monitor's border,
/// joined with the edges at the same place whose extents overlap, on either side, however many
/// hops apart.
pub fn lines(areas: &[RatioRect]) -> Vec<Line> {
    let mut out = Vec::new();
    for axis in [Axis::X, Axis::Y] {
        let mut at_places: Vec<(f64, Vec<Edge>)> = Vec::new();
        let mut add = |at: f64, e: Edge| match at_places.iter_mut().find(|(k, _)| near(*k, at)) {
            Some((_, list)) => list.push(e),
            None => at_places.push((at, vec![e])),
        };
        for (i, a) in areas.iter().enumerate() {
            let (from, to) = (across_start(*a, axis), across_end(*a, axis));
            if end(*a, axis) < 1.0 - EPSILON {
                add(
                    end(*a, axis),
                    Edge {
                        zone: i,
                        before: true,
                        from,
                        to,
                    },
                );
            }
            if start(*a, axis) > EPSILON {
                add(
                    start(*a, axis),
                    Edge {
                        zone: i,
                        before: false,
                        from,
                        to,
                    },
                );
            }
        }
        for (at, mut list) in at_places {
            list.sort_by(|p, q| p.from.total_cmp(&q.from));
            let mut groups: Vec<Vec<Edge>> = Vec::new();
            for e in list {
                let (joined, rest): (Vec<Vec<Edge>>, Vec<Vec<Edge>>) = groups.into_iter().partition(|g| {
                    g.iter()
                        .any(|o| e.from < o.to - EPSILON && e.to > o.from + EPSILON)
                });
                groups = rest;
                let mut merged: Vec<Edge> = joined.into_iter().flatten().collect();
                merged.push(e);
                groups.push(merged);
            }
            for g in groups {
                let mut before: Vec<usize> = g.iter().filter(|e| e.before).map(|e| e.zone).collect();
                let mut after: Vec<usize> = g.iter().filter(|e| !e.before).map(|e| e.zone).collect();
                before.sort_unstable();
                after.sort_unstable();
                out.push(Line {
                    axis,
                    at,
                    from: g.iter().map(|e| e.from).fold(f64::INFINITY, f64::min),
                    to: g.iter().map(|e| e.to).fold(f64::NEG_INFINITY, f64::max),
                    before,
                    after,
                });
            }
        }
    }
    out
}

/// How far `line` can go: each zone it moves keeps `min` on that axis, and it stops at zones
/// beside it that it does not move.
pub fn line_range(areas: &[RatioRect], line: &Line, min: f64) -> (f64, f64) {
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    let axis = line.axis;
    for (i, a) in areas.iter().enumerate() {
        if line.before.contains(&i) {
            lo = lo.max(start(*a, axis) + min);
        } else if line.after.contains(&i) {
            hi = hi.min(end(*a, axis) - min);
        } else {
            let overlaps =
                across_start(*a, axis) < line.to - EPSILON && across_end(*a, axis) > line.from + EPSILON;
            if !overlaps {
                continue;
            }
            if end(*a, axis) <= line.at + EPSILON {
                lo = lo.max(end(*a, axis));
            }
            if start(*a, axis) >= line.at - EPSILON {
                hi = hi.min(start(*a, axis));
            }
        }
    }
    (lo, hi)
}

/// Moves `line` to `to` (a fraction of the work area), as far as it can go. Returns where it
/// is now.
pub fn move_line(areas: &mut [RatioRect], line: &Line, to: f64, min: f64) -> f64 {
    let (lo, hi) = line_range(areas, line, min);
    if lo > hi + EPSILON {
        return line.at;
    }
    let at = to.clamp(lo, hi.max(lo));
    if near(at, line.at) {
        return line.at;
    }
    for &i in &line.before {
        areas[i] = with_edge(areas[i], line.axis, true, at);
    }
    for &i in &line.after {
        areas[i] = with_edge(areas[i], line.axis, false, at);
    }
    at
}

/// The line that `side` of zone `zone` is on: `None` on the monitor's border.
pub fn line_of(areas: &[RatioRect], zone: usize, side: Side) -> Option<Line> {
    lines(areas).into_iter().find(|l| {
        l.axis == side.axis()
            && if side.is_end() {
                l.before.contains(&zone)
            } else {
                l.after.contains(&zone)
            }
    })
}

/// [`line_of`], carried on through the places where four zones meet: in a grid of quarters, the
/// whole middle line rather than its half beside the zone, as a spreadsheet's column moves as a
/// whole. That is what a window resized by hand moves: a grid stays a grid, and a corner can
/// move both of its lines.
pub fn grid_line_of(areas: &[RatioRect], zone: usize, side: Side) -> Option<Line> {
    let all = lines(areas);
    let mut line = all
        .iter()
        .find(|l| {
            l.axis == side.axis()
                && if side.is_end() {
                    l.before.contains(&zone)
                } else {
                    l.after.contains(&zone)
                }
        })?
        .clone();
    loop {
        let next = all.iter().find(|o| {
            o.axis == line.axis
                && near(o.at, line.at)
                && (near(o.from, line.to) || near(o.to, line.from))
                && !o
                    .before
                    .iter()
                    .chain(&o.after)
                    .any(|z| line.before.contains(z) || line.after.contains(z))
        });
        let Some(o) = next else { break };
        line.from = line.from.min(o.from);
        line.to = line.to.max(o.to);
        line.before.extend(&o.before);
        line.after.extend(&o.after);
        line.before.sort_unstable();
        line.after.sort_unstable();
    }
    Some(line)
}

/// The sides of a window that moved from `start` to `now` (visible frames), by more than
/// `threshold` pixels, with where each is now.
pub fn sides_moved(start: Rect, now: Rect, threshold: i32) -> Vec<(Side, i32)> {
    Side::ALL
        .into_iter()
        .filter(|s| (s.of(now) - s.of(start)).abs() > threshold)
        .map(|s| (s, s.of(now)))
        .collect()
}

/// A window in zone `zone` was resized by hand from `start` to `now` (its visible frame, in
/// pixels) on a monitor whose work area is `work`: each side that moved takes the line its
/// zone's side is on along by as much ([`grid_line_of`]), moving the zones on both sides of it
/// (each keeps `min`).
/// A side on the monitor's border has no line and stays. Returns the zones as they are now, and
/// whether any line moved.
pub fn follow_resize(
    areas: &[RatioRect],
    zone: usize,
    start: Rect,
    now: Rect,
    work: Rect,
    min: f64,
) -> (Vec<RatioRect>, bool) {
    let mut out = areas.to_vec();
    if zone >= out.len() || work.width() <= 0 || work.height() <= 0 {
        return (out, false);
    }
    let mut moved = false;
    for (side, at) in sides_moved(start, now, 2) {
        let delta = at - side.of(start);
        let span = match side.axis() {
            Axis::X => f64::from(work.width()),
            Axis::Y => f64::from(work.height()),
        };
        let Some(line) = grid_line_of(&out, zone, side) else {
            continue;
        };
        let to = side.of_ratio(out[zone]) + f64::from(delta) / span;
        let landed = move_line(&mut out, &line, to, min);
        moved |= !near(landed, line.at);
    }
    (out, moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::Preset;
    use proptest::prelude::*;

    const MIN: f64 = 0.05;

    fn r(l: f64, t: f64, rt: f64, b: f64) -> RatioRect {
        RatioRect::new(l, t, rt, b).unwrap()
    }

    #[test]
    fn half_and_stack_has_a_long_line_and_a_short_one() {
        let z = Preset::HalfAndStack.zones();
        let ls = lines(&z);
        assert_eq!(ls.len(), 2, "{ls:?}");
        let x = ls.iter().find(|l| l.axis == Axis::X).unwrap();
        assert_eq!(
            (x.at, x.before.clone(), x.after.clone()),
            (0.5, vec![0], vec![1, 2])
        );
        let y = ls.iter().find(|l| l.axis == Axis::Y).unwrap();
        assert_eq!((y.at, y.from, y.to), (0.5, 0.5, 1.0));
        assert_eq!((y.before.clone(), y.after.clone()), (vec![1], vec![2]));
    }

    #[test]
    fn quarters_have_half_lines_that_make_whole_grid_lines() {
        // In the editor, each half of a middle line moves on its own.
        let z = Preset::Quarters.zones();
        let ls = lines(&z);
        assert_eq!(ls.len(), 4, "{ls:?}");
        assert_eq!(line_of(&z, 0, Side::Right).unwrap().after, vec![1]);
        // A window's resize takes the whole middle line.
        let grid = grid_line_of(&z, 0, Side::Right).unwrap();
        assert_eq!(
            (grid.before.clone(), grid.after.clone()),
            (vec![0, 2], vec![1, 3])
        );
        assert_eq!((grid.from, grid.to), (0.0, 1.0));
        // Where nothing continues it, the grid line is the line.
        let h = Preset::HalfAndStack.zones();
        assert_eq!(grid_line_of(&h, 1, Side::Bottom), line_of(&h, 1, Side::Bottom));
    }

    #[test]
    fn moving_a_line_moves_every_zone_on_it() {
        let mut z = Preset::HalfAndStack.zones();
        let x = line_of(&z, 0, Side::Right).unwrap();
        assert!((move_line(&mut z, &x, 0.6, MIN) - 0.6).abs() < 1e-9);
        assert_eq!(z[0], r(0.0, 0.0, 0.6, 1.0));
        assert_eq!(z[1], r(0.6, 0.0, 1.0, 0.5));
        assert_eq!(z[2], r(0.6, 0.5, 1.0, 1.0));
    }

    #[test]
    fn a_line_stops_before_a_zone_gets_too_small() {
        let mut z = Preset::Halves.zones();
        let x = line_of(&z, 0, Side::Right).unwrap();
        assert!((move_line(&mut z, &x, 0.99, MIN) - 0.95).abs() < 1e-9);
        assert!((z[1].width() - MIN).abs() < 1e-9);
    }

    #[test]
    fn sides_on_the_border_have_no_line() {
        let z = Preset::Halves.zones();
        assert!(line_of(&z, 0, Side::Left).is_none());
        assert!(line_of(&z, 0, Side::Top).is_none());
        assert!(line_of(&z, 1, Side::Right).is_none());
        assert!(line_of(&z, 1, Side::Left).is_some());
    }

    #[test]
    fn a_window_widened_into_its_neighbour_takes_the_line_along() {
        // Left half of 1920 x 1032; the window in it is dragged 192 px wider by its right edge.
        let z = Preset::Halves.zones();
        let work = Rect::new(0, 0, 1920, 1032);
        let start = Rect::new(4, 32, 956, 1028);
        let now = Rect::new(4, 32, 1148, 1028);
        let (after, moved) = follow_resize(&z, 0, start, now, work, MIN);
        assert!(moved);
        assert!((after[0].right() - 0.6).abs() < 1e-9, "{:?}", after[0]);
        assert!((after[1].left() - 0.6).abs() < 1e-9);
        assert!((after[1].right() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_corner_moves_both_lines() {
        // Quarters: the top-left window's bottom-right corner, dragged right and down.
        let z = Preset::Quarters.zones();
        let work = Rect::new(0, 0, 2000, 1000);
        let start = Rect::new(4, 32, 996, 496);
        let now = Rect::new(4, 32, 1196, 596);
        let (after, moved) = follow_resize(&z, 0, start, now, work, MIN);
        assert!(moved);
        assert!((after[0].right() - 0.6).abs() < 1e-9 && (after[0].bottom() - 0.6).abs() < 1e-9);
        // The middle lines run right across: every quarter follows.
        assert!((after[3].left() - 0.6).abs() < 1e-9 && (after[3].top() - 0.6).abs() < 1e-9);
        assert!((after[1].bottom() - 0.6).abs() < 1e-9 && (after[2].right() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn a_resize_against_the_border_moves_nothing() {
        let z = Preset::Halves.zones();
        let work = Rect::new(0, 0, 1920, 1032);
        // The left edge of the left window, dragged right: there is no line there.
        let (after, moved) = follow_resize(
            &z,
            0,
            Rect::new(4, 32, 956, 1028),
            Rect::new(200, 32, 956, 1028),
            work,
            MIN,
        );
        assert!(!moved);
        assert_eq!(after, z);
        // A move of a pixel or two is no resize.
        let (_, moved) = follow_resize(
            &z,
            0,
            Rect::new(4, 32, 956, 1028),
            Rect::new(4, 32, 958, 1028),
            work,
            MIN,
        );
        assert!(!moved);
    }

    #[test]
    fn the_stacked_zones_share_their_own_line() {
        // In the bottom-right zone of "half and stack", the window's top edge moves the line
        // between the two stacked zones only.
        let z = Preset::HalfAndStack.zones();
        let work = Rect::new(0, 0, 1920, 1000);
        let start = Rect::new(964, 532, 1916, 996);
        let now = Rect::new(964, 432, 1916, 996);
        let (after, moved) = follow_resize(&z, 2, start, now, work, MIN);
        assert!(moved);
        assert!((after[2].top() - 0.4).abs() < 1e-9 && (after[1].bottom() - 0.4).abs() < 1e-9);
        assert_eq!(after[0], z[0], "the full-height half does not change");
    }

    proptest! {
        #[test]
        fn zones_stay_tiled_whatever_the_resize(
            preset in prop::sample::select(Preset::ALL.to_vec()),
            zone_pick in 0usize..4,
            dl in -900i32..900, dt in -500i32..500, dr in -900i32..900, db in -500i32..500,
        ) {
            let z = preset.zones();
            let zone = zone_pick % z.len();
            let work = Rect::new(0, 0, 1920, 1032);
            let start = z[zone].to_rect(work);
            let now = Rect::new(start.left + dl, start.top + dt, start.right + dr, start.bottom + db);
            let (after, _) = follow_resize(&z, zone, start, now, work, MIN);
            let total: f64 = after.iter().map(|a| a.width() * a.height()).sum();
            prop_assert!((total - 1.0).abs() < 1e-6, "{after:?}");
            for a in &after {
                prop_assert!(a.width() >= MIN - 1e-9 && a.height() >= MIN - 1e-9, "{a:?}");
            }
            for (i, a) in after.iter().enumerate() {
                for b in &after[i + 1..] {
                    let w = (a.right().min(b.right()) - a.left().max(b.left())).max(0.0);
                    let h = (a.bottom().min(b.bottom()) - a.top().max(b.top())).max(0.0);
                    prop_assert!(w * h < 1e-9, "{a:?} overlaps {b:?}");
                }
            }
        }
    }
}
