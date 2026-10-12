//! Hiding and showing background tabs without ever blocking the engine thread.
//!
//! The shell cloak is a call into Explorer, and minimising waits for the app that owns the
//! window: either can hang. They run on a thread of their own, which owns the shell interface
//! (on a single-threaded COM apartment, as measured in ADR 0003). The engine hands it a batch
//! and waits at most a deadline, handling its own messages meanwhile, as it does for moves
//! (`mover`). The thread works through requests in order, so an answer that comes late never
//! undoes a newer request; late answers are picked up by the next call.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use mosaik_core::journal::HideMethod;
use mosaik_core::window::WindowHandle;
use mosaik_win::error::Error;
use mosaik_win::hide::{Hidden, Hider};
use mosaik_win::window;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage,
    WM_APP, WM_QUIT, WM_USER,
};

/// How long the engine waits for a batch.
pub(crate) const WAIT: Duration = Duration::from_millis(1500);

/// Posted to the worker when requests are waiting.
const WM_JOBS: u32 = WM_APP + 7;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Request {
    /// Hide it this way, or by minimising when that fails.
    Hide(HideMethod),
    /// Bring it back where it is.
    Show(Hidden),
    /// Bring it back minimised: in the taskbar, not on screen.
    ShowMinimized(Hidden),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Hide,
    Show,
}

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    /// The window has closed: nothing to hide or bring back.
    pub gone: bool,
    pub message: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Answer {
    pub window: WindowHandle,
    pub kind: Kind,
    /// For a hide: how the window was hidden.
    pub result: Result<Option<Hidden>, Failure>,
}

fn failure(e: &Error) -> Failure {
    Failure {
        gone: matches!(e, Error::WindowGone),
        message: e.to_string(),
    }
}

fn run_one(hider: &mut Hider, window: WindowHandle, request: Request) -> Answer {
    let hwnd = window::hwnd(window);
    match request {
        Request::Hide(method) => {
            let mut result = hider.hide(hwnd, method);
            if let Err(e) = &result
                && !matches!(e, Error::WindowGone)
                && method != HideMethod::Minimize
            {
                tracing::warn!(error = %e, ?method, "window not hidden; minimising it instead");
                result = hider.hide(hwnd, HideMethod::Minimize);
            }
            Answer {
                window,
                kind: Kind::Hide,
                result: result.map(Some).map_err(|e| failure(&e)),
            }
        }
        Request::Show(hidden) => Answer {
            window,
            kind: Kind::Show,
            result: hider.show(hwnd, &hidden).map(|()| None).map_err(|e| failure(&e)),
        },
        Request::ShowMinimized(hidden) => Answer {
            window,
            kind: Kind::Show,
            result: hider
                .show_minimized(hwnd, &hidden)
                .map(|()| None)
                .map_err(|e| failure(&e)),
        },
    }
}

/// The thread that hides and shows windows.
pub(crate) struct Worker {
    jobs: mpsc::Sender<(WindowHandle, Request)>,
    answers: mpsc::Receiver<Answer>,
    thread_id: u32,
    /// Requests sent and not answered yet.
    open: usize,
}

impl Worker {
    pub(crate) fn start() -> Result<Self, String> {
        let (jobs, job_rx) = mpsc::channel::<(WindowHandle, Request)>();
        let (answer_tx, answers) = mpsc::channel::<Answer>();
        let (ready_tx, ready) = mpsc::channel::<u32>();
        std::thread::Builder::new()
            .name("mosaik-hide".into())
            .spawn(move || {
                // SAFETY: COM for this thread's whole life; the shell interface lives here.
                let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let mut msg = MSG::default();
                // SAFETY: creates this thread's message queue before its id is handed out, so
                // nothing posted to it is lost.
                let _ = unsafe { PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE) };
                // SAFETY: no preconditions.
                let _ = ready_tx.send(unsafe { GetCurrentThreadId() });
                let mut hider = Hider::new();
                loop {
                    // SAFETY: this thread's message loop (COM uses it too).
                    let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if got.0 <= 0 {
                        break;
                    }
                    if msg.message == WM_JOBS && msg.hwnd.is_invalid() {
                        while let Ok((window, request)) = job_rx.try_recv() {
                            if answer_tx.send(run_one(&mut hider, window, request)).is_err() {
                                return;
                            }
                        }
                        continue;
                    }
                    // SAFETY: standard dispatch of this thread's messages.
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            })
            .map_err(|e| format!("its thread could not start: {e}"))?;
        let thread_id = ready
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "its thread did not start".to_owned())?;
        Ok(Self {
            jobs,
            answers,
            thread_id,
            open: 0,
        })
    }

    /// Whether requests are still running (a hide that may not have happened yet).
    pub(crate) fn busy(&self) -> bool {
        self.open > 0
    }

    /// Sends `batch`, then waits until every request sent so far is answered or `wait` has
    /// passed, handling this thread's messages meanwhile. Returns every answer that arrived,
    /// late ones to earlier batches included, in the order the requests ran.
    pub(crate) fn run(&mut self, batch: Vec<(WindowHandle, Request)>, wait: Duration) -> Vec<Answer> {
        let mut answers = Vec::new();
        let mut sent = false;
        for (window, request) in batch {
            if self.jobs.send((window, request)).is_ok() {
                self.open += 1;
                sent = true;
            } else {
                answers.push(Answer {
                    window,
                    kind: match request {
                        Request::Hide(_) => Kind::Hide,
                        Request::Show(_) | Request::ShowMinimized(_) => Kind::Show,
                    },
                    result: Err(Failure {
                        gone: false,
                        message: "Mosaïk's thread for hiding windows has stopped".into(),
                    }),
                });
            }
        }
        // SAFETY: posts to the worker's queue, which exists (made before its id was sent).
        if sent && unsafe { PostThreadMessageW(self.thread_id, WM_JOBS, WPARAM(0), LPARAM(0)) }.is_err() {
            tracing::warn!("the hiding thread was not woken");
        }
        let deadline = Instant::now() + wait;
        loop {
            while let Ok(a) = self.answers.try_recv() {
                self.open = self.open.saturating_sub(1);
                answers.push(a);
            }
            if self.open == 0 || Instant::now() >= deadline {
                break;
            }
            crate::mover::pump_for(5);
        }
        answers
    }

    /// Answers that arrived since the last call, without waiting.
    pub(crate) fn late(&mut self) -> Vec<Answer> {
        let mut answers = Vec::new();
        while let Ok(a) = self.answers.try_recv() {
            self.open = self.open.saturating_sub(1);
            answers.push(a);
        }
        answers
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // SAFETY: asks the worker thread to end its loop; harmless if it has ended already.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}
