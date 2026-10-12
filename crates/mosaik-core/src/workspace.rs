//! Making, remembering and opening workspaces, as decisions on plain data. The engine gathers
//! the windows on screen, calls these functions and applies what they return.
//!
//! - [`capture`] turns the windows on screen into a workspace. Windows that tile (side by side,
//!   not overlapping) become zones; windows stacked exactly on top of each other share a zone;
//!   maximised and overlapping windows keep their own place. When an existing workspace is
//!   saved again, windows still sitting in one of its zones keep that zone.
//! - [`remember`] updates a workspace from where its windows are now, when the user leaves it.
//! - [`plan_switch`] decides which open window fills each slot, where it goes, what to start
//!   and what to minimise when a workspace opens.

use std::collections::{HashMap, HashSet};

use crate::geometry::{Insets, RatioRect, Rect};
use crate::layout::{Launch, Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use crate::matching::{AppMatch, TitleMatch, assign};
use crate::place::{Resolver, Target};
use crate::settings::SwitchSettings;
use crate::topology::{Monitor, MonitorRef, Topology, map_monitors};
use crate::window::{WindowFacts, WindowHandle};
use crate::zone::{FrameStyle, Zone, ZoneColor, ZoneId};

/// A window on screen, as capture sees it.
#[derive(Clone, Debug)]
pub struct Seen<'a> {
    pub facts: &'a WindowFacts,
    /// What to call the app, e.g. "Visual Studio Code": the name of a new zone it fills.
    pub label: String,
    /// How to start the app again when it is not open.
    pub launch: Option<Launch>,
}

/// Two frames are "the same place" when every edge is within this many pixels (at 100%), or 1%
/// of the work area, whichever is larger.
const SAME_PLACE: i32 = 12;
/// Windows "tile" when the area they share is under this fraction of the smaller one.
const TILE_OVERLAP: f64 = 0.02;
/// A window sits in a zone when its frame is within this many pixels (at 100%) of the zone's
/// window area on every edge.
const IN_ZONE: i32 = 8;

fn tolerance(work: Rect, dpi: u32) -> (i32, i32) {
    let px = crate::geometry::scale_for_dpi(SAME_PLACE, dpi);
    (px.max(work.width() / 100), px.max(work.height() / 100))
}

fn same_place(a: Rect, b: Rect, tol: (i32, i32)) -> bool {
    (a.left - b.left).abs() <= tol.0
        && (a.right - b.right).abs() <= tol.0
        && (a.top - b.top).abs() <= tol.1
        && (a.bottom - b.bottom).abs() <= tol.1
}

fn overlap_fraction(a: Rect, b: Rect) -> f64 {
    let shared = a.overlap_area(b);
    let smaller = a.area().min(b.area());
    if smaller <= 0 {
        return 0.0;
    }
    // Areas of real screens are far below 2^52, so the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    let f = shared as f64 / smaller as f64;
    f
}

/// Largest distance between matching edges.
pub fn edge_distance(a: Rect, b: Rect) -> i32 {
    [
        a.left - b.left,
        a.top - b.top,
        a.right - b.right,
        a.bottom - b.bottom,
    ]
    .into_iter()
    .map(i32::abs)
    .max()
    .unwrap_or(0)
}

/// The zone of an existing layout whose window area this frame fills, if any.
fn zone_holding(frame: Rect, zones: &[(ZoneId, Rect, u32)], style: FrameStyle) -> Option<ZoneId> {
    zones
        .iter()
        .map(|(id, rect, dpi)| (id, edge_distance(style.window_area(*rect, *dpi), frame), *dpi))
        .filter(|(_, d, dpi)| *d <= crate::geometry::scale_for_dpi(IN_ZONE, *dpi))
        .min_by_key(|(_, d, _)| *d)
        .map(|(id, _, _)| id.clone())
}

