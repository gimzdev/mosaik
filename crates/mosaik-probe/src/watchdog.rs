//! Recovery after the main process dies: a stand-in main process hides four windows (one with
//! each method, plus a real app shell-cloaked), records them in the journal and waits. The probe
//! kills it and times how long until every window is back, with the watchdog's exit code and
//! log. Run twice: once with the watchdog started by the probe, once started by the stand-in
//! itself, as Mosaïk starts it (the watchdog must outlive the process that started it).

use std::time::Instant;

use mosaik_core::geometry::Rect;
use mosaik_core::journal::{HiddenWindow, HideMethod, Journal};
use mosaik_core::store::Store;
use mosaik_core::window::WindowHandle;
use mosaik_win::hide::Hider;
use mosaik_win::process::{Process, own_pid, spawn};
use mosaik_win::window::{Scanner, frame_rect, handle, hwnd, pid, title};
use serde_json::{Value, json};

use crate::apps::{catalog, close, open};
use crate::dummy;
use crate::report::round2;
use crate::state::{is_hidden, is_shown};

fn journal_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("mosaik-probe-watchdog")
}

fn journal_path() -> std::path::PathBuf {
    journal_dir().join(mosaik_core::journal::FILE)
}

/// Where the stand-in writes what went wrong, since nobody reads its console.
fn error_file() -> std::path::PathBuf {
    journal_dir().join("stand-in-error.txt")
}

/// Written by the stand-in once everything is hidden and recorded: the watchdog's process id
/// when the stand-in started it, else empty.
fn ready_file() -> std::path::PathBuf {
    journal_dir().join("stand-in-ready.txt")
}

fn method_named(m: &str) -> Option<HideMethod> {
    match m {
        "cloak" => Some(HideMethod::ShellCloak),
        "minimize" => Some(HideMethod::Minimize),
        "park" => Some(HideMethod::Park),
        _ => None,
    }
}

fn watchdog_exe() -> Result<std::path::PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("mosaik-watchdog.exe"))
}

fn start_watchdog(watch: u32) -> Result<Process, String> {
    spawn(
        &watchdog_exe()?.to_string_lossy(),
        &format!(
            "--watch {watch} --quiet --journal \"{}\"",
            journal_path().display()
        ),
    )
    .map_err(|e| e.to_string())
}

/// The stand-in main process: `watchdog-main --windows h1,h2,.. --methods m1,m2,..
/// [--start-watchdog]`.
pub fn main_process(args: &[String]) -> Result<Value, String> {
    let result = stand_in(args);
    if let Err(e) = &result {
        let _ = std::fs::write(error_file(), e);
    }
    result
}

fn stand_in(args: &[String]) -> Result<Value, String> {
    let get = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_default()
    };
    let handles: Vec<u64> = get("--windows")
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let methods: Vec<HideMethod> = get("--methods").split(',').filter_map(method_named).collect();
    // As Mosaïk does: the watchdog first, then anything hidden.
    let watchdog = if args.iter().any(|a| a == "--start-watchdog") {
        Some(start_watchdog(own_pid())?)
    } else {
        None
    };
    let store = Store::new(journal_dir());
    let mut journal = Journal::new(own_pid());
    let mut hider = Hider::new();
    for (h, m) in handles.iter().zip(methods) {
        let w = hwnd(WindowHandle(*h));
        let hidden = hider
            .hide(w, m)
            .map_err(|e| format!("hiding window {h} with {m:?}: {e}"))?;
        journal.record(HiddenWindow {
            handle: WindowHandle(*h),
            pid: pid(w),
            method: m,
            restore_rect: hidden.restore_rect,
            was_maximized: hidden.was_maximized,
            exe: None,
            title: title(w),
        });
        store.save(&journal).map_err(|e| e.to_string())?;
    }
    let pid = watchdog.as_ref().map(|w| w.pid.to_string()).unwrap_or_default();
    std::fs::write(ready_file(), pid).map_err(|e| e.to_string())?;
    // Wait to be killed.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

/// The watchdog on its own first: `--rescue` with nothing to do must exit 0 and log.
fn smoke_test() -> Result<(), String> {
    let dir = journal_dir().join("smoke");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let w = spawn(
        &watchdog_exe()?.to_string_lossy(),
        &format!(
            "--rescue --quiet --journal \"{}\"",
            dir.join(mosaik_core::journal::FILE).display()
        ),
    )
    .map_err(|e| e.to_string())?;
    if !w.wait(10_000) {
        let _ = w.kill();
        return Err("the watchdog did not finish a rescue with nothing to do within 10 s".into());
    }
    let code = w.exit_code();
    let log = std::fs::read_to_string(dir.join("watchdog.log")).unwrap_or_default();
    println!("smoke test: exit {code:?}, log {} line(s)", log.lines().count());
    if code != Some(0) || log.is_empty() {
        return Err(format!(
            "the watchdog's smoke test failed: exit {code:?}, log {log:?}"
        ));
    }
    Ok(())
}

pub fn run() -> Result<Value, String> {
    let _ = std::fs::remove_dir_all(journal_dir());
    std::fs::create_dir_all(journal_dir()).map_err(|e| e.to_string())?;
    smoke_test()?;
    let mut results = serde_json::Map::new();
    for (name, by_stand_in) in [
        ("started by the probe", false),
        ("started by the main process", true),
    ] {
        println!("== watchdog {name}");
        let r = scenario(by_stand_in)?;
        results.insert(name.into(), r);
    }
    Ok(Value::Object(results))
}

