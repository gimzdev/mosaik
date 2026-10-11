//! Bringing back every window Mosaïk hid: used by the watchdog after a crash, by "Rescue
//! windows" in the tray, by `mosaikctl rescue` and by `mosaik --rescue`.
//!
//! Two sources, so one lost source never strands a window: the journal file, and the marker
//! property Mosaïk sets on each window it hides.

use mosaik_core::geometry::Rect;
use mosaik_core::journal::{HiddenWindow, HideMethod, Journal};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::GetPropW;
use windows::core::w;

use crate::hide::{Hidden, Hider};
use crate::monitors::topology;
use crate::window::{exists, handle, hwnd, pid, top_level};

/// What a rescue did, for the user and the logs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub restored: Vec<String>,
    pub gone: usize,
    pub failed: Vec<String>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut s = format!("{} window(s) restored", self.restored.len());
        if self.gone > 0 {
            s.push_str(&format!(", {} had already closed", self.gone));
        }
        if !self.failed.is_empty() {
            s.push_str(&format!(
                ", {} could not be restored: {}",
                self.failed.len(),
                self.failed.join("; ")
            ));
        }
        s
    }
}

fn marker(h: HWND) -> Option<HideMethod> {
    // SAFETY: reads a window property; returns null when absent.
    let v = unsafe { GetPropW(h, w!("Mosaik.Hidden")) };
    match v.0 as usize {
        1 => Some(HideMethod::ShellCloak),
        2 => Some(HideMethod::Minimize),
        3 => Some(HideMethod::Park),
        _ => None,
    }
}

/// Keeps a parked window's way home on a monitor that still exists.
fn reachable_restore(r: Rect) -> Rect {
    match topology() {
        Ok(t) => mosaik_core::place::reachable(r, &t),
        Err(_) => r,
    }
}

/// Restores every window in `journal`, then every other window still carrying Mosaïk's marker.
/// Needs COM initialised on the calling thread (for shell-cloaked windows).
pub fn restore_all(journal: &Journal) -> Report {
    let mut report = Report::default();
    let mut hider = Hider::new();
    let mut done: Vec<u64> = Vec::new();
    for entry in &journal.hidden {
        let h = hwnd(entry.handle);
        // A closed window, or its handle reused by another process: nothing to restore.
        if !exists(h) || pid(h) != entry.pid {
            report.gone += 1;
            continue;
        }
        restore_one(&mut hider, h, entry, &mut report);
        done.push(entry.handle.0);
    }
    for h in top_level() {
        if done.contains(&handle(h).0) {
            continue;
        }
        if let Some(method) = marker(h) {
            let entry = HiddenWindow {
                handle: handle(h),
                pid: pid(h),
                method,
                // Without the journal the old place is unknown: parked windows come back to
                // the primary monitor.
                restore_rect: Rect::from_origin_size(100, 100, 1000, 700),
                was_maximized: false,
                exe: None,
                title: crate::window::title(h),
            };
            restore_one(&mut hider, h, &entry, &mut report);
        }
    }
    report
}

fn restore_one(hider: &mut Hider, h: HWND, entry: &HiddenWindow, report: &mut Report) {
    let hidden = Hidden {
        method: entry.method,
        restore_rect: if entry.method == HideMethod::Park {
            reachable_restore(entry.restore_rect)
        } else {
            entry.restore_rect
        },
        was_maximized: entry.was_maximized,
    };
    let label = if entry.title.is_empty() {
        format!("window {}", entry.handle)
    } else {
        entry.title.clone()
    };
    match hider.show(h, &hidden) {
        Ok(()) => report.restored.push(label),
        Err(e) => report.failed.push(format!("{label}: {e}")),
    }
}

/// True when the window carries Mosaïk's hidden-window marker.
pub fn has_marker(h: HWND) -> bool {
    marker(h).is_some()
}
