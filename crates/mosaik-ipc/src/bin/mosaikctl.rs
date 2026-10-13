//! `mosaikctl`: talk to the running Mosaïk from a terminal or a script.
//!
//! ```text
//! mosaikctl state [--json]                   monitors, DPI, windows and their rectangles
//! mosaikctl workspaces [--json]              the saved workspaces
//! mosaikctl switch <workspace>               open a workspace
//! mosaikctl save [<name>]                    save the windows on screen (the open workspace by default)
//! mosaikctl rename <workspace> <name>        rename a workspace
//! mosaikctl delete <workspace>               delete a workspace (its windows stay where they are)
//! mosaikctl hotkey <workspace> <shortcut>    give a workspace its own shortcut (`none` removes it)
//! mosaikctl check-hotkey <shortcut> [<workspace>]  whether a shortcut can be used
//! mosaikctl zones <workspace>                a workspace's zones, as JSON
//! mosaikctl apps <workspace> [--json]        a workspace's apps, and the windows it could take
//! mosaikctl add-app <workspace> <window> [<zone>]  add a window's app (its handle, from `apps`)
//! mosaikctl move-app <workspace> <app> <zone>  put an app in another zone
//! mosaikctl remove-app <workspace> <app>     take an app out (its window stays)
//! mosaikctl rescue                           bring back every window Mosaïk hid (works even if it is not running)
//! mosaikctl switcher | settings | quit | version
//! ```
//!
//! A `<workspace>` is a workspace's name (case does not count) or its id, names first.
//!
//! Exit codes: 0 done, 1 something went wrong (the message says what to do), 2 wrong usage,
//! 3 Mosaïk is not running.
//!
//! Only talks to a Mosaïk that passes the connection checks in `mosaik_ipc::pipe`.

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
    use mosaik_core::layout::SlotId;
    use mosaik_core::store::Store;
    use mosaik_core::window::WindowHandle;
    use mosaik_core::zone::ZoneId;
    use mosaik_ipc::pipe::{Client, PipeError, Program, pipe_name};
    use mosaik_ipc::protocol::{Command, ErrorCode, Reply, StateSnapshot, WorkspaceApps};

    const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    const USAGE: &str = "usage: mosaikctl <command>
  state [--json]                 monitors, windows and rectangles
  workspaces [--json]            the saved workspaces
  switch <workspace>             open a workspace
  save [<workspace>]             save the windows on screen
  arrange [<layout>]             put every window on screen in the zone that suits it;
                                 with a layout, the main screen takes it first: full, halves,
                                 thirds, main-side, side-main, quarters, half-stack,
                                 stack-half, rows, three-rows
  preview [<layout>] [--json]    what arrange would do on the main screen, without doing it
  undo-arrange                   put back what the last arrange changed
  rename <workspace> <name>      rename a workspace
  delete <workspace>             delete a workspace (its windows stay)
  hotkey <workspace> <shortcut>  give a workspace its own shortcut (`none` removes it)
  check-hotkey <shortcut> [<workspace>]  whether a shortcut can be used
  zones <workspace>              a workspace's zones, as JSON
  apps <workspace> [--json]      a workspace's apps, and the windows on screen it could take
  add-app <workspace> <window> [<zone>]  add a window's app (its handle, from apps)
  move-app <workspace> <app> <zone>      put an app in another zone
  remove-app <workspace> <app>   take an app out (its window stays)
  sets [--json]                  sets of your apps that go together for a task
  open-set <set>                 open a set (its id, from sets): its apps start, laid out
  sort-windows                   sort the windows open now into workspaces, one per task
  rescue                         bring back every window Mosaïk hid
  switcher | settings | quit | version
