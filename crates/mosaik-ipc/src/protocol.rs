//! Messages, their framing, and version checks.

use mosaik_core::classify::Verdict;
use mosaik_core::editing::ZoneEdit;
use mosaik_core::geometry::{RatioRect, Rect};
use mosaik_core::journal::{HiddenWindow, HideMethod};
use mosaik_core::layout::{SlotId, TabGroupId, WorkspaceId};
use mosaik_core::settings::{EdgeSide, Settings};
use mosaik_core::task::Task;
use mosaik_core::window::{Elevation, WindowHandle};
use mosaik_core::zone::{ZoneColor, ZoneId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Raised when commands or replies change; a client may be older than Mosaïk, never newer.
/// 4: `arrange` takes a layout, and `preview_arrange` shows what it would do.
/// 5: a workspace's apps (`apps`, `add_app`, `move_app`, `remove_app`, and their names in
/// `workspaces`); Arrange minimises the windows it has no zone for, rather than making them
/// tabs, and its preview says the task the main monitor is laid out for.
/// 6: sets of apps (`sets`, `open_set`), and the windows open sorted into workspaces
/// (`sort_windows`).
/// 7: apps' icons (`icon` and `icons`: keys for the window's `app_icons`), a workspace's zones in
/// small (`layout`), and in `settings` where the side menu is, whether the settings can be saved,
/// and whether Mosaïk runs as installed (`side_menu`, `read_only`, `packaged`).
/// 8: windows moved on the home page's picture of the screen (`moves` on `arrange` and
/// `preview_arrange`), and each preview window's handle (`window`).
/// 9: what opening a workspace would do (`preview_workspace`), with the apps it would start
/// (`starting`) and each tab group's tabs (`tabs`); zones are numbered 1, 2, 3 by place.
pub const PROTOCOL_VERSION: u32 = 9;

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
    /// Put every window on screen in the zone that suits it: the open workspace's zones, or new
    /// ones on a monitor without zones (a new workspace when none is open). Windows left over
    /// once every zone has one are minimised. With `preset` (a layout's id: "halves",
    /// "thirds"...), the main monitor's zones make way for that layout first.
    Arrange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset: Option<String>,
        /// The windows the user moved on the home page's picture of the main screen, in the
        /// order they were moved.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        moves: Vec<ArrangeMove>,
    },
    /// What `arrange` (with this `preset` and these `moves`, if any) would do now on the main
    /// monitor, without doing it: the home page shows it.
    PreviewArrange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        moves: Vec<ArrangeMove>,
    },
    /// What `switch` to `workspace` would do now on the main monitor, without doing it: its
    /// zones, its apps' windows from where they are to their places (apps not open start
    /// there), and the other windows it would minimise. Home and the workspace's page show it.
    PreviewWorkspace {
        workspace: String,
    },
    /// Put back what the last `arrange` changed (for ten minutes, until a workspace is opened,
    /// saved or edited).
    UndoArrange,
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
    /// A workspace's apps, its zones, and the windows on screen it could take.
    Apps {
        workspace: String,
    },
    /// Adds the app of `window` (on screen now) to a workspace, in `zone`; without one, in its
    /// first empty zone, else where the window is. In the open workspace, the window goes there
    /// at once. Answers with `apps`.
    AddApp {
        workspace: String,
        window: WindowHandle,
        #[serde(default)]
        zone: Option<ZoneId>,
    },
    /// Puts an app of a workspace in one of its zones (a tab leaves its group). In the open
    /// workspace, its window moves there at once. Answers with `apps`.
    MoveApp {
        workspace: String,
        app: SlotId,
        zone: ZoneId,
    },
    /// Takes an app out of a workspace; its window stays where it is. Answers with `apps`.
    RemoveApp {
        workspace: String,
        app: SlotId,
    },
    /// While the interface records a shortcut, Mosaïk's own shortcuts let go, so pressing one
    /// reaches the recorder instead of opening the switcher. They come back with `hold: false`,
    /// or after a minute in any case.
    HoldHotkeys {
        hold: bool,
    },
    /// Sets of the apps installed that go together for a task (coding, games...).
    Sets,
    /// Opens a set (its id, from `sets`): its workspace, made the first time, laid out for its
    /// task; the apps that are not open start.
    OpenSet {
        set: String,
    },
    /// Sorts the windows open now into workspaces, one per task they say (the first time
    /// Mosaïk runs). No window moves; a workspace with the same name is left as it is.
    SortWindows,
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
        /// Where the side menu's line is now (`None`: it is off, or Mosaïk is in safe mode).
        #[serde(default)]
        side_menu: Option<SideMenuState>,
        /// The settings file was saved by a newer Mosaïk: it is read, and never written over.
        #[serde(default)]
        read_only: bool,
        /// Mosaïk runs as installed (the Store's package), so its first run starts it with
        /// Windows; a build run from a folder is left as it is (its Run entry would name a folder
        /// that may go away).
        #[serde(default)]
        packaged: bool,
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
    /// What `preview_arrange` or `preview_workspace` found.
    Preview(Box<ArrangePreview>),
    /// What `apps` found (also the answer to `add_app`, `move_app` and `remove_app`).
    Apps(Box<WorkspaceApps>),
    /// The sets `sets` found.
    Sets {
        sets: Vec<AppSetInfo>,
    },
}

