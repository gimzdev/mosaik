//! Moving other apps' windows without ever blocking the engine thread.
//!
//! Moving a window is a synchronous call into the app that owns it: an app that is busy or hung
//! would hold the caller until it answers. Each window is therefore moved on a short-lived thread
//! of its own (so windows also move in parallel), while the engine thread keeps handling its
//! messages, and waits at most a deadline: a window that has not answered by then is reported
//! as not responding, and its thread finishes whenever the app does.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use mosaik_core::geometry::{Point, Rect};
use mosaik_core::window::WindowHandle;
use mosaik_win::{arrange, place, window};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{CreateEventW, SetEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW,
    PostQuitMessage, QS_ALLINPUT, TranslateMessage, WM_QUIT,
};

/// Where a window goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum To {
    /// Its visible frame covers this rectangle.
    Frame(Rect),
    /// Maximised on the monitor with this work area.
    Maximized(Rect),
}

/// How a move went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub window: WindowHandle,
    /// Where the frame was meant to land (the target, or the app's own size fitted on screen).
    pub result: Result<Rect, String>,
}

/// How long a switch waits for the apps it moves.
pub(crate) const WAIT: Duration = Duration::from_millis(2500);
/// How long one window's move (a tab shown, a window dropped) is waited for.
pub(crate) const WAIT_ONE: Duration = Duration::from_millis(1500);

/// Wakes a thread waiting for workers (`pump_for`) as soon as one of them has answered. Without
/// it the wait looked again every 5 ms, which Windows rounds up to its timer's tick (15.6 ms
/// unless a program asks for finer): each wait for a move, a tab shown or hidden, ended up to
/// 16 ms after the answer came, several times per switch. An auto-reset event, shared with the
/// workers, closed once the last of them lets go of it.
pub(crate) struct Wake(HANDLE);

// SAFETY: an event can be set and waited on from any thread.
unsafe impl Send for Wake {}
// SAFETY: as above; the handle is only read.
unsafe impl Sync for Wake {}

impl Wake {
    /// `None` when Windows could not make the event: the waits then look every 5 ms, as before.
    pub(crate) fn new() -> Option<Arc<Self>> {
        // SAFETY: an unnamed auto-reset event, not set, closed when the value is dropped.
        unsafe { CreateEventW(None, false, false, None) }
            .ok()
            .map(|h| Arc::new(Self(h)))
    }

