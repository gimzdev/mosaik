//! Mosaïk's watchdog: a tiny separate process that waits for the main process to exit and, if it
//! did not clean up (a crash, a kill, a power-user `taskkill`), restores every window it had
//! hidden, using the journal in Mosaïk's data folder and the marker on each window.
//!
//! ```text
//! mosaik-watchdog --watch <pid> --journal <path>   wait for <pid>, then restore what is left
//! mosaik-watchdog --rescue --journal <path>        restore now
//! ```
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

    use mosaik_core::journal::Journal;
    use mosaik_core::store::{Store, StoreError};
    use mosaik_win::process::Process;
    use mosaik_win::rescue::{Report, restore_all};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
    use windows::Win32::System::SystemInformation::GetLocalTime;
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW};
    use windows::core::PCWSTR;

    pub fn run(args: Vec<String>) -> i32 {
        let mut watch: Option<u32> = None;
        let mut journal: Option<PathBuf> = None;
        let mut rescue = false;
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--watch" => watch = it.next().and_then(|p| p.parse().ok()),
                "--journal" => journal = it.next().map(PathBuf::from),
                "--rescue" => rescue = true,
                _ => return 2,
            }
        }
        let Some(journal) = journal else { return 2 };
        let mut log = Log::open(journal.parent().unwrap_or(std::path::Path::new(".")));
        mosaik_win::dpi::make_process_per_monitor_v2();

        if let Some(pid) = watch {
            // Wait for the main process to end, however it ends. If it is already gone,
            // restore straight away.
            log.line(&format!("watching process {pid}"));
            match Process::open(pid) {
                Ok(p) => {
                    while !p.wait(u32::MAX) {}
                    log.line(&format!("process {pid} ended"));
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
        let (report, saved) = rescue_from(&journal, &mut log);
        log.line(&format!(
            "done: {} restored, {} gone, {} failed{}",
            report.restored.len(),
            report.gone,
            report.failed.len(),
            match &saved {
                Ok(()) => String::new(),
                Err(e) => format!("; journal not updated: {e}"),
            }
        ));
        if !report.failed.is_empty() || saved.is_err() {
            tell_user(&report, saved.err());
        }
        if report.restored.is_empty() && report.failed.is_empty() {
            0
        } else if report.failed.is_empty() {
            1
        } else {
            3
        }
    }

    /// Restores everything the journal lists (or, if the journal cannot be read, every window
    /// carrying Mosaïk's marker) and keeps only what could not be restored. The exit code tells
    /// the caller what happened: 0 nothing to do, 1 windows restored, 3 some could not be
    /// restored.
    fn rescue_from(journal_path: &std::path::Path, log: &mut Log) -> (Report, Result<(), StoreError>) {
        let dir = journal_path.parent().map(PathBuf::from).unwrap_or_default();
        let store = Store::new(dir);
        // A damaged journal is kept aside by the store; the markers still find the windows.
        let journal = match store.load::<Journal>() {
            Ok(loaded) => loaded.value,
            Err(e) => {
                log.line(&format!("journal unreadable ({e}); using window markers"));
                Journal::default()
            }
        };
        log.line(&format!("journal lists {} window(s)", journal.hidden.len()));
        let report = restore_all(&journal);
        let mut saved = Ok(());
        if !journal.hidden.is_empty() || !report.remaining.is_empty() {
            let mut left = Journal::new(journal.owner_pid);
            left.hidden.clone_from(&report.remaining);
            saved = store.save(&left);
        }
        (report, saved)
    }

    /// The watchdog has no window of its own: when something could not be put right, it says
    /// so in a message box, with what to do next.
    fn tell_user(report: &Report, save_error: Option<StoreError>) {
        let mut text = format!(
            "Mosaïk stopped unexpectedly and brought your windows back.\n\n{}.",
            report.summary()
        );
        if !report.failed.is_empty() {
            text.push_str(
                "\n\nWindows that could not be restored stay listed: start Mosaïk and choose Rescue windows in its tray menu, or run \"mosaikctl rescue\".",
            );
        }
        if let Some(e) = save_error {
            text.push_str(&format!(
                "\n\nThe list of hidden windows could not be updated: {e}"
            ));
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
