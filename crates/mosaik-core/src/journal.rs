//! The hidden-windows journal: every window Mosaïk has hidden (a background tab) or parked, and
//! how to bring it back. The main process rewrites it on every change; if the main process
//! dies, the watchdog reads it and restores each window, so a crash never strands a window.

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::window::WindowHandle;

pub const JOURNAL_VERSION: u32 = 1;

/// How a window was hidden, which decides how it is brought back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HideMethod {
    /// Cloaked through the shell: still "visible" to Windows but not drawn. Undone by uncloaking.
    ShellCloak,
    /// Minimised without activation. Undone by restoring without activation.
    Minimize,
    /// Moved out of view. Undone by moving it back to `restore_rect`.
    Park,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenWindow {
    pub handle: WindowHandle,
    /// The process that owned the window, so a reused handle is not mistaken for it.
    pub pid: u32,
    pub method: HideMethod,
    /// The window rectangle (as `GetWindowRect`) before it was hidden.
    pub restore_rect: Rect,
    pub was_maximized: bool,
    /// For the rescue report.
    pub exe: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    /// The Mosaïk process that wrote it.
    pub owner_pid: u32,
    pub hidden: Vec<HiddenWindow>,
}

impl Journal {
    pub fn new(owner_pid: u32) -> Self {
        Self {
            version: JOURNAL_VERSION,
            owner_pid,
            hidden: Vec::new(),
        }
    }

    /// Records a hidden window, replacing any older entry for the same handle.
    pub fn record(&mut self, entry: HiddenWindow) {
        self.forget(entry.handle);
        self.hidden.push(entry);
    }

    pub fn forget(&mut self, handle: WindowHandle) -> Option<HiddenWindow> {
        let i = self.hidden.iter().position(|h| h.handle == handle)?;
        Some(self.hidden.remove(i))
    }

    pub fn is_empty(&self) -> bool {
        self.hidden.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(h: u64, method: HideMethod) -> HiddenWindow {
        HiddenWindow {
            handle: WindowHandle(h),
            pid: 42,
            method,
            restore_rect: Rect::new(0, 0, 800, 600),
            was_maximized: false,
            exe: Some("code.exe".into()),
            title: "lib.rs".into(),
        }
    }

    #[test]
    fn records_replace_and_forget() {
        let mut j = Journal::new(7);
        j.record(entry(1, HideMethod::ShellCloak));
        j.record(entry(1, HideMethod::Minimize));
        j.record(entry(2, HideMethod::Park));
        assert_eq!(j.hidden.len(), 2);
        assert_eq!(
            j.hidden[0].method,
            HideMethod::Minimize,
            "the newer entry replaced the older one"
        );
        assert_eq!(
            j.forget(WindowHandle(2)).map(|h| h.method),
            Some(HideMethod::Park)
        );
        assert_eq!(j.forget(WindowHandle(2)), None);
        let json = serde_json::to_string(&j).unwrap();
        assert!(json.contains("\"method\":\"minimize\""));
        assert_eq!(serde_json::from_str::<Journal>(&json).unwrap(), j);
    }
}
