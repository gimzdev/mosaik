//! A workspace's apps, from the Workspaces page: each with its zone, the windows on screen it
//! could take, and the changes the user makes there (add a window's app, put an app in another
//! zone, take one out). Each change is saved before anything moves; in the open workspace, the
//! window it is about goes to its place at once, since the user asked for it.

use std::collections::HashSet;

use mosaik_core::editing::{add_app, empty_zone, move_app, remove_app};
use mosaik_core::layout::{Launch, Place, Slot, SlotId, Workspace};
use mosaik_core::window::WindowHandle;
use mosaik_core::zone::ZoneId;
use mosaik_ipc::protocol::{AppZone, OpenWindow, Reply, WorkspaceApp, WorkspaceApps};
use mosaik_win::{apps, window};

use crate::editing::{failed, no_monitors, not_found, refused};
use crate::thread::Engine;

impl Engine {
    /// What to call an app of a workspace: its own label; for one saved before apps had one,
    /// its open window's app, or its program's description.
    pub(crate) fn app_name(&mut self, slot: &Slot, window: Option<WindowHandle>) -> String {
        if slot.label.is_some() {
            return slot.name();
        }
        if let Some(f) = window.and_then(|h| self.scanner.facts(window::hwnd(h))) {
            let path = self.scanner.process_path(f.pid);
            return self.apps.label(&f, path.as_deref());
        }
        if let Some(Launch::Program { path, .. }) = &slot.launch
            && slot
                .app
                .exe
                .as_deref()
                .and_then(mosaik_core::window::known_app_name)
                .is_none()
            && let Some(d) = self.apps.description(path)
        {
            return d;
        }
        slot.name()
    }

    /// The open workspace's apps saved before apps had names take the names of their windows
    /// on screen, so lists show them as people know them from then on.
    pub(crate) fn name_open_apps(&mut self) {
        let Some(active) = &self.active else { return };
        let (id, windows) = (active.id.clone(), active.windows.clone());
        let Some(index) = self.file.workspaces.iter().position(|w| w.id == id) else {
            return;
        };
        let unnamed: Vec<(SlotId, WindowHandle)> = self.file.workspaces[index]
            .layout
            .slots
            .iter()
            .filter(|s| s.label.is_none())
            .filter_map(|s| windows.get(&s.id).map(|h| (s.id.clone(), *h)))
            .collect();
        let mut named = Vec::new();
        for (slot, h) in unnamed {
            if let Some(f) = self.scanner.facts(window::hwnd(h)) {
                let path = self.scanner.process_path(f.pid);
                named.push((slot, self.apps.label(&f, path.as_deref())));
            }
        }
        if named.is_empty() {
            return;
        }
        for (slot, label) in named {
            if let Some(s) = self.file.workspaces[index]
                .layout
                .slots
                .iter_mut()
                .find(|s| s.id == slot)
            {
                s.label = Some(label);
            }
        }
        self.persist();
    }

    /// A workspace's apps, its zones, and the windows on screen it could take.
    pub(crate) fn apps_reply(&mut self, which: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        if self.is_open(index) {
            self.name_open_apps();
        }
        let id = self.file.workspaces[index].id.clone();
        let filled = self
            .active
            .as_ref()
            .filter(|a| a.id == id)
            .map(|a| a.windows.clone())
            .unwrap_or_default();
        // Open windows that are not this workspace's already, minimised ones included, front to
        // back.
        let taken: HashSet<WindowHandle> = filled.values().copied().collect();
        let on_screen: Vec<_> = self
            .scan_marked()
            .into_iter()
            .filter(|f| {
                self.addable(f) && !self.hiding.hidden.contains_key(&f.handle) && !taken.contains(&f.handle)
            })
            .collect();
        let windows = on_screen
            .iter()
            .map(|f| {
                let path = self.scanner.process_path(f.pid);
                OpenWindow {
                    window: f.handle,
                    label: self.apps.label(f, path.as_deref()),
                    title: f.title.trim().to_owned(),
                    icon: mosaik_core::icons::key(f.aumid.as_deref(), path.as_deref()),
                }
            })
            .collect();
        let ws: Workspace = self.file.workspaces[index].clone();
        let apps = ws
            .layout
            .slots
            .iter()
            .map(|s| WorkspaceApp {
                id: s.id.clone(),
                label: self.app_name(s, filled.get(&s.id).copied()),
                icon: mosaik_core::icons::of_slot(s),
                zone: match ws.layout.effective_place(s) {
                    Some(Place::Zone { zone }) => Some(zone.clone()),
                    _ => None,
                },
                tab: matches!(s.place, Place::Tab { .. }),
            })
            .collect();
        let zones = ws
            .layout
            .zones
            .iter()
            .map(|z| AppZone {
                id: z.id.clone(),
                name: z.name.clone(),
                color: z.color,
            })
            .collect();
        Reply::Apps(Box::new(WorkspaceApps {
            workspace: ws.id,
            name: ws.name,
            apps,
            zones,
            windows,
        }))
    }

