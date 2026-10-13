//! Editing workspaces from the interface: their zones (the zone editor), names and shortcuts,
//! as checks and changes on plain data.
//!
//! The editor works on the monitors connected now: each zone says which of them it is on and
//! where, as fractions of its work area. A zone of a monitor that is not connected shows on the
//! one standing in for it (as it does on screen) and, unless the editor moves it to another
//! monitor, keeps belonging to its own monitor when saved.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::RatioRect;
use crate::layout::{Place, Workspace};
use crate::place::Resolver;
use crate::topology::{MonitorId, Topology, fnv1a64, map_monitors};
use crate::workspace::next_id;
use crate::zone::{FrameStyle, Zone, ZoneColor, ZoneId};

/// The longest workspace or zone name, in characters.
pub const MAX_NAME: usize = 60;
/// The most zones on one monitor: more would be too small to drop a window into.
pub const MAX_ZONES_PER_MONITOR: usize = 16;
/// The smallest a zone may be, as a fraction of its monitor's work area on each side.
pub const MIN_ZONE: f64 = 0.05;

/// A zone as the editor shows and returns it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ZoneEdit {
    /// The zone's id; `None` for a zone made in the editor.
    pub id: Option<ZoneId>,
    pub name: String,
    pub color: ZoneColor,
    /// The monitor it is on, one of those connected now.
    pub monitor: MonitorId,
    /// Where on that monitor's work area.
    pub area: RatioRect,
}

/// Why an edit cannot be saved; each says what to change.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("Give it a name.")]
    EmptyName,
    #[error("Names are {MAX_NAME} characters at most.")]
    LongName,
    #[error("Every zone needs a name.")]
    UnnamedZone,
    #[error("Zone names are {MAX_NAME} characters at most: \u{201c}{0}\u{2026}\u{201d} is longer.")]
    LongZoneName(String),
    #[error("Another workspace is called \u{201c}{0}\u{201d}.")]
    NameTaken(String),
    #[error("A zone is on a monitor that is not connected any more. Open the editor again.")]
    UnknownMonitor,
    #[error("A zone this workspace does not have was edited. Open the editor again.")]
    UnknownZone,
    #[error("The same zone was sent twice. Open the editor again.")]
    DuplicateZone,
    #[error("Zone \u{201c}{0}\u{201d} is too small: zones are at least a twentieth of the screen each way.")]
    TooSmall(String),
    #[error("A monitor has more than {MAX_ZONES_PER_MONITOR} zones.")]
    TooManyZones,
}

/// Checks a workspace or zone name, returning it trimmed.
pub fn checked_name(name: &str) -> Result<String, EditError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EditError::EmptyName);
    }
    if name.chars().count() > MAX_NAME {
        return Err(EditError::LongName);
    }
    Ok(name.to_owned())
}

/// Checks a zone's name; the problem says which zone.
fn zone_name(name: &str) -> Result<String, EditError> {
    checked_name(name).map_err(|e| match e {
        EditError::EmptyName => EditError::UnnamedZone,
        _ => EditError::LongZoneName(name.trim().chars().take(20).collect()),
    })
}

/// A workspace's zones on the monitors connected now.
pub fn zones_for_editor(ws: &Workspace, topology: &Topology) -> Vec<ZoneEdit> {
    let mapping = map_monitors(&ws.layout.monitors, topology);
    ws.layout
        .zones
        .iter()
        .filter_map(|z| {
            let monitor = topology.monitors().get(*mapping.get(z.monitor)?)?;
            Some(ZoneEdit {
                id: Some(z.id.clone()),
                name: z.name.clone(),
                color: z.color,
                monitor: monitor.id.clone(),
                area: z.area,
            })
        })
        .collect()
}

/// A short token for the zones an editor shows: the editor sends it back with its changes,
/// which are refused when it no longer matches (the monitors changed, or another window saved
/// other zones meanwhile) rather than overwriting what changed. Windows moving between zones
/// do not change it.
pub fn revision(ws: &Workspace, topology: &Topology) -> String {
    let monitors: Vec<&str> = ws.layout.monitors.iter().map(|m| m.id.0.as_str()).collect();
    // Serialising plain data cannot fail; an empty string would only make the token coarser.
    let zones = serde_json::to_string(&(&ws.layout.zones, monitors)).unwrap_or_default();
    format!(
        "{:016x}",
        fnv1a64(format!("{}|{zones}", topology.fingerprint()).as_bytes())
    )
}

