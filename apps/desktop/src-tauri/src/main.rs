//! `mosaik.exe`: the engine, the tray icon and the interface windows in one process, plus the
//! watchdog beside it.
//!
//! ```text
//! mosaik                start (or, if Mosaïk already runs, open its settings)
//! mosaik --safe-mode    start without moving, hiding or registering anything
//! mosaik --rescue       bring back every window Mosaïk hid, then exit
//! ```

// No console window in release builds; debug builds keep one for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod logging;

#[cfg(windows)]
fn main() {
    std::process::exit(app::run(std::env::args().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Mosaïk runs on Windows.");
    std::process::exit(2);
}
