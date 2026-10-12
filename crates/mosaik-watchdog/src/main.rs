//! Mosaïk's watchdog: a tiny separate process that waits for the main process to exit and, if it
//! did not clean up (a crash, a kill, a power-user `taskkill`), restores every window it had
//! hidden, using the journal in Mosaïk's data folder and the marker on each window.
//!
//! ```text
//! mosaik-watchdog --watch <pid> --journal <path>   wait for <pid>, then restore what is left
//! mosaik-watchdog --rescue --journal <path>        restore now
//! ```
//!
//! `--quiet` writes to the log only, without the message box (for scripts and the probes).
//!
//! Each run appends a few lines to `watchdog.log` next to the journal (timings and results, never
//! window titles), so a rescue that went wrong can be understood afterwards.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    std::process::exit(windows_main::run(std::env::args().skip(1).collect()));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("mosaik-watchdog only runs on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
mod windows_main {
    use std::path::PathBuf;

    use std::io::Write;
    use std::time::Instant;

    use mosaik_core::store::Store;
    use mosaik_win::process::Process;
    use mosaik_win::rescue::{Outcome, restore_hidden};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::System::SystemInformation::GetLocalTime;
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW};
    use windows::core::PCWSTR;

    /// Exit code `mosaik.exe` uses when it stops itself after telling the user why it could not
    /// start: the watchdog then has nothing to add.
    const STARTUP_FAILURE_REPORTED: u32 = 2;

    pub fn run(args: Vec<String>) -> i32 {
        let mut watch: Option<u32> = None;
        let mut journal: Option<PathBuf> = None;
        let mut rescue = false;
        let mut quiet = false;
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--watch" => watch = it.next().and_then(|p| p.parse().ok()),
                "--journal" => journal = it.next().map(PathBuf::from),
                "--rescue" => rescue = true,
                "--quiet" => quiet = true,
                _ => return 2,
            }
        }
        let Some(journal) = journal else { return 2 };
        let mut log = Log::open(journal.parent().unwrap_or(std::path::Path::new(".")));
        mosaik_win::dpi::make_process_per_monitor_v2();

        // How the main process ended: `Some(0)` when it quit normally.
        let mut ended_with = None;
        if let Some(pid) = watch {
            // Wait for the main process to end, however it ends. If it is already gone,
            // restore straight away.
            log.line(&format!("watching process {pid}"));
            match Process::open(pid) {
                Ok(p) => {
                    while !p.wait(u32::MAX) {}
                    ended_with = p.exit_code();
                    log.line(&format!("process {pid} ended with code {ended_with:?}"));
                }
                Err(e) => log.line(&format!("process {pid} not found ({e}); restoring now")),
            }
        } else if rescue {
            log.line("rescue requested");
        } else {
            return 2;
        }
        // COM only now: a single-threaded apartment must pump messages, and nothing is pumped
        // while waiting above.
        // SAFETY: initialises COM for this thread once, before any COM call (shell cloak).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        log.line("COM ready");
        let dir = journal.parent().map(PathBuf::from).unwrap_or_default();
        let outcome = restore_hidden(&Store::new(dir));
        let report = &outcome.report;
        log.line(&format!(
            "done: {} restored, {} gone, {} failed{}",
            report.restored.len(),
            report.gone,
            report.failed.len(),
            outcome
                .problem
                .as_ref()
                .map(|p| format!("; {p}"))
                .unwrap_or_default()
        ));
        let crashed = watch.is_some() && !matches!(ended_with, Some(0) | Some(STARTUP_FAILURE_REPORTED));
        if !quiet && (crashed || !report.failed.is_empty() || outcome.problem.is_some()) {
            tell_user(&outcome, crashed);
        }
        if report.restored.is_empty() && report.failed.is_empty() {
            0
        } else if report.failed.is_empty() {
            1
        } else {
            3
        }
    }

    /// The watchdog has no window of its own: it says what happened in a message box, with
    /// what to do next.
    fn tell_user(outcome: &Outcome, crashed: bool) {
        let report = &outcome.report;
        let mut text = if crashed {
            String::from("Mosaïk has stopped.")
        } else {
            String::from("Mosaïk could not bring every window back.")
        };
        if !report.restored.is_empty() {
            text.push_str(&format!(
                " The {} window(s) it had hidden are back.",
                report.restored.len()
            ));
        }
        if !report.failed.is_empty() {
            text.push_str(&format!(
                "\n\n{} window(s) could not be restored (usually apps running as administrator). Start Mosaïk and choose Rescue windows in its tray menu, or run \"mosaikctl rescue\" as administrator.",
                report.failed.len()
            ));
        }
        if let Some(p) = &outcome.problem {
            text.push_str(&format!("\n\n{p}"));
        }
        if crashed {
            text.push_str("\n\nStart Mosaïk again from the Start menu. What happened is in %LOCALAPPDATA%\\Mosaik\\logs.");
        }
        let text = mosaik_win::text::wide(&text);
        let title = mosaik_win::text::wide("Mosaïk");
        // SAFETY: both strings outlive the call; no owner window.
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
            );
        }
    }

    /// The watchdog's log: a few lines per run, appended, restarted when it grows past 64 KB.
    struct Log {
        file: Option<std::fs::File>,
        start: Instant,
    }

    impl Log {
        fn open(dir: &std::path::Path) -> Self {
            let path = dir.join("watchdog.log");
            let too_big = std::fs::metadata(&path).is_ok_and(|m| m.len() > 64 * 1024);
            let file = std::fs::create_dir_all(dir).ok().and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(!too_big)
                    .write(true)
                    .truncate(too_big)
                    .open(&path)
                    .ok()
            });
            Self {
                file,
                start: Instant::now(),
            }
        }

        fn line(&mut self, text: &str) {
            // SAFETY: GetLocalTime only fills the returned struct.
            let t = unsafe { GetLocalTime() };
            let elapsed = self.start.elapsed().as_millis();
            if let Some(f) = &mut self.file {
                let _ = writeln!(
                    f,
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} +{elapsed} ms  {text}",
                    t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
                );
            }
        }
    }
}