/// Where the side menu's line is: on the edge chosen, or on the other one (`moved`) when the
/// pointer goes on past the chosen edge to another screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideMenuState {
    pub side: EdgeSide,
    pub moved: bool,
}

/// A zone of a workspace in small: where it is on the workspace's first monitor, and its colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MiniZone {
    pub area: RatioRect,
    pub color: ZoneColor,
}

/// A set of apps that go together for a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSetInfo {
    /// What `open_set` takes ("coding").
    pub id: String,
    /// Its name, and its workspace's ("Coding").
    pub name: String,
    /// Its apps' names, in the order of their zones.
    pub apps: Vec<String>,
    /// Each app's icon key, in the same order (`mosaik_core::icons`).
    #[serde(default)]
    pub icons: Vec<Option<String>>,
    /// How many of them are open now.
    pub open: u32,
    /// Its workspace, once it has one.
    pub workspace: Option<WorkspaceId>,
}

/// A workspace's apps (front to back), its zones, and the windows on screen it could take.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApps {
    pub workspace: WorkspaceId,
    pub name: String,
    pub apps: Vec<WorkspaceApp>,
    pub zones: Vec<AppZone>,
    pub windows: Vec<OpenWindow>,
}

/// An app of a workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApp {
    pub id: SlotId,
    /// What to call it ("Visual Studio Code").
    pub label: String,
    /// Its icon key (`mosaik_core::icons`).
    #[serde(default)]
    pub icon: Option<String>,
    /// The zone it goes in (a tab: its group's), if any.
    pub zone: Option<ZoneId>,
    /// It is a tab of a group.
    pub tab: bool,
}

/// A zone an app can go in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppZone {
    pub id: ZoneId,
    /// Its number in reading order ("1").
    pub name: String,
    pub color: ZoneColor,
}

/// A window on screen that is not part of the workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenWindow {
    pub window: WindowHandle,
    pub label: String,
    pub title: String,
    /// Its app's icon key (`mosaik_core::icons`).
    #[serde(default)]
    pub icon: Option<String>,
}

/// What Arrange (`preview_arrange`), or opening a workspace (`preview_workspace`), would do on
/// the main monitor, in screen pixels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangePreview {
    /// The main monitor's work area: the zones and windows below are on it.
    pub work_area: Rect,
    /// Its zones then: the open workspace's, new ones, the picked layout's, or the workspace's.
    pub zones: Vec<PreviewZone>,
    /// Its windows, front to back.
    pub windows: Vec<PreviewWindow>,
    /// Windows that go to the other monitors.
    pub elsewhere: u32,
    /// What the main monitor's windows say the user is doing, which Arrange's layout is made for
    /// (`None` for a layout picked, a workspace, or no windows).
    #[serde(default)]
    pub task: Option<Task>,
}