/// The workspace with the editor's zones. A window or tab group in a zone that was removed
/// keeps the place it had on screen, as a place of its own.
pub fn apply_zones(
    ws: &Workspace,
    edits: &[ZoneEdit],
    topology: &Topology,
    style: FrameStyle,
) -> Result<Workspace, EditError> {
    let shown_on: HashMap<ZoneId, MonitorId> = zones_for_editor(ws, topology)
        .into_iter()
        .filter_map(|z| Some((z.id?, z.monitor)))
        .collect();
    let mut out = ws.clone();
    let layout = &mut out.layout;
    let mut taken: HashSet<String> = layout.zones.iter().map(|z| z.id.0.clone()).collect();
    let mut sent: HashSet<&ZoneId> = HashSet::new();
    // Zones the editor did not show stay as they are.
    let mut zones: Vec<Zone> = layout
        .zones
        .iter()
        .filter(|z| !shown_on.contains_key(&z.id))
        .cloned()
        .collect();
    for e in edits {
        let monitor = topology.get(&e.monitor).ok_or(EditError::UnknownMonitor)?;
        let name = zone_name(&e.name)?;
        if e.area.width() < MIN_ZONE - 1e-9 || e.area.height() < MIN_ZONE - 1e-9 {
            return Err(EditError::TooSmall(name));
        }
        let (id, kept_monitor) = match &e.id {
            Some(id) => {
                let shown = shown_on.get(id).ok_or(EditError::UnknownZone)?;
                if !sent.insert(id) {
                    return Err(EditError::DuplicateZone);
                }
                // A zone still on the monitor it showed on keeps its own monitor (which may be
                // one not connected now); one moved to another monitor belongs to that one.
                let own = layout.zones.iter().find(|z| &z.id == id).map(|z| z.monitor);
                (id.clone(), own.filter(|_| shown == &e.monitor))
            }
            None => {
                let id = ZoneId(next_id("z", &taken));
                taken.insert(id.0.clone());
                (id, None)
            }
        };
        let monitor = kept_monitor.unwrap_or_else(|| layout.monitor_index(monitor, topology));
        zones.push(Zone {
            id,
            name,
            color: e.color,
            monitor,
            area: e.area,
        });
    }
    // The limit is per monitor the zones belong to: zones of an unplugged monitor shown on
    // another one do not count against it.
    let mut per_monitor: HashMap<usize, usize> = HashMap::new();
    for z in &zones {
        *per_monitor.entry(z.monitor).or_default() += 1;
    }
    if per_monitor.values().any(|n| *n > MAX_ZONES_PER_MONITOR) {
        return Err(EditError::TooManyZones);
    }

    let kept: HashSet<ZoneId> = zones.iter().map(|z| z.id.clone()).collect();
    free_places(&mut out.layout, ws, |z| !kept.contains(z), topology, style);
    out.layout.zones = zones;
    Ok(out)
}

/// The workspace without the zones `gone` (a layout picked on the home page makes way for its
/// own on a monitor). Their windows and tab groups keep the places they had on screen.
pub fn without_zones(ws: &Workspace, gone: &[ZoneId], topology: &Topology, style: FrameStyle) -> Workspace {
    let mut out = ws.clone();
    free_places(&mut out.layout, ws, |z| gone.contains(z), topology, style);
    out.layout.zones.retain(|z| !gone.contains(&z.id));
    out
}

/// Turns the places in zones that are going (`gone`) into places of their own, where those
/// windows were on screen in `ws`: inside the zone's gaps, and for tabs below the band their
/// strip needs.
fn free_places(
    layout: &mut crate::layout::Layout,
    ws: &Workspace,
    gone: impl Fn(&ZoneId) -> bool,
    topology: &Topology,
    style: FrameStyle,
) {
    let resolver = Resolver::new(&ws.layout, topology, style);
    let place_without = |place: &Place, style: FrameStyle| -> Option<Place> {
        let Place::Zone { zone } = place else {
            return None;
        };
        if !gone(zone) {
            return None;
        }
        let z = ws.layout.zone(zone)?;
        let area = resolver
            .zone_rect(zone)
            .and_then(|(rect, dpi)| {
                let work_area = resolver.monitor(z.monitor)?.work_area;
                RatioRect::from_rect(style.window_area(rect, dpi), work_area).ok()
            })
            .unwrap_or(z.area);
        Some(Place::Free {
            monitor: z.monitor,
            area,
        })
    };
    for s in &mut layout.slots {
        if let Some(p) = place_without(&s.place, style) {
            s.place = p;
        }
    }
    for g in &mut layout.tab_groups {
        if let Some(p) = place_without(&g.place, style.for_tabs()) {
            g.place = p;
        }
    }
}