A <workspace> is its name or its id.";

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
            ("rename", [w, name]) => Command::RenameWorkspace {
                workspace: (*w).to_owned(),
                name: (*name).to_owned(),
            },
            ("delete", [w]) => Command::DeleteWorkspace {
                workspace: (*w).to_owned(),
            },
            ("hotkey", [w, key]) => Command::SetWorkspaceHotkey {
                workspace: (*w).to_owned(),
                hotkey: (!key.eq_ignore_ascii_case("none")).then(|| (*key).to_owned()),
            },
            ("check-hotkey", [key]) => Command::CheckHotkey {
                hotkey: (*key).to_owned(),
                workspace: None,
            },
            ("check-hotkey", [key, w]) => Command::CheckHotkey {
                hotkey: (*key).to_owned(),
                workspace: Some((*w).to_owned()),
            },
            ("zones", [w]) => Command::Zones {
                workspace: (*w).to_owned(),
            },
            ("apps", [w]) => Command::Apps {
                workspace: (*w).to_owned(),
            },
            ("add-app", [w, window, zone @ ..]) if zone.len() <= 1 => {
                let Some(window) = handle(window) else {
                    eprintln!("{window} is not a window handle: `mosaikctl apps <workspace>` lists them.");
                    return 2;
                };
                Command::AddApp {
                    workspace: (*w).to_owned(),
                    window,
                    zone: zone.first().map(|z| ZoneId::new(*z)),
                }
            }
            ("move-app", [w, app, zone]) => Command::MoveApp {
                workspace: (*w).to_owned(),
                app: SlotId((*app).to_owned()),
                zone: ZoneId::new(*zone),
            },
            ("remove-app", [w, app]) => Command::RemoveApp {
                workspace: (*w).to_owned(),
                app: SlotId((*app).to_owned()),
            },
            ("arrange", []) => Command::Arrange { preset: None },
            ("arrange", [layout]) => Command::Arrange {
                preset: Some((*layout).to_owned()),
            },
            ("preview", []) => Command::PreviewArrange { preset: None },
            ("preview", [layout]) => Command::PreviewArrange {
                preset: Some((*layout).to_owned()),
            },
            ("undo-arrange", []) => Command::UndoArrange,
            ("sets", []) => Command::Sets,
            ("open-set", [set]) => Command::OpenSet {
                set: (*set).to_owned(),
            },
            ("sort-windows", []) => Command::SortWindows,
            ("rescue", []) => Command::Rescue,
            ("switcher", []) => Command::ShowSwitcher,
            ("settings", []) => Command::ShowSettings,
            ("quit", []) => Command::Quit,
            ("version", []) => {
                println!("mosaikctl {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            _ => {
                eprintln!("{USAGE}");
                return 2;
            }
        };
        let rescue = matches!(cmd, Command::Rescue);
        let mut cmd = cmd;
        let reply = pipe_name().and_then(|name| {
            let mut client = Client::connect(&name, &Program::mosaik(), CONNECT_TIMEOUT)?;
            client.request(
                Command::Hello {
                    client: format!("mosaikctl {}", env!("CARGO_PKG_VERSION")),
                },
                REQUEST_TIMEOUT,
            )?;
            if let Some(typed) = workspace_of(&mut cmd) {
                *typed = resolve(&mut client, typed.as_str())?;
            }
            client.request(cmd, REQUEST_TIMEOUT)
        });
        match reply {
            // A busy engine cannot be trusted to bring windows back: do it here.
            Ok(response)
                if rescue
                    && matches!(
                        response.reply,
                        Reply::Error {
                            code: ErrorCode::Busy,
                            ..
                        }
                    ) =>
            {
                eprintln!("Mosaïk is not answering; restoring windows directly.");
                rescue_locally()
            }
            Ok(response) => print_reply(&response.reply, json),
            // Without an answer from Mosaïk, a rescue is done here: bringing windows back must
            // never depend on the app that hid them.
            Err(e) if rescue => {
                if !matches!(e, PipeError::NotRunning) {
                    eprintln!("Mosaïk did not answer ({e}); restoring windows directly.");
                }
                rescue_locally()
            }
            Err(e) => {
                eprintln!("{}", explain(&e));
                if matches!(e, PipeError::NotRunning) { 3 } else { 1 }
            }
        }
    }

    /// The workspace a command is about, as typed.
    fn workspace_of(cmd: &mut Command) -> Option<&mut String> {
        match cmd {
            Command::Switch { workspace }
            | Command::Zones { workspace }
            | Command::Apps { workspace }
            | Command::AddApp { workspace, .. }
            | Command::MoveApp { workspace, .. }
            | Command::RemoveApp { workspace, .. }
            | Command::RenameWorkspace { workspace, .. }
            | Command::DeleteWorkspace { workspace }
            | Command::SetWorkspaceHotkey { workspace, .. }
            | Command::CheckHotkey {
                workspace: Some(workspace),
                ..
            } => Some(workspace),
            _ => None,
        }
    }

    /// A window handle as `apps` prints it (`0x2A04F2`), or in decimal.
    fn handle(text: &str) -> Option<WindowHandle> {
        let t = text.trim();
        let n = match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(hex, 16).ok()?,
            None => t.parse().ok()?,
        };
        Some(WindowHandle(n))
    }

    /// The id of the workspace with the name the user typed; what they typed when no
    /// workspace has that name (it may be an id). Mosaïk itself looks for ids first, which is
    /// right for programs but not for people: a workspace renamed from "Code" keeps the id
    /// "code", and "code" typed here means the workspace now called that.
    fn resolve(client: &mut Client, typed: &str) -> Result<String, PipeError> {
        let reply = client.request(Command::Workspaces, REQUEST_TIMEOUT)?;
        let named = match reply.reply {
            Reply::Workspaces { workspaces, .. } => workspaces
                .into_iter()
                .find(|w| mosaik_core::workspace::same_name(&w.name, typed))
                .map(|w| w.id.0),
            _ => None,
        };
        Ok(named.unwrap_or_else(|| typed.to_owned()))
    }

    /// What happened and what to do about it.
    fn explain(e: &PipeError) -> String {
        match e {
            PipeError::NotRunning => {
                "Mosaïk is not running. Start it from the Start menu, then try again.".into()
            }
            PipeError::Busy => "Mosaïk is busy with other requests. Try again in a moment.".into(),
            PipeError::TimedOut(_) | PipeError::Closed => format!(
                "Mosaïk did not answer ({e}); it may be stuck. Run `mosaikctl rescue` to bring hidden windows back, then restart Mosaïk."
            ),
            PipeError::Impostor(_) | PipeError::NameTaken { .. } => format!(
                "Not talking to it: {e}. Close that program or restart Windows, then start Mosaïk again."
            ),
            PipeError::Denied => {
                format!("{e}. If Mosaïk runs as administrator, run this command as administrator too.")
            }
            PipeError::Protocol(p) => format!("Mosaïk and mosaikctl do not understand each other: {p}"),
            PipeError::Stopped | PipeError::Win32 { .. } => format!("mosaikctl: {e}"),
        }
    }

    fn print_json(value: &impl serde::Serialize) -> i32 {
        match serde_json::to_string_pretty(value) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(e) => {
                eprintln!("mosaikctl: could not write JSON: {e}");
                1
            }
        }
    }

    /// Window titles come from other apps: never let one send control characters (terminal
    /// escape sequences) to the user's terminal.
    fn printable(text: &str) -> String {
        text.chars().filter(|c| !c.is_control()).collect()
    }

    fn print_reply(reply: &Reply, json: bool) -> i32 {
        match reply {
            Reply::State(state) if json => print_json(state),
            Reply::State(state) => {
                print_state(state);
                0
            }
            Reply::Workspaces { workspaces, active } => {
                if json {
                    return print_json(&serde_json::json!({ "workspaces": workspaces, "active": active }));
                }
                if workspaces.is_empty() {
                    println!("No workspaces yet. Open the switcher and press S to save one.");
                } else {
                    for w in workspaces {
                        let mark = if active.as_ref() == Some(&w.id) { "*" } else { " " };
                        println!(
                            "{mark} {:<28} {} windows, {} zones{}",
                            printable(&w.name),
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
                println!("{}", printable(message));
                0
            }
            Reply::Error { message, hint, .. } => {
                eprintln!("{}", printable(message));
                if let Some(h) = hint {
                    eprintln!("{}", printable(h));
                }
                1
            }
            Reply::Settings { settings, .. } => print_json(settings),
            Reply::Zones { .. } | Reply::Preview(_) => print_json(reply),
            Reply::Sets { .. } if json => print_json(reply),
            Reply::Sets { sets } => {
                if sets.is_empty() {
                    println!("No sets: Mosaïk found no apps that go together.");
                }
                for set in sets {
                    println!(
                        "{:<16} {}  ({} open)",
                        printable(&format!("{} ({})", set.name, set.id)),
                        printable(&set.apps.join(", ")),
                        set.open
                    );
                }
                0
            }
            Reply::Apps(_) if json => print_json(reply),
            Reply::Apps(list) => {
                let WorkspaceApps {
                    name,
                    apps,
                    zones,
                    windows,
                    ..
                } = list.as_ref();
                println!("{}", printable(name));
                if apps.is_empty() {
                    println!("  No apps yet.");
                }
                for a in apps {
                    let zone = a
                        .zone
                        .as_ref()
                        .and_then(|z| zones.iter().find(|x| &x.id == z))
                        .map_or_else(
                            || "its own place".to_owned(),
                            |z| format!("{} ({})", z.name, z.id.0),
                        );
                    println!(
                        "  {:<6} {:<28} {}{}",
                        a.id.0,
                        printable(&a.label),
                        printable(&zone),
                        if a.tab { ", a tab" } else { "" }
                    );
                }
                if !windows.is_empty() {
                    println!("Windows on screen it could take:");
                    for w in windows {
                        println!(
                            "  {:<10} {:<28} {}",
                            w.window,
                            printable(&w.label),
                            printable(&w.title)
                        );
                    }
                }
                0
            }
            Reply::Hotkey {
                hotkey,
                problem,
                warnings,
            } => {
                if json {
                    return print_json(reply);
                }
                match problem {
                    Some(p) => {
                        eprintln!("{hotkey} cannot be used: {}", printable(p));
                        return 1;
                    }
                    None => println!("{hotkey} can be used."),
                }
                for w in warnings {
                    println!("  note: {}", printable(w));
                }
                0
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
                printable(&m.name),
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
                printable(w.exe.as_deref().unwrap_or("?")),
                w.frame_rect.width(),
                w.frame_rect.height(),
                w.frame_rect.left,
                w.frame_rect.top,
                printable(w.zone.as_ref().map_or("", |z| z.0.as_str())),
                printable(&w.title)
            );
        }
        if !s.hidden.is_empty() {
            println!("Hidden by Mosaïk: {}", s.hidden.len());
        }
    }

    /// Without Mosaïk: restore what its journal lists and every window carrying its marker,
    /// here and now. Windows that could not be restored stay in the journal for the next try.
    fn rescue_locally() -> i32 {
        mosaik_win::dpi::make_process_per_monitor_v2();
        // SAFETY: initialises COM once for this thread before the shell-cloak calls.
        let _ = unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            )
        };
        let outcome = match std::env::var_os("APPDATA") {
            Some(appdata) => {
                mosaik_win::rescue::restore_hidden(&Store::new(std::path::Path::new(&appdata).join("Mosaik")))
            }
            None => mosaik_win::rescue::Outcome {
                report: mosaik_win::rescue::restore_all(&Journal::default()),
                problem: Some(
                    "APPDATA is not set, so Mosaïk's list of hidden windows was not found; the windows it marked were restored."
                        .into(),
                ),
            },
        };
        println!("{}", printable(&outcome.report.summary()));
        if let Some(problem) = &outcome.problem {
            eprintln!("{problem}");
        }
        i32::from(!outcome.report.failed.is_empty() || outcome.problem.is_some())
    }
}
