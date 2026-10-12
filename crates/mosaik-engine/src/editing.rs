//! What the interface changes: settings, a workspace's zones, name and shortcut, deleting a
//! workspace. Each change is checked (with the reason when refused), saved, then applied at
//! once.

use mosaik_core::editing::{ZoneEdit, apply_zones, rename, zones_for_editor};
use mosaik_core::hotkey::{Hotkey, check};
use mosaik_core::layout::WorkspaceId;
use mosaik_core::settings::{SETTINGS_VERSION, Settings};
use mosaik_ipc::protocol::{ErrorCode, MonitorState, Reply};
use mosaik_win::hotkey::{Registration, register};
use mosaik_win::startup::{self, Startup};

use crate::hotkeys::{SWITCHER, WORKSPACE_KEYS};
use crate::switcher;
use crate::thread::Engine;

/// The id used to check that a shortcut is free, released at once.
const PROBE_KEY: i32 = 99;

fn refused(message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code: ErrorCode::BadRequest,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

fn failed(message: impl Into<String>) -> Reply {
    Reply::Error {
        code: ErrorCode::Failed,
        message: message.into(),
        hint: None,
    }
}

fn not_found(which: &str) -> Reply {
    Reply::Error {
        code: ErrorCode::NotFound,
        message: format!("There is no workspace called \u{201c}{}\u{201d}.", which.trim()),
        hint: Some("It may have been deleted meanwhile.".into()),
    }
}

/// A registered shortcut of one workspace.
pub(crate) struct WorkspaceKey {
    pub id: i32,
    pub workspace: WorkspaceId,
    _registration: Registration,
}

impl Engine {
    pub(crate) fn settings_reply(&self) -> Reply {
        Reply::Settings {
            settings: Box::new(self.settings.clone()),
        }
    }

    /// Why a shortcut cannot be used for the switcher (`mine` is `None`) or for the workspace
    /// `mine`, if it cannot: against the rules (AltGr, Windows' own), already Mosaïk's for
    /// something else, or taken by another app.
    fn hotkey_problem(&self, h: Hotkey, mine: Option<&WorkspaceId>) -> Option<String> {
        if let Err(problem) = check(h, &mosaik_win::keyboard::layouts()) {
            return Some(problem.to_string());
        }
        let switcher = self.hotkey_state().shortcut;
        if mine.is_some() && switcher == h.to_string() {
            return Some(format!("{h} opens the switcher."));
        }
        if let Some(other) = self
            .file
            .workspaces
            .iter()
            .find(|w| w.hotkey == Some(h) && Some(&w.id) != mine)
        {
            return Some(format!("{h} already switches to \u{201c}{}\u{201d}.", other.name));
        }
        // Mosaïk's own shortcut for the same thing is registered already; any other must be free.
        let ours = match mine {
            None => switcher == h.to_string(),
            Some(m) => self.workspace_has(m, h),
        };
        if !ours && let Err(e) = register(Some(self.window), PROBE_KEY, h) {
            return Some(e.to_string());
        }
        None
    }

    fn workspace_has(&self, id: &WorkspaceId, h: Hotkey) -> bool {
        self.file
            .workspaces
            .iter()
            .any(|w| &w.id == id && w.hotkey == Some(h))
    }

    /// Whether a shortcut can be used for the switcher, or for the workspace `workspace`.
    pub(crate) fn check_hotkey_reply(&self, text: &str, workspace: Option<&str>) -> Reply {
        let h: Hotkey = match text.parse() {
            Ok(h) => h,
            Err(e) => {
                return Reply::Hotkey {
                    hotkey: text.trim().to_owned(),
                    problem: Some(e.to_string()),
                    warnings: Vec::new(),
                };
            }
        };
        let warnings = check(h, &mosaik_win::keyboard::layouts())
            .map(|w| w.iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        let mine = workspace
            .and_then(|w| self.find_workspace(w))
            .map(|i| self.file.workspaces[i].id.clone());
        Reply::Hotkey {
            hotkey: h.to_string(),
            problem: self.hotkey_problem(h, mine.as_ref()),
            warnings,
        }
    }

    /// Saves the settings and applies them at once. A switcher shortcut that cannot be used is
    /// refused with the reason; nothing is saved then.
    pub(crate) fn set_settings(&mut self, mut new: Settings) -> Reply {
        new.version = SETTINGS_VERSION;
        let hotkey_changed = new.switcher_hotkey != self.settings.switcher_hotkey;
        if hotkey_changed
            && new.switcher_hotkey.to_string() != self.hotkey_state().shortcut
            && let Some(problem) = self.hotkey_problem(new.switcher_hotkey, None)
        {
            return refused(problem, Some("Pick another shortcut."));
        }
        let mut notes = Vec::new();
        if new.start_with_windows != self.settings.start_with_windows {
            match startup::set(new.start_with_windows) {
                Ok(Startup::On | Startup::Off) => {}
                Ok(Startup::OffByUser) => {
                    new.start_with_windows = false;
                    notes.push(
                        "Starting with Windows was turned off in Task Manager; turn Mosaïk on again there, under Startup apps.",
                    );
                }
                Err(e) => return failed(format!("Starting with Windows could not be changed: {e}")),
            }
        }
        let old = std::mem::replace(&mut self.settings, new);
        if let Err(e) = self.store.save(&self.settings) {
            self.settings = old;
            return failed(format!("Your settings could not be saved: {e}"));
        }
        if hotkey_changed {
            // The old shortcut goes first, so a swap between the two works.
            self.hotkey = None;
            self.register_hotkey();
        }
        if old.theme != self.settings.theme {
            switcher::set_theme(self.settings.theme);
        }
        if old.frames != self.settings.frames
            || old.theme != self.settings.theme
            || old.tab_hiding != self.settings.tab_hiding
        {
            self.refresh_frames();
        }
        Reply::Done {
            message: if notes.is_empty() {
                "Settings saved.".into()
            } else {
                format!("Settings saved. {}", notes.join(" "))
            },
        }
    }

    pub(crate) fn zones_reply(&mut self, which: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(topology) = self.refresh_topology() else {
            return failed("Mosaïk could not read the monitor layout.");
        };
        let ws = &self.file.workspaces[index];
        Reply::Zones {
            workspace: ws.id.clone(),
            name: ws.name.clone(),
            monitors: topology
                .monitors()
                .iter()
                .map(|m| MonitorState {
                    id: m.id.0.clone(),
                    name: m.name.clone(),
                    primary: m.primary,
                    bounds: m.bounds,
                    work_area: m.work_area,
                    dpi: m.dpi,
                    scale_percent: m.scale_percent(),
                })
                .collect(),
            zones: zones_for_editor(ws, &topology),
        }
    }

    pub(crate) fn set_zones(&mut self, which: &str, zones: &[ZoneEdit]) -> Reply {
        if let Some(problem) = &self.file_problem {
            return failed(format!("Workspaces cannot be saved: {problem}"));
        }
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(topology) = self.refresh_topology() else {
            return failed("Mosaïk could not read the monitor layout.");
        };
        let updated = match apply_zones(&self.file.workspaces[index], zones, &topology) {
            Ok(w) => w,
            Err(e) => return refused(e.to_string(), None),
        };
        let name = updated.name.clone();
        let id = updated.id.clone();
        self.file.workspaces[index] = updated;
        self.persist();
        if self.active.as_ref().is_some_and(|a| a.id == id) {
            self.refresh_live_zones();
            self.refresh_frames();
        }
        Reply::Done {
            message: format!("Zones of \u{201c}{name}\u{201d} saved."),
        }
    }

    pub(crate) fn rename_workspace(&mut self, which: &str, name: &str) -> Reply {
        if let Some(problem) = &self.file_problem {
            return failed(format!("Workspaces cannot be saved: {problem}"));
        }
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        match rename(&self.file.workspaces[index], name, &self.file.workspaces) {
            Ok(w) => {
                let message = format!("Renamed to \u{201c}{}\u{201d}.", w.name);
                self.file.workspaces[index] = w;
                self.persist();
                Reply::Done { message }
            }
            Err(e) => refused(e.to_string(), None),
        }
    }

    /// Deletes a workspace. Its windows stay where they are; if it was open, none is open now
    /// and its hidden tabs come back.
    pub(crate) fn delete_workspace(&mut self, which: &str) -> Reply {
        if let Some(problem) = &self.file_problem {
            return failed(format!("Workspaces cannot be saved: {problem}"));
        }
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let ws = self.file.workspaces.remove(index);
        if self.active.as_ref().is_some_and(|a| a.id == ws.id) {
            self.active = None;
            self.file.active = None;
            self.refresh_frames();
        }
        self.persist();
        self.register_workspace_hotkeys();
        Reply::Done {
            message: format!("Deleted \u{201c}{}\u{201d}.", ws.name),
        }
    }

    pub(crate) fn set_workspace_hotkey(&mut self, which: &str, hotkey: Option<&str>) -> Reply {
        if let Some(problem) = &self.file_problem {
            return failed(format!("Workspaces cannot be saved: {problem}"));
        }
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let id = self.file.workspaces[index].id.clone();
        let wanted = match hotkey.map(str::parse::<Hotkey>) {
            None => None,
            Some(Ok(h)) => {
                if let Some(problem) = self.hotkey_problem(h, Some(&id)) {
                    return refused(problem, Some("Pick another shortcut."));
                }
                Some(h)
            }
            Some(Err(e)) => return refused(e.to_string(), None),
        };
        self.file.workspaces[index].hotkey = wanted;
        self.persist();
        let problems = self.register_workspace_hotkeys();
        let name = self.file.workspaces[index].name.clone();
        if let Some(p) = problems.first() {
            return failed(p.clone());
        }
        Reply::Done {
            message: match wanted {
                Some(h) => format!("{h} now switches to \u{201c}{name}\u{201d}."),
                None => format!("\u{201c}{name}\u{201d} has no shortcut now."),
            },
        }
    }

    /// Registers every workspace's own shortcut (again). Returns what could not be registered.
    pub(crate) fn register_workspace_hotkeys(&mut self) -> Vec<String> {
        self.workspace_keys.clear();
        if self.config.safe_mode {
            return Vec::new();
        }
        let mut problems = Vec::new();
        let with_keys: Vec<(WorkspaceId, String, Hotkey)> = self
            .file
            .workspaces
            .iter()
            .filter_map(|w| w.hotkey.map(|h| (w.id.clone(), w.name.clone(), h)))
            .collect();
        for (n, (workspace, name, h)) in with_keys.into_iter().enumerate() {
            let id = WORKSPACE_KEYS + i32::try_from(n).unwrap_or(0);
            if id == SWITCHER {
                continue;
            }
            match register(Some(self.window), id, h) {
                Ok(registration) => self.workspace_keys.push(WorkspaceKey {
                    id,
                    workspace,
                    _registration: registration,
                }),
                Err(e) => problems.push(format!("\u{201c}{name}\u{201d} has no shortcut: {e}")),
            }
        }
        problems
    }

    /// A workspace's own shortcut was pressed.
    pub(crate) fn workspace_key(&mut self, id: i32) {
        let Some(workspace) = self
            .workspace_keys
            .iter()
            .find(|k| k.id == id)
            .map(|k| k.workspace.0.clone())
        else {
            return;
        };
        if let Reply::Error { message, hint, .. } = self.switch(&workspace) {
            self.notice(
                "Mosaïk",
                hint.map_or(message.clone(), |h| format!("{message} {h}")),
            );
        }
    }
}
