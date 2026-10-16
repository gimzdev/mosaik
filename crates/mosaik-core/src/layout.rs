//! Workspaces and their layouts: the zones, the windows that belong to the workspace (slots) and
//! how those windows are grouped into tabs.

use serde::{Deserialize, Serialize};

use crate::geometry::RatioRect;
use crate::hotkey::Hotkey;
use crate::matching::AppMatch;
use crate::topology::MonitorRef;
use crate::zone::{Zone, ZoneId};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(pub String);

/// The id of what Arrange put on screen, which is not saved (ADR 0030). `workspace_id` never
/// makes it (it keeps letters, digits and dashes); a file edited by hand that has it gives that
/// workspace another when it is read.
const ARRANGED: &str = "~arranged";

impl WorkspaceId {
    /// What Arrange put on screen: its zones, kept for tags, snapping and tabs until a workspace
    /// is opened or saved.
    pub fn arranged() -> Self {
        Self(ARRANGED.to_owned())
    }

    pub fn is_arranged(&self) -> bool {
        self.0 == ARRANGED
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlotId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabGroupId(pub String);

/// A project's workspace: a name, an optional hotkey of its own, and its layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    /// Optional global hotkey that switches straight to this workspace. None by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<Hotkey>,
    pub layout: Layout,
}

/// Everything a workspace puts back: zones, windows and tabs, plus the monitors they were
/// arranged on (the fingerprint used to re-fit them to whatever is connected now).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub monitors: Vec<MonitorRef>,
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub tab_groups: Vec<TabGroup>,
}

/// One window that belongs to the workspace: how to recognise it, how to start it if it is not
/// open, and where it goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: SlotId,
    /// What to call its app in lists ("Visual Studio Code"), from when its window joined
    /// (older workspaces have none: [`Slot::name`] makes one up).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub app: AppMatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<Launch>,
    pub place: Place,
}

/// Where a window goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Place {
    /// Inside a zone (below its tag band).
    Zone { zone: ZoneId },
    /// Somewhere of its own, as a fraction of a monitor's work area.
    Free { monitor: usize, area: RatioRect },
    /// Maximised on a monitor.
    Maximized { monitor: usize },
    /// A tab of a tab group; the group decides where it goes.
    Tab { group: TabGroupId },
}

/// Windows that share one rectangle, one visible at a time, with a tab strip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: TabGroupId,
    /// Tab order, left to right.
    pub tabs: Vec<SlotId>,
    pub active: SlotId,
    /// Where the group sits. Never `Place::Tab`.
    pub place: Place,
}

/// How to start a window's app when the workspace opens and it is missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Launch {
    /// A program with arguments, like a shortcut.
    Program {
        path: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        working_dir: Option<String>,
    },
    /// A packaged (Store) app, started by its Application User Model ID.
    Packaged { aumid: String },
}

impl Slot {
    /// What to call its app: its label, else a name made from its window or its program.
    pub fn name(&self) -> String {
        if let Some(label) = self.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            return label.to_owned();
        }
        // A packaged app's id starts with its publisher's and its own name: "Microsoft.
        // WindowsCalculator_8wekyb3d8bbwe!App" is "Windows Calculator".
        let packaged = || {
            self.app
                .aumid
                .as_deref()
                .and_then(|a| a.split(['_', '!']).next())
                .and_then(|family| family.rsplit('.').next())
                .filter(|n| !n.is_empty())
                .map(spaced)
        };
        // Store apps run in a frame of the frame host, whose title is the app's name.
        let frame_host = self
            .app
            .exe
            .as_deref()
            .is_some_and(|e| e.eq_ignore_ascii_case("applicationframehost.exe"))
            || self
                .app
                .class
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case("ApplicationFrameWindow"));
        if frame_host {
            return self
                .app
                .title
                .as_ref()
                .map(|t| t.text().trim().to_owned())
                .filter(|t| !t.is_empty())
                .or_else(packaged)
                .unwrap_or_else(|| "App".to_owned());
        }
        if let Some(exe) = self.app.exe.as_deref() {
            if let Some(known) = crate::window::known_app_name(exe) {
                return known.to_owned();
            }
            let stem = exe.trim_end_matches(".exe");
            let mut chars = stem.chars();
            if let Some(first) = chars.next() {
                return first.to_uppercase().chain(chars).collect();
            }
        }
        packaged().unwrap_or_else(|| "App".to_owned())
    }
}

/// "WindowsCalculator" as "Windows Calculator": a space before each capital that follows a
/// small letter.
fn spaced(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_uppercase() && previous.is_some_and(char::is_lowercase) {
            out.push(' ');
        }
        out.push(c);
        previous = Some(c);
    }
    out
}

