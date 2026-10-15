//! Toasts: what Mosaïk has to say while it works, shown at once by a corner of the screen the
//! pointer is on (`mosaik_overlay::toast`), one at a time, and kept with the window's notes. A
//! toast never opens Mosaïk's window over what the user is doing; a click on one does.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use mosaik_overlay::toast::{self, FADE_IN_MS, FADE_OUT_MS, Toast};
use mosaik_win::input;

use crate::UiEvent;
use crate::thread::{Engine, timer};

/// A frame of a fade, in milliseconds.
const FRAME_MS: u32 = 16;

/// How long a toast stays, by how much it says: long enough to read, never long.
pub(crate) fn hold_ms(title: &str, body: &str) -> u32 {
    let chars = u32::try_from(title.chars().count() + body.chars().count()).unwrap_or(u32::MAX);
    (4_000 + chars.saturating_sub(60).saturating_mul(45)).min(9_000)
}

#[derive(Default)]
pub(crate) struct Toasts {
    /// Waiting to be shown, oldest first. Added to from `&self` (most notices are).
    queue: RefCell<VecDeque<(String, String)>>,
    overlay: Option<Toast>,
    /// The toast's window could not be made: said in the log once; notes still keep everything.
    failed: bool,
    /// The one showing: since when, and for how long once in.
    showing: Option<(Instant, u32)>,
}

impl Engine {
    /// Shows `title` and `body` in a toast, after the ones waiting.
    pub(crate) fn queue_toast(&self, title: String, body: String) {
        self.toasts.queue.borrow_mut().push_back((title, body));
        if self.toasts.showing.is_none() {
            self.set_timer(timer::TOAST, 1);
        }
    }

    /// The next frame of the toast: in, held, out, then the next one.
    pub(crate) fn toast_tick(&mut self) {
        self.kill_timer(timer::TOAST);
        let animate = mosaik_win::os::animations();
        let Some((since, hold)) = self.toasts.showing else {
            self.next_toast();
            return;
        };
        let t = u32::try_from(since.elapsed().as_millis()).unwrap_or(u32::MAX);
        let fade_in = if animate { FADE_IN_MS } else { 0 };
        let fade_out = if animate { FADE_OUT_MS } else { 0 };
        let alpha = |done: f32| {
            // A byte of opacity.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let a = (done.clamp(0.0, 1.0) * 255.0).round() as u8;
            a
        };
        #[allow(clippy::cast_precision_loss)]
        let ratio = |a: u32, b: u32| a as f32 / b.max(1) as f32;
        if t < fade_in {
            self.fade_toast(alpha(mosaik_overlay::edge::ease(ratio(t, fade_in))));
            self.set_timer(timer::TOAST, FRAME_MS);
        } else if t < fade_in + hold {
            self.fade_toast(255);
            self.set_timer(timer::TOAST, (fade_in + hold - t).max(1));
        } else if t < fade_in + hold + fade_out {
            self.fade_toast(alpha(1.0 - ratio(t - fade_in - hold, fade_out)));
            self.set_timer(timer::TOAST, FRAME_MS);
        } else {
            self.end_toast();
        }
    }

    fn fade_toast(&mut self, alpha: u8) {
        if let Some(t) = &mut self.toasts.overlay
            && let Err(e) = t.fade(alpha)
        {
            tracing::debug!(error = %e, "toast not faded");
        }
    }

    fn end_toast(&mut self) {
        if let Some(t) = &mut self.toasts.overlay {
            t.hide();
        }
        self.toasts.showing = None;
        if !self.toasts.queue.borrow().is_empty() {
            self.set_timer(timer::TOAST, 250);
        }
    }

    fn next_toast(&mut self) {
        let Some((title, body)) = self.toasts.queue.borrow_mut().pop_front() else {
            return;
        };
        if self.toasts.failed {
            return;
        }
        if self.toasts.overlay.is_none() {
            match Toast::new() {
                Ok(t) => {
                    toast::wake_on_click(self.window, crate::thread::WM_TOAST);
                    self.toasts.overlay = Some(t);
                }
                Err(e) => {
                    self.toasts.failed = true;
                    tracing::error!(error = %e, "toasts cannot show; notes keep what they would say");
                    return;
                }
            }
        }
        // By the corner of the screen the pointer is on, as Windows' own notifications are by
        // the clock of the screen being used.
        let Some(topology) = self.read_topology() else {
            return;
        };
        let pointer = input::cursor();
        let monitor = topology
            .monitors()
            .iter()
            .find(|m| m.bounds.contains(pointer))
            .unwrap_or_else(|| topology.primary())
            .clone();
        let palette = self.quick_palette();
        if self.canvas_ready().is_none() {
            return;
        }
        let Some(mut canvas) = self.watch.take_canvas() else {
            return;
        };
        let start = if mosaik_win::os::animations() { 1 } else { 255 };
        let screen = toast::Screen {
            work: monitor.work_area,
            dpi: monitor.dpi,
        };
        let shown = self
            .toasts
            .overlay
            .as_mut()
            .map(|t| t.show(&mut canvas, screen, (&title, &body), palette, start));
        self.watch.put_canvas(canvas);
        match shown {
            Some(Ok(())) => {
                self.toasts.showing = Some((Instant::now(), hold_ms(&title, &body)));
                self.set_timer(timer::TOAST, FRAME_MS);
            }
            Some(Err(e)) => tracing::debug!(error = %e, "toast not drawn"),
            None => {}
        }
    }

    /// The toast was clicked: it goes, and Mosaïk's window opens, where the message is kept.
    pub(crate) fn toast_clicked(&mut self) {
        if !toast::take_clicked() {
            return;
        }
        self.kill_timer(timer::TOAST);
        if let Some(t) = &mut self.toasts.overlay {
            t.hide();
        }
        self.toasts.showing = None;
        self.toasts.queue.borrow_mut().clear();
        (self.ui)(UiEvent::ShowSettings);
    }
}

#[cfg(test)]
mod tests {
    use super::hold_ms;

    #[test]
    fn a_toast_stays_long_enough_to_read_and_never_long() {
        assert_eq!(hold_ms("Mosaïk", "Saved."), 4_000);
        let long = "word ".repeat(40);
        assert!(hold_ms("A title", &long) > 4_000);
        assert_eq!(hold_ms("A title", &"x".repeat(5_000)), 9_000);
    }
}
