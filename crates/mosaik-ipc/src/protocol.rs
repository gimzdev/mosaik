//! Messages, their framing, and version checks.

use mosaik_core::classify::Verdict;
use mosaik_core::geometry::Rect;
use mosaik_core::journal::{HiddenWindow, HideMethod};
use mosaik_core::layout::{SlotId, TabGroupId, WorkspaceId};
use mosaik_core::window::{Elevation, WindowHandle};
use mosaik_core::zone::ZoneId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;

/// The longest message accepted, so a broken or hostile client cannot make Mosaïk allocate
/// without bound.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// First message from a client: who it is.
    Hello {
        client: String,
    },
    /// Monitors, DPI, windows and rectangles: what `mosaikctl state --json` prints.
    State,
    Workspaces,
    /// Switch to a workspace, by id or by name.
    Switch {
        workspace: String,
    },
    /// Save the current arrangement into a workspace (the active one when `None`).
    Save {
        workspace: Option<String>,
    },
    /// Bring back every hidden window now.
    Rescue,
    ShowSwitcher,
    Quit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    pub id: u64,
    #[serde(flatten)]
    pub reply: Reply,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Hello {
        app_version: String,
        protocol: u32,
    },
    State(Box<StateSnapshot>),
    Workspaces {
        workspaces: Vec<WorkspaceSummary>,
        active: Option<WorkspaceId>,
    },
    Done {
        message: String,
    },
    Error {
        code: ErrorCode,
        message: String,
        hint: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnsupportedVersion,
    BadRequest,
    NotFound,
    Busy,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub app_version: String,
    pub protocol: u32,
    /// Started with `--safe-mode`: nothing is moved or hidden.
    pub safe_mode: bool,
    pub topology_fingerprint: String,
    pub monitors: Vec<MonitorState>,
    pub windows: Vec<WindowState>,
    pub active_workspace: Option<WorkspaceId>,
    pub hotkey: HotkeyState,
    pub hidden: Vec<HiddenWindow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorState {
    pub id: String,
    pub name: String,
    pub primary: bool,
    pub bounds: Rect,
    pub work_area: Rect,
    pub dpi: u32,
    pub scale_percent: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub handle: WindowHandle,
    pub pid: u32,
    pub exe: Option<String>,
    pub class: String,
    pub title: String,
    pub window_rect: Rect,
    pub frame_rect: Rect,
    pub dpi: u32,
    pub verdict: Verdict,
    pub elevation: Elevation,
    pub zone: Option<ZoneId>,
    pub slot: Option<SlotId>,
    pub tab_group: Option<TabGroupId>,
    pub hidden: Option<HideMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyState {
    pub shortcut: String,
    pub registered: bool,
    pub problem: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: WorkspaceId,
    pub name: String,
    pub windows: usize,
    pub zones: usize,
    pub hotkey: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("message too large (over {MAX_MESSAGE_BYTES} bytes)")]
    TooLarge,
    #[error("not a valid message: {0}")]
    Invalid(String),
    #[error("this client speaks protocol {client}, Mosaïk speaks {server}: update Mosaïk or the client")]
    UnsupportedVersion { client: u32, server: u32 },
}

impl Request {
    pub fn new(id: u64, command: Command) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            command,
        }
    }
}

impl Response {
    pub fn new(id: u64, reply: Reply) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            reply,
        }
    }

    pub fn error(id: u64, code: ErrorCode, message: impl Into<String>, hint: Option<&str>) -> Self {
        Self::new(
            id,
            Reply::Error {
                code,
                message: message.into(),
                hint: hint.map(str::to_owned),
            },
        )
    }
}

/// One message as a line of JSON.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut bytes = serde_json::to_vec(message).map_err(|e| ProtocolError::Invalid(e.to_string()))?;
    if bytes.len() >= MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

/// Parses a request line, refusing versions this server does not know.
pub fn decode_request(line: &[u8]) -> Result<Request, ProtocolError> {
    #[derive(Deserialize)]
    struct Version {
        v: u32,
    }
    let version: Version = serde_json::from_slice(line).map_err(|e| ProtocolError::Invalid(e.to_string()))?;
    if version.v > PROTOCOL_VERSION || version.v == 0 {
        return Err(ProtocolError::UnsupportedVersion {
            client: version.v,
            server: PROTOCOL_VERSION,
        });
    }
    serde_json::from_slice(line).map_err(|e| ProtocolError::Invalid(e.to_string()))
}

pub fn decode_response(line: &[u8]) -> Result<Response, ProtocolError> {
    serde_json::from_slice(line).map_err(|e| ProtocolError::Invalid(e.to_string()))
}

/// Collects bytes from a stream and hands out complete lines.
#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > MAX_MESSAGE_BYTES && !self.buf.contains(&b'\n') {
            self.buf.clear();
            return Err(ProtocolError::TooLarge);
        }
        Ok(())
    }

    /// The next complete line, without its newline.
    pub fn next_line(&mut self) -> Option<Vec<u8>> {
        let end = self.buf.iter().position(|&b| b == b'\n')?;
        let line: Vec<u8> = self.buf.drain(..=end).take(end).collect();
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_as_single_lines() {
        for c in [
            Command::Hello {
                client: "mosaikctl".into(),
            },
            Command::State,
            Command::Switch {
                workspace: "Ship the release".into(),
            },
            Command::Save { workspace: None },
            Command::Rescue,
        ] {
            let bytes = encode(&Request::new(7, c.clone())).unwrap();
            assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
            let back = decode_request(&bytes[..bytes.len() - 1]).unwrap();
            assert_eq!(back.command, c);
            assert_eq!(back.id, 7);
        }
    }

    #[test]
    fn wire_format_is_stable() {
        let json = String::from_utf8(
            encode(&Request::new(
                1,
                Command::Switch {
                    workspace: "Brand".into(),
                },
            ))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            json,
            "{\"v\":1,\"id\":1,\"cmd\":\"switch\",\"workspace\":\"Brand\"}\n"
        );
        let err = Response::error(
            2,
            ErrorCode::NotFound,
            "No workspace called \"X\"",
            Some("mosaikctl workspaces lists them"),
        );
        let json = String::from_utf8(encode(&err).unwrap()).unwrap();
        assert!(json.starts_with("{\"v\":1,\"id\":2,\"reply\":\"error\",\"code\":\"not_found\""));
    }

    #[test]
    fn newer_clients_are_refused_plainly() {
        let err = decode_request(br#"{"v":2,"id":1,"cmd":"state"}"#).unwrap_err();
        assert_eq!(err, ProtocolError::UnsupportedVersion { client: 2, server: 1 });
        assert!(err.to_string().contains("update Mosaïk"));
        assert!(matches!(
            decode_request(br#"{"v":1,"id":1,"cmd":"format_disk"}"#),
            Err(ProtocolError::Invalid(_))
        ));
        assert!(matches!(
            decode_request(b"not json"),
            Err(ProtocolError::Invalid(_))
        ));
    }

    #[test]
    fn line_buffer_splits_and_limits() {
        let mut b = LineBuffer::default();
        b.push(b"{\"a\":1}\n{\"b\"").unwrap();
        assert_eq!(b.next_line().as_deref(), Some(&b"{\"a\":1}"[..]));
        assert_eq!(b.next_line(), None);
        b.push(b":2}\n").unwrap();
        assert_eq!(b.next_line().as_deref(), Some(&b"{\"b\":2}"[..]));
        let huge = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        assert_eq!(b.push(&huge), Err(ProtocolError::TooLarge));
    }
}
