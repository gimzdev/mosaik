//! A 32-bit premultiplied bitmap in a memory device context: what a layered window shows, and
//! what the canvas draws into.

use std::ffi::c_void;

use mosaik_win::error::{Error, Result};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
};

use crate::paint::Pixels;

/// A top-down DIB selected into its own memory DC.
pub struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u32,
    width: usize,
    height: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Result<Self> {
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

    pub fn dc(&self) -> HDC {
        self.dc
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Copies pixels of exactly this surface's size; other sizes are ignored.
    pub fn copy_from(&mut self, p: &Pixels) {
        let n = self.width * self.height;
        if p.width != self.width || p.height != self.height || p.data.len() != n {
            return;
        }
        // SAFETY: the DIB section holds exactly width * height 32-bit pixels, and `p.data` has the
        // same length; the regions do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(p.data.as_ptr(), self.bits, n) };
    }

    /// Makes every pixel transparent.
    pub fn clear(&mut self) {
        // SAFETY: the DIB section holds exactly width * height 32-bit pixels.
        unsafe { std::ptr::write_bytes(self.bits, 0, self.width * self.height) };
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
