//! Editing workspaces from the interface: their zones (the zone editor), names and shortcuts,
//! as checks and changes on plain data.
//!
//! The editor works on the monitors connected now: each zone says which of them it is on and
//! where, as fractions of its work area. A zone of a monitor that is not connected shows on the
//! one standing in for it (as it does on screen) and, unless the editor moves it to another
//! monitor, keeps belonging to its own monitor when saved.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::RatioRect;
use crate::layout::{Place, Workspace};
use crate::topology::{MonitorId, Topology, map_monitors};
use crate::zone::{Zone, ZoneColor, ZoneId};

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
    #[error("Another workspace is called \u{201c}{0}\u{201d}.")]
    NameTaken(String),
    #[error("A zone is on a monitor that is not connected any more. Open the editor again.")]
    UnknownMonitor,
    #[error("A zone this workspace does not have was edited. Open the editor again.")]
    UnknownZone,
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

/// The workspace with the editor's zones. A window or tab group in a zone that was removed
/// keeps its place on screen, as a place of its own.
pub fn apply_zones(ws: &Workspace, edits: &[ZoneEdit], topology: &Topology) -> Result<Workspace, EditError> {
    let before = zones_for_editor(ws, topology);
    let shown: HashSet<ZoneId> = before.iter().filter_map(|z| z.id.clone()).collect();
    for e in edits {
        if e.id.as_ref().is_some_and(|id| !shown.contains(id)) {
            return Err(EditError::UnknownZone);
        }
        if topology.get(&e.monitor).is_none() {
            return Err(EditError::UnknownMonitor);
        }
        let name = checked_name(&e.name)?;
        if e.area.width() < MIN_ZONE - 1e-9 || e.area.height() < MIN_ZONE - 1e-9 {
            return Err(EditError::TooSmall(name));
        }
    }
    for m in topology.monitors() {
        if edits.iter().filter(|e| e.monitor == m.id).count() > MAX_ZONES_PER_MONITOR {
            return Err(EditError::TooManyZones);
        }
    }

    let mut out = ws.clone();
    let layout = &mut out.layout;
    // Zones the editor did not show stay; the others are replaced by the edit.
    let mut zones: Vec<Zone> = layout
        .zones
        .iter()
        .filter(|z| !shown.contains(&z.id))
        .cloned()
        .collect();
    let mut taken: HashSet<String> = layout.zones.iter().map(|z| z.id.0.clone()).collect();
    for e in edits {
        let Some(monitor) = topology.get(&e.monitor) else {
            return Err(EditError::UnknownMonitor);
        };
        // A zone still on the monitor it showed on keeps its own monitor (which may be one not
        // connected now); one moved to another monitor belongs to that one.
        let original = e.id.as_ref().and_then(|id| {
            let shown_on = &before.iter().find(|b| b.id.as_ref() == Some(id))?.monitor;
            let zone = layout.zones.iter().find(|z| &z.id == id)?;
            (shown_on == &e.monitor).then_some(zone.monitor)
        });
        let index = match original {
            Some(i) => i,
            None => layout.monitor_index(monitor),
        };
        let id = match &e.id {
            Some(id) => id.clone(),
            None => {
                let id = ZoneId(
                    (1..)
                        .map(|n| format!("z{n}"))
                        .find(|c| !taken.contains(c))
                        .unwrap_or_else(|| "z0".into()),
                );
                taken.insert(id.0.clone());
                id
            }
        };
        zones.push(Zone {
            id,
            name: checked_name(&e.name)?,
            color: e.color,
            monitor: index,
            area: e.area,
        });
    }
    // A window (or tab group) whose zone is gone keeps the place the zone had.
    let kept: HashSet<&ZoneId> = zones.iter().map(|z| &z.id).collect();
    let removed: Vec<(ZoneId, Place)> = layout
        .zones
        .iter()
        .filter(|z| !kept.contains(&z.id))
        .map(|z| {
            (
                z.id.clone(),
                Place::Free {
                    monitor: z.monitor,
                    area: z.area,
                },
            )
        })
        .collect();
    let place_of = |p: &Place| -> Option<Place> {
        match p {
            Place::Zone { zone } => removed.iter().find(|(id, _)| id == zone).map(|(_, p)| p.clone()),
            _ => None,
        }
    };
    for s in &mut layout.slots {
        if let Some(p) = place_of(&s.place) {
            s.place = p;
        }
    }
    for g in &mut layout.tab_groups {
        if let Some(p) = place_of(&g.place) {
            g.place = p;
        }
    }
    layout.zones = zones;
    Ok(out)
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
            apply_zones(&ws, &zones, &t).unwrap(),
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
        let out = apply_zones(&ws, &zones, &t).unwrap();
        assert_eq!(out.layout.problems(), vec![]);
        assert_eq!(out.layout.zones.len(), 3);
        let new = out.layout.zones.iter().find(|z| z.name == "Notes").unwrap();
        assert_eq!(new.id, ZoneId::new("z4"), "a new id, not one just removed");
        // The tab group that sat in z2 keeps z2's place.
        assert_eq!(
            out.layout.tab_groups[0].place,
            Place::Free {
                monitor: 0,
                area: ratio(0.5, 0.0, 1.0, 1.0)
            }
        );
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
        let out = apply_zones(&ws, &shown, &one).unwrap();
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
        assert_eq!(apply_zones(&ws, &zones, &t), Err(EditError::EmptyName));
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].area = ratio(0.0, 0.0, 0.01, 1.0);
        assert!(matches!(
            apply_zones(&ws, &zones, &t),
            Err(EditError::TooSmall(_))
        ));
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].id = Some(ZoneId::new("nope"));
        assert_eq!(apply_zones(&ws, &zones, &t), Err(EditError::UnknownZone));
        let mut zones = zones_for_editor(&ws, &t);
        zones[0].monitor = MonitorId::new("gone");
        assert_eq!(apply_zones(&ws, &zones, &t), Err(EditError::UnknownMonitor));
        let many: Vec<ZoneEdit> = (0..17)
            .map(|i| ZoneEdit {
                id: None,
                name: format!("Z{i}"),
                color: ZoneColor::Slate,
                monitor: t.monitors()[0].id.clone(),
                area: RatioRect::FULL,
            })
            .collect();
        assert_eq!(apply_zones(&ws, &many, &t), Err(EditError::TooManyZones));
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
