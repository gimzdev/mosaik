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

    /// The preset with the id the zone editor and the home page use, if there is one.
    pub fn from_id(id: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Its name, as the interface shows it (`PRESETS` in apps/desktop/src/editor/model.ts).
    pub const fn label(self) -> &'static str {
        match self {
            Preset::Full => "Whole screen",
            Preset::Halves => "Halves",
            Preset::Thirds => "Thirds",
            Preset::MainSide => "Main and side",
            Preset::SideMain => "Side and main",
            Preset::Quarters => "Quarters",
            Preset::HalfAndStack => "Half beside two stacked",
            Preset::StackAndHalf => "Two stacked beside half",
            Preset::Rows => "Top and bottom",
            Preset::ThreeRows => "Three rows",
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

    /// The preset for up to four windows on a monitor without zones (a tab group counts as
    /// one), by the shape of its work area: columns on a landscape monitor, rows on one turned on
    /// its side. More windows get a grid ([`zones_for_windows`]).
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

/// The narrowest and the lowest a zone of an Arrange grid gets, in pixels: most apps are still
/// usable at that size (a browser keeps about 500 px).
const GRID_COLUMN: i32 = 600;
const GRID_ROW: i32 = 420;

/// The zones Arrange gives a monitor with `windows` windows and no zones of the open workspace,
/// `width` by `height` pixels: up to four windows, the preset that suits them
/// ([`Preset::for_windows`]); more, a grid of as many zones as the monitor holds well (columns of
/// at least 600 px and rows of at least 420 px, at most four columns and three rows), the top
/// rows holding fewer, larger zones for the windows that matter most. Windows beyond them are
/// minimised.
pub fn zones_for_windows(windows: usize, width: i32, height: i32) -> Vec<RatioRect> {
    let columns = usize::try_from(width / GRID_COLUMN).unwrap_or(1).clamp(1, 4);
    let rows = usize::try_from(height / GRID_ROW).unwrap_or(1).clamp(1, 3);
    let room = columns * rows;
    if windows <= 4 || room <= 4 {
        return Preset::for_windows(windows.min(4), width, height).zones();
    }
    grid(windows.min(room), columns)
}

/// `cells` zones in rows of at most `columns`, as even as can be, the rows with fewer (wider)
/// zones on top.
fn grid(cells: usize, columns: usize) -> Vec<RatioRect> {
    let rows = cells.div_ceil(columns.max(1)).max(1);
    let (base, wider) = (cells / rows, cells % rows);
    let mut out = Vec::with_capacity(cells);
    // Counts of a few cells: exact as f64.
    #[allow(clippy::cast_precision_loss)]
    let frac = |i: usize, n: usize| if i >= n { 1.0 } else { i as f64 / n as f64 };
    for row in 0..rows {
        let n = if row < rows - wider { base } else { base + 1 };
        for column in 0..n {
            let r = RatioRect::new(
                frac(column, n),
                frac(row, rows),
                frac(column + 1, n),
                frac(row + 1, rows),
            )
            .unwrap_or(RatioRect::FULL);
            out.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The zones of `zones` cover the work area once, with no overlap and no gap.
    fn tile(zones: &[RatioRect]) -> bool {
        let area: f64 = zones.iter().map(|z| z.width() * z.height()).sum();
        let overlap = zones.iter().enumerate().any(|(i, a)| {
            zones[i + 1..].iter().any(|b| {
                a.left() < b.right() - EPS
                    && b.left() < a.right() - EPS
                    && a.top() < b.bottom() - EPS
                    && b.top() < a.bottom() - EPS
            })
        });
        (area - 1.0).abs() < 1e-6 && !overlap
    }

    #[test]
    fn up_to_four_windows_get_the_preset_that_suits_them() {
        for n in 0..=4 {
            assert_eq!(
                zones_for_windows(n, 1920, 1032),
                Preset::for_windows(n, 1920, 1032).zones(),
                "{n} windows"
            );
        }
    }

    #[test]
    fn more_windows_get_a_grid_with_larger_zones_on_top() {
        // The owner's screen: five windows in two rows, two then three; six in three and three.
        let five = zones_for_windows(5, 1920, 1032);
        assert_eq!(five.len(), 5);
        assert!(tile(&five), "{five:?}");
        assert_eq!(five.iter().filter(|z| z.top() < EPS).count(), 2);
        assert!(five[0].width() > five[4].width());
        let six = zones_for_windows(6, 1920, 1032);
        assert_eq!(six.len(), 6);
        assert!(tile(&six));
        assert!(
            six.iter()
                .all(|z| (z.width() - 1.0 / 3.0).abs() < EPS && (z.height() - 0.5).abs() < EPS)
        );
    }

    #[test]
    fn a_monitor_holds_no_more_zones_than_it_has_room_for() {
        // 1920 by 1032: three columns of at least 600 px, two rows of at least 420 px.
        assert_eq!(zones_for_windows(11, 1920, 1032).len(), 6);
        // 2560 by 1392: four columns, three rows.
        let nine = zones_for_windows(9, 2560, 1392);
        assert_eq!(nine.len(), 9);
        assert!(tile(&nine));
        assert_eq!(zones_for_windows(20, 2560, 1392).len(), 12);
        // A small laptop screen holds four; a monitor on its side, three rows.
        assert_eq!(zones_for_windows(7, 1366, 728).len(), 4);
        assert_eq!(zones_for_windows(7, 1080, 1872).len(), 3);
        for (w, h) in [
            (1920, 1032),
            (2560, 1392),
            (3440, 1392),
            (1366, 728),
            (1080, 1872),
        ] {
            for n in 1..15 {
                assert!(tile(&zones_for_windows(n, w, h)), "{n} windows on {w}x{h}");
            }
        }
    }

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
    fn ids_and_names_are_the_interfaces() {
        let ids: std::collections::HashSet<&str> = Preset::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids.len(), Preset::ALL.len());
        // The ids the zone editor and the home page send, and the names they show, read from
        // the interface's own list: `{ id: 'halves', label: 'Halves' }`.
        let model = include_str!("../../../apps/desktop/src/editor/model.ts");
        let listed: Vec<(&str, &str)> = model
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let id = l.strip_prefix("{ id: '")?.split('\'').next()?;
                let label = l.split("label: '").nth(1)?.split('\'').next()?;
                Some((id, label))
            })
            .collect();
        assert_eq!(listed.len(), Preset::ALL.len(), "{listed:?}");
        for (id, label) in listed {
            let preset = Preset::from_id(id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(preset.label(), label, "{id}");
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
