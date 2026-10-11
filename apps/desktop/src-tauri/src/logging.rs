//! Mosaïk's log: `%LOCALAPPDATA%\Mosaik\logs\mosaik.log`, on this computer only, started afresh
//! when it passes 1 MB. It records what Mosaïk did and how long it took, never window titles.
//! `MOSAIK_LOG=debug` makes it more detailed.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

use tracing::Level;

pub fn folder() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Mosaik").join("logs"))
}

/// Starts logging to the file. Returns where, or `None` if the log cannot be written (Mosaïk
/// then runs without a log rather than not at all).
pub fn start() -> Option<PathBuf> {
    let dir = folder()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("mosaik.log");
    let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > 1024 * 1024);
    let file = OpenOptions::new()
        .create(true)
        .append(!too_big)
        .write(true)
        .truncate(too_big)
        .open(&path)
        .ok()?;
    let level = match std::env::var("MOSAIK_LOG").as_deref() {
        Ok("trace") => Level::TRACE,
        Ok("debug") => Level::DEBUG,
        _ => Level::INFO,
    };
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_max_level(level)
        .with_target(false)
        .try_init()
        .ok()?;
    Some(path)
}
