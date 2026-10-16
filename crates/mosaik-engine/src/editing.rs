//! What the interface changes: settings, a workspace's zones, name and shortcut, deleting a
//! workspace. Each change is checked (with the reason when refused) and saved, then applied at
//! once; when it cannot be saved, nothing changes and the reply says why.

use mosaik_core::editing::{ZoneEdit, apply_zones, rename, revision, with_layout, zones_for_editor};
use mosaik_core::hotkey::{Hotkey, LayoutTyping, check};
use mosaik_core::layout::{Workspace, WorkspaceId};
use mosaik_core::presets::Preset;
use mosaik_core::settings::{SETTINGS_VERSION, Settings};
use mosaik_ipc::protocol::{ErrorCode, Reply};
use mosaik_win::hotkey::{Registration, register};
use mosaik_win::startup::{self, Startup};

use crate::hotkeys::WORKSPACE_KEYS;
use crate::switcher;
use crate::thread::Engine;

/// The id used to check that a shortcut is free, released at once.
const PROBE_KEY: i32 = 99;

pub(crate) fn refused(message: impl Into<String>, hint: Option<&str>) -> Reply {
    Reply::Error {
        code: ErrorCode::BadRequest,
        message: message.into(),
        hint: hint.map(str::to_owned),
    }
}

pub(crate) fn failed(message: impl Into<String>) -> Reply {
    Reply::Error {
        code: ErrorCode::Failed,
        message: message.into(),
        hint: None,
    }
}

/// A workspace as it was before a layout was picked for it, and as that left it: put back while
/// it is still so.
pub(crate) struct LayoutUndo {
    pub(crate) before: Workspace,
    pub(crate) after: Workspace,
}

/// The layout called `id`, or the reply that says there is none.
pub(crate) fn preset_named(id: &str) -> Result<Preset, Reply> {
    Preset::from_id(id.trim()).ok_or_else(|| {
        let known: Vec<&str> = Preset::ALL.iter().map(|p| p.id()).collect();
        refused(
            format!("There is no layout called \u{201c}{}\u{201d}.", id.trim()),
            Some(&format!("The layouts are {}.", known.join(", "))),
        )
    })
}

pub(crate) fn not_found(which: &str) -> Reply {
    Reply::Error {
        code: ErrorCode::NotFound,
        message: format!("There is no workspace called \u{201c}{}\u{201d}.", which.trim()),
        hint: Some("It may have been renamed or deleted meanwhile.".into()),
    }
}

