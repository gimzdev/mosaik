//! Editing workspaces from the interface: their zones (the zone editor), their apps, names and
//! shortcuts, as checks and changes on plain data.
//!
//! The editor works on the monitors connected now: each zone says which of them it is on and
//! where, as fractions of its work area. A zone of a monitor that is not connected shows on the
//! one standing in for it (as it does on screen) and, unless the editor moves it to another
//! monitor, keeps belonging to its own monitor when saved.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::geometry::RatioRect;
use crate::layout::{Launch, Place, Slot, SlotId, Workspace};
use crate::matching::AppMatch;
use crate::place::Resolver;
use crate::topology::{MonitorId, Topology, fnv1a64, map_monitors};
use crate::window::WindowFacts;
use crate::workspace::{app_match, free_place, next_id};
use crate::zone::{FrameStyle, Zone, ZoneColor, ZoneId};

/// The longest workspace name, in characters.
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
    /// Its number in reading order, as shown. What the editor sends back is not kept: zones are
    /// numbered by place when saved.
    #[serde(default)]
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
    #[error("The same zone was sent twice. Open the editor again.")]
    DuplicateZone,
    #[error("Zone {0} is too small: zones are at least a twentieth of the screen each way.")]
    TooSmall(String),
    #[error("A monitor has more than {MAX_ZONES_PER_MONITOR} zones.")]
    TooManyZones,
    #[error("This app is not in the workspace any more.")]
    GoneApp,
    #[error("This zone is not in the workspace any more.")]
    GoneZone,
    #[error("This app is not in a zone: pick a zone for it instead.")]
    NotInZone,
}

/// Checks a workspace name, returning it trimmed.
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