/// The workspace renamed; the name must be free among `others` (same rule as the switcher:
/// case and surrounding spaces do not count).
pub fn rename(ws: &Workspace, name: &str, others: &[Workspace]) -> Result<Workspace, EditError> {
    let name = checked_name(name)?;
    if let Some(other) = others
        .iter()
        .find(|o| o.id != ws.id && crate::workspace::same_name(&o.name, &name))
    {
        return Err(EditError::NameTaken(other.name.clone()));
    }
    let mut out = ws.clone();
    out.name = name;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::layout::{Layout, Slot, SlotId, TabGroup, TabGroupId, WorkspaceId};
    use crate::matching::AppMatch;
    use crate::topology::Monitor;

    fn ratio(l: f64, t: f64, r: f64, b: f64) -> RatioRect {
        RatioRect::new(l, t, r, b).unwrap()
    }

    /// Two zones on the left monitor of a two-monitor layout, one on the other; a window in
    /// each, and a tab group in the right half.
    fn workspace(t: &Topology) -> Workspace {
        let monitors: Vec<_> = t.monitors().iter().map(Monitor::to_ref).collect();
        let zone = |id: &str, monitor: usize, area: RatioRect| Zone {
            id: ZoneId::new(id),
            name: id.to_uppercase(),
            color: ZoneColor::Lapis,
            monitor,
            area,
        };
        let slot = |id: &str, place: Place| Slot {
            id: SlotId(id.into()),
            app: AppMatch::exe("a.exe"),
            launch: None,
            place,
        };
        Workspace {
            id: WorkspaceId("w".into()),
            name: "W".into(),
            hotkey: None,
            layout: Layout {
                monitors,
                zones: vec![
                    zone("z1", 0, ratio(0.0, 0.0, 0.5, 1.0)),
                    zone("z2", 0, ratio(0.5, 0.0, 1.0, 1.0)),
                    zone("z3", 1, RatioRect::FULL),
                ],
                slots: vec![
                    slot(
                        "s1",
                        Place::Zone {
                            zone: ZoneId::new("z1"),
                        },
                    ),
                    slot(
                        "s2",
                        Place::Tab {
                            group: TabGroupId("g1".into()),
                        },
                    ),
                    slot(
                        "s3",
                        Place::Tab {
                            group: TabGroupId("g1".into()),
                        },
                    ),
                ],
                tab_groups: vec![TabGroup {
                    id: TabGroupId("g1".into()),
                    tabs: vec![SlotId("s2".into()), SlotId("s3".into())],
                    active: SlotId("s2".into()),
                    place: Place::Zone {
                        zone: ZoneId::new("z2"),
                    },
                }],
            },
        }
    }

    #[test]
    fn editing_round_trips_and_removed_zones_leave_windows_in_place() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let mut zones = zones_for_editor(&ws, &t);
        assert_eq!(zones.len(), 3);
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()).unwrap(),
            ws,
            "saving unchanged changes nothing"
        );
        // z2 goes; z1 takes the whole monitor; a new zone appears on the other monitor.
        zones.retain(|z| z.id != Some(ZoneId::new("z2")));
        zones[0].area = RatioRect::FULL;
        zones.push(ZoneEdit {
            id: None,
            name: "  Notes ".into(),
            color: ZoneColor::Saffron,
            monitor: zones[1].monitor.clone(),
            area: ratio(0.0, 0.0, 0.3, 1.0),
        });
        let out = apply_zones(&ws, &zones, &t, FrameStyle::default()).unwrap();
        assert_eq!(out.layout.problems(), vec![]);
        assert_eq!(out.layout.zones.len(), 3);
        let new = out.layout.zones.iter().find(|z| z.name == "Notes").unwrap();
        assert_eq!(new.id, ZoneId::new("z4"), "a new id, not one just removed");
        // The tab group that sat in z2 keeps its place there.
        assert!(matches!(
            out.layout.tab_groups[0].place,
            Place::Free { monitor: 0, .. }
        ));
        assert_eq!(
            out.layout.slots[0].place,
            Place::Zone {
                zone: ZoneId::new("z1")
            }
        );
    }

    #[test]
    fn editing_with_a_monitor_unplugged_keeps_its_zones_on_it() {
        let two = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&two);
        let one = fixtures::single_1080p_100();
        let mut shown = zones_for_editor(&ws, &one);
        assert_eq!(
            shown.len(),
            3,
            "every zone shows, on the monitor standing in for its own"
        );
        shown[0].name = "Code".into();
        let out = apply_zones(&ws, &shown, &one, FrameStyle::default()).unwrap();
        assert_eq!(out.layout.problems(), vec![]);
        let z3 = out
            .layout
            .zones
            .iter()
            .find(|z| z.id == ZoneId::new("z3"))
            .unwrap();
        assert_eq!(z3.monitor, 1, "still on the monitor that is unplugged");
        assert_eq!(
            out.layout
                .zones
                .iter()
                .find(|z| z.id == ZoneId::new("z1"))
                .unwrap()
                .name,
            "Code"
        );
    }

    #[test]
    fn bad_edits_are_refused_with_a_reason() {
        let t = fixtures::single_1080p_100();
        let ws = workspace(&fixtures::uhd_200_plus_1080p_100());
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].name = "   ".into();
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::UnnamedZone)
        );
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].area = ratio(0.0, 0.0, 0.01, 1.0);
        assert!(matches!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::TooSmall(_))
        ));
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].id = Some(ZoneId::new("nope"));
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::UnknownZone)
        );
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].monitor = MonitorId::new("gone");
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::UnknownMonitor)
        );
        let many: Vec<ZoneEdit> = (0..17)
            .map(|i| ZoneEdit {
                id: None,
                name: format!("Z{i}"),
                color: ZoneColor::Slate,
                monitor: t.monitors()[0].id.clone(),
                area: RatioRect::FULL,
            })
            .collect();
        assert_eq!(
            apply_zones(&ws, &many, &t, FrameStyle::default()),
            Err(EditError::TooManyZones)
        );
    }

    /// Two zones side by side on the one monitor of `t`, a window in each.
    fn halves(t: &Topology) -> Workspace {
        let mut ws = workspace(t);
        ws.layout.monitors.truncate(1);
        ws.layout.zones.truncate(2);
        ws.layout.slots.truncate(1);
        ws.layout.tab_groups.clear();
        ws
    }

    #[test]
    fn a_zone_added_on_a_stand_in_monitor_leaves_the_others_on_it() {
        // Saved on the 1080p monitor; now the 4K primary stands in for it, with another monitor
        // beside it.
        let ws = halves(&fixtures::single_1080p_100());
        let now = fixtures::uhd_200_plus_1080p_100();
        let stand_in = now.primary().id.clone();
        let mut zones = zones_for_editor(&ws, &now);
        assert!(zones.iter().all(|z| z.monitor == stand_in));
        zones.push(ZoneEdit {
            id: None,
            name: "Notes".into(),
            color: ZoneColor::Saffron,
            monitor: stand_in.clone(),
            area: ratio(0.0, 0.0, 0.25, 1.0),
        });
        let out = apply_zones(&ws, &zones, &now, FrameStyle::default()).unwrap();
        assert_eq!(out.layout.problems(), vec![]);
        assert!(
            zones_for_editor(&out, &now).iter().all(|z| z.monitor == stand_in),
            "every zone still shows where it was edited"
        );
        assert_eq!(out.layout.monitors.len(), 1, "no second monitor for the stand-in");
    }

    #[test]
    fn windows_of_a_removed_zone_keep_the_place_they_had_on_screen() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let style = FrameStyle::default();
        let resolver = crate::place::Resolver::new(&ws.layout, &t, style);
        let before_window = resolver.target(&ws.layout.slots[0].place);
        let before_tabs = resolver.target(&ws.layout.slots[1].place);
        let zones: Vec<ZoneEdit> = zones_for_editor(&ws, &t)
            .into_iter()
            .filter(|z| z.monitor != t.primary().id)
            .collect();
        let out = apply_zones(&ws, &zones, &t, style).unwrap();
        assert!(matches!(out.layout.slots[0].place, Place::Free { .. }));
        assert!(matches!(out.layout.tab_groups[0].place, Place::Free { .. }));
        let after = crate::place::Resolver::new(&out.layout, &t, style);
        assert_eq!(
            after.target(&out.layout.slots[0].place),
            before_window,
            "same window place"
        );
        assert_eq!(
            after.target(&out.layout.slots[1].place),
            before_tabs,
            "tabs keep room for their strip above them"
        );
    }

    #[test]
    fn zones_taken_away_leave_their_windows_where_they_were() {
        // A layout picked on the home page replaces the main monitor's zones: the windows and
        // tab groups that were in them keep their places on screen until Arrange moves them,
        // and the other monitor's zones stay.
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let style = FrameStyle::default();
        let primary_index = ws
            .layout
            .monitors
            .iter()
            .position(|m| t.get(&m.id).is_some_and(|c| c.id == t.primary().id))
            .unwrap();
        let gone: Vec<ZoneId> = ws
            .layout
            .zones
            .iter()
            .filter(|z| z.monitor == primary_index)
            .map(|z| z.id.clone())
            .collect();
        assert!(!gone.is_empty());
        let before = crate::place::Resolver::new(&ws.layout, &t, style);
        let window_was = before.target(&ws.layout.slots[0].place);
        let out = without_zones(&ws, &gone, &t, style);
        assert!(out.layout.zones.iter().all(|z| !gone.contains(&z.id)));
        assert_eq!(
            out.layout.zones.len(),
            ws.layout.zones.len() - gone.len(),
            "the other monitor's zones stay"
        );
        assert!(matches!(out.layout.slots[0].place, Place::Free { .. }));
        let after = crate::place::Resolver::new(&out.layout, &t, style);
        assert_eq!(after.target(&out.layout.slots[0].place), window_was);
        // Taking nothing away changes nothing.
        assert_eq!(without_zones(&ws, &[], &t, style), ws);
    }

    #[test]
    fn zones_of_two_monitors_shown_on_one_count_per_monitor() {
        // Ten zones on each of two monitors; with one unplugged, all twenty show on the other,
        // and saving them unchanged is fine.
        let two = fixtures::uhd_200_plus_1080p_100();
        let mut ws = workspace(&two);
        ws.layout.zones = (0..20u32)
            .map(|i| Zone {
                id: ZoneId::new(format!("z{}", i + 1)),
                name: format!("Z{i}"),
                color: ZoneColor::Slate,
                monitor: usize::from(i >= 10),
                area: ratio(f64::from(i % 10) / 10.0, 0.0, f64::from(i % 10 + 1) / 10.0, 1.0),
            })
            .collect();
        ws.layout.slots.truncate(1);
        ws.layout.slots[0].place = Place::Zone {
            zone: ZoneId::new("z1"),
        };
        ws.layout.tab_groups.clear();
        let one = fixtures::single_1080p_100();
        let shown = zones_for_editor(&ws, &one);
        assert_eq!(shown.len(), 20);
        assert_eq!(apply_zones(&ws, &shown, &one, FrameStyle::default()).unwrap(), ws);
    }

    #[test]
    fn a_zone_sent_twice_is_refused() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let mut zones = zones_for_editor(&ws, &t);
        zones.push(zones[0].clone());
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::DuplicateZone)
        );
    }

    #[test]
    fn zone_name_problems_say_which_zone() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let mut zones = zones_for_editor(&ws, &t);
        zones[1].name = "x".repeat(61);
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default())
                .unwrap_err()
                .to_string(),
            format!(
                "Zone names are {MAX_NAME} characters at most: \u{201c}{}\u{2026}\u{201d} is longer.",
                "x".repeat(20)
            )
        );
        zones[1].name = " ".into();
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default()),
            Err(EditError::UnnamedZone)
        );
    }

    #[test]
    fn the_revision_changes_with_the_zones_and_the_monitors() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let r = revision(&ws, &t);
        assert_eq!(revision(&ws.clone(), &t), r);
        let mut renamed = ws.clone();
        renamed.layout.zones[0].name = "Other".into();
        assert_ne!(revision(&renamed, &t), r);
        assert_ne!(revision(&ws, &fixtures::single_1080p_100()), r);
        // Windows moving between zones does not make an open editor stale.
        let mut moved = ws.clone();
        moved.layout.slots[0].place = Place::Zone {
            zone: ZoneId::new("z2"),
        };
        assert_eq!(revision(&moved, &t), r);
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(checked_name("  Ship it "), Ok("Ship it".into()));
        assert_eq!(checked_name(""), Err(EditError::EmptyName));
        assert_eq!(checked_name(&"é".repeat(61)), Err(EditError::LongName));
        let t = fixtures::single_1080p_100();
        let a = workspace(&t);
        let mut b = workspace(&t);
        b.id = WorkspaceId("b".into());
        b.name = "Été".into();
        assert_eq!(
            rename(&a, " ÉTÉ ", &[a.clone(), b.clone()]),
            Err(EditError::NameTaken("Été".into()))
        );
        assert_eq!(rename(&a, "Spring", &[a.clone(), b]).unwrap().name, "Spring");
        assert_eq!(
            rename(&a, "w", std::slice::from_ref(&a)).unwrap().name,
            "w",
            "its own name is free"
        );
    }
}
