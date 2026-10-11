//! Mosaïk's native overlays: the zone preview shown while dragging, the coloured frames and
//! tags around managed windows, and tab strips. They are layered windows drawn by Mosaïk (not
//! DWM border colours, which only exist on Windows 11), so Windows 10 and 11 look the same, and
//! they never take focus or clicks they do not need.
//!
//! Drawing is plain CPU rasterisation into a premultiplied-alpha bitmap: overlays are simple
//! shapes, rendered once per size and cached, so moving a preview costs one
//! `UpdateLayeredWindow` call.

pub mod paint;

#[cfg(windows)]
pub mod layered;
