//! A window's own title bar, as room for its zone's tag and its tab group's strip.
//!
//! Every app says where its window can be grabbed to move it: Windows asks the window
//! (`WM_NCHITTEST`) and it answers "caption" for the empty, draggable stretches of its title bar,
//! whether Windows draws that title bar (a classic one) or the app does (browsers, editors,
//! terminals, Office). Mosaïk asks the same question along one row of the title bar ([`scan`]) and
//! draws the tag, or the strip, in such a stretch ([`TitleRoom::spot`]), so the window keeps its
//! whole zone. A window with no stretch wide enough wears its tag above it, in a band, as it does
//! when tags are set to go above windows.

use crate::geometry::{Point, Rect, scale_for_dpi};
use crate::zone::CHIP_HEIGHT;

/// What a window answers for a point of its title bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// Its empty, draggable title bar.
    Caption,
    /// Anything else: a button, a tab, a menu, a search box, a border, its content.
    Other,
}

// How a title bar is read, in device-independent pixels.

/// The row read, below the frame's top edge: the middle of most title bars (Windows draws them
/// 31 px high, apps 22 to 48 px).
pub const ROW: i32 = 15;
/// Between two points read along the row.
const STEP: i32 = 6;
/// Left alone at each end of the row: the window's resize borders.
const EDGE: i32 = 6;
/// Narrower stretches do not count (the gaps between a browser's tabs).
const MIN_RUN: i32 = 24;
/// How far below the frame's top edge the caption is followed down.
const MAX_HEIGHT: i32 = 64;
/// The precision of the caption's top and bottom.
const FINE: i32 = 2;

// How tags and strips sit in a title bar, in device-independent pixels.

/// Space kept between a tag or strip and the ends of its stretch.
const MARGIN: i32 = 6;
/// The shortest a tag gets in a title bar (the first letters of the zone's name).
const TAG_LEAST: i32 = 48;
/// The narrowest a strip gets in a title bar: the zone's tag, the visible tab's chip and the
/// "+N" chip of the others.
const STRIP_LEAST: i32 = 180;
/// Left free at the start of a strip's stretch: the chips take the mouse, and the window must
/// still have a stretch of title bar to be grabbed by.
const GRAB: i32 = 40;

/// A free stretch of a title bar, as its distances from the frame's left and right edges. When
/// the window is resized, a stretch keeps to what bounds it (the tabs or the title on its left,
/// the window's buttons on its right) until the title bar is read again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub from_left: i32,
    pub from_right: i32,
}

/// Where a window's title bar is free.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleRoom {
    /// The caption's top and bottom, from the frame's top edge.
    pub top: i32,
    pub bottom: i32,
    /// The free stretches, left to right; none when the title bar has no room.
    pub runs: Vec<Run>,
}

/// What is to go in a title bar, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    /// The width it would like.
    pub need: i32,
    /// The least width it can do with.
    pub least: i32,
    /// Left free at the start of the stretch.
    pub keep: i32,
    pub margin: i32,
    pub height: i32,
}

impl Fit {
    /// A zone's tag, `natural` pixels wide with its whole name, on a monitor at `dpi`.
    pub fn tag(natural: i32, dpi: u32) -> Self {
        let px = |v: i32| scale_for_dpi(v, dpi);
        Self {
            need: natural,
            least: px(TAG_LEAST),
            keep: 0,
            margin: px(MARGIN),
            height: px(CHIP_HEIGHT),
        }
    }

    /// A tab group's strip, `natural` pixels wide with every chip at its own width.
    pub fn strip(natural: i32, dpi: u32) -> Self {
        let px = |v: i32| scale_for_dpi(v, dpi);
        Self {
            need: natural,
            least: px(STRIP_LEAST),
            keep: px(GRAB),
            margin: px(MARGIN),
            height: px(CHIP_HEIGHT),
        }
    }
}