/// A window the user moved on the picture of the main screen: into the `zone`-th of the
/// preview's zones (its index in [`ArrangePreview::zones`]). The window there takes its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrangeMove {
    pub window: WindowHandle,
    pub zone: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewZone {
    pub name: String,
    pub color: ZoneColor,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewWindow {
    /// The window, for a move ([`ArrangeMove`]).
    #[serde(default)]
    pub window: WindowHandle,
    /// The app's name ("Visual Studio Code").
    pub label: String,
    /// Its app's icon key (`mosaik_core::icons`).
    #[serde(default)]
    pub icon: Option<String>,
    /// Where its frame is now.
    pub now: Rect,
    /// Where it would go (`None`: it stays where it is, or is minimised).
    pub then: Option<Rect>,
    /// It would be minimised: every zone has a window, or it is not the workspace's.
    pub minimized: bool,
    /// Its app is not open: opening the workspace starts it there (`now` is `then`, and
    /// `window` is 0).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starting: bool,
    /// The tabs of its tab group, in order, itself among them; empty when it is not a tab.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<PreviewTab>,
}

/// A tab of a tab group in a preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewTab {
    /// Its app's name.
    pub label: String,
    /// Its app's icon key (`mosaik_core::icons`).
    #[serde(default)]
    pub icon: Option<String>,
    /// It is the tab that shows.
    pub shown: bool,
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
    /// Windows whose title bars are being read (where their tags go may still change), so
    /// tests can wait for them.
    #[serde(default)]
    pub title_bars_reading: u32,
    /// The quick menu at the screen's edge, while it shows, so tests can point at it by its
    /// numbers.
    #[serde(default)]
    pub quick_menu: Option<QuickMenuState>,
}

