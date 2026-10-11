//! The `Topology` fixture library: monitor arrangements the owner cannot plug in, so multi-monitor
//! and high-DPI behaviour is tested by numbers on every build. Enable the `fixtures` feature to use
//! it from other crates.

use crate::geometry::{Rect, scale_for_dpi};
use crate::topology::{Monitor, MonitorId, Topology};

/// Which edge the taskbar sits on and whether it hides itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskbarSpec {
    pub edge: Edge,
    pub auto_hide: bool,
    /// Thickness at 100% scale. Windows 11's taskbar is 48 px.
    pub logical_thickness: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

impl TaskbarSpec {
    pub const BOTTOM: Self = Self::on(Edge::Bottom);
    pub const NONE: Self = Self {
        edge: Edge::Bottom,
        auto_hide: true,
        logical_thickness: 0,
    };

    pub const fn on(edge: Edge) -> Self {
        Self {
            edge,
            auto_hide: false,
            logical_thickness: 48,
        }
    }

    pub const fn auto_hidden(edge: Edge) -> Self {
        Self {
            edge,
            auto_hide: true,
            logical_thickness: 48,
        }
    }

    /// The work area Windows reports for a monitor with this taskbar. An auto-hidden taskbar
    /// leaves the whole monitor as work area.
    pub fn work_area(self, bounds: Rect, dpi: u32) -> Rect {
        if self.auto_hide {
            return bounds;
        }
        let t = scale_for_dpi(self.logical_thickness, dpi);
        match self.edge {
            Edge::Left => Rect::new(bounds.left + t, bounds.top, bounds.right, bounds.bottom),
            Edge::Top => Rect::new(bounds.left, bounds.top + t, bounds.right, bounds.bottom),
            Edge::Right => Rect::new(bounds.left, bounds.top, bounds.right - t, bounds.bottom),
            Edge::Bottom => Rect::new(bounds.left, bounds.top, bounds.right, bounds.bottom - t),
        }
    }
}

/// A non-primary monitor. Wrap it in [`primary`] for the primary one.
pub fn monitor(id: &str, bounds: Rect, dpi: u32, taskbar: TaskbarSpec) -> Monitor {
    Monitor {
        id: MonitorId::new(id),
        name: id.to_owned(),
        bounds,
        work_area: taskbar.work_area(bounds, dpi),
        dpi,
        primary: false,
    }
}

pub fn primary(mut m: Monitor) -> Monitor {
    m.primary = true;
    m
}

fn topology(monitors: Vec<Monitor>) -> Topology {
    match Topology::new(monitors) {
        Ok(t) => t,
        // Fixtures are written by hand; an empty one is a bug in this file.
        Err(e) => unreachable!("fixture topology is invalid: {e}"),
    }
}

/// One 1920x1080 monitor at 100%, taskbar at the bottom: the owner's desk.
pub fn single_1080p_100() -> Topology {
    single_with_taskbar(TaskbarSpec::BOTTOM)
}

/// The owner's monitor with the taskbar somewhere else, or auto-hidden.
pub fn single_with_taskbar(taskbar: TaskbarSpec) -> Topology {
    topology(vec![primary(monitor(
        "primary-1080p",
        Rect::new(0, 0, 1920, 1080),
        96,
        taskbar,
    ))])
}

/// The owner's monitor at another scale (125%, 150%, 200%...). Resolution does not change.
pub fn single_1080p_at(dpi: u32) -> Topology {
    topology(vec![primary(monitor(
        "primary-1080p",
        Rect::new(0, 0, 1920, 1080),
        dpi,
        TaskbarSpec::BOTTOM,
    ))])
}

/// A 4K monitor at 200% as primary, with a 1080p monitor at 100% to its right.
pub fn uhd_200_plus_1080p_100() -> Topology {
    topology(vec![
        primary(monitor(
            "primary-4k",
            Rect::new(0, 0, 3840, 2160),
            192,
            TaskbarSpec::BOTTOM,
        )),
        monitor(
            "right-1080p",
            Rect::new(3840, 540, 5760, 1620),
            96,
            TaskbarSpec::BOTTOM,
        ),
    ])
}

/// Three monitors: 1080p at 100% on the left (negative x), 1440p at 125% primary, 1080p at
/// 100% on the right, offset vertically.
pub fn three_monitors() -> Topology {
    topology(vec![
        monitor(
            "left-1080p",
            Rect::new(-1920, 180, 0, 1260),
            96,
            TaskbarSpec::BOTTOM,
        ),
        primary(monitor(
            "primary-1440p",
            Rect::new(0, 0, 2560, 1440),
            120,
            TaskbarSpec::BOTTOM,
        )),
        monitor(
            "right-1080p",
            Rect::new(2560, 180, 4480, 1260),
            96,
            TaskbarSpec::BOTTOM,
        ),
    ])
}

