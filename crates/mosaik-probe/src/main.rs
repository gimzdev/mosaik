//! Measurements behind Mosaïk's design decisions (see `docs/adr`). Each command prints a short
//! summary and writes the full numbers as JSON with `--out <file>`.
//!
//! ```text
//! mosaik-probe env        monitors and DPI as Mosaïk sees them
//! mosaik-probe classify   how every open window is classified (titles are not recorded)
//! mosaik-probe matrix     placement error and hiding methods across real apps
//! mosaik-probe drag       zone-preview latency while dragging a window (moves the mouse)
//! mosaik-probe watchdog   window recovery after the main process is killed
//! mosaik-probe hotkey     the switcher shortcut on the installed keyboard layouts (presses keys)
//! mosaik-probe idle       CPU and memory of an engine-like loop over 60 s
//! ```
//!
//! The probes only touch windows they open themselves, and close them afterwards.

// A command-line tool: printing is its output.
#![allow(clippy::print_stdout)]

#[cfg(windows)]
mod apps;
#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod classify;
#[cfg(windows)]
mod drag;
#[cfg(windows)]
mod dummy;
#[cfg(windows)]
mod env;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
mod idle;
#[cfg(windows)]
mod kbd;
#[cfg(windows)]
mod matrix;
#[cfg_attr(not(windows), allow(dead_code))]
mod report;
#[cfg(windows)]
mod watchdog;

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        eprintln!("usage: mosaik-probe <env|classify|matrix|drag|watchdog|hotkey|idle> [--out file.json]");
        std::process::exit(2);
    };
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned();
    mosaik_win::dpi::make_process_per_monitor_v2();
    // SAFETY: initialises COM once for this thread before any COM use.
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };
    let result = match command.as_str() {
        "env" => env::run(),
        "classify" => classify::run(),
        "matrix" => matrix::run(&args),
        "drag" => drag::run(),
        "watchdog" => watchdog::run(),
        "watchdog-main" => watchdog::main_process(&args),
        "hotkey" => hotkey::run(),
        "idle" => idle::run(&args),
        "dummy" => dummy::run(&args),
        _ => Err(format!("unknown command {command}")),
    };
    match result {
        Ok(value) => {
            if let Some(path) = out
                && let Err(e) = report::write(&path, &value)
            {
                eprintln!("could not write {path}: {e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("probe failed: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("mosaik-probe only runs on Windows");
    std::process::exit(2);
}
