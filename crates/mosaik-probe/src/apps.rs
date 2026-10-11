//! Opening real apps for the probes and finding the window each one opens.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Rules, Verdict, classify};
use mosaik_win::process::shell_open;
use mosaik_win::window::{Scanner, handle, top_level};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

/// An app the probes open: how to start a new window of it, and how to recognise that window.
pub struct App {
    pub name: &'static str,
    pub target: String,
    pub args: &'static str,
    /// The program that owns the window, lower case.
    pub exe: &'static str,
    /// The window class, when the program owns several kinds of windows.
    pub class: Option<&'static str>,
}

/// Chrome, Edge, VS Code (Electron), Windows Terminal, Explorer, Word (Office), Calculator (UWP)
/// and Notepad. Browsers open private windows so no session or history is touched.
pub fn catalog() -> Vec<App> {
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    vec![
        App {
            name: "Notepad",
            target: "notepad.exe".into(),
            args: "",
            exe: "notepad.exe",
            class: None,
        },
        App {
            name: "Edge",
            target: "msedge.exe".into(),
            args: "--inprivate --new-window about:blank",
            exe: "msedge.exe",
            class: None,
        },
        App {
            name: "Chrome",
            target: "chrome.exe".into(),
            args: "--incognito --new-window about:blank",
            exe: "chrome.exe",
            class: None,
        },
        App {
            name: "VS Code",
            target: format!("{local}\\Programs\\Microsoft VS Code\\Code.exe"),
            args: "--new-window",
            exe: "code.exe",
            class: None,
        },
        App {
            name: "Windows Terminal",
            target: "wt.exe".into(),
            args: "-w new",
            exe: "windowsterminal.exe",
            class: None,
        },
        App {
            name: "Explorer",
            target: "explorer.exe".into(),
            args: "C:\\Windows",
            exe: "explorer.exe",
            class: Some("CabinetWClass"),
        },
        App {
            name: "Word",
            target: "winword.exe".into(),
            args: "/q /w",
            exe: "winword.exe",
            class: Some("OpusApp"),
        },
        App {
            name: "Calculator (UWP)",
            target: "calc.exe".into(),
            args: "",
            exe: "applicationframehost.exe",
            class: Some("ApplicationFrameWindow"),
        },
    ]
}

pub struct Opened {
    pub hwnd: HWND,
    pub exe: String,
    pub class: String,
    pub open_ms: f64,
}

/// Opens `app` and waits (up to 25 s) for a new, manageable window of it.
pub fn open(app: &App, scanner: &mut Scanner) -> Result<Opened, String> {
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
    let start = Instant::now();
    shell_open(&app.target, app.args).map_err(|e| e.to_string())?;
    while start.elapsed() < Duration::from_secs(25) {
        mosaik_win::events::pump_until(150, || false);
        for h in top_level() {
            if before.contains(&handle(h).0) {
                continue;
            }
            let Some(f) = scanner.facts(h) else { continue };
            let exe_ok = f.exe.as_deref() == Some(app.exe);
            let class_ok = app.class.is_none_or(|c| f.class.eq_ignore_ascii_case(c));
            if exe_ok && class_ok && classify(&f, &Rules::default()) == Verdict::Manage {
                // Let the app finish its first layout before measuring anything.
                mosaik_win::events::pump_until(800, || false);
                return Ok(Opened {
                    hwnd: h,
                    exe: f.exe.unwrap_or_default(),
                    class: f.class,
                    open_ms: start.elapsed().as_secs_f64() * 1000.0,
                });
            }
        }
    }
    Err(format!("{}: no new window within 25 s", app.name))
}

/// Asks a window to close, like clicking its close button.
pub fn close(hwnd: HWND) {
    // SAFETY: posts WM_CLOSE to a window the probe opened; harmless if it is already gone.
    let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
}
