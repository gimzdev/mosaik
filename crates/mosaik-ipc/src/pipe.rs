//! The per-user named pipe: `\\.\pipe\Mosaik-<session>-<user SID>`.
//!
//! Only the user running Mosaïk (and SYSTEM) may open it, remote clients are refused, and the
//! server claims the name with "first instance" so another program cannot sit on it while
//! Mosaïk runs. A client checks the server is a Mosaïk process before trusting its answers.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use mosaik_win::error::{Context, Error, Result};
use mosaik_win::process::own_pid;
use mosaik_win::text::{from_wide, wide};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
    HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, FlushFileBuffers,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::core::{PCWSTR, PWSTR};

use crate::protocol::{
    LineBuffer, ProtocolError, Request, Response, decode_request, decode_response, encode,
};

/// The current user's SID as a string ("S-1-5-21-...").
fn user_sid() -> Result<String> {
    // SAFETY: token of our own process, closed below; the buffer holds a TOKEN_USER and its SID;
    // the string is freed with LocalFree as the API requires.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ctx("OpenProcessToken")?;
        let mut buf = [0u8; 256];
        let mut len = 0u32;
        let r = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), 256, &mut len);
        let _ = CloseHandle(token);
        r.ctx("GetTokenInformation(TokenUser)")?;
        let user = &*(buf.as_ptr().cast::<TOKEN_USER>());
        let mut s = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut s).ctx("ConvertSidToStringSidW")?;
        let text = s.to_string().map_err(|e| Error::Other(e.to_string()));
        let _ = LocalFree(Some(HLOCAL(s.0.cast::<c_void>())));
        text
    }
}

/// The pipe name for the current user in the current session.
pub fn pipe_name() -> Result<String> {
    let mut session = 0u32;
    // SAFETY: valid out pointer for our own process id.
    unsafe { ProcessIdToSessionId(own_pid(), &mut session) }.ctx("ProcessIdToSessionId")?;
    Ok(format!(r"\\.\pipe\Mosaik-{session}-{}", user_sid()?))
}

struct Handle(HANDLE);

// SAFETY: a kernel handle is not tied to the thread that opened it, and `Handle` owns it
// exclusively, so moving it to another thread (the pipe server runs on its own) is sound.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: owned handle, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn write_all(h: HANDLE, bytes: &[u8]) -> Result<()> {
    let mut sent = 0usize;
    while sent < bytes.len() {
        let mut n = 0u32;
        // SAFETY: writes from a live slice into a pipe handle we own.
        unsafe { WriteFile(h, Some(&bytes[sent..]), Some(&mut n), None) }.ctx("WriteFile")?;
        sent += n as usize;
    }
    // SAFETY: waits until the other end has read everything.
    let _ = unsafe { FlushFileBuffers(h) };
    Ok(())
}

/// Reads until one complete line is available. `None` when the other end closed the pipe.
fn read_line(h: HANDLE, buffer: &mut LineBuffer) -> Result<Option<Vec<u8>>> {
    loop {
        if let Some(line) = buffer.next_line() {
            return Ok(Some(line));
        }
        let mut chunk = [0u8; 8192];
        let mut n = 0u32;
        // SAFETY: reads into a live buffer from a pipe handle we own.
        match unsafe { ReadFile(h, Some(&mut chunk), Some(&mut n), None) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_BROKEN_PIPE.to_hresult() => return Ok(None),
            Err(e) => {
                return Err(Error::Win32 {
                    call: "ReadFile",
                    source: e,
                });
            }
        }
        if n == 0 {
            return Ok(None);
        }
        buffer
            .push(&chunk[..n as usize])
            .map_err(|e| Error::Other(e.to_string()))?;
    }
}

/// Owns the pipe while Mosaïk runs. Serves one client at a time: clients are short-lived
/// (`mosaikctl` sends a request or two and exits).
pub struct Server {
    pipe: Handle,
}