/// The quick menu's handle on screen, or the menu itself while it is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickMenuState {
    pub rect: Rect,
    pub open: bool,
    /// The open menu's rows, top to bottom.
    pub rows: Vec<QuickMenuRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickMenuRow {
    pub label: String,
    pub rect: Rect,
    pub enabled: bool,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: WorkspaceId,
    pub name: String,
    pub windows: usize,
    pub zones: usize,
    pub hotkey: Option<String>,
    /// Its apps' names, front to back ("Visual Studio Code").
    #[serde(default)]
    pub apps: Vec<String>,
    /// Each app's icon key, in the same order (`mosaik_core::icons`).
    #[serde(default)]
    pub icons: Vec<Option<String>>,
    /// Its zones on its first monitor.
    #[serde(default)]
    pub layout: Vec<MiniZone>,
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
            Command::Arrange {
                preset: None,
                moves: Vec::new(),
            },
            Command::Arrange {
                preset: Some("thirds".into()),
                moves: Vec::new(),
            },
            Command::PreviewArrange {
                preset: Some("halves".into()),
                moves: vec![ArrangeMove {
                    window: WindowHandle(42),
                    zone: 1,
                }],
            },
            Command::PreviewWorkspace {
                workspace: "Ship the release".into(),
            },
            Command::UndoArrange,
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
            "{\"v\":9,\"id\":1,\"cmd\":\"switch\",\"workspace\":\"Brand\"}\n"
        );
        let err = Response::error(
            2,
            ErrorCode::NotFound,
            "No workspace called \"X\"",
            Some("mosaikctl workspaces lists them"),
        );
        let json = String::from_utf8(encode(&err).unwrap()).unwrap();
        assert!(json.starts_with("{\"v\":9,\"id\":2,\"reply\":\"error\",\"code\":\"not_found\""));
    }

    #[test]
    fn an_arrange_without_moves_reads_as_before() {
        // Protocol 7 clients (and the side menu) send no moves: nothing moved.
        let r: Request = serde_json::from_str(r#"{"v":7,"id":3,"cmd":"arrange","preset":"thirds"}"#).unwrap();
        assert_eq!(
            r.command,
            Command::Arrange {
                preset: Some("thirds".into()),
                moves: Vec::new(),
            }
        );
        let line = serde_json::to_string(&Request::new(
            4,
            Command::PreviewArrange {
                preset: None,
                moves: Vec::new(),
            },
        ))
        .unwrap();
        assert!(!line.contains("moves"), "{line}");
        // A preview window from Mosaïk 7 has no handle: 0, which no window has.
        let w: PreviewWindow = serde_json::from_str(
            r#"{"label":"Code","icon":null,"now":{"left":0,"top":0,"right":9,"bottom":9},"then":null,"minimized":true}"#,
        )
        .unwrap();
        assert_eq!(w.window, WindowHandle(0));
        // Nor does it start, nor have tabs; a window that does neither says nothing of them.
        assert!(!w.starting && w.tabs.is_empty());
        let line = serde_json::to_string(&w).unwrap();
        assert!(!line.contains("starting") && !line.contains("tabs"), "{line}");
    }

    #[test]
    fn newer_clients_are_refused_plainly() {
        let err = decode_request(br#"{"v":10,"id":1,"cmd":"state"}"#).unwrap_err();
        assert_eq!(
            err,
            ProtocolError::UnsupportedVersion {
                client: 10,
                server: 9
            }
        );
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
            title_bars_reading: 1,
            quick_menu: Some(QuickMenuState {
                rect: Rect::new(1654, 196, 1920, 556),
                open: true,
                rows: vec![QuickMenuRow {
                    label: "Arrange windows".into(),
                    rect: Rect::new(1670, 272, 1906, 306),
                    enabled: true,
                }],
            }),
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
            Command::Arrange {
                preset: None,
                moves: Vec::new(),
            },
            Command::Arrange {
                preset: Some("half-stack".into()),
                moves: vec![ArrangeMove {
                    window: WindowHandle(0x0003_04a2),
                    zone: 2,
                }],
            },
            Command::PreviewArrange {
                preset: None,
                moves: Vec::new(),
            },
            Command::PreviewArrange {
                preset: Some("thirds".into()),
                moves: vec![ArrangeMove {
                    window: WindowHandle(0x0003_04a2),
                    zone: 0,
                }],
            },
            Command::PreviewWorkspace {
                workspace: "ship".into(),
            },
            Command::UndoArrange,
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
            Command::Apps {
                workspace: "ship".into(),
            },
            Command::AddApp {
                workspace: "ship".into(),
                window: WindowHandle(0x2_04F2),
                zone: Some(ZoneId::new("z2")),
            },
            Command::MoveApp {
                workspace: "ship".into(),
                app: SlotId("s1".into()),
                zone: ZoneId::new("z1"),
            },
            Command::RemoveApp {
                workspace: "ship".into(),
                app: SlotId("s2".into()),
            },
            Command::Sets,
            Command::OpenSet { set: "coding".into() },
            Command::SortWindows,
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
                    apps: vec![
                        "Visual Studio Code".into(),
                        "Google Chrome".into(),
                        "Windows Terminal".into(),
                        "Slack".into(),
                    ],
                    icons: vec![
                        Some(r"exe:C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into()),
                        Some(r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe".into()),
                        Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
                        None,
                    ],
                    layout: vec![
                        MiniZone {
                            area: mosaik_core::geometry::RatioRect::new(0.0, 0.0, 0.6, 1.0)
                                .unwrap_or(mosaik_core::geometry::RatioRect::FULL),
                            color: ZoneColor::Lapis,
                        },
                        MiniZone {
                            area: mosaik_core::geometry::RatioRect::new(0.6, 0.0, 1.0, 1.0)
                                .unwrap_or(mosaik_core::geometry::RatioRect::FULL),
                            color: ZoneColor::Verdigris,
                        },
                    ],
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
                side_menu: Some(SideMenuState {
                    side: mosaik_core::settings::EdgeSide::Left,
                    moved: true,
                }),
                read_only: false,
                packaged: true,
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
            Reply::Preview(Box::new(ArrangePreview {
                work_area: Rect::new(0, 0, 1920, 1032),
                zones: vec![
                    PreviewZone {
                        name: "1".into(),
                        color: ZoneColor::Lapis,
                        rect: Rect::new(0, 0, 960, 1032),
                    },
                    PreviewZone {
                        name: "2".into(),
                        color: ZoneColor::Verdigris,
                        rect: Rect::new(960, 0, 1920, 1032),
                    },
                ],
                windows: vec![
                    PreviewWindow {
                        window: WindowHandle(0x0003_04a2),
                        label: "Visual Studio Code".into(),
                        icon: Some(
                            r"exe:C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into(),
                        ),
                        now: Rect::new(120, 80, 1100, 760),
                        then: Some(Rect::new(0, 0, 960, 1032)),
                        minimized: false,
                        starting: false,
                        tabs: Vec::new(),
                    },
                    PreviewWindow {
                        window: WindowHandle(0x0001_02f8),
                        label: "Google Chrome".into(),
                        icon: Some(r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe".into()),
                        now: Rect::new(700, 200, 1800, 980),
                        then: Some(Rect::new(960, 0, 1920, 1032)),
                        minimized: false,
                        starting: false,
                        tabs: vec![
                            PreviewTab {
                                label: "Google Chrome".into(),
                                icon: Some(
                                    r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
                                ),
                                shown: true,
                            },
                            PreviewTab {
                                label: "Claude".into(),
                                icon: Some("app:Claude".into()),
                                shown: false,
                            },
                        ],
                    },
                    PreviewWindow {
                        window: WindowHandle(0),
                        label: "Windows Terminal".into(),
                        icon: Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
                        now: Rect::new(0, 516, 960, 1032),
                        then: Some(Rect::new(0, 516, 960, 1032)),
                        minimized: false,
                        starting: true,
                        tabs: Vec::new(),
                    },
                    PreviewWindow {
                        window: WindowHandle(0x0005_0c1e),
                        label: "Microsoft Edge".into(),
                        icon: None,
                        now: Rect::new(300, 300, 1200, 900),
                        then: None,
                        minimized: true,
                        starting: false,
                        tabs: Vec::new(),
                    },
                ],
                elsewhere: 1,
                task: Some(Task::Coding),
            })),
            Reply::Apps(Box::new(WorkspaceApps {
                workspace: WorkspaceId("ship".into()),
                name: "Ship the release".into(),
                apps: vec![
                    WorkspaceApp {
                        id: SlotId("s1".into()),
                        label: "Visual Studio Code".into(),
                        icon: Some(
                            r"exe:C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into(),
                        ),
                        zone: Some(ZoneId::new("z1")),
                        tab: false,
                    },
                    WorkspaceApp {
                        id: SlotId("s2".into()),
                        label: "Windows Terminal".into(),
                        icon: Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
                        zone: None,
                        tab: false,
                    },
                ],
                zones: vec![AppZone {
                    id: ZoneId::new("z1"),
                    name: "Code".into(),
                    color: ZoneColor::Lapis,
                }],
                windows: vec![OpenWindow {
                    window: WindowHandle(0x2_04F2),
                    label: "Google Chrome".into(),
                    title: "Release notes - Google Chrome".into(),
                    icon: Some(r"exe:C:\Program Files\Google\Chrome\Application\chrome.exe".into()),
                }],
            })),
            Reply::Sets {
                sets: vec![AppSetInfo {
                    id: "coding".into(),
                    name: "Coding".into(),
                    apps: vec![
                        "Visual Studio Code".into(),
                        "Terminal".into(),
                        "Google Chrome".into(),
                        "Claude".into(),
                        "File Explorer".into(),
                    ],
                    icons: vec![
                        Some(r"exe:C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe".into()),
                        Some("app:Microsoft.WindowsTerminal_8wekyb3d8bbwe!App".into()),
                        Some("app:Chrome".into()),
                        Some("app:Claude".into()),
                        Some("app:Microsoft.Windows.Explorer".into()),
                    ],
                    open: 2,
                    workspace: None,
                }],
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
