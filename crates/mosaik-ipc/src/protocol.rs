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

/// The longest message accepted, newline included, so a broken or hostile client cannot make
/// Mosaïk allocate without bound. A `state` reply for a few hundred windows is about 200 KB.
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
    /// A code from a newer Mosaïk: the message still says what went wrong.
    #[serde(other)]
    Unknown,
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
    if bytes.len() + 1 > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

/// The id of a request that could not be decoded, when it has one, so the error reply can be
/// matched to it.
pub fn request_id(line: &[u8]) -> u64 {
    #[derive(Deserialize)]
    struct Id {
        id: u64,
    }
    serde_json::from_slice::<Id>(line).map_or(0, |i| i.id)
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
///
/// A line longer than [`MAX_MESSAGE_BYTES`] (newline included) is an error, and so is a partial
/// line that can no longer fit; after an error the stream cannot be trusted to be in step, so
/// the connection must be closed.
#[derive(Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
    /// Bytes already searched for a newline, so each byte is scanned once.
    scanned: usize,
}

impl LineBuffer {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next complete line, without its newline; `Ok(None)` until one has arrived.
    pub fn next_line(&mut self) -> Result<Option<Vec<u8>>, ProtocolError> {
        let found = self.buf[self.scanned..].iter().position(|&b| b == b'\n');
        let Some(offset) = found else {
            self.scanned = self.buf.len();
            if self.buf.len() >= MAX_MESSAGE_BYTES {
                self.buf.clear();
                self.scanned = 0;
                return Err(ProtocolError::TooLarge);
            }
            return Ok(None);
        };
        let end = self.scanned + offset;
        self.scanned = 0;
        if end + 1 > MAX_MESSAGE_BYTES {
            self.buf.clear();
            return Err(ProtocolError::TooLarge);
        }
        let line = self.buf[..end].to_vec();
        self.buf.drain(..=end);
        Ok(Some(line))
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
    fn line_buffer_splits_lines_across_reads() {
        let mut b = LineBuffer::default();
        b.push(b"{\"a\":1}\n{\"b\"");
        assert_eq!(b.next_line().unwrap().as_deref(), Some(&b"{\"a\":1}"[..]));
        assert_eq!(b.next_line().unwrap(), None);
        b.push(b":2}\n");
        assert_eq!(b.next_line().unwrap().as_deref(), Some(&b"{\"b\":2}"[..]));
        assert_eq!(b.next_line().unwrap(), None);
    }

    #[test]
    fn line_buffer_limit_is_exact() {
        // The longest line allowed: MAX_MESSAGE_BYTES including its newline.
        let mut b = LineBuffer::default();
        let mut longest = vec![b'x'; MAX_MESSAGE_BYTES - 1];
        longest.push(b'\n');
        b.push(&longest);
        assert_eq!(
            b.next_line().unwrap().map(|l| l.len()),
            Some(MAX_MESSAGE_BYTES - 1)
        );
        // One byte more is refused, whether the newline has arrived or not.
        let mut b = LineBuffer::default();
        let mut too_long = vec![b'x'; MAX_MESSAGE_BYTES];
        too_long.push(b'\n');
        b.push(&too_long);
        assert_eq!(b.next_line(), Err(ProtocolError::TooLarge));
        let mut b = LineBuffer::default();
        for _ in 0..(MAX_MESSAGE_BYTES / 8192) {
            b.push(&[b'x'; 8192]);
            if b.next_line().is_err() {
                return;
            }
        }
        panic!("a partial line that can no longer fit was not refused");
    }

    #[test]
    fn encode_refuses_what_the_reader_would_refuse() {
        let big = Response::new(
            1,
            Reply::Done {
                message: "x".repeat(MAX_MESSAGE_BYTES),
            },
        );
        assert_eq!(encode(&big), Err(ProtocolError::TooLarge));
    }

    #[test]
    fn unknown_error_codes_from_a_newer_mosaik_still_decode() {
        let r = decode_response(
            br#"{"v":1,"id":3,"reply":"error","code":"quota_exceeded","message":"Too many workspaces","hint":null}"#,
        )
        .unwrap();
        assert_eq!(
            r.reply,
            Reply::Error {
                code: ErrorCode::Unknown,
                message: "Too many workspaces".into(),
                hint: None
            }
        );
    }

    #[test]
    fn broken_requests_keep_their_id_when_they_have_one() {
        assert_eq!(request_id(br#"{"v":1,"id":41,"cmd":"format_disk"}"#), 41);
        assert_eq!(request_id(b"garbage"), 0);
    }

    #[test]
    fn state_replies_round_trip() {
        use mosaik_core::geometry::Rect;
        let state = StateSnapshot {
            app_version: "1.0.0".into(),
            protocol: PROTOCOL_VERSION,
            safe_mode: false,
            topology_fingerprint: "ab12".into(),
            monitors: vec![MonitorState {
                id: "m1".into(),
                name: "Display 1".into(),
                primary: true,
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1032),
                dpi: 96,
                scale_percent: 100,
            }],
            windows: vec![WindowState {
                handle: WindowHandle(0x1234),
                pid: 99,
                exe: Some("code.exe".into()),
                class: "Chrome_WidgetWin_1".into(),
                title: "main.rs".into(),
                window_rect: Rect::new(-7, 0, 967, 1039),
                frame_rect: Rect::new(0, 0, 960, 1032),
                dpi: 96,
                verdict: Verdict::Manage,
                elevation: Elevation::Same,
                zone: Some(ZoneId::new("code")),
                slot: None,
                tab_group: None,
                hidden: None,
            }],
            active_workspace: None,
            hotkey: HotkeyState {
                shortcut: "Win+Alt+Space".into(),
                registered: true,
                problem: None,
            },
            hidden: vec![],
        };
        let r = Response::new(9, Reply::State(Box::new(state)));
        let bytes = encode(&r).unwrap();
        assert_eq!(decode_response(&bytes[..bytes.len() - 1]).unwrap(), r);
    }
}