impl Server {
    /// Claims the pipe. Fails if another process already holds the name (usually another
    /// Mosaïk already running for this user and session).
    pub fn bind(name: &str) -> Result<Self> {
        let sid = user_sid()?;
        // Protected DACL: full access for this user and SYSTEM only.
        let sddl = wide(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"));
        let name = wide(name);
        // SAFETY: the security descriptor is freed after the pipe is created; strings outlive
        // the calls.
        unsafe {
            let mut sd = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
            .ctx("ConvertStringSecurityDescriptorToSecurityDescriptorW")?;
            let sa = SECURITY_ATTRIBUTES {
                nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            let h = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                FILE_FLAGS_AND_ATTRIBUTES(PIPE_ACCESS_DUPLEX.0 | FILE_FLAG_FIRST_PIPE_INSTANCE.0),
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                Some(&sa),
            );
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            if h.is_invalid() {
                return Err(mosaik_win::error::last_error("CreateNamedPipeW"));
            }
            Ok(Self { pipe: Handle(h) })
        }
    }

    /// Waits for a client and answers its requests until it disconnects. Returns false when
    /// asked to stop (a request answered with `stop = true`).
    pub fn serve_one(&self, handler: &mut dyn FnMut(Request) -> (Response, bool)) -> Result<bool> {
        let h = self.pipe.0;
        // SAFETY: blocking connect on our pipe; ERROR_PIPE_CONNECTED means a client connected
        // between creation and this call, which is success.
        if let Err(e) = unsafe { ConnectNamedPipe(h, None) }
            && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
        {
            return Err(Error::Win32 {
                call: "ConnectNamedPipe",
                source: e,
            });
        }
        let mut buffer = LineBuffer::default();
        let mut keep_going = true;
        while let Some(line) = read_line(h, &mut buffer)? {
            let (response, stop) = match decode_request(&line) {
                Ok(request) => handler(request),
                Err(e) => (protocol_error(&e), false),
            };
            let bytes = encode(&response).map_err(|e| Error::Other(e.to_string()))?;
            write_all(h, &bytes)?;
            if stop {
                keep_going = false;
                break;
            }
        }
        // SAFETY: ends this client's session so the next one can connect.
        let _ = unsafe { DisconnectNamedPipe(h) };
        Ok(keep_going)
    }
}

fn protocol_error(e: &ProtocolError) -> Response {
    use crate::protocol::ErrorCode;
    match e {
        ProtocolError::UnsupportedVersion { .. } => {
            Response::error(0, ErrorCode::UnsupportedVersion, e.to_string(), None)
        }
        _ => Response::error(0, ErrorCode::BadRequest, e.to_string(), None),
    }
}

/// The program a client accepts as Mosaïk.
pub const MOSAIK_PROGRAM: &str = "mosaik.exe";

/// A connection to the running Mosaïk.
pub struct Client {
    pipe: Handle,
    buffer: LineBuffer,
    next_id: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("Mosaïk is not running")]
    NotRunning,
    #[error("the program answering on Mosaïk's pipe is not Mosaïk ({0}); not talking to it")]
    Impostor(String),
    #[error(transparent)]
    Other(#[from] Error),
}

impl Client {
    /// Connects to Mosaïk's pipe, checking the server really is `mosaik.exe`.
    pub fn connect(name: &str, timeout: Duration) -> std::result::Result<Self, ConnectError> {
        Self::connect_to(name, timeout, &[MOSAIK_PROGRAM])
    }

    /// Connects to a pipe served by one of `programs` (tests serve from their own binary).
    pub fn connect_to(
        name: &str,
        timeout: Duration,
        programs: &[&str],
    ) -> std::result::Result<Self, ConnectError> {
        let wname = wide(name);
        let start = Instant::now();
        let handle = loop {
            // SAFETY: opens the pipe for reading and writing; the handle is owned below.
            let r = unsafe {
                CreateFileW(
                    PCWSTR(wname.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            };
            match r {
                Ok(h) => break Handle(h),
                Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() && start.elapsed() < timeout => {
                    // SAFETY: waits for the server to accept another client.
                    let _ = unsafe { WaitNamedPipeW(PCWSTR(wname.as_ptr()), 500) };
                }
                Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() => {
                    return Err(ConnectError::Other(Error::Other(
                        "Mosaïk is busy with another client".into(),
                    )));
                }
                Err(_) => return Err(ConnectError::NotRunning),
            }
        };
        let program = server_program(handle.0).unwrap_or_default();
        if !programs.iter().any(|p| program.eq_ignore_ascii_case(p)) {
            return Err(ConnectError::Impostor(if program.is_empty() {
                "unknown".into()
            } else {
                program
            }));
        }
        Ok(Self {
            pipe: handle,
            buffer: LineBuffer::default(),
            next_id: 1,
        })
    }

    pub fn request(&mut self, command: crate::protocol::Command) -> Result<Response> {
        let id = self.next_id;
        self.next_id += 1;
        let bytes = encode(&Request::new(id, command)).map_err(|e| Error::Other(e.to_string()))?;
        write_all(self.pipe.0, &bytes)?;
        let line = read_line(self.pipe.0, &mut self.buffer)?
            .ok_or_else(|| Error::Other("Mosaïk closed the connection".into()))?;
        decode_response(&line).map_err(|e| Error::Other(e.to_string()))
    }
}

/// The file name of the process on the server end of a pipe.
fn server_program(pipe: HANDLE) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: valid pipe handle and out pointer; the process handle is closed below.
    unsafe {
        GetNamedPipeServerProcessId(pipe, &mut pid).ok()?;
        let p = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?);
        let mut buf = [0u16; 1024];
        let mut len = 1024u32;
        QueryFullProcessImageNameW(p.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).ok()?;
        let path = from_wide(&buf[..len as usize]);
        Some(path.rsplit('\\').next().unwrap_or(&path).to_owned())
    }
}
