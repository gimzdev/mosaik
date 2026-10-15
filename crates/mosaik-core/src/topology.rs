//! Monitors and how they are arranged.
//!
//! A [`Topology`] is the set of monitors Windows reports right now. Layouts remember the
//! monitors they were made on as [`MonitorRef`]s; [`map_monitors`] decides where each
//! remembered monitor went when the arrangement changes (a monitor unplugged, a laptop docked,
//! a scale changed).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::{BASE_DPI, Point, Rect};

/// A stable name for a physical (or virtual) monitor, built by the Windows layer from the
/// display's device path. It survives reboots and resolution changes; it is not a handle.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorId(pub String);

impl MonitorId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Monitor {
    pub id: MonitorId,
    /// What the user calls it, for example "DELL U2720Q". For display only.
    pub name: String,
    /// The whole monitor, in physical pixels.
    pub bounds: Rect,
    /// The monitor minus the taskbar and other app bars.
    pub work_area: Rect,
    /// Effective DPI: 96 is 100%, 144 is 150%, 192 is 200%.
    pub dpi: u32,
    pub primary: bool,
}

impl Monitor {
    /// The scale factor as a whole percentage, e.g. 150.
    pub fn scale_percent(&self) -> u32 {
        (self.dpi * 100 + BASE_DPI / 2) / BASE_DPI
    }

    pub fn to_ref(&self) -> MonitorRef {
        MonitorRef {
            id: self.id.clone(),
            name: self.name.clone(),
            bounds: self.bounds,
            work_area: self.work_area,
            dpi: self.dpi,
            primary: self.primary,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TopologyError {
    #[error("Windows reported no monitors")]
    NoMonitors,
}

/// The monitors as Windows reports them right now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topology {
    monitors: Vec<Monitor>,
}

impl Topology {
    /// Builds a topology and repairs what Windows can briefly report during a display change:
    /// no primary or several, an empty or out-of-bounds work area, a zero DPI, duplicate ids.
    pub fn new(mut monitors: Vec<Monitor>) -> Result<Self, TopologyError> {
        monitors.retain(|m| !m.bounds.is_empty());
        if monitors.is_empty() {
            return Err(TopologyError::NoMonitors);
        }
        let mut seen = std::collections::HashSet::new();
        for (i, m) in monitors.iter_mut().enumerate() {
            if !seen.insert(m.id.clone()) {
                m.id = MonitorId(format!("{}#{i}", m.id.0));
            }
            if m.dpi == 0 {
                m.dpi = BASE_DPI;
            }
            if m.work_area.is_empty() || !m.bounds.contains_rect(m.work_area) {
                m.work_area = m.bounds;
            }
        }
        let primaries = monitors.iter().filter(|m| m.primary).count();
        if primaries != 1 {
            let chosen = monitors
                .iter()
                .position(|m| m.primary)
                .or_else(|| monitors.iter().position(|m| m.bounds.contains(Point::new(0, 0))))
                .unwrap_or(0);
            for (i, m) in monitors.iter_mut().enumerate() {
                m.primary = i == chosen;
            }
        }
        Ok(Self { monitors })
    }

    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    pub fn primary(&self) -> &Monitor {
        // `new` guarantees exactly one primary and at least one monitor.
        self.monitors
            .iter()
            .find(|m| m.primary)
            .unwrap_or(&self.monitors[0])
    }

    pub fn get(&self, id: &MonitorId) -> Option<&Monitor> {
        self.monitors.iter().find(|m| &m.id == id)
    }

    pub fn monitor_containing(&self, p: Point) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.bounds.contains(p))
    }

    /// The monitor containing `p`, or the closest one (Win32 `MONITOR_DEFAULTTONEAREST`).
    pub fn nearest_monitor(&self, p: Point) -> &Monitor {
        self.monitor_containing(p).unwrap_or_else(|| {
            self.monitors
                .iter()
                .min_by_key(|m| m.bounds.distance_squared_to(p))
                .unwrap_or(&self.monitors[0])
        })
    }

    /// The monitor sharing the most area with `r`, or the one nearest its centre.
    pub fn monitor_for_rect(&self, r: Rect) -> &Monitor {
        let best = self
            .monitors
            .iter()
            .map(|m| (m.bounds.overlap_area(r), m))
            .filter(|(area, _)| *area > 0)
            .max_by_key(|(area, _)| *area);
        match best {
            Some((_, m)) => m,
            None => self.nearest_monitor(r.center()),
        }
    }

    /// True when at least a corner region of `r` (a 32 px square by default) is on some
    /// monitor's work area, so the user can grab and move the window.
    pub fn is_reachable(&self, r: Rect, grab: i32) -> bool {
        let handle = Rect::from_origin_size(r.left, r.top, grab.min(r.width()), grab.min(r.height()));
        self.monitors.iter().any(|m| m.work_area.contains_rect(handle))
    }

