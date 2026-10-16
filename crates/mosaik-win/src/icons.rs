//! Apps' icons for Mosaïk's window: the picture Windows shows for an app in the Start menu and on
//! the taskbar, read through the shell (packaged and desktop apps alike) and written as a PNG
//! with Windows' own encoder (WIC). A thread of its own, with COM, reads them one at a time and
//! keeps what it read, so each app's icon is read once per run.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use mosaik_core::icons::{self, Source};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
    HBITMAP, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
    WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHCreateMemStream, SIIGBF_ICONONLY,
};
use windows::core::PCWSTR;

use crate::error::{Error, Result};
use crate::text::wide;

/// The most keys read in one request: the window asks for 24 at a time, a long list (Add an app's,
/// every app of the Start menu) a part at a time (`apps/desktop/src/icons.tsx`).
pub const MOST_PER_REQUEST: usize = 96;

type Answer = Vec<(String, Option<Vec<u8>>)>;

/// Reads icons on its own thread. Cloned freely; the thread ends when the last clone goes.
#[derive(Clone)]
pub struct IconReader {
    asks: Sender<(Vec<String>, Sender<Answer>)>,
}

impl IconReader {
    /// Starts the thread; icons come out `px` pixels square.
    pub fn start(px: u32) -> Result<Self> {
        let (asks, inbox) = mpsc::channel();
        std::thread::Builder::new()
            .name("mosaik-icons".into())
            .spawn(move || serve(&inbox, px))
            .map_err(|e| Error::Other(format!("the icon reader did not start ({e})")))?;
        Ok(Self { asks })
    }

    /// Each key with its icon as a PNG, or `None` when Windows has none for it (or it takes
    /// longer than `wait` in all). Keys past [`MOST_PER_REQUEST`] are left out.
    pub fn read(&self, keys: Vec<String>, wait: Duration) -> Answer {
        let keys: Vec<String> = keys.into_iter().take(MOST_PER_REQUEST).collect();
        let (reply, answer) = mpsc::channel();
        if self.asks.send((keys.clone(), reply)).is_err() {
            return keys.into_iter().map(|k| (k, None)).collect();
        }
        answer
            .recv_timeout(wait)
            .unwrap_or_else(|_| keys.into_iter().map(|k| (k, None)).collect())
    }
}

fn serve(inbox: &Receiver<(Vec<String>, Sender<Answer>)>, px: u32) {
    // SAFETY: COM for this thread only, for the shell's and WIC's objects; undone below.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let mut kept: HashMap<String, Option<Vec<u8>>> = HashMap::new();
    while let Ok((keys, reply)) = inbox.recv() {
        let answer = keys
            .into_iter()
            .map(|k| {
                // An app without a picture is told by its name: nothing to report.
                let png = kept
                    .entry(k.clone())
                    .or_insert_with(|| png_for(&k, px).ok())
                    .clone();
                (k, png)
            })
            .collect();
        let _ = reply.send(answer);
    }
    if com {
        // SAFETY: pairs the successful initialisation above, on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// The icon for `key`, `px` pixels square, as a PNG.
pub fn png_for(key: &str, px: u32) -> Result<Vec<u8>> {
    let path = match icons::parse(key).ok_or_else(|| Error::Other(format!("not an icon key: {key}")))? {
        Source::App(id) => format!(r"shell:AppsFolder\{id}"),
        Source::Program(path) => path.to_owned(),
    };
    let side = i32::try_from(px).map_err(|_| Error::Other("icon size too large".into()))?;
    let name = wide(&path);
    // SAFETY: a NUL-terminated path that outlives the call; no bind context.
    let item: IShellItemImageFactory = unsafe { SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None) }
        .map_err(|e| Error::Other(format!("the shell does not know {path} ({})", e.message())))?;
    // SAFETY: asks the shell for the item's icon as a bitmap of our own, deleted below.
    let bitmap = unsafe { item.GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY) }
        .map_err(|e| Error::Other(format!("no picture for {path} ({})", e.message())))?;
    let pixels = read_pixels(bitmap);
    // SAFETY: the bitmap is ours (GetImage hands it over) and no longer selected anywhere.
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    let (w, h, mut bgra) = pixels?;
    icons::straighten(&mut bgra);
    encode_png(w, h, &bgra)
}

