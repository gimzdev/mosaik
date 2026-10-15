//! What opening a workspace would do, without doing it: Home shows it when a workspace is picked
//! (the owner: "we shouldnt open the workspace when clicking on it it should show it in the anim
//! as preview and show an open button"), and the workspace's page shows its zones with its apps
//! in them. It is the plan opening it follows (`plan_switch`), so the picture is what happens.

use std::collections::{HashMap, HashSet};

use mosaik_core::geometry::Rect;
use mosaik_core::layout::{Slot, SlotId, Workspace};
use mosaik_core::place::{Resolver, Target, fit_size};
use mosaik_core::window::{WindowFacts, WindowHandle};
use mosaik_core::workspace::plan_switch;
use mosaik_ipc::protocol::{ArrangePreview, ErrorCode, PreviewTab, PreviewWindow, PreviewZone, Reply};

use crate::editing::not_found;
use crate::thread::Engine;

impl Engine {
    /// What `switch` to `which` would do now on the main monitor: its zones, its apps' windows
    /// from where they are to their places (apps not open start there; a tab group shows as
    /// the tab that would show, with its tabs), and the other windows it would minimise.
    pub(crate) fn preview_workspace(&mut self, which: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(topology) = self.read_topology() else {
            return Reply::Error {
                code: ErrorCode::Failed,
                message: "Mosaïk could not read the monitor layout.".into(),
                hint: None,
            };
        };
        let ws = self.file.workspaces[index].clone();
        let previous: Vec<WindowHandle> = self
            .active
            .as_ref()
            .map(|a| a.windows.values().copied().collect())
            .unwrap_or_default();
        let all = self.scan_marked();
        let candidates: Vec<&WindowFacts> = all.iter().filter(|f| self.movable(f)).collect();
        let style = self.style();
        let plan = plan_switch(
            &ws,
            &candidates,
            &previous,
            &topology,
            style,
            self.settings.switching,
        );
        let main = topology.primary();
        let work_area = main.work_area;
        let on_main = |r: Rect| topology.monitor_for_rect(r).id == main.id;
        let resolver = Resolver::new(&ws.layout, &topology, style);
        let zones: Vec<PreviewZone> = ws
            .layout
            .zones
            .iter()
            .filter_map(|z| {
                let (rect, _) = resolver.zone_rect(&z.id)?;
                on_main(rect).then(|| PreviewZone {
                    name: z.name.clone(),
                    color: z.color,
                    rect,
                })
            })
            .collect();
        let rect_of = |t: &Target| match t {
            Target::Frame(r) => Some(*r),
            Target::Maximized(m) => topology.get(m).map(|m| m.work_area),
        };
        // Where window `f` lands in `r`: a window of a fixed size keeps it, as it is moved.
        let lands = |f: &WindowFacts, r: Rect| {
            if f.resizable {
                r
            } else {
                fit_size(r, f.frame_rect.size(), work_area)
            }
        };
        let facts: HashMap<WindowHandle, &WindowFacts> = candidates.iter().map(|f| (f.handle, *f)).collect();
        let window_of: HashMap<SlotId, WindowHandle> = plan.assigned.iter().cloned().collect();
        let tabs = self.preview_tabs(&ws, &plan.tabs, &window_of);

        let mut windows = Vec::new();
        let mut elsewhere = 0u32;
        let mut drawn: HashSet<WindowHandle> = HashSet::new();
        for m in &plan.moves {
            let Some(f) = facts.get(&m.window) else { continue };
            let Some(r) = rect_of(&m.target) else { continue };
            if !on_main(r) {
                elsewhere += 1;
                continue;
            }
            let then = lands(f, r);
            let slot = ws.layout.slot(&m.slot);
            windows.push(PreviewWindow {
                window: m.window,
                label: self.window_label(slot, f),
                icon: self.window_icon(f),
                // A minimised window comes back in its place.
                now: if f.minimized { then } else { f.frame_rect },
                then: Some(then),
                minimized: false,
                starting: false,
                tabs: group_tabs(&ws, &m.slot, &tabs),
            });
            drawn.insert(m.window);
        }
        // Its windows in full screen stay as they are.
        for (slot, h) in &plan.assigned {
            let Some(f) = facts.get(h).filter(|f| f.full_screen) else {
                continue;
            };
            if drawn.contains(h) || !on_main(f.frame_rect) {
                continue;
            }
            windows.push(PreviewWindow {
                window: *h,
                label: self.window_label(ws.layout.slot(slot), f),
                icon: self.window_icon(f),
                now: f.frame_rect,
                then: Some(f.frame_rect),
                minimized: false,
                starting: false,
                tabs: Vec::new(),
            });
        }
        // Apps it starts, in their places. A tab group with a tab open shows that tab; one
        // with none shows the tab it would show, once.
        let mut groups_started: HashSet<&mosaik_core::layout::TabGroupId> = HashSet::new();
        for (slot_id, _) in &plan.launch {
            let Some(slot) = ws.layout.slot(slot_id) else {
                continue;
            };
            if let Some(g) = ws.layout.group_of(slot_id) {
                let shown_open = plan.tabs.iter().any(|t| t.group == g.id);
                if shown_open || !groups_started.insert(&g.id) {
                    continue;
                }
            }
            let Some(r) = resolver.target(&slot.place).as_ref().and_then(rect_of) else {
                continue;
            };
            if !on_main(r) {
                elsewhere += 1;
                continue;
            }
            // The tab it would show: the group's own, for a group with no tab open.
            let shown = ws
                .layout
                .group_of(slot_id)
                .and_then(|g| ws.layout.slot(&g.active))
                .unwrap_or(slot);
            windows.push(PreviewWindow {
                window: WindowHandle(0),
                label: self.app_name(shown, None),
                icon: mosaik_core::icons::of_slot(shown),
                now: r,
                then: Some(r),
                minimized: false,
                starting: true,
                tabs: group_tabs(&ws, slot_id, &tabs),
            });
        }
        // The other windows on the main monitor go to the taskbar.
        for h in &plan.minimize {
            let Some(f) = facts.get(h).filter(|f| on_main(f.frame_rect)) else {
                continue;
            };
            windows.push(PreviewWindow {
                window: *h,
                label: self.window_label(None, f),
                icon: self.window_icon(f),
                now: f.frame_rect,
                then: None,
                minimized: true,
                starting: false,
                tabs: Vec::new(),
            });
        }
        Reply::Preview(Box::new(ArrangePreview {
            work_area,
            zones,
            windows,
            elsewhere,
            task: None,
        }))
    }

