//! Drag to snap: while a window is dragged, the zone under (or near) the cursor lights up; on
//! release the window is pulled into it. Holding Shift moves the window freely; a setting
//! swaps that, so zones only appear while Shift is held.

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect, scale_for_dpi};
use crate::zone::ZoneId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapSettings {
    /// Show zones while dragging and snap on release.
    pub enabled: bool,
    /// Snap only while Shift is held, instead of unless Shift is held.
    pub invert_shift: bool,
    /// How close (in device-independent pixels) the cursor must be to a zone to be pulled in.
    pub near: i32,
    /// Resizing a window in a zone by an edge it shares with other zones moves that edge for
    /// all of them, and their windows follow (`crate::lines::follow_resize`). Shift resizes
    /// freely, as it moves freely.
    pub resize_zones: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            invert_shift: false,
            near: 48,
            resize_zones: true,
        }
    }
}

impl SnapSettings {
    /// Whether zones show (and the window snaps) given the state of the Shift key.
    pub fn active(self, shift_down: bool) -> bool {
        self.enabled && shift_down == self.invert_shift
    }
}

/// What a drag of a window turns out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    /// Not known yet: the window has not changed since the drag began.
    Undecided,
    Moving,
    /// By an edge or a corner, or from the keyboard: never snaps.
    Resizing,
}

/// The pointer during a drag, as far as it tells a move from a resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// One of Windows' resize arrows: Windows is resizing the window, whatever the window is.
    ResizeArrow,
    /// Any other pointer.
    Other,
    /// No pointer on screen (touch, pen): it tells nothing.
    Hidden,
}

impl DragKind {
    /// When a drag begins. `inside`: the pointer is inside the window's frame, away from its
    /// edges. By the time the start is noticed a quick drag can have carried the pointer out of
    /// the window (Windows only starts a move once the pointer moves), so the position counts
    /// only when the pointer's shape tells nothing.
    pub fn at_start(pointer: Pointer, inside: bool) -> Self {
        match pointer {
            Pointer::ResizeArrow => Self::Resizing,
            Pointer::Other => Self::Undecided,
            Pointer::Hidden if inside => Self::Undecided,
            Pointer::Hidden => Self::Resizing,
        }
    }

    /// For a drag still undecided: the window's frame was `start` when it began and is `now`.
    pub fn once_changed(start: Rect, now: Rect, pointer: Pointer) -> Self {
        if now == start {
            return Self::Undecided;
        }
        if pointer == Pointer::ResizeArrow {
            return Self::Resizing;
        }
        if now.size() == start.size() {
            return Self::Moving;
        }
        // The size changed. A resize by an edge or a corner, with the mouse, a pen or the
        // keyboard, keeps the opposite side and the opposite top or bottom where they were; a
        // maximised or snapped window dragged by its title bar takes back its own size and
        // travels with the pointer, every side moving.
        let kept_side = now.left == start.left || now.right == start.right;
        let kept_top_or_bottom = now.top == start.top || now.bottom == start.bottom;
        if kept_side && kept_top_or_bottom {
            Self::Resizing
        } else {
            Self::Moving
        }
    }
}

/// `p` is where Windows' own snapping reacts to a dragged window: on the left, right or top edge
/// of the desktop, where the pointer stops (the edge pixel of a monitor with no monitor beyond
/// it). The bottom edge counts only at its corners, which are on the left and right edges.
pub fn at_screen_edge(p: Point, monitors: &[Rect]) -> bool {
    let on_any = |q: Point| monitors.iter().any(|m| m.contains(q));
    monitors.iter().filter(|m| m.contains(p)).any(|m| {
        (p.x == m.left && !on_any(Point::new(p.x - 1, p.y)))
            || (p.x == m.right - 1 && !on_any(Point::new(p.x + 1, p.y)))
            || (p.y == m.top && !on_any(Point::new(p.x, p.y - 1)))
    })
}

/// A zone as it is on screen right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveZone {
    pub id: ZoneId,
    pub rect: Rect,
    /// DPI of the zone's monitor, to scale the "near" distance.
    pub dpi: u32,
}

