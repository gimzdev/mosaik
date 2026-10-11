//! Mosaïk's watchdog: a tiny separate process that waits for the main process to exit and, if it
//! did not clean up (a crash, a kill, a power-user `taskkill`), restores every window it had
//! hidden, using the journal in Mosaïk's data folder and the marker on each window.
//!
//! ```text
//! mosaik-watchdog --watch <pid> --journal <path>   wait for <pid>, then restore what is left
//! mosaik-watchdog --rescue --journal <path>        restore now
//! ```
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

    use mosaik_core::journal::Journal;
    use mosaik_core::store::Store;
    use mosaik_win::process::Process;
    use mosaik_win::rescue::{Report, restore_all};
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

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
        mosaik_win::dpi::make_process_per_monitor_v2();
        // SAFETY: initialises COM for this thread once, before any COM call (shell cloak).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

        if let Some(pid) = watch {
            // Wait for the main process to end, however it ends. If it is already gone,
            // restore straight away.
            if let Ok(p) = Process::open(pid) {
                while !p.wait(u32::MAX) {}
            }
        } else if !rescue {
            return 2;
        }
        let report = rescue_from(&journal);
        if report.restored.is_empty() && report.failed.is_empty() {
            0
        } else if report.failed.is_empty() {
            1
        } else {
            3
        }
    }

    /// Restores everything the journal lists and clears it. The exit code tells the caller what
    /// happened: 0 nothing to do, 1 windows restored, 3 some could not be restored.
    fn rescue_from(journal_path: &std::path::Path) -> Report {
        let dir = journal_path.parent().map(PathBuf::from).unwrap_or_default();
        let store = Store::new(dir);
        let journal = store.load::<Journal>().map(|l| l.value).unwrap_or_default();
        let report = restore_all(&journal);
        if !journal.hidden.is_empty() {
            let _ = store.save(&Journal::new(journal.owner_pid));
        }
        report
    }
}