/// Which of two zones, each a monitor and an area on it, is read first: monitor by monitor, then
/// row by row from the top, then from the left. A zone whose top is within 2% of another's is
/// in its row, so zones a few pixels apart still read across.
pub fn reading_order(a: (usize, RatioRect), b: (usize, RatioRect)) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then_with(|| {
        if (a.1.top() - b.1.top()).abs() < 0.02 {
            a.1.left().total_cmp(&b.1.left())
        } else {
            a.1.top().total_cmp(&b.1.top())
        }
    })
}

impl Layout {
    pub fn zone(&self, id: &ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|z| &z.id == id)
    }

    /// Names the zones 1, 2, 3... in reading order: monitor by monitor, then row by row from the
    /// top, then from the left (a zone whose top is within 2% of another's is in its row). A
    /// zone's name says where it is, never what it holds (the owner: "names like assistant code
    /// zone etc are absolute bullshit they should be 1 2 3 4 5 6"). Its id and colour stay.
    pub fn number_zones(&mut self) {
        let mut order: Vec<usize> = (0..self.zones.len()).collect();
        order.sort_by(|&a, &b| {
            let (za, zb) = (&self.zones[a], &self.zones[b]);
            reading_order((za.monitor, za.area), (zb.monitor, zb.area))
        });
        for (n, i) in order.into_iter().enumerate() {
            self.zones[i].name = (n + 1).to_string();
        }
    }

    pub fn slot(&self, id: &SlotId) -> Option<&Slot> {
        self.slots.iter().find(|s| &s.id == id)
    }

    pub fn tab_group(&self, id: &TabGroupId) -> Option<&TabGroup> {
        self.tab_groups.iter().find(|g| &g.id == id)
    }

    /// The place that actually applies to a slot: a tab's group place, otherwise its own.
    pub fn effective_place<'a>(&'a self, slot: &'a Slot) -> Option<&'a Place> {
        match &slot.place {
            Place::Tab { group } => self.tab_group(group).map(|g| &g.place),
            other => Some(other),
        }
    }

    /// Problems that would make the layout misbehave: dangling references, monitor indices out
    /// of range, tab groups that do not agree with their slots. Empty when the layout is sound.
    pub fn problems(&self) -> Vec<LayoutProblem> {
        let mut out = Vec::new();
        let monitor_ok = |m: usize| m < self.monitors.len();
        for z in &self.zones {
            if !monitor_ok(z.monitor) {
                out.push(LayoutProblem::ZoneMonitorMissing(z.id.clone()));
            }
        }
        let mut ids = std::collections::HashSet::new();
        for z in &self.zones {
            if !ids.insert(z.id.0.as_str()) {
                out.push(LayoutProblem::DuplicateId(z.id.0.clone()));
            }
        }
        let mut slot_ids = std::collections::HashSet::new();
        for s in &self.slots {
            if !slot_ids.insert(s.id.0.as_str()) {
                out.push(LayoutProblem::DuplicateId(s.id.0.clone()));
            }
            match &s.place {
                Place::Zone { zone } if self.zone(zone).is_none() => {
                    out.push(LayoutProblem::SlotZoneMissing(s.id.clone()));
                }
                Place::Free { monitor, .. } | Place::Maximized { monitor } if !monitor_ok(*monitor) => {
                    out.push(LayoutProblem::SlotMonitorMissing(s.id.clone()));
                }
                Place::Tab { group } => match self.tab_group(group) {
                    Some(g) if g.tabs.contains(&s.id) => {}
                    _ => out.push(LayoutProblem::TabGroupMismatch(s.id.clone())),
                },
                _ => {}
            }
        }
        let mut group_ids = std::collections::HashSet::new();
        // Which group lists each slot: a slot is a tab of one group at most.
        let mut listed: std::collections::HashMap<&SlotId, &TabGroupId> = std::collections::HashMap::new();
        for g in &self.tab_groups {
            if !group_ids.insert(g.id.0.as_str()) {
                out.push(LayoutProblem::DuplicateId(g.id.0.clone()));
            }
            let distinct: std::collections::HashSet<&SlotId> = g.tabs.iter().collect();
            if !self.place_usable(&g.place)
                || g.tabs.len() < 2
                || distinct.len() != g.tabs.len()
                || !g.tabs.contains(&g.active)
            {
                out.push(LayoutProblem::TabGroupBroken(g.id.clone()));
            }
            for t in &g.tabs {
                let points_back = self
                    .slot(t)
                    .is_some_and(|s| matches!(&s.place, Place::Tab { group } if *group == g.id));
                let elsewhere = listed.insert(t, &g.id).is_some_and(|other| *other != g.id);
                if !points_back || elsewhere {
                    out.push(LayoutProblem::TabGroupMismatch(t.clone()));
                }
            }
        }
        out
    }

    /// A place a window can go to: not a tab, and every reference in it exists.
    fn place_usable(&self, place: &Place) -> bool {
        match place {
            Place::Tab { .. } => false,
            Place::Zone { zone } => self.zone(zone).is_some(),
            Place::Free { monitor, .. } | Place::Maximized { monitor } => *monitor < self.monitors.len(),
        }
    }

    /// Mends the tab groups [`Layout::problems`] finds damaged (a file edited by hand, or written
    /// by a faulty version): each one ends, and its windows become windows of their own in the
    /// group's place, or maximised when that place is damaged too. Other problems only make
    /// Mosaïk skip a zone or a window, so they are left for the user to see. Returns how many
    /// groups ended.
    pub fn repair_tabs(&mut self) -> usize {
        let mut broken: std::collections::HashSet<TabGroupId> = std::collections::HashSet::new();
        for p in self.problems() {
            match p {
                LayoutProblem::TabGroupBroken(g) => {
                    broken.insert(g);
                }
                LayoutProblem::TabGroupMismatch(slot) => {
                    for g in self.tab_groups.iter().filter(|g| g.tabs.contains(&slot)) {
                        broken.insert(g.id.clone());
                    }
                    if let Some(Place::Tab { group }) = self.slot(&slot).map(|s| &s.place) {
                        broken.insert(group.clone());
                    }
                }
                LayoutProblem::DuplicateId(id)
                    if self.tab_groups.iter().filter(|g| g.id.0 == id).count() > 1 =>
                {
                    broken.insert(TabGroupId(id));
                }
                _ => {}
            }
        }
        if broken.is_empty() {
            return 0;
        }
        let fallback = Place::Maximized { monitor: 0 };
        let place_of = |layout: &Layout, group: &TabGroupId| -> Place {
            layout
                .tab_groups
                .iter()
                .find(|g| &g.id == group)
                .map(|g| g.place.clone())
                .filter(|p| layout.place_usable(p))
                .unwrap_or_else(|| fallback.clone())
        };
        let places: Vec<(usize, Place)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match &s.place {
                Place::Tab { group } if broken.contains(group) || self.tab_group(group).is_none() => {
                    Some((i, place_of(self, group)))
                }
                _ => None,
            })
            .collect();
        for (i, place) in places {
            self.slots[i].place = place;
        }
        let before = self.tab_groups.len();
        self.tab_groups.retain(|g| !broken.contains(&g.id));
        before - self.tab_groups.len()
    }
}

