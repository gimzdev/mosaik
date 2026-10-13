//! Sets of apps (ADR 0022): the Start menu as Mosaïk reads it (the shell's apps folder,
//! `mosaik_win::apps::start_apps`) against Windows' own list (`Get-StartApps`), the sets Mosaïk
//! makes of it, and, with `--start`, an app started through its Start menu entry as a set
//! starts one: File Explorer (see [`start_explorer`]).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Rules, Verdict, classify};
use mosaik_core::layout::Launch;
use mosaik_core::sets::{Installed, Running, suggest};
use mosaik_win::window::{Scanner, handle, top_level};
use serde_json::{Value, json};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::UI::Shell::{IShellWindows, ShellWindows};

/// File Explorer's entry in the Start menu.
const EXPLORER_ID: &str = "Microsoft.Windows.Explorer";

pub fn run(args: &[String]) -> Result<Value, String> {
    let watch = Instant::now();
    let ours =
        mosaik_win::apps::start_apps().map_err(|e| format!("the Start menu could not be read: {e}"))?;
    let read_ms = watch.elapsed().as_secs_f64() * 1000.0;
    let theirs = crate::apps::start_apps()?;
    let ids =
        |list: &mut dyn Iterator<Item = &str>| -> HashSet<String> { list.map(str::to_lowercase).collect() };
    let our_ids = ids(&mut ours.iter().map(|a| a.id.as_str()));
    let their_ids = ids(&mut theirs.iter().map(|a| a.id.as_str()));
    let mut only_windows: Vec<&String> = their_ids.difference(&our_ids).collect();
    only_windows.sort();
    let only_ours = our_ids.difference(&their_ids).count();
    let both = our_ids.intersection(&their_ids).count();
    let packaged = ours.iter().filter(|a| a.id.contains('!')).count();
    let with_program = ours.iter().filter(|a| a.path.is_some()).count();
    println!(
        "sets: {} Start menu apps read in {:.0} ms ({} packaged, {} with their program); Get-StartApps lists {}: {} in both, {} only here, {} only there",
        ours.len(),
        read_ms,
        packaged,
        with_program,
        theirs.len(),
        both,
        only_ours,
        only_windows.len()
    );
    let installed: Vec<Installed> = ours
        .iter()
        .map(|a| Installed {
            name: a.name.clone(),
            id: a.id.clone(),
            path: a.path.clone(),
        })
        .collect();
    // The apps open now, as the engine counts them: the windows it would manage.
    let own = mosaik_win::process::own_pid();
    let running: Vec<Running> = Scanner::new()
        .scan()
        .into_iter()
        .filter(|f| f.pid != own && classify(f, &Rules::default()) == Verdict::Manage)
        .map(|f| Running {
            exe: f.exe,
            aumid: f.aumid,
        })
        .collect();
    let sets = suggest(&installed, &running);
    for s in &sets {
        let apps: Vec<String> = s
            .apps
            .iter()
            .map(|a| {
                if a.running {
                    format!("{} (open)", a.name)
                } else {
                    a.name.clone()
                }
            })
            .collect();
        println!("sets: {}: {}", s.name(), apps.join(", "));
    }
    let started = if args.iter().any(|a| a == "--start") {
        Some(start_explorer())
    } else {
        None
    };
    let ok = !ours.is_empty()
        && only_windows.is_empty()
        && started.as_ref().is_none_or(|s| s["ok"] == json!(true));
    Ok(json!({
        "read": ours.len(),
        "read_ms": read_ms,
        "packaged": packaged,
        "with_program": with_program,
        "windows_lists": theirs.len(),
        "in_both": both,
        "only_here": only_ours,
        "only_there": only_windows.len(),
        "only_there_first": only_windows.iter().take(5).collect::<Vec<_>>(),
        "sets": sets
            .iter()
            .map(|s| json!({
                "name": s.name(),
                "apps": s.apps.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
                "open": s.apps.iter().filter(|a| a.running).count(),
            }))
            .collect::<Vec<_>>(),
        "started": started,
        "ok": ok,
    }))
}

/// How many folder windows and tabs File Explorer has open (on Windows 11, each tab counts).
fn explorer_views() -> Option<i32> {
    // SAFETY: the shell's list of its folder views, created on this thread (COM is initialised
    // in `main`) and asked how many it holds.
    unsafe {
        let views: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        views.Count().ok()
    }
}

/// Starts File Explorer through its Start menu entry, the way a set starts a desktop app
/// (`shell:AppsFolder\<id>`, `mosaik_win::apps::start`), and waits up to 30 s for it: a new
/// window, closed again, or a new tab in a File Explorer window already open (left open:
/// closing one tab from outside could close the window and the user's other tabs).
fn start_explorer() -> Value {
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
    let views_before = explorer_views();
    let watch = Instant::now();
    let launch = Launch::Program {
        path: format!("shell:AppsFolder\\{EXPLORER_ID}"),
        args: Vec::new(),
        working_dir: None,
    };
    if let Err(e) = mosaik_win::apps::start(&launch) {
        println!("sets: File Explorer was not started through its Start menu entry: {e}");
        return json!({ "error": e.to_string(), "ok": false });
    }
    let mut scanner = Scanner::new();
    let mut found = None;
    let mut tab = false;
    while found.is_none() && !tab && watch.elapsed() < Duration::from_secs(30) {
        mosaik_win::events::pump_until(250, || false);
        found = top_level().into_iter().find(|&h| {
            !before.contains(&handle(h).0) && scanner.facts(h).is_some_and(|f| f.class == "CabinetWClass")
        });
        if found.is_none() {
            // A tab shows up in the shell's list a moment before a new window would be seen.
            tab = views_before.zip(explorer_views()).is_some_and(|(b, n)| n > b) && {
                mosaik_win::events::pump_until(1500, || false);
                found = top_level().into_iter().find(|&h| {
                    !before.contains(&handle(h).0)
                        && scanner.facts(h).is_some_and(|f| f.class == "CabinetWClass")
                });
                found.is_none()
            };
        }
    }
    let open_ms = watch.elapsed().as_secs_f64() * 1000.0;
    let closed = found.is_some_and(|h| {
        crate::apps::close(h);
        mosaik_win::events::pump_until(2000, || !mosaik_win::window::exists(h));
        !mosaik_win::window::exists(h)
    });
    let seen = match (found.is_some(), tab) {
        (true, _) => format!("a new window in {open_ms:.0} ms; closed {closed}"),
        (false, true) => {
            format!("a new tab in a File Explorer window already open, in {open_ms:.0} ms (left open)")
        }
        (false, false) => "no new window or tab within 30 s".to_owned(),
    };
    println!("sets: File Explorer started through its Start menu entry: {seen}");
    json!({
        "window": found.is_some(),
        "tab": tab,
        "open_ms": (found.is_some() || tab).then_some(open_ms),
        "closed": closed,
        "ok": (found.is_some() && closed) || tab,
    })
}
