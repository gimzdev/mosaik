//! CPU and memory of an engine-like loop at rest: the hotkey registered, the window-tracking
//! WinEvent hooks installed (all windows, out of context), and a full reconcile scan every 2 s.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use mosaik_core::hotkey::Hotkey;
use mosaik_win::events;
use mosaik_win::process::{Process, own_pid};
use mosaik_win::window::Scanner;
use serde_json::{Value, json};
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE,
    EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, KillTimer, MSG, MsgWaitForMultipleObjects,
    PM_REMOVE, PeekMessageW, QS_ALLINPUT, SetTimer, TranslateMessage, WM_TIMER,
};

use crate::report::round2;

pub fn run(args: &[String]) -> Result<Value, String> {
    let seconds: u64 = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let me = Process::open(own_pid()).map_err(|e| e.to_string())?;
    let _hotkey =
        mosaik_win::hotkey::register(None, 7, Hotkey::default_switcher()).map_err(|e| e.to_string())?;
    let _hooks = events::hook(
        &[
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ],
        0,
    )
    .map_err(|e| e.to_string())?;
    let mut scanner = Scanner::new();
    // SAFETY: a thread timer (no window); its WM_TIMER is handled in the loop below.
    let timer = unsafe { SetTimer(None, 0, 2000, None) };
    // Warm-up: the first scan fills caches; measure from a settled state.
    scanner.scan();
    std::thread::sleep(Duration::from_secs(2));
    let cpu_start = me.cpu_time().unwrap_or_default();
    let start = Instant::now();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut scans = 0u64;
    let mut msg = MSG::default();
    while start.elapsed() < Duration::from_secs(seconds) {
        // SAFETY: waits for input on this thread's queue; standard message handling.
        unsafe {
            let _ = MsgWaitForMultipleObjects(None, false, 1000, QS_ALLINPUT);
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_TIMER {
                    scanner.scan();
                    scans += 1;
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        for e in events::drain() {
            *counts.entry(format!("{:#06x}", e.event)).or_default() += 1;
        }
    }
    let wall = start.elapsed();
    let cpu = me.cpu_time().unwrap_or_default().saturating_sub(cpu_start);
    // SAFETY: kills the thread timer started above.
    let _ = unsafe { KillTimer(None, timer) };
    let mut info = SYSTEM_INFO::default();
    // SAFETY: fills a sized struct.
    unsafe { GetSystemInfo(&mut info) };
    let cpus = f64::from(info.dwNumberOfProcessors.max(1));
    let one_core = cpu.as_secs_f64() / wall.as_secs_f64() * 100.0;
    let (private, working) = me.memory().unwrap_or((0, 0));
    #[allow(clippy::cast_precision_loss)]
    let mb = |b: usize| round2(b as f64 / 1_048_576.0);
    let result = json!({
        "seconds": round2(wall.as_secs_f64()),
        "cpu_ms": round2(cpu.as_secs_f64() * 1000.0),
        "cpu_percent_of_one_core": round2(one_core),
        "cpu_percent_of_machine": round2(one_core / cpus),
        "logical_cpus": cpus,
        "reconcile_scans": scans,
        "events": counts,
        "private_mb": mb(private),
        "working_set_mb": mb(working),
    });
    println!(
        "{:.0} s: CPU {:.3}% of one core ({:.4}% of the machine), {} scans, private {} MB, working set {} MB",
        wall.as_secs_f64(),
        one_core,
        one_core / cpus,
        scans,
        mb(private),
        mb(working)
    );
    println!("events: {}", result["events"]);
    Ok(result)
}
