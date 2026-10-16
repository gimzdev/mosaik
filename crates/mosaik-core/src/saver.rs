//! Writing a file Mosaïk keeps off the thread that changes it.
//!
//! The engine's thread changes the workspaces as the user works (a tab clicked, a window
//! dropped), and each write ends with the disk confirming it (`sync_all`): a click waited as long
//! as the disk did, three times for a tab (the review of 2026-10-15). A [`Saver`] writes the
//! latest value on a thread of its own once changes stop for a moment ([`Saver::later`]), at
//! least every [`MOST_WAIT`] while they go on; a write the user asked for and waits on
//! ([`Saver::now`]) goes at once, and nothing older is written after it.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::store::{Saved, Store};

/// While changes keep coming, the latest is still written this often.
pub const MOST_WAIT: Duration = Duration::from_secs(2);

enum Job<T> {
    /// Write this once changes stop (it replaces any value waiting).
    Later(Box<T>),
    /// Write this now and say how it went (any value waiting is older: it is dropped).
    Now(Box<T>, mpsc::Sender<Result<(), String>>),
    /// Write the value waiting now, then say so.
    Flush(mpsc::Sender<()>),
}

/// A thread that writes values of `T` to `store`, latest first.
pub struct Saver<T: Saved + Send + 'static> {
    jobs: Option<mpsc::Sender<Job<T>>>,
    thread: Option<JoinHandle<()>>,
    /// The last write that failed on the thread, for the engine to tell once.
    failed: Arc<Mutex<Option<String>>>,
}

