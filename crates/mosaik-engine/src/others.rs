//! Other programs that arrange, snap or group windows (`mosaik_core::others`), and Windows' own
//! snapping: the user is told once, ever, rather than having two of them fight silently.

use mosaik_core::geometry::Point;
use mosaik_core::others::{self, Kind};

use crate::thread::Engine;

impl Engine {
    /// Tells the user something once, ever: the key is kept in the settings (in memory only while
    /// the settings file could not be read, so Mosaïk's defaults never replace it).
    pub(crate) fn tell_once_ever(&mut self, key: String, title: String, body: String) {
        if self.settings.notices_shown.contains(&key) {
            return;
        }
        self.notice(title, body);
        if self.settings.notice_shown(&key, self.settings_on_disk)
            && let Err(e) = self.store.save(&self.settings)
        {
            // The notice may come again next time; nothing else depends on it.
            tracing::warn!(error = %e, "settings not saved after a notice");
        }
    }

    /// When Mosaïk starts, and once things settle after a switch or a display change: other
    /// programs that arrange windows. Nothing is listed once each of them has been told.
    pub(crate) fn check_others(&mut self) {
        // A probe or test runs beside the user's own Mosaïk: the warning is not for it to give.
        if self.config.manage_only.is_some() || self.config.safe_mode {
            return;
        }
        let shown = &self.settings.notices_shown;
        if others::KNOWN
            .iter()
            .all(|o| shown.contains(&others::notice_key(o)))
        {
            return;
        }
        let running = match mosaik_win::process::running_programs() {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "running programs not listed");
                return;
            }
        };
        for o in others::found(running.iter().map(String::as_str)) {
            // Snapping zones only meet Mosaïk's own drags: said when those are on.
            if o.kind == Kind::Zones && !self.settings.snap.enabled {
                continue;
            }
            tracing::info!(other = o.name, "another program that arranges windows is running");
            let (title, body) = others::notice(&o, self.settings.snap.invert_shift);
            self.tell_once_ever(others::notice_key(&o), title, body);
        }
    }

    /// After a window was dropped on a zone or a tab at `pointer`, and went there: at an edge of
    /// the screen where Windows' own snapping reacts to the same drag. Mosaïk's place wins (the
    /// window ends where the user dropped it, job 0048); the user is told once why Windows showed
    /// its previews too.
    pub(crate) fn note_windows_snapping(&mut self, pointer: Point) {
        let Some(topology) = &self.topology else { return };
        let monitors: Vec<_> = topology.monitors().iter().map(|m| m.bounds).collect();
        if !mosaik_core::snap::at_screen_edge(pointer, &monitors) || !mosaik_win::os::snapping_on() {
            return;
        }
        self.tell_once_ever(
            "windows-snapping".into(),
            "Windows snaps windows too".into(),
            "Dropped at the edge of the screen, a window also sets off Windows' own snapping. Mosaïk keeps it where you dropped it, on the zone or the tab. To stop Windows' previews when you drag, turn off \u{201c}Snap windows\u{201d} in Windows Settings, System, Multitasking.".into(),
        );
    }
}
