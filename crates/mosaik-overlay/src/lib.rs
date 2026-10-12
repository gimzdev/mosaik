//! Mosaïk's native overlays: the zone preview shown while dragging, the coloured frames and
//! tags around managed windows, tab strips, and the quick menu at the screen's edge. They are layered windows drawn by Mosaïk (not
//! DWM border colours, which only exist on Windows 11), so Windows 10 and 11 look the same, and
//! they never take focus or clicks they do not need.
//!
//! Overlays are drawn with Direct2D and DirectWrite (software rendering: they are small) into
//! premultiplied bitmaps, once per size, so moving one costs a single window move. Rings are
//! four thin windows rather than one large transparent one, so a frame around a big window
//! costs kilobytes, not megabytes.

pub mod paint;

#[cfg(windows)]
pub mod canvas;
#[cfg(windows)]
pub mod edge;
#[cfg(windows)]
pub mod frame;
#[cfg(windows)]
pub mod layered;
#[cfg(windows)]
pub mod shapes;
#[cfg(windows)]
pub mod surface;
#[cfg(windows)]
pub mod tabbar;
#[cfg(windows)]
pub mod tint;
