//! The per-user named pipe: `\\.\pipe\Mosaik-<session>-<user SID>`.
//!
//! Who may talk to whom:
//!
//! - The server's pipe is owned by this user, grants access to this user (and SYSTEM) only,
//!   carries the server's own integrity level as a no-read-up, no-write-up label (a sandboxed,
//!   low-integrity process cannot open it, and a normal process cannot drive an elevated
//!   Mosaïk), refuses remote clients, and claims the name as "first instance" so nobody else
//!   can serve on it while Mosaïk runs.
//! - A client opens the pipe for identification only (the server may learn who the client is,
//!   never act as it), then checks the pipe before sending anything: owned by this user, at
//!   medium integrity or above, served from this session, by the expected program. A program
//!   that took the name first (another account, a service, a sandboxed process) fails these
//!   checks and gets nothing.
//!
//! Every connection gets its own pipe instance, used once, and all I/O has a deadline: a client
//! that stalls ties up nothing but its own instance, and an engine that stops answering gives
//! the client an error instead of a hang.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mosaik_win::process::own_pid;
use mosaik_win::text::{from_wide, wide};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING,
    ERROR_NO_DATA, ERROR_OPERATION_ABORTED, ERROR_PATH_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_NOT_CONNECTED, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree,
    WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    ACE_HEADER, ACL, EqualSid, GetAce, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
    LABEL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    SID_AND_ATTRIBUTES, SYSTEM_MANDATORY_LABEL_ACE, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
    TokenIntegrityLevel, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, GetNamedPipeServerSessionId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ResetEvent, SetEvent,
    WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR};

use crate::protocol::{
    Command, ErrorCode, LineBuffer, ProtocolError, Request, Response, decode_request, decode_response,
    encode, request_id,
};

/// What went wrong with the connection, in words the caller can turn into a next step.
#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("Mosaïk is not running")]
    NotRunning,
    #[error("another program already serves on Mosaïk's connection ({holder})")]
    NameTaken { holder: String },
    #[error("the program on Mosaïk's connection failed a safety check: {0}")]
    Impostor(String),
    #[error(
        "Windows refused the connection: Mosaïk may be running as administrator, or another account's program holds its connection"
    )]
    Denied,
    #[error("Mosaïk is busy with other requests")]
    Busy,
    #[error("no answer within {} s", .0.as_secs_f32())]
    TimedOut(Duration),
    #[error("the connection closed")]
    Closed,
    #[error("stopped")]
    Stopped,
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("{call} failed: {source}")]
    Win32 {
        call: &'static str,
        source: windows::core::Error,
    },
}

pub type Result<T> = std::result::Result<T, PipeError>;

fn win(call: &'static str) -> impl FnOnce(windows::core::Error) -> PipeError {
    move |source| PipeError::Win32 { call, source }
}

/// Win32 errors that mean the other end has gone.
fn is_closed(e: &windows::core::Error) -> bool {
    [ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED]
        .iter()
        .any(|c| e.code() == c.to_hresult())
}

/// An owned kernel handle, closed once when dropped.
struct Handle(HANDLE);

// SAFETY: a kernel handle is not tied to the thread that opened it, and `Handle` owns it
// exclusively, so moving it to another thread (each connection is served on its own) is sound.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: owned handle, closed once.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

/// A manual-reset event.
struct Event(Handle);

impl Event {
    fn new() -> Result<Self> {
        // SAFETY: an unnamed event with default security; owned by the returned value.
        let h = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(win("CreateEventW"))?;
        Ok(Self(Handle(h)))
    }
}

// SAFETY: setting, resetting and waiting on an event are thread-safe kernel operations; the
// handle stays open as long as any owner holds the `Arc` around it.
unsafe impl Sync for Event {}

// ---------------------------------------------------------------------------------------------
// Accounts, integrity levels, security descriptors.

/// `SYSTEM_MANDATORY_LABEL_ACE_TYPE` from winnt.h.
const MANDATORY_LABEL_ACE: u8 = 0x11;
/// `SECURITY_MANDATORY_MEDIUM_RID` from winnt.h: a normal, unelevated, unsandboxed program.
const MEDIUM_INTEGRITY: u32 = 0x2000;