    /// Replaces workspace `index` with `updated` and saves; puts it back when it cannot be
    /// saved, and says why.
    fn save_workspace(&mut self, index: usize, updated: Workspace) -> Result<(), String> {
        let before = std::mem::replace(&mut self.file.workspaces[index], updated);
        self.save_file().inspect_err(|_| {
            self.file.workspaces[index] = before;
        })
    }

    /// Whether workspace `index` is the open one.
    fn is_open(&self, index: usize) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.id == self.file.workspaces[index].id)
    }

    /// In the open workspace, the windows of `zone` go to their places in it (one just added or
    /// moved there, that is), and tab groups show what they should. `added`, minimised, is shown
    /// again in its place.
    fn place_in(&mut self, zone: &ZoneId, added: Option<WindowHandle>) {
        let Some(topology) = self.refresh_topology() else {
            return;
        };
        let only: HashSet<ZoneId> = HashSet::from([zone.clone()]);
        let placed = self.refit_with(&topology, Some(&only), added);
        self.sync_tabs();
        // A window its app keeps larger than the zone: the zones make room for it.
        self.make_room(&topology, &placed);
        self.refresh_frames();
    }

    /// Adds the app of `window` (open, or minimised) to a workspace: in `zone`, else its first
    /// empty zone, else where the window is. In the open workspace, a minimised window is shown
    /// again in its place.
    pub(crate) fn add_app_to(&mut self, which: &str, window: WindowHandle, zone: Option<ZoneId>) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(topology) = self.refresh_topology() else {
            return no_monitors();
        };
        let Some(facts) = self
            .scanner
            .facts(window::hwnd(window))
            .filter(|f| self.addable(f))
        else {
            return refused(
                "This window is not open any more.",
                Some("Pick it again from the list."),
            );
        };
        let open = self.is_open(index);
        if open && self.active.as_ref().and_then(|a| a.slot_of(window)).is_some() {
            return refused("This window is part of the workspace already.", None);
        }
        let path = self.scanner.process_path(facts.pid);
        let label = self.apps.label(&facts, path.as_deref());
        let launch = apps::launch_for(&facts, path.as_deref());
        let ws = &self.file.workspaces[index];
        let zone = zone.or_else(|| empty_zone(ws));
        let (updated, slot) = match add_app(ws, &facts, &label, launch, zone.as_ref(), &topology) {
            Ok(x) => x,
            Err(e) => return refused(e.to_string(), None),
        };
        let id = updated.id.0.clone();
        if let Err(e) = self.save_workspace(index, updated) {
            return failed(e);
        }
        tracing::info!(slot = %slot.0, zone = ?zone, open, "app added");
        if open {
            if let Some(a) = &mut self.active {
                a.windows.insert(slot, window);
            }
            match &zone {
                Some(z) => self.place_in(z, Some(window)),
                None => self.refresh_frames(),
            }
        }
        self.apps_reply(&id)
    }

    /// Puts an app of a workspace in one of its zones.
    pub(crate) fn move_app_to(&mut self, which: &str, slot: &SlotId, zone: &ZoneId) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let updated = match move_app(&self.file.workspaces[index], slot, zone) {
            Ok(w) => w,
            Err(e) => return refused(e.to_string(), Some("Look at the list again.")),
        };
        let id = updated.id.0.clone();
        if updated != self.file.workspaces[index] {
            if let Err(e) = self.save_workspace(index, updated) {
                return failed(e);
            }
            tracing::info!(slot = %slot.0, zone = %zone.0, "app moved");
            if self.is_open(index) {
                if let Some(a) = &mut self.active {
                    // It may have been a tab shown for now: it is no longer one.
                    a.shown.retain(|_, s| s != slot);
                }
                self.place_in(zone, None);
            }
        }
        self.apps_reply(&id)
    }

    /// Takes an app out of a workspace; its window, if open, stays where it is.
    pub(crate) fn remove_app_from(&mut self, which: &str, slot: &SlotId) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let updated = match remove_app(&self.file.workspaces[index], slot) {
            Ok(w) => w,
            Err(e) => return refused(e.to_string(), Some("Look at the list again.")),
        };
        let id = updated.id.0.clone();
        if let Err(e) = self.save_workspace(index, updated) {
            return failed(e);
        }
        tracing::info!(slot = %slot.0, "app removed");
        if self.is_open(index) {
            if let Some(a) = &mut self.active {
                a.windows.remove(slot);
                a.shown.retain(|_, s| s != slot);
            }
            // A tab it hid comes back; the group it leaves shows what it should.
            self.sync_tabs();
            self.refresh_frames();
        }
        self.apps_reply(&id)
    }
}