/// Every zone of a layout as it lands on the monitors connected now.
fn live_zones(layout: &Layout, topology: &Topology, style: FrameStyle) -> Vec<(ZoneId, Rect, u32)> {
    let resolver = Resolver::new(layout, topology, style);
    layout
        .zones
        .iter()
        .filter_map(|z| resolver.zone_rect(&z.id).map(|(r, dpi)| (z.id.clone(), r, dpi)))
        .collect()
}

/// The index of `monitor` in `refs`, adding it at the end when it is new. Refs of monitors that
/// are connected are refreshed, so a layout remembers the latest arrangement.
fn monitor_index(refs: &mut Vec<MonitorRef>, monitor: &Monitor) -> usize {
    if let Some(i) = refs.iter().position(|r| r.id == monitor.id) {
        refs[i] = monitor.to_ref();
        return i;
    }
    refs.push(monitor.to_ref());
    refs.len() - 1
}

fn next_id(prefix: &str, taken: &HashSet<String>) -> String {
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id))
        .unwrap_or_else(|| format!("{prefix}0"))
}

/// What to remember about a window to find it again: program, packaged app, class and its
/// title at the time (which only breaks ties).
pub fn app_match(w: &WindowFacts) -> AppMatch {
    let mut m = AppMatch::of(w);
    if !w.title.trim().is_empty() {
        m.title = Some(TitleMatch::Like(w.title.clone()));
    }
    m
}

/// A free place for `frame` on its monitor.
fn free_place(refs: &mut Vec<MonitorRef>, topology: &Topology, frame: Rect) -> Place {
    let monitor = topology.monitor_for_rect(frame);
    let index = monitor_index(refs, monitor);
    match RatioRect::from_rect(frame, monitor.work_area) {
        Ok(area) => Place::Free { monitor: index, area },
        // Entirely off its monitor's work area: remember it as filling it.
        Err(_) => Place::Free {
            monitor: index,
            area: RatioRect::FULL,
        },
    }
}

