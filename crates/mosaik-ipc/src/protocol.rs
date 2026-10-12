//! Messages, their framing, and version checks.

use mosaik_core::classify::Verdict;
use mosaik_core::editing::ZoneEdit;
use mosaik_core::geometry::Rect;
use mosaik_core::journal::{HiddenWindow, HideMethod};
use mosaik_core::layout::{SlotId, TabGroupId, WorkspaceId};
use mosaik_core::settings::Settings;
use mosaik_core::window::{Elevation, WindowHandle};
use mosaik_core::zone::ZoneId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Raised when commands or replies change; a client may be older than Mosaïk, never newer.
pub const PROTOCOL_VERSION: u32 = 2;

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

/// What a client asks. A `workspace` field takes a workspace's id or, when no id matches, its
/// name (case and surrounding spaces do not count).
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
    Switch {
        workspace: String,
    },
    /// Save the windows on screen as the workspace with this name (a new one when no
    /// workspace has it; the open one when `None`).
    Save {
        workspace: Option<String>,
    },
    /// Bring back every hidden window now.
    Rescue,
    ShowSwitcher,
    /// Open the settings window (what a second launch of Mosaïk asks the running one).
    ShowSettings,
    Quit,
    /// The user's settings.
    Settings,
    /// Saves the user's settings: all of them, as `settings` returned them with the changes
    /// made (a field left out takes its default). A shortcut that cannot be used is refused
    /// with the reason; everything else takes effect at once.
    SetSettings {
        settings: Box<Settings>,
    },
    /// Whether a shortcut can be used for the switcher, or for `workspace`, with the reason
    /// when it cannot (the recorder asks before saving).
    CheckHotkey {
        hotkey: String,
        #[serde(default)]
        workspace: Option<String>,
    },
    /// One workspace's zones on the monitors connected now, for the zone editor.
    Zones {
        workspace: String,
    },
    /// Replaces one workspace's zones (from the zone editor). `revision` is the one `zones`
    /// gave: when the monitors or the zones changed since, nothing is saved.
    SetZones {
        workspace: String,
        zones: Vec<ZoneEdit>,
        revision: String,
    },
    RenameWorkspace {
        workspace: String,
        name: String,
    },
    DeleteWorkspace {
        workspace: String,
    },
    /// Gives a workspace a global shortcut of its own, or takes it away (`None`).
    SetWorkspaceHotkey {
        workspace: String,
        hotkey: Option<String>,
    },
    /// While the interface records a shortcut, Mosaïk's own shortcuts let go, so pressing one
    /// reaches the recorder instead of opening the switcher. They come back with `hold: false`,
    /// or after a minute in any case.
    HoldHotkeys {
        hold: bool,
    },
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
    /// The user's settings, and the switcher shortcut as registered (it can differ from the
    /// one in the settings: taken by another app, Mosaïk falls back to another).
    Settings {
        settings: Box<Settings>,
        hotkey: HotkeyState,
    },
    /// What `check_hotkey` found: the shortcut as Mosaïk writes it, why it cannot be used (if
    /// so), and what is worth knowing about it otherwise.
    Hotkey {
        hotkey: String,
        problem: Option<String>,
        warnings: Vec<String>,
    },
    /// A workspace's zones on the monitors connected now (also the answer to `set_zones`, with
    /// the ids of new zones).
    Zones {
        workspace: WorkspaceId,
        name: String,
        monitors: Vec<MonitorState>,
        zones: Vec<ZoneEdit>,
        /// Sent back with `set_zones`.
        revision: String,
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
    /// The watchdog is running; without it Mosaïk hides nothing.
    pub watchdog: bool,
    pub topology_fingerprint: String,
    pub monitors: Vec<MonitorState>,
    pub windows: Vec<WindowState>,
    pub active_workspace: Option<WorkspaceId>,
    pub hotkey: HotkeyState,
    pub hidden: Vec<HiddenWindow>,
    /// The tab strips on screen, so tests can click a tab by its numbers.
    #[serde(default)]
    pub tab_strips: Vec<TabStripState>,
}