/// Reads the title bar of a window whose visible frame is `frame`, on a monitor at `dpi`, with
/// `hit` saying what the window answers for each point. `None` when the window did not answer
/// (busy, or not responding); it is read again later.
pub fn scan(frame: Rect, dpi: u32, mut hit: impl FnMut(Point) -> Option<Hit>) -> Option<TitleRoom> {
    let px = |v: i32| scale_for_dpi(v, dpi);
    let y = frame.top + px(ROW);
    let (from, to) = (frame.left + px(EDGE), frame.right - px(EDGE));
    if to - from < px(MIN_RUN) || y >= frame.bottom {
        return Some(TitleRoom::default());
    }
    let step = px(STEP).max(1);
    // Stretches as [left, right) on screen; the open one as (first, last) caption point.
    let mut runs: Vec<(i32, i32)> = Vec::new();
    let mut open: Option<(i32, i32)> = None;
    let mut x = from;
    while x < to {
        let caption = hit(Point::new(x, y))? == Hit::Caption;
        open = match (caption, open) {
            (true, None) => Some((x, x)),
            (true, Some((first, _))) => Some((first, x)),
            (false, Some((first, last))) => {
                runs.push((first, last + 1));
                None
            }
            (false, None) => None,
        };
        x += step;
    }
    if let Some((first, last)) = open {
        runs.push((first, last + 1));
    }
    runs.retain(|(l, r)| r - l >= px(MIN_RUN));
    let Some(&(l, r)) = runs.last() else {
        return Some(TitleRoom::default());
    };
    // The caption's top and bottom where a tag most often goes: the middle of the rightmost
    // stretch.
    let mid = l + (r - l) / 2;
    let fine = px(FINE).max(1);
    let mut top = y;
    while top - fine >= frame.top && hit(Point::new(mid, top - fine))? == Hit::Caption {
        top -= fine;
    }
    let limit = (frame.top + px(MAX_HEIGHT)).min(frame.bottom);
    let mut last = y;
    while last + fine < limit && hit(Point::new(mid, last + fine))? == Hit::Caption {
        last += fine;
    }
    Some(TitleRoom {
        top: top - frame.top,
        bottom: last + 1 - frame.top,
        runs: runs
            .into_iter()
            .map(|(l, r)| Run {
                from_left: l - frame.left,
                from_right: frame.right - r,
            })
            .collect(),
    })
}

impl TitleRoom {
    /// The free stretches on a window whose frame is now `frame`, as `[left, right)` on screen.
    pub fn runs_on(&self, frame: Rect) -> Vec<(i32, i32)> {
        self.runs
            .iter()
            .map(|r| (frame.left + r.from_left, frame.right - r.from_right))
            .filter(|(l, r)| r > l)
            .collect()
    }

