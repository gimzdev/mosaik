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

use crate::geometry::{RatioRect, Rect};
use crate::layout::{Launch, Layout, Place, Slot, SlotId, Workspace, WorkspaceId};
use crate::matching::{AppMatch, TitleMatch, assign};
use crate::place::{Resolver, Target};
use crate::settings::SwitchSettings;
use crate::topology::{MonitorId, MonitorRef, Topology, map_monitors, remembered_index};
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
/// A window sits in a zone when it and the zone's window area cover at least this much of each
/// other.
const IN_ZONE: f64 = 0.7;

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

/// The windows to check once moved windows have settled, each with where it was last meant to
/// land, in the order they were moved. A window moved twice before then (zones that followed a
/// resize, then made room for an app's own size) is checked against its last place only:
/// checked against both, it would be put back where the first move meant it to go.
pub fn last_places(placed: Vec<(WindowHandle, Rect)>) -> Vec<(WindowHandle, Rect)> {
    let last: HashMap<WindowHandle, usize> = placed.iter().enumerate().map(|(i, (w, _))| (*w, i)).collect();
    placed
        .into_iter()
        .enumerate()
        .filter(|(i, (w, _))| last.get(w) == Some(i))
        .map(|(_, p)| p)
        .collect()
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

/// Whether a window whose visible frame is `frame` sits in the zone at `zone` (on a monitor at
/// `dpi`): its centre is in the zone, and it and the zone's window area cover at least 70% of
/// each other. That holds for a window Mosaïk put there, including an app that kept a size a
/// little larger than the zone (a browser in a narrow zone), but not for a small window
/// floating over the zone or a large one spanning two zones. One rule for everything that asks
/// "is this window in its zone?": frames, remembering, saving again.
pub fn holds(zone: Rect, dpi: u32, frame: Rect, style: FrameStyle) -> bool {
    if !zone.contains(frame.center()) {
        return false;
    }
    let area = style.window_area(zone, dpi);
    let larger = frame.area().max(area.area());
    if larger <= 0 {
        return false;
    }
    // Areas of real screens are far below 2^52, so the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    let covered = frame.overlap_area(area) as f64 / larger as f64;
    covered >= IN_ZONE
}

/// Whether a window that cannot be resized (a game, a fixed-size tool) sits in the zone at
/// `zone`: Mosaïk puts it in the zone at its own size (`place::fit_size`), centred across and at
/// the top of a zone larger than it, over the middle of a zone smaller than it. So along each
/// axis its middle is in the zone, or the zone's middle is in it, and it overlaps the zone's
/// window area by 70% of as much as its size allows: all of a window smaller than the zone, all
/// of a zone smaller than the window. A window that could fill the zone and does not is
/// floating over it ([`holds`]).
pub fn holds_fixed(zone: Rect, dpi: u32, frame: Rect, style: FrameStyle) -> bool {
    let area = style.window_area(zone, dpi);
    let best = i64::from(frame.width().min(area.width()).max(0))
        * i64::from(frame.height().min(area.height()).max(0));
    if best <= 0 {
        return false;
    }
    let (middle, area_middle) = (frame.center(), area.center());
    let across =
        (zone.left..zone.right).contains(&middle.x) || (frame.left..frame.right).contains(&area_middle.x);
    let down =
        (zone.top..zone.bottom).contains(&middle.y) || (frame.top..frame.bottom).contains(&area_middle.y);
    if !(across && down) {
        return false;
    }
    // Areas of real screens are far below 2^52, so the conversion is exact.
    #[allow(clippy::cast_precision_loss)]
    let covered = frame.overlap_area(area) as f64 / best as f64;
    covered >= IN_ZONE
}

/// [`holds`] for a window the user can resize, [`holds_fixed`] for one that cannot be.
pub fn holds_window(zone: Rect, dpi: u32, frame: Rect, style: FrameStyle, resizable: bool) -> bool {
    if resizable {
        holds(zone, dpi, frame, style)
    } else {
        holds_fixed(zone, dpi, frame, style)
    }
}

/// The zone a window sits in (see [`holds_window`]); the smallest one when zones overlap.
pub fn zone_holding(
    frame: Rect,
    resizable: bool,
    zones: &[(ZoneId, Rect, u32)],
    style: FrameStyle,
) -> Option<ZoneId> {
    holding(
        frame,
        resizable,
        zones.iter().map(|(id, rect, dpi)| (id, *rect, *dpi)),
        style,
    )
    .cloned()
}

/// Of `zones` (each with its rectangle and its monitor's DPI), the one a window whose visible
/// frame is `frame` sits in ([`holds_window`]). When several hold it (zones that overlap), the
/// one whose middle is nearest the window's, then the smallest.
pub fn holding<'a, Z>(
    frame: Rect,
    resizable: bool,
    zones: impl IntoIterator<Item = (&'a Z, Rect, u32)>,
    style: FrameStyle,
) -> Option<&'a Z> {
    let middle = frame.center();
    let off = |r: Rect| {
        let c = r.center();
        let (dx, dy) = (
            i64::from(c.x) - i64::from(middle.x),
            i64::from(c.y) - i64::from(middle.y),
        );
        dx * dx + dy * dy
    };
    zones
        .into_iter()
        .filter(|(_, rect, dpi)| holds_window(*rect, *dpi, frame, style, resizable))
        .min_by_key(|(_, rect, _)| (off(*rect), rect.area()))
        .map(|(z, _, _)| z)
}

/// Whether two workspace names are the same for the user: case and surrounding spaces do not
/// count, in any language.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
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

