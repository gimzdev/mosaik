//! From "where the user should see the window" to "the rectangle Windows needs".
//!
//! Windows 10 and 11 draw invisible resize borders outside the visible frame (about 7 px at 100%
//! on the left, right and bottom). Mosaïk places by the visible frame, measured with
//! `DWMWA_EXTENDED_FRAME_BOUNDS`, so neighbouring windows meet exactly with no gaps.

use crate::geometry::{Insets, Point, Rect, Size};
use crate::layout::{Layout, Place};
use crate::topology::{MonitorId, Topology, map_monitors};
use crate::zone::FrameStyle;

/// The rectangle to pass to `SetWindowPos` so the visible frame covers `frame`.
pub fn window_rect_for(frame: Rect, insets: Insets) -> Rect {
    frame.outset(insets)
}

/// Where a window goes when it will not take the target's size: a fixed-size window, or an app
/// whose minimum size is larger than its zone (Chrome will not go below about 500 px wide).
///
/// Along each axis, a window smaller than the target is centred in it. A larger one keeps the
/// target's edge that is nearer the work area's edge, so it grows toward the middle of the
/// screen, over neighbouring zones, instead of off the screen. The result stays inside the work
/// area whenever it fits there.
pub fn fit_size(target: Rect, size: Size, work_area: Rect) -> Rect {
    fn axis(start: i32, end: i32, length: i32, area_start: i32, area_end: i32) -> i32 {
        let span = end - start;
        if length <= span {
            start + (span - length) / 2
        } else if area_end - end < start - area_start {
            end - length
        } else {
            start
        }
    }
    let left = axis(
        target.left,
        target.right,
        size.width,
        work_area.left,
        work_area.right,
    );
    let top = axis(
        target.top,
        target.bottom,
        size.height,
        work_area.top,
        work_area.bottom,
    );
    Rect::from_origin_size(left, top, size.width, size.height).fit_inside(work_area)
}

/// Keeps a window reachable: if no monitor's work area holds a grab handle at its top-left
/// corner (a 64 px square, roughly the title bar's left end), the window is moved onto the
/// nearest monitor. Windows never land off-screen.
pub fn reachable(frame: Rect, topology: &Topology) -> Rect {
    if topology.is_reachable(frame, 64) {
        return frame;
    }
    let monitor = topology.nearest_monitor(Point::new(frame.left, frame.top));
    frame.fit_inside(monitor.work_area)
}

/// Where a layout place lands on the monitors connected now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Put the visible frame here.
    Frame(Rect),
    /// Maximise on this monitor.
    Maximized(MonitorId),
}

/// Resolves every place of a layout against the current topology at once (so monitor mapping
/// is decided once per switch).
pub struct Resolver<'a> {
    layout: &'a Layout,
    topology: &'a Topology,
    mapping: Vec<usize>,
    style: FrameStyle,
}

impl<'a> Resolver<'a> {
    pub fn new(layout: &'a Layout, topology: &'a Topology, style: FrameStyle) -> Self {
        let mapping = map_monitors(&layout.monitors, topology);
        Self {
            layout,
            topology,
            mapping,
            style,
        }
    }

