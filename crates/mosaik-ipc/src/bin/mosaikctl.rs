//! `mosaikctl`: talk to the running Mosaïk from a terminal or a script.
//!
//! ```text
//! mosaikctl state [--json]        monitors, DPI, windows and their rectangles
//! mosaikctl workspaces            the saved workspaces
//! mosaikctl switch <workspace>    switch to a workspace (by name or id)
//! mosaikctl save [<workspace>]    save the current arrangement
//! mosaikctl rescue                bring back every window Mosaïk hid (works even if it is not running)
//! mosaikctl switcher              open the switcher
//! mosaikctl version
//! ```
//!
//! Exit codes: 0 done, 1 Mosaïk reported a problem, 2 wrong usage, 3 Mosaïk is not running.

// A command-line tool: printing is its output.
#![allow(clippy::print_stdout)]

#[cfg(windows)]
fn main() {
    std::process::exit(windows_main::run(std::env::args().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("mosaikctl only runs on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
mod windows_main {
    use std::time::Duration;

    use mosaik_core::journal::Journal;
    use mosaik_core::store::Store;
    use mosaik_ipc::pipe::{Client, ConnectError, pipe_name};
    use mosaik_ipc::protocol::{Command, ErrorCode, Reply, StateSnapshot};

    const USAGE: &str = "usage: mosaikctl <state [--json] | workspaces | switch <workspace> | save [<workspace>] | rescue | switcher | version>";

    pub fn run(args: Vec<String>) -> i32 {
        let Some(command) = args.first().map(String::as_str) else {
            eprintln!("{USAGE}");
            return 2;
        };
        let json = args.iter().any(|a| a == "--json");
        let rest: Vec<&str> = args
            .iter()
            .skip(1)
            .filter(|a| *a != "--json")
            .map(String::as_str)
            .collect();
        let cmd = match (command, rest.as_slice()) {
            ("state", []) => Command::State,
            ("workspaces", []) => Command::Workspaces,
            ("switch", [w]) => Command::Switch {
                workspace: (*w).to_owned(),
            },
            ("save", []) => Command::Save { workspace: None },
            ("save", [w]) => Command::Save {
                workspace: Some((*w).to_owned()),
            },
            ("rescue", []) => Command::Rescue,
            ("switcher", []) => Command::ShowSwitcher,
            ("version", []) => {
                println!("mosaikctl {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            _ => {
                eprintln!("{USAGE}");
                return 2;
            }
        };
        let name = match pipe_name() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("mosaikctl: {e}");
                return 1;
            }
        };
        let mut client = match Client::connect(&name, Duration::from_secs(5)) {
            Ok(c) => c,
            Err(ConnectError::NotRunning) if matches!(cmd, Command::Rescue) => return rescue_locally(),
            Err(ConnectError::NotRunning) => {
                eprintln!("Mosaïk is not running. Start it from the Start menu, then try again.");
                return 3;
            }
            Err(e) => {
                eprintln!("mosaikctl: {e}");
                return 1;
            }
        };
        let _ = client.request(Command::Hello {
            client: format!("mosaikctl {}", env!("CARGO_PKG_VERSION")),
        });
        match client.request(cmd) {
            Ok(response) => print_reply(&response.reply, json),
            Err(e) => {
                eprintln!("mosaikctl: {e}");
                1
            }
        }
    }

    fn print_reply(reply: &Reply, json: bool) -> i32 {
        match reply {
            Reply::State(state) if json => {
                println!("{}", serde_json::to_string_pretty(state).unwrap_or_default());
                0
            }
            Reply::State(state) => {
                print_state(state);
                0
            }
            Reply::Workspaces { workspaces, active } => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({ "workspaces": workspaces, "active": active })
                        )
                        .unwrap_or_default()
                    );
                } else if workspaces.is_empty() {
                    println!("No workspaces yet. Open the switcher and press S to save one.");
                } else {
                    for w in workspaces {
                        let mark = if active.as_ref() == Some(&w.id) { "*" } else { " " };
                        println!(
                            "{mark} {:<28} {} windows, {} zones{}",
                            w.name,
                            w.windows,
                            w.zones,
                            w.hotkey.as_ref().map(|h| format!(", {h}")).unwrap_or_default()
                        );
                    }
                }
                0
            }
            Reply::Done { message }
            | Reply::Hello {
                app_version: message, ..
            } => {
                println!("{message}");
                0
            }
            Reply::Error { code, message, hint } => {
                eprintln!("{message}");
                if let Some(h) = hint {
                    eprintln!("{h}");
                }
                if *code == ErrorCode::UnsupportedVersion {
                    2
                } else {
                    1
                }
            }
        }
    }

    fn print_state(s: &StateSnapshot) {
        println!(
            "Mosaïk {} (protocol {}){}",
            s.app_version,
            s.protocol,
            if s.safe_mode { ", safe mode" } else { "" }
        );
        println!(
            "Shortcut {}: {}",
            s.hotkey.shortcut,
            if s.hotkey.registered {
                "registered".to_owned()
            } else {
                s.hotkey
                    .problem
                    .clone()
                    .unwrap_or_else(|| "not registered".into())
            }
        );
        println!("Monitors (fingerprint {}):", s.topology_fingerprint);
        for m in &s.monitors {
            println!(
                "  {}{}  {}x{} at ({}, {})  work {}x{} at ({}, {})  {}%",
                m.name,
                if m.primary { " (primary)" } else { "" },
                m.bounds.width(),
                m.bounds.height(),
                m.bounds.left,
                m.bounds.top,
                m.work_area.width(),
                m.work_area.height(),
                m.work_area.left,
                m.work_area.top,
                m.scale_percent
            );
        }
        println!("Windows:");
        for w in &s.windows {
            println!(
                "  {:<24} {:>5}x{:<5} at ({:>5}, {:>5})  {:<10} {}",
                w.exe.as_deref().unwrap_or("?"),
                w.frame_rect.width(),
                w.frame_rect.height(),
                w.frame_rect.left,
                w.frame_rect.top,
                w.zone.as_ref().map_or("", |z| z.0.as_str()),
                w.title
            );
        }
        if !s.hidden.is_empty() {
            println!("Hidden by Mosaïk: {}", s.hidden.len());
        }
    }

    /// Mosaïk is not running: restore what its journal lists, here and now.
    fn rescue_locally() -> i32 {
        let Ok(appdata) = std::env::var("APPDATA") else {
            eprintln!("APPDATA is not set; cannot find Mosaïk's data folder.");
            return 1;
        };
        mosaik_win::dpi::make_process_per_monitor_v2();
        // SAFETY: initialises COM once for this thread before the shell-cloak calls.
        let _ = unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            )
        };
        let store = Store::new(std::path::Path::new(&appdata).join("Mosaik"));
        let journal = store.load::<Journal>().map(|l| l.value).unwrap_or_default();
        let report = mosaik_win::rescue::restore_all(&journal);
        if !journal.hidden.is_empty() {
            let _ = store.save(&Journal::new(journal.owner_pid));
        }
        println!("{}", report.summary());
        i32::from(!report.failed.is_empty())
    }
}