/// The first of `prefix1`, `prefix2`... not in `taken`.
pub(crate) fn next_id(prefix: &str, taken: &HashSet<String>) -> String {
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
pub(crate) fn free_place(refs: &mut Vec<MonitorRef>, topology: &Topology, frame: Rect) -> Place {
    let monitor = topology.monitor_for_rect(frame);
    let index = remembered_index(refs, monitor, topology);
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
        remembered_index(&mut layout.monitors, m, topology);
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
            z.monitor = remembered_index(&mut layout.monitors, now, topology);
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
        if let Some(z) = zone_holding(w.frame_rect, w.resizable, &kept_live, style) {
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

    // New zones, coloured monitor by monitor, from the left; every zone is numbered once they
    // are all in, in reading order.
    let mut taken_zone_ids: HashSet<String> = kept.iter().map(|z| z.id.0.clone()).collect();
    let mut order: Vec<usize> = (0..groups.len()).filter(|g| tiles[*g]).collect();
    order.sort_by_key(|g| {
        let (m, r, _) = &groups[*g];
        (*m, r.left, r.top)
    });
    let mut group_zone: HashMap<usize, ZoneId> = HashMap::new();
    for (n, g) in order.into_iter().enumerate() {
        let (m, rect, _) = &groups[g];
        let monitor = &topology.monitors()[*m];
        let Ok(area) = RatioRect::from_rect(*rect, monitor.work_area) else {
            continue;
        };
        let id = ZoneId(next_id("z", &taken_zone_ids));
        taken_zone_ids.insert(id.0.clone());
        let monitor_index = remembered_index(&mut layout.monitors, monitor, topology);
        layout.zones.push(Zone {
            id: id.clone(),
            name: String::new(),
            color: ZoneColor::nth(kept.len() + n),
            monitor: monitor_index,
            area,
        });
        group_zone.insert(g, id);
    }
    let mut zones = kept;
    zones.append(&mut layout.zones);
    layout.zones = zones;
    layout.number_zones();

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
                monitor: remembered_index(&mut layout.monitors, &topology.monitors()[m], topology),
            },
            Pick::Free => free_place(&mut layout.monitors, topology, w.frame_rect),
        };
        let id = SlotId(next_id("s", &slot_ids));
        slot_ids.insert(id.0.clone());
        layout.slots.push(Slot {
            id,
            label: Some(seen.label.clone()),
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

/// What a save builds on (`capture`'s `previous`): the zones on screen (what Arrange put there,
/// or the open workspace's) under the id, name and shortcut of the workspace saved over, none for
/// a new one (ADR 0030). Saving keeps what the user sees: Arrange's zones are saved nowhere
/// else, and a workspace saved over keeps none of its old zones that are not on screen. With
/// nothing on screen, the workspace saved over keeps its own.
pub fn saved_over(target: Option<&Workspace>, on_screen: Option<&Workspace>) -> Option<Workspace> {
    let Some(screen) = on_screen else {
        return target.cloned();
    };
    Some(match target {
        Some(t) => Workspace {
            layout: screen.layout.clone(),
            ..t.clone()
        },
        None => Workspace {
            hotkey: None,
            ..screen.clone()
        },
    })
}

/// Where a workspace's windows are now, for [`remember`]: each slot with the window that fills
/// it (if it is open), front to back.
#[derive(Clone, Debug)]
pub struct Present<'a> {
    pub slot: SlotId,
    pub window: &'a WindowFacts,
}

/// Updates `workspace` from where the given windows are now (the engine passes the window the
/// user just moved by hand): a window in one of its zones stays in that zone, a maximised one
/// stays maximised on its monitor, any other remembers its own place. Minimised windows keep the
/// place they had. The given windows move to the front of the slot order, front to back.
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
        // The zone it is placed in, when it is still there: a window larger than its zone can
        // cover a neighbouring zone just as well.
        let placed_in = workspace
            .layout
            .slot(&p.slot)
            .and_then(|s| workspace.layout.effective_place(s))
            .and_then(|place| match place {
                Place::Zone { zone } => zones.iter().find(|(id, _, _)| id == zone),
                _ => None,
            })
            .filter(|(_, rect, dpi)| holds_window(*rect, *dpi, w.frame_rect, style, w.resizable))
            .map(|(id, _, _)| id.clone());
        let place = if w.maximized {
            Place::Maximized {
                monitor: remembered_index(&mut refs, topology.monitor_for_rect(w.frame_rect), topology),
            }
        } else if let Some(zone) =
            placed_in.or_else(|| zone_holding(w.frame_rect, w.resizable, &zones, style))
        {
            Place::Zone { zone }
        } else {
            free_place(&mut refs, topology, w.frame_rect)
        };
        let Some(slot) = out.layout.slots.iter_mut().find(|s| s.id == p.slot) else {
            continue;
        };
        // The latest title, unless the user wrote a title rule of their own.
        if matches!(slot.app.title, None | Some(TitleMatch::Like(_))) && !w.title.trim().is_empty() {
            slot.app.title = Some(TitleMatch::Like(w.title.clone()));
        }
        // A tab moves its whole group: the others follow when they are shown. A hidden tab
        // says nothing about where its group is.
        if let Place::Tab { group } = &slot.place {
            let group = group.clone();
            if matches!(w.cloak, crate::window::Cloak::None) {
                out.layout.move_group(&group, place);
            }
            continue;
        }
        slot.place = place;
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
    /// The zone the target fills (with its monitor's DPI), for a window in a zone or a tab of a
    /// group in one: a window wearing its tag above it goes below a band in it instead.
    pub zone: Option<(Rect, u32)>,
}

/// Everything opening a workspace does, in order: move, start what is missing, minimise what
/// belonged to the previous workspace, bring the new windows forward (back to front) and focus
/// the front one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPlan {
    /// Every slot an open window fills, background tabs included, front to back.
    pub assigned: Vec<(SlotId, WindowHandle)>,
    /// Background tabs are not moved: a tab goes to its group's place when it is shown.
    pub moves: Vec<Move>,
    pub launch: Vec<(SlotId, Launch)>,
    /// Slots with no open window and no way to start one.
    pub missing: Vec<SlotId>,
    pub minimize: Vec<WindowHandle>,
    /// Back to front. Background tabs are not raised.
    pub raise: Vec<WindowHandle>,
    pub focus: Option<WindowHandle>,
    /// Tab groups: which window shows, which hide.
    pub tabs: Vec<TabShow>,
}

/// A tab group as a switch leaves it: its windows (in tab order) and the one that shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabShow {
    pub group: crate::layout::TabGroupId,
    pub windows: Vec<WindowHandle>,
    pub active: WindowHandle,
}