    /// Where `fit` goes in the title bar of a window whose frame is `frame`: in the rightmost
    /// stretch with room for what it would like, else in the widest one with room for the least
    /// it can do with; `None` when no stretch has that. The whole usable stretch is returned
    /// (a strip lays its chips out in it, a tag sits at its right end), `fit.height` high and
    /// centred on the caption.
    pub fn spot(&self, frame: Rect, fit: Fit) -> Option<Rect> {
        let room = |(l, r): (i32, i32)| r - l - 2 * fit.margin - fit.keep;
        let runs = self.runs_on(frame);
        let (l, r) = runs
            .iter()
            .rev()
            .find(|run| room(**run) >= fit.need)
            .or_else(|| {
                runs.iter()
                    .filter(|run| room(**run) >= fit.least)
                    .max_by_key(|run| room(**run))
            })
            .copied()?;
        let middle = frame.top + (self.top + self.bottom) / 2;
        let top = middle - fit.height / 2;
        Some(Rect::new(
            l + fit.margin + fit.keep,
            top,
            r - fit.margin,
            top + fit.height,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A title bar drawn as caption stretches `[left, right)` between `top` and `bottom` (both
    /// from the frame's top), on a frame at the origin.
    fn bar(runs: &[(i32, i32)], top: i32, bottom: i32) -> impl FnMut(Point) -> Option<Hit> + '_ {
        move |p: Point| {
            let caption = p.y >= top && p.y < bottom && runs.iter().any(|(l, r)| p.x >= *l && p.x < *r);
            Some(if caption { Hit::Caption } else { Hit::Other })
        }
    }

    #[test]
    fn a_classic_title_bar_is_free_from_the_icon_to_the_buttons() {
        // 960 px wide, caption from the icon (36) to the buttons (822), 1 to 31 px down.
        let frame = Rect::new(0, 0, 960, 600);
        let room = scan(frame, 96, bar(&[(36, 822)], 1, 31)).unwrap();
        assert_eq!(room.top, 1);
        assert_eq!(room.bottom, 30);
        assert_eq!(
            room.runs,
            vec![Run {
                from_left: 36,
                from_right: 143
            }]
        );
        // A tag sits in the stretch, clear of its ends, centred on the caption.
        let spot = room.spot(frame, Fit::tag(70, 96)).unwrap();
        assert_eq!(spot, Rect::new(42, 5, 811, 26));
    }

    #[test]
    fn the_rightmost_stretch_with_room_wins_else_the_widest() {
        // A browser: a little caption left of the tabs, the tabs, then free space up to the
        // buttons.
        let frame = Rect::new(0, 0, 1200, 800);
        let room = scan(frame, 96, bar(&[(8, 120), (700, 1062)], 0, 40)).unwrap();
        assert_eq!(room.runs.len(), 2);
        let right = room.spot(frame, Fit::tag(90, 96)).unwrap();
        assert!(right.left >= 700 && right.right <= 1062, "{right:?}");
        // Too wide for any stretch: the widest one that has room for the least, with the start
        // of the stretch left free to grab the window by.
        let strip = room.spot(frame, Fit::strip(900, 96)).unwrap();
        let (left, right) = room.runs_on(frame)[1];
        assert_eq!((strip.left, strip.right), (left + 6 + 40, right - 6));
    }

    #[test]
    fn no_stretch_wide_enough_is_no_room() {
        let frame = Rect::new(0, 0, 400, 300);
        // Only the gaps between tabs answer "caption".
        let room = scan(frame, 96, bar(&[(100, 112), (200, 212)], 0, 40)).unwrap();
        assert!(room.runs.is_empty());
        assert_eq!(room.spot(frame, Fit::tag(60, 96)), None);
        // A stretch with room for a tag but not for a strip.
        let room = scan(frame, 96, bar(&[(150, 260)], 0, 30)).unwrap();
        assert!(room.spot(frame, Fit::tag(60, 96)).is_some());
        assert_eq!(room.spot(frame, Fit::strip(300, 96)), None);
    }

    #[test]
    fn a_window_that_does_not_answer_is_read_again_later() {
        let frame = Rect::new(0, 0, 800, 600);
        let mut asked = 0;
        let room = scan(frame, 96, |_| {
            asked += 1;
            (asked < 10).then_some(Hit::Caption)
        });
        assert_eq!(room, None);
        // A window too small to have a title bar is not asked at all.
        assert_eq!(
            scan(Rect::new(0, 0, 30, 10), 96, |_| panic!("asked")),
            Some(TitleRoom::default())
        );
    }

    #[test]
    fn stretches_keep_to_the_edges_that_bound_them_when_the_window_is_resized() {
        let measured = Rect::new(0, 0, 960, 600);
        let room = scan(measured, 96, bar(&[(36, 822)], 1, 31)).unwrap();
        // Narrower and moved: the stretch still starts after the icon and ends at the buttons.
        let now = Rect::new(100, 50, 900, 650);
        assert_eq!(room.runs_on(now), vec![(136, 757)]);
        // So narrow that nothing is left between them: no room until it is read again.
        assert!(room.runs_on(Rect::new(0, 0, 150, 600)).is_empty());
    }

    #[test]
    fn everything_scales_with_the_monitor() {
        // The same title bar at 200%: twice the row, steps and margins.
        let frame = Rect::new(-1920, -100, 0, 1100);
        let shifted: Vec<(i32, i32)> = vec![(-1920 + 72, -1920 + 1644)];
        let room = scan(frame, 192, |p: Point| {
            let y = p.y - frame.top;
            let caption = (2..62).contains(&y) && shifted.iter().any(|(l, r)| p.x >= *l && p.x < *r);
            Some(if caption { Hit::Caption } else { Hit::Other })
        })
        .unwrap();
        assert_eq!(room.runs.len(), 1);
        let spot = room.spot(frame, Fit::tag(140, 192)).unwrap();
        assert_eq!(spot.height(), 42);
        assert!(
            spot.top >= frame.top + 2 && spot.bottom <= frame.top + 62,
            "{spot:?}"
        );
    }

    proptest! {
        /// Whatever the title bar: every stretch found is caption from end to end (so nothing Mosaïk
        /// draws there covers a button, a tab or a menu), and every caption stretch wide enough
        /// is found. Controls are wider than the step between points read (12 px at most), as
        /// buttons, tabs and boxes are.
        #[test]
        fn stretches_found_are_caption_and_wide_ones_are_found(
            widths in proptest::collection::vec(14i32..300, 1..14),
            first_free in any::<bool>(),
            dpi in prop::sample::select(vec![96u32, 120, 144, 192]),
        ) {
            let mut truth: Vec<(i32, i32)> = Vec::new();
            let mut x = 0;
            for (i, w) in widths.iter().enumerate() {
                if (i % 2 == 0) == first_free {
                    truth.push((x, x + w));
                }
                x += w;
            }
            let width = x.max(100);
            let frame = Rect::new(0, 0, width, 900);
            let bottom = scale_for_dpi(31, dpi);
            let room = scan(frame, dpi, bar(&truth, 0, bottom)).unwrap();
            for (l, r) in room.runs_on(frame) {
                prop_assert!(truth.iter().any(|(tl, tr)| *tl <= l && r <= *tr), "{l}..{r} not inside {truth:?}");
            }
            let px = |v: i32| scale_for_dpi(v, dpi);
            let (from, to) = (px(EDGE), width - px(EDGE));
            for (tl, tr) in &truth {
                let (l, r) = ((*tl).max(from), (*tr).min(to));
                if r - l >= px(MIN_RUN) + 2 * px(STEP) {
                    prop_assert!(room.runs_on(frame).iter().any(|(fl, fr)| *fl <= l + px(STEP) && *fr >= r - px(STEP)), "{l}..{r} missed: {:?}", room.runs);
                }
            }
        }
    }
}
