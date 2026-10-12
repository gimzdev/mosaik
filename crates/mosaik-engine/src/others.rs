//! Other programs that arrange, snap or group windows (`mosaik_core::others`): the user is told
//! once, ever, about each one found running, when Mosaïk starts and when a workspace opens.

use mosaik_core::others;

use crate::thread::Engine;

impl Engine {
    pub(crate) fn check_others(&mut self) {
        // A probe or test runs beside the user's own Mosaïk: the warning is not for it to give.
        if self.config.manage_only.is_some() {
            return;
        }
        let running = match mosaik_win::process::running_programs() {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, "running programs not listed");
                return;
            }
        };
        let mut told = false;
        for o in others::found(running.iter().map(String::as_str)) {
            let key = others::notice_key(&o);
            if self.settings.notices_shown.contains(&key) {
                continue;
            }
            tracing::info!(other = o.name, "another program that arranges windows is running");
            let (title, body) = others::notice(&o, self.settings.snap.invert_shift);
            self.notice(title, body);
            self.settings.notices_shown.push(key);
            told = true;
        }
        if told && let Err(e) = self.store.save(&self.settings) {
            // The notice may come again next time; nothing else depends on it.
            tracing::warn!(error = %e, "settings not saved after a notice");
        }
    }
}
