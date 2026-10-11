//! Screen captures of small regions, compared numerically (never saved): used to check that a
//! window shown again repaints at once instead of flashing stale or black content. The analysis
//! is in `pixels`.

use mosaik_core::geometry::Rect;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
};

/// The pixels of `r` on screen as 0x00RRGGBB, top row first.
pub fn capture(r: Rect) -> Option<Vec<u32>> {
    let (w, h) = (r.width(), r.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let n = usize::try_from(w).ok()? * usize::try_from(h).ok()?;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: GDI objects created here are released before returning; the DIB holds `n` pixels.
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let result = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .ok()
            .and_then(|bmp| {
                let old = SelectObject(mem, HGDIOBJ(bmp.0));
                let ok = BitBlt(mem, 0, 0, w, h, Some(screen), r.left, r.top, SRCCOPY | CAPTUREBLT).is_ok();
                let pixels = (ok && !bits.is_null()).then(|| {
                    std::slice::from_raw_parts(bits.cast::<u32>(), n)
                        .iter()
                        .map(|p| p & 0x00FF_FFFF)
                        .collect()
                });
                SelectObject(mem, old);
                let _ = DeleteObject(HGDIOBJ(bmp.0));
                pixels
            });
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}
