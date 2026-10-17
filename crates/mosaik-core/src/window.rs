//! What the Windows layer reports about a top-level window, as plain data.

use serde::{Deserialize, Serialize};

use crate::geometry::{Insets, Rect};

/// A window handle (`HWND`) as a number. Handles are reused by Windows after a window closes,
/// so a handle alone never identifies a window across sessions; slots match by [`crate::matching::AppMatch`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowHandle(pub u64);

impl std::fmt::Display for WindowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:X}", self.0)
    }
}

/// Why DWM is not drawing a window, from `DWMWA_CLOAKED`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cloak {
    #[default]
    None,
    /// The app cloaked its own window (for example a UWP window still starting up).
    ByApp,
    /// The shell cloaked it: usually because it is on another virtual desktop.
    ByShell,
    /// Its owner is cloaked.
    Inherited,
    /// Mosaïk cloaked it (a background tab). Tracked by Mosaïk itself, since Windows reports
    /// shell cloaks the same way.
    ByMosaik,
}

/// Whether Mosaïk may move the window: Windows blocks a normal process from moving windows of a
/// higher integrity level (elevated apps, some system tools).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Elevation {
    /// Same level as Mosaïk or lower: manageable.
    #[default]
    Same,
    /// Higher integrity than Mosaïk: Windows will refuse moves and hides.
    Higher,
    /// The process could not be inspected (protected processes); treated like `Higher`.
    Unknown,
}

/// Facts about one top-level window, gathered by `mosaik-win`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowFacts {
    pub handle: WindowHandle,
    pub pid: u32,
    /// The program's file name in lower case, e.g. `code.exe`.
    pub exe: Option<String>,
    /// Application User Model ID, for packaged apps.
    pub aumid: Option<String>,
    pub class: String,
    pub title: String,
    /// `IsWindowVisible` (true even when cloaked).
    pub visible: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub cloak: Cloak,
    /// `WS_CHILD`.
    pub child: bool,
    /// Has an owner window.
    pub owned: bool,
    /// `WS_EX_TOOLWINDOW`.
    pub tool_window: bool,
    /// `WS_EX_APPWINDOW`: forces a taskbar button even when owned.
    pub app_window: bool,
    /// `WS_EX_NOACTIVATE`.
    pub no_activate: bool,
    /// `WS_POPUP`.
    pub popup: bool,
    /// `WS_CAPTION`.
    pub caption: bool,
    /// `WS_THICKFRAME`: the user can resize it.
    pub resizable: bool,
    /// `WS_MINIMIZEBOX`: it can be minimised, as app windows can (a splash screen cannot).
    #[serde(default)]
    pub minimize_box: bool,
    /// A game or a video in full screen ([`is_full_screen`]).
    #[serde(default)]
    pub full_screen: bool,
    /// Passes the Alt+Tab rule: it is its own root owner's last active popup.
    pub alt_tab: bool,
    pub elevation: Elevation,
    /// `GetWindowRect`: includes the invisible resize borders.
    pub window_rect: Rect,
    /// `DWMWA_EXTENDED_FRAME_BOUNDS`: what the user sees.
    pub frame_rect: Rect,
    /// DPI of the window (`GetDpiForWindow`).
    pub dpi: u32,
    /// The process belongs to Mosaïk itself (its UI, overlays or watchdog).
    pub own_process: bool,
    /// The command Windows' taskbar relaunches the window's app with, when the app says one:
    /// Git Bash's mintty window says `git-bash.exe`, a Chrome profile's window
    /// `chrome.exe --profile-directory=…` (job 0185).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relaunch: Option<String>,
}

impl WindowFacts {
    /// The invisible borders around the visible frame.
    pub fn frame_insets(&self) -> Insets {
        Insets::between(self.window_rect, self.frame_rect)
    }

    /// The program file (lower case, `git-bash.exe`) of the command that relaunches the window's
    /// app, when it says one.
    pub fn relaunch_program(&self) -> Option<String> {
        match crate::layout::Launch::from_command_line(self.relaunch.as_deref()?)? {
            crate::layout::Launch::Program { path, .. } => {
                let name = path.rsplit(['\\', '/']).next().unwrap_or(&path);
                Some(name.to_lowercase())
            }
            crate::layout::Launch::Packaged { .. } => None,
        }
    }
}

