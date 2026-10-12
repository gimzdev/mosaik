//! A layered window that shows a premultiplied bitmap at a screen position, never takes focus
//! and (optionally) lets clicks through.

use mosaik_core::geometry::{Point, Rect};
use mosaik_win::error::{Context, Error, Result};
use mosaik_win::own::{WindowClass, destroy};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, GetDC, ReleaseDC};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DefWindowProcW, DeferWindowPos, EndDeferWindowPos, GW_HWNDPREV, GWL_EXSTYLE,
    GetWindow, GetWindowLongPtrW, HWND_TOP, HWND_TOPMOST, MA_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos,
    ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_CLOSE, WM_MOUSEACTIVATE, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::paint::Pixels;
use crate::surface::Surface;

/// Where an overlay sits in the stack of windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    /// Above every normal window.
    Topmost,
    /// Right below another window: the window stays on top of the overlay (the dragged window
    /// over the zones it can drop into).
    Below(isize),
    /// Right above another window and below whatever is above it: a frame around a window is
    /// covered by the windows that cover it, and is not darkened by the window's own shadow.
    Above(isize),
}

/// A layered window owned by Mosaïk.
pub struct LayeredWindow {
    hwnd: HWND,
    surface: Option<Surface>,
    shown: Option<Rect>,
}

/// Overlays only show things: they never take the focus, and they close with Mosaïk, not when
/// another program asks every window to close.
pub(crate) unsafe extern "system" fn overlay_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn blend(alpha: u8) -> BLENDFUNCTION {
    BLENDFUNCTION {
        BlendOp: u8::try_from(AC_SRC_OVER).unwrap_or(0),
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: u8::try_from(AC_SRC_ALPHA).unwrap_or(1),
    }
}

impl LayeredWindow {
    /// A top-most overlay, created hidden. Overlays that only show pass `click_through`.
    pub fn new(class_name: &str, click_through: bool) -> Result<Self> {
        Self::create(class_name, click_through, true)
    }

    /// Like [`LayeredWindow::new`]; `topmost` false for overlays stacked with other windows.
    pub fn create(class_name: &str, click_through: bool, topmost: bool) -> Result<Self> {
        let class = WindowClass::register(class_name, Some(overlay_proc))?;
        let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if topmost {
            ex |= WS_EX_TOPMOST;
        }
        if click_through {
            ex |= WS_EX_TRANSPARENT;
        }
        let hwnd = class.create("", WS_POPUP, ex, Rect::new(0, 0, 1, 1), None)?;
        Ok(Self {
            hwnd,
            surface: None,
            shown: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// The bitmap to draw into, `width` by `height` pixels (made anew when the size changes,
    /// otherwise reused with its old content).
    pub fn surface(&mut self, width: usize, height: usize) -> Result<&mut Surface> {
        let fits = self
            .surface
            .as_ref()
            .is_some_and(|s| s.width() == width.max(1) && s.height() == height.max(1));
        if !fits {
            self.surface = None;
            self.surface = Some(Surface::new(width, height)?);
        }
        self.surface
            .as_mut()
            .ok_or_else(|| Error::Other("no surface".into()))
    }

    /// Shows the surface at `at` (its top-left corner), with `alpha` applied on top of the
    /// bitmap's own transparency, in one call that moves, resizes and repaints the window.
    pub fn present(&mut self, at: Point, alpha: u8) -> Result<()> {
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| Error::Other("nothing drawn yet".into()))?;
        let pos = POINT { x: at.x, y: at.y };
        let size = SIZE {
            cx: i32::try_from(surface.width()).unwrap_or(1),
            cy: i32::try_from(surface.height()).unwrap_or(1),
        };
        let origin = POINT { x: 0, y: 0 };
        let b = blend(alpha);
        // SAFETY: the screen DC is released below; the memory DC holds a 32-bit premultiplied
        // DIB of exactly `size`.
        unsafe {
            let screen = GetDC(None);
            let r = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&pos),
                Some(&size),
                Some(surface.dc()),
                Some(&origin),
                COLORREF(0),
                Some(&b),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            r.ctx("UpdateLayeredWindow")?;
            if self.shown.is_none() {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
        self.shown = Some(Rect::from_origin_size(at.x, at.y, size.cx, size.cy));
        Ok(())
    }

    /// Changes only the overall opacity, keeping the bitmap Windows already has.
    pub fn set_alpha(&mut self, alpha: u8) -> Result<()> {
        if self.shown.is_none() {
            return Ok(());
        }
        let b = blend(alpha);
        // SAFETY: no source DC: Windows keeps the current bitmap and applies the new blend.
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                None,
                None,
                None,
                None,
                COLORREF(0),
                Some(&b),
                ULW_ALPHA,
            )
        }
        .ctx("UpdateLayeredWindow")
    }

    /// Shows `pixels` at `at` (which must have the bitmap's size).
    pub fn show(&mut self, at: Rect, pixels: &Pixels) -> Result<()> {
        self.surface(pixels.width, pixels.height)?.copy_from(pixels);
        self.present(at.origin(), 255)
    }

    /// Moves the overlay without redrawing it.
    pub fn move_to(&mut self, at: Point) {
        let Some(shown) = self.shown else { return };
        if shown.origin() == at {
            return;
        }
        // SAFETY: our own window; position only.
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                at.x,
                at.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
        self.shown = Some(Rect::from_origin_size(at.x, at.y, shown.width(), shown.height()));
    }

    pub fn hide(&mut self) {
        if self.shown.take().is_some() {
            // SAFETY: our own window.
            let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }

    pub fn shown_at(&self) -> Option<Rect> {
        self.shown
    }

    /// Frees the bitmap of a hidden overlay (large previews are only kept while dragging).
    pub fn release_surface(&mut self) {
        self.surface = None;
    }
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        self.surface = None;
        destroy(self.hwnd);
    }
}

/// Puts `overlays` (front to back) right below `stack`'s window, or at the top of the top-most
/// band, in one step so they never flash in the wrong order. Hidden overlays are skipped.
pub fn restack(overlays: &[&LayeredWindow], stack: Stack) {
    let shown: Vec<HWND> = overlays
        .iter()
        .filter(|o| o.shown.is_some())
        .map(|o| o.hwnd)
        .collect();
    restack_windows(&shown, stack);
}

fn topmost(h: HWND) -> bool {
    // SAFETY: reads a window's extended style; any handle is accepted.
    let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) };
    ex & isize::try_from(WS_EX_TOPMOST.0).unwrap_or(0) != 0
}