/// A landscape 1440p primary with a portrait 1080x1920 monitor to its right, top above the
/// primary's top.
pub fn portrait_beside_landscape() -> Topology {
    topology(vec![
        primary(monitor(
            "primary-1440p",
            Rect::new(0, 0, 2560, 1440),
            96,
            TaskbarSpec::BOTTOM,
        )),
        monitor(
            "portrait-1080x1920",
            Rect::new(2560, -240, 3640, 1680),
            96,
            TaskbarSpec::NONE,
        ),
    ])
}

/// One 3440x1440 ultrawide at 100%.
pub fn ultrawide() -> Topology {
    topology(vec![primary(monitor(
        "ultrawide",
        Rect::new(0, 0, 3440, 1440),
        96,
        TaskbarSpec::BOTTOM,
    ))])
}

/// A 1080p primary with a 1440p monitor at 150% to its left and a 1080p monitor above it, so
/// both x and y go negative.
pub fn left_and_above_primary() -> Topology {
    topology(vec![
        primary(monitor(
            "primary-1080p",
            Rect::new(0, 0, 1920, 1080),
            96,
            TaskbarSpec::BOTTOM,
        )),
        monitor(
            "left-1440p",
            Rect::new(-2560, -300, 0, 1140),
            144,
            TaskbarSpec::BOTTOM,
        ),
        monitor("above-1080p", Rect::new(0, -1080, 1920, 0), 96, TaskbarSpec::NONE),
    ])
}

/// The two monitors a Virtual Display Driver test adds: 4K at 200% and 1080p at 125%, next to
/// the owner's physical monitor.
pub fn owner_with_two_virtual() -> Topology {
    topology(vec![
        primary(monitor(
            "primary-1080p",
            Rect::new(0, 0, 1920, 1080),
            96,
            TaskbarSpec::BOTTOM,
        )),
        monitor(
            "vdd-4k",
            Rect::new(1920, -540, 5760, 1620),
            192,
            TaskbarSpec::NONE,
        ),
        monitor("vdd-1080p", Rect::new(-1920, 0, 0, 1080), 120, TaskbarSpec::NONE),
    ])
}

/// Every fixture, including the taskbar on each edge and auto-hidden, for "across any
/// topology" tests.
pub fn all() -> Vec<Topology> {
    let mut v = vec![
        single_1080p_100(),
        single_1080p_at(120),
        single_1080p_at(144),
        single_1080p_at(192),
        uhd_200_plus_1080p_100(),
        three_monitors(),
        portrait_beside_landscape(),
        ultrawide(),
        left_and_above_primary(),
        owner_with_two_virtual(),
    ];
    for edge in [Edge::Left, Edge::Top, Edge::Right, Edge::Bottom] {
        v.push(single_with_taskbar(TaskbarSpec::on(edge)));
        v.push(single_with_taskbar(TaskbarSpec::auto_hidden(edge)));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_well_formed() {
        for t in all() {
            assert_eq!(t.monitors().iter().filter(|m| m.primary).count(), 1);
            let p = t.primary();
            assert_eq!(
                (p.bounds.left, p.bounds.top),
                (0, 0),
                "Windows always puts the primary at (0, 0)"
            );
            for m in t.monitors() {
                assert!(m.bounds.contains_rect(m.work_area), "{}", m.id.0);
                for o in t.monitors().iter().filter(|o| o.id != m.id) {
                    assert_eq!(
                        m.bounds.overlap_area(o.bounds),
                        0,
                        "{} overlaps {}",
                        m.id.0,
                        o.id.0
                    );
                }
            }
        }
    }

    #[test]
    fn taskbar_thickness_scales_with_dpi() {
        let t = uhd_200_plus_1080p_100();
        assert_eq!(t.primary().work_area, Rect::new(0, 0, 3840, 2160 - 96));
        let left = single_with_taskbar(TaskbarSpec::on(Edge::Left));
        assert_eq!(left.primary().work_area.left, 48);
        let hidden = single_with_taskbar(TaskbarSpec::auto_hidden(Edge::Bottom));
        assert_eq!(hidden.primary().work_area, hidden.primary().bounds);
    }
}
