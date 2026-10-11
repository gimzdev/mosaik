//! Recovery after the main process dies: a stand-in main process hides four windows (one with
//! each method, plus a real app shell-cloaked), records them in the journal, starts the real
//! watchdog and waits. The probe kills it and times how long until every window is back.

use std::time::Instant;

use mosaik_core::geometry::Rect;
use mosaik_core::journal::{HiddenWindow, HideMethod, Journal};
use mosaik_core::store::Store;
use mosaik_core::window::{Cloak, WindowHandle};
use mosaik_win::hide::Hider;
use mosaik_win::process::{Process, own_pid, spawn};
use mosaik_win::window::{Scanner, cloak, frame_rect, handle, hwnd, pid, title};
use serde_json::{Value, json};
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

use crate::apps::{catalog, close, open};
use crate::dummy;
use crate::report::round2;

fn journal_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("mosaik-probe-watchdog")
}

fn visible(h: windows::Win32::Foundation::HWND) -> bool {
    // SAFETY: read-only.
    let shown = unsafe { IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() };
    shown && cloak(h) == Cloak::None
}

/// The stand-in main process: `watchdog-main --windows h1,h2,.. --methods m1,m2,..`.
pub fn main_process(args: &[String]) -> Result<Value, String> {
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
    let methods: Vec<HideMethod> = get("--methods")
        .split(',')
        .filter_map(|m| match m {
            "cloak" => Some(HideMethod::ShellCloak),
            "minimize" => Some(HideMethod::Minimize),
            "park" => Some(HideMethod::Park),
            _ => None,
        })
        .collect();
    let store = Store::new(journal_dir());
    let mut journal = Journal::new(own_pid());
    let mut hider = Hider::new();
    for (h, m) in handles.iter().zip(methods) {
        let w = hwnd(WindowHandle(*h));
        let hidden = hider.hide(w, m).map_err(|e| e.to_string())?;
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
    let watchdog = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("mosaik-watchdog.exe");
    let journal_path = journal_dir().join("hidden-windows.json");
    spawn(
        &watchdog.to_string_lossy(),
        &format!("--watch {} --journal \"{}\"", own_pid(), journal_path.display()),
    )
    .map_err(|e| e.to_string())?;
    // Wait to be killed.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

pub fn run() -> Result<Value, String> {
    let _ = std::fs::remove_dir_all(journal_dir());
    std::fs::create_dir_all(journal_dir()).map_err(|e| e.to_string())?;
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
        &format!("watchdog-main --windows {list} --methods {methods}"),
    )
    .map_err(|e| e.to_string())?;
    // Wait until the stand-in has hidden everything and started its watchdog.
    let store = Store::new(journal_dir());
    let ready = mosaik_win::events::pump_until(15_000, || {
        store
            .load::<Journal>()
            .is_ok_and(|j| j.value.hidden.len() == windows.len())
            && windows.iter().all(|(h, _)| !visible(*h))
    });
    if !ready {
        let _ = main.kill();
        return Err("the stand-in main process did not hide every window".into());
    }
    mosaik_win::events::pump_until(500, || false);
    let killed = Instant::now();
    main.kill().map_err(|e| e.to_string())?;
    let all_back = mosaik_win::events::pump_until(10_000, || windows.iter().all(|(h, _)| visible(*h)));
    let restore_ms = killed.elapsed().as_secs_f64() * 1000.0;
    mosaik_win::events::pump_until(300, || false);
    let rows: Vec<Value> = windows
        .iter()
        .zip(&frames_before)
        .map(|((h, m), before)| {
            let after = frame_rect(*h);
            json!({
                "method": m,
                "visible": visible(*h),
                "frame_error_px": before.zip(after).map(|(b, a)| mosaik_win::place::edge_error(b, a)),
            })
        })
        .collect();
    let journal_after = store
        .load::<Journal>()
        .map(|j| j.value.hidden.len())
        .unwrap_or(usize::MAX);
    for (h, _) in &windows {
        close(*h);
    }
    for p in &processes {
        if !p.wait(3000) {
            let _ = p.kill();
        }
    }
    let result = json!({
        "all_restored": all_back,
        "kill_to_all_visible_ms": round2(restore_ms),
        "windows": rows,
        "journal_entries_left": journal_after,
    });
    println!(
        "all restored: {all_back} in {restore_ms:.0} ms after the kill; journal entries left: {journal_after}"
    );
    for r in &rows {
        println!("  {r}");
    }
    Ok(result)
}