/// The window to insert after so that `ours` end up right above `target`: the first window above
/// it that is not one of ours, or the top of its band.
fn above(target: HWND, ours: &[HWND]) -> HWND {
    let mut prev = target;
    loop {
        // SAFETY: walks the z-order of top-level windows; a null result ends the walk.
        match unsafe { GetWindow(prev, GW_HWNDPREV) } {
            Ok(p) if !p.is_invalid() && ours.contains(&p) => prev = p,
            Ok(p) if !p.is_invalid() => {
                // A normal window at the top of its band has top-most windows above it: going
                // after one of those would make the overlay top-most.
                return if topmost(p) && !topmost(target) {
                    HWND_TOP
                } else {
                    p
                };
            }
            _ => return if topmost(target) { HWND_TOPMOST } else { HWND_TOP },
        }
    }
}

/// Stacks our windows (front to back) as `stack` says, in one step so they never flash in the
/// wrong order.
pub fn restack_windows(shown: &[HWND], stack: Stack) {
    if shown.is_empty() {
        return;
    }
    let first_after = match stack {
        Stack::Topmost => HWND_TOPMOST,
        Stack::Below(h) => HWND(h as *mut core::ffi::c_void),
        Stack::Above(h) => above(HWND(h as *mut core::ffi::c_void), shown),
    };
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW;
    // SAFETY: a deferred batch of z-order changes on our own windows; if the batch cannot be
    // made (a window just closed), each is placed on its own below.
    unsafe {
        if let Ok(mut batch) = BeginDeferWindowPos(i32::try_from(shown.len()).unwrap_or(1)) {
            let mut after = first_after;
            let mut ok = true;
            for h in shown {
                match DeferWindowPos(batch, *h, Some(after), 0, 0, 0, 0, flags) {
                    Ok(next) => batch = next,
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
                after = *h;
            }
            if ok && EndDeferWindowPos(batch).is_ok() {
                return;
            }
        }
        let mut after = first_after;
        for h in shown {
            let _ = SetWindowPos(*h, Some(after), 0, 0, 0, 0, flags);
            after = *h;
        }
    }
}
