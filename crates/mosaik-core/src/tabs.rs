//! Which windows of tab groups show and which hide, as a decision on plain data. The engine says
//! what it knows (each group's open tabs, the tab that should show, whether its strip can be on
//! screen, which windows it keeps hidden) and applies the plan.
//!
//! The rule that keeps windows from being lost: a window stays hidden only while it is a
//! background tab of a group of the open workspace whose other tabs can be reached, through its
//! strip or through its visible tab's taskbar button. Every other window Mosaïk hid shows again.

use std::collections::HashSet;

use crate::layout::{SlotId, TabGroupId};
use crate::window::WindowHandle;

/// How a group's background tabs can be reached right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Through its strip, on screen above the visible tab.
    Strip,
    /// The visible tab is minimised or maximised: its strip comes back with it, and its taskbar
    /// button leads there meanwhile.
    Later,
    /// No strip can be drawn (no room above the window, drawing failed, a window would not
    /// hide): every tab shows, stacked in the group's place.
    None,
}

/// A tab group of the open workspace, as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupNow {
    pub id: TabGroupId,
    /// Every tab in order, open or not.
    pub order: Vec<SlotId>,
    /// The open windows filling its tabs, in tab order. Tabs whose window is closed, or hidden
    /// by its own app (closed to the tray), are left out.
    pub open: Vec<(SlotId, WindowHandle)>,
    /// The tab that should show: the user's choice, or one an app brought forward.
    pub wanted: Option<SlotId>,
    pub reach: Reach,
}

impl GroupNow {
    /// The tab that shows: the wanted one when it is open; else the open tab to its right, or
    /// else to its left (as when the visible tab closes); else the first open one.
    pub fn visible(&self) -> Option<&(SlotId, WindowHandle)> {
        let open = |s: &SlotId| self.open.iter().find(|(o, _)| o == s);
        let Some(wanted) = &self.wanted else {
            return self.open.first();
        };
        if let Some(t) = open(wanted) {
            return Some(t);
        }
        match self.order.iter().position(|s| s == wanted) {
            Some(i) => self.order[i + 1..]
                .iter()
                .find_map(open)
                .or_else(|| self.order[..i].iter().rev().find_map(open)),
            None => self.open.first(),
        }
    }
}

/// What to do so the screen matches the groups.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabPlan {
    /// The tab each group shows.
    pub visible: Vec<(TabGroupId, SlotId, WindowHandle)>,
    /// Background tabs that are to be hidden (those hidden already included).
    pub hide: Vec<WindowHandle>,
    /// Windows Mosaïk hid that are to show again: visible tabs, tabs of groups that cannot be
    /// reached, and windows that are no longer a tab of the open workspace.
    pub show: Vec<WindowHandle>,
}

/// Decides which windows show and which hide. `hidden` lists the windows Mosaïk keeps hidden.
pub fn plan_tabs(groups: &[GroupNow], hidden: &[WindowHandle]) -> TabPlan {
    let mut plan = TabPlan::default();
    for g in groups {
        if let Some((slot, window)) = g.visible() {
            plan.visible.push((g.id.clone(), slot.clone(), *window));
        }
    }
    // A window some group shows is never hidden, even if a damaged layout lists it twice.
    let shown: HashSet<WindowHandle> = plan.visible.iter().map(|(_, _, w)| *w).collect();
    for g in groups {
        if g.open.len() < 2 || g.reach == Reach::None {
            continue;
        }
        for (_, w) in &g.open {
            if !shown.contains(w) && !plan.hide.contains(w) {
                plan.hide.push(*w);
            }
        }
    }
    let keep: HashSet<WindowHandle> = plan.hide.iter().copied().collect();
    plan.show = hidden.iter().copied().filter(|h| !keep.contains(h)).collect();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: u32) -> SlotId {
        SlotId(format!("s{n}"))
    }

    fn w(n: u64) -> WindowHandle {
        WindowHandle(n)
    }

    /// A group of slots s1..=sN, where the slots in `open` have windows 1..
    fn group(n: u32, open: &[u32], wanted: Option<u32>, reach: Reach) -> GroupNow {
        GroupNow {
            id: TabGroupId("g1".into()),
            order: (1..=n).map(s).collect(),
            open: open.iter().map(|i| (s(*i), w(u64::from(*i)))).collect(),
            wanted: wanted.map(s),
            reach,
        }
    }

    #[test]
    fn the_wanted_tab_shows_and_the_others_hide() {
        let p = plan_tabs(&[group(3, &[1, 2, 3], Some(2), Reach::Strip)], &[w(2), w(3)]);
        assert_eq!(p.visible, vec![(TabGroupId("g1".into()), s(2), w(2))]);
        assert_eq!(p.hide, vec![w(1), w(3)]);
        assert_eq!(p.show, vec![w(2)], "the visible tab comes out of hiding");
    }

    #[test]
    fn a_closed_tab_gives_way_to_its_right_then_its_left() {
        assert_eq!(
            group(4, &[1, 3, 4], Some(2), Reach::Strip).visible(),
            Some(&(s(3), w(3)))
        );
        assert_eq!(
            group(4, &[1, 2], Some(4), Reach::Strip).visible(),
            Some(&(s(2), w(2)))
        );
        assert_eq!(group(4, &[3], None, Reach::Strip).visible(), Some(&(s(3), w(3))));
        assert_eq!(group(4, &[], Some(1), Reach::Strip).visible(), None);
        // A wanted slot the group does not list (a stale choice): the first open tab.
        assert_eq!(
            group(3, &[2, 3], Some(9), Reach::Strip).visible(),
            Some(&(s(2), w(2)))
        );
    }

    #[test]
    fn tabs_that_cannot_be_reached_all_show() {
        let p = plan_tabs(&[group(3, &[1, 2, 3], Some(1), Reach::None)], &[w(2), w(3)]);
        assert!(p.hide.is_empty());
        assert_eq!(p.show, vec![w(2), w(3)]);
    }

    #[test]
    fn a_minimised_group_keeps_its_background_tabs_hidden() {
        let p = plan_tabs(&[group(2, &[1, 2], Some(1), Reach::Later)], &[w(2)]);
        assert_eq!(p.hide, vec![w(2)]);
        assert!(p.show.is_empty());
    }

    #[test]
    fn a_group_with_one_open_tab_hides_nothing() {
        let p = plan_tabs(&[group(3, &[2], Some(1), Reach::Strip)], &[w(2)]);
        assert!(p.hide.is_empty());
        assert_eq!(p.show, vec![w(2)]);
    }

    #[test]
    fn windows_that_are_no_tab_any_more_show_again() {
        // w(7) was a background tab of a group that ended, or of another workspace.
        let p = plan_tabs(&[group(2, &[1, 2], Some(1), Reach::Strip)], &[w(2), w(7)]);
        assert_eq!(p.hide, vec![w(2)]);
        assert_eq!(p.show, vec![w(7)]);
        let p = plan_tabs(&[], &[w(7)]);
        assert_eq!(p.show, vec![w(7)]);
    }

    #[test]
    fn a_window_shown_by_one_group_is_never_hidden_by_another() {
        let mut other = group(2, &[1, 2], Some(1), Reach::Strip);
        other.id = TabGroupId("g2".into());
        let first = group(2, &[1, 2], Some(2), Reach::Strip);
        let p = plan_tabs(&[first, other], &[]);
        assert!(!p.hide.contains(&w(1)) && !p.hide.contains(&w(2)), "{p:?}");
    }
}