/// Makes a workspace from the windows on screen, front to back. With `previous` (the same
/// workspace saved earlier), its id, hotkey and zones are kept: windows sitting in one of its
/// zones stay there, the others are captured afresh.
pub fn capture(
    id: WorkspaceId,
    name: String,
    windows: &[Seen<'_>],
    topology: &Topology,
    style: FrameStyle,
    previous: Option<&Workspace>,
) -> Workspace {
    let mut layout = Layout {
        monitors: Vec::new(),
        ..Layout::default()
    };
    // The monitors connected now first, in order, so indices are easy to read in the file.
    for m in topology.monitors() {
        monitor_index(&mut layout.monitors, m);
    }

    // Zones kept from the previous save, re-indexed onto this layout's monitors.
    let kept_live = previous.map_or_else(Vec::new, |p| live_zones(&p.layout, topology, style));
    let mut kept: Vec<Zone> = Vec::new();
    if let Some(p) = previous {
        let mapping = map_monitors(&p.layout.monitors, topology);
        for z in &p.layout.zones {
            let Some(now) = mapping.get(z.monitor).and_then(|i| topology.monitors().get(*i)) else {
                continue;
            };
            let mut z = z.clone();
            z.monitor = monitor_index(&mut layout.monitors, now);
            kept.push(z);
        }
    }

    // Which windows take which place.
    #[derive(Clone)]
    enum Pick {
        Kept(ZoneId),
        Group(usize),
        Maximized(usize),
        Free,
    }
    let mut picks: Vec<Pick> = Vec::with_capacity(windows.len());
    // Groups of windows in the same place: (monitor in topology, rect, members).
    let mut groups: Vec<(usize, Rect, Vec<usize>)> = Vec::new();
    for (i, seen) in windows.iter().enumerate() {
        let w = seen.facts;
        let monitor = topology.monitor_for_rect(w.frame_rect);
        let m_index = topology
            .monitors()
            .iter()
            .position(|m| m.id == monitor.id)
            .unwrap_or(0);
        if w.maximized {
            picks.push(Pick::Maximized(m_index));
            continue;
        }
        if let Some(z) = zone_holding(w.frame_rect, &kept_live, style) {
            picks.push(Pick::Kept(z));
            continue;
        }
        let Some(rect) = w.frame_rect.intersection(monitor.work_area) else {
            picks.push(Pick::Free);
            continue;
        };
        let tol = tolerance(monitor.work_area, monitor.dpi);
        match groups
            .iter()
            .position(|(m, r, _)| *m == m_index && same_place(*r, rect, tol))
        {
            Some(g) => {
                groups[g].2.push(i);
                picks.push(Pick::Group(g));
            }
            None => {
                groups.push((m_index, rect, vec![i]));
                picks.push(Pick::Group(groups.len() - 1));
            }
        }
    }

    // A group becomes a zone unless it overlaps a group behind it (or a kept zone): a window
    // floating over others keeps its own place, and what it floats over still gets a zone.
    // Groups are ordered by their front-most window.
    let kept_rects: Vec<Rect> = kept_live.iter().map(|(_, r, _)| *r).collect();
    let tiles: Vec<bool> = groups
        .iter()
        .map(|(m, r, members)| {
            let front = members.first().copied().unwrap_or(usize::MAX);
            let over_one_behind = groups.iter().any(|(om, or, others)| {
                let other_front = others.first().copied().unwrap_or(usize::MAX);
                om == m && other_front > front && overlap_fraction(*r, *or) >= TILE_OVERLAP
            });
            let over_kept = kept_rects
                .iter()
                .any(|k| overlap_fraction(*r, *k) >= TILE_OVERLAP);
            !over_one_behind && !over_kept
        })
        .collect();

    // New zones, named after the front window of each group, coloured in reading order.
    let mut taken_zone_ids: HashSet<String> = kept.iter().map(|z| z.id.0.clone()).collect();
    let mut names: HashSet<String> = kept.iter().map(|z| z.name.clone()).collect();
    let mut order: Vec<usize> = (0..groups.len()).filter(|g| tiles[*g]).collect();
    order.sort_by_key(|g| {
        let (m, r, _) = &groups[*g];
        (*m, r.left, r.top)
    });
    let mut group_zone: HashMap<usize, ZoneId> = HashMap::new();
    for (n, g) in order.into_iter().enumerate() {
        let (m, rect, members) = &groups[g];
        let monitor = &topology.monitors()[*m];
        let Ok(area) = RatioRect::from_rect(*rect, monitor.work_area) else {
            continue;
        };
        let id = ZoneId(next_id("z", &taken_zone_ids));
        taken_zone_ids.insert(id.0.clone());
        let base = members
            .first()
            .map_or_else(|| "Zone".to_owned(), |i| windows[*i].label.clone());
        let base = if base.trim().is_empty() {
            "Zone".to_owned()
        } else {
            base
        };
        let name = (1..)
            .map(|k| {
                if k == 1 {
                    base.clone()
                } else {
                    format!("{base} {k}")
                }
            })
            .find(|candidate| !names.contains(candidate))
            .unwrap_or(base);
        names.insert(name.clone());
        let monitor_index = monitor_index(&mut layout.monitors, monitor);
        layout.zones.push(Zone {
            id: id.clone(),
            name,
            color: ZoneColor::nth(kept.len() + n),
            monitor: monitor_index,
            area,
        });
        group_zone.insert(g, id);
    }
    let mut zones = kept;
    zones.append(&mut layout.zones);
    layout.zones = zones;

    // Slots, front to back.
    let mut slot_ids = HashSet::new();
    for (seen, pick) in windows.iter().zip(picks) {
        let w = seen.facts;
        let place = match pick {
            Pick::Kept(zone) => Place::Zone { zone },
            Pick::Group(g) => match group_zone.get(&g) {
                Some(zone) => Place::Zone { zone: zone.clone() },
                None => free_place(&mut layout.monitors, topology, w.frame_rect),
            },
            Pick::Maximized(m) => Place::Maximized {
                monitor: monitor_index(&mut layout.monitors, &topology.monitors()[m]),
            },
            Pick::Free => free_place(&mut layout.monitors, topology, w.frame_rect),
        };
        let id = SlotId(next_id("s", &slot_ids));
        slot_ids.insert(id.0.clone());
        layout.slots.push(Slot {
            id,
            app: app_match(w),
            launch: seen.launch.clone(),
            place,
        });
    }

    Workspace {
        id,
        name,
        hotkey: previous.and_then(|p| p.hotkey),
        layout,
    }
}

/// Where a workspace's windows are now, for [`remember`]: each slot with the window that fills
/// it (if it is open), front to back.
#[derive(Clone, Debug)]
pub struct Present<'a> {
    pub slot: SlotId,
    pub window: &'a WindowFacts,
}

