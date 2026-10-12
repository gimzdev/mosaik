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
    pub args: String,
    /// The program that owns the window, lower case.
    pub exe: &'static str,
    /// The window class, when the program owns several kinds of windows.
    pub class: Option<&'static str>,
}

/// A page that changes colour four times a second (Mosaïk's zone colours), so a browser that has
/// stopped drawing shows up as a frozen capture.
const ANIMATED_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><title>Mosaik probe page</title>\
<style>html,body{margin:0;height:100%}body{animation:c 1s steps(1,end) infinite}\
@keyframes c{0%{background:#c93450}25%{background:#12786a}50%{background:#2b44d9}75%{background:#f4b13a}}</style>";

/// Writes the animated page to the temp folder and returns its address.
fn animated_page() -> String {
    let dir = std::env::temp_dir().join("mosaik-probe");
    let path = dir.join("animated.html");
    if std::fs::create_dir_all(&dir).is_err() || std::fs::write(&path, ANIMATED_PAGE).is_err() {
        return "about:blank".into();
    }
    let p = path.to_string_lossy().replace('\\', "/").replace(' ', "%20");
    format!("file:///{p}")
}

/// The first of `paths` that exists (a packaged app's alias counts), or nothing: the app is not
/// installed, and opening it says so rather than letting Windows show an error.
fn first_existing(paths: &[String]) -> String {
    paths
        .iter()
        .find(|p| std::path::Path::new(p).symlink_metadata().is_ok())
        .cloned()
        .unwrap_or_default()
}

/// A Firefox profile of the probe's own, so the user's profile, session and history are never
/// touched: no welcome pages, no question when a window with several tabs closes.
fn firefox_profile() -> String {
    let dir = std::env::temp_dir().join("mosaik-probe").join("firefox");
    let prefs = "user_pref(\"browser.aboutwelcome.enabled\", false);\n\
user_pref(\"browser.shell.checkDefaultBrowser\", false);\n\
user_pref(\"browser.startup.homepage_override.mstone\", \"ignore\");\n\
user_pref(\"browser.tabs.warnOnClose\", false);\n\
user_pref(\"datareporting.policy.firstRunURL\", \"\");\n";
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("user.js"), prefs);
    dir.to_string_lossy().into_owned()
}

/// Chrome, Edge, VS Code (Electron), Windows Terminal, Explorer, Word (Office), Calculator (UWP)
/// and Notepad; then, where installed, Firefox, Notepad++, Paint, Excel and Git Bash (mintty).
/// Browsers open private windows (Firefox a profile of its own) so no session or history is
/// touched, on a page that keeps drawing.
pub fn catalog() -> Vec<App> {
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    let (local, pf, pf86) = (env("LOCALAPPDATA"), env("ProgramFiles"), env("ProgramFiles(x86)"));
    let page = animated_page();
    vec![
        App {
            name: "Notepad",
            target: "notepad.exe".into(),
            args: String::new(),
            exe: "notepad.exe",
            class: None,
        },
        App {
            name: "Edge",
            target: "msedge.exe".into(),
            args: format!("--inprivate --new-window {page}"),
            exe: "msedge.exe",
            class: None,
        },
        App {
            name: "Chrome",
            target: "chrome.exe".into(),
            args: format!("--incognito --new-window {page}"),
            exe: "chrome.exe",
            class: None,
        },
        App {
            name: "VS Code",
            target: format!("{local}\\Programs\\Microsoft VS Code\\Code.exe"),
            args: "--new-window".into(),
            exe: "code.exe",
            class: None,
        },
        App {
            name: "Windows Terminal",
            target: "wt.exe".into(),
            args: "-w new".into(),
            exe: "windowsterminal.exe",
            class: None,
        },
        App {
            name: "Explorer",
            target: "explorer.exe".into(),
            args: "C:\\Windows".into(),
            exe: "explorer.exe",
            class: Some("CabinetWClass"),
        },
        App {
            name: "Word",
            target: "winword.exe".into(),
            args: "/q /w".into(),
            exe: "winword.exe",
            class: Some("OpusApp"),
        },
        App {
            name: "Calculator (UWP)",
            target: "calc.exe".into(),
            args: String::new(),
            exe: "applicationframehost.exe",
            class: Some("ApplicationFrameWindow"),
        },
        App {
            name: "Firefox",
            target: first_existing(&[
                format!("{pf}\\Mozilla Firefox\\firefox.exe"),
                format!("{local}\\Mozilla Firefox\\firefox.exe"),
            ]),
            args: format!("-no-remote -profile \"{}\" -new-window {page}", firefox_profile()),
            exe: "firefox.exe",
            class: Some("MozillaWindowClass"),
        },
        App {
            name: "Notepad++",
            target: first_existing(&[
                format!("{pf}\\Notepad++\\notepad++.exe"),
                format!("{pf86}\\Notepad++\\notepad++.exe"),
            ]),
            args: "-multiInst -nosession".into(),
            exe: "notepad++.exe",
            class: Some("Notepad++"),
        },
        App {
            name: "Paint",
            target: first_existing(&[format!("{local}\\Microsoft\\WindowsApps\\mspaint.exe")]),
            args: String::new(),
            exe: "mspaint.exe",
            class: Some("MSPaintApp"),
        },
        App {
            name: "Excel",
            target: first_existing(&[
                format!("{pf}\\Microsoft Office\\root\\Office16\\EXCEL.EXE"),
                format!("{pf86}\\Microsoft Office\\root\\Office16\\EXCEL.EXE"),
            ]),
            args: "/x".into(),
            exe: "excel.exe",
            class: Some("XLMAIN"),
        },
        App {
            name: "Git Bash (mintty)",
            target: first_existing(&[format!("{pf}\\Git\\git-bash.exe")]),
            args: String::new(),
            exe: "mintty.exe",
            class: Some("mintty"),
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
    if app.target.is_empty() {
        return Err(format!("{}: not installed", app.name));
    }
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
    let start = Instant::now();
    shell_open(&app.target, &app.args).map_err(|e| e.to_string())?;
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