pub(crate) fn no_monitors() -> Reply {
    failed("Mosaïk could not read the monitor layout.")
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
            hotkey: self.hotkey_state(),
            side_menu: self.side_menu_state(),
            read_only: self.settings_on_disk == mosaik_core::settings::OnDisk::Newer,
            packaged: mosaik_win::package::is_packaged(),
        }
    }

    /// Why a shortcut cannot be used for the switcher (`mine` is `None`) or for the workspace
    /// `mine`, if it cannot: against the rules (AltGr, Windows' own), already Mosaïk's for
    /// something else, or taken by another app.
    fn hotkey_problem(
        &self,
        h: Hotkey,
        mine: Option<&WorkspaceId>,
        layouts: &[LayoutTyping],
    ) -> Option<String> {
        if let Err(problem) = check(h, layouts) {
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
        // Mosaïk's own registration of this shortcut for the same thing is no conflict; any
        // other must be free.
        let ours = match mine {
            None => switcher == h.to_string(),
            Some(m) => self.key_of(m).is_some_and(|k| k.1 == h),
        };
        if !ours && let Err(e) = register(Some(self.window), PROBE_KEY, h) {
            return Some(e.to_string());
        }
        None
    }

    /// The registered shortcut of a workspace, as (its `WM_HOTKEY` id, the shortcut).
    fn key_of(&self, workspace: &WorkspaceId) -> Option<(i32, Hotkey)> {
        let key = self.workspace_keys.iter().find(|k| &k.workspace == workspace)?;
        let ws = self.file.workspaces.iter().find(|w| &w.id == workspace)?;
        Some((key.id, ws.hotkey?))
    }

    /// Whether a shortcut can be used for the switcher, or for the workspace `workspace`.
    pub(crate) fn check_hotkey_reply(&self, text: &str, workspace: Option<&str>) -> Reply {
        let mine = match workspace {
            None => None,
            Some(w) => match self.find_workspace(w) {
                Some(i) => Some(self.file.workspaces[i].id.clone()),
                None => return not_found(w),
            },
        };
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
        let layouts = mosaik_win::keyboard::layouts();
        let warnings = check(h, &layouts)
            .map(|w| w.iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        Reply::Hotkey {
            hotkey: h.to_string(),
            problem: self.hotkey_problem(h, mine.as_ref(), &layouts),
            warnings,
        }
    }

    /// Saves the settings and applies them at once. A switcher shortcut that cannot be used is
    /// refused with the reason; nothing is saved then.
    pub(crate) fn set_settings(&mut self, mut new: Settings) -> Reply {
        new.version = SETTINGS_VERSION;
        if let Some(problem) = new.problem() {
            return refused(problem, None);
        }
        if self.settings_on_disk == mosaik_core::settings::OnDisk::Newer {
            return refused(
                "These settings were saved by a newer Mosaïk, so this one leaves them as they are."
                    .to_owned(),
                Some("Update Mosaïk to change them."),
            );
        }
        let hotkey_changed = new.switcher_hotkey != self.settings.switcher_hotkey;
        if hotkey_changed
            && new.switcher_hotkey.to_string() != self.hotkey_state().shortcut
            && let Some(problem) =
                self.hotkey_problem(new.switcher_hotkey, None, &mosaik_win::keyboard::layouts())
        {
            return refused(problem, Some("Pick another shortcut."));
        }
        let mut notes = Vec::new();
        let startup_changed = new.start_with_windows != self.settings.start_with_windows;
        if startup_changed {
            match startup::set(new.start_with_windows) {
                Ok(Startup::On | Startup::Off) => {}
                Ok(Startup::Refused) => {
                    new.start_with_windows = false;
                    notes.push(
                        "Windows did not let Mosaïk start with it: it is turned off under Startup apps (in Task Manager or Settings), where only you can turn it on.",
                    );
                }
                Err(e) => return failed(format!("Starting with Windows could not be changed: {e}")),
            }
        }
        // Notices shown since the interface read the settings stay shown.
        for key in &self.settings.notices_shown {
            if !new.notices_shown.contains(key) {
                new.notices_shown.push(key.clone());
            }
        }
        let old = std::mem::replace(&mut self.settings, new);
        if let Err(e) = self.store.save(&self.settings) {
            // Put everything back as it was, starting with Windows included.
            if startup_changed && old.start_with_windows != self.settings.start_with_windows {
                let _ = startup::set(old.start_with_windows);
            }
            self.settings = old;
            return failed(format!("Your settings could not be saved: {e}"));
        }
        // The user's own save: the file holds their settings from now on.
        self.settings_on_disk = mosaik_core::settings::OnDisk::Readable;
        // While shortcuts are held for the recorder, the new one is registered on release.
        if hotkey_changed && !self.config.safe_mode && !self.hotkeys_held {
            // The old shortcut goes first, so a swap between the two works.
            self.hotkey = None;
            self.register_hotkey();
        }
        if old.theme != self.settings.theme {
            switcher::set_theme(self.settings.theme);
        }
        if old.edge != self.settings.edge || old.theme != self.settings.theme {
            self.place_quick();
        }
        if old.frames.effective_style() != self.settings.frames.effective_style() {
            // Tags moved between title bars and bands above windows, or bands came or went:
            // windows take their places again.
            if let Some(topology) = self.topology.clone() {
                self.refit_open_workspace(&topology);
            }
        } else if old.frames != self.settings.frames
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

    /// A workspace's zones, for the editor; also the ones Arrange put on screen, by their id
    /// (`mosaikctl state` gives it), which are not saved and so not edited.
    pub(crate) fn zones_reply(&self, which: &str) -> Reply {
        let arranged = self
            .arranged
            .as_ref()
            .map(|a| &a.ws)
            .filter(|w| w.id.0 == which.trim());
        let Some(ws) = arranged.or_else(|| self.find_workspace(which).map(|i| &self.file.workspaces[i]))
        else {
            return not_found(which);
        };
        let Some(topology) = self.read_topology() else {
            return no_monitors();
        };
        Reply::Zones {
            workspace: ws.id.clone(),
            name: ws.name.clone(),
            monitors: topology.monitors().iter().map(Into::into).collect(),
            zones: zones_for_editor(ws, &topology),
            revision: revision(ws, &topology),
        }
    }

    /// Saves the zone editor's zones. Windows of the open workspace move to their zones at
    /// once; the reply is the zones as saved (new zones have their ids).
    pub(crate) fn set_zones(&mut self, which: &str, zones: &[ZoneEdit], seen: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(topology) = self.read_topology() else {
            return no_monitors();
        };
        let current = &self.file.workspaces[index];
        if revision(current, &topology) != seen {
            return refused(
                "The monitors or these zones changed since the editor opened, so nothing was saved.",
                Some("Open the editor again to see them as they are now."),
            );
        }
        let updated = match apply_zones(current, zones, &topology, self.style()) {
            Ok(w) => w,
            Err(e) => return refused(e.to_string(), None),
        };
        let id = updated.id.clone();
        let before = std::mem::replace(&mut self.file.workspaces[index], updated);
        if let Err(e) = self.save_file() {
            self.file.workspaces[index] = before;
            return failed(e);
        }
        if self.active.as_ref().is_some_and(|a| a.id == id) {
            self.refit_open_workspace(&topology);
        }
        self.zones_reply(&id.0)
    }

    /// Lays workspace `which`'s zones on the main monitor out as `preset`, every app kept in a
    /// zone (`editing::with_layout`, ADR 0033), and saves it; in the open workspace its windows
    /// move at once. What it was can be put back (`undo_workspace_layout`).
    pub(crate) fn set_workspace_layout(&mut self, which: &str, preset: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let preset = match preset_named(preset) {
            Ok(p) => p,
            Err(reply) => return reply,
        };
        let Some(topology) = self.read_topology() else {
            return no_monitors();
        };
        let before = self.file.workspaces[index].clone();
        let mut ws = before.clone();
        let main = ws.layout.monitor_index(topology.primary(), &topology);
        let updated = with_layout(&ws, main, &preset.zones());
        let message = format!(
            "\u{201c}{}\u{201d} is laid out in {}.",
            before.name,
            preset.label()
        );
        if updated == before {
            return Reply::Done { message };
        }
        let id = updated.id.clone();
        self.file.workspaces[index] = updated.clone();
        if let Err(e) = self.save_file() {
            self.file.workspaces[index] = before;
            return failed(e);
        }
        tracing::info!(preset = preset.id(), "workspace laid out");
        self.layout_undo = Some(LayoutUndo {
            before,
            after: updated,
        });
        if self.active.as_ref().is_some_and(|a| a.id == id) {
            self.refit_open_workspace(&topology);
        }
        Reply::Done { message }
    }

    /// Puts workspace `which` back as it was before the last layout picked for it, while it has
    /// not changed since.
    pub(crate) fn undo_workspace_layout(&mut self, which: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let Some(undo) = self.layout_undo.take() else {
            return Reply::Error {
                code: ErrorCode::NotFound,
                message: "There is nothing to undo.".into(),
                hint: None,
            };
        };
        if self.file.workspaces[index] != undo.after {
            return refused(
                format!(
                    "\u{201c}{}\u{201d} changed since its layout was picked, so it stays as it is.",
                    self.file.workspaces[index].name
                ),
                None,
            );
        }
        let id = undo.before.id.clone();
        let message = format!("\u{201c}{}\u{201d} is back as it was.", undo.before.name);
        self.file.workspaces[index] = undo.before;
        if let Err(e) = self.save_file() {
            self.file.workspaces[index] = undo.after;
            return failed(e);
        }
        tracing::info!("workspace layout undone");
        if self.active.as_ref().is_some_and(|a| a.id == id)
            && let Some(topology) = self.read_topology()
        {
            self.refit_open_workspace(&topology);
        }
        Reply::Done { message }
    }

    pub(crate) fn rename_workspace(&mut self, which: &str, name: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let renamed = match rename(&self.file.workspaces[index], name, &self.file.workspaces) {
            Ok(w) => w,
            Err(e) => return refused(e.to_string(), None),
        };
        let message = format!("Renamed to \u{201c}{}\u{201d}.", renamed.name);
        let before = std::mem::replace(&mut self.file.workspaces[index], renamed);
        if let Err(e) = self.save_file() {
            self.file.workspaces[index] = before;
            return failed(e);
        }
        Reply::Done { message }
    }

    /// Deletes a workspace. Its windows stay where they are; if it was open, none is open now
    /// and its hidden tabs come back.
    pub(crate) fn delete_workspace(&mut self, which: &str) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let ws = self.file.workspaces.remove(index);
        let active_before = self.file.active.clone();
        if active_before.as_ref() == Some(&ws.id) {
            self.file.active = None;
        }
        if let Err(e) = self.save_file() {
            self.file.workspaces.insert(index, ws);
            self.file.active = active_before;
            return failed(e);
        }
        self.workspace_keys.retain(|k| k.workspace != ws.id);
        // What Arrange put on screen stays; it was arranged from no workspace now.
        if let Some(a) = &mut self.arranged
            && a.from.as_ref() == Some(&ws.id)
        {
            a.from = None;
        }
        if self.active.as_ref().is_some_and(|a| a.id == ws.id) {
            self.active = None;
            self.show_all_hidden();
            self.refresh_frames();
        }
        Reply::Done {
            message: format!("Deleted \u{201c}{}\u{201d}.", ws.name),
        }
    }

    pub(crate) fn set_workspace_hotkey(&mut self, which: &str, hotkey: Option<&str>) -> Reply {
        let Some(index) = self.find_workspace(which) else {
            return not_found(which);
        };
        let id = self.file.workspaces[index].id.clone();
        let wanted = match hotkey.map(str::parse::<Hotkey>) {
            None => None,
            Some(Ok(h)) => {
                if let Some(problem) = self.hotkey_problem(h, Some(&id), &mosaik_win::keyboard::layouts()) {
                    return refused(problem, Some("Pick another shortcut."));
                }
                Some(h)
            }
            Some(Err(e)) => return refused(e.to_string(), None),
        };
        let before = std::mem::replace(&mut self.file.workspaces[index].hotkey, wanted);
        if let Err(e) = self.save_file() {
            self.file.workspaces[index].hotkey = before;
            return failed(e);
        }
        let name = self.file.workspaces[index].name.clone();
        self.workspace_keys.retain(|k| k.workspace != id);
        if let Some(h) = wanted
            && !self.config.safe_mode
            && !self.hotkeys_held
            && let Err(problem) = self.register_workspace_key(&id, h)
        {
            return failed(format!(
                "{h} is saved for \u{201c}{name}\u{201d} but does not work yet: {problem}"
            ));
        }
        Reply::Done {
            message: match wanted {
                Some(h) => format!("{h} now switches to \u{201c}{name}\u{201d}."),
                None => format!("\u{201c}{name}\u{201d} has no shortcut now."),
            },
        }
    }

    /// Registers one workspace's shortcut under the first free `WM_HOTKEY` id.
    fn register_workspace_key(&mut self, workspace: &WorkspaceId, h: Hotkey) -> Result<(), String> {
        let id = (WORKSPACE_KEYS..)
            .find(|n| self.workspace_keys.iter().all(|k| k.id != *n))
            .unwrap_or(WORKSPACE_KEYS);
        let registration = register(Some(self.window), id, h).map_err(|e| e.to_string())?;
        self.workspace_keys.push(WorkspaceKey {
            id,
            workspace: workspace.clone(),
            _registration: registration,
        });
        Ok(())
    }

    /// Registers every workspace's own shortcut, at start. A shortcut from a file edited by
    /// hand that the recorder would refuse (a bare F-key, Ctrl+Alt with a letter) is refused
    /// here too. Returns what could not be registered, one sentence each.
    pub(crate) fn register_workspace_hotkeys(&mut self) -> Vec<String> {
        self.workspace_keys.clear();
        let layouts = mosaik_win::keyboard::layouts();
        let with_keys: Vec<(WorkspaceId, String, Hotkey)> = self
            .file
            .workspaces
            .iter()
            .filter_map(|w| w.hotkey.map(|h| (w.id.clone(), w.name.clone(), h)))
            .collect();
        let mut problems = Vec::new();
        for (workspace, name, h) in with_keys {
            let result = match check(h, &layouts) {
                Ok(_) => self.register_workspace_key(&workspace, h),
                Err(problem) => Err(problem.to_string()),
            };
            if let Err(problem) = result {
                problems.push(format!(
                    "{h} does not switch to \u{201c}{name}\u{201d}: {problem}"
                ));
            }
        }
        problems
    }

    /// Lets go of every shortcut while the interface records one, or takes them back.
    pub(crate) fn hold_hotkeys(&mut self, hold: bool) -> Reply {
        if self.config.safe_mode {
            return Reply::Done {
                message: "Safe mode: no shortcut is registered.".into(),
            };
        }
        if !hold {
            self.release_hotkeys();
            return Reply::Done {
                message: "Mosaïk's shortcuts are back.".into(),
            };
        }
        self.hotkey = None;
        self.workspace_keys.clear();
        self.hotkeys_held = true;
        self.set_timer(crate::thread::timer::HOTKEYS, 60_000);
        Reply::Done {
            message: "Mosaïk's shortcuts are off while you record one.".into(),
        }
    }

    /// Registers every shortcut again after `hold_hotkeys`.
    pub(crate) fn release_hotkeys(&mut self) {
        self.kill_timer(crate::thread::timer::HOTKEYS);
        if !std::mem::take(&mut self.hotkeys_held) {
            return;
        }
        self.hotkey = None;
        self.register_hotkey();
        let problems = self.register_workspace_hotkeys();
        if !problems.is_empty() {
            self.notice("Some workspace shortcuts are not set", problems.join(" "));
        }
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
