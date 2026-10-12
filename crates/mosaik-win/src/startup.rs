//! Starting Mosaïk with Windows, the way that fits how it was installed: the Store package's
//! startup task (which the user can also turn off in Task Manager), or for a build run from a
//! folder, an entry in the user's own "Run" key. Nothing needs administrator rights.

use std::sync::mpsc;
use std::time::Duration;

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
use windows::core::{HSTRING, PCWSTR};

use crate::error::{Error, Result};
use crate::text::wide;

/// The startup task declared in the package manifest (installer/msix).
const TASK_ID: &str = "MosaikStartup";
/// How long Windows may take to answer about the startup task.
const TASK_TIMEOUT: Duration = Duration::from_secs(5);
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Mosaik";

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
    let command = wide(&format!("\"{}\"", exe.display()));
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