fn scenario(by_stand_in: bool) -> Result<Value, String> {
    for f in [
        journal_path(),
        ready_file(),
        error_file(),
        journal_dir().join("watchdog.log"),
    ] {
        let _ = std::fs::remove_file(f);
    }
    let work = mosaik_win::monitors::topology()
        .map_err(|e| e.to_string())?
        .primary()
        .work_area;
    let mut windows = Vec::new();
    let mut processes: Vec<Process> = Vec::new();
    for (i, method) in ["cloak", "minimize", "park"].iter().enumerate() {
        let off = i32::try_from(i).unwrap_or(0) * 60;
        let (p, h) = dummy::spawn(
            &format!("Mosaïk watchdog probe {method}"),
            Rect::from_origin_size(work.left + 100 + off, work.top + 100 + off, 500, 340),
            false,
        )?;
        processes.push(p);
        windows.push((h, *method));
    }
    // A real app too, shell-cloaked, to see the cloak lifted from a window of another program.
    let mut scanner = Scanner::new();
    let notepad = catalog()
        .into_iter()
        .find(|a| a.name == "Notepad")
        .map(|a| open(&a, &mut scanner));
    if let Some(Ok(n)) = &notepad {
        windows.push((n.hwnd, "cloak"));
    }
    let frames_before: Vec<Option<Rect>> = windows.iter().map(|(h, _)| frame_rect(*h)).collect();
    let list = windows
        .iter()
        .map(|(h, _)| handle(*h).0.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let methods = windows.iter().map(|(_, m)| *m).collect::<Vec<_>>().join(",");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let main = spawn(
        &exe.to_string_lossy(),
        &format!(
            "watchdog-main --windows {list} --methods {methods}{}",
            if by_stand_in { " --start-watchdog" } else { "" }
        ),
    )
    .map_err(|e| e.to_string())?;
    // Wait until the stand-in has hidden everything.
    let store = Store::new(journal_dir());
    let hidden_each = || {
        windows
            .iter()
            .all(|(h, m)| method_named(m).is_some_and(|m| is_hidden(*h, m)))
    };
    let ready = mosaik_win::events::pump_until(15_000, || {
        ready_file().exists()
            && store
                .load::<Journal>()
                .is_ok_and(|j| j.value.hidden.len() == windows.len())
            && hidden_each()
    });
    let give_back = |windows: &[(windows::Win32::Foundation::HWND, &str)]| {
        let journal = store
            .load::<Journal>()
            .map(|j| j.value)
            .unwrap_or_else(|_| Journal::new(0));
        let _ = mosaik_win::rescue::restore_all(&journal);
        for (h, _) in windows {
            close(*h);
        }
    };
    if !ready {
        let _ = main.kill();
        let why = std::fs::read_to_string(error_file()).unwrap_or_else(|_| {
            let still: Vec<&str> = windows
                .iter()
                .filter(|(h, m)| !method_named(m).is_some_and(|m| is_hidden(*h, m)))
                .map(|(_, m)| *m)
                .collect();
            format!("windows not hidden: {still:?}")
        });
        // Whatever happened, give every window back before failing.
        give_back(&windows);
        return Err(format!(
            "the stand-in main process did not hide every window ({why})"
        ));
    }
    let watchdog = if by_stand_in {
        let pid: u32 = std::fs::read_to_string(ready_file())
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .ok_or("the stand-in did not say which watchdog it started")?;
        Process::open(pid).map_err(|e| format!("the stand-in's watchdog is not running: {e}"))?
    } else {
        start_watchdog(main.pid)?
    };
    mosaik_win::events::pump_until(500, || false);
    let watching = !watchdog.wait(0);
    let killed = Instant::now();
    main.kill().map_err(|e| e.to_string())?;
    let all_back = mosaik_win::events::pump_until(30_000, || windows.iter().all(|(h, _)| is_shown(*h)));
    let restore_ms = killed.elapsed().as_secs_f64() * 1000.0;
    let watchdog_exited = watchdog.wait(5_000);
    let watchdog_exit = watchdog.exit_code();
    if !watchdog_exited {
        let _ = watchdog.kill();
    }
    mosaik_win::events::pump_until(300, || false);
    let rows: Vec<Value> = windows
        .iter()
        .zip(&frames_before)
        .map(|((h, m), before)| {
            let after = frame_rect(*h);
            json!({
                "method": m,
                "shown": is_shown(*h),
                "frame_error_px": before.zip(after).map(|(b, a)| mosaik_win::place::edge_error(b, a)),
            })
        })
        .collect();
    let journal_after = store
        .load::<Journal>()
        .map(|j| j.value.hidden.len())
        .unwrap_or(usize::MAX);
    if all_back {
        for (h, _) in &windows {
            close(*h);
        }
    } else {
        give_back(&windows);
    }
    for p in &processes {
        if !p.wait(3000) {
            let _ = p.kill();
        }
    }
    let log = std::fs::read_to_string(journal_dir().join("watchdog.log")).unwrap_or_default();
    println!(
        "all restored: {all_back} in {restore_ms:.0} ms after the kill; journal entries left: {journal_after}"
    );
    println!(
        "watchdog: watching before the kill {watching}, exit code {watchdog_exit:?}{}",
        if watchdog_exited {
            ""
        } else {
            " (still running; stopped)"
        }
    );
    for r in &rows {
        println!("  {r}");
    }
    println!("watchdog log:");
    for line in log.lines() {
        println!("  {line}");
    }
    Ok(json!({
        "all_restored": all_back,
        "kill_to_all_visible_ms": round2(restore_ms),
        "watchdog_was_watching": watching,
        "watchdog_exit_code": watchdog_exit,
        "windows": rows,
        "journal_entries_left": journal_after,
        "watchdog_log": log.lines().collect::<Vec<_>>(),
    }))
}
