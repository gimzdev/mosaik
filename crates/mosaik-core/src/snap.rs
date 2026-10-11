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
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            invert_shift: false,
            near: 48,
        }
    }
}

impl SnapSettings {
    /// Whether zones show (and the window snaps) given the state of the Shift key.
    pub fn active(self, shift_down: bool) -> bool {
        self.enabled && shift_down == self.invert_shift
    }
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