    /// A window's name in a preview: its slot's app, or its own app.
    fn window_label(&mut self, slot: Option<&Slot>, f: &WindowFacts) -> String {
        match slot {
            Some(s) => self.app_name(s, Some(f.handle)),
            None => {
                let path = self.scanner.process_path(f.pid);
                self.apps.label(f, path.as_deref())
            }
        }
    }

    fn window_icon(&mut self, f: &WindowFacts) -> Option<String> {
        let path = self.scanner.process_path(f.pid);
        mosaik_core::icons::key(f.aumid.as_deref(), path.as_deref())
    }

    /// Each tab group's tabs in order, with the one that would show: the open tab the plan
    /// shows, or the group's own when none is open.
    fn preview_tabs(
        &mut self,
        ws: &Workspace,
        shows: &[mosaik_core::workspace::TabShow],
        window_of: &HashMap<SlotId, WindowHandle>,
    ) -> HashMap<mosaik_core::layout::TabGroupId, Vec<PreviewTab>> {
        let mut out = HashMap::new();
        for g in &ws.layout.tab_groups {
            let showing = shows.iter().find(|t| t.group == g.id).map(|t| t.active);
            let mut tabs = Vec::with_capacity(g.tabs.len());
            for id in &g.tabs {
                let Some(slot) = ws.layout.slot(id) else { continue };
                let window = window_of.get(id).copied();
                let shown = match showing {
                    Some(active) => window == Some(active),
                    None => *id == g.active,
                };
                tabs.push(PreviewTab {
                    label: self.app_name(slot, window),
                    icon: mosaik_core::icons::of_slot(slot),
                    shown,
                });
            }
            out.insert(g.id.clone(), tabs);
        }
        out
    }
}

/// The tabs of the group `slot` is a tab of; none for a window of its own.
fn group_tabs(
    ws: &Workspace,
    slot: &SlotId,
    tabs: &HashMap<mosaik_core::layout::TabGroupId, Vec<PreviewTab>>,
) -> Vec<PreviewTab> {
    ws.layout
        .group_of(slot)
        .and_then(|g| tabs.get(&g.id))
        .cloned()
        .unwrap_or_default()
}
