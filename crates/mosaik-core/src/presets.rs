//! Ready-made zone shapes for one monitor: the zone editor's "Start from a preset" menu
//! (`apps/desktop/src/editor/model.ts` has the same shapes, by the same names) and Arrange,
//! which gives a monitor without zones the one that suits its windows.

use crate::geometry::RatioRect;

/// A shape of zones on one monitor, as fractions of its work area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    /// One zone over the whole work area.
    Full,
    /// Two columns.
    Halves,
    /// Three columns.
    Thirds,
    /// Two thirds, then one third.
    MainSide,
    /// One third, then two thirds.
    SideMain,
    /// Four quarters.
    Quarters,
    /// The left half at full height; the right half split into a top and a bottom zone.
    HalfAndStack,
    /// The left half split into a top and a bottom zone; the right half at full height.
    StackAndHalf,
    /// Two rows: for a monitor turned on its side, or two wide windows.
    Rows,
    /// Three rows, for a monitor turned on its side.
    ThreeRows,
}

impl Preset {
    pub const ALL: [Preset; 10] = [
        Preset::Full,
        Preset::Halves,
        Preset::Thirds,
        Preset::MainSide,
        Preset::SideMain,
        Preset::Quarters,
        Preset::HalfAndStack,
        Preset::StackAndHalf,
        Preset::Rows,
        Preset::ThreeRows,
    ];