/// The zone a window dragged with the cursor at `cursor` should go to. Inside zones, the
/// smallest zone containing the cursor wins (so a small zone drawn over a big one stays
/// reachable); outside them, the closest zone within the "near" distance.
pub fn target(cursor: Point, zones: &[LiveZone], settings: SnapSettings) -> Option<&ZoneId> {
    let inside = zones.iter().filter(|z| z.rect.contains(cursor)).min_by_key(|z| {
        (
            z.rect.area(),
            z.rect.center().x.abs_diff(cursor.x) + z.rect.center().y.abs_diff(cursor.y),
        )
    });
    if let Some(z) = inside {
        return Some(&z.id);
    }
    zones
        .iter()
        .filter_map(|z| {
            let d2 = z.rect.distance_squared_to(cursor);
            let near = i64::from(scale_for_dpi(settings.near, z.dpi));
            (d2 <= near * near).then_some((d2, z))
        })
        .min_by_key(|(d2, z)| (*d2, z.rect.area()))
        .map(|(_, z)| &z.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_edges_are_where_the_pointer_stops() {
        let one = [Rect::new(0, 0, 1920, 1080)];
        assert!(at_screen_edge(Point::new(576, 0), &one));
        assert!(at_screen_edge(Point::new(0, 516), &one));
        assert!(at_screen_edge(Point::new(1919, 300), &one));
        assert!(!at_screen_edge(Point::new(528, 18), &one));
        // Two side by side, the second one above the line of the first: the shared edge is
        // crossed, not an edge; the part of the second's left edge above the first is one.
        let two = [Rect::new(0, 0, 1920, 1080), Rect::new(1920, -200, 3840, 880)];
        assert!(!at_screen_edge(Point::new(1919, 500), &two));
        assert!(!at_screen_edge(Point::new(1920, 500), &two));
        assert!(at_screen_edge(Point::new(1920, -100), &two));
        assert!(at_screen_edge(Point::new(2500, -200), &two));
        assert!(!at_screen_edge(Point::new(5000, 0), &two));
    }

    #[test]
    fn any_edge_or_corner_resize_without_arrows_is_a_resize() {
        // Pen input, apps drawing their own pointer, cursor tools: the pointer tells nothing,
        // and the frame does: an edge or corner resize keeps one side and one of top or bottom.
        let start = Rect::new(100, 100, 900, 700);
        for now in [
            Rect::new(60, 100, 900, 700),  // left edge
            Rect::new(100, 60, 900, 700),  // top edge
            Rect::new(60, 60, 900, 700),   // top-left corner
            Rect::new(100, 60, 940, 700),  // top-right corner
            Rect::new(60, 100, 900, 740),  // bottom-left corner
            Rect::new(100, 100, 940, 740), // bottom-right corner
        ] {
            for pointer in [Pointer::Other, Pointer::Hidden] {
                assert_eq!(
                    DragKind::once_changed(start, now, pointer),
                    DragKind::Resizing,
                    "{now:?}"
                );
            }
        }
    }

    #[test]
    fn windows_snapping_does_not_watch_the_middle_of_the_bottom_edge() {
        let one = [Rect::new(0, 0, 1920, 1080)];
        // Over the taskbar, at the bottom: Windows does not snap there.
        assert!(!at_screen_edge(Point::new(900, 1079), &one));
        // Its corners are the left and right edges, which it does watch.
        assert!(at_screen_edge(Point::new(0, 1079), &one));
        assert!(at_screen_edge(Point::new(1919, 1079), &one));
    }

    #[test]
    fn a_resize_arrow_means_a_resize_from_the_start() {
        assert_eq!(DragKind::at_start(Pointer::ResizeArrow, true), DragKind::Resizing);
        assert_eq!(
            DragKind::at_start(Pointer::ResizeArrow, false),
            DragKind::Resizing
        );
    }

    #[test]
    fn a_quick_drag_out_of_the_window_is_still_a_move() {
        // The pointer already left the window when the start was noticed (dragged up from the
        // title bar onto a tag): the arrow pointer says it is no resize.
        let start = Rect::new(964, 552, 1916, 1028);
        assert_eq!(DragKind::at_start(Pointer::Other, false), DragKind::Undecided);
        let moved = Rect::new(900, 520, 1852, 996);
        assert_eq!(
            DragKind::once_changed(start, moved, Pointer::Other),
            DragKind::Moving
        );
    }

    #[test]
    fn without_a_pointer_the_position_decides() {
        assert_eq!(DragKind::at_start(Pointer::Hidden, true), DragKind::Undecided);
        assert_eq!(DragKind::at_start(Pointer::Hidden, false), DragKind::Resizing);
    }

    #[test]
    fn what_the_frame_does_decides_the_rest() {
        let start = Rect::new(100, 100, 900, 700);
        // Nothing yet.
        assert_eq!(
            DragKind::once_changed(start, start, Pointer::Other),
            DragKind::Undecided
        );
        // A resize by the left edge moves the origin too: the arrow tells.
        let left_edge = Rect::new(60, 100, 900, 700);
        assert_eq!(
            DragKind::once_changed(start, left_edge, Pointer::ResizeArrow),
            DragKind::Resizing
        );
        // From the keyboard: the size changes, the window stays.
        let keyboard = Rect::new(100, 100, 940, 700);
        assert_eq!(
            DragKind::once_changed(start, keyboard, Pointer::Other),
            DragKind::Resizing
        );
        // A maximised window dragged by its title bar takes back its size and travels.
        let maximised = Rect::new(0, 0, 1920, 1032);
        let restored = Rect::new(500, 10, 1300, 610);
        assert_eq!(
            DragKind::once_changed(maximised, restored, Pointer::Other),
            DragKind::Moving
        );
        // Touch: no pointer, the frame alone.
        let moved = Rect::new(140, 120, 940, 720);
        assert_eq!(
            DragKind::once_changed(start, moved, Pointer::Hidden),
            DragKind::Moving
        );
    }

    fn zones() -> Vec<LiveZone> {
        vec![
            LiveZone {
                id: ZoneId::new("code"),
                rect: Rect::new(0, 0, 1152, 1032),
                dpi: 96,
            },
            LiveZone {
                id: ZoneId::new("terminal"),
                rect: Rect::new(1160, 0, 1920, 516),
                dpi: 96,
            },
            LiveZone {
                id: ZoneId::new("review"),
                rect: Rect::new(1160, 516, 1920, 1032),
                dpi: 96,
            },
            // A small zone drawn on top of "code".
            LiveZone {
                id: ZoneId::new("notes"),
                rect: Rect::new(100, 100, 400, 300),
                dpi: 96,
            },
        ]
    }

    #[test]
    fn shift_moves_freely_unless_inverted() {
        let s = SnapSettings::default();
        assert!(s.active(false));
        assert!(!s.active(true));
        let inverted = SnapSettings {
            invert_shift: true,
            ..s
        };
        assert!(!inverted.active(false));
        assert!(inverted.active(true));
        assert!(!SnapSettings { enabled: false, ..s }.active(false));
    }

    #[test]
    fn the_smallest_zone_under_the_cursor_wins() {
        let z = zones();
        let s = SnapSettings::default();
        assert_eq!(target(Point::new(200, 200), &z, s), Some(&ZoneId::new("notes")));
        assert_eq!(target(Point::new(800, 900), &z, s), Some(&ZoneId::new("code")));
        assert_eq!(target(Point::new(1500, 700), &z, s), Some(&ZoneId::new("review")));
    }

    #[test]
    fn near_a_zone_pulls_it_in_far_does_not() {
        let z = zones();
        let s = SnapSettings::default();
        // In the 8 px gap between code and terminal: closer to terminal.
        assert_eq!(
            target(Point::new(1157, 100), &z, s),
            Some(&ZoneId::new("terminal"))
        );
        // Below the work area, 40 px from "review".
        assert_eq!(
            target(Point::new(1500, 1071), &z, s),
            Some(&ZoneId::new("review"))
        );
        assert_eq!(target(Point::new(1500, 1200), &z, s), None);
        assert_eq!(target(Point::new(10, 10), &[], s), None);
    }
}
