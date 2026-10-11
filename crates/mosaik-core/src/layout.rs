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

impl Layout {
    pub fn zone(&self, id: &ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|z| &z.id == id)
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
        for g in &self.tab_groups {
            let place_ok = match &g.place {
                Place::Tab { .. } => false,
                Place::Zone { zone } => self.zone(zone).is_some(),
                Place::Free { monitor, .. } | Place::Maximized { monitor } => monitor_ok(*monitor),
            };
            if !place_ok || g.tabs.is_empty() || !g.tabs.contains(&g.active) {
                out.push(LayoutProblem::TabGroupBroken(g.id.clone()));
            }
            for t in &g.tabs {
                let points_back = self
                    .slot(t)
                    .is_some_and(|s| matches!(&s.place, Place::Tab { group } if *group == g.id));
                if !points_back {
                    out.push(LayoutProblem::TabGroupMismatch(t.clone()));
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutProblem {
    DuplicateId(String),
    ZoneMonitorMissing(ZoneId),
    SlotZoneMissing(SlotId),
    SlotMonitorMissing(SlotId),
    TabGroupMismatch(SlotId),
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
                    app: AppMatch::exe("code.exe"),
                    launch: None,
                    place: Place::Tab {
                        group: TabGroupId("g1".into()),
                    },
                },
                Slot {
                    id: SlotId("s2".into()),
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
}
