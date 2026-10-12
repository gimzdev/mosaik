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
//! mosaik-probe workspace  save, switch and reopen with the real engine (--drag: drag to snap;
//!                         --browser edge|chrome: browser tabs dragged out into a workspace;
//!                         --apps: real apps into zones and tabs; all three move the mouse)
//! ```
//!
//! The probes only touch windows they open themselves, and close them afterwards.

// A command-line tool: printing is its output.
#![allow(clippy::print_stdout)]

#[cfg(windows)]
mod app_drag;
#[cfg(windows)]
mod apps;
#[cfg(windows)]
mod backdrop;
#[cfg(windows)]
mod browser;
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
mod pixels;
#[cfg_attr(not(windows), allow(dead_code))]
mod report;
#[cfg(windows)]
mod state;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod watchdog;
#[cfg(windows)]
mod workspace;

#[cfg(windows)]
fn main() {
    // MOSAIK_LOG=debug (or info, trace): what the engine under test decides, on stderr.
    if let Ok(level) = std::env::var("MOSAIK_LOG") {
        let level = match level.as_str() {
            "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
            "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
            _ => tracing_subscriber::filter::LevelFilter::INFO,
        };
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .with_max_level(level)
            .with_target(false)
            .try_init();
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Started with no arguments by the engine under test, which starts a workspace's missing
    // apps by their program file: the workspace probe says which window to show.
    if args.is_empty()
        && let Ok(title) = std::env::var(workspace::RELAUNCH_TITLE)
    {
        args = vec!["dummy".into(), "--title".into(), title, "--topmost".into()];
    }
    let Some(command) = args.first().cloned() else {
        eprintln!(
            "usage: mosaik-probe <env|classify|matrix|drag|watchdog|hotkey|idle|workspace> [--out file.json]"
        );
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
        "workspace" => workspace::run(&args),
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
