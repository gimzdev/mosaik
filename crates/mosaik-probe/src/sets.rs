//! Sets of apps (ADR 0022): the Start menu as Mosaïk reads it (the shell's apps folder,
//! `mosaik_win::apps::start_apps`) against Windows' own list (`Get-StartApps`), the sets Mosaïk
//! makes of it, and, with `--start`, an app started through its Start menu entry as a set
//! starts one: File Explorer, whose new window is closed again.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use mosaik_core::layout::Launch;
use mosaik_core::sets::{Installed, suggest};
use mosaik_win::window::{Scanner, handle, top_level};
use serde_json::{Value, json};

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
    let sets = suggest(&installed, &[]);
    for s in &sets {
        let apps: Vec<&str> = s.apps.iter().map(|a| a.name.as_str()).collect();
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
            }))
            .collect::<Vec<_>>(),
        "started": started,
        "ok": ok,
    }))
}

/// Starts File Explorer through its Start menu entry, the way a set starts a desktop app
/// (`shell:AppsFolder\<id>`, `mosaik_win::apps::start`), waits up to 15 s for its new window,
/// and closes it.
fn start_explorer() -> Value {
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
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
    while found.is_none() && watch.elapsed() < Duration::from_secs(15) {
        mosaik_win::events::pump_until(250, || false);
        found = top_level().into_iter().find(|&h| {
            !before.contains(&handle(h).0) && scanner.facts(h).is_some_and(|f| f.class == "CabinetWClass")
        });
    }
    let open_ms = watch.elapsed().as_secs_f64() * 1000.0;
    let closed = found.is_some_and(|h| {
        crate::apps::close(h);
        mosaik_win::events::pump_until(2000, || !mosaik_win::window::exists(h));
        !mosaik_win::window::exists(h)
    });
    println!(
        "sets: File Explorer started through its Start menu entry: {}; closed {closed}",
        if found.is_some() {
            format!("a new window in {open_ms:.0} ms")
        } else {
            "no new window within 15 s".to_owned()
        }
    );
    json!({
        "window": found.is_some(),
        "open_ms": found.map(|_| open_ms),
        "closed": closed,
        "ok": found.is_some() && closed,
    })
}