impl Layout {
    /// The index of the layout's monitor that `monitor` (connected now) is or stands in for,
    /// adding it when it is new (see [`crate::topology::remembered_index`]).
    pub fn monitor_index(
        &mut self,
        monitor: &crate::topology::Monitor,
        now: &crate::topology::Topology,
    ) -> usize {
        crate::topology::remembered_index(&mut self.monitors, monitor, now)
    }
}

/// Tab groups: windows that share one place, one visible at a time.
impl Layout {
    fn group_mut(&mut self, id: &TabGroupId) -> Option<&mut TabGroup> {
        self.tab_groups.iter_mut().find(|g| &g.id == id)
    }

    fn slot_mut(&mut self, id: &SlotId) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| &s.id == id)
    }

    /// The group a slot is a tab of.
    /// Each app's place front to back, as opening the workspace stacks its windows and its
    /// picture lists them (ADR 0032): the order of its slots, a tab of a group where the
    /// group's front-most tab is (whichever tab shows).
    pub fn ranks(&self) -> std::collections::HashMap<SlotId, usize> {
        let own: std::collections::HashMap<&SlotId, usize> =
            self.slots.iter().enumerate().map(|(i, s)| (&s.id, i)).collect();
        self.slots
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let rank = self.group_of(&s.id).map_or(i, |g| {
                    g.tabs
                        .iter()
                        .filter_map(|t| own.get(t).copied())
                        .min()
                        .unwrap_or(i)
                });
                (s.id.clone(), rank)
            })
            .collect()
    }

    pub fn group_of(&self, slot: &SlotId) -> Option<&TabGroup> {
        match &self.slot(slot)?.place {
            Place::Tab { group } => self.tab_group(group),
            _ => None,
        }
    }

    fn new_group_id(&self) -> TabGroupId {
        let taken: std::collections::HashSet<&str> =
            self.tab_groups.iter().map(|g| g.id.0.as_str()).collect();
        TabGroupId(
            (1..)
                .map(|n| format!("g{n}"))
                .find(|id| !taken.contains(id.as_str()))
                .unwrap_or_else(|| "g0".into()),
        )
    }

    /// Makes `slot` a tab next to `onto` (in `onto`'s group, or a new group in `onto`'s place),
    /// just before the tab `before` (at the end when `None`), and the visible one. A slot already
    /// in another group leaves it first. Returns the group, or `None` when either slot is
    /// unknown or they are the same.
    pub fn join_tab(&mut self, onto: &SlotId, slot: &SlotId, before: Option<&SlotId>) -> Option<TabGroupId> {
        if onto == slot || self.slot(slot).is_none() {
            return None;
        }
        let onto_place = self.slot(onto)?.place.clone();
        // A tab of another group leaves it first (taking the target's place for a moment).
        let from = self.group_of(slot).map(|g| g.id.clone());
        let to = self.group_of(onto).map(|g| g.id.clone());
        if from.is_some() && from != to {
            let place = keep_place(&onto_place, self)?;
            self.leave_tab(slot, place);
        }
        let group = match onto_place {
            Place::Tab { group } => group,
            place => {
                let id = self.new_group_id();
                self.tab_groups.push(TabGroup {
                    id: id.clone(),
                    tabs: vec![onto.clone()],
                    active: onto.clone(),
                    place,
                });
                if let Some(s) = self.slot_mut(onto) {
                    s.place = Place::Tab { group: id.clone() };
                }
                id
            }
        };
        let g = self.group_mut(&group)?;
        g.tabs.retain(|t| t != slot);
        let at = before
            .and_then(|b| g.tabs.iter().position(|t| t == b))
            .unwrap_or(g.tabs.len());
        g.tabs.insert(at, slot.clone());
        g.active = slot.clone();
        if let Some(s) = self.slot_mut(slot) {
            s.place = Place::Tab { group: group.clone() };
        }
        Some(group)
    }

    /// Makes a new tab group of `tabs` (in that order: at least two different slots, none of
    /// them a tab yet) at `place`, showing `active`. Returns its id, or `None` when that would
    /// not be a sound group.
    pub fn make_group(&mut self, tabs: &[SlotId], active: &SlotId, place: Place) -> Option<TabGroupId> {
        let distinct: std::collections::HashSet<&SlotId> = tabs.iter().collect();
        let free = tabs.iter().all(|t| {
            self.slot(t)
                .is_some_and(|s| !matches!(s.place, Place::Tab { .. }))
        });
        if matches!(place, Place::Tab { .. })
            || tabs.len() < 2
            || distinct.len() != tabs.len()
            || !tabs.contains(active)
            || !free
        {
            return None;
        }
        let id = self.new_group_id();
        for t in tabs {
            if let Some(s) = self.slot_mut(t) {
                s.place = Place::Tab { group: id.clone() };
            }
        }
        self.tab_groups.push(TabGroup {
            id: id.clone(),
            tabs: tabs.to_vec(),
            active: active.clone(),
            place,
        });
        Some(id)
    }

    /// Takes `slot` out of its tab group and gives it `place`. When the visible tab leaves, its
    /// right-hand neighbour (or else the left) becomes visible; a group left with one tab ends,
    /// and that window takes the group's place.
    pub fn leave_tab(&mut self, slot: &SlotId, place: Place) -> bool {
        let Some(group) = self.group_of(slot).cloned() else {
            return false;
        };
        let Some(i) = group.tabs.iter().position(|t| t == slot) else {
            return false;
        };
        let mut tabs = group.tabs.clone();
        tabs.remove(i);
        if let Some(s) = self.slot_mut(slot) {
            s.place = place;
        }
        if tabs.len() <= 1 {
            self.tab_groups.retain(|g| g.id != group.id);
            if let Some(last) = tabs.first()
                && let Some(s) = self.slot_mut(last)
            {
                s.place = group.place.clone();
            }
            return true;
        }
        let active = if &group.active == slot {
            tabs[i.min(tabs.len() - 1)].clone()
        } else {
            group.active.clone()
        };
        if let Some(g) = self.group_mut(&group.id) {
            g.tabs = tabs;
            g.active = active;
        }
        true
    }

    /// Moves a tab just before the tab `before` of its group (to the end when `None` or when
    /// `before` is not in the group). Positions are by slot, so tabs whose window is closed (and
    /// so not on the strip) keep their places around it. Returns whether the order changed.
    pub fn move_tab_before(&mut self, slot: &SlotId, before: Option<&SlotId>) -> bool {
        let Some(id) = self.group_of(slot).map(|g| g.id.clone()) else {
            return false;
        };
        let Some(g) = self.group_mut(&id) else {
            return false;
        };
        let Some(from) = g.tabs.iter().position(|t| t == slot) else {
            return false;
        };
        let t = g.tabs.remove(from);
        let at = before
            .filter(|b| *b != slot)
            .and_then(|b| g.tabs.iter().position(|x| x == b))
            .unwrap_or(g.tabs.len());
        g.tabs.insert(at, t);
        at != from
    }

    /// Makes `slot` the visible tab of its group.
    pub fn show_tab(&mut self, slot: &SlotId) -> bool {
        let Some(id) = self.group_of(slot).map(|g| g.id.clone()) else {
            return false;
        };
        match self.group_mut(&id) {
            Some(g) => {
                g.active = slot.clone();
                true
            }
            None => false,
        }
    }

    /// Moves a whole group (its windows follow when they are next shown).
    pub fn move_group(&mut self, group: &TabGroupId, place: Place) {
        if matches!(place, Place::Tab { .. }) {
            return;
        }
        if let Some(g) = self.group_mut(group) {
            g.place = place;
        }
    }
}

