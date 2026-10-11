//! Starting programs and measuring processes (memory, CPU time).

use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetCurrentProcessId, GetProcessTimes, OpenProcess,
    PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, PWSTR};

use crate::error::{Context, Error, Result};
use crate::text::wide;

pub fn own_pid() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentProcessId() }
}

/// A process this code started or opened; its handle is closed when dropped.
pub struct Process {
    pub pid: u32,
    handle: HANDLE,
}

impl Process {
    /// Opens an existing process for waiting, measuring and (for our own children) ending it.
    pub fn open(pid: u32) -> Result<Self> {
        // SAFETY: opens a handle that `Drop` closes.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                false,
                pid,
            )
            .or_else(|_| {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    pid,
                )
            })
        }
        .ctx("OpenProcess")?;
        Ok(Self { pid, handle })
    }

    pub fn handle(&self) -> HANDLE {
        self.handle
    }

    /// Waits up to `ms` for the process to exit; true if it did.
    pub fn wait(&self, ms: u32) -> bool {
        // SAFETY: valid process handle with SYNCHRONIZE access.
        unsafe { WaitForSingleObject(self.handle, ms) == WAIT_OBJECT_0 }
    }

    pub fn kill(&self) -> Result<()> {
        // SAFETY: valid process handle; only used on processes this code started.
        unsafe { TerminateProcess(self.handle, 1) }.ctx("TerminateProcess")
    }

    /// Private bytes and working set, in bytes.
    pub fn memory(&self) -> Option<(usize, usize)> {
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(0);
        c.cb = size;
        // SAFETY: the EX struct starts with the basic one and its size is passed.
        unsafe { GetProcessMemoryInfo(self.handle, (&raw mut c).cast::<PROCESS_MEMORY_COUNTERS>(), size) }
            .ok()?;
        Some((c.PrivateUsage, c.WorkingSetSize))
    }

    /// CPU time used so far (user + kernel).
    pub fn cpu_time(&self) -> Option<Duration> {
        let (mut a, mut b, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: valid out pointers.
        unsafe { GetProcessTimes(self.handle, &mut a, &mut b, &mut k, &mut u) }.ok()?;
        let ticks = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
        // FILETIME counts 100 ns units.
        Some(Duration::from_nanos((ticks(k) + ticks(u)) * 100))
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this value and closed once.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Starts `program` with `args` (a command-line tail) and returns the new process.
pub fn spawn(program: &str, args: &str) -> Result<Process> {
    let mut command = wide(&format!("\"{program}\" {args}"));
    let si = STARTUPINFOW {
        cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(0),
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: the command line buffer is mutable as CreateProcessW requires; handles in `pi`
    // are closed (thread) or kept by `Process` (process).
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )
        .ctx("CreateProcessW")?;
        let _ = CloseHandle(pi.hThread);
    }
    Ok(Process {
        pid: pi.dwProcessId,
        handle: pi.hProcess,
    })
}

/// Opens a file, program or URI the way Explorer would (packaged apps, `ms-settings:`...).
pub fn shell_open(target: &str, args: &str) -> Result<()> {
    let (t, a, verb) = (wide(target), wide(args), wide("open"));
    // SAFETY: NUL-terminated strings that outlive the call.
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(t.as_ptr()),
            PCWSTR(a.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute returns a value greater than 32 on success.
    if r.0 as usize > 32 {
        Ok(())
    } else {
        Err(Error::Other(format!(
            "could not open {target} (code {})",
            r.0 as usize
        )))
    }
}
