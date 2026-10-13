//! Which edge of the main monitor holds the quick menu's handle. The pointer has to rest on the
//! handle to open the menu, which is easy where the screen's edge stops the pointer and takes a
//! pixel-perfect hand where it runs on past it, into another monitor or onto the taskbar (the
//! owner, 2026-10-13: "we have to be pixel perfect"). So the handle goes on the edge the user
//! chose when the pointer stops there, else on the other edge when it stops there.

use crate::geometry::{Point, Rect};
use crate::settings::EdgeSide;

/// The pointer stops at `side` of a monitor of `bounds` with `work` area, at height `y`: the
/// taskbar is not along that edge, and no monitor of `others` lies beyond it there.
pub fn stops_at(side: EdgeSide, bounds: Rect, work: Rect, others: &[Rect], y: i32) -> bool {
    let (along_taskbar, beyond) = match side {
        EdgeSide::Left => (work.left != bounds.left, Point::new(bounds.left - 1, y)),
        EdgeSide::Right => (work.right != bounds.right, Point::new(bounds.right, y)),
    };
    !along_taskbar && !others.iter().any(|m| m.contains(beyond))
}

/// The edge for the handle and whether the pointer runs past it: `chosen` when the pointer stops
/// there, else the other edge when it stops there, else `chosen`, run past.
pub fn handle_edge(chosen: EdgeSide, bounds: Rect, work: Rect, others: &[Rect], y: i32) -> (EdgeSide, bool) {
    let other = match chosen {
        EdgeSide::Left => EdgeSide::Right,
        EdgeSide::Right => EdgeSide::Left,
    };
    if stops_at(chosen, bounds, work, others, y) {
        (chosen, false)
    } else if stops_at(other, bounds, work, others, y) {
        (other, false)
    } else {
        (chosen, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect::new(0, 0, 1920, 1080);
    const WORK: Rect = Rect::new(0, 0, 1920, 1032);

    #[test]
    fn the_handle_stays_where_the_user_put_it_when_the_pointer_stops_there() {
        assert_eq!(
            handle_edge(EdgeSide::Right, MAIN, WORK, &[], 300),
            (EdgeSide::Right, false)
        );
        assert_eq!(
            handle_edge(EdgeSide::Left, MAIN, WORK, &[], 300),
            (EdgeSide::Left, false)
        );
    }

    #[test]
    fn a_monitor_beyond_the_chosen_edge_sends_the_handle_to_the_other_one() {
        // The owner's PC: two virtual screens of 800 x 600 to the right of the main one.
        let virtual_screen = Rect::new(1920, 0, 2720, 600);
        assert_eq!(
            handle_edge(EdgeSide::Right, MAIN, WORK, &[virtual_screen], 300),
            (EdgeSide::Left, false)
        );
        // Lower down, where nothing lies beyond the edge, the pointer stops there.
        assert_eq!(
            handle_edge(EdgeSide::Right, MAIN, WORK, &[virtual_screen], 800),
            (EdgeSide::Right, false)
        );
        // Monitors on both sides: the chosen edge, which the pointer runs past.
        let left = Rect::new(-1920, 0, 0, 1080);
        assert_eq!(
            handle_edge(EdgeSide::Right, MAIN, WORK, &[virtual_screen, left], 300),
            (EdgeSide::Right, true)
        );
    }

    #[test]
    fn the_taskbar_along_an_edge_is_not_a_place_to_stop() {
        let taskbar_right = Rect::new(0, 0, 1872, 1080);
        assert_eq!(
            handle_edge(EdgeSide::Right, MAIN, taskbar_right, &[], 300),
            (EdgeSide::Left, false)
        );
    }
}