/// Updates `workspace` from where its windows are now: a window in one of its zones stays in
/// that zone, a maximised one stays maximised on its monitor, any other remembers its own place.
/// Minimised and closed windows keep the place they had. Slots are reordered front to back.
pub fn remember(
    workspace: &Workspace,
    present: &[Present<'_>],
    topology: &Topology,
    style: FrameStyle,
) -> Workspace {
    let mut out = workspace.clone();
    let zones = live_zones(&workspace.layout, topology, style);
    let mut refs = out.layout.monitors.clone();
    for p in present {
        let w = p.window;
        if w.minimized || !w.visible {
            continue;
        }
        let Some(slot) = out.layout.slots.iter_mut().find(|s| s.id == p.slot) else {
            continue;
        };
        if matches!(slot.place, Place::Tab { .. }) {
            continue;
        }
        slot.place = if w.maximized {
            Place::Maximized {
                monitor: monitor_index(&mut refs, topology.monitor_for_rect(w.frame_rect)),
            }
        } else if let Some(zone) = zone_holding(w.frame_rect, &zones, style) {
            Place::Zone { zone }
        } else {
            free_place(&mut refs, topology, w.frame_rect)
        };
        // The latest title, unless the user wrote a title rule of their own.
        if matches!(slot.app.title, None | Some(TitleMatch::Like(_))) && !w.title.trim().is_empty() {
            slot.app.title = Some(TitleMatch::Like(w.title.clone()));
        }
    }
    out.layout.monitors = refs;
    // Front to back: windows on screen in their current order, then the others as they were.
    let rank: HashMap<&SlotId, usize> = present.iter().enumerate().map(|(i, p)| (&p.slot, i)).collect();
    out.layout
        .slots
        .sort_by_key(|s| rank.get(&s.id).copied().unwrap_or(usize::MAX));
    out
}

/// One window to put in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Move {
    pub slot: SlotId,
    pub window: WindowHandle,
    pub target: Target,
}

/// Everything opening a workspace does, in order: move, start what is missing, minimise what
/// belonged to the previous workspace, bring the new windows forward (back to front) and focus
/// the front one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPlan {
    pub moves: Vec<Move>,
    pub launch: Vec<(SlotId, Launch)>,
    /// Slots with no open window and no way to start one.
    pub missing: Vec<SlotId>,
    pub minimize: Vec<WindowHandle>,
    /// Back to front.
    pub raise: Vec<WindowHandle>,
    pub focus: Option<WindowHandle>,
}

