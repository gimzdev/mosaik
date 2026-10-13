//! Starting Mosaïk with Windows, the way that fits how it was installed: the Store package's
//! startup task (which the user can also turn off in Task Manager), or for a build run from a
//! folder, an entry in the user's own "Run" key. Nothing needs administrator rights.
//!
//! Started with Windows, Mosaïk waits in the tray; started by hand (from the Start menu, say), it
//! opens its window, so a click on its icon never seems to do nothing.

use std::sync::mpsc;
use std::time::Duration;

use windows::ApplicationModel::Activation::ActivationKind;
use windows::ApplicationModel::{AppInstance, StartupTask, StartupTaskState};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR};

use crate::error::{Error, Result};
use crate::text::wide;

/// The startup task declared in the package manifest (installer/msix).
const TASK_ID: &str = "MosaikStartup";
/// How long Windows may take to answer about the startup task.
const TASK_TIMEOUT: Duration = Duration::from_secs(5);
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Mosaik";
/// On Mosaïk's command line when Windows starts it: it waits in the tray. The Run key's entry
/// carries it; the package's startup task cannot, and is told by how it was activated.
pub const BACKGROUND: &str = "--background";

/// How starting with Windows ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    On,
    Off,
    /// Windows did not let it be turned on: the user turned it off under Startup apps (Task
    /// Manager or Settings), or declined, or a policy forbids it. Only the user can change that,
    /// there.
    Refused,
}

/// Whether Windows started this process as the user signed in (so it waits in the tray), rather
/// than the user (so it shows its window).
pub fn started_with_windows(args: &[String]) -> bool {
    args.iter().any(|a| a == BACKGROUND) || (crate::package::is_packaged() && activated_by_startup_task())
}

/// Whether the package's startup task started this process. Asked on a thread of its own, like
/// the startup task itself: it is a WinRT call, and the caller's thread may set up COM its own way
/// later. Not knowing counts as no.
fn activated_by_startup_task() -> bool {
    let (send, answer) = mpsc::channel();
    let asked = std::thread::Builder::new()
        .name("mosaik-activation".into())
        .spawn(move || {
            let kind = AppInstance::GetActivatedEventArgs().and_then(|a| a.Kind());
            let _ = send.send(kind.is_ok_and(|k| k == ActivationKind::StartupTask));
        });
    asked.is_ok() && answer.recv_timeout(TASK_TIMEOUT).unwrap_or(false)
}

/// The Run key's command line for Mosaïk at `exe`: started with Windows, it waits in the tray.
fn run_command(exe: &std::path::Path) -> String {
    format!("\"{}\" {BACKGROUND}", exe.display())
}

/// For a build run from a folder: its Run entry, when there is one, written again, so it names
/// this program and starts it in the tray (entries from before 1.1 had no `--background`). A
/// missing entry stays missing.
pub fn refresh_run_key() -> Result<()> {
    if crate::package::is_packaged() || !run_key_exists() {
        return Ok(());
    }
    run_key(true).map(|_| ())
}

fn run_key_exists() -> bool {
    let key = wide(RUN_KEY);
    let name = wide(RUN_VALUE);
    let mut size = 0u32;
    // SAFETY: asks for the size of one string value under the user's own Run key; no data is
    // read, and both names are NUL-terminated.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&raw mut size),
        )
    };
    r == ERROR_SUCCESS
}

/// Turns starting with Windows on or off.
pub fn set(enabled: bool) -> Result<Startup> {
    if crate::package::is_packaged() {
        startup_task(enabled)
    } else {
        run_key(enabled)
    }
}

/// The package's startup task, asked on a thread of its own: the WinRT calls wait for Windows,
/// and the engine's thread (hooks, overlays, shortcuts) must never wait with them.
fn startup_task(enabled: bool) -> Result<Startup> {
    let (send, answer) = mpsc::channel();
    std::thread::Builder::new()
        .name("mosaik-startup-task".into())
        .spawn(move || {
            let _ = send.send(ask_startup_task(enabled));
        })
        .map_err(|e| Error::Other(format!("could not ask Windows ({e})")))?;
    answer
        .recv_timeout(TASK_TIMEOUT)
        .map_err(|_| Error::Other("Windows did not answer in time; try again".into()))?
}

fn ask_startup_task(enabled: bool) -> Result<Startup> {
    fn plain(what: &'static str) -> impl Fn(windows::core::Error) -> Error {
        move |e| Error::Other(format!("{what} ({})", e.message()))
    }
    let task = StartupTask::GetAsync(&HSTRING::from(TASK_ID))
        .and_then(|op| op.join())
        .map_err(plain("the package's startup task was not found"))?;
    if !enabled {
        task.Disable()
            .map_err(plain("the startup task could not be turned off"))?;
        return Ok(Startup::Off);
    }
    let state = task
        .RequestEnableAsync()
        .and_then(|op| op.join())
        .map_err(plain("the startup task could not be turned on"))?;
    Ok(match state {
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Startup::On,
        _ => Startup::Refused,
    })
}

fn run_key(enabled: bool) -> Result<Startup> {
    let key = wide(RUN_KEY);
    let name = wide(RUN_VALUE);
    if !enabled {
        // SAFETY: deletes one value under the user's own Run key; both strings are NUL-terminated.
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), PCWSTR(name.as_ptr())) };
        return if r == ERROR_SUCCESS || r == ERROR_FILE_NOT_FOUND {
            Ok(Startup::Off)
        } else {
            Err(Error::Other(format!(
                "the Run entry could not be removed (error {})",
                r.0
            )))
        };
    }
    let exe =
        std::env::current_exe().map_err(|e| Error::Other(format!("Mosaïk's own path is unknown: {e}")))?;
    let command = wide(&run_command(&exe));
    let bytes = u32::try_from(command.len() * 2).map_err(|_| Error::Other("path too long".into()))?;
    // SAFETY: writes one string value under the user's own Run key; the data is the
    // NUL-terminated UTF-16 command line and `bytes` is its size including the NUL.
    let r = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes,
        )
    };
    if r == ERROR_SUCCESS {
        Ok(Startup::On)
    } else {
        Err(Error::Other(format!(
            "the Run entry could not be written (error {})",
            r.0
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn started_by_the_run_key_it_waits_in_the_tray() {
        let args: Vec<String> = ["mosaik.exe", BACKGROUND].map(String::from).into();
        assert!(started_with_windows(&args));
    }

    #[test]
    fn started_by_hand_it_shows_its_window() {
        // A build run from a folder (not the package): only the flag tells.
        let args: Vec<String> = vec!["mosaik.exe".into()];
        assert!(!started_with_windows(&args));
        let args: Vec<String> = ["mosaik.exe", "--safe-mode"].map(String::from).into();
        assert!(!started_with_windows(&args));
    }

    #[test]
    fn the_run_key_starts_it_in_the_tray() {
        let command = run_command(Path::new(r"C:\Program Files\Mosaik\mosaik.exe"));
        assert_eq!(command, r#""C:\Program Files\Mosaik\mosaik.exe" --background"#);
    }
}