impl TabShow {
    /// The tabs to keep hidden.
    pub fn background(&self) -> impl Iterator<Item = WindowHandle> + '_ {
        self.windows.iter().copied().filter(|w| *w != self.active)
    }
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
    // Tab groups: the visible tab shows, or when its window is not open, the tab next to it
    // (the same rule the engine applies on screen, `crate::tabs`).
    for g in &to.layout.tab_groups {
        let now = crate::tabs::GroupNow {
            id: g.id.clone(),
            order: g.tabs.clone(),
            open: g
                .tabs
                .iter()
                .filter_map(|s| assigned.get(s).map(|w| (s.clone(), *w)))
                .collect(),
            wanted: Some(g.active.clone()),
            reach: crate::tabs::Reach::Strip,
        };
        let Some((_, active)) = now.visible().cloned() else {
            continue;
        };
        plan.tabs.push(TabShow {
            group: g.id.clone(),
            windows: now.open.iter().map(|(_, w)| *w).collect(),
            active,
        });
    }
    let background: HashSet<WindowHandle> = plan.tabs.iter().flat_map(TabShow::background).collect();
    // A window in full screen (a video, a game) is the workspace's, but stays as it is: moving,
    // raising or minimising it would take it out of full screen.
    let full_screen: HashSet<WindowHandle> = windows
        .iter()
        .filter(|w| w.full_screen)
        .map(|w| w.handle)
        .collect();
    // Front to back by slot; a tab group is as far forward as its front-most tab, whichever tab
    // shows (a hidden tab saved last takes the group's place when the others are closed).
    let position: HashMap<&SlotId, usize> = to
        .layout
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| (&s.id, i))
        .collect();
    let rank = |slot: &Slot| -> usize {
        let own = position.get(&slot.id).copied().unwrap_or(usize::MAX);
        to.layout.group_of(&slot.id).map_or(own, |g| {
            g.tabs
                .iter()
                .filter_map(|t| position.get(t).copied())
                .min()
                .unwrap_or(own)
        })
    };
    let mut ordered: Vec<&Slot> = to.layout.slots.iter().collect();
    ordered.sort_by_key(|s| rank(s));
    for slot in ordered {
        match assigned.get(&slot.id) {
            Some(window) => {
                plan.assigned.push((slot.id.clone(), *window));
                if background.contains(window) || full_screen.contains(window) {
                    continue;
                }
                if let Some(target) = resolver.target(&slot.place) {
                    plan.moves.push(Move {
                        slot: slot.id.clone(),
                        window: *window,
                        target,
                        zone: resolver.zone_of(&slot.place),
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
        // The monitors the workspace is on: its zones' and its apps' places. Every other window
        // there goes to the taskbar, so nothing stays on top of it (the owner's game launcher
        // did); a window of the workspace left goes wherever it is. Only windows on screen now:
        // one hidden (an app closed to the tray), on another desktop or gone is left alone,
        // since minimising would show it.
        let mut its: HashSet<&MonitorId> = to
            .layout
            .zones
            .iter()
            .filter_map(|z| resolver.zone_rect(&z.id))
            .map(|(r, _)| &topology.monitor_for_rect(r).id)
            .collect();
        for slot in &to.layout.slots {
            match resolver.target(&slot.place) {
                Some(Target::Frame(r)) => {
                    its.insert(&topology.monitor_for_rect(r).id);
                }
                Some(Target::Maximized(m)) => {
                    if let Some(m) = topology.get(&m) {
                        its.insert(&m.id);
                    }
                }
                None => {}
            }
        }
        plan.minimize = windows
            .iter()
            .filter(|w| w.visible && !w.minimized && w.cloak == crate::window::Cloak::None)
            .filter(|w| !used.contains(&w.handle) && !full_screen.contains(&w.handle))
            .filter(|w| {
                previous.contains(&w.handle) || its.contains(&topology.monitor_for_rect(w.frame_rect).id)
            })
            .map(|w| w.handle)
            .collect();
    }
    // Slots are front to back: raise from the back so the first slot ends on top.
    plan.raise = plan.moves.iter().rev().map(|m| m.window).collect();
    plan.focus = plan.moves.first().map(|m| m.window);
    plan
}

/// One tab of a tab group on screen, when saving.
#[derive(Clone, Debug)]
pub enum TabSeen<'a> {
    /// A window that was captured, at this position among the captured windows.
    Captured(usize),
    /// A tab Mosaïk keeps hidden: it becomes a slot of the group (it has no place of its own).
    Hidden(Seen<'a>),
}

/// A tab group on screen, when saving: its tabs in order and the one that shows.
#[derive(Clone, Debug)]
pub struct GroupSeen<'a> {
    pub tabs: Vec<TabSeen<'a>>,
    /// Position in `tabs` of the tab that shows; it must be a captured one, since the group
    /// goes where it is.
    pub visible: usize,
}

/// After [`capture`]: the tab groups on screen become tab groups of the workspace again, where
/// their visible tab was captured. Hidden tabs become slots of their group; the slots made for
/// them are returned with their windows. A group that would not be sound (its visible tab not
/// captured, fewer than two tabs) is left out, and its captured windows stay windows of their
/// own.
pub fn regroup(ws: &mut Workspace, groups: &[GroupSeen<'_>]) -> Vec<(SlotId, WindowHandle)> {
    let mut made = Vec::new();
    for g in groups {
        let Some(TabSeen::Captured(v)) = g.tabs.get(g.visible) else {
            continue;
        };
        let Some((visible, place)) = ws.layout.slots.get(*v).map(|s| (s.id.clone(), s.place.clone())) else {
            continue;
        };
        if matches!(place, Place::Tab { .. }) {
            continue;
        }
        let mut taken: HashSet<String> = ws.layout.slots.iter().map(|s| s.id.0.clone()).collect();
        let mut tabs: Vec<SlotId> = Vec::with_capacity(g.tabs.len());
        let mut added: Vec<(Slot, WindowHandle)> = Vec::new();
        for t in &g.tabs {
            match t {
                TabSeen::Captured(i) => {
                    if let Some(s) = ws.layout.slots.get(*i)
                        && !matches!(s.place, Place::Tab { .. })
                        && !tabs.contains(&s.id)
                    {
                        tabs.push(s.id.clone());
                    }
                }
                TabSeen::Hidden(seen) => {
                    let id = SlotId(next_id("s", &taken));
                    taken.insert(id.0.clone());
                    tabs.push(id.clone());
                    added.push((
                        Slot {
                            id,
                            label: Some(seen.label.clone()),
                            app: app_match(seen.facts),
                            launch: seen.launch.clone(),
                            place: place.clone(),
                        },
                        seen.facts.handle,
                    ));
                }
            }
        }
        if tabs.len() < 2 {
            continue;
        }
        for (slot, window) in added {
            made.push((slot.id.clone(), window));
            ws.layout.slots.push(slot);
        }
        // Sound by the checks above; were it not, the hidden tabs would simply stay windows of
        // their own in the group's place.
        let _ = ws.layout.make_group(&tabs, &visible, place);
    }
    made
}

/// Where the windows already filling a workspace's slots go on the monitors connected now:
/// after a display change, when a window the workspace started appears, or when one is added.
/// Minimised windows are left where they are, but for `shown`: a window just added to the
/// workspace, which goes to its place and shows again.
pub fn plan_refit(
    ws: &Workspace,
    windows: &[(SlotId, &WindowFacts)],
    topology: &Topology,
    style: FrameStyle,
    shown: Option<WindowHandle>,
) -> Vec<Move> {
    let resolver = Resolver::new(&ws.layout, topology, style);
    windows
        .iter()
        .filter(|(_, w)| !w.minimized || Some(w.handle) == shown)
        .filter_map(|(slot, w)| {
            let s = ws.layout.slot(slot)?;
            let target = resolver.target(&s.place)?;
            Some(Move {
                slot: slot.clone(),
                window: w.handle,
                target,
                zone: resolver.zone_of(&s.place),
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_moved_twice_is_checked_against_its_last_place() {
        // Job 0092: zones followed a resize, then made room for a browser's own size; the
        // check that follows put two windows back where the first move had meant them to go,
        // 104 px off their zones.
        let (a, b) = (WindowHandle(1), WindowHandle(2));
        let first = Rect::new(4, 32, 1302, 1028);
        let second = Rect::new(4, 32, 1198, 1028);
        let other = Rect::new(1310, 32, 1916, 562);
        assert_eq!(
            last_places(vec![(a, first), (b, other), (a, second)]),
            vec![(b, other), (a, second)]
        );
        assert_eq!(last_places(vec![(a, first)]), vec![(a, first)]);
        assert!(last_places(Vec::new()).is_empty());
    }
    use crate::fixtures;
    use crate::geometry::Size;
    use crate::window::WindowFacts;

    /// An app window as most are: titled, with a caption and a frame to resize it by.
    fn win(h: u64, exe: &str, title: &str, frame: Rect) -> WindowFacts {
        WindowFacts {
            handle: WindowHandle(h),
            exe: Some(exe.into()),
            class: "C".into(),
            title: title.into(),
            visible: true,
            caption: true,
            resizable: true,
            minimize_box: true,
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
    fn tiled_windows_become_zones_numbered_by_place() {
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
        // Numbered in reading order (the owner: "they should be 1 2 3 4 5 6"), never named after
        // the apps in them: the left half 1, the top right 2, the bottom right 3.
        let names: Vec<&str> = w.layout.zones.iter().map(|z| z.name.as_str()).collect();
        assert_eq!(names, ["1", "2", "3"]);
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
            Some(Target::Frame(Rect::new(960, 28, 1920, 516)))
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
        assert_eq!(w.layout.zones[0].name, "1");
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
        // A name saved before zones were numbered.
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
        first.layout.zones[0].name = "1".into();
        assert_eq!(
            second.layout.zones, first.layout.zones,
            "zones neither shrink nor move; the old name gives way to the zone's number"
        );
        assert_eq!(second.layout.slots.len(), 3);
        for s in &second.layout.slots {
            assert!(matches!(s.place, Place::Zone { .. }));
        }
    }

    /// `windows` put inside the zones of `ws`, as Mosaïk puts them.
    fn placed_in(ws: &Workspace, windows: &[WindowFacts], t: &Topology) -> Vec<WindowFacts> {
        let r = Resolver::new(&ws.layout, t, style());
        windows
            .iter()
            .zip(&ws.layout.slots)
            .map(|(w, s)| {
                let Some(Target::Frame(f)) = r.target(&s.place) else {
                    unreachable!("every slot has a zone")
                };
                WindowFacts {
                    frame_rect: f,
                    ..w.clone()
                }
            })
            .collect()
    }

    #[test]
    fn a_save_takes_the_zones_on_screen_and_the_name_and_shortcut_saved_over() {
        // ADR 0030: Arrange changes no workspace, so its zones are on screen only. Saved over
        // "Email" (three zones), the screen Arrange left (two halves) is what is kept: Email's
        // own zones would leave the arranged windows over zones they do not fill.
        let t = fixtures::single_1080p_100();
        let three = snapped();
        let seen3: Vec<Seen> = three.iter().map(|w| seen(w, "App")).collect();
        let mut email = capture(
            WorkspaceId("email".into()),
            "Email".into(),
            &seen3,
            &t,
            style(),
            None,
        );
        email.hotkey = "Win+Alt+E".parse().ok();
        assert!(email.hotkey.is_some());
        let halves = vec![
            win(
                1,
                "code.exe",
                "a - Visual Studio Code",
                Rect::new(0, 0, 960, 1032),
            ),
            win(
                2,
                "chrome.exe",
                "b - Google Chrome",
                Rect::new(960, 0, 1920, 1032),
            ),
        ];
        let seen2: Vec<Seen> = halves.iter().map(|w| seen(w, "App")).collect();
        let arranged = capture(
            WorkspaceId::arranged(),
            "Arranged".into(),
            &seen2,
            &t,
            style(),
            None,
        );
        assert_eq!(arranged.layout.zones.len(), 2);
        let now = placed_in(&arranged, &halves, &t);
        let seen_now: Vec<Seen> = now.iter().map(|w| seen(w, "App")).collect();

        let base = saved_over(Some(&email), Some(&arranged));
        let saved = capture(
            email.id.clone(),
            email.name.clone(),
            &seen_now,
            &t,
            style(),
            base.as_ref(),
        );
        assert_eq!(saved.id, email.id);
        assert_eq!(saved.name, "Email");
        assert_eq!(saved.hotkey, email.hotkey, "the shortcut stays the workspace's");
        assert_eq!(saved.layout.zones, arranged.layout.zones, "the zones on screen");
        assert_eq!(saved.layout.problems(), vec![]);
        for s in &saved.layout.slots {
            assert!(matches!(s.place, Place::Zone { .. }), "{:?}", s.place);
        }
        // Saved over Email's own zones, as before, all three would stay.
        let old = capture(
            email.id.clone(),
            email.name.clone(),
            &seen_now,
            &t,
            style(),
            Some(&email),
        );
        assert!(old.layout.zones.len() >= 3);
    }

    #[test]
    fn a_new_workspace_takes_the_zones_on_screen_but_no_shortcut() {
        let t = fixtures::single_1080p_100();
        let three = snapped();
        let seen3: Vec<Seen> = three.iter().map(|w| seen(w, "App")).collect();
        let mut coding = capture(
            WorkspaceId("coding".into()),
            "Coding".into(),
            &seen3,
            &t,
            style(),
            None,
        );
        coding.hotkey = "Win+Alt+C".parse().ok();
        let base = saved_over(None, Some(&coding));
        assert_eq!(base.as_ref().map(|b| &b.layout), Some(&coding.layout));
        assert_eq!(
            base.and_then(|b| b.hotkey),
            None,
            "a shortcut opens one workspace"
        );
        // Nothing on screen: a workspace saved over keeps its own zones; a new one starts afresh.
        assert_eq!(saved_over(Some(&coding), None), Some(coding.clone()));
        assert_eq!(saved_over(None, None), None);
    }

    #[test]
    fn no_saved_workspace_gets_the_id_of_what_arrange_put_on_screen() {
        let taken = vec![WorkspaceId("arranged".into())];
        for name in ["~arranged", "Arranged", "~", "", " ~arranged "] {
            assert!(!workspace_id(name, &[]).is_arranged(), "{name:?}");
            assert!(!workspace_id(name, &taken).is_arranged(), "{name:?}");
        }
        assert!(WorkspaceId::arranged().is_arranged());
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
    fn a_fixed_window_over_two_zones_stays_in_the_zone_it_was_put_in() {
        // A 1280 x 720 game in the left third covers the middle third too: remembered after a
        // move by hand, it stays in the zone it was put in, which still holds it.
        let t = fixtures::single_1080p_100();
        let thirds: Vec<WindowFacts> = [0, 640, 1280]
            .into_iter()
            .enumerate()
            .map(|(i, x)| win(i as u64 + 1, "app.exe", "App", Rect::new(x, 0, x + 640, 1032)))
            .collect();
        let seen: Vec<Seen> = thirds.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
        assert_eq!(w.layout.zones.len(), 3);
        let r = Resolver::new(&w.layout, &t, style());
        let first = w.layout.slots[0].clone();
        let Some(Target::Frame(area)) = r.target(&first.place) else {
            unreachable!()
        };
        let game = WindowFacts {
            resizable: false,
            frame_rect: crate::place::fit_size(area, Size::new(1280, 720), t.primary().work_area),
            ..thirds[0].clone()
        };
        let after = remember(
            &w,
            &[Present {
                slot: first.id.clone(),
                window: &game,
            }],
            &t,
            style(),
        );
        assert_eq!(after.layout.slot(&first.id).map(|s| &s.place), Some(&first.place));
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
    fn a_window_moved_on_a_stand_in_monitor_keeps_the_zones_there() {
        let saved_on = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(
            WorkspaceId("w".into()),
            "W".into(),
            &seen,
            &saved_on,
            style(),
            None,
        );
        // Another desk: the 1080p monitor is not there; the 4K primary stands in for it.
        let now = fixtures::uhd_200_plus_1080p_100();
        let stand_in = now.primary().bounds;
        let zones_on_stand_in = |layout: &Layout| {
            let r = Resolver::new(layout, &now, style());
            layout.zones.iter().all(|z| {
                r.zone_rect(&z.id)
                    .is_some_and(|(rect, _)| stand_in.contains_rect(rect))
            })
        };
        assert!(zones_on_stand_in(&w.layout));
        let floating = WindowFacts {
            frame_rect: Rect::new(200, 200, 1400, 1000),
            ..ws[2].clone()
        };
        let after = remember(
            &w,
            &[Present {
                slot: w.layout.slots[2].id.clone(),
                window: &floating,
            }],
            &now,
            style(),
        );
        assert_eq!(after.layout.problems(), vec![]);
        assert!(
            zones_on_stand_in(&after.layout),
            "the zones stay on the monitor they show on"
        );
        assert_eq!(
            after.layout.monitors.len(),
            1,
            "the stand-in is not remembered as a monitor of its own"
        );
        let Place::Free { monitor: 0, area } = after.layout.slots[0].place else {
            unreachable!("{:?}", after.layout.slots[0].place)
        };
        assert_eq!(
            area.to_rect(now.primary().work_area),
            Rect::new(200, 200, 1400, 1000)
        );
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
        assert_eq!(plan.moves[0].target, Target::Frame(Rect::new(0, 28, 960, 1032)));
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
    fn a_window_in_full_screen_is_left_where_it_is_and_not_started_again() {
        // Review of job 0078: the workspace's browser plays a video in full screen. Opening the
        // workspace leaves it in full screen, and does not start a second browser; leaving a
        // workspace does not minimise its video either.
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let all: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &all, &t, style(), None);
        let video = WindowFacts {
            full_screen: true,
            caption: false,
            resizable: false,
            frame_rect: Rect::new(0, 0, 1920, 1080),
            ..ws[1].clone()
        };
        let plan = plan_switch(
            &w,
            &[&ws[0], &video, &ws[2]],
            &[],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert!(plan.assigned.iter().any(|(_, h)| *h == video.handle), "{plan:?}");
        assert!(plan.launch.is_empty() && plan.missing.is_empty(), "{plan:?}");
        assert!(!plan.moves.iter().any(|m| m.window == video.handle), "{plan:?}");
        assert!(!plan.raise.contains(&video.handle) && plan.focus != Some(video.handle));
        // Leaving for a workspace without the browser.
        let other = capture(
            WorkspaceId("o".into()),
            "O".into(),
            &[seen(&ws[0], "App")],
            &t,
            style(),
            None,
        );
        let plan = plan_switch(
            &other,
            &[&ws[0], &video],
            &[video.handle],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert!(plan.minimize.is_empty(), "{plan:?}");
    }

    #[test]
    fn only_windows_on_screen_are_minimised() {
        let t = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
        let mail = win(9, "outlook.exe", "Inbox", Rect::new(0, 0, 1920, 1032));
        let elsewhere = WindowFacts {
            cloak: crate::window::Cloak::ByShell,
            ..win(8, "teams.exe", "Chat", Rect::new(0, 0, 900, 900))
        };
        // The previous workspace also had a chat app now closed to the tray (not in the list).
        let tray = WindowHandle(7);
        let plan = plan_switch(
            &w,
            &[&mail, &elsewhere],
            &[mail.handle, elsewhere.handle, tray],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert_eq!(plan.minimize, vec![mail.handle]);
    }

    #[test]
    fn opening_a_workspace_minimises_the_other_windows_on_its_monitors() {
        // The owner, opening "Coding" from Home: a game launcher and the Riot client stayed on
        // top of it ("here's what happens when i click on code wtf"; his log: moved=3
        // minimized=0), as only the windows of a workspace left were minimised. Every other
        // window on a monitor the workspace is on goes to the taskbar; one on its other monitor
        // stays, as does one in full screen, and a window of the workspace left goes wherever
        // it is.
        let t = fixtures::uhd_200_plus_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &t, style(), None);
        let launcher = win(11, "launcher.exe", "Launcher", Rect::new(400, 300, 1600, 1000));
        let riot = win(
            12,
            "riotclientux.exe",
            "Riot Client",
            Rect::new(800, 200, 2000, 900),
        );
        let side = win(13, "spotify.exe", "Spotify", Rect::new(4000, 600, 5000, 1400));
        let left = win(14, "outlook.exe", "Inbox", Rect::new(4100, 700, 5200, 1500));
        let video = WindowFacts {
            full_screen: true,
            ..win(15, "vlc.exe", "Film", Rect::new(0, 0, 3840, 2160))
        };
        let plan = plan_switch(
            &w,
            &[&launcher, &riot, &side, &left, &video, &ws[0], &ws[1], &ws[2]],
            &[left.handle],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert_eq!(plan.moves.len(), 3);
        assert_eq!(plan.minimize, vec![launcher.handle, riot.handle, left.handle]);
        // Without the setting, nothing is minimised.
        let quiet = SwitchSettings {
            minimize_others: false,
            launch_missing: true,
        };
        let plan = plan_switch(&w, &[&launcher, &riot, &ws[0]], &[], &t, style(), quiet);
        assert!(plan.minimize.is_empty());
    }

    #[test]
    fn one_rule_for_being_in_a_zone() {
        let zone = Rect::new(1500, 0, 1920, 1032);
        let s = style();
        let area = s.window_area(zone, 96);
        assert!(holds(zone, 96, area, s), "exactly in place");
        // A browser that will not go below 502 px, fitted against the screen edge.
        let wide = Rect::new(1912 - 502, area.top, 1912, area.bottom);
        assert!(holds(zone, 96, wide, s), "an app that kept its own size");
        assert!(
            !holds(zone, 96, Rect::new(1600, 300, 1800, 500), s),
            "a small window floating over it"
        );
        assert!(
            !holds(zone, 96, Rect::new(900, 0, 1920, 1032), s),
            "a window spanning two zones"
        );
        // Hiding tags moves the window area by the band: windows placed before still count.
        assert!(holds(zone, 96, area, s.without_tags()));
    }

    #[test]
    fn a_window_of_a_fixed_size_sits_in_its_zone_at_its_own_size() {
        // A game in a 640x480 window that cannot be resized, put in the middle of a half-screen
        // zone (job 0076: the owner wants games and idle clickers in zones too).
        let zone = Rect::new(960, 0, 1920, 1032);
        let s = style();
        let area = s.window_area(zone, 96);
        let work = Rect::new(0, 0, 1920, 1032);
        // Where Mosaïk puts it: centred across, at the top.
        let game = crate::place::fit_size(area, Size::new(640, 480), work);
        assert_eq!(game.top, area.top);
        assert!(holds_window(zone, 96, game, s, false), "a game Mosaïk put there");
        // Where earlier versions put it, in the middle of the zone: still in it.
        let middle = Rect::from_origin_size(
            area.left + (area.width() - 640) / 2,
            area.top + (area.height() - 480) / 2,
            640,
            480,
        );
        assert!(holds_window(zone, 96, middle, s, false));
        // Wider than a third and short: over the next zone across, at the top.
        let thirds = Rect::new(0, 0, 640, 1032);
        let short = crate::place::fit_size(s.window_area(thirds, 96), Size::new(1280, 300), work);
        assert!(holds_window(thirds, 96, short, s, false), "{short:?}");
        assert!(
            !holds_window(zone, 96, game, s, true),
            "a window that could fill the zone is only floating over it"
        );
        // Larger than the zone: fitted on screen, most of it over the zone.
        let wide = Rect::new(1912 - 1100, 100, 1912, 800);
        assert!(holds_window(zone, 96, wide, s, false));
        // Spanning two zones: not in either.
        let across = Rect::from_origin_size(700, 300, 640, 480);
        assert!(!holds_window(zone, 96, across, s, false));
        assert_eq!(
            zone_holding(game, false, &[(ZoneId::new("z"), zone, 96)], s),
            Some(ZoneId::new("z"))
        );
    }

    #[test]
    fn a_fixed_window_larger_than_its_zone_is_in_it_where_mosaik_puts_it() {
        // Review of job 0078: 1280 x 720 in a right-third zone, and 1600 x 900 in a half, hang
        // over the next zone where `fit_size` puts them, and were taken for floating windows.
        let work = Rect::new(0, 0, 1920, 1032);
        let s = style();
        let thirds = [
            (ZoneId::new("left"), Rect::new(0, 0, 640, 1032), 96),
            (ZoneId::new("middle"), Rect::new(640, 0, 1280, 1032), 96),
            (ZoneId::new("right"), Rect::new(1280, 0, 1920, 1032), 96),
        ];
        let halves = [
            (ZoneId::new("left"), Rect::new(0, 0, 960, 1032), 96),
            (ZoneId::new("right"), Rect::new(960, 0, 1920, 1032), 96),
        ];
        for (zones, size) in [
            (&thirds[..], Size::new(1280, 720)),
            (&halves[..], Size::new(1600, 900)),
            (&halves[..], Size::new(1000, 1000)),
        ] {
            for (id, zone, _) in zones {
                let frame = crate::place::fit_size(s.window_area(*zone, 96), size, work);
                assert!(
                    holds_window(*zone, 96, frame, s, false),
                    "{size:?} at {frame:?} in {id:?}"
                );
                // It may cover a neighbouring zone as well: the zone it was put in is kept then
                // (`remember`, and the engine's `zone_of`).
                assert!(
                    zone_holding(frame, false, zones, s).is_some(),
                    "{size:?} at {frame:?}"
                );
            }
        }
    }

    #[test]
    fn a_window_belongs_to_the_zone_whose_middle_is_nearest() {
        // Zones that overlap (review of job 0078): a window filling the larger one belongs to it,
        // not to the smaller one it also covers mostly.
        let s = style();
        let large = (ZoneId::new("large"), Rect::new(0, 0, 1000, 1000), 96);
        let small = (ZoneId::new("small"), Rect::new(0, 0, 900, 900), 96);
        let zones = [large.clone(), small.clone()];
        for (zone, id) in [(large.1, &large.0), (small.1, &small.0)] {
            let filling = s.window_area(zone, 96);
            assert_eq!(zone_holding(filling, true, &zones, s).as_ref(), Some(id));
        }
    }

    #[test]
    fn refit_places_the_same_windows_on_new_monitors() {
        let one = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &one, style(), None);
        let minimized = WindowFacts {
            minimized: true,
            ..ws[2].clone()
        };
        let windows = [
            (w.layout.slots[0].id.clone(), &ws[0]),
            (w.layout.slots[1].id.clone(), &ws[1]),
            (w.layout.slots[2].id.clone(), &minimized),
        ];
        for t in fixtures::all() {
            let moves = plan_refit(&w, &windows, &t, style(), None);
            assert_eq!(moves.len(), 2, "the minimised window stays put");
            for m in moves {
                let Target::Frame(f) = m.target else {
                    unreachable!("zoned slots resolve to frames")
                };
                assert!(t.is_reachable(f, 32), "{f:?} on {}", t.fingerprint());
            }
        }
    }

    #[test]
    fn refit_shows_a_minimised_window_just_added() {
        // The owner: "i should be able to add claude", minimised as opening a workspace leaves
        // the windows it does not use (job 0146: added to a zone, it stayed minimised). The
        // window added goes to its slot's place; another minimised one stays put.
        let one = fixtures::single_1080p_100();
        let ws = snapped();
        let seen: Vec<Seen> = ws.iter().map(|w| seen(w, "App")).collect();
        let w = capture(WorkspaceId("w".into()), "W".into(), &seen, &one, style(), None);
        let added = WindowFacts {
            minimized: true,
            ..ws[1].clone()
        };
        let other = WindowFacts {
            minimized: true,
            ..ws[2].clone()
        };
        let windows = [
            (w.layout.slots[0].id.clone(), &ws[0]),
            (w.layout.slots[1].id.clone(), &added),
            (w.layout.slots[2].id.clone(), &other),
        ];
        for t in fixtures::all() {
            let moves = plan_refit(&w, &windows, &t, style(), Some(added.handle));
            let moved: Vec<WindowHandle> = moves.iter().map(|m| m.window).collect();
            assert_eq!(moved, vec![ws[0].handle, added.handle], "on {}", t.fingerprint());
        }
    }

    #[test]
    fn tab_groups_show_one_window_and_regroup_after_saving() {
        let t = fixtures::single_1080p_100();
        // An editor in the left half with a second editor as its hidden tab, a browser on the
        // right.
        let ws = [
            win(
                1,
                "code.exe",
                "a - x - Visual Studio Code",
                Rect::new(0, 0, 960, 1032),
            ),
            WindowFacts {
                cloak: crate::window::Cloak::ByMosaik,
                ..win(
                    2,
                    "code.exe",
                    "b - y - Visual Studio Code",
                    Rect::new(0, 0, 960, 1032),
                )
            },
            win(
                3,
                "chrome.exe",
                "Docs - Google Chrome",
                Rect::new(960, 0, 1920, 1032),
            ),
        ];
        let captured = [seen(&ws[0], "Code"), seen(&ws[2], "Chrome")];
        let mut w = capture(WorkspaceId("w".into()), "W".into(), &captured, &t, style(), None);
        let _ = regroup(
            &mut w,
            &[GroupSeen {
                tabs: vec![TabSeen::Captured(0), TabSeen::Hidden(seen(&ws[1], "Code"))],
                visible: 0,
            }],
        );
        assert_eq!(w.layout.problems(), vec![]);
        assert_eq!(w.layout.zones.len(), 2, "a hidden tab makes no zone of its own");
        assert_eq!(w.layout.tab_groups.len(), 1);
        let g = &w.layout.tab_groups[0];
        assert_eq!(g.tabs.len(), 2);
        assert_eq!(g.active, w.layout.slots[0].id);
        assert!(matches!(g.place, Place::Zone { .. }));
        assert_eq!(w.layout.slots.len(), 3);

        let plan = plan_switch(
            &w,
            &[&ws[0], &ws[1], &ws[2]],
            &[],
            &t,
            style(),
            SwitchSettings::default(),
        );
        assert_eq!(plan.assigned.len(), 3, "the hidden tab belongs to the workspace");
        assert_eq!(plan.tabs.len(), 1);
        assert_eq!(plan.tabs[0].active, WindowHandle(1));
        assert_eq!(
            plan.tabs[0].background().collect::<Vec<_>>(),
            vec![WindowHandle(2)]
        );
        assert_eq!(
            plan.moves.iter().map(|m| m.window).collect::<Vec<_>>(),
            vec![WindowHandle(1), WindowHandle(3)],
            "a background tab moves when it is shown, not before"
        );
        assert!(
            !plan.raise.contains(&WindowHandle(2)),
            "a background tab is not raised"
        );
        assert_eq!(plan.focus, Some(WindowHandle(1)));
        // The visible tab sits below a band with room for the strip, tags or not.
        let no_tags = style().without_tags();
        let plan = plan_switch(&w, &[&ws[0], &ws[2]], &[], &t, no_tags, SwitchSettings::default());
        let code = plan.moves.iter().find(|m| m.window == WindowHandle(1)).unwrap();
        let browser = plan.moves.iter().find(|m| m.window == WindowHandle(3)).unwrap();
        let (Target::Frame(code), Target::Frame(browser)) = (&code.target, &browser.target) else {
            panic!("both go to zones");
        };
        assert_eq!(code.top - browser.top, style().tag_band);
        // The visible tab's window is closed: the other one shows.
        let plan = plan_switch(&w, &[&ws[1], &ws[2]], &[], &t, style(), SwitchSettings::default());
        assert_eq!(plan.tabs[0].active, WindowHandle(2));
        assert_eq!(plan.focus, Some(WindowHandle(2)));
    }

    #[test]
    fn regroup_puts_the_group_where_its_visible_tab_is() {
        let t = fixtures::single_1080p_100();
        // X was the group's first tab; the user showed Y and dragged it to the right half. X is
        // hidden where the group used to be.
        let x = WindowFacts {
            cloak: crate::window::Cloak::ByMosaik,
            ..win(1, "code.exe", "X", Rect::new(0, 32, 956, 1028))
        };
        let y = win(2, "notepad.exe", "Y", Rect::new(964, 32, 1916, 1028));
        let captured = [seen(&y, "Notepad")];
        let mut w = capture(WorkspaceId("w".into()), "W".into(), &captured, &t, style(), None);
        let _ = regroup(
            &mut w,
            &[GroupSeen {
                tabs: vec![TabSeen::Hidden(seen(&x, "Code")), TabSeen::Captured(0)],
                visible: 1,
            }],
        );
        assert_eq!(w.layout.problems(), vec![]);
        let g = &w.layout.tab_groups[0];
        assert_eq!(
            g.tabs,
            vec![w.layout.slots[1].id.clone(), w.layout.slots[0].id.clone()]
        );
        assert_eq!(g.active, w.layout.slots[0].id, "Y shows");
        let plan = plan_switch(&w, &[&x, &y], &[], &t, style(), SwitchSettings::default());
        assert_eq!(plan.moves.len(), 1);
        assert!(
            matches!(plan.moves[0].target, Target::Frame(r) if r.left > 960),
            "the group stays on the right, where Y is: {:?}",
            plan.moves[0].target
        );
        // A group whose visible tab was not captured, or of one tab, is left out.
        let mut w2 = capture(WorkspaceId("w".into()), "W".into(), &captured, &t, style(), None);
        let _ = regroup(
            &mut w2,
            &[
                GroupSeen {
                    tabs: vec![TabSeen::Hidden(seen(&x, "Code")), TabSeen::Captured(0)],
                    visible: 0,
                },
                GroupSeen {
                    tabs: vec![TabSeen::Captured(0), TabSeen::Captured(0)],
                    visible: 0,
                },
            ],
        );
        assert!(w2.layout.tab_groups.is_empty());
        assert_eq!(w2.layout.slots.len(), 1);
        assert_eq!(w2.layout.problems(), vec![]);
    }

    #[test]
    fn remembering_a_tab_keeps_its_title_and_moves_its_group() {
        let t = fixtures::single_1080p_100();
        let a = win(
            1,
            "code.exe",
            "a - Visual Studio Code",
            Rect::new(0, 32, 956, 1028),
        );
        let b = WindowFacts {
            cloak: crate::window::Cloak::ByMosaik,
            ..win(
                2,
                "code.exe",
                "b - Visual Studio Code",
                Rect::new(0, 32, 956, 1028),
            )
        };
        let captured = [seen(&a, "Code")];
        let mut w = capture(WorkspaceId("w".into()), "W".into(), &captured, &t, style(), None);
        let _ = regroup(
            &mut w,
            &[GroupSeen {
                tabs: vec![TabSeen::Captured(0), TabSeen::Hidden(seen(&b, "Code"))],
                visible: 0,
            }],
        );
        let (sa, sb) = (w.layout.slots[0].id.clone(), w.layout.slots[1].id.clone());
        // The visible tab, renamed, is dragged to the right half by hand.
        let moved = win(
            1,
            "code.exe",
            "c - Visual Studio Code",
            Rect::new(1000, 100, 1800, 900),
        );
        let after = remember(
            &w,
            &[Present {
                slot: sa.clone(),
                window: &moved,
            }],
            &t,
            style(),
        );
        assert_eq!(after.layout.problems(), vec![]);
        assert_eq!(
            after.layout.slot(&sa).unwrap().app.title,
            Some(TitleMatch::Like("c - Visual Studio Code".into()))
        );
        assert!(matches!(after.layout.tab_groups[0].place, Place::Free { .. }));
        // A hidden tab says nothing about where its group is.
        let after2 = remember(&after, &[Present { slot: sb, window: &b }], &t, style());
        assert_eq!(
            after2.layout.tab_groups[0].place,
            after.layout.tab_groups[0].place
        );
    }

    #[test]
    fn ids_and_names() {
        assert!(same_name("  Été ", "ÉTÉ"));
        assert!(!same_name("Été", "Ete"));
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
}