/// Decides how `to` opens. `windows` are the windows Mosaïk may move, front to back (minimised
/// ones included); `previous` the windows of the workspace being left.
pub fn plan_switch(
    to: &Workspace,
    windows: &[&WindowFacts],
    previous: &[WindowHandle],
    topology: &Topology,
    style: FrameStyle,
    settings: SwitchSettings,
) -> SwitchPlan {
    let slots: Vec<(SlotId, &AppMatch)> = to.layout.slots.iter().map(|s| (s.id.clone(), &s.app)).collect();
    let assigned: HashMap<SlotId, WindowHandle> = assign(&slots, windows).into_iter().collect();
    let resolver = Resolver::new(&to.layout, topology, style);
    let mut plan = SwitchPlan::default();
    for slot in &to.layout.slots {
        match assigned.get(&slot.id) {
            Some(window) => {
                if let Some(target) = to.layout.effective_place(slot).and_then(|p| resolver.target(p)) {
                    plan.moves.push(Move {
                        slot: slot.id.clone(),
                        window: *window,
                        target,
                    });
                }
            }
            None => match (&slot.launch, settings.launch_missing) {
                (Some(launch), true) => plan.launch.push((slot.id.clone(), launch.clone())),
                _ => plan.missing.push(slot.id.clone()),
            },
        }
    }
    let used: HashSet<WindowHandle> = assigned.values().copied().collect();
    if settings.minimize_others {
        let minimized: HashSet<WindowHandle> =
            windows.iter().filter(|w| w.minimized).map(|w| w.handle).collect();
        plan.minimize = previous
            .iter()
            .filter(|h| !used.contains(h) && !minimized.contains(h))
            .copied()
            .collect();
    }
    // Slots are front to back: raise from the back so the first slot ends on top.
    plan.raise = plan.moves.iter().rev().map(|m| m.window).collect();
    plan.focus = plan.moves.first().map(|m| m.window);
    plan
}

/// A short, readable id for a new workspace: its name in lower case with dashes ("Ship the
/// release" -> "ship-the-release"), made unique among `taken`.
pub fn workspace_id(name: &str, taken: &[WorkspaceId]) -> WorkspaceId {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let base = if slug.is_empty() { "workspace" } else { slug };
    let taken: HashSet<&str> = taken.iter().map(|t| t.0.as_str()).collect();
    let id = (1..)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|candidate| !taken.contains(candidate.as_str()))
        .unwrap_or_else(|| base.to_owned());
    WorkspaceId(id)
}

/// The name offered for a new workspace: "Workspace 1", "Workspace 2"... the first one free.
pub fn suggested_name(existing: &[Workspace]) -> String {
    let names: HashSet<String> = existing.iter().map(|w| w.name.to_lowercase()).collect();
    (1..)
        .map(|n| format!("Workspace {n}"))
        .find(|n| !names.contains(&n.to_lowercase()))
        .unwrap_or_else(|| "Workspace".to_owned())
}