impl<T: Saved + Send + 'static> Saver<T> {
    /// Starts the thread; values given to [`Saver::later`] wait `quiet` without a newer one
    /// before they are written. `None` when the thread could not start.
    pub fn start(store: Store, quiet: Duration) -> Option<Self> {
        let (jobs, rx) = mpsc::channel::<Job<T>>();
        let failed = Arc::new(Mutex::new(None));
        let told = Arc::clone(&failed);
        let thread = std::thread::Builder::new()
            .name("mosaik-saver".into())
            .spawn(move || run(&store, &rx, quiet, &told))
            .ok()?;
        Some(Self {
            jobs: Some(jobs),
            thread: Some(thread),
            failed,
        })
    }

    /// Writes `value` once nothing newer comes for a moment. Whether the thread took it (it has
    /// stopped otherwise: the caller writes it itself).
    pub fn later(&self, value: T) -> bool {
        self.jobs
            .as_ref()
            .is_some_and(|j| j.send(Job::Later(Box::new(value))).is_ok())
    }

    /// Writes `value` now, after nothing older, and waits at most `wait` to say how it went.
    pub fn now(&self, value: T, wait: Duration) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        let sent = self
            .jobs
            .as_ref()
            .is_some_and(|j| j.send(Job::Now(Box::new(value), tx)).is_ok());
        if !sent {
            return Err("the thread that writes files has stopped".into());
        }
        rx.recv_timeout(wait)
            .unwrap_or_else(|_| Err("the disk took too long to answer".into()))
    }

    /// Writes the value waiting, if any, and waits at most `wait` for it. Whether it was done.
    pub fn flush(&self, wait: Duration) -> bool {
        let (tx, rx) = mpsc::channel();
        self.jobs.as_ref().is_some_and(|j| j.send(Job::Flush(tx)).is_ok()) && rx.recv_timeout(wait).is_ok()
    }

    /// Why the last write made on the thread failed, once.
    pub fn take_failure(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

impl<T: Saved + Send + 'static> Drop for Saver<T> {
    /// The value waiting is written before the thread ends.
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run<T: Saved>(
    store: &Store,
    rx: &mpsc::Receiver<Job<T>>,
    quiet: Duration,
    failed: &Mutex<Option<String>>,
) {
    let write = |value: Option<Box<T>>| {
        if let Some(v) = value
            && let Err(e) = store.save(&*v)
        {
            *failed.lock().unwrap_or_else(PoisonError::into_inner) = Some(e.to_string());
        }
    };
    let mut pending: Option<(Box<T>, Instant)> = None;
    loop {
        let job = match &pending {
            Some((_, since)) => {
                let wait = quiet.min(MOST_WAIT.saturating_sub(since.elapsed()));
                match rx.recv_timeout(wait) {
                    Ok(job) => Some(job),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => {
                        write(pending.take().map(|(v, _)| v));
                        return;
                    }
                }
            }
            None => match rx.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            },
        };
        match job {
            // Quiet for a moment, or waiting since long enough: the latest is written.
            None => write(pending.take().map(|(v, _)| v)),
            Some(Job::Later(v)) => {
                let since = pending.as_ref().map_or_else(Instant::now, |(_, s)| *s);
                pending = Some((v, since));
                if since.elapsed() >= MOST_WAIT {
                    write(pending.take().map(|(v, _)| v));
                }
            }
            Some(Job::Now(v, reply)) => {
                pending = None;
                let _ = reply.send(store.save(&*v).map_err(|e| e.to_string()));
            }
            Some(Job::Flush(done)) => {
                write(pending.take().map(|(v, _)| v));
                let _ = done.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests may unwrap: a panic is a failed test.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::store::WorkspacesFile;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("mosaik-saver-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn read(&self) -> Option<String> {
            let file: Option<WorkspacesFile> = fs::read(self.0.join("workspaces.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            file.and_then(|f| f.active).map(|a| a.0)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A workspaces file told apart by its open workspace's id.
    fn file(active: &str) -> WorkspacesFile {
        WorkspacesFile {
            active: Some(crate::layout::WorkspaceId(active.into())),
            ..WorkspacesFile::default()
        }
    }

    const QUIET: Duration = Duration::from_millis(80);

    #[test]
    fn a_change_is_written_once_changes_stop_the_latest_only() {
        let d = TempDir::new("later");
        let saver = Saver::start(Store::new(&d.0), QUIET).unwrap();
        assert!(saver.later(file("a")));
        assert!(saver.later(file("b")));
        // Not on the caller's time: nothing written yet.
        assert_eq!(d.read(), None);
        std::thread::sleep(QUIET * 4);
        assert_eq!(d.read().as_deref(), Some("b"));
        assert_eq!(saver.take_failure(), None);
    }

    #[test]
    fn a_write_waited_on_goes_at_once_and_nothing_older_follows_it() {
        let d = TempDir::new("now");
        let saver = Saver::start(Store::new(&d.0), QUIET).unwrap();
        assert!(saver.later(file("older")));
        assert_eq!(saver.now(file("newer"), Duration::from_secs(5)), Ok(()));
        assert_eq!(d.read().as_deref(), Some("newer"));
        std::thread::sleep(QUIET * 4);
        assert_eq!(d.read().as_deref(), Some("newer"));
    }

    #[test]
    fn flushing_or_stopping_writes_what_waits() {
        let d = TempDir::new("flush");
        let saver = Saver::start(Store::new(&d.0), Duration::from_secs(60)).unwrap();
        assert!(saver.later(file("a")));
        assert!(saver.flush(Duration::from_secs(5)));
        assert_eq!(d.read().as_deref(), Some("a"));
        assert!(saver.later(file("b")));
        drop(saver);
        assert_eq!(d.read().as_deref(), Some("b"));
    }

    #[test]
    fn changes_that_keep_coming_are_still_written_every_little_while() {
        let d = TempDir::new("steady");
        let saver = Saver::start(Store::new(&d.0), Duration::from_millis(500)).unwrap();
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < MOST_WAIT + Duration::from_millis(800) {
            n += 1;
            assert!(saver.later(file(&format!("v{n}"))));
            std::thread::sleep(Duration::from_millis(100));
        }
        // Never quiet for 500 ms, yet written within MOST_WAIT of the first change.
        assert!(d.read().is_some());
    }

    #[test]
    fn a_write_that_fails_is_told_once() {
        let d = TempDir::new("fails");
        // The folder to write in is a file: no write can succeed.
        let blocked = d.0.join("not-a-folder");
        fs::write(&blocked, b"x").unwrap();
        let saver = Saver::start(Store::new(&blocked), QUIET).unwrap();
        assert!(saver.later(file("a")));
        assert!(saver.flush(Duration::from_secs(5)));
        assert!(saver.take_failure().is_some());
        assert_eq!(saver.take_failure(), None);
        assert!(saver.now(file("b"), Duration::from_secs(5)).is_err());
    }
}