/// A tab group's strip as drawn: where it is, each tab's chip and window, the one that shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabStripState {
    pub group: TabGroupId,
    pub rect: Rect,
    /// In tab order.
    pub tabs: Vec<TabState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabState {
    pub window: WindowHandle,
    pub chip: Rect,
    pub visible: bool,
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

impl From<&mosaik_core::topology::Monitor> for MonitorState {
    fn from(m: &mosaik_core::topology::Monitor) -> Self {
        Self {
            id: m.id.0.clone(),
            name: m.name.clone(),
            primary: m.primary,
            bounds: m.bounds,
            work_area: m.work_area,
            dpi: m.dpi,
            scale_percent: m.scale_percent(),
        }
    }
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
    /// Where the window's zone tag is drawn (in its title bar, or above it), when it shows.
    #[serde(default)]
    pub tag: Option<Rect>,
    /// Where dropping another window on this one makes them tabs: the stretch of its title bar
    /// holding its tag, or the band above it.
    #[serde(default)]
    pub tab_drop: Option<Rect>,
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
            Command::Settings,
            Command::SetSettings {
                settings: Box::default(),
            },
            Command::CheckHotkey {
                hotkey: "Win+Alt+Space".into(),
                workspace: None,
            },
            Command::Zones {
                workspace: "Brand".into(),
            },
            Command::SetZones {
                workspace: "Brand".into(),
                zones: vec![ZoneEdit {
                    id: None,
                    name: "Notes".into(),
                    color: mosaik_core::zone::ZoneColor::Saffron,
                    monitor: mosaik_core::topology::MonitorId::new("m1"),
                    area: mosaik_core::geometry::RatioRect::FULL,
                }],
                revision: "0".into(),
            },
            Command::RenameWorkspace {
                workspace: "Brand".into(),
                name: "Brand 2027".into(),
            },
            Command::DeleteWorkspace {
                workspace: "Brand".into(),
            },
            Command::SetWorkspaceHotkey {
                workspace: "Brand".into(),
                hotkey: Some("Win+Alt+J".into()),
            },
            Command::SetWorkspaceHotkey {
                workspace: "Brand".into(),
                hotkey: None,
            },
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
            "{\"v\":2,\"id\":1,\"cmd\":\"switch\",\"workspace\":\"Brand\"}\n"
        );
        let err = Response::error(
            2,
            ErrorCode::NotFound,
            "No workspace called \"X\"",
            Some("mosaikctl workspaces lists them"),
        );
        let json = String::from_utf8(encode(&err).unwrap()).unwrap();
        assert!(json.starts_with("{\"v\":2,\"id\":2,\"reply\":\"error\",\"code\":\"not_found\""));
    }

    #[test]
    fn newer_clients_are_refused_plainly() {
        let err = decode_request(br#"{"v":3,"id":1,"cmd":"state"}"#).unwrap_err();
        assert_eq!(err, ProtocolError::UnsupportedVersion { client: 3, server: 2 });
        // Older clients are understood.
        assert!(decode_request(br#"{"v":1,"id":1,"cmd":"state"}"#).is_ok());
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
            watchdog: true,
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
                tag: Some(Rect::new(820, 5, 890, 26)),
                tab_drop: Some(Rect::new(42, 5, 890, 26)),
            }],
            active_workspace: None,
            hotkey: HotkeyState {
                shortcut: "Win+Alt+Space".into(),
                registered: true,
                problem: None,
            },
            hidden: vec![],
            tab_strips: vec![TabStripState {
                group: TabGroupId("g1".into()),
                rect: Rect::new(4, 4, 956, 32),
                tabs: vec![TabState {
                    window: WindowHandle(0x1234),
                    chip: Rect::new(90, 7, 200, 28),
                    visible: true,
                }],
            }],
        };
        let r = Response::new(9, Reply::State(Box::new(state)));
        let bytes = encode(&r).unwrap();
        assert_eq!(decode_response(&bytes[..bytes.len() - 1]).unwrap(), r);
    }
}