/// The frame a window has inside a zone, the inverse of [`FrameStyle::window_area`] (for tests
/// and tools: the zone a window at `frame` would need).
pub fn zone_around(frame: Rect, style: FrameStyle, dpi: u32) -> Rect {
    let half_gap = crate::geometry::scale_for_dpi(style.gap, dpi) / 2;
    let band = crate::geometry::scale_for_dpi(style.tag_band, dpi);
    frame.outset(Insets::new(half_gap, band + half_gap, half_gap, half_gap))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::window::WindowFacts;

    fn win(h: u64, exe: &str, title: &str, frame: Rect) -> WindowFacts {
        WindowFacts {
            handle: WindowHandle(h),
            exe: Some(exe.into()),
            class: "C".into(),
            title: title.into(),
            visible: true,
            frame_rect: frame,
            window_rect: frame,
            dpi: 96,
            ..WindowFacts::default()
        }
    }

    fn seen<'a>(w: &'a WindowFacts, label: &str) -> Seen<'a> {
        Seen {
            facts: w,
            label: label.into(),
            launch: Some(Launch::Program {
                path: format!("C:\\Apps\\{}", w.exe.clone().unwrap_or_default()),
                args: Vec::new(),
                working_dir: None,
            }),
        }
    }

    fn style() -> FrameStyle {
        FrameStyle::default()
    }

    /// Windows 11 snap: editor on the left half, browser top right, terminal bottom right.
    fn snapped() -> Vec<WindowFacts> {
        vec![
            win(
                1,
                "code.exe",
                "lib.rs - mosaik - Visual Studio Code",
                Rect::new(0, 0, 960, 1032),
            ),
            win(
                2,
                "chrome.exe",
                "Pull request - Google Chrome",
                Rect::new(960, 0, 1920, 516),
            ),
            win(3, "windowsterminal.exe", "pwsh", Rect::new(960, 516, 1920, 1032)),
        ]
    }

    #[test]
    fn tiled_windows_become_zones_named_after_their_apps() {
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws
            .iter()
            .zip(["Visual Studio Code", "Google Chrome", "Terminal"])
            .map(|(w, l)| seen(w, l))
            .collect();
        let w = capture(WorkspaceId("w".into()), "Mosaïk".into(), &seen, &t, style(), None);
        assert_eq!(w.layout.problems(), vec![]);
        assert_eq!(w.layout.zones.len(), 3);
        let names: Vec<&str> = w.layout.zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(names, ["Visual Studio Code", "Google Chrome", "Terminal"]);
        let colors: Vec<ZoneColor> = w.layout.zones.iter().map(|z| z.color).collect();
        assert_eq!(
            colors,
            [ZoneColor::Lapis, ZoneColor::Verdigris, ZoneColor::Saffron]
        );
        // Every window is in its zone, slots front to back.
        for (slot, zone) in w.layout.slots.iter().zip(&w.layout.zones) {
            assert_eq!(
                slot.place,
                Place::Zone {
                    zone: zone.id.clone()
                }
            );
        }
        assert_eq!(
            w.layout.slots[0].app.title,
            Some(TitleMatch::Like("lib.rs - mosaik - Visual Studio Code".into()))
        );
        // The zones are exactly where the windows were; windows then sit inside them, below
        // their tags.
        let r = Resolver::new(&w.layout, &t, style());
        assert_eq!(
            r.zone_rect(&w.layout.zones[0].id),
            Some((Rect::new(0, 0, 960, 1032), 96))
        );
        assert_eq!(
            r.target(&w.layout.slots[1].place),
            Some(Target::Frame(Rect::new(964, 32, 1916, 512)))
        );
    }

    #[test]
    fn stacked_windows_share_a_zone_and_overlapping_ones_float() {
        let t = fixtures::single_1080p_100();
        let ws = vec![
            // A small window floating over the editor.
            win(1, "notepad.exe", "notes", Rect::new(200, 200, 600, 500)),
            // Two windows snapped to the same half (a few pixels apart).
            win(
                2,
                "code.exe",
                "a - x - Visual Studio Code",
                Rect::new(0, 0, 960, 1032),
            ),
            win(
                3,
                "code.exe",
                "b - y - Visual Studio Code",
                Rect::new(2, 0, 961, 1030),
            ),
            // Maximised.
            win(4, "outlook.exe", "Inbox", Rect::new(0, 0, 1920, 1032)),
        ];
        let mut maximized = ws.clone();
        maximized[3].maximized = true;
        let messy: Vec<Seen> = maximized
            .iter()
            .zip(["Notepad", "Editor", "Editor", "Outlook"])
            .map(|(w, l)| seen(w, l))
            .collect();
        let w = capture(WorkspaceId("w".into()), "Messy".into(), &messy, &t, style(), None);
        assert_eq!(w.layout.problems(), vec![]);
        // The notes float over the editors, which share one zone; the mail stays maximised.
        assert_eq!(w.layout.zones.len(), 1);
        assert_eq!(w.layout.zones[0].name, "Editor");
        assert!(matches!(w.layout.slots[0].place, Place::Free { .. }));
        assert_eq!(
            w.layout.slots[1].place,
            Place::Zone {
                zone: w.layout.zones[0].id.clone()
            }
        );
        assert_eq!(w.layout.slots[1].place, w.layout.slots[2].place);
        assert_eq!(w.layout.slots[3].place, Place::Maximized { monitor: 0 });

        // Cascaded windows: each overlaps the one behind it, so only the back one is a zone.
        let cascade = [
            win(1, "a.exe", "a", Rect::new(200, 200, 1200, 900)),
            win(2, "b.exe", "b", Rect::new(100, 100, 1100, 800)),
            win(3, "c.exe", "c", Rect::new(0, 0, 1000, 700)),
        ];
        let stacked: Vec<Seen> = cascade.iter().map(|w| seen(w, "App")).collect();
        let w = capture(
            WorkspaceId("w".into()),
            "Cascade".into(),
            &stacked,
            &t,
            style(),
            None,
        );
        assert_eq!(w.layout.zones.len(), 1);
        assert!(matches!(w.layout.slots[0].place, Place::Free { .. }));
        assert!(matches!(w.layout.slots[1].place, Place::Free { .. }));
        assert!(matches!(w.layout.slots[2].place, Place::Zone { .. }));
    }

    #[test]
    fn saving_again_keeps_zones_that_windows_sit_in() {
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let seen1: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let mut first = capture(WorkspaceId("w".into()), "W".into(), &seen1, &t, style(), None);
        first.layout.zones[0].name = "Code".into();
        // Mosaïk placed the windows inside their zones; a new window is added beside nothing.
        let r = Resolver::new(&first.layout, &t, style());
        let placed: Vec<WindowFacts> = ws
            .iter()
            .zip(&first.layout.slots)
            .map(|(w, s)| {
                let Some(Target::Frame(f)) = r.target(&s.place) else {
                    unreachable!("every slot has a zone")
                };
                WindowFacts {
                    frame_rect: f,
                    ..w.clone()
                }
            })
            .collect();
        let seen2: Vec<Seen> = placed.iter().map(|w| seen(w, "App")).collect();
        let second = capture(
            WorkspaceId("w".into()),
            "W".into(),
            &seen2,
            &t,
            style(),
            Some(&first),
        );
        assert_eq!(
            second.layout.zones, first.layout.zones,
            "zones neither shrink nor get renamed"
        );
        assert_eq!(second.layout.slots.len(), 3);
        for s in &second.layout.slots {
            assert!(matches!(s.place, Place::Zone { .. }));
        }
    }

    #[test]
    fn capture_works_on_every_topology_and_lands_on_screen() {
        for t in fixtures::all() {
            let ws: Vec<WindowFacts> = t
                .monitors()
                .iter()
                .enumerate()
                .flat_map(|(i, m)| {
                    let a = m.work_area;
                    let mid = a.left + a.width() / 2;
                    let i = u64::try_from(i).unwrap_or(0);
                    vec![
                        win(i * 10 + 1, "a.exe", "a", Rect::new(a.left, a.top, mid, a.bottom)),
                        win(i * 10 + 2, "b.exe", "b", Rect::new(mid, a.top, a.right, a.bottom)),
                    ]
                })
                .collect();
            let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
            let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
            assert_eq!(w.layout.problems(), vec![], "{}", t.fingerprint());
            assert_eq!(w.layout.zones.len(), ws.len(), "{}", t.fingerprint());
            let r = Resolver::new(&w.layout, &t, style());
            for s in &w.layout.slots {
                let Some(Target::Frame(f)) = r.target(&s.place) else {
                    unreachable!("zoned slots resolve to frames")
                };
                assert!(t.is_reachable(f, 32), "{f:?} off screen on {}", t.fingerprint());
                let m = t.monitor_for_rect(f);
                assert!(m.work_area.contains_rect(f));
            }
        }
    }

    #[test]
    fn remember_follows_windows_and_keeps_closed_ones() {
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
        let r = Resolver::new(&w.layout, &t, style());
        // The editor moved into the terminal's zone, the browser was maximised, the terminal
        // was closed, and the browser is now in front.
        let Some(Target::Frame(terminal_area)) = r.target(&w.layout.slots[2].place) else {
            unreachable!()
        };
        let editor = WindowFacts {
            frame_rect: terminal_area,
            title: "main.rs - mosaik - Visual Studio Code".into(),
            ..ws[0].clone()
        };
        let browser = WindowFacts {
            maximized: true,
            frame_rect: Rect::new(0, 0, 1920, 1032),
            ..ws[1].clone()
        };
        let present = [
            Present {
                slot: w.layout.slots[1].id.clone(),
                window: &browser,
            },
            Present {
                slot: w.layout.slots[0].id.clone(),
                window: &editor,
            },
        ];
        let after = remember(&w, &present, &t, style());
        assert_eq!(after.layout.problems(), vec![]);
        let ids: Vec<&str> = after.layout.slots.iter().map(|s| s.id.0.as_str()).collect();
        assert_eq!(ids, ["s2", "s1", "s3"], "front to back, closed last");
        assert_eq!(after.layout.slots[0].place, Place::Maximized { monitor: 0 });
        assert_eq!(
            after.layout.slots[1].place, w.layout.slots[2].place,
            "editor in the terminal's zone"
        );
        assert_eq!(
            after.layout.slots[2].place, w.layout.slots[2].place,
            "closed window keeps its place"
        );
        assert_eq!(
            after.layout.slots[1].app.title,
            Some(TitleMatch::Like("main.rs - mosaik - Visual Studio Code".into()))
        );
        // A window moved somewhere of its own remembers that place.
        let floating = WindowFacts {
            frame_rect: Rect::new(100, 100, 700, 500),
            ..ws[2].clone()
        };
        let after = remember(
            &w,
            &[Present {
                slot: w.layout.slots[2].id.clone(),
                window: &floating,
            }],
            &t,
            style(),
        );
        let Place::Free { monitor: 0, area } = after.layout.slots[0].place else {
            unreachable!("{:?}", after.layout.slots[0].place)
        };
        assert_eq!(area.to_rect(t.primary().work_area), Rect::new(100, 100, 700, 500));
    }

    #[test]
    fn switching_moves_starts_minimises_and_focuses() {
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
        // Now: the editor is open (minimised), the browser is open elsewhere, the terminal is
        // closed, and a mail window from the previous workspace is on screen.
        let editor = WindowFacts {
            minimized: true,
            ..ws[0].clone()
        };
        let browser = WindowFacts {
            frame_rect: Rect::new(10, 10, 500, 500),
            ..ws[1].clone()
        };
        let mail = win(9, "outlook.exe", "Inbox", Rect::new(0, 0, 1920, 1032));
        let plan = plan_switch(
            &w,
            &[&mail, &browser, &editor],
            &[mail.handle, browser.handle],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert_eq!(plan.moves.len(), 2);
        assert_eq!(plan.moves[0].window, WindowHandle(1));
        assert_eq!(plan.moves[0].target, Target::Frame(Rect::new(4, 32, 956, 1028)));
        assert_eq!(plan.launch.len(), 1, "the terminal is started");
        assert_eq!(
            plan.minimize,
            vec![WindowHandle(9)],
            "mail left, the browser stays"
        );
        assert_eq!(plan.raise, vec![WindowHandle(2), WindowHandle(1)]);
        assert_eq!(plan.focus, Some(WindowHandle(1)));

        let quiet = SwitchSettings {
            minimize_others: false,
            launch_missing: false,
        };
        let plan = plan_switch(&w, &[&browser], &[mail.handle], &t, style(), quiet);
        assert!(plan.minimize.is_empty());
        assert!(plan.launch.is_empty());
        assert_eq!(plan.missing.len(), 2);
    }

    #[test]
    fn ids_and_names() {
        let taken = [WorkspaceId("ship-the-release".into())];
        assert_eq!(workspace_id("Ship the release!", &taken).0, "ship-the-release-2");
        assert_eq!(workspace_id("  Été 2026 ", &[]).0, "été-2026");
        assert_eq!(workspace_id("!!!", &[]).0, "workspace");
        let one = Workspace {
            id: WorkspaceId("a".into()),
            name: "Workspace 1".into(),
            hotkey: None,
            layout: Layout::default(),
        };
        assert_eq!(suggested_name(&[]), "Workspace 1");
        assert_eq!(suggested_name(&[one]), "Workspace 2");
    }

    #[test]
    fn zone_around_inverts_window_area() {
        for dpi in [96, 120, 144, 192] {
            let frame = Rect::new(100, 200, 900, 800);
            let zone = zone_around(frame, style(), dpi);
            assert_eq!(style().window_area(zone, dpi), frame, "at {dpi}");
        }
    }
}