    fn monitor(&self, index: usize) -> Option<&'a crate::topology::Monitor> {
        self.mapping
            .get(index)
            .and_then(|i| self.topology.monitors().get(*i))
    }

    /// The zone's rectangle on screen now, or `None` if the zone is unknown.
    pub fn zone_rect(&self, zone: &crate::zone::ZoneId) -> Option<(Rect, u32)> {
        let z = self.layout.zone(zone)?;
        let m = self.monitor(z.monitor)?;
        Some((z.area.to_rect(m.work_area), m.dpi))
    }

    /// Where a place lands. A tab lands where its group does, below a band with room for the
    /// group's strip; a group whose own place is a tab (a damaged file) lands nowhere.
    pub fn target(&self, place: &Place) -> Option<Target> {
        match place {
            Place::Tab { group } => {
                let g = self.layout.tab_group(group)?;
                self.direct(&g.place, self.style.for_tabs())
            }
            other => self.direct(other, self.style),
        }
    }

    fn direct(&self, place: &Place, style: FrameStyle) -> Option<Target> {
        match place {
            Place::Zone { zone } => {
                let (rect, dpi) = self.zone_rect(zone)?;
                Some(Target::Frame(reachable(
                    style.window_area(rect, dpi),
                    self.topology,
                )))
            }
            Place::Free { monitor, area } => {
                let m = self.monitor(*monitor)?;
                Some(Target::Frame(reachable(area.to_rect(m.work_area), self.topology)))
            }
            Place::Maximized { monitor } => Some(Target::Maximized(self.monitor(*monitor)?.id.clone())),
            Place::Tab { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::geometry::RatioRect;
    use crate::topology::Monitor;
    use crate::zone::{Zone, ZoneColor, ZoneId};
    use proptest::prelude::*;

    #[test]
    fn invisible_borders_are_added_back() {
        let frame = Rect::new(4, 32, 956, 1028);
        assert_eq!(
            window_rect_for(frame, Insets::new(7, 0, 7, 7)),
            Rect::new(-3, 32, 963, 1035)
        );
    }

    #[test]
    fn smaller_windows_are_centred_in_the_target() {
        let work = Rect::new(0, 0, 1920, 1032);
        assert_eq!(
            fit_size(Rect::new(0, 0, 960, 1032), Size::new(400, 300), work),
            Rect::new(280, 366, 680, 666)
        );
    }

    #[test]
    fn larger_windows_grow_toward_the_middle_of_the_screen() {
        let work = Rect::new(0, 0, 1920, 1032);
        // Measured: Chrome keeps 502 px in a 420 px zone at the right edge. It must end at the
        // zone's right edge, not 82 px past the screen's.
        let zone = Rect::new(1492, 8, 1912, 1024);
        assert_eq!(
            fit_size(zone, Size::new(502, 1016), work),
            Rect::new(1410, 8, 1912, 1024)
        );
        // The same zone on the left keeps its left edge.
        let left = Rect::new(8, 8, 428, 1024);
        assert_eq!(
            fit_size(left, Size::new(502, 1016), work),
            Rect::new(8, 8, 510, 1024)
        );
        // A small zone in a corner: the window stays inside the work area.
        assert_eq!(
            fit_size(Rect::new(1800, 0, 1920, 100), Size::new(400, 300), work),
            Rect::new(1520, 0, 1920, 300)
        );
    }

    #[test]
    fn unreachable_windows_come_back() {
        let t = fixtures::single_1080p_100();
        assert_eq!(
            reachable(Rect::new(3000, 200, 3500, 600), &t),
            Rect::new(1420, 200, 1920, 600)
        );
        let fine = Rect::new(100, 100, 600, 400);
        assert_eq!(reachable(fine, &t), fine);
    }

    fn two_zone_layout(t: &Topology) -> Layout {
        let zone = |id: &str, l: f64, r: f64| Zone {
            id: ZoneId::new(id),
            name: id.into(),
            color: ZoneColor::Lapis,
            monitor: 0,
            area: RatioRect::new(l, 0.0, r, 1.0).unwrap(),
        };
        Layout {
            monitors: t.monitors().iter().map(Monitor::to_ref).collect(),
            zones: vec![zone("a", 0.0, 0.6), zone("b", 0.6, 1.0)],
            slots: vec![],
            tab_groups: vec![],
        }
    }

    #[test]
    fn layouts_follow_the_work_area() {
        let made_on = fixtures::single_1080p_100();
        let layout = two_zone_layout(&made_on);
        let r = Resolver::new(&layout, &made_on, FrameStyle::default());
        assert_eq!(
            r.target(&Place::Zone {
                zone: ZoneId::new("a")
            }),
            Some(Target::Frame(Rect::new(4, 32, 1148, 1028)))
        );
        // Same layout with the taskbar moved to the left edge: the zones shift right with it.
        let left = fixtures::single_with_taskbar(fixtures::TaskbarSpec::on(fixtures::Edge::Left));
        let r = Resolver::new(&layout, &left, FrameStyle::default());
        let Some(Target::Frame(f)) = r.target(&Place::Zone {
            zone: ZoneId::new("a"),
        }) else {
            panic!()
        };
        assert_eq!(f.left, 48 + 4);
    }

    proptest! {
        /// Zones made on one arrangement and opened on any other land on a connected monitor's
        /// work area, never off-screen.
        #[test]
        fn zones_never_land_off_screen(from in 0usize..18, to in 0usize..18, l in 0.0f64..0.9, w in 0.05f64..0.5, t in 0.0f64..0.9, h in 0.05f64..0.5, m in 0usize..3) {
            let all = fixtures::all();
            let made_on = &all[from % all.len()];
            let now = &all[to % all.len()];
            let monitor = m % made_on.monitors().len();
            let area = RatioRect::new(l, t, (l + w).min(1.0), (t + h).min(1.0)).unwrap();
            let layout = Layout {
                monitors: made_on.monitors().iter().map(Monitor::to_ref).collect(),
                zones: vec![Zone { id: ZoneId::new("z"), name: "Z".into(), color: ZoneColor::Saffron, monitor, area }],
                slots: vec![],
                tab_groups: vec![],
            };
            let r = Resolver::new(&layout, now, FrameStyle::default());
            let Some(Target::Frame(frame)) = r.target(&Place::Zone { zone: ZoneId::new("z") }) else { panic!("zone did not resolve") };
            prop_assert!(now.monitors().iter().any(|mon| mon.work_area.contains_rect(frame)), "{:?} is off every work area", frame);
        }

        /// Whatever size an app insists on, it stays inside the work area when it fits there, and
        /// keeps as much of the target as its size allows.
        #[test]
        fn fit_size_stays_on_screen(tl in 0i32..1800, tt in 0i32..1000, tw in 1i32..1920, th in 1i32..1032, w in 1i32..1920, h in 1i32..1032) {
            let work = Rect::new(0, 0, 1920, 1032);
            let target = Rect::from_origin_size(tl, tt, tw, th).fit_inside(work);
            let r = fit_size(target, Size::new(w, h), work);
            prop_assert!(work.contains_rect(r), "{:?} leaves {:?}", r, work);
            prop_assert_eq!((r.width(), r.height()), (w, h));
            let overlap = r.overlap_area(target);
            let best = i64::from(w.min(target.width())) * i64::from(h.min(target.height()));
            prop_assert_eq!(overlap, best, "{:?} in {:?}", r, target);
        }

        /// Ratios survive a scale change: the same layout on the same monitor at another DPI
        /// covers the same fraction of the work area (to within a pixel per edge).
        #[test]
        fn ratios_survive_dpi_changes(dpi_a in prop::sample::select(vec![96u32, 120, 144, 168, 192]), dpi_b in prop::sample::select(vec![96u32, 120, 144, 168, 192]), l in 0.0f64..0.5, r in 0.5f64..=1.0) {
            let a = fixtures::single_1080p_at(dpi_a);
            let b = fixtures::single_1080p_at(dpi_b);
            let area = RatioRect::new(l, 0.0, r, 1.0).unwrap();
            let on_a = area.to_rect(a.primary().work_area);
            let back = RatioRect::from_rect(on_a, a.primary().work_area).unwrap();
            let on_b = back.to_rect(b.primary().work_area);
            let direct = area.to_rect(b.primary().work_area);
            prop_assert!((on_b.left - direct.left).abs() <= 1 && (on_b.right - direct.right).abs() <= 1);
        }
    }
}
