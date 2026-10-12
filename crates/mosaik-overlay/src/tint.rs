//! A solid shape, translucent or not: the zone a dragged window would drop into, and the band a
//! window wears its tag in above it. Windows composes it at a fixed opacity from a solid colour,
//! so it costs no bitmap however large the zone (a 4K zone drawn as a bitmap would be 33 MB).

use mosaik_core::geometry::Rect;
use mosaik_core::zone::Rgb;
use mosaik_win::error::{Context, Result};
use mosaik_win::own::{WindowClass, destroy};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HDC, HGDIOBJ,
    InvalidateRect, PAINTSTRUCT, SetWindowRgn,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, DefWindowProcW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, LWA_ALPHA,
    MA_NOACTIVATE, SW_HIDE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SWP_SHOWWINDOW,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WM_CLOSE,
    WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_PAINT, WNDPROC, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::layered::{Stack, restack_windows};

fn colorref(Rgb(r, g, b): Rgb) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

unsafe extern "system" fn tint_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            // SAFETY: reads the colour stored on our own window; the brush is deleted below; the
            // DC is the one Windows passed for this message.
            unsafe {
                let color = u32::try_from(GetWindowLongPtrW(hwnd, GWLP_USERDATA)).unwrap_or(0);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(COLORREF(color));
                FillRect(HDC(wparam.0 as *mut core::ffi::c_void), &rc, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
            LRESULT(1)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: an empty paint cycle on our own window (the background is the whole look).
            unsafe {
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// A band above a window works as that window's title bar: it takes the mouse as a tab strip
/// does, and the engine moves the window with it. Everything else is a tint's.
unsafe extern "system" fn band_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(r) = crate::tabbar::mouse(hwnd, msg, lparam) {
        return r;
    }
    // SAFETY: a tint's handling of its own window's messages.
    unsafe { tint_proc(hwnd, msg, wparam, lparam) }
}

/// Which corners of a tint are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corners {
    All,
    /// The top two: a band that sits on a window's top edge.
    Top,
}

/// A never-activated rounded rectangle of one colour: click-through, or a window's band.
pub struct Tint {
    hwnd: HWND,
    shown: Option<Rect>,
    look: Option<(Rgb, u8, i32)>,
    shape: Option<(i32, i32, i32, Corners)>,
}

impl Tint {
    /// A tint clicks go through.
    pub fn new() -> Result<Self> {
        Self::make("MosaikTint", Some(tint_proc), WS_EX_TRANSPARENT)
    }

    /// The band above a window: solid, it works as the window's title bar, so it takes the
    /// mouse (`tabbar::take_inputs` names it) rather than let a press reach whatever is behind.
    pub fn band() -> Result<Self> {
        Self::make("MosaikBand", Some(band_proc), WINDOW_EX_STYLE(0))
    }

    fn make(class: &str, proc: WNDPROC, input: WINDOW_EX_STYLE) -> Result<Self> {
        let class = WindowClass::register_with(class, proc, CS_HREDRAW | CS_VREDRAW)?;
        let ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | input;
        let hwnd = class.create("", WS_POPUP, ex, Rect::new(0, 0, 1, 1), None)?;
        Ok(Self {
            hwnd,
            shown: None,
            look: None,
            shape: None,
        })
    }

    /// Whether `hwnd` is this tint's window.
    pub fn is(&self, hwnd: isize) -> bool {
        self.hwnd.0 as isize == hwnd
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// Shows the tint over `at` in `color` at `alpha` (0 to 255), `corners` rounded by `radius`.
    pub fn show(
        &mut self,
        at: Rect,
        color: Rgb,
        alpha: u8,
        (radius, corners): (i32, Corners),
        stack: Stack,
    ) -> Result<()> {
        let look = (color, alpha, radius);
        if self.look != Some(look) {
            // SAFETY: our own window: its colour, its opacity, and a repaint.
            unsafe {
                SetWindowLongPtrW(
                    self.hwnd,
                    GWLP_USERDATA,
                    isize::try_from(colorref(color)).unwrap_or(0),
                );
                SetLayeredWindowAttributes(self.hwnd, COLORREF(0), alpha, LWA_ALPHA)
                    .ctx("SetLayeredWindowAttributes")?;
                let _ = InvalidateRect(Some(self.hwnd), None, true);
            }
            self.look = Some(look);
        }
        // SAFETY: moves, sizes and shows our own window without activating it.
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                at.left,
                at.top,
                at.width().max(1),
                at.height().max(1),
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
            )
            .ctx("SetWindowPos")?;
        }
        let shape = (at.width(), at.height(), radius, corners);
        if self.shape != Some(shape) {
            // Square bottom corners: the rounded ones fall below the window, where nothing shows.
            let below = match corners {
                Corners::All => 0,
                Corners::Top => radius,
            };
            // SAFETY: a region for our own window; Windows owns it once set.
            unsafe {
                let rgn = CreateRoundRectRgn(
                    0,
                    0,
                    at.width() + 1,
                    at.height() + 1 + below,
                    2 * radius,
                    2 * radius,
                );
                let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            }
            self.shape = Some(shape);
        }
        self.shown = Some(at);
        restack_windows(&[self.hwnd], stack);
        Ok(())
    }

    pub fn is_shown(&self) -> bool {
        self.shown.is_some()
    }

    pub fn hide(&mut self) {
        if self.shown.take().is_some() {
            // SAFETY: our own window.
            let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
    }
}

impl Drop for Tint {
    fn drop(&mut self) {
        destroy(self.hwnd);
    }
}
