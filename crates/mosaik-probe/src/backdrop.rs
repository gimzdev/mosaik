//! A solid magenta window right under the app being measured. Screen captures then show the app,
//! the backdrop (the app is really hidden) or something else (black, a stale frame), and never
//! the user's own windows.

use mosaik_core::geometry::Rect;
use mosaik_win::own::{WindowClass, destroy, plain_wndproc};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HGDIOBJ, PAINTSTRUCT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_NOTOPMOST, HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetWindowPos, ShowWindow, WM_ERASEBKGND, WM_PAINT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

pub struct Backdrop {
    hwnd: HWND,
}

impl Backdrop {
    /// Creates the backdrop (hidden) over `area`, normally the work area being measured.
    pub fn new(area: Rect) -> Result<Self, String> {
        let class =
            WindowClass::register("MosaikProbeBackdrop", Some(backdrop_proc)).map_err(|e| e.to_string())?;
        let hwnd = class
            .create(
                "Mosaïk probe backdrop",
                WS_POPUP,
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                area,
                None,
            )
            .map_err(|e| e.to_string())?;
        Ok(Self { hwnd })
    }

    /// Shows the backdrop above the user's windows, then puts `app` above the backdrop.
    pub fn under(&self, app: HWND) {
        // SAFETY: showing our own window without activating it.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        raise(self.hwnd);
        raise(app);
        mosaik_win::events::pump_until(200, || false);
    }

    pub fn hide(&self) {
        // SAFETY: hiding our own window.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        destroy(self.hwnd);
    }
}

/// Puts a window at the top of the normal (not always-on-top) windows without activating it:
/// made topmost and back, which Windows allows from a background process.
pub fn raise(hwnd: HWND) {
    // SAFETY: z-order changes only, on our own window or one the probe opened.
    unsafe {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
        let _ = SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, flags);
    }
}

unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: standard paint cycle on our own window; the brush is deleted after use.
            unsafe {
                let dc = BeginPaint(hwnd, &mut ps);
                // COLORREF is 0x00BBGGRR; magenta reads the same both ways.
                let brush = CreateSolidBrush(COLORREF(0x00FF_00FF));
                let _ = FillRect(dc, &ps.rcPaint, brush);
                let _ = DeleteObject(HGDIOBJ(brush.0));
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { plain_wndproc(hwnd, msg, wparam, lparam) },
    }
}
