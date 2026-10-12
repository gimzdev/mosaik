//! Screen captures of small regions, compared numerically: used to check that a window shown
//! again repaints at once instead of flashing stale or black content (the analysis is in
//! `pixels`). With `--shots`, the workspace probe also saves pictures of its own windows, for a
//! look at what Mosaïk draws on them.

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

/// Saves the pixels of `r` on screen as a 24-bit BMP file.
pub fn save_bmp(r: Rect, path: &std::path::Path) -> Result<(), String> {
    let pixels = capture(r).ok_or("the screen could not be captured")?;
    let (w, h) = (
        usize::try_from(r.width()).map_err(|e| e.to_string())?,
        usize::try_from(r.height()).map_err(|e| e.to_string())?,
    );
    // Rows are padded to four bytes and stored bottom row first.
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let u32le = |v: usize| u32::try_from(v).unwrap_or(u32::MAX).to_le_bytes();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&u32le(size));
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&u32le(54));
    out.extend_from_slice(&u32le(40));
    out.extend_from_slice(&u32le(w));
    out.extend_from_slice(&u32le(h));
    out.extend_from_slice(&[1, 0, 24, 0]);
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&u32le(row * h));
    out.extend_from_slice(&[0; 16]);
    for y in (0..h).rev() {
        let start = out.len();
        for p in &pixels[y * w..(y + 1) * w] {
            out.extend_from_slice(&p.to_le_bytes()[..3]);
        }
        out.resize(start + row, 0);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