/// The place a tab gets while it moves between groups: the target's own place, or its group's.
fn keep_place(onto: &Place, layout: &Layout) -> Option<Place> {
    match onto {
        Place::Tab { group } => layout.tab_group(group).map(|g| g.place.clone()),
        other => Some(other.clone()),
    }
}

/// Something wrong in a saved layout (a file edited by hand, or damaged), in words for the
/// user: what is wrong, not where in the file.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutProblem {
    #[error("two of its parts share the id \u{201c}{0}\u{201d}")]
    DuplicateId(String),
    #[error("a zone is on a monitor the workspace does not know")]
    ZoneMonitorMissing(ZoneId),
    #[error("an app's zone is gone")]
    SlotZoneMissing(SlotId),
    #[error("an app is on a monitor the workspace does not know")]
    SlotMonitorMissing(SlotId),
    #[error("an app and its tab group do not agree")]
    TabGroupMismatch(SlotId),
    #[error("a tab group has tabs that are gone")]
    TabGroupBroken(TabGroupId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::topology::Monitor;
    use crate::zone::ZoneColor;

    fn layout() -> Layout {
        let t = fixtures::single_1080p_100();
        Layout {
            monitors: t.monitors().iter().map(Monitor::to_ref).collect(),
            zones: vec![Zone {
                id: ZoneId::new("z1"),
                name: "Code".into(),
                color: ZoneColor::Lapis,
                monitor: 0,
                area: RatioRect::new(0.0, 0.0, 0.6, 1.0).unwrap(),
            }],
            slots: vec![
                Slot {
                    id: SlotId("s1".into()),
                    label: None,
                    app: AppMatch::exe("code.exe"),
                    launch: None,
                    place: Place::Tab {
                        group: TabGroupId("g1".into()),
                    },
                },
                Slot {
                    id: SlotId("s2".into()),
                    label: None,
                    app: AppMatch::exe("windowsterminal.exe"),
                    launch: None,
                    place: Place::Tab {
                        group: TabGroupId("g1".into()),
                    },
                },
            ],
            tab_groups: vec![TabGroup {
                id: TabGroupId("g1".into()),
                tabs: vec![SlotId("s1".into()), SlotId("s2".into())],
                active: SlotId("s2".into()),
                place: Place::Zone {
                    zone: ZoneId::new("z1"),
                },
            }],
        }
    }

    #[test]
    fn zones_are_numbered_in_reading_order() {
        // Quarters given bottom-right first, a zone on the second monitor among them: numbered
        // monitor by monitor, row by row, left to right, whatever they were called.
        let zone = |id: &str, name: &str, monitor: usize, l: f64, t: f64, r: f64, b: f64| Zone {
            id: ZoneId::new(id),
            name: name.into(),
            color: ZoneColor::Lapis,
            monitor,
            area: RatioRect::new(l, t, r, b).unwrap(),
        };
        let mut l = layout();
        l.zones = vec![
            zone("d", "Assistant", 0, 0.5, 0.5, 1.0, 1.0),
            zone("e", "Zone", 1, 0.0, 0.0, 1.0, 1.0),
            zone("c", "Terminal", 0, 0.0, 0.5, 0.5, 1.0),
            zone("b", "Browser", 0, 0.5, 0.01, 1.0, 0.5),
            zone("a", "Code", 0, 0.0, 0.0, 0.5, 0.5),
        ];
        l.number_zones();
        let named: Vec<(&str, &str)> = l
            .zones
            .iter()
            .map(|z| (z.id.0.as_str(), z.name.as_str()))
            .collect();
        assert_eq!(
            named,
            vec![("d", "4"), ("e", "5"), ("c", "3"), ("b", "2"), ("a", "1")]
        );
    }

    #[test]
    fn sound_layout_has_no_problems() {
        assert_eq!(layout().problems(), vec![]);
        let l = layout();
        let s = l.slot(&SlotId("s1".into())).unwrap();
        assert_eq!(
            l.effective_place(s),
            Some(&Place::Zone {
                zone: ZoneId::new("z1")
            })
        );
    }

    #[test]
    fn dangling_references_are_reported() {
        let mut l = layout();
        l.tab_groups[0].active = SlotId("nope".into());
        l.zones[0].monitor = 3;
        l.slots.push(Slot {
            id: SlotId("s3".into()),
            label: None,
            app: AppMatch::exe("x.exe"),
            launch: None,
            place: Place::Zone {
                zone: ZoneId::new("gone"),
            },
        });
        let p = l.problems();
        assert!(p.contains(&LayoutProblem::TabGroupBroken(TabGroupId("g1".into()))));
        assert!(p.contains(&LayoutProblem::ZoneMonitorMissing(ZoneId::new("z1"))));
        assert!(p.contains(&LayoutProblem::SlotZoneMissing(SlotId("s3".into()))));
    }

    fn three_in_zones() -> Layout {
        let mut l = layout();
        // Start from plain slots: s1 and s2 in z1, s3 in z1 too.
        l.tab_groups.clear();
        for s in &mut l.slots {
            s.place = Place::Zone {
                zone: ZoneId::new("z1"),
            };
        }
        l.slots.push(Slot {
            id: SlotId("s3".into()),
            label: None,
            app: AppMatch::exe("chrome.exe"),
            launch: None,
            place: Place::Free {
                monitor: 0,
                area: RatioRect::new(0.6, 0.0, 1.0, 1.0).unwrap(),
            },
        });
        l
    }

    fn sid(s: &str) -> SlotId {
        SlotId(s.into())
    }

    #[test]
    fn dropping_a_window_on_another_makes_a_group_in_its_place() {
        let mut l = three_in_zones();
        let g = l.join_tab(&sid("s1"), &sid("s3"), None).unwrap();
        assert_eq!(l.problems(), vec![]);
        let group = l.tab_group(&g).unwrap();
        assert_eq!(group.tabs, vec![sid("s1"), sid("s3")]);
        assert_eq!(group.active, sid("s3"), "the dropped window shows");
        assert_eq!(
            group.place,
            Place::Zone {
                zone: ZoneId::new("z1")
            }
        );
        // A third joins at the front.
        assert_eq!(
            l.join_tab(&sid("s3"), &sid("s2"), Some(&sid("s1"))),
            Some(g.clone())
        );
        assert_eq!(
            l.tab_group(&g).unwrap().tabs,
            vec![sid("s2"), sid("s1"), sid("s3")]
        );
        assert_eq!(l.problems(), vec![]);
        assert_eq!(l.join_tab(&sid("s1"), &sid("s1"), None), None);
    }

    #[test]
    fn pulling_tabs_out_ends_the_group_at_one() {
        let mut l = three_in_zones();
        let g = l.join_tab(&sid("s1"), &sid("s2"), None).unwrap();
        l.join_tab(&sid("s1"), &sid("s3"), None);
        l.show_tab(&sid("s1"));
        let free = Place::Free {
            monitor: 0,
            area: RatioRect::new(0.1, 0.1, 0.5, 0.5).unwrap(),
        };
        // The visible tab leaves: its right-hand neighbour shows.
        assert!(l.leave_tab(&sid("s1"), free.clone()));
        assert_eq!(l.tab_group(&g).unwrap().active, sid("s2"));
        assert_eq!(l.slot(&sid("s1")).unwrap().place, free);
        assert_eq!(l.problems(), vec![]);
        // One tab left: the group ends and the window keeps the group's place.
        assert!(l.leave_tab(&sid("s2"), free));
        assert!(l.tab_groups.is_empty());
        assert_eq!(
            l.slot(&sid("s3")).unwrap().place,
            Place::Zone {
                zone: ZoneId::new("z1")
            }
        );
        assert_eq!(l.problems(), vec![]);
        assert!(
            !l.leave_tab(&sid("s3"), Place::Maximized { monitor: 0 }),
            "not a tab"
        );
    }

    #[test]
    fn tabs_reorder_and_move_between_groups() {
        let mut l = three_in_zones();
        let g1 = l.join_tab(&sid("s1"), &sid("s2"), None).unwrap();
        l.join_tab(&sid("s1"), &sid("s3"), None);
        assert!(l.move_tab_before(&sid("s3"), Some(&sid("s1"))));
        assert!(!l.move_tab_before(&sid("s3"), Some(&sid("s1"))), "already there");
        assert_eq!(
            l.tab_group(&g1).unwrap().tabs,
            vec![sid("s3"), sid("s1"), sid("s2")]
        );
        // A fourth window, and s2 moves from g1 into a new group with it.
        l.slots.push(Slot {
            id: sid("s4"),
            label: None,
            app: AppMatch::exe("notepad.exe"),
            launch: None,
            place: Place::Maximized { monitor: 0 },
        });
        let g2 = l.join_tab(&sid("s4"), &sid("s2"), None).unwrap();
        assert_ne!(g1, g2);
        assert_eq!(l.tab_group(&g1).unwrap().tabs, vec![sid("s3"), sid("s1")]);
        assert_eq!(l.tab_group(&g2).unwrap().tabs, vec![sid("s4"), sid("s2")]);
        assert_eq!(l.tab_group(&g2).unwrap().place, Place::Maximized { monitor: 0 });
        assert_eq!(l.problems(), vec![]);
        l.move_group(
            &g2,
            Place::Zone {
                zone: ZoneId::new("z1"),
            },
        );
        assert_eq!(l.problems(), vec![]);
    }

    #[test]
    fn json_shape_is_readable() {
        let json = serde_json::to_value(layout()).unwrap();
        assert_eq!(json["slots"][0]["place"]["kind"], "tab");
        assert_eq!(
            json["tab_groups"][0]["place"],
            serde_json::json!({"kind": "zone", "zone": "z1"})
        );
        let back: Layout = serde_json::from_value(json).unwrap();
        assert_eq!(back, layout());
    }

    #[test]
    fn damaged_groups_are_found_and_mended() {
        let g = |id: &str| TabGroupId(id.into());
        let tab = |id: &str| Place::Tab { group: g(id) };
        // A group of one, a slot listed by two groups, a group whose place is a tab (which once
        // made placing loop forever), a duplicate id and a slot pointing at no group.
        let mut l = three_in_zones();
        l.slots.push(Slot {
            id: sid("s4"),
            label: None,
            app: AppMatch::exe("notepad.exe"),
            launch: None,
            place: tab("gone"),
        });
        l.slots[0].place = tab("g1");
        l.slots[1].place = tab("g2");
        l.slots[2].place = tab("g3");
        l.tab_groups = vec![
            TabGroup {
                id: g("g1"),
                tabs: vec![sid("s1")],
                active: sid("s1"),
                place: Place::Zone {
                    zone: ZoneId::new("z1"),
                },
            },
            TabGroup {
                id: g("g2"),
                tabs: vec![sid("s2"), sid("s1")],
                active: sid("s2"),
                place: tab("g2"),
            },
            TabGroup {
                id: g("g3"),
                tabs: vec![sid("s3"), sid("s3")],
                active: sid("s3"),
                place: Place::Free {
                    monitor: 9,
                    area: RatioRect::FULL,
                },
            },
            TabGroup {
                id: g("g3"),
                tabs: vec![sid("s3"), sid("s2")],
                active: sid("s3"),
                place: Place::Maximized { monitor: 0 },
            },
        ];
        let p = l.problems();
        for expected in [
            LayoutProblem::TabGroupBroken(g("g1")),
            LayoutProblem::TabGroupBroken(g("g2")),
            LayoutProblem::TabGroupBroken(g("g3")),
            LayoutProblem::DuplicateId("g3".into()),
            LayoutProblem::TabGroupMismatch(sid("s1")),
            LayoutProblem::TabGroupMismatch(sid("s4")),
        ] {
            assert!(p.contains(&expected), "{expected:?} not in {p:?}");
        }
        // Placing a slot of the self-referencing group ends instead of looping.
        let t = fixtures::single_1080p_100();
        let r = crate::place::Resolver::new(&l, &t, crate::zone::FrameStyle::default());
        assert_eq!(r.target(&tab("g2")), None);

        assert_eq!(l.repair_tabs(), 4);
        assert_eq!(l.problems(), vec![]);
        assert!(l.tab_groups.is_empty());
        // Each window keeps its group's place when that place is sound, else it is maximised.
        assert_eq!(
            l.slot(&sid("s1")).unwrap().place,
            Place::Zone {
                zone: ZoneId::new("z1")
            }
        );
        assert_eq!(l.slot(&sid("s2")).unwrap().place, Place::Maximized { monitor: 0 });
        assert_eq!(l.slot(&sid("s4")).unwrap().place, Place::Maximized { monitor: 0 });
        assert_eq!(layout().clone().repair_tabs(), 0, "a sound layout is left alone");
    }

    #[test]
    fn make_group_refuses_unsound_groups() {
        let mut l = three_in_zones();
        let z1 = Place::Zone {
            zone: ZoneId::new("z1"),
        };
        assert_eq!(
            l.make_group(&[sid("s1")], &sid("s1"), z1.clone()),
            None,
            "one tab"
        );
        assert_eq!(
            l.make_group(&[sid("s1"), sid("s1")], &sid("s1"), z1.clone()),
            None,
            "the same slot twice"
        );
        assert_eq!(
            l.make_group(&[sid("s1"), sid("s2")], &sid("s3"), z1.clone()),
            None,
            "showing a slot that is not a tab"
        );
        assert_eq!(
            l.make_group(&[sid("s1"), sid("nope")], &sid("s1"), z1.clone()),
            None,
            "an unknown slot"
        );
        let g = l
            .make_group(&[sid("s2"), sid("s1")], &sid("s1"), z1.clone())
            .unwrap();
        assert_eq!(l.problems(), vec![]);
        assert_eq!(l.tab_group(&g).unwrap().tabs, vec![sid("s2"), sid("s1")]);
        assert_eq!(
            l.make_group(&[sid("s1"), sid("s3")], &sid("s1"), z1),
            None,
            "a slot that is a tab already"
        );
    }

    #[test]
    fn tabs_move_relative_to_other_tabs() {
        let mut l = three_in_zones();
        let g = l.join_tab(&sid("s1"), &sid("s2"), None).unwrap();
        l.join_tab(&sid("s1"), &sid("s3"), None);
        // s2 is closed, so the strip shows [s1, s3]: dragging s3 before s1 moves it before s1,
        // whatever the closed tab's position.
        assert!(l.move_tab_before(&sid("s3"), Some(&sid("s1"))));
        assert_eq!(
            l.tab_group(&g).unwrap().tabs,
            vec![sid("s3"), sid("s1"), sid("s2")]
        );
        assert!(l.move_tab_before(&sid("s3"), None));
        assert_eq!(
            l.tab_group(&g).unwrap().tabs,
            vec![sid("s1"), sid("s2"), sid("s3")]
        );
        assert!(
            !l.move_tab_before(&sid("s3"), Some(&sid("s3"))),
            "before itself: no change"
        );
        assert!(
            !l.move_tab_before(&sid("s3"), Some(&sid("nope"))),
            "already at the end"
        );
    }

    mod properties {
        use super::*;
        use proptest::prelude::*;

        #[derive(Clone, Debug)]
        enum Op {
            Join(usize, usize, Option<usize>),
            Leave(usize),
            MoveBefore(usize, Option<usize>),
            Show(usize),
            MoveGroup(usize),
        }

        fn op(n: usize) -> impl Strategy<Value = Op> {
            prop_oneof![
                (0..n, 0..n, proptest::option::of(0..n)).prop_map(|(a, b, c)| Op::Join(a, b, c)),
                (0..n).prop_map(Op::Leave),
                (0..n, proptest::option::of(0..n)).prop_map(|(a, b)| Op::MoveBefore(a, b)),
                (0..n).prop_map(Op::Show),
                (0..n).prop_map(Op::MoveGroup),
            ]
        }

        fn slots(n: usize) -> Layout {
            let mut l = layout();
            l.tab_groups.clear();
            l.slots = (0..n)
                .map(|i| Slot {
                    id: SlotId(format!("s{i}")),
                    label: None,
                    app: AppMatch::exe("app.exe"),
                    launch: None,
                    place: Place::Zone {
                        zone: ZoneId::new("z1"),
                    },
                })
                .collect();
            l
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]
            /// Whatever the user does with tabs, the layout stays sound: every group has two
            /// tabs or more, shows one of them, sits in a real place, and each slot is a tab of
            /// one group at most, pointing back at it.
            #[test]
            fn tab_operations_keep_layouts_sound(ops in proptest::collection::vec(op(5), 1..40)) {
                let mut l = slots(5);
                let id = |i: usize| SlotId(format!("s{i}"));
                for o in ops {
                    match o {
                        Op::Join(a, b, before) => {
                            let before = before.map(id);
                            let _ = l.join_tab(&id(a), &id(b), before.as_ref());
                        }
                        Op::Leave(a) => {
                            let _ = l.leave_tab(&id(a), Place::Maximized { monitor: 0 });
                        }
                        Op::MoveBefore(a, b) => {
                            let b = b.map(id);
                            let _ = l.move_tab_before(&id(a), b.as_ref());
                        }
                        Op::Show(a) => {
                            let _ = l.show_tab(&id(a));
                        }
                        Op::MoveGroup(a) => {
                            if let Some(g) = l.group_of(&id(a)).map(|g| g.id.clone()) {
                                l.move_group(&g, Place::Free { monitor: 0, area: RatioRect::FULL });
                            }
                        }
                    }
                    prop_assert_eq!(l.problems(), vec![]);
                    let mut clone = l.clone();
                    prop_assert_eq!(clone.repair_tabs(), 0);
                }
            }
        }
    }

    #[test]
    fn apps_rank_front_to_back_with_a_group_as_far_forward_as_its_front_tab() {
        // ADR 0032: the order a workspace's picture lists a zone's apps in, and opening it
        // stacks them in: its slots' order, a tab group where its front-most tab is.
        let mut l = layout();
        let window = |id: &str| Slot {
            id: SlotId(id.into()),
            label: None,
            app: AppMatch::exe("a.exe"),
            launch: None,
            place: Place::Zone {
                zone: ZoneId::new("z1"),
            },
        };
        // s3, then the group's s1, then s4, then the group's s2.
        l.slots = vec![window("s3"), l.slots[0].clone(), window("s4"), l.slots[1].clone()];
        let ranks = l.ranks();
        let rank = |id: &str| ranks.get(&SlotId(id.into())).copied();
        assert_eq!(
            (rank("s3"), rank("s1"), rank("s2"), rank("s4")),
            (Some(0), Some(1), Some(1), Some(2))
        );
        assert_eq!(rank("gone"), None);
    }
}