    /// An answer was sent: the waiting thread wakes.
    pub(crate) fn set(&self) {
        // SAFETY: the event is open as long as this value lives.
        let _ = unsafe { SetEvent(self.0) };
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        // SAFETY: our own event handle, closed once, by the last owner.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// How long one wait for workers may last: until the deadline when a worker wakes it, else a
/// short look at a time.
pub(crate) fn slice(deadline: Instant, wake: Option<&Wake>) -> u32 {
    if wake.is_none() {
        return 5;
    }
    let left = deadline.saturating_duration_since(Instant::now()).as_millis();
    u32::try_from(left).unwrap_or(u32::MAX).max(1)
}

/// Moves every window at once and waits until each has answered or `wait` has passed, handling
/// this thread's messages meanwhile. Results come back in no particular order.
pub(crate) fn move_all(moves: &[(WindowHandle, To)], wait: Duration) -> Vec<Outcome> {
    let (tx, rx) = mpsc::channel::<Outcome>();
    let wake = Wake::new();
    for &(window, to) in moves {
        let worker_tx = tx.clone();
        let worker_wake = wake.clone();
        let spawned = std::thread::Builder::new()
            .name("mosaik-move".into())
            .spawn(move || {
                let hwnd = window::hwnd(window);
                // A game or a video in full screen is never moved: it would leave full screen.
                let result = if window::geometry(hwnd).is_some_and(|g| g.full_screen) {
                    Err("it is in full screen, so Mosaïk leaves it there".into())
                } else {
                    match to {
                        To::Frame(r) => place::move_frame(hwnd, r)
                            .map(|m| m.wanted)
                            .map_err(|e| e.to_string()),
                        To::Maximized(area) => place::maximize_on(hwnd, area)
                            .map(|()| area)
                            .map_err(|e| e.to_string()),
                    }
                };
                let _ = worker_tx.send(Outcome { window, result });
                if let Some(w) = worker_wake {
                    w.set();
                }
            });
        if let Err(e) = spawned {
            let _ = tx.send(Outcome {
                window,
                result: Err(format!("could not start moving it: {e}")),
            });
        }
    }
    drop(tx);
    let deadline = Instant::now() + wait;
    let mut done: Vec<Outcome> = Vec::with_capacity(moves.len());
    while done.len() < moves.len() {
        while let Ok(o) = rx.try_recv() {
            done.push(o);
        }
        if done.len() == moves.len() || Instant::now() >= deadline {
            break;
        }
        pump_for(slice(deadline, wake.as_deref()), wake.as_deref());
    }
    for &(window, _) in moves {
        if !done.iter().any(|o| o.window == window) {
            done.push(Outcome {
                window,
                result: Err("the app is not responding".into()),
            });
        }
    }
    done
}

/// A window [`stack`] moved down, and the window that was just above it before: Undo puts it
/// back under that one.
pub(crate) type Lowered = (WindowHandle, Option<WindowHandle>);

/// Runs `job` on a thread of its own and waits at most `wait` for it, handling this thread's
/// messages meanwhile: calls into other apps that land in order, without holding up the engine.
/// `None` when it did not finish in time.
fn on_a_thread<T: Send + 'static>(wait: Duration, job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (finished, done) = mpsc::channel::<T>();
    let wake = Wake::new();
    let thread_wake = wake.clone();
    std::thread::Builder::new()
        .name("mosaik-stack".into())
        .spawn(move || {
            let _ = finished.send(job());
            if let Some(w) = thread_wake {
                w.set();
            }
        })
        .ok()?;
    let deadline = Instant::now() + wait;
    loop {
        match done.try_recv() {
            Ok(out) => return Some(out),
            Err(mpsc::TryRecvError::Disconnected) => return None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            return None;
        }
        pump_for(slice(deadline, wake.as_deref()), wake.as_deref());
    }
}

/// Stacks the windows of one zone in `order` (front to back), as little as it takes (ADR 0032):
/// a window in front of the one that should come before it (the one before it alike, always on
/// top or not) is put just below that one; the others stay where they are. The order the
/// windows were used in, which the next plan reads, changes no more than that, and nothing comes
/// up over Mosaïk's own window. Returns the windows moved down, each with the window that was
/// just above it; none when the apps did not answer within `wait`.
pub(crate) fn stack(order: &[WindowHandle], wait: Duration) -> Vec<Lowered> {
    if order.len() < 2 {
        return Vec::new();
    }
    let order = order.to_vec();
    on_a_thread(wait, move || {
        let mut z = window::top_level();
        let at = |z: &[HWND], h: HWND| z.iter().position(|x| *x == h);
        let (mut last, mut last_on_top): (Option<HWND>, Option<HWND>) = (None, None);
        let mut lowered = Vec::new();
        for h in order.iter().map(|h| window::hwnd(*h)) {
            let on_top = arrange::topmost(h);
            let before = if on_top { last_on_top } else { last };
            if let Some(b) = before
                && let (Some(i), Some(j)) = (at(&z, h), at(&z, b))
                && i < j
            {
                let above = i.checked_sub(1).and_then(|k| z.get(k)).copied();
                match arrange::put_below(h, b) {
                    Ok(true) => {
                        z.remove(i);
                        let j = at(&z, b).unwrap_or(z.len().saturating_sub(1));
                        z.insert(j + 1, h);
                        lowered.push((window::handle(h), above.map(window::handle)));
                    }
                    Ok(false) => tracing::debug!("window not stacked: one of the two is always on top"),
                    Err(e) => tracing::debug!(error = %e, "window not stacked"),
                }
            }
            if on_top {
                last_on_top = Some(h);
            } else {
                last = Some(h);
            }
        }
        lowered
    })
    .unwrap_or_default()
}

/// Puts the windows [`stack`] moved down back under the windows that were above them, the last
/// moved first (Undo), on a thread of its own, waiting at most `wait`.
pub(crate) fn unstack(lowered: &[Lowered], wait: Duration) {
    let lowered = lowered.to_vec();
    let done = on_a_thread(wait, move || {
        for (h, above) in lowered.iter().rev() {
            let Some(above) = above else { continue };
            let (h, above) = (window::hwnd(*h), window::hwnd(*above));
            if window::exists(h)
                && window::exists(above)
                && let Err(e) = arrange::put_below(h, above)
            {
                tracing::debug!(error = %e, "window not put back in its place");
            }
        }
    });
    if done.is_none() {
        tracing::debug!("windows not put back in their places in time");
    }
}

/// One window following the pointer (dragged by the band Mosaïk draws above it), moved on a
/// thread of its own: the engine never waits for the app, and an app slow to move gets only the
/// latest place. The moves are synchronous on that thread, so they land in order, and before a
/// move asked later from another thread (the drop into a zone).
pub(crate) struct Follower {
    places: Option<mpsc::Sender<Point>>,
    done: mpsc::Receiver<()>,
    wake: Option<Arc<Wake>>,
}

impl Follower {
    /// `None` when no thread could be started: the window then stays where it is until the drop.
    pub(crate) fn start(window: WindowHandle) -> Option<Self> {
        let (places, wanted) = mpsc::channel::<Point>();
        let (finished, done) = mpsc::channel::<()>();
        let wake = Wake::new();
        let thread_wake = wake.clone();
        std::thread::Builder::new()
            .name("mosaik-follow".into())
            .spawn(move || {
                let hwnd = window::hwnd(window);
                while let Ok(mut at) = wanted.recv() {
                    // Only the latest place matters.
                    while let Ok(later) = wanted.try_recv() {
                        at = later;
                    }
                    arrange::move_to(hwnd, at.x, at.y);
                }
                let _ = finished.send(());
                if let Some(w) = thread_wake {
                    w.set();
                }
            })
            .ok()?;
        Some(Self {
            places: Some(places),
            done,
            wake,
        })
    }

