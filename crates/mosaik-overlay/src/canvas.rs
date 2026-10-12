//! Drawing overlays with Direct2D and DirectWrite into a [`Surface`]: smooth rounded shapes and
//! crisp text with transparency, the same on Windows 10 and 11.
//!
//! The render target is a software one: overlays are small, so drawing on the CPU is quick,
//! never waits on the GPU and never loses its device after a driver update.

use std::collections::HashMap;

use mosaik_core::zone::Rgb;
use mosaik_win::error::{Context, Result};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DASH_STYLE_DASH, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::{BOOL, w};

use mosaik_win::text::wide;

use crate::surface::Surface;

/// A colour with opacity, components from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub fn of(c: Rgb, alpha: f32) -> Self {
        Self(
            f32::from(c.0) / 255.0,
            f32::from(c.1) / 255.0,
            f32::from(c.2) / 255.0,
            alpha,
        )
    }

    fn d2d(self) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: self.0,
            g: self.1,
            b: self.2,
            a: self.3,
        }
    }
}

/// A rectangle in pixels of the surface, fractional for anti-aliased edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    fn d2d(self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }

    pub fn inset(self, by: f32) -> Self {
        Self::new(self.left + by, self.top + by, self.right - by, self.bottom - by)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Normal,
    SemiBold,
}