/// Programs whose file describes something other than the app people know (Steam's windows
/// belong to its web helper, "Steam Client WebHelper"), by file name in lower case.
const APP_NAMES: &[(&str, &str)] = &[
    ("explorer.exe", "File Explorer"),
    ("steamwebhelper.exe", "Steam"),
    ("steam.exe", "Steam"),
    ("windowsterminal.exe", "Windows Terminal"),
    ("conhost.exe", "Console"),
];

/// The name people know an app by, for the few programs whose file says something else
/// (`None`: its file's description says it).
pub fn known_app_name(exe: &str) -> Option<&'static str> {
    let exe = exe.to_lowercase();
    APP_NAMES.iter().find(|(e, _)| *e == exe).map(|(_, name)| *name)
}

/// Whether a window whose visible frame is `frame` is a game or a video in full screen: over the
/// whole of its monitor (`monitor`, its bounds), taskbar included, within 2 px, and not
/// maximised. A window with Windows' title bar and resize frame is not, even there: it is a
/// window placed over a monitor without a taskbar (by Mosaïk, in a zone as large as the
/// monitor). Browsers, players and games in full screen drop both, or never had them.
pub fn is_full_screen(frame: Rect, monitor: Rect, maximized: bool, caption: bool, resizable: bool) -> bool {
    !maximized && !(caption && resizable) && covers_monitor(frame, monitor)
}

/// Whether a window whose visible frame is `frame` lies over the whole of `monitor` (its bounds,
/// taskbar included), within 2 px.
pub fn covers_monitor(frame: Rect, monitor: Rect) -> bool {
    [
        frame.left - monitor.left,
        frame.top - monitor.top,
        frame.right - monitor.right,
        frame.bottom - monitor.bottom,
    ]
    .iter()
    .all(|d| d.abs() <= 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_whose_files_say_something_else_have_their_own_names() {
        // Job 0100, on the owner's screen: Steam's window belongs to its web helper, Windows
        // Terminal's file calls itself a host, and a console window belongs to its host.
        assert_eq!(known_app_name("steamwebhelper.exe"), Some("Steam"));
        assert_eq!(known_app_name("SteamWebHelper.exe"), Some("Steam"));
        assert_eq!(known_app_name("WindowsTerminal.exe"), Some("Windows Terminal"));
        assert_eq!(known_app_name("conhost.exe"), Some("Console"));
        // Others go by their file's description.
        assert_eq!(known_app_name("chrome.exe"), None);
    }

    #[test]
    fn full_screen_is_the_whole_monitor_without_a_window_frame() {
        let monitor = Rect::new(0, 0, 1920, 1080);
        // A browser or a player in full screen, a borderless game.
        assert!(is_full_screen(monitor, monitor, false, false, false));
        // A game a pixel short of the monitor, as some keep the taskbar reachable.
        assert!(is_full_screen(
            Rect::new(0, 0, 1920, 1079),
            monitor,
            false,
            false,
            false
        ));
        // A window with its title bar and resize frame over a monitor with no taskbar, as Mosaïk
        // places one in a zone as large as the monitor (review of job 0078).
        assert!(!is_full_screen(
            Rect::new(2, 2, 1918, 1078),
            monitor,
            false,
            true,
            true
        ));
        // Maximised, or across two monitors, or in a zone.
        assert!(!is_full_screen(monitor, monitor, true, false, false));
        assert!(!is_full_screen(
            Rect::new(0, 0, 3840, 1080),
            monitor,
            false,
            false,
            false
        ));
        assert!(!is_full_screen(
            Rect::new(4, 4, 956, 1028),
            monitor,
            false,
            false,
            false
        ));
        // On a monitor left of the main one.
        let left = Rect::new(-2560, -360, 0, 1080);
        assert!(is_full_screen(left, left, false, true, false));
    }
}
