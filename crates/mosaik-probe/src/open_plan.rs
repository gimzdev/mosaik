//! What opening a workspace would do with the windows open now, without opening it: the window
//! each of its apps would take, and the apps it would start instead (the owner, 2026-10-16: with
//! Windows Terminal and MSYS2 open, opening a workspace started another terminal and Windows
//! asked which app to open). Reads only: nothing moves, nothing starts, nothing is saved.

use mosaik_core::classify::{Pending, Verdict, classify};
use mosaik_core::matching::assign;
use mosaik_core::settings::Settings;
use mosaik_core::store::{WorkspacesFile, data_folder};
use mosaik_win::window::Scanner;
use serde_json::{Value, json};

/// Words that name a terminal or a shell in a Start menu entry or a program's path.
const SHELLS: [&str; 12] = [
    "terminal",
    "powershell",
    "pwsh",
    "msys",
    "mintty",
    "ucrt",
    "mingw",
    "clang",
    "bash",
    "cmd",
    "wsl",
    "git",
];

pub fn run(args: &[String]) -> Result<Value, String> {
    let dir = data_folder(&|k| std::env::var_os(k)).ok_or("Mosaïk's data folder is not known")?;
    let read = |name: &str| std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"));
    let file: WorkspacesFile =
        serde_json::from_slice(&read("workspaces.json")?).map_err(|e| format!("workspaces.json: {e}"))?;
    let settings: Settings = read("settings.json")
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let own = mosaik_win::process::own_pid();
    let all = Scanner::new().scan();
    // The windows the engine may give a workspace (`Engine::movable`), front to back.
    let windows: Vec<_> = all
        .iter()
        .filter(|f| {
            f.pid != own
                && matches!(
                    classify(f, &settings.rules),
                    Verdict::Manage | Verdict::Pending(Pending::Minimized | Pending::FullScreen)
                )
        })
        .collect();
    let describe = |f: &mosaik_core::window::WindowFacts| {
        json!({
            "handle": format!("{:x}", f.handle.0),
            "exe": f.exe,
            "aumid": f.aumid,
            "class": f.class,
            "title": f.title,
            "relaunch": f.relaunch,
            "minimized": f.minimized,
            "cloak": format!("{:?}", f.cloak),
        })
    };
    for f in &windows {
        println!("open-plan: window {}", describe(f));
    }
    // Windows not given to workspaces, but whose program is a terminal's or a shell's.
    for f in all
        .iter()
        .filter(|f| !windows.iter().any(|w| w.handle == f.handle))
    {
        let exe = f.exe.as_deref().unwrap_or("").to_lowercase();
        if SHELLS.iter().any(|s| exe.contains(s)) && !f.title.trim().is_empty() {
            println!(
                "open-plan: other window {} ({:?})",
                describe(f),
                classify(f, &settings.rules)
            );
        }
    }
    let only = args
        .iter()
        .position(|a| a == "--workspace")
        .and_then(|i| args.get(i + 1));
    let mut report = Vec::new();
    for ws in &file.workspaces {
        if only.is_some_and(|n| !ws.name.eq_ignore_ascii_case(n)) {
            continue;
        }
        let slots: Vec<_> = ws.layout.slots.iter().map(|s| (s.id.clone(), &s.app)).collect();
        let taken = assign(&slots, &windows);
        println!(
            "open-plan: workspace {:?} ({} apps)",
            ws.name,
            ws.layout.slots.len()
        );
        let mut apps = Vec::new();
        for s in &ws.layout.slots {
            let window = taken.iter().find(|(id, _)| *id == s.id).map(|(_, h)| *h);
            let could: Vec<Value> = windows
                .iter()
                .filter_map(|w| {
                    s.app
                        .score(w)
                        .map(|score| json!({ "handle": format!("{:x}", w.handle.0), "score": score }))
                })
                .collect();
            let line = json!({
                "slot": s.id.0,
                "name": s.name(),
                "app": s.app,
                "launch": s.launch,
                "starts_as": s.launch_to_start(),
                "takes": window.map(|h| format!("{:x}", h.0)),
                "could_take": could,
            });
            println!("open-plan:   {line}");
            apps.push(line);
        }
        report.push(json!({ "workspace": ws.name, "apps": apps }));
    }
    // The Start menu's terminals and shells, as "Add an app" lists them.
    let entries =
        mosaik_win::apps::start_apps().map_err(|e| format!("the Start menu could not be read: {e}"))?;
    for a in &entries {
        let text = format!("{} {} {}", a.name, a.id, a.path.as_deref().unwrap_or("")).to_lowercase();
        if SHELLS.iter().any(|s| text.contains(s)) {
            println!(
                "open-plan: Start menu {:?}: id {:?}, program {:?}",
                a.name, a.id, a.path
            );
        }
    }
    Ok(json!({ "ok": true, "workspaces": report }))
}