impl Weight {
    fn dwrite(self) -> DWRITE_FONT_WEIGHT {
        match self {
            Weight::Normal => DWRITE_FONT_WEIGHT_NORMAL,
            Weight::SemiBold => DWRITE_FONT_WEIGHT_SEMI_BOLD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Leading,
    Center,
}

pub struct Canvas {
    factory: ID2D1Factory,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    dwrite: IDWriteFactory,
    /// Text formats by size (in hundredths of a pixel) and weight.
    formats: HashMap<(u32, Weight), IDWriteTextFormat>,
    font: Vec<u16>,
}

/// Windows 11's UI font when present, else Windows 10's.
fn ui_font(dwrite: &IDWriteFactory) -> Vec<u16> {
    let mut collection = None;
    // SAFETY: reads the system font collection; out parameters are locals.
    let found = unsafe {
        dwrite.GetSystemFontCollection(&mut collection, false).is_ok()
            && collection.is_some_and(|c| {
                let mut index = 0u32;
                let mut exists = BOOL(0);
                c.FindFamilyName(w!("Segoe UI Variable Text"), &mut index, &mut exists)
                    .is_ok()
                    && exists.as_bool()
            })
    };
    wide(if found {
        "Segoe UI Variable Text"
    } else {
        "Segoe UI"
    })
}

impl Canvas {
    pub fn new() -> Result<Self> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            // Device-independent pixels map 1:1 to physical pixels; callers scale by DPI.
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: factory, target and brush creation with valid, fully initialised parameters.
        unsafe {
            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ctx("D2D1CreateFactory")?;
            let target = factory.CreateDCRenderTarget(&props).ctx("CreateDCRenderTarget")?;
            // Grayscale: ClearType needs an opaque background, overlays are transparent.
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target
                .CreateSolidColorBrush(&Rgba(0.0, 0.0, 0.0, 1.0).d2d(), None)
                .ctx("CreateSolidColorBrush")?;
            let dwrite: IDWriteFactory =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ctx("DWriteCreateFactory")?;
            let font = ui_font(&dwrite);
            Ok(Self {
                factory,
                target,
                brush,
                dwrite,
                formats: HashMap::new(),
                font,
            })
        }
    }

    fn format(&mut self, size: f32, weight: Weight) -> Result<IDWriteTextFormat> {
        // Sizes are small positive numbers; hundredths of a pixel are plenty.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = ((size.max(1.0) * 100.0).round() as u32, weight);
        if let Some(f) = self.formats.get(&key) {
            return Ok(f.clone());
        }
        // SAFETY: plain DirectWrite object creation; the font name is NUL-terminated.
        let f = unsafe {
            let f = self
                .dwrite
                .CreateTextFormat(
                    windows::core::PCWSTR(self.font.as_ptr()),
                    None,
                    weight.dwrite(),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )
                .ctx("CreateTextFormat")?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .ctx("SetParagraphAlignment")?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .ctx("SetWordWrapping")?;
            // Long names end with an ellipsis instead of overflowing.
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            let sign = self
                .dwrite
                .CreateEllipsisTrimmingSign(&f)
                .ctx("CreateEllipsisTrimmingSign")?;
            f.SetTrimming(&trimming, &sign).ctx("SetTrimming")?;
            f
        };
        self.formats.insert(key, f.clone());
        Ok(f)
    }

    /// The width a box needs to show all of `text` at `size` (pixels) and `weight`, in whole
    /// pixels with one to spare: a box exactly as wide as the text's measured width can still
    /// get the text cut short with an ellipsis (job 0063: a tab's chip read "Probe Termi…").
    pub fn text_width(&mut self, text: &str, size: f32, weight: Weight) -> f32 {
        let measured = self.measured_width(text, size, weight);
        if measured > 0.0 {
            measured.ceil() + 1.0
        } else {
            0.0
        }
    }

    fn measured_width(&mut self, text: &str, size: f32, weight: Weight) -> f32 {
        let Ok(format) = self.format(size, weight) else {
            return 0.0;
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: a temporary layout to read metrics from; the out struct is a local.
        unsafe {
            self.dwrite
                .CreateTextLayout(&wide, &format, 100_000.0, 10_000.0)
                .and_then(|layout| {
                    let mut m = DWRITE_TEXT_METRICS::default();
                    layout
                        .GetMetrics(&mut m)
                        .map(|()| m.widthIncludingTrailingWhitespace)
                })
                .unwrap_or(0.0)
        }
    }

    /// Clears `surface` and runs `draw` on it.
    pub fn render(
        &mut self,
        surface: &mut Surface,
        draw: impl FnOnce(&mut Pen<'_>) -> Result<()>,
    ) -> Result<()> {
        surface.clear();
        let area = RECT {
            left: 0,
            top: 0,
            right: i32::try_from(surface.width()).unwrap_or(1),
            bottom: i32::try_from(surface.height()).unwrap_or(1),
        };
        // SAFETY: binds the render target to the surface's DC, which outlives this call.
        unsafe { self.target.BindDC(surface.dc(), &area) }.ctx("BindDC")?;
        // SAFETY: drawing happens between BeginDraw and EndDraw on the bound target.
        unsafe { self.target.BeginDraw() };
        let drawn = {
            let mut pen = Pen { canvas: self };
            draw(&mut pen)
        };
        // SAFETY: ends the draw started above.
        let ended = unsafe { self.target.EndDraw(None, None) }.ctx("EndDraw");
        drawn.and(ended)
    }
}

/// Drawing operations, valid inside [`Canvas::render`].
pub struct Pen<'a> {
    canvas: &'a mut Canvas,
}

impl Pen<'_> {
    fn brush(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        // SAFETY: changes the colour of the canvas's own brush.
        unsafe { self.canvas.brush.SetColor(&c.d2d()) };
        &self.canvas.brush
    }

    /// Strokes a rounded rectangle with a dashed line `width` wide, drawn inside `a`.
    pub fn dash_round(&self, a: Area, radius: f32, width: f32, c: Rgba) {
        let props = D2D1_STROKE_STYLE_PROPERTIES {
            dashStyle: D2D1_DASH_STYLE_DASH,
            ..Default::default()
        };
        // SAFETY: a stroke style from the canvas's own factory, used at once.
        let Ok(style) = (unsafe { self.canvas.factory.CreateStrokeStyle(&props, None) }) else {
            self.stroke_round(a, radius, width, c);
            return;
        };
        let half = width / 2.0;
        let rr = D2D1_ROUNDED_RECT {
            rect: a.inset(half).d2d(),
            radiusX: (radius - half).max(0.0),
            radiusY: (radius - half).max(0.0),
        };
        // SAFETY: inside BeginDraw/EndDraw on the bound target.
        unsafe {
            self.canvas
                .target
                .DrawRoundedRectangle(&rr, self.brush(c), width, Some(&style));
        }
    }

    pub fn fill_round(&self, a: Area, radius: f32, c: Rgba) {
        let rr = D2D1_ROUNDED_RECT {
            rect: a.d2d(),
            radiusX: radius,
            radiusY: radius,
        };
        // SAFETY: inside BeginDraw/EndDraw on the bound target.
        unsafe { self.canvas.target.FillRoundedRectangle(&rr, self.brush(c)) };
    }

    /// Strokes a rounded rectangle with a line `width` wide, drawn inside `a`.
    pub fn stroke_round(&self, a: Area, radius: f32, width: f32, c: Rgba) {
        let half = width / 2.0;
        let rr = D2D1_ROUNDED_RECT {
            rect: a.inset(half).d2d(),
            radiusX: (radius - half).max(0.0),
            radiusY: (radius - half).max(0.0),
        };
        // SAFETY: inside BeginDraw/EndDraw on the bound target.
        unsafe {
            self.canvas
                .target
                .DrawRoundedRectangle(&rr, self.brush(c), width, None);
        }
    }

    pub fn text(
        &mut self,
        text: &str,
        a: Area,
        size: f32,
        weight: Weight,
        align: Align,
        c: Rgba,
    ) -> Result<()> {
        let format = self.canvas.format(size, weight)?;
        // SAFETY: changes the alignment of a format the canvas owns; it is set before every use.
        unsafe {
            format
                .SetTextAlignment(match align {
                    Align::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                })
                .ctx("SetTextAlignment")?;
        }
        let wide: Vec<u16> = text.encode_utf16().collect();
        let brush = self.brush(c);
        // SAFETY: inside BeginDraw/EndDraw on the bound target.
        unsafe {
            self.canvas.target.DrawText(
                &wide,
                &format,
                &a.d2d(),
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }
}
