//! Sets of apps (ADR 0022): the Start menu as Mosaïk reads it (the shell's apps folder,
//! `mosaik_win::apps::start_apps`) against Windows' own list (`Get-StartApps`), the sets Mosaïk
//! makes of it with each app's icon as Mosaïk's window shows it, and, with `--start`, an app
//! started through its Start menu entry as a set starts one: Character Map (see
//! [`start_character_map`]).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use mosaik_core::classify::{Rules, Verdict, classify};
use mosaik_core::layout::Launch;
use mosaik_core::sets::{Installed, Running, suggest};
use mosaik_win::window::{Scanner, handle, top_level};
use serde_json::{Value, json};

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
    // Each app's picture, as Mosaïk's window shows it beside the set (job 0123: only the
    // Terminal had one).
    let keys: Vec<Option<String>> = sets
        .iter()
        .flat_map(|s| s.apps.iter())
        .map(|a| mosaik_core::icons::of_app(&a.app, Some(&a.launch)))
        .collect();
    let wanted: Vec<String> = keys.iter().flatten().cloned().collect();
    let pictures: std::collections::HashMap<String, Option<usize>> =
        match mosaik_win::icons::IconReader::start(48) {
            Ok(reader) => reader
                .read(wanted, Duration::from_secs(20))
                .into_iter()
                .map(|(k, png)| (k, png.map(|p| p.len())))
                .collect(),
            Err(e) => {
                println!("sets: the icon reader did not start: {e}");
                std::collections::HashMap::new()
            }
        };
    let icons: Vec<Value> = sets
        .iter()
        .flat_map(|s| s.apps.iter())
        .zip(&keys)
        .map(|(a, k)| {
            let bytes = k.as_ref().and_then(|k| pictures.get(k).copied().flatten());
            json!({ "app": a.name, "key": k, "png_bytes": bytes })
        })
        .collect();
    let with_picture = icons.iter().filter(|i| !i["png_bytes"].is_null()).count();
    println!("sets: {with_picture} of {} apps with their own icon", icons.len());
    for i in icons.iter().filter(|i| i["png_bytes"].is_null()) {
        println!("sets: no icon for {} ({})", i["app"], i["key"]);
    }
    let started = if args.iter().any(|a| a == "--start") {
        Some(start_character_map(&ours))
    } else {
        None
    };
    let ok = !ours.is_empty()
        && only_windows.is_empty()
        && with_picture == icons.len()
        && started
            .as_ref()
            .is_none_or(|s| s.is_null() || s["ok"] == json!(true));
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
        "icons": icons,
        "started": started,
        "ok": ok,
    }))
}

/// Starts Character Map through its Start menu entry, the way a set starts a desktop app
/// (`shell:AppsFolder\\<id>`, on a thread of its own: `mosaik_win::apps::start_on_its_own_thread`,
/// as the engine does), waits up to 20 s for its window and closes it. Character Map is small,
/// changes nothing, and opens a new window each time. File Explorer, tried first, showed a new
/// window once (job 0110) and none in jobs 0111 and 0112, with its own windows open.
fn start_character_map(apps: &[mosaik_win::apps::StartApp]) -> Value {
    let program = |a: &mosaik_win::apps::StartApp| {
        a.path
            .as_deref()
            .unwrap_or(&a.id)
            .to_lowercase()
            .ends_with("\\charmap.exe")
    };
    let Some(entry) = apps.iter().find(|a| program(a)) else {
        println!("sets: no Character Map in the Start menu: nothing started");
        return Value::Null;
    };
    let before: HashSet<u64> = top_level().into_iter().map(|h| handle(h).0).collect();
    let watch = Instant::now();
    let launch = Launch::Program {
        path: format!("shell:AppsFolder\\{}", entry.id),
        args: Vec::new(),
        working_dir: None,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    if let Err(e) = mosaik_win::apps::start_on_its_own_thread(launch, move |outcome| {
        let _ = tx.send(outcome.map_err(|e| e.to_string()));
    }) {
        println!("sets: Character Map's launch thread did not start: {e}");
        return json!({ "error": e.to_string(), "ok": false });
    }
    let started = rx.recv_timeout(Duration::from_secs(20));
    let started_ms = watch.elapsed().as_secs_f64() * 1000.0;
    let failure = match &started {
        Ok(Ok(_)) => None,
        Ok(Err(e)) => Some(e.clone()),
        Err(e) => Some(format!("no answer within 20 s ({e})")),
    };
    if let Some(e) = failure {
        println!("sets: Character Map was not started through its Start menu entry: {e}");
        return json!({ "error": e, "ok": false });
    }
    let mut scanner = Scanner::new();
    // Character Map's own window, shown, as the engine would manage it (job 0116 closed another
    // window of the program, and left its window open).
    let new_windows = |scanner: &mut Scanner| -> Vec<windows::Win32::Foundation::HWND> {
        top_level()
            .into_iter()
            .filter(|&h| {
                !before.contains(&handle(h).0)
                    && scanner.facts(h).is_some_and(|f| {
                        f.exe
                            .as_deref()
                            .is_some_and(|e| e.eq_ignore_ascii_case("charmap.exe"))
                            && classify(&f, &Rules::default()) == Verdict::Manage
                    })
            })
            .collect()
    };
    let mut found = None;
    while found.is_none() && watch.elapsed() < Duration::from_secs(20) {
        mosaik_win::events::pump_until(250, || false);
        found = new_windows(&mut scanner).first().copied();
    }
    let open_ms = watch.elapsed().as_secs_f64() * 1000.0;
    // A moment for a second window, had it been started twice.
    mosaik_win::events::pump_until(1500, || false);
    let shown = new_windows(&mut scanner);
    for &h in &shown {
        crate::apps::close(h);
    }
    mosaik_win::events::pump_until(2000, || shown.iter().all(|&h| !mosaik_win::window::exists(h)));
    let closed = found.is_some() && shown.iter().all(|&h| !mosaik_win::window::exists(h));
    println!(
        "sets: Character Map started through its Start menu entry ({:.0} ms to start): {}",
        started_ms,
        if found.is_some() {
            format!(
                "its window in {open_ms:.0} ms; {} window(s); closed {closed}",
                shown.len()
            )
        } else {
            "no window within 20 s".to_owned()
        }
    );
    json!({
        "start_ms": started_ms,
        "window": found.is_some(),
        "open_ms": found.map(|_| open_ms),
        "windows": shown.len(),
        "closed": closed,
        "ok": found.is_some() && shown.len() == 1 && closed,
    })
}