/// A 32-bit bitmap's pixels, top row first: width, height, then blue, green, red and alpha.
fn read_pixels(bitmap: HBITMAP) -> Result<(u32, u32, Vec<u8>)> {
    let mut info = BITMAP::default();
    let size = i32::try_from(std::mem::size_of::<BITMAP>()).unwrap_or(0);
    // SAFETY: fills `info`, a BITMAP of the size given, from a bitmap we own.
    let got = unsafe { GetObjectW(HGDIOBJ(bitmap.0), size, Some((&raw mut info).cast())) };
    if got == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
        return Err(Error::Other("the icon's bitmap could not be read".into()));
    }
    let (w, h) = (info.bmWidth, info.bmHeight.abs());
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
            biWidth: w,
            // Negative: rows top to bottom.
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let (wu, hu) = (w.unsigned_abs(), h.unsigned_abs());
    let mut pixels = vec![0u8; wu as usize * hu as usize * 4];
    // SAFETY: the screen's device context, released below.
    let dc = unsafe { GetDC(None) };
    // SAFETY: copies `h` rows of 32-bit pixels into a buffer of exactly that size.
    let rows = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            hu,
            Some(pixels.as_mut_ptr().cast()),
            &raw mut header,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: releases the device context taken above.
    let _ = unsafe { ReleaseDC(None, dc) };
    if rows <= 0 {
        return Err(Error::Other("the icon's pixels could not be read".into()));
    }
    Ok((wu, hu, pixels))
}

/// Straight blue-green-red-alpha pixels, top row first, as a PNG (WIC's encoder).
fn encode_png(w: u32, h: u32, bgra: &[u8]) -> Result<Vec<u8>> {
    fn failed(what: &'static str) -> impl Fn(windows::core::Error) -> Error {
        move |e| Error::Other(format!("{what} ({})", e.message()))
    }
    // SAFETY: WIC's factory, made on this thread (COM is set up by the caller's thread).
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
            .map_err(failed("Windows' image encoder is missing"))?;
    // SAFETY: an empty stream in memory, ours.
    let stream = unsafe { SHCreateMemStream(None) }.ok_or_else(|| Error::Other("no memory stream".into()))?;
    // SAFETY: the calls below follow WIC's order: encoder, frame, size, format, pixels, commit.
    unsafe {
        let encoder = factory
            .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
            .map_err(failed("no PNG encoder"))?;
        encoder
            .Initialize(&stream, WICBitmapEncoderNoCache)
            .map_err(failed("the PNG encoder did not start"))?;
        let mut frame = None;
        let mut options = None;
        encoder
            .CreateNewFrame(&mut frame, &mut options)
            .map_err(failed("no PNG frame"))?;
        let frame = frame.ok_or_else(|| Error::Other("no PNG frame".into()))?;
        frame
            .Initialize(options.as_ref())
            .map_err(failed("the PNG frame did not start"))?;
        frame.SetSize(w, h).map_err(failed("the PNG's size"))?;
        let mut format = GUID_WICPixelFormat32bppBGRA;
        frame
            .SetPixelFormat(&mut format)
            .map_err(failed("the PNG's pixel format"))?;
        if format != GUID_WICPixelFormat32bppBGRA {
            return Err(Error::Other("the PNG encoder wants other pixels".into()));
        }
        frame
            .WritePixels(h, w * 4, bgra)
            .map_err(failed("the PNG's pixels"))?;
        frame.Commit().map_err(failed("the PNG frame"))?;
        encoder.Commit().map_err(failed("the PNG"))?;

        let mut stat = STATSTG::default();
        stream
            .Stat(&mut stat, STATFLAG_NONAME)
            .map_err(failed("the PNG's size"))?;
        let size = u32::try_from(stat.cbSize).map_err(|_| Error::Other("the PNG is too large".into()))?;
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(failed("the PNG's start"))?;
        let mut png = vec![0u8; size as usize];
        let mut read = 0u32;
        stream
            .Read(png.as_mut_ptr().cast(), size, Some(&raw mut read))
            .ok()
            .map_err(failed("the PNG's bytes"))?;
        png.truncate(read as usize);
        Ok(png)
    }
}