/// Token information, in an 8-byte aligned buffer so the structures inside can be read in place.
struct TokenInfo {
    buf: Vec<u64>,
}

impl TokenInfo {
    fn query(class: TOKEN_INFORMATION_CLASS) -> Result<Self> {
        // SAFETY: our own process token, closed when `token` drops; the first call only reports
        // the size, the second fills a buffer at least that large.
        unsafe {
            let mut raw = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw).map_err(win("OpenProcessToken"))?;
            let token = Handle(raw);
            let mut len = 0u32;
            let _ = GetTokenInformation(token.0, class, None, 0, &mut len);
            let words = (len as usize).div_ceil(8).max(8);
            let mut buf = vec![0u64; words];
            let bytes = u32::try_from(words * 8).unwrap_or(u32::MAX);
            GetTokenInformation(token.0, class, Some(buf.as_mut_ptr().cast()), bytes, &mut len)
                .map_err(win("GetTokenInformation"))?;
            Ok(Self { buf })
        }
    }

    /// The SID of a `TOKEN_USER` or `TOKEN_MANDATORY_LABEL`: both start with a
    /// `SID_AND_ATTRIBUTES` whose SID points into this buffer.
    fn sid(&self) -> PSID {
        // SAFETY: the buffer is 8-byte aligned, at least as large as SID_AND_ATTRIBUTES, and was
        // filled by GetTokenInformation with one of the two structures above.
        unsafe { (*self.buf.as_ptr().cast::<SID_AND_ATTRIBUTES>()).Sid }
    }
}

fn sid_string(sid: PSID) -> Result<String> {
    // SAFETY: `sid` is a valid SID; the string is freed with LocalFree as the API requires.
    unsafe {
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(sid, &mut s).map_err(win("ConvertSidToStringSidW"))?;
        let text = s.to_string().map_err(|_| PipeError::Win32 {
            call: "ConvertSidToStringSidW",
            source: windows::core::Error::from_thread(),
        });
        let _ = LocalFree(Some(HLOCAL(s.0.cast::<c_void>())));
        text
    }
}

fn session() -> Result<u32> {
    let mut session = 0u32;
    // SAFETY: valid out pointer for our own process id.
    unsafe { ProcessIdToSessionId(own_pid(), &mut session) }.map_err(win("ProcessIdToSessionId"))?;
    Ok(session)
}

/// The pipe name for the current user in the current session.
pub fn pipe_name() -> Result<String> {
    let user = TokenInfo::query(TokenUser)?;
    Ok(format!(
        r"\\.\pipe\Mosaik-{}-{}",
        session()?,
        sid_string(user.sid())?
    ))
}

/// Owner: this user. DACL (protected from inheritance): full access for this user and SYSTEM.
/// SACL: a label at this process's own integrity level that refuses reads and writes from below.
fn server_sddl() -> Result<String> {
    let user = sid_string(TokenInfo::query(TokenUser)?.sid())?;
    let level = sid_string(TokenInfo::query(TokenIntegrityLevel)?.sid())?;
    Ok(format!(
        "O:{user}D:P(A;;GA;;;{user})(A;;GA;;;SY)S:(ML;;NWNR;;;{level})"
    ))
}

/// A security descriptor made from SDDL, freed when dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn from_sddl(sddl: &str) -> Result<Self> {
        let text = wide(sddl);
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the string outlives the call; the descriptor is freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(text.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
        }
        .map_err(win("ConvertStringSecurityDescriptorToSecurityDescriptorW"))?;
        Ok(Self(sd))
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by the system with LocalAlloc, freed once.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

// SAFETY: the descriptor is plain memory owned by this value and only read after creation.
unsafe impl Send for Descriptor {}