    /// The window's rectangle (invisible borders included) goes to start at `at`.
    pub(crate) fn to(&self, at: Point) {
        if let Some(places) = &self.places {
            let _ = places.send(at);
        }
    }

    /// No more places: waits at most `wait`, handling this thread's messages, for the app to
    /// take the last one. Returns whether it did.
    pub(crate) fn finish(mut self, wait: Duration) -> bool {
        self.places = None;
        let deadline = Instant::now() + wait;
        loop {
            match self.done.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return true,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return false;
            }
            pump_for(slice(deadline, self.wake.as_deref()), self.wake.as_deref());
        }
    }
}

/// Handles this thread's messages for up to `ms` milliseconds, or until `wake` is set (a worker
/// answered). Messages for the engine's own window only record what arrived (it is busy), and
/// are acted on when it is done. A request to end the loop is put back for the main loop.
pub(crate) fn pump_for(ms: u32, wake: Option<&Wake>) {
    let handles = wake.map(|w| [w.0]);
    // SAFETY: waits for input on this thread's queue (also input already there but looked at,
    // which a long wait must not sleep through), the event (open while `wake` lives), or the
    // timeout.
    let r = unsafe {
        MsgWaitForMultipleObjectsEx(
            handles.as_ref().map(|h| &h[..]),
            ms,
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE,
        )
    };
    if r == WAIT_TIMEOUT {
        return;
    }
    let mut msg = MSG::default();
    // SAFETY: the standard way to handle this thread's waiting messages.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                PostQuitMessage(i32::try_from(msg.wParam.0).unwrap_or(0));
                return;
            }
            crate::thread::forward_close(&msg);
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
