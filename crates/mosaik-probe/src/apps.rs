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

/// An app of the Start menu: its name, and its id in the shell's apps folder.
#[derive(Clone, Debug)]
pub struct StartApp {
    pub name: String,
    pub id: String,
}

/// Every app of the Start menu (desktop apps and packaged apps), as Windows lists them
/// (`Get-StartApps`, in Windows PowerShell).
pub fn start_apps() -> Result<Vec<StartApp>, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); \
             @(Get-StartApps | Select-Object Name, AppID) | ConvertTo-Json -Compress",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Get-StartApps: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let list: Vec<serde_json::Value> =
        serde_json::from_str(text.trim()).map_err(|e| format!("Get-StartApps: {e}"))?;
    Ok(list
        .into_iter()
        .filter_map(|v| {
            Some(StartApp {
                name: v.get("Name")?.as_str()?.to_owned(),
                id: v.get("AppID")?.as_str()?.to_owned(),
            })
        })
        .collect())
}

/// Why the probe does not open `app` on the user's PC, if it does not: it installs or removes
/// software, is a page or a document, a system or administrator tool, uses the camera or the
/// microphone, signs in to an account (chat, mail, cloud, game launchers, passwords, VPN), drives
/// hardware, or takes over the screen. Opening those could change something; everything else is
/// opened, tested and closed.
pub fn skip_reason(app: &StartApp) -> Option<&'static str> {
    const RULES: [(&str, &[&str]); 10] = [
        (
            "installs, updates or removes software",
            &[
                "uninstall",
                "setup",
                "install",
                "update",
                "upgrade",
                "repair",
                "reset",
                "recovery",
                "remove",
                "modify",
                "store",
                "winget",
            ],
        ),
        (
            "a page or a document",
            &[
                "help",
                "manual",
                "guide",
                "readme",
                "read me",
                "license",
                "licence",
                "eula",
                "documentation",
                "website",
                "web site",
                "release notes",
                "changelog",
                "what's new",
                "what\u{2019}s new",
                "tips",
                "get started",
                "getting started",
                "support",
                "feedback",
                "report",
                ".url",
                ".pdf",
                ".chm",
                ".txt",
                ".html",
                "http",
            ],
        ),
        (
            "a system or administrator tool",
            &[
                "administrative",
                "windows tools",
                "computer management",
                "services",
                "event viewer",
                "task manager",
                "task scheduler",
                "registry",
                "regedit",
                "system configuration",
                "system information",
                "resource monitor",
                "performance monitor",
                "defrag",
                "disk ",
                "cleanup",
                "format",
                "backup",
                "restore",
                "memory diagnostic",
                "security",
                "firewall",
                "defender",
                "bitlocker",
                "credential",
                "policy",
                "odbc",
                "iscsi",
                "component services",
                "print management",
                "control panel",
                "settings",
                "device manager",
                "sandbox",
                "hyper-v",
                "quick assist",
                "remote desktop",
                "troubleshoot",
                "diagnostic",
                "debug",
                "wsl",
                "ubuntu",
                "debian",
                "kali",
                "linux",
                "virtual",
                "vmware",
                "virtualbox",
            ],
        ),
        (
            "accessibility, camera, microphone or phone",
            &[
                "narrator",
                "magnifier",
                "on-screen keyboard",
                "voice",
                "speech",
                "live captions",
                "steps recorder",
                "camera",
                "recorder",
                "phone",
                "mobile",
            ],
        ),
        (
            "signs in to an account",
            &[
                "onedrive",
                "teams",
                "outlook",
                "mail",
                "calendar",
                "people",
                "skype",
                "discord",
                "slack",
                "whatsapp",
                "telegram",
                "signal",
                "messenger",
                "zoom",
                "webex",
                "steam",
                "epic games",
                "battle.net",
                "ea app",
                "origin",
                "ubisoft",
                "gog",
                "riot",
                "rockstar",
                "launcher",
                "vpn",
                "xbox",
                "game bar",
                "spotify",
                "dropbox",
                "google drive",
                "icloud",
                "1password",
                "bitwarden",
                "lastpass",
                "keepass",
                "dashlane",
                "authenticator",
                "proton",
                "nordvpn",
            ],
        ),
        (
            "drives hardware",
            &[
                "nvidia",
                "geforce",
                "amd ",
                "radeon",
                "intel ",
                "intel\u{ae}",
                "realtek",
                "dolby",
                "razer",
                "logitech",
                "corsair",
                "steelseries",
                "armoury",
                "msi ",
                "dell",
                "lenovo",
                "synaptics",
                "wacom",
                "bluetooth",
                "audio console",
                "printer",
                "scanner",
                "virtual display",
                "displays",
            ],
        ),
        (
            "takes over the screen",
            &[
                "snipping",
                "screen sketch",
                "screenshot",
                "sharex",
                "greenshot",
                "obs studio",
                "lightshot",
            ],
        ),
        ("Mosaïk itself", &["mosaik", "mosa\u{ef}k"]),
        (
            "a game, an emulator, an assistant or the app the user is talking to",
            &[
                "league of legends",
                "tftacademy",
                "curseforge",
                "bluestacks",
                "claude",
                "copilot",
                "click to do",
                "family",
            ],
        ),
        (
            "a duplicate of a console already tested, or a developer test tool",
            &[
                "native tools command prompt",
                "cross tools command prompt",
                "powershell (x86)",
                "powershell ise (x86)",
                "application verifier",
                "cert kit",
                "software development kit",
                "module docs",
                "alienware",
                "killer",
                "preferences",
                "onenote",
                "sticky notes",
                "to do",
                "canva",
            ],
        ),
    ];
    if app.name.eq_ignore_ascii_case("Run") {
        return Some("a system or administrator tool");
    }
    let text = format!("{} {}", app.name, app.id).to_lowercase();
    RULES
        .iter()
        .find(|(_, words)| words.iter().any(|w| text.contains(w)))
        .map(|(why, _)| *why)
}

/// Opens a Start menu app as Windows would (through the shell's apps folder), without letting it
/// ask for administrator rights (it runs with the user's, or does not start), and waits up to
/// 20 s for a new window of it that Mosaïk would manage.
pub fn open_start_app(app: &StartApp, scanner: &mut Scanner) -> Result<Opened, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
    let start = Instant::now();
    std::process::Command::new("cmd.exe")
        .args(["/c", "start", "", &format!("shell:AppsFolder\\{}", app.id)])
        .env("__COMPAT_LAYER", "RunAsInvoker")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("not started: {e}"))?;
    let own = mosaik_win::process::own_pid();
    while start.elapsed() < Duration::from_secs(20) {
        mosaik_win::events::pump_until(250, || false);
        for h in top_level() {
            if before.contains(&handle(h).0) {
                continue;
            }
            let Some(f) = scanner.facts(h) else { continue };
            if f.pid == own || classify(&f, &Rules::default()) != Verdict::Manage {
                continue;
            }
            // A window that lasts (not a splash that goes away at once).
            mosaik_win::events::pump_until(1500, || false);
            if !mosaik_win::window::exists(h) {
                continue;
            }
            return Ok(Opened {
                hwnd: h,
                exe: f.exe.unwrap_or_default(),
                class: f.class,
                open_ms: start.elapsed().as_secs_f64() * 1000.0,
            });
        }
    }
    Err("no new window within 20 s (already open, opens none, or did not start)".into())
}
