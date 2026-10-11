//! Starting the watchdog (ADR 0006): a separate process that restores every hidden window if
//! Mosaïk ends without doing it itself.

use std::path::{Path, PathBuf};

use mosaik_win::process::{Process, own_pid, spawn};

/// The watchdog next to the running executable.
pub fn executable() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name("mosaik-watchdog.exe"))
        .filter(|p| p.is_file())
}

/// Starts the watchdog on this process. Mosaïk refuses to hide windows without it.
pub fn start(journal: &Path) -> Result<Process, String> {
    let exe = executable().ok_or_else(|| {
        "mosaik-watchdog.exe is missing next to mosaik.exe; reinstall Mosaïk to restore it".to_owned()
    })?;
    spawn(
        &exe.to_string_lossy(),
        &format!("--watch {} --journal \"{}\"", own_pid(), journal.display()),
    )
    .map_err(|e| format!("the watchdog could not start: {e}"))
}