/// The integrity level in a mandatory label SACL; `None` when there is no label (Windows then
/// treats the object as medium).
fn label_level(sacl: *const ACL) -> Option<u32> {
    if sacl.is_null() {
        return None;
    }
    // SAFETY: `sacl` points to a valid ACL returned by GetSecurityInfo and still owned by its
    // security descriptor; each ACE is read through GetAce, and a label ACE's SID starts at its
    // SidStart field.
    unsafe {
        for i in 0..u32::from((*sacl).AceCount) {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if GetAce(sacl, i, &mut ace).is_err() || ace.is_null() {
                continue;
            }
            if (*ace.cast::<ACE_HEADER>()).AceType != MANDATORY_LABEL_ACE {
                continue;
            }
            let label = ace.cast::<SYSTEM_MANDATORY_LABEL_ACE>();
            let sid = PSID((&raw mut (*label).SidStart).cast::<c_void>());
            let count = *GetSidSubAuthorityCount(sid);
            if count == 0 {
                return None;
            }
            return Some(*GetSidSubAuthority(sid, u32::from(count) - 1));
        }
    }
    None
}

/// The full path of the program serving a pipe.
fn server_image(pipe: HANDLE) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: valid pipe handle and out pointers; the process handle is closed when dropped.
    unsafe {
        GetNamedPipeServerProcessId(pipe, &mut pid).ok()?;
        let p = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut buf = [0u16; 1024];
        let mut len = 1024u32;
        QueryFullProcessImageNameW(p.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok()?;
        Some(from_wide(&buf[..len as usize]))
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

/// The program a client expects on the other end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// This exact file (the Mosaïk installed next to `mosaikctl`).
    Path(PathBuf),
    /// Any program with this file name.
    Name(String),
}

impl Program {
    /// `mosaik.exe` next to the running program when there is one, else any `mosaik.exe`.
    pub fn mosaik() -> Self {
        std::env::current_exe()
            .ok()
            .map(|exe| exe.with_file_name("mosaik.exe"))
            .filter(|p| p.is_file())
            .map_or_else(|| Self::Name("mosaik.exe".into()), Self::Path)
    }

    fn accepts(&self, image: &str) -> bool {
        match self {
            Self::Path(p) => paths_equal(p, image),
            Self::Name(n) => file_name(image).eq_ignore_ascii_case(n),
        }
    }
}

fn paths_equal(expected: &Path, image: &str) -> bool {
    let expected = expected.to_string_lossy();
    let strip = |s: &str| s.trim_start_matches(r"\\?\").to_owned();
    strip(&expected).eq_ignore_ascii_case(&strip(image))
}

/// Checks the other end of a freshly opened pipe before anything is sent to it.
fn verify_server(pipe: HANDLE, program: &Program) -> Result<()> {
    let mut server_session = 0u32;
    // SAFETY: valid pipe handle and out pointer.
    unsafe { GetNamedPipeServerSessionId(pipe, &mut server_session) }
        .map_err(|_| PipeError::Impostor("its session could not be checked".into()))?;
    if server_session != session()? {
        return Err(PipeError::Impostor("it runs in another Windows session".into()));
    }
    let me = TokenInfo::query(TokenUser)?;
    let mut owner = PSID::default();
    let mut sacl: *mut ACL = std::ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: valid handle opened with GENERIC_READ (which includes READ_CONTROL, enough for the
    // owner and the label); the returned descriptor owns `owner` and `sacl` and is freed below.
    let status = unsafe {
        GetSecurityInfo(
            pipe,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            Some(&mut sacl),
            Some(&mut sd),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(PipeError::Impostor("its security could not be read".into()));
    }
    let descriptor = Descriptor(sd);
    // SAFETY: both SIDs are valid: one from our token, one inside `descriptor`.
    let owned_by_me = unsafe { EqualSid(owner, me.sid()) }.is_ok();
    let level = label_level(sacl).unwrap_or(MEDIUM_INTEGRITY);
    drop(descriptor);
    if !owned_by_me {
        return Err(PipeError::Impostor("it belongs to another account".into()));
    }
    if level < MEDIUM_INTEGRITY {
        return Err(PipeError::Impostor("it runs sandboxed, at low integrity".into()));
    }
    let image = server_image(pipe).unwrap_or_default();
    if !program.accepts(&image) {
        return Err(PipeError::Impostor(impostor(&image)));
    }
    Ok(())
}

/// Why the program serving the pipe, at `image`, is not the one expected, in words: another
/// copy of Mosaïk (the Store's, while a build runs from a folder) is named as such, with where it
/// is, rather than as a stranger with Mosaïk's own file name.
fn impostor(image: &str) -> String {
    if image.is_empty() {
        "it is an unknown program, not Mosaïk".to_owned()
    } else if file_name(image).eq_ignore_ascii_case("mosaik.exe") {
        format!("it is another copy of Mosaïk ({image}), not this one")
    } else {
        format!("it is {}, not Mosaïk", file_name(image))
    }
}

// ---------------------------------------------------------------------------------------------
// Overlapped I/O with deadlines.

fn remaining_ms(deadline: Instant) -> u32 {
    u32::try_from(deadline.saturating_duration_since(Instant::now()).as_millis()).unwrap_or(u32::MAX)
}

/// One end of a pipe opened for overlapped I/O, with the event its operations signal.
struct Pipe {
    handle: Handle,
    event: Event,
}

impl Pipe {
    fn new(handle: HANDLE) -> Result<Self> {
        let handle = Handle(handle);
        Ok(Self {
            handle,
            event: Event::new()?,
        })
    }

    /// Runs one overlapped operation to completion, or cancels it at the deadline. Returns the
    /// bytes transferred. The OVERLAPPED and the caller's buffer stay alive until the operation
    /// is over, cancelled or not.
    fn complete(
        &self,
        call: &'static str,
        deadline: Instant,
        timeout: Duration,
        start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
    ) -> Result<u32> {
        let h = self.handle.0;
        let event = self.event.0.0;
        let mut ov = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        // SAFETY: our own event.
        unsafe { ResetEvent(event) }.map_err(win("ResetEvent"))?;
        match start(&raw mut ov) {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                // SAFETY: waits on our own event.
                let waited = unsafe { WaitForSingleObject(event, remaining_ms(deadline)) };
                if waited != WAIT_OBJECT_0 {
                    // SAFETY: cancels this one operation, then waits until it is really over so
                    // `ov` and the buffer are no longer in use when this function returns.
                    unsafe {
                        let _ = CancelIoEx(h, Some(&raw const ov));
                        let mut n = 0u32;
                        let _ = GetOverlappedResult(h, &raw const ov, &mut n, true);
                    }
                    return Err(if waited == WAIT_TIMEOUT {
                        PipeError::TimedOut(timeout)
                    } else {
                        PipeError::Win32 {
                            call: "WaitForSingleObject",
                            source: windows::core::Error::from_thread(),
                        }
                    });
                }
            }
            Err(e) if is_closed(&e) => return Err(PipeError::Closed),
            Err(e) => return Err(PipeError::Win32 { call, source: e }),
        }
        let mut n = 0u32;
        // SAFETY: the operation has completed (synchronously or signalled); `ov` is ours.
        match unsafe { GetOverlappedResult(h, &raw const ov, &mut n, true) } {
            Ok(()) => Ok(n),
            Err(e) if is_closed(&e) || e.code() == ERROR_OPERATION_ABORTED.to_hresult() => {
                Err(PipeError::Closed)
            }
            Err(e) => Err(PipeError::Win32 { call, source: e }),
        }
    }

    fn write_all(&self, bytes: &[u8], timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        let mut sent = 0usize;
        while sent < bytes.len() {
            let chunk = &bytes[sent..];
            let n = self.complete("WriteFile", deadline, timeout, |ov| {
                // SAFETY: `chunk` and `ov` outlive the operation (see `complete`).
                unsafe { WriteFile(self.handle.0, Some(chunk), None, Some(ov)) }
            })?;
            if n == 0 {
                return Err(PipeError::Closed);
            }
            sent += n as usize;
        }
        Ok(())
    }

    /// Reads until one complete line is available. `Ok(None)`: the other end closed the pipe.
    fn read_line(&self, buffer: &mut LineBuffer, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(line) = buffer.next_line()? {
                return Ok(Some(line));
            }
            let mut chunk = [0u8; 8192];
            let read = self.complete("ReadFile", deadline, timeout, |ov| {
                // SAFETY: `chunk` and `ov` outlive the operation (see `complete`).
                unsafe { ReadFile(self.handle.0, Some(&mut chunk), None, Some(ov)) }
            });
            let n = match read {
                Ok(n) => n,
                Err(PipeError::Closed) => return Ok(None),
                Err(e) => return Err(e),
            };
            if n == 0 {
                return Ok(None);
            }
            buffer.push(&chunk[..n as usize]);
        }
    }

    /// Before closing from this end: lets the other end read what was sent last, by waiting
    /// (briefly) for it to close first.
    fn linger(&self, timeout: Duration) {
        let mut sink = LineBuffer::default();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            match self.read_line(&mut sink, deadline.saturating_duration_since(Instant::now())) {
                Ok(Some(_)) => continue,
                Ok(None) | Err(_) => return,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Server.

/// Wakes a server blocked in [`Server::accept`] so its thread can end.
#[derive(Clone)]
pub struct Stopper(Arc<Event>);

impl Stopper {
    pub fn stop(&self) {
        // SAFETY: signals our own event.
        let _ = unsafe { SetEvent(self.0.0.0) };
    }
}

/// How long a connection may stay quiet, and how long a reply may take to be taken.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub idle: Duration,
    pub write: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(30),
            write: Duration::from_secs(5),
        }
    }
}

/// How a connection ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The client closed its end: the normal case.
    ClientLeft,
    /// The handler asked to stop (after answering).
    StopRequested,
    /// Nothing arrived within the idle limit.
    Idle,
    /// The client broke the protocol or did not take its reply; it was told when possible.
    Dropped(String),
}

/// Owns the pipe name while Mosaïk runs. Each client gets its own instance.
pub struct Server {
    name: Vec<u16>,
    descriptor: Descriptor,
    waiting: Option<Pipe>,
    stop: Arc<Event>,
}

/// Instances at once: more than Mosaïk's own clients ever need, few enough to bound the threads.
const MAX_INSTANCES: u32 = 16;

impl Server {
    /// Claims the pipe name. Fails with [`PipeError::NameTaken`] when another process already
    /// serves on it: usually Mosaïk itself, already running for this user and session.
    pub fn bind(name: &str) -> Result<Self> {
        let descriptor = Descriptor::from_sddl(&server_sddl()?)?;
        let wname = wide(name);
        let stop = Arc::new(Event::new()?);
        let mut server = Self {
            name: wname,
            descriptor,
            waiting: None,
            stop,
        };
        match server.instance(true) {
            Ok(first) => server.waiting = Some(first),
            Err(PipeError::Win32 { source, .. }) if source.code() == ERROR_ACCESS_DENIED.to_hresult() => {
                return Err(PipeError::NameTaken { holder: holder(name) });
            }
            Err(e) => return Err(e),
        }
        Ok(server)
    }

    pub fn stopper(&self) -> Stopper {
        Stopper(Arc::clone(&self.stop))
    }

    fn instance(&self, first: bool) -> Result<Pipe> {
        let sa = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: self.descriptor.0.0,
            bInheritHandle: false.into(),
        };
        let mut mode = PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_OVERLAPPED.0;
        if first {
            mode |= FILE_FLAG_FIRST_PIPE_INSTANCE.0;
        }
        // SAFETY: name and security attributes outlive the call; the handle is owned by `Pipe`.
        let h = unsafe {
            CreateNamedPipeW(
                PCWSTR(self.name.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(mode),
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                MAX_INSTANCES,
                65536,
                65536,
                0,
                Some(&raw const sa),
            )
        };
        if h.is_invalid() {
            // Read the error before anything else can overwrite it.
            return Err(PipeError::Win32 {
                call: "CreateNamedPipeW",
                source: windows::core::Error::from_thread(),
            });
        }
        Pipe::new(h)
    }

    /// Waits for the next client. `Ok(None)` once stopped. The next instance is created before
    /// returning, so other clients can connect while this one is served (on its own thread).
    pub fn accept(&mut self) -> Result<Option<Connection>> {
        loop {
            let pipe = match self.waiting.take() {
                Some(p) => p,
                None => match self.instance(false) {
                    Ok(p) => p,
                    Err(PipeError::Win32 { source, .. }) if source.code() == ERROR_PIPE_BUSY.to_hresult() => {
                        // Every instance is serving a client: wait for one to finish.
                        // SAFETY: waits on our own stop event.
                        if unsafe { WaitForSingleObject(self.stop.0.0, 100) } == WAIT_OBJECT_0 {
                            return Ok(None);
                        }
                        continue;
                    }
                    Err(e) => return Err(e),
                },
            };
            if self.wait_for_client(&pipe)? {
                self.waiting = self.instance(false).ok();
                return Ok(Some(Connection { pipe }));
            }
            self.waiting = Some(pipe);
            return Ok(None);
        }
    }

    /// True when a client connected, false when stopped.
    fn wait_for_client(&self, pipe: &Pipe) -> Result<bool> {
        let h = pipe.handle.0;
        let event = pipe.event.0.0;
        let mut ov = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        // SAFETY: our own event and pipe; `ov` lives until the connect completes or is
        // cancelled and waited for below.
        unsafe {
            ResetEvent(event).map_err(win("ResetEvent"))?;
            match ConnectNamedPipe(h, Some(&raw mut ov)) {
                Ok(()) => return Ok(true),
                Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => return Ok(true),
                // A client that connected and left before we looked: this instance is spent.
                Err(e) if e.code() == ERROR_NO_DATA.to_hresult() => return Ok(true),
                Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {}
                Err(e) => {
                    return Err(PipeError::Win32 {
                        call: "ConnectNamedPipe",
                        source: e,
                    });
                }
            }
            let handles = [event, self.stop.0.0];
            let woke: WAIT_EVENT = WaitForMultipleObjects(&handles, false, u32::MAX);
            if woke == WAIT_OBJECT_0 {
                let mut n = 0u32;
                return match GetOverlappedResult(h, &raw const ov, &mut n, true) {
                    Ok(()) => Ok(true),
                    Err(e) if e.code() == ERROR_NO_DATA.to_hresult() => Ok(true),
                    Err(e) => Err(PipeError::Win32 {
                        call: "ConnectNamedPipe",
                        source: e,
                    }),
                };
            }
            let _ = CancelIoEx(h, Some(&raw const ov));
            let mut n = 0u32;
            let _ = GetOverlappedResult(h, &raw const ov, &mut n, true);
            Ok(false)
        }
    }
}

/// Names whoever serves on `name`, for the "already running" message. Connects for
/// identification only and sends nothing.
fn holder(name: &str) -> String {
    let wname = wide(name);
    // SAFETY: opens the pipe for identification only; the handle is closed when dropped.
    let opened = unsafe {
        CreateFileW(
            PCWSTR(wname.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            None,
        )
    };
    match opened {
        Ok(h) => {
            let h = Handle(h);
            let mut pid = 0u32;
            // SAFETY: valid pipe handle and out pointer.
            let _ = unsafe { GetNamedPipeServerProcessId(h.0, &mut pid) };
            match server_image(h.0) {
                Some(image) => format!("{}, process {pid}", file_name(&image)),
                None => format!("process {pid}"),
            }
        }
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => "a program of another account".into(),
        Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() => "a program that is busy".into(),
        Err(_) => "an unknown program".into(),
    }
}

/// One client's connection, served on its own thread.
pub struct Connection {
    pipe: Pipe,
}

impl Connection {
    /// Answers requests until the client leaves, the handler asks to stop, the client stays
    /// quiet too long, or it breaks the protocol (it is told why when possible). The instance
    /// is closed when this returns.
    pub fn serve(self, handler: &mut dyn FnMut(Request) -> (Response, bool), limits: Limits) -> Ended {
        let mut buffer = LineBuffer::default();
        loop {
            let line = match self.pipe.read_line(&mut buffer, limits.idle) {
                Ok(Some(line)) => line,
                Ok(None) => return Ended::ClientLeft,
                Err(PipeError::TimedOut(_)) => return Ended::Idle,
                Err(PipeError::Protocol(e)) => {
                    let reply = Response::error(0, ErrorCode::BadRequest, e.to_string(), None);
                    if let Ok(bytes) = encode(&reply) {
                        let _ = self.pipe.write_all(&bytes, limits.write);
                    }
                    self.pipe.linger(Duration::from_secs(1));
                    return Ended::Dropped(e.to_string());
                }
                Err(e) => return Ended::Dropped(e.to_string()),
            };
            let (response, stop) = match decode_request(&line) {
                Ok(request) => handler(request),
                Err(e) => (protocol_error(request_id(&line), &e), false),
            };
            let bytes = match encode(&response) {
                Ok(b) => b,
                Err(e) => {
                    let failed = Response::error(
                        response.id,
                        ErrorCode::Failed,
                        format!("the reply could not be sent: {e}"),
                        None,
                    );
                    match encode(&failed) {
                        Ok(b) => b,
                        Err(e) => return Ended::Dropped(e.to_string()),
                    }
                }
            };
            if let Err(e) = self.pipe.write_all(&bytes, limits.write) {
                return Ended::Dropped(format!("the client did not take its reply: {e}"));
            }
            if stop {
                self.pipe.linger(Duration::from_secs(1));
                return Ended::StopRequested;
            }
        }
    }
}

fn protocol_error(id: u64, e: &ProtocolError) -> Response {
    match e {
        ProtocolError::UnsupportedVersion { .. } => {
            Response::error(id, ErrorCode::UnsupportedVersion, e.to_string(), None)
        }
        _ => Response::error(id, ErrorCode::BadRequest, e.to_string(), None),
    }
}

// ---------------------------------------------------------------------------------------------
// Client.

/// A connection to the running Mosaïk.
pub struct Client {
    pipe: Pipe,
    buffer: LineBuffer,
    next_id: u64,
}

impl Client {
    /// Connects to Mosaïk's pipe within `timeout`, and checks the other end (see the module
    /// documentation) before anything is sent.
    pub fn connect(name: &str, program: &Program, timeout: Duration) -> Result<Self> {
        let wname = wide(name);
        let deadline = Instant::now() + timeout;
        let handle = loop {
            // SAFETY: opens the pipe for overlapped I/O, identification-only impersonation; the
            // handle is owned by `Pipe` below.
            let opened = unsafe {
                CreateFileW(
                    PCWSTR(wname.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
            };
            match opened {
                Ok(h) => break h,
                Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() => {
                    if Instant::now() >= deadline {
                        return Err(PipeError::Busy);
                    }
                    // SAFETY: waits for a free instance, at most until the deadline.
                    let _ = unsafe { WaitNamedPipeW(PCWSTR(wname.as_ptr()), remaining_ms(deadline).max(1)) };
                }
                Err(e)
                    if e.code() == ERROR_FILE_NOT_FOUND.to_hresult()
                        || e.code() == ERROR_PATH_NOT_FOUND.to_hresult() =>
                {
                    return Err(PipeError::NotRunning);
                }
                Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => return Err(PipeError::Denied),
                Err(e) => {
                    return Err(PipeError::Win32 {
                        call: "CreateFileW",
                        source: e,
                    });
                }
            }
        };
        let pipe = Pipe::new(handle)?;
        verify_server(pipe.handle.0, program)?;
        Ok(Self {
            pipe,
            buffer: LineBuffer::default(),
            next_id: 1,
        })
    }

    /// Sends one request and waits up to `timeout` for its reply.
    pub fn request(&mut self, command: Command, timeout: Duration) -> Result<Response> {
        let id = self.next_id;
        self.next_id += 1;
        let bytes = encode(&Request::new(id, command))?;
        self.pipe.write_all(&bytes, timeout)?;
        let line = self
            .pipe
            .read_line(&mut self.buffer, timeout)?
            .ok_or(PipeError::Closed)?;
        let response = decode_response(&line)?;
        // A reply to request 0 is the server refusing a line it could not read at all.
        if response.id != id && response.id != 0 {
            return Err(PipeError::Protocol(ProtocolError::Invalid(format!(
                "the reply is for request {}, not {id}",
                response.id
            ))));
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::impostor;

    #[test]
    fn another_copy_of_mosaik_is_named_as_one() {
        let store = r"C:\Program Files\WindowsApps\Gimzdev.Mosak_1.0.0.0_x64__yjd3d5zpwkx98\mosaik.exe";
        assert_eq!(
            impostor(store),
            format!("it is another copy of Mosaïk ({store}), not this one")
        );
        assert_eq!(impostor(r"C:\Tools\spy.exe"), "it is spy.exe, not Mosaïk");
        assert_eq!(impostor(""), "it is an unknown program, not Mosaïk");
    }
}