    /// A short, stable fingerprint of the arrangement: monitor ids, positions and scales.
    /// Two topologies with the same fingerprint place every zone on the same pixels.
    pub fn fingerprint(&self) -> String {
        let mut parts: Vec<String> = self
            .monitors
            .iter()
            .map(|m| {
                format!(
                    "{}@{},{},{},{}/{},{},{},{}/{}{}",
                    m.id.0,
                    m.bounds.left,
                    m.bounds.top,
                    m.bounds.right,
                    m.bounds.bottom,
                    m.work_area.left,
                    m.work_area.top,
                    m.work_area.right,
                    m.work_area.bottom,
                    m.dpi,
                    if m.primary { "*" } else { "" }
                )
            })
            .collect();
        parts.sort();
        format!("{:016x}", fnv1a64(parts.join("|").as_bytes()))
    }
}

/// FNV-1a: tiny, stable across Rust versions (unlike `DefaultHasher`), good enough for a
/// fingerprint that is compared, never trusted.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A monitor as a layout remembers it: what it was called, where it was and its scale when the
/// layout was saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorRef {
    pub id: MonitorId,
    pub name: String,
    pub bounds: Rect,
    pub work_area: Rect,
    pub dpi: u32,
    pub primary: bool,
}

/// Decides which current monitor stands in for each remembered one. Returns, for every entry
/// of `saved`, an index into `now.monitors()`.
///
/// 1. The same monitor (same id) when it is still connected.
/// 2. Otherwise the unclaimed monitor in the most similar position relative to the primary
///    monitor (left of it, above it, ...), so a two-monitor layout lands sensibly on a new
///    pair of monitors.
/// 3. Otherwise (fewer monitors than before) the monitor closest to where it used to be,
///    which may hold several remembered monitors.
pub fn map_monitors(saved: &[MonitorRef], now: &Topology) -> Vec<usize> {
    let saved_primary = saved
        .iter()
        .find(|m| m.primary)
        .map_or(Point::default(), |m| m.bounds.center());
    let now_primary = now.primary().bounds.center();
    // More remembered monitors than connected ones: the rest share the closest monitor.
    saved
        .iter()
        .zip(claims(saved, now))
        .map(|(s, r)| {
            r.unwrap_or_else(|| {
                let target = Point::new(
                    s.bounds.center().x - saved_primary.x + now_primary.x,
                    s.bounds.center().y - saved_primary.y + now_primary.y,
                );
                let m = now.nearest_monitor(target);
                now.monitors().iter().position(|x| x.id == m.id).unwrap_or(0)
            })
        })
        .collect()
}

/// The index in `saved` of the remembered monitor that `monitor` (connected now) stands for,
/// for a zone or window being placed on it: its own entry, refreshed with the latest
/// arrangement; else the remembered monitor it stands in for now, so what is placed there stays
/// with the zones shown there (and goes back with them when their monitor returns); else a new
/// entry.
pub fn remembered_index(saved: &mut Vec<MonitorRef>, monitor: &Monitor, now: &Topology) -> usize {
    if let Some(i) = saved.iter().position(|r| r.id == monitor.id) {
        saved[i] = monitor.to_ref();
        return i;
    }
    let stands_in_for = claims(saved, now).iter().position(|c| {
        c.and_then(|i| now.monitors().get(i))
            .is_some_and(|m| m.id == monitor.id)
    });
    if let Some(i) = stands_in_for {
        return i;
    }
    saved.push(monitor.to_ref());
    saved.len() - 1
}

