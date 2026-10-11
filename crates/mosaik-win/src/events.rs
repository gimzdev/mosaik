//! WinEvent hooks and the message pump they need.
//!
//! Hooks are out-of-context: Windows queues each event to the thread that installed the hook,
//! which must pump messages. The callback only records the event; the thread's loop drains the
//! queue afterwards, because the callback can be re-entered and must not do real work.

use std::cell::RefCell;
use std::collections::VecDeque;

use mosaik_core::window::WindowHandle;
use windows::Win32::Foundation::{HWND, WAIT_TIMEOUT};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_QUIT,
};

use crate::error::{Error, Result};
use crate::time;
use crate::window::handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinEvent {
    pub event: u32,
    pub window: WindowHandle,
    pub object: i32,
    pub child: i32,
    /// When Windows generated it (GetTickCount milliseconds).
    pub time_ms: u32,
    /// When the hook received it (performance counter ticks).
    pub received: i64,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<WinEvent>> = const { RefCell::new(VecDeque::new()) };
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    time_ms: u32,
) {
    let e = WinEvent {
        event,
        window: handle(hwnd),
        object,
        child,
        time_ms,
        received: time::now(),
    };
    QUEUE.with(|q| {
        if let Ok(mut q) = q.try_borrow_mut() {
            q.push_back(e);
        }
    });
}

/// Installed hooks; removed when dropped. Lives on the thread that installed them.
pub struct Hooks(Vec<HWINEVENTHOOK>);

impl Drop for Hooks {
    fn drop(&mut self) {
        for h in self.0.drain(..) {
            // SAFETY: each handle came from SetWinEventHook on this thread and is unhooked once.
            let _ = unsafe { UnhookWinEvent(h) };
        }
    }
}

/// Hooks each `(first, last)` event range, out of context, skipping Mosaïk's own windows.
/// `process` limits a hook to one process (0 for all).
pub fn hook(ranges: &[(u32, u32)], process: u32) -> Result<Hooks> {
    let mut hooks = Hooks(Vec::new());
    for &(first, last) in ranges {
        // SAFETY: out-of-context hook with a callback that only records the event.
        let h = unsafe {
            SetWinEventHook(
                first,
                last,
                None,
                Some(on_event),
                process,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if h.is_invalid() {
            return Err(Error::Other(format!(
                "SetWinEventHook({first:#x}..{last:#x}) failed"
            )));
        }
        hooks.0.push(h);
    }
    Ok(hooks)
}

/// Takes every event recorded so far on this thread.
pub fn drain() -> Vec<WinEvent> {
    QUEUE.with(|q| {
        q.try_borrow_mut()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default()
    })
}

/// Dispatches waiting messages. Returns false when WM_QUIT arrives.
pub fn pump() -> bool {
    let mut msg = MSG::default();
    // SAFETY: standard message loop on the calling thread.
    unsafe {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

/// Pumps messages until `done` returns true or `timeout_ms` passes. Returns whether `done`
/// became true. Sleeps between messages, waking at least every 10 ms to check `done` (which
/// may watch something that sends no message, like another process exiting).
pub fn pump_until(timeout_ms: u32, mut done: impl FnMut() -> bool) -> bool {
    let start = time::now();
    loop {
        if !pump() {
            return false;
        }
        if done() {
            return true;
        }
        let elapsed_ms = time::micros(start, time::now()) / 1000.0;
        if elapsed_ms >= f64::from(timeout_ms) {
            return false;
        }
        // Whole milliseconds left, at least 1.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let left = (f64::from(timeout_ms) - elapsed_ms).ceil().max(1.0) as u32;
        // SAFETY: waits for input on this thread's queue or the timeout.
        let r = unsafe { MsgWaitForMultipleObjects(None, false, left.min(10), QS_ALLINPUT) };
        if r == WAIT_TIMEOUT && done() {
            return true;
        }
    }
}
