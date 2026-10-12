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
    /// It covers its whole monitor, taskbar included, without being maximised: a game or a
    /// video in full screen.
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
}

impl WindowFacts {
    /// The invisible borders around the visible frame.
    pub fn frame_insets(&self) -> Insets {
        Insets::between(self.window_rect, self.frame_rect)
    }
}
