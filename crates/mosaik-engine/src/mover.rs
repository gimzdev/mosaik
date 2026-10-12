//! Moving other apps' windows without ever blocking the engine thread.
//!
//! Moving a window is a synchronous call into the app that owns it: an app that is busy or hung
//! would hold the caller until it answers. Each window is therefore moved on a short-lived thread
//! of its own (so windows also move in parallel), while the engine thread keeps handling its
//! messages, and waits at most a deadline: a window that has not answered by then is reported
//! as not responding, and its thread finishes whenever the app does.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use mosaik_core::geometry::Rect;
use mosaik_core::window::WindowHandle;
use mosaik_win::{place, window};
use windows::Win32::Foundation::WAIT_TIMEOUT;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostQuitMessage, QS_ALLINPUT,
    TranslateMessage, WM_QUIT,
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

/// Moves every window at once and waits until each has answered or `wait` has passed, handling
/// this thread's messages meanwhile. Results come back in no particular order.
pub(crate) fn move_all(moves: &[(WindowHandle, To)], wait: Duration) -> Vec<Outcome> {
    let (tx, rx) = mpsc::channel::<Outcome>();
    for &(window, to) in moves {
        let worker_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("mosaik-move".into())
            .spawn(move || {
                let hwnd = window::hwnd(window);
                // A game or a video in full screen is never moved: it would leave full screen.
                if window::geometry(hwnd).is_some_and(|g| g.full_screen) {
                    let _ = worker_tx.send(Outcome {
                        window,
                        result: Err("it is in full screen, so Mosaïk leaves it there".into()),
                    });
                    return;
                }
                let result = match to {
                    To::Frame(r) => place::move_frame(hwnd, r)
                        .map(|m| m.wanted)
                        .map_err(|e| e.to_string()),
                    To::Maximized(area) => place::maximize_on(hwnd, area)
                        .map(|()| area)
                        .map_err(|e| e.to_string()),
                };
                let _ = worker_tx.send(Outcome { window, result });
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
        pump_for(5);
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

/// Handles this thread's messages for up to `ms` milliseconds. Messages for the engine's own
/// window only record what arrived (it is busy), and are acted on when it is done. A request to
/// end the loop is put back for the main loop.
pub(crate) fn pump_for(ms: u32) {
    // SAFETY: waits for input on this thread's queue or the timeout.
    let r = unsafe { MsgWaitForMultipleObjects(None, false, ms, QS_ALLINPUT) };
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
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
