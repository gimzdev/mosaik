//! Mosaïk's engine: one thread that owns everything Mosaïk does to other apps' windows.
//!
//! The engine thread runs a Win32 message loop: global hotkeys, window events and timers arrive
//! there, and every change to a window is made from there, in order. Everything else talks to it
//! through [`Engine::request`] with the commands of `mosaik_ipc::protocol`: the interface in the
//! same process, and `mosaikctl` through the named pipe ([`server`]). What the interface must
//! show (the switcher, a notice) comes back as [`UiEvent`]s.
//!
//! Windows only; an empty library elsewhere so the workspace builds everywhere.
#![cfg(windows)]

mod hotkeys;
mod mover;
pub mod server;
mod state;
mod switcher;
mod tabs;
mod thread;
mod watch;
pub mod watchdog;
mod workspace;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use mosaik_ipc::protocol::{Command, ErrorCode, Reply};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Where Mosaïk keeps its files, and how it was started.
#[derive(Clone, Debug)]
pub struct Config {
    /// Settings, workspaces and the hidden-windows journal (`%APPDATA%\Mosaik`).
    pub data_dir: PathBuf,
    /// Started with `--safe-mode`: nothing is moved or hidden, hotkeys are not registered.
    pub safe_mode: bool,
    pub app_version: String,
    /// Process ids that are Mosaïk's own (its WebView2 processes, the watchdog): never managed.
    pub own_pids: Vec<u32>,
    /// The watchdog is running. Without it Mosaïk hides nothing (ADR 0006).
    pub watchdog: bool,
    /// For probes and tests: manage only the windows of these programs (file names such as
    /// `mosaik-probe.exe`), so a test run never touches the user's own windows, and apps it
    /// starts are recognised too. `None` in the app.
    pub manage_only: Option<Vec<String>>,
}

/// Something the interface has to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
    ShowSettings,
    /// Tell the user something once, plainly (a shortcut that could not be registered...).
    Notice {
        title: String,
        body: String,
    },
    /// The engine stopped on request (`mosaikctl` or the tray): the app should exit.
    Exit,
}

/// Receives [`UiEvent`]s. Called on the engine thread: it must hand the work over to the
/// interface's own thread and return at once.
pub type UiSink = Arc<dyn Fn(UiEvent) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("the engine could not start: {0}")]
    Start(String),
}

enum Message {
    Request {
        command: Command,
        reply: mpsc::Sender<Reply>,
    },
    Stop,
}

/// Posted to the engine's window when its inbox has messages.
pub(crate) const WM_WAKE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// A handle to the running engine. Cheap to share (`Arc<Engine>`) between the interface and the
/// pipe server.
pub struct Engine {
    inbox: mpsc::Sender<Message>,
    /// The engine's message-only window, as an integer: `HWND` is not `Send`, and the window is
    /// only ever posted to from here.
    window: isize,
    /// Signalled when the engine thread has finished.
    done: std::sync::Mutex<Option<mpsc::Receiver<()>>>,
    thread: std::sync::Mutex<Option<JoinHandle<()>>>,
}

/// How long [`Engine::stop`] waits for windows to come back before giving up on the engine
/// thread (the watchdog restores whatever is left once the process ends).
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

impl Engine {
    /// Starts the engine thread and waits until it is ready (hotkeys registered, first scan
    /// done) or has failed.
    pub fn start(config: Config, ui: UiSink) -> Result<Self, EngineError> {
        let (inbox, messages) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("mosaik-engine".into())
            .spawn(move || thread::run(config, messages, ui, ready_tx, done_tx))
            .map_err(|e| EngineError::Start(e.to_string()))?;
        let window = match ready.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(window)) => window,
            Ok(Err(e)) => return Err(EngineError::Start(e)),
            Err(_) => return Err(EngineError::Start("the engine did not start within 15 s".into())),
        };
        Ok(Self {
            inbox,
            window,
            done: std::sync::Mutex::new(Some(done)),
            thread: std::sync::Mutex::new(Some(handle)),
        })
    }

    fn wake(&self) {
        // SAFETY: posts a message to the engine's own window; harmless if it has just closed.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(self.window as *mut std::ffi::c_void)),
                WM_WAKE,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }

    /// Runs one command on the engine thread and waits for its reply.
    pub fn request(&self, command: Command, timeout: Duration) -> Reply {
        let (reply, answer) = mpsc::channel();
        if self.inbox.send(Message::Request { command, reply }).is_err() {
            return stopped();
        }
        self.wake();
        match answer.recv_timeout(timeout) {
            Ok(reply) => reply,
            Err(mpsc::RecvTimeoutError::Disconnected) => stopped(),
            Err(mpsc::RecvTimeoutError::Timeout) => Reply::Error {
                code: ErrorCode::Busy,
                message: format!("Mosaïk's engine did not answer within {} s.", timeout.as_secs()),
                hint: Some(
                    "If windows are missing, run `mosaikctl rescue`: it restores them without the engine."
                        .into(),
                ),
            },
        }
    }

    /// Stops the engine: every hidden window is brought back, hotkeys are released. Waits up to
    /// a few seconds: an app that hangs while its window is restored must not keep Mosaïk from
    /// exiting (the watchdog finishes the job once the process is gone).
    pub fn stop(&self) {
        if self.inbox.send(Message::Stop).is_ok() {
            self.wake();
        }
        let done = self.done.lock().ok().and_then(|mut d| d.take());
        let finished = done.is_some_and(|d| d.recv_timeout(STOP_TIMEOUT).is_ok());
        let handle = self.thread.lock().ok().and_then(|mut t| t.take());
        match (finished, handle) {
            (true, Some(h)) => {
                let _ = h.join();
            }
            (false, _) => {
                tracing::error!("the engine did not stop within 3 s; the watchdog restores what is left");
            }
            (true, None) => {}
        }
    }
}

pub(crate) fn stopped() -> Reply {
    Reply::Error {
        code: ErrorCode::Failed,
        message: "Mosaïk's engine has stopped.".into(),
        hint: Some("Restart Mosaïk.".into()),
    }
}
