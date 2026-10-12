//! Starting programs and measuring processes (memory, CPU time).

use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, GetCurrentProcessId, GetExitCodeProcess, GetProcessTimes,
    OpenProcess, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
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

    /// The exit code once the process has ended; `None` while it runs.
    pub fn exit_code(&self) -> Option<u32> {
        let mut code = 0u32;
        // SAFETY: valid process handle with query access; out pointer to a local.
        unsafe { GetExitCodeProcess(self.handle, &mut code) }.ok()?;
        // STILL_ACTIVE (259) means it has not ended.
        (code != 259).then_some(code)
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

/// The program file names of the processes running now in this user's session (lower case, each
/// once): one snapshot of the process list, without opening any process. Other sessions (another
/// user signed in on the same PC) are left out: their programs do not touch these windows.
pub fn running_programs() -> Result<std::collections::BTreeSet<String>> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let session = |pid: u32| {
        let mut id = 0u32;
        // SAFETY: reads a process's session id into a local; no handle is opened.
        unsafe { ProcessIdToSessionId(pid, &raw mut id) }
            .ok()
            .map(|()| id)
    };
    let own = session(own_pid());
    // SAFETY: a snapshot of the process list; the handle is closed below.
    let snapshot =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ctx("CreateToolhelp32Snapshot")?;
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0),
        ..Default::default()
    };
    let mut names = std::collections::BTreeSet::new();
    // SAFETY: `entry` is sized as the API requires and outlives each call.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) }.is_ok();
    while more {
        if own.is_some() && session(entry.th32ProcessID) == own {
            names.insert(crate::text::from_wide(&entry.szExeFile).to_lowercase());
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) }.is_ok();
    }
    // SAFETY: the snapshot handle was opened above.
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(names)
}

/// The ids of the processes running now: one snapshot of the process list.
pub fn running_pids() -> Result<std::collections::BTreeSet<u32>> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    // SAFETY: a snapshot of the process list; the handle is closed below.
    let snapshot =
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ctx("CreateToolhelp32Snapshot")?;
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0),
        ..Default::default()
    };
    let mut pids = std::collections::BTreeSet::new();
    // SAFETY: `entry` is sized as the API requires and outlives each call.
    let mut more = unsafe { Process32FirstW(snapshot, &raw mut entry) }.is_ok();
    while more {
        pids.insert(entry.th32ProcessID);
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &raw mut entry) }.is_ok();
    }
    // SAFETY: the snapshot handle was opened above.
    let _ = unsafe { CloseHandle(snapshot) };
    Ok(pids)
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
