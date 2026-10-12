//! Laying out a tab strip, left to right: the zone's tag, then one chip per tab (shrunk to fit,
//! never below a width that keeps a few letters readable), a "+N" chip for tabs that do not fit,
//! and, while a window is dragged over the strip, the "Drop to add" slot where it would go.
//!
//! Widths are in pixels at the monitor's scale; text widths come from the caller (it measures
//! with the real font). Everything stays inside the strip, so every chip drawn can be clicked.

/// A horizontal span inside the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub x: f32,
    pub w: f32,
}

impl Span {
    pub fn end(self) -> f32 {
        self.x + self.w
    }

    pub fn contains(self, x: f32) -> bool {
        x >= self.x && x < self.end()
    }
}

/// Sizes that do not depend on the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub gap: f32,
    /// The narrowest a chip gets before tabs go into the "+N" chip.
    pub min_chip: f32,
    /// The widest a chip gets, however long its title.
    pub max_chip: f32,
    /// Width of the "+N" chip.
    pub more: f32,
}

impl Metrics {
    /// The strip's sizes at `scale` (1.0 at 100%).
    pub fn at(scale: f32) -> Self {
        Self {
            gap: 4.0 * scale,
            min_chip: 56.0 * scale,
            max_chip: 220.0 * scale,
            more: 40.0 * scale,
        }
    }
}

/// What to lay out.
#[derive(Clone, Debug, PartialEq)]
pub struct Input<'a> {
    pub width: f32,
    /// The tag's natural width (0 for a strip without a tag).
    pub tag: f32,
    /// Each tab's natural chip width, in tab order.
    pub tabs: &'a [f32],
    /// The tab that shows: it is always drawn.
    pub active: Option<usize>,
    /// Where the "Drop to add" slot goes (before this tab; the end when it is the number of
    /// tabs), and its natural width.
    pub drop: Option<(usize, f32)>,
}

/// Where everything goes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub tag: Option<Span>,
    /// The chips drawn: each tab's index and span, in tab order.
    pub chips: Vec<(usize, Span)>,
    /// The "+N" chip and how many tabs it stands for.
    pub more: Option<(Span, usize)>,
    pub drop: Option<Span>,
}

impl Layout {
    /// The tab whose chip is at `x`.
    pub fn tab_at(&self, x: f32) -> Option<usize> {
        self.chips.iter().find(|(_, s)| s.contains(x)).map(|(i, _)| *i)
    }

    /// Where a tab dropped at `x` goes: before the first drawn chip whose middle is right of
    /// `x` (its tab index), or `None` for the end.
    pub fn insert_before(&self, x: f32) -> Option<usize> {
        self.chips
            .iter()
            .find(|(_, s)| x < s.x + s.w / 2.0)
            .map(|(i, _)| *i)
    }

    /// Where the last thing drawn ends (0 when nothing is).
    pub fn end(&self) -> f32 {
        self.chips
            .iter()
            .map(|(_, s)| *s)
            .chain(self.tag)
            .chain(self.more.map(|(s, _)| s))
            .chain(self.drop)
            .map(Span::end)
            .fold(0.0, f32::max)
    }

    /// The same layout against the right end of a strip `width` wide, as in a title bar, where
    /// the strip sits next to the window's buttons.
    pub fn align_right(mut self, width: f32) -> Self {
        let dx = (width - self.end()).max(0.0);
        let shift = |s: &mut Span| s.x += dx;
        self.chips.iter_mut().for_each(|(_, s)| shift(s));
        self.tag.iter_mut().for_each(shift);
        self.more.iter_mut().for_each(|(s, _)| shift(s));
        self.drop.iter_mut().for_each(shift);
        self
    }
}

/// Shares `room` among chips of these natural widths, none below `min`: chips that would go
/// below it are held at it and the others shrink in proportion.
fn fit(naturals: &[f32], room: f32, min: f32) -> Vec<f32> {
    let total: f32 = naturals.iter().sum();
    if total <= room {
        return naturals.to_vec();
    }
    let mut held = vec![false; naturals.len()];
    loop {
        let held_n = held.iter().filter(|h| **h).count();
        let flexible: f32 = naturals
            .iter()
            .zip(&held)
            .filter(|(_, h)| !**h)
            .map(|(w, _)| w)
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let left = room - min * held_n as f32;
        let factor = if flexible > 0.0 {
            (left / flexible).max(0.0)
        } else {
            0.0
        };
        let mut changed = false;
        for (i, w) in naturals.iter().enumerate() {
            if !held[i] && w * factor < min {
                held[i] = true;
                changed = true;
            }
        }
        if !changed {
            return naturals
                .iter()
                .zip(&held)
                .map(|(w, h)| if *h { min } else { w * factor })
                .collect();
        }
    }
}