/// The wire format both sides rely on, checked against `fixtures/wire.json`, which the
/// interface's own tests read too: a change here that the TypeScript types do not follow fails
/// one side or the other. `UPDATE_FIXTURES=1 cargo test -p mosaik-ipc` rewrites the file.
#[cfg(test)]
mod wire {
    use super::*;

    fn samples() -> serde_json::Value {
        let commands = vec![
            Command::Hello {
                client: "settings".into(),
            },
            Command::State,
            Command::Workspaces,
            Command::Switch {
                workspace: "ship".into(),
            },
            Command::Save { workspace: None },
            Command::Rescue,
            Command::ShowSwitcher,
            Command::ShowSettings,
            Command::Quit,
            Command::Settings,
            Command::SetSettings {
                settings: Box::default(),
            },
            Command::CheckHotkey {
                hotkey: "Win+Alt+J".into(),
                workspace: Some("ship".into()),
            },
            Command::Zones {
                workspace: "ship".into(),
            },
            Command::SetZones {
                workspace: "ship".into(),
                zones: vec![zone()],
                revision: "5e2b9c0f41d3a877".into(),
            },
            Command::RenameWorkspace {
                workspace: "ship".into(),
                name: "Ship 1.0".into(),
            },
            Command::DeleteWorkspace {
                workspace: "ship".into(),
            },
            Command::SetWorkspaceHotkey {
                workspace: "ship".into(),
                hotkey: Some("Win+Alt+J".into()),
            },
            Command::HoldHotkeys { hold: true },
        ];
        let replies = vec![
            Reply::Hello {
                app_version: "1.0.0".into(),
                protocol: PROTOCOL_VERSION,
            },
            Reply::Workspaces {
                workspaces: vec![WorkspaceSummary {
                    id: WorkspaceId("ship".into()),
                    name: "Ship the release".into(),
                    windows: 4,
                    zones: 3,
                    hotkey: None,
                }],
                active: Some(WorkspaceId("ship".into())),
            },
            Reply::Done {
                message: "2 window(s) restored".into(),
            },
            Reply::Error {
                code: ErrorCode::NotFound,
                message: "No workspace called \"x\".".into(),
                hint: Some("mosaikctl workspaces lists them.".into()),
            },
            Reply::Settings {
                settings: Box::default(),
                hotkey: HotkeyState {
                    shortcut: "Win+Alt+Space".into(),
                    registered: true,
                    problem: None,
                },
            },
            Reply::Hotkey {
                hotkey: "Ctrl+Alt+K".into(),
                problem: Some("Ctrl+Alt is AltGr on many keyboards.".into()),
                warnings: vec!["Many apps use Ctrl with a letter.".into()],
            },
            Reply::Zones {
                workspace: WorkspaceId("ship".into()),
                name: "Ship the release".into(),
                monitors: vec![MonitorState {
                    id: "m1".into(),
                    name: "DELL U2720Q".into(),
                    primary: true,
                    bounds: Rect::new(0, 0, 3840, 2160),
                    work_area: Rect::new(0, 0, 3840, 2112),
                    dpi: 192,
                    scale_percent: 200,
                }],
                zones: vec![zone()],
                revision: "5e2b9c0f41d3a877".into(),
            },
        ];
        serde_json::json!({ "commands": commands, "replies": replies })
    }

    fn zone() -> ZoneEdit {
        ZoneEdit {
            id: Some(mosaik_core::zone::ZoneId::new("z1")),
            name: "Code".into(),
            color: mosaik_core::zone::ZoneColor::Lapis,
            monitor: mosaik_core::topology::MonitorId::new("m1"),
            area: mosaik_core::geometry::RatioRect::new(0.0, 0.0, 0.6, 1.0)
                .unwrap_or(mosaik_core::geometry::RatioRect::FULL),
        }
    }

    #[test]
    fn matches_the_shared_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/wire.json");
        let now = samples();
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            let mut text = serde_json::to_string_pretty(&now).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
        }
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved, now,
            "run UPDATE_FIXTURES=1 cargo test -p mosaik-ipc and update protocol.ts"
        );
    }
}