    /// The id the zone editor uses for it.
    pub const fn id(self) -> &'static str {
        match self {
            Preset::Full => "full",
            Preset::Halves => "halves",
            Preset::Thirds => "thirds",
            Preset::MainSide => "main-side",
            Preset::SideMain => "side-main",
            Preset::Quarters => "quarters",
            Preset::HalfAndStack => "half-stack",
            Preset::StackAndHalf => "stack-half",
            Preset::Rows => "rows",
            Preset::ThreeRows => "three-rows",
        }
    }

    /// Its zones, in reading order (top to bottom, then left to right).
    pub fn zones(self) -> Vec<RatioRect> {
        const THIRD: f64 = 1.0 / 3.0;
        const TWO_THIRDS: f64 = 2.0 / 3.0;
        let r = |l: f64, t: f64, rt: f64, b: f64| RatioRect::new(l, t, rt, b).unwrap_or(RatioRect::FULL);
        match self {
            Preset::Full => vec![RatioRect::FULL],
            Preset::Halves => vec![r(0.0, 0.0, 0.5, 1.0), r(0.5, 0.0, 1.0, 1.0)],
            Preset::Thirds => vec![
                r(0.0, 0.0, THIRD, 1.0),
                r(THIRD, 0.0, TWO_THIRDS, 1.0),
                r(TWO_THIRDS, 0.0, 1.0, 1.0),
            ],
            Preset::MainSide => vec![r(0.0, 0.0, TWO_THIRDS, 1.0), r(TWO_THIRDS, 0.0, 1.0, 1.0)],
            Preset::SideMain => vec![r(0.0, 0.0, THIRD, 1.0), r(THIRD, 0.0, 1.0, 1.0)],
            Preset::Quarters => vec![
                r(0.0, 0.0, 0.5, 0.5),
                r(0.5, 0.0, 1.0, 0.5),
                r(0.0, 0.5, 0.5, 1.0),
                r(0.5, 0.5, 1.0, 1.0),
            ],
            Preset::HalfAndStack => vec![
                r(0.0, 0.0, 0.5, 1.0),
                r(0.5, 0.0, 1.0, 0.5),
                r(0.5, 0.5, 1.0, 1.0),
            ],
            Preset::StackAndHalf => vec![
                r(0.0, 0.0, 0.5, 0.5),
                r(0.5, 0.0, 1.0, 1.0),
                r(0.0, 0.5, 0.5, 1.0),
            ],
            Preset::Rows => vec![r(0.0, 0.0, 1.0, 0.5), r(0.0, 0.5, 1.0, 1.0)],
            Preset::ThreeRows => vec![
                r(0.0, 0.0, 1.0, THIRD),
                r(0.0, THIRD, 1.0, TWO_THIRDS),
                r(0.0, TWO_THIRDS, 1.0, 1.0),
            ],
        }
    }

    /// The preset Arrange gives a monitor without zones for `windows` windows (a tab group
    /// counts as one), by the shape of its work area: columns on a landscape monitor, rows on
    /// one turned on its side, and no more zones than an ultrawide or a normal monitor holds
    /// well. Windows beyond its zones become tabs.
    pub fn for_windows(windows: usize, width: i32, height: i32) -> Preset {
        let portrait = height > width;
        // 21:9 and wider: three columns side by side are still wide.
        let ultrawide = i64::from(width) * 9 >= i64::from(height) * 21;
        match (windows, portrait, ultrawide) {
            (0 | 1, _, _) => Preset::Full,
            (2, true, _) => Preset::Rows,
            (_, true, _) => Preset::ThreeRows,
            (2, false, _) => Preset::Halves,
            (_, false, true) => Preset::Thirds,
            (3, false, false) => Preset::HalfAndStack,
            (_, false, false) => Preset::Quarters,
        }
    }

    /// How large each zone is compared with the largest one (1.0), in the order of
    /// [`Preset::zones`]: Arrange puts the windows that matter most in the largest zones.
    pub fn sizes(self) -> Vec<f64> {
        let zones = self.zones();
        let largest = zones
            .iter()
            .map(|z| z.width() * z.height())
            .fold(0.0_f64, f64::max);
        zones
            .iter()
            .map(|z| {
                if largest > 0.0 {
                    z.width() * z.height() / largest
                } else {
                    1.0
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn every_preset_tiles_its_monitor_exactly() {
        for p in Preset::ALL {
            let zones = p.zones();
            let total: f64 = zones.iter().map(|z| z.width() * z.height()).sum();
            assert!((total - 1.0).abs() < EPS, "{p:?} covers {total}");
            for (i, a) in zones.iter().enumerate() {
                for b in &zones[i + 1..] {
                    let w = (a.right().min(b.right()) - a.left().max(b.left())).max(0.0);
                    let h = (a.bottom().min(b.bottom()) - a.top().max(b.top())).max(0.0);
                    assert!(w * h < EPS, "{p:?}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn half_and_stack_is_a_full_height_half_beside_two_stacked_zones() {
        // The owner's request: "half left full, right half split".
        let z = Preset::HalfAndStack.zones();
        assert_eq!(z.len(), 3);
        assert_eq!(
            (z[0].left(), z[0].top(), z[0].right(), z[0].bottom()),
            (0.0, 0.0, 0.5, 1.0)
        );
        assert_eq!(
            (z[1].left(), z[1].top(), z[1].right(), z[1].bottom()),
            (0.5, 0.0, 1.0, 0.5)
        );
        assert_eq!(
            (z[2].left(), z[2].top(), z[2].right(), z[2].bottom()),
            (0.5, 0.5, 1.0, 1.0)
        );
        // Its mirror keeps the full-height half on the right.
        let m = Preset::StackAndHalf.zones();
        assert!(m.contains(&RatioRect::new(0.5, 0.0, 1.0, 1.0).unwrap()));
        assert_eq!(Preset::HalfAndStack.sizes(), vec![1.0, 0.5, 0.5]);
    }

    #[test]
    fn ids_are_distinct_and_match_the_editor() {
        let ids: std::collections::HashSet<&str> = Preset::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), Preset::ALL.len());
        // The ids the zone editor's menu sends (apps/desktop/src/editor/model.ts).
        for id in [
            "full",
            "halves",
            "thirds",
            "main-side",
            "side-main",
            "quarters",
            "half-stack",
            "stack-half",
            "rows",
            "three-rows",
        ] {
            assert!(ids.contains(id), "{id}");
        }
    }

    #[test]
    fn arrange_picks_a_shape_for_the_windows_and_the_monitor() {
        // 1920 x 1032: a usual landscape work area.
        assert_eq!(Preset::for_windows(1, 1920, 1032), Preset::Full);
        assert_eq!(Preset::for_windows(2, 1920, 1032), Preset::Halves);
        assert_eq!(Preset::for_windows(3, 1920, 1032), Preset::HalfAndStack);
        assert_eq!(Preset::for_windows(4, 1920, 1032), Preset::Quarters);
        assert_eq!(Preset::for_windows(9, 1920, 1032), Preset::Quarters);
        // An ultrawide holds three columns.
        assert_eq!(Preset::for_windows(3, 3440, 1392), Preset::Thirds);
        assert_eq!(Preset::for_windows(6, 3440, 1392), Preset::Thirds);
        // A monitor on its side gets rows.
        assert_eq!(Preset::for_windows(2, 1080, 1872), Preset::Rows);
        assert_eq!(Preset::for_windows(5, 1080, 1872), Preset::ThreeRows);
        assert_eq!(Preset::for_windows(0, 1080, 1872), Preset::Full);
    }
}
