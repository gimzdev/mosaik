//! A layered window that shows a premultiplied bitmap at a screen rectangle.

use std::ffi::c_void;

use mosaik_core::geometry::Rect;
use mosaik_win::error::{Context, Error, Result};
use mosaik_win::own::{WindowClass, destroy, plain_wndproc};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC,
    SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::paint::Pixels;

/// A 32-bit top-down DIB selected into a memory DC, holding what the window shows.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    width: usize,
    height: usize,
}

impl Surface {
    fn new(width: usize, height: usize) -> Result<Self> {
        let w = i32::try_from(width.max(1)).map_err(|_| Error::Other("overlay too wide".into()))?;
        let h = i32::try_from(height.max(1)).map_err(|_| Error::Other("overlay too tall".into()))?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
                biWidth: w,
                // Negative height: rows top to bottom, like `Pixels`.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: a memory DC compatible with the screen and a DIB section described by `info`;
        // both are released in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(Error::Other("CreateCompatibleDC failed".into()));
            }
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) if !bits.is_null() => b,
                _ => {
                    let _ = DeleteDC(dc);
                    return Err(Error::Other("CreateDIBSection failed".into()));
                }
            };
            let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits: bits.cast::<u32>(),
                width: width.max(1),
                height: height.max(1),
            })
        }
    }

    fn copy_from(&mut self, p: &Pixels) {
        let n = self.width * self.height;
        if p.width != self.width || p.height != self.height || p.data.len() != n {
            return;
        }
        // SAFETY: the DIB section holds exactly width * height 32-bit pixels, and `p.data` has the
        // same length; the regions do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(p.data.as_ptr(), self.bits, n) };
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: restores the DC's original bitmap, then frees the objects created in `new`.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

/// A top-most, click-through (optional), never-activated layered window.
pub struct LayeredWindow {
    hwnd: HWND,
    surface: Option<Surface>,
    shown: Option<Rect>,
}

impl LayeredWindow {
    /// Creates the window hidden. Overlays that only show (the drag preview, frames) pass
    /// `click_through`; the tab strip does not.
    pub fn new(class_name: &str, click_through: bool) -> Result<Self> {
        let class = WindowClass::register(class_name, Some(plain_wndproc))?;
        let mut ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
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

    /// Shows `pixels` at `at` (which must have the bitmap's size) in one call that moves,
    /// resizes and repaints the window.
    pub fn show(&mut self, at: Rect, pixels: &Pixels) -> Result<()> {
        let needs_new = self
            .surface
            .as_ref()
            .is_none_or(|s| s.width != pixels.width || s.height != pixels.height);
        if needs_new {
            self.surface = Some(Surface::new(pixels.width, pixels.height)?);
        }
        let surface = self
            .surface
            .as_mut()
            .ok_or_else(|| Error::Other("no surface".into()))?;
        surface.copy_from(pixels);
        let pos = POINT {
            x: at.left,
            y: at.top,
        };
        let size = SIZE {
            cx: i32::try_from(pixels.width).unwrap_or(1),
            cy: i32::try_from(pixels.height).unwrap_or(1),
        };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: u8::try_from(AC_SRC_OVER).unwrap_or(0),
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: u8::try_from(AC_SRC_ALPHA).unwrap_or(1),
        };
        // SAFETY: the screen DC is released below; the memory DC holds a 32-bit premultiplied
        // DIB of exactly `size`.
        unsafe {
            let screen = GetDC(None);
            let r = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&pos),
                Some(&size),
                Some(surface.dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            r.ctx("UpdateLayeredWindow")?;
            if self.shown.is_none() {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
        self.shown = Some(at);
        Ok(())
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
}

impl Drop for LayeredWindow {
    fn drop(&mut self) {
        self.surface = None;
        destroy(self.hwnd);
    }
}