/// Steps 1 and 2 of [`map_monitors`]: remembered monitors paired one to one with connected
/// ones. A connected monitor is claimed by at most one remembered monitor, and when some
/// remembered monitor is left without one, every connected monitor is claimed.
fn claims(saved: &[MonitorRef], now: &Topology) -> Vec<Option<usize>> {
    let mut result: Vec<Option<usize>> = vec![None; saved.len()];
    let mut claimed = vec![false; now.monitors().len()];

    for (s, slot) in saved.iter().zip(result.iter_mut()) {
        if let Some(i) = now.monitors().iter().position(|m| m.id == s.id)
            && !claimed[i]
        {
            *slot = Some(i);
            claimed[i] = true;
        }
    }

    let saved_primary = saved
        .iter()
        .find(|m| m.primary)
        .map_or(Point::default(), |m| m.bounds.center());
    let now_primary = now.primary().bounds.center();
    let relative = |c: Point, origin: Point| Point::new(c.x - origin.x, c.y - origin.y);

    // A remembered primary prefers the current primary when its own monitor is gone.
    if let Some(si) = saved.iter().position(|m| m.primary)
        && result[si].is_none()
    {
        let pi = now.monitors().iter().position(|m| m.primary).unwrap_or(0);
        if !claimed[pi] {
            result[si] = Some(pi);
            claimed[pi] = true;
        }
    }

    // Pair the rest by position relative to their primary, closest pairs first.
    let mut pairs: Vec<(i64, usize, usize)> = Vec::new();
    for (si, s) in saved.iter().enumerate().filter(|(si, _)| result[*si].is_none()) {
        let sp = relative(s.bounds.center(), saved_primary);
        for (ni, n) in now.monitors().iter().enumerate().filter(|(ni, _)| !claimed[*ni]) {
            let np = relative(n.bounds.center(), now_primary);
            let dx = i64::from(sp.x) - i64::from(np.x);
            let dy = i64::from(sp.y) - i64::from(np.y);
            pairs.push((dx * dx + dy * dy, si, ni));
        }
    }
    pairs.sort_unstable();
    for (_, si, ni) in pairs {
        if result[si].is_none() && !claimed[ni] {
            result[si] = Some(ni);
            claimed[ni] = true;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    #[test]
    fn repairs_primary_and_work_area() {
        let mut a = fixtures::monitor(
            "A",
            Rect::new(0, 0, 1920, 1080),
            96,
            fixtures::TaskbarSpec::BOTTOM,
        );
        a.primary = false;
        let mut b = fixtures::monitor(
            "B",
            Rect::new(1920, 0, 3840, 1080),
            96,
            fixtures::TaskbarSpec::BOTTOM,
        );
        b.primary = false;
        b.work_area = Rect::new(0, 0, 10, 10);
        let t = Topology::new(vec![a, b]).unwrap();
        assert_eq!(
            t.primary().id,
            MonitorId::new("A"),
            "the monitor at (0, 0) becomes primary"
        );
        assert_eq!(t.monitors()[1].work_area, t.monitors()[1].bounds);
        assert!(Topology::new(vec![]).is_err());
    }

    #[test]
    fn nearest_and_rect_monitor() {
        let t = fixtures::left_and_above_primary();
        assert_eq!(
            t.nearest_monitor(Point::new(-100, 500)).id,
            MonitorId::new("left-1440p")
        );
        assert_eq!(
            t.nearest_monitor(Point::new(5000, 5000)).id,
            MonitorId::new("primary-1080p")
        );
        let straddling = Rect::new(-300, 100, 200, 600);
        assert_eq!(t.monitor_for_rect(straddling).id, MonitorId::new("left-1440p"));
    }

    #[test]
    fn fingerprint_changes_with_scale() {
        let a = fixtures::single_1080p_100();
        let mut monitors = a.monitors().to_vec();
        assert_eq!(
            a.fingerprint(),
            Topology::new(monitors.clone()).unwrap().fingerprint()
        );
        monitors[0].dpi = 120;
        assert_ne!(a.fingerprint(), Topology::new(monitors).unwrap().fingerprint());
    }

    #[test]
    fn same_monitors_map_to_themselves() {
        for t in fixtures::all() {
            let saved: Vec<_> = t.monitors().iter().map(Monitor::to_ref).collect();
            let map = map_monitors(&saved, &t);
            assert_eq!(map, (0..saved.len()).collect::<Vec<_>>(), "{}", t.fingerprint());
        }
    }

    #[test]
    fn unplugged_monitor_falls_back_to_a_remaining_one() {
        let three = fixtures::three_monitors();
        let saved: Vec<_> = three.monitors().iter().map(Monitor::to_ref).collect();
        let one = fixtures::single_1080p_100();
        let map = map_monitors(&saved, &one);
        assert_eq!(map, vec![0, 0, 0]);
    }

    #[test]
    fn new_pair_of_monitors_keeps_left_and_right() {
        // Saved on primary + right monitor; now docked to a different pair: primary + right.
        let saved = fixtures::uhd_200_plus_1080p_100();
        let saved: Vec<_> = saved.monitors().iter().map(Monitor::to_ref).collect();
        let now = Topology::new(vec![
            fixtures::monitor(
                "other-right",
                Rect::new(2560, 0, 4480, 1080),
                96,
                fixtures::TaskbarSpec::BOTTOM,
            ),
            fixtures::primary(fixtures::monitor(
                "other-primary",
                Rect::new(0, 0, 2560, 1440),
                120,
                fixtures::TaskbarSpec::BOTTOM,
            )),
        ])
        .unwrap();
        let map = map_monitors(&saved, &now);
        assert_eq!(now.monitors()[map[0]].id, MonitorId::new("other-primary"));
        assert_eq!(now.monitors()[map[1]].id, MonitorId::new("other-right"));
    }
}
