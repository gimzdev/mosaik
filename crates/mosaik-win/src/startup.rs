//! Starting Mosaïk with Windows, the way that fits how it was installed: the Store package's
//! startup task (which the user can also turn off in Task Manager), or for a build run from a
//! folder, an entry in the user's own "Run" key. Nothing needs administrator rights.

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};
use windows::core::{HSTRING, PCWSTR};

use crate::error::{Context, Error, Result};
use crate::text::wide;

/// The startup task declared in the package manifest.
const TASK_ID: &str = "MosaikStartup";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Mosaik";

/// Whether this process runs from the Store package.
pub fn is_packaged() -> bool {
    let mut len = 0u32;
    // SAFETY: asks for the length only; no buffer is written.
    let r = unsafe { GetCurrentPackageFullName(&mut len, None) };
    r != APPMODEL_ERROR_NO_PACKAGE
}

/// How starting with Windows ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Startup {
    On,
    Off,
    /// The user turned it off in Task Manager (or a policy did): only they can turn it back on
    /// there.
    OffByUser,
}

/// Turns starting with Windows on or off.
pub fn set(enabled: bool) -> Result<Startup> {
    if is_packaged() {
        startup_task(enabled)
    } else {
        run_key(enabled)
    }
}

fn startup_task(enabled: bool) -> Result<Startup> {
    let task = StartupTask::GetAsync(&HSTRING::from(TASK_ID))
        .and_then(|op| op.join())
        .ctx("StartupTask.GetAsync")?;
    if !enabled {
        task.Disable().ctx("StartupTask.Disable")?;
        return Ok(Startup::Off);
    }
    let state = task
        .RequestEnableAsync()
        .and_then(|op| op.join())
        .ctx("StartupTask.RequestEnableAsync")?;
    Ok(match state {
        StartupTaskState::Enabled | StartupTaskState::EnabledByPolicy => Startup::On,
        StartupTaskState::DisabledByUser | StartupTaskState::DisabledByPolicy => Startup::OffByUser,
        _ => Startup::Off,
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