/// Lays the strip out.
pub fn layout(input: &Input<'_>, m: Metrics) -> Layout {
    let width = input.width.max(0.0);
    let mut out = Layout::default();
    // The tag takes at most two fifths of the strip; its text ends with an ellipsis.
    let tag = input.tag.min(width * 0.4).max(0.0);
    let mut x = 0.0;
    if tag > 0.0 {
        out.tag = Some(Span { x: 0.0, w: tag });
        x = tag + m.gap * 2.0;
    }
    let drop_w = input.drop.map_or(0.0, |(_, w)| w.min(width * 0.4));
    let drop_room = if input.drop.is_some() { drop_w + m.gap } else { 0.0 };
    let room = (width - x - drop_room).max(0.0);
    let n = input.tabs.len();
    let naturals: Vec<f32> = input
        .tabs
        .iter()
        .map(|w| w.clamp(m.min_chip, m.max_chip))
        .collect();
    let per_chip = m.min_chip + m.gap;
    #[allow(clippy::cast_precision_loss)]
    let all_fit = per_chip * n as f32 <= room;
    // Which tabs are drawn: all of them, or as many as fit around the visible one.
    let (start, count) = if all_fit {
        (0, n)
    } else {
        let room_left = (room - m.more - m.gap).max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = ((room_left / per_chip).floor() as usize).min(n);
        let active = input.active.unwrap_or(0).min(n.saturating_sub(1));
        let start = active.saturating_sub(k / 2).min(n - k);
        (start, k)
    };
    let shown = &naturals[start..start + count];
    let hidden = n - count;
    #[allow(clippy::cast_precision_loss)]
    let gaps = m.gap * count as f32;
    let chip_room = if hidden > 0 { room - m.more - m.gap } else { room };
    let widths = fit(shown, (chip_room - gaps).max(0.0), m.min_chip);
    // The drop slot goes before the tab it names, or at the nearer end of the drawn chips.
    let drop_before = input.drop.map(|(at, _)| at.clamp(start, start + count));
    // In a strip too narrow for anything (a sliver of a zone), what does not fit is left out.
    let fits = |x: f32, w: f32| x + w <= width + 0.01;
    for (k, w) in widths.iter().enumerate() {
        let index = start + k;
        if drop_before == Some(index) && fits(x, drop_w) {
            out.drop = Some(Span { x, w: drop_w });
            x += drop_w + m.gap;
        }
        out.chips.push((index, Span { x, w: *w }));
        x += w + m.gap;
    }
    if drop_before == Some(start + count) && fits(x, drop_w) {
        out.drop = Some(Span { x, w: drop_w });
        x += drop_w + m.gap;
    }
    if hidden > 0 && fits(x, m.more) {
        out.more = Some((Span { x, w: m.more }, hidden));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn m() -> Metrics {
        Metrics::at(1.0)
    }

    #[test]
    fn short_titles_keep_their_width() {
        let tabs = [80.0, 120.0];
        let l = layout(
            &Input {
                width: 900.0,
                tag: 60.0,
                tabs: &tabs,
                active: Some(0),
                drop: None,
            },
            m(),
        );
        assert_eq!(l.tag, Some(Span { x: 0.0, w: 60.0 }));
        assert_eq!(
            l.chips,
            vec![(0, Span { x: 68.0, w: 80.0 }), (1, Span { x: 152.0, w: 120.0 })]
        );
        assert_eq!(l.more, None);
        assert_eq!(l.tab_at(70.0), Some(0));
        assert_eq!(l.tab_at(150.0), None, "the gap between chips");
        assert_eq!(l.insert_before(100.0), Some(0));
        assert_eq!(l.insert_before(110.0), Some(1));
        assert_eq!(l.insert_before(500.0), None);
    }

    #[test]
    fn long_titles_shrink_and_short_ones_hold() {
        let tabs = [300.0, 300.0, 60.0];
        let l = layout(
            &Input {
                width: 400.0,
                tag: 0.0,
                tabs: &tabs,
                active: Some(2),
                drop: None,
            },
            m(),
        );
        assert_eq!(l.chips.len(), 3);
        let w: Vec<f32> = l.chips.iter().map(|(_, s)| s.w).collect();
        assert!((w[0] - w[1]).abs() < 0.01 && w[0] > w[2], "{w:?}");
        assert!(l.chips[2].1.end() <= 400.0);
    }

    #[test]
    fn tabs_that_do_not_fit_go_in_a_more_chip_around_the_visible_one() {
        let tabs = [150.0; 10];
        let l = layout(
            &Input {
                width: 400.0,
                tag: 300.0,
                tabs: &tabs,
                active: Some(9),
                drop: None,
            },
            m(),
        );
        assert!(
            (l.tag.unwrap().w - 160.0).abs() < 0.01,
            "the tag takes two fifths at most"
        );
        let drawn: Vec<usize> = l.chips.iter().map(|(i, _)| *i).collect();
        assert!(drawn.contains(&9), "{drawn:?}");
        let (more, hidden) = l.more.unwrap();
        assert_eq!(hidden + drawn.len(), 10);
        assert!(more.end() <= 400.0);
    }

    #[test]
    fn the_drop_slot_goes_between_chips() {
        let tabs = [80.0, 80.0];
        let l = layout(
            &Input {
                width: 900.0,
                tag: 0.0,
                tabs: &tabs,
                active: Some(0),
                drop: Some((1, 100.0)),
            },
            m(),
        );
        let drop = l.drop.unwrap();
        assert!(drop.x > l.chips[0].1.end() && drop.end() < l.chips[1].1.x);
    }

    proptest! {
        /// Whatever the titles and the room: everything drawn is inside the strip, nothing
        /// overlaps, chips are in tab order, the visible tab is drawn whenever any chip is, and
        /// every tab is either drawn or counted in the "+N" chip.
        #[test]
        fn everything_fits(
            width in 0.0f32..2500.0,
            tag in 0.0f32..500.0,
            tabs in proptest::collection::vec(1.0f32..600.0, 0..24),
            active in 0usize..24,
            drop in proptest::option::of((0usize..25, 1.0f32..300.0)),
        ) {
            let active = (!tabs.is_empty()).then(|| active % tabs.len());
            let drop = drop.map(|(at, w)| (at.min(tabs.len()), w));
            let l = layout(&Input { width, tag, tabs: &tabs, active, drop }, m());
            let mut spans: Vec<Span> = l.chips.iter().map(|(_, s)| *s).collect();
            spans.extend(l.tag);
            spans.extend(l.more.map(|(s, _)| s));
            spans.extend(l.drop);
            for s in &spans {
                prop_assert!(s.x >= 0.0 && s.w >= 0.0);
                prop_assert!(s.end() <= width + 0.01, "{s:?} past {width}");
            }
            spans.sort_by(|a, b| a.x.total_cmp(&b.x));
            for pair in spans.windows(2) {
                prop_assert!(pair[0].end() <= pair[1].x + 0.01, "{pair:?} overlap");
            }
            let drawn: Vec<usize> = l.chips.iter().map(|(i, _)| *i).collect();
            prop_assert!(drawn.windows(2).all(|p| p[0] < p[1]));
            // Every tab is drawn or counted, unless the strip is too narrow for even the "+N"
            // chip.
            if l.more.is_some() || !drawn.is_empty() || tabs.is_empty() {
                prop_assert_eq!(drawn.len() + l.more.map_or(0, |(_, n)| n), tabs.len());
            } else {
                prop_assert!(width < m().more + m().gap * 3.0 + l.tag.map_or(0.0, |t| t.w) + l.drop.map_or(0.0, |d| d.w + m().gap));
            }
            if let (Some(a), false) = (active, drawn.is_empty()) {
                prop_assert!(drawn.contains(&a), "visible tab {a} not drawn: {drawn:?}");
            }
            // Against the right end (in a title bar): the same chips, still inside, ending there.
            let right = l.clone().align_right(width);
            prop_assert_eq!(right.chips.len(), l.chips.len());
            if l.end() > 0.0 {
                prop_assert!((right.end() - width).abs() < 0.01, "ends at {} not {width}", right.end());
            }
            for ((_, a), (_, b)) in l.chips.iter().zip(&right.chips) {
                prop_assert!((b.w - a.w).abs() < 0.001 && b.x >= a.x - 0.001 && b.end() <= width + 0.01);
            }
        }
    }
}