/// The workspace with the editor's zones, numbered in reading order. A window or tab group in a
/// zone that was removed keeps the place it had on screen, as a place of its own.
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
    let mut edited: HashSet<ZoneId> = HashSet::new();
    // Zones the editor did not show stay as they are.
    let mut zones: Vec<Zone> = layout
        .zones
        .iter()
        .filter(|z| !shown_on.contains_key(&z.id))
        .cloned()
        .collect();
    for e in edits {
        let monitor = topology.get(&e.monitor).ok_or(EditError::UnknownMonitor)?;
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
        edited.insert(id.clone());
        zones.push(Zone {
            id,
            name: String::new(),
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
    out.layout.number_zones();
    // A zone made too small is named by its number, as the editor shows it.
    if let Some(small) = out.layout.zones.iter().find(|z| {
        edited.contains(&z.id) && (z.area.width() < MIN_ZONE - 1e-9 || z.area.height() < MIN_ZONE - 1e-9)
    }) {
        return Err(EditError::TooSmall(small.name.clone()));
    }
    Ok(out)
}

/// The workspace without the zones `gone` (a layout picked on the home page makes way for its
/// own on a monitor). Their windows and tab groups keep the places they had on screen.
pub fn without_zones(ws: &Workspace, gone: &[ZoneId], topology: &Topology, style: FrameStyle) -> Workspace {
    let mut out = ws.clone();
    free_places(&mut out.layout, ws, |z| gone.contains(z), topology, style);
    out.layout.zones.retain(|z| !gone.contains(&z.id));
    out.layout.number_zones();
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

/// The first zone of `ws` without a window or tab group in it: where a new app goes.
pub fn empty_zone(ws: &Workspace) -> Option<ZoneId> {
    let layout = &ws.layout;
    layout
        .zones
        .iter()
        .find(|z| {
            !layout
                .slots
                .iter()
                .any(|s| matches!(layout.effective_place(s), Some(Place::Zone { zone }) if *zone == z.id))
        })
        .map(|z| z.id.clone())
}

/// The workspace with the app of a window on screen (`facts`, called `label`) as a new app,
/// in `zone`, or where the window is now without one. Returns it with the new app's id.
pub fn add_app(
    ws: &Workspace,
    facts: &WindowFacts,
    label: &str,
    launch: Option<Launch>,
    zone: Option<&ZoneId>,
    topology: &Topology,
) -> Result<(Workspace, SlotId), EditError> {
    let mut out = ws.clone();
    let place = match zone {
        Some(z) if out.layout.zone(z).is_none() => return Err(EditError::GoneZone),
        Some(z) => Place::Zone { zone: z.clone() },
        None => free_place(&mut out.layout.monitors, topology, facts.frame_rect),
    };
    let taken: HashSet<String> = out.layout.slots.iter().map(|s| s.id.0.clone()).collect();
    let id = SlotId(next_id("s", &taken));
    out.layout.slots.push(Slot {
        id: id.clone(),
        label: Some(label.trim().to_owned()).filter(|l| !l.is_empty()),
        app: app_match(facts),
        launch,
        place,
    });
    Ok((out, id))
}

/// The zone of `ws` with the fewest apps, the first by number on a tie: where an app with no
/// window goes when no zone is empty, behind the apps there.
fn quietest_zone(ws: &Workspace) -> Option<ZoneId> {
    let layout = &ws.layout;
    let count = |z: &ZoneId| {
        layout
            .slots
            .iter()
            .filter(|s| matches!(layout.effective_place(s), Some(Place::Zone { zone }) if zone == z))
            .count()
    };
    let number = |name: &str| name.parse::<u32>().unwrap_or(u32::MAX);
    layout
        .zones
        .iter()
        .min_by(|a, b| {
            count(&a.id)
                .cmp(&count(&b.id))
                .then(number(&a.name).cmp(&number(&b.name)))
        })
        .map(|z| z.id.clone())
}

/// The workspace with an app of the Start menu that is not open (ADR 0033: "Add an app"), called
/// `label`, its windows found by `found` and started by `launch`, as a new app: in `zone`, else
/// its first empty zone, else the zone with the fewest apps, behind them; with no zones, on the
/// whole main monitor. It starts when the workspace opens. Returns it with the new app's id.
pub fn add_installed_app(
    ws: &Workspace,
    label: &str,
    found: AppMatch,
    launch: Launch,
    zone: Option<&ZoneId>,
    topology: &Topology,
) -> Result<(Workspace, SlotId), EditError> {
    let mut out = ws.clone();
    let place = match zone
        .cloned()
        .or_else(|| empty_zone(ws))
        .or_else(|| quietest_zone(ws))
    {
        Some(z) if out.layout.zone(&z).is_none() => return Err(EditError::GoneZone),
        Some(z) => Place::Zone { zone: z },
        None => Place::Maximized {
            monitor: out.layout.monitor_index(topology.primary(), topology),
        },
    };
    let taken: HashSet<String> = out.layout.slots.iter().map(|s| s.id.0.clone()).collect();
    let id = SlotId(next_id("s", &taken));
    out.layout.slots.push(Slot {
        id: id.clone(),
        label: Some(label.trim().to_owned()).filter(|l| !l.is_empty()),
        app: found,
        launch: Some(launch),
        place,
    });
    Ok((out, id))
}

/// The workspace with its app `slot` in the zone `zone`. A tab leaves its group for it, unless
/// the group is in that zone already.
pub fn move_app(ws: &Workspace, slot: &SlotId, zone: &ZoneId) -> Result<Workspace, EditError> {
    let current = ws.layout.slot(slot).ok_or(EditError::GoneApp)?;
    if ws.layout.zone(zone).is_none() {
        return Err(EditError::GoneZone);
    }
    let place = Place::Zone { zone: zone.clone() };
    if ws.layout.effective_place(current) == Some(&place) {
        return Ok(ws.clone());
    }
    let mut out = ws.clone();
    if !out.layout.leave_tab(slot, place.clone())
        && let Some(s) = out.layout.slots.iter_mut().find(|s| &s.id == slot)
    {
        s.place = place;
    }
    Ok(out)
}

/// The workspace with its apps `a` and `b` trading places (one dropped on the other in its
/// picture). In two zones they trade zones, a tab leaving its group for the other zone (ADR
/// 0031), and their places front to back; in one zone, they trade their places in it
/// ([`trade_in_zone`], ADR 0032).
pub fn swap_apps(ws: &Workspace, a: &SlotId, b: &SlotId) -> Result<Workspace, EditError> {
    let zone_of = |slot: &SlotId| -> Result<ZoneId, EditError> {
        let s = ws.layout.slot(slot).ok_or(EditError::GoneApp)?;
        match ws.layout.effective_place(s) {
            Some(Place::Zone { zone }) => Ok(zone.clone()),
            _ => Err(EditError::NotInZone),
        }
    };
    let (za, zb) = (zone_of(a)?, zone_of(b)?);
    if za == zb {
        return Ok(trade_in_zone(ws, a, b));
    }
    let out = move_app(ws, a, &zb)?;
    let mut out = move_app(&out, b, &za)?;
    let at = |id: &SlotId, slots: &[crate::layout::Slot]| slots.iter().position(|s| &s.id == id);
    if let (Some(i), Some(j)) = (at(a, &out.layout.slots), at(b, &out.layout.slots)) {
        out.layout.slots.swap(i, j);
    }
    Ok(out)
}

/// Two apps of one zone trade their places in it, as its picture lists them (the owner: "i
/// should be able to permute chrome with claude"): their order front to back (the order of the
/// layout's slots, which opening the workspace follows), and their places in tab groups. Two
/// tabs of one group trade places in its strip, the tab showing staying the one showing; a tab
/// and a window trade the tab's place in its group, showing if it was, for the window's.
fn trade_in_zone(ws: &Workspace, a: &SlotId, b: &SlotId) -> Workspace {
    let mut out = ws.clone();
    let layout = &mut out.layout;
    let at = |id: &SlotId| layout.slots.iter().position(|s| &s.id == id);
    let (Some(ia), Some(ib)) = (at(a), at(b)) else {
        return out;
    };
    if ia == ib {
        return out;
    }
    let (pa, pb) = (layout.slots[ia].place.clone(), layout.slots[ib].place.clone());
    let one_group = matches!((&pa, &pb), (Place::Tab { group: ga }, Place::Tab { group: gb }) if ga == gb);
    layout.slots[ia].place = pb;
    layout.slots[ib].place = pa;
    layout.slots.swap(ia, ib);
    let other = |id: &SlotId| {
        if id == a {
            Some(b.clone())
        } else if id == b {
            Some(a.clone())
        } else {
            None
        }
    };
    for g in &mut layout.tab_groups {
        for tab in &mut g.tabs {
            if let Some(o) = other(tab) {
                *tab = o;
            }
        }
        // The app showing in a group it leaves gives the showing to the one taking its place.
        if !one_group && let Some(o) = other(&g.active) {
            g.active = o;
        }
    }
    out
}

/// Whether `areas`, the zones of one monitor as the lines between them were dragged on Home's
/// picture (ADR 0031), can be its zones: none under [`MIN_ZONE`] either way, and none over
/// another.
pub fn areas_fit(areas: &[RatioRect]) -> bool {
    let small = |a: &RatioRect| a.width() < MIN_ZONE - 1e-9 || a.height() < MIN_ZONE - 1e-9;
    let over = |a: &RatioRect, b: &RatioRect| {
        let w = a.right().min(b.right()) - a.left().max(b.left());
        let h = a.bottom().min(b.bottom()) - a.top().max(b.top());
        w > 1e-6 && h > 1e-6
    };
    !areas.is_empty()
        && !areas.iter().any(small)
        && !areas
            .iter()
            .enumerate()
            .any(|(i, a)| areas[i + 1..].iter().any(|b| over(a, b)))
}

/// The workspace without its app `slot` (a tab leaves its group first). Its window, when open,
/// stays where it is.
pub fn remove_app(ws: &Workspace, slot: &SlotId) -> Result<Workspace, EditError> {
    if ws.layout.slot(slot).is_none() {
        return Err(EditError::GoneApp);
    }
    let mut out = ws.clone();
    // The place it is given does not matter: it goes.
    out.layout.leave_tab(slot, Place::Maximized { monitor: 0 });
    out.layout.slots.retain(|s| &s.id != slot);
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
            name: id.trim_start_matches('z').to_owned(),
            color: ZoneColor::Lapis,
            monitor,
            area,
        };
        let slot = |id: &str, place: Place| Slot {
            id: SlotId(id.into()),
            label: None,
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
        let new = out
            .layout
            .zones
            .iter()
            .find(|z| z.color == ZoneColor::Saffron)
            .unwrap();
        assert_eq!(new.id, ZoneId::new("z4"), "a new id, not one just removed");
        // Numbered in reading order whatever the editor called it: the whole left monitor is 1;
        // on the right one, the zone kept and the new one start at its top left, kept first.
        let names: Vec<(&str, &str)> = out
            .layout
            .zones
            .iter()
            .map(|z| (z.id.0.as_str(), z.name.as_str()))
            .collect();
        assert_eq!(names, [("z1", "1"), ("z3", "2"), ("z4", "3")]);
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
        // A name sent back is not kept: zones are numbered by place.
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
            "1"
        );
    }

    #[test]
    fn bad_edits_are_refused_with_a_reason() {
        let t = fixtures::single_1080p_100();
        let ws = workspace(&fixtures::uhd_200_plus_1080p_100());
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
                name: (i + 1).to_string(),
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
                name: (i + 1).to_string(),
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
    fn zones_are_numbered_by_place_and_a_small_one_is_named_by_its_number() {
        // The owner: zone names "should be 1 2 3 4 5 6". The right half moved to the top left
        // and made a quarter: it reads first now, so it is 1 and the left half 2, whatever the
        // editor sent; made too thin, the problem names it by that number.
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let mut zones = zones_for_editor(&ws, &t);
        let names: Vec<&str> = zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(names, ["1", "2", "3"], "the editor shows the numbers");
        zones[1].area = ratio(0.0, 0.0, 0.5, 0.5);
        zones[1].name = "Browser".into();
        zones[0].area = ratio(0.0, 0.5, 0.5, 1.0);
        let out = apply_zones(&ws, &zones, &t, FrameStyle::default()).unwrap();
        let names: Vec<(&str, &str)> = out
            .layout
            .zones
            .iter()
            .map(|z| (z.id.0.as_str(), z.name.as_str()))
            .collect();
        assert_eq!(names, [("z1", "2"), ("z2", "1"), ("z3", "3")]);
        zones[1].area = ratio(0.0, 0.0, 0.01, 0.5);
        assert_eq!(
            apply_zones(&ws, &zones, &t, FrameStyle::default())
                .unwrap_err()
                .to_string(),
            "Zone 1 is too small: zones are at least a twentieth of the screen each way."
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

    fn sid(id: &str) -> SlotId {
        SlotId(id.into())
    }

    #[test]
    fn an_app_not_open_is_added_in_a_zone_behind_the_apps_there() {
        // ADR 0033 ("Add an app", apps not open too): an app of the Start menu joins the
        // workspace in the zone asked, else the first empty one, else the one with the fewest
        // apps, behind them; with no zones, on the whole main monitor. It starts when the
        // workspace opens.
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let found = AppMatch::exe("code.exe");
        let launch = Launch::Program {
            path: "shell:AppsFolder\\Microsoft.VisualStudioCode".into(),
            args: Vec::new(),
            working_dir: None,
        };
        let add = |ws: &Workspace, zone: Option<&str>| {
            add_installed_app(
                ws,
                " Visual Studio Code ",
                found.clone(),
                launch.clone(),
                zone.map(ZoneId::new).as_ref(),
                &t,
            )
        };
        let place = |ws: &Workspace, id: &SlotId| ws.layout.slot(id).unwrap().place.clone();
        let in_zone = |z: &str| Place::Zone { zone: ZoneId::new(z) };
        let (asked, id) = add(&ws, Some("z2")).unwrap();
        assert_eq!(asked.layout.problems(), vec![]);
        let s = asked.layout.slot(&id).unwrap();
        assert_eq!(s.name(), "Visual Studio Code");
        assert_eq!(s.app, found);
        assert_eq!(s.launch.as_ref(), Some(&launch));
        assert_eq!(s.place, in_zone("z2"));
        assert_eq!(
            asked.layout.slots.last().map(|s| &s.id),
            Some(&id),
            "behind its zone's apps"
        );
        // No zone asked: the empty one, then the one with the fewest apps, the first by number.
        let (first, a) = add(&ws, None).unwrap();
        assert_eq!(place(&first, &a), in_zone("z3"));
        let (second, b) = add(&first, None).unwrap();
        assert_eq!(place(&second, &b), in_zone("z1"));
        // No zones at all: the whole main monitor.
        let mut bare = ws.clone();
        bare.layout.zones.clear();
        bare.layout.slots.clear();
        bare.layout.tab_groups.clear();
        let (alone, c) = add(&bare, None).unwrap();
        assert!(matches!(place(&alone, &c), Place::Maximized { .. }));
        assert_eq!(alone.layout.problems(), vec![]);
        assert_eq!(add(&ws, Some("gone")), Err(EditError::GoneZone));
    }

    #[test]
    fn apps_are_added_moved_and_removed() {
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        // z1 holds s1, z2 the group of s2 and s3; z3 is empty.
        assert_eq!(empty_zone(&ws), Some(ZoneId::new("z3")));

        let notes = crate::window::WindowFacts {
            exe: Some("notepad.exe".into()),
            class: "Notepad".into(),
            title: "todo.txt - Notepad".into(),
            frame_rect: crate::geometry::Rect::new(100, 100, 900, 700),
            ..crate::window::WindowFacts::default()
        };
        let (added, id) = add_app(&ws, &notes, " Notepad ", None, Some(&ZoneId::new("z3")), &t).unwrap();
        assert_eq!(id, sid("s4"));
        assert_eq!(added.layout.problems(), vec![]);
        let s4 = added.layout.slot(&id).unwrap();
        assert_eq!(s4.name(), "Notepad");
        assert_eq!(
            s4.place,
            Place::Zone {
                zone: ZoneId::new("z3")
            }
        );
        assert_eq!(s4.app.exe.as_deref(), Some("notepad.exe"));
        assert_eq!(empty_zone(&added), None, "every zone has a window now");
        // Without a zone, it keeps the place it has on screen.
        let (free, _) = add_app(&ws, &notes, "Notepad", None, None, &t).unwrap();
        assert!(matches!(free.layout.slots[3].place, Place::Free { .. }));
        assert_eq!(
            add_app(&ws, &notes, "Notepad", None, Some(&ZoneId::new("gone")), &t),
            Err(EditError::GoneZone)
        );

        // A tab moved to another zone leaves its group, which ends with one tab left.
        let moved = move_app(&added, &sid("s3"), &ZoneId::new("z1")).unwrap();
        assert_eq!(moved.layout.problems(), vec![]);
        assert!(moved.layout.tab_groups.is_empty());
        assert_eq!(
            moved.layout.slot(&sid("s3")).unwrap().place,
            Place::Zone {
                zone: ZoneId::new("z1")
            }
        );
        assert_eq!(
            moved.layout.slot(&sid("s2")).unwrap().place,
            Place::Zone {
                zone: ZoneId::new("z2")
            }
        );
        // A tab "moved" to its group's zone stays a tab.
        assert_eq!(move_app(&ws, &sid("s2"), &ZoneId::new("z2")).unwrap(), ws);
        assert_eq!(
            move_app(&ws, &sid("nope"), &ZoneId::new("z1")),
            Err(EditError::GoneApp)
        );
        assert_eq!(
            move_app(&ws, &sid("s1"), &ZoneId::new("gone")),
            Err(EditError::GoneZone)
        );

        // Removing a tab ends its group of two: the other tab keeps the group's zone.
        let removed = remove_app(&ws, &sid("s2")).unwrap();
        assert_eq!(removed.layout.problems(), vec![]);
        assert!(removed.layout.slot(&sid("s2")).is_none());
        assert!(removed.layout.tab_groups.is_empty());
        assert_eq!(
            removed.layout.slot(&sid("s3")).unwrap().place,
            Place::Zone {
                zone: ZoneId::new("z2")
            }
        );
        let removed = remove_app(&removed, &sid("s1")).unwrap();
        assert_eq!(removed.layout.slots.len(), 1);
        assert_eq!(empty_zone(&removed), Some(ZoneId::new("z1")));
        assert_eq!(remove_app(&ws, &sid("nope")), Err(EditError::GoneApp));
    }

    #[test]
    fn apps_without_a_label_are_named_after_their_program() {
        let slot = |exe: Option<&str>, aumid: Option<&str>| Slot {
            id: sid("s1"),
            label: None,
            app: crate::matching::AppMatch {
                exe: exe.map(str::to_owned),
                aumid: aumid.map(str::to_owned),
                ..crate::matching::AppMatch::default()
            },
            launch: None,
            place: Place::Maximized { monitor: 0 },
        };
        assert_eq!(slot(Some("windowsterminal.exe"), None).name(), "Windows Terminal");
        assert_eq!(slot(Some("obsidian.exe"), None).name(), "Obsidian");
        assert_eq!(
            slot(None, Some("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")).name(),
            "Windows Calculator"
        );
        assert_eq!(slot(None, None).name(), "App");
        // The owner's "Check" listed six "Applicationframehost": Store apps (Calculator,
        // Settings) run in a frame of the frame host, whose title is the app's name.
        let mut calculator = slot(Some("applicationframehost.exe"), None);
        calculator.app.class = Some("ApplicationFrameWindow".into());
        calculator.app.title = Some(crate::matching::TitleMatch::Like("Calculator".into()));
        assert_eq!(calculator.name(), "Calculator");
        calculator.app.title = None;
        calculator.app.aumid = Some("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
        assert_eq!(calculator.name(), "Windows Calculator");
        // And "Windows Explorer", which people know as File Explorer.
        assert_eq!(slot(Some("explorer.exe"), None).name(), "File Explorer");
        let mut labelled = slot(Some("code.exe"), None);
        labelled.label = Some("Visual Studio Code".into());
        assert_eq!(labelled.name(), "Visual Studio Code");
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

    #[test]
    fn areas_stretched_on_the_picture_fit_when_none_is_too_small_or_over_another() {
        // ADR 0031: lines between zones dragged on Home's picture; the plan takes the areas only
        // when they fit (`arrange::Stretched`).
        let wide = [ratio(0.0, 0.0, 0.6, 1.0), ratio(0.6, 0.0, 1.0, 1.0)];
        assert!(areas_fit(&wide));
        assert!(!areas_fit(&[]), "none");
        assert!(
            !areas_fit(&[ratio(0.0, 0.0, 0.97, 1.0), ratio(0.97, 0.0, 1.0, 1.0)]),
            "a zone under a twentieth of the screen"
        );
        assert!(
            !areas_fit(&[ratio(0.0, 0.0, 0.7, 1.0), ratio(0.6, 0.0, 1.0, 1.0)]),
            "two zones over each other"
        );
    }

    #[test]
    fn two_apps_trade_zones() {
        // ADR 0031: an app dropped on another in a workspace's picture: they trade zones.
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = workspace(&t);
        let sid = |s: &str| SlotId(s.into());
        let zone_of = |w: &Workspace, s: &str| {
            w.layout
                .slot(&sid(s))
                .and_then(|x| w.layout.effective_place(x))
                .cloned()
        };
        let z = |id: &str| {
            Some(Place::Zone {
                zone: ZoneId::new(id),
            })
        };
        // s1 alone in z1; s2 and s3 tabs of a group in z2: s1 and s2 trade, s2 leaving its group.
        let out = swap_apps(&ws, &sid("s1"), &sid("s2")).unwrap();
        assert_eq!(zone_of(&out, "s1"), z("z2"));
        assert_eq!(zone_of(&out, "s2"), z("z1"));
        assert_eq!(zone_of(&out, "s3"), z("z2"), "the other tab stays");
        assert_eq!(out.layout.problems(), vec![]);
        // And their places front to back (ADR 0032): each takes the other's.
        let order: Vec<&str> = out.layout.slots.iter().map(|s| s.id.0.as_str()).collect();
        assert_eq!(order, ["s2", "s1", "s3"]);
        assert_eq!(swap_apps(&ws, &sid("s1"), &sid("gone")), Err(EditError::GoneApp));
        // An app with a place of its own, not a zone: refused.
        let mut free = ws.clone();
        free.layout.slots[0].place = Place::Maximized { monitor: 0 };
        assert_eq!(
            swap_apps(&free, &sid("s1"), &sid("s2")),
            Err(EditError::NotInZone)
        );
    }

    #[test]
    fn two_apps_of_one_zone_trade_places_in_it() {
        // ADR 0032, the owner: "i should be able to permute chrome with claude". In one zone, two
        // apps trade their places in it, as the picture shows them: front to back, and in a tab
        // group's strip.
        let t = fixtures::uhd_200_plus_1080p_100();
        let sid = |s: &str| SlotId(s.into());
        let in_z2 = || Place::Zone {
            zone: ZoneId::new("z2"),
        };
        let g1 = || Place::Tab {
            group: TabGroupId("g1".into()),
        };
        // z2: the tab group g1 (s2 showing, s3), then s4 and s5, windows of their own.
        let mut ws = workspace(&t);
        for id in ["s4", "s5"] {
            ws.layout.slots.push(Slot {
                id: sid(id),
                label: None,
                app: AppMatch::exe("a.exe"),
                launch: None,
                place: in_z2(),
            });
        }
        let order = |w: &Workspace| {
            let ids: Vec<&str> = w.layout.slots.iter().map(|s| s.id.0.as_str()).collect();
            ids.join(" ")
        };
        let strip = |w: &Workspace, g: &str| {
            let g = w.layout.tab_group(&TabGroupId(g.into())).expect("the group");
            let tabs: Vec<&str> = g.tabs.iter().map(|t| t.0.as_str()).collect();
            (tabs.join(" "), g.active.0.clone())
        };
        let place = |w: &Workspace, s: &str| w.layout.slot(&sid(s)).map(|x| x.place.clone());

        // Two tabs of one group trade places in its strip; the tab showing stays the one showing.
        let out = swap_apps(&ws, &sid("s3"), &sid("s2")).unwrap();
        assert_eq!(strip(&out, "g1"), ("s3 s2".into(), "s2".into()));
        assert_eq!(out.layout.problems(), vec![]);

        // Two windows of their own trade places front to back.
        let out = swap_apps(&ws, &sid("s5"), &sid("s4")).unwrap();
        assert_eq!(order(&out), "s1 s2 s3 s5 s4");
        assert_eq!(
            (place(&out, "s4"), place(&out, "s5")),
            (Some(in_z2()), Some(in_z2()))
        );
        assert_eq!(strip(&out, "g1"), strip(&ws, "g1"), "the group is left as it was");

        // A tab and a window: the window takes the tab's place in the group (showing, as the tab
        // was), the tab the window's place in the zone, out of the group.
        let out = swap_apps(&ws, &sid("s2"), &sid("s4")).unwrap();
        assert_eq!(strip(&out, "g1"), ("s4 s3".into(), "s4".into()));
        assert_eq!(
            (place(&out, "s2"), place(&out, "s4")),
            (Some(in_z2()), Some(g1()))
        );
        assert_eq!(order(&out), "s1 s4 s3 s2 s5");
        assert_eq!(out.layout.problems(), vec![]);
        // Traded back, the workspace is as it was.
        assert_eq!(swap_apps(&out, &sid("s4"), &sid("s2")).unwrap(), ws);

        // Two tabs of two groups in one zone trade groups; each group keeps showing its place.
        let mut two = ws.clone();
        two.layout.tab_groups.push(TabGroup {
            id: TabGroupId("g2".into()),
            tabs: vec![sid("s4"), sid("s5")],
            active: sid("s5"),
            place: in_z2(),
        });
        for id in ["s4", "s5"] {
            two.layout
                .slots
                .iter_mut()
                .find(|s| s.id == sid(id))
                .unwrap()
                .place = Place::Tab {
                group: TabGroupId("g2".into()),
            };
        }
        assert_eq!(two.layout.problems(), vec![]);
        let out = swap_apps(&two, &sid("s3"), &sid("s5")).unwrap();
        assert_eq!(strip(&out, "g1"), ("s2 s5".into(), "s2".into()));
        assert_eq!(strip(&out, "g2"), ("s4 s3".into(), "s3".into()));
        assert_eq!(out.layout.problems(), vec![]);

        // An app dropped on itself: nothing changes.
        assert_eq!(swap_apps(&ws, &sid("s4"), &sid("s4")).unwrap(), ws);
    }
}
