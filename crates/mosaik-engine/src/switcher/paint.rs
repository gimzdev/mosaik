//! Drawing the switcher with Direct2D and DirectWrite: crisp text and smooth corners at any DPI,
//! in the light, dark and high-contrast themes.

use mosaik_core::switcher::{Entry, Model};
use mosaik_core::zone::Rgb;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory, ID2D1DCRenderTarget,
    ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows::core::w;

/// What marks the workspace on screen, after its name (as in Mosaïk's window).
const OPEN: &str = "On screen";

/// Layout at 96 DPI; everything is scaled by `dpi / 96`.
pub(crate) mod size {
    pub const WIDTH: f32 = 480.0;
    pub const HEADER: f32 = 52.0;
    pub const FOOTER: f32 = 40.0;
    pub const ROW: f32 = 60.0;
    pub const PAD: f32 = 8.0;
    pub const EMPTY: f32 = 150.0;
    pub const MAX_ROWS: usize = 7;
}

/// The colours of one theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Theme {
    pub panel: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub line: Rgba,
    pub selection: Rgba,
    pub selection_text: Rgba,
    pub accent: Rgba,
    pub thumb: Rgba,
    /// High contrast: zone colours are drawn as outlines in the text colour.
    pub high_contrast: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rgba(pub f32, pub f32, pub f32, pub f32);

impl Rgba {
    pub const fn hex(rgb: u32, alpha: f32) -> Self {
        Self(
            ((rgb >> 16) & 0xFF) as f32 / 255.0,
            ((rgb >> 8) & 0xFF) as f32 / 255.0,
            (rgb & 0xFF) as f32 / 255.0,
            alpha,
        )
    }

    pub fn of(c: Rgb) -> Self {
        Self(
            f32::from(c.0) / 255.0,
            f32::from(c.1) / 255.0,
            f32::from(c.2) / 255.0,
            1.0,
        )
    }

    /// GDI's 0x00BBGGRR, for the list box background brush.
    pub fn colorref(self) -> u32 {
        // Clamped to 0..=255 first, so the conversion is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        c(self.0) | (c(self.1) << 8) | (c(self.2) << 16)
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

impl Theme {
    /// The window's dark colours, as the side menu and the toasts (`Palette::dark`).
    pub const DARK: Theme = Theme {
        panel: Rgba::hex(0x1B2130, 1.0),
        text: Rgba::hex(0xEDEFEA, 1.0),
        muted: Rgba::hex(0xA0A9B8, 1.0),
        line: Rgba::hex(0xFFFFFF, 0.11),
        selection: Rgba::hex(0x262E3F, 1.0),
        selection_text: Rgba::hex(0xFFFFFF, 1.0),
        accent: Rgba::hex(0x8A9BFF, 1.0),
        thumb: Rgba::hex(0x0D1016, 1.0),
        high_contrast: false,
    };

    /// Paper and ink.
    pub const LIGHT: Theme = Theme {
        panel: Rgba::hex(0xF7F8F4, 1.0),
        text: Rgba::hex(0x161B25, 1.0),
        muted: Rgba::hex(0x556070, 1.0),
        line: Rgba::hex(0x161B25, 0.10),
        selection: Rgba::hex(0xE4E7E1, 1.0),
        selection_text: Rgba::hex(0x161B25, 1.0),
        accent: Rgba::hex(0x2B44D9, 1.0),
        thumb: Rgba::hex(0xEDEFEA, 1.0),
        high_contrast: false,
    };
}

/// What one list row shows.
#[derive(Clone, Copy)]
pub(crate) struct Row<'a> {
    pub entry: &'a Entry,
    /// The workspace in use now.
    pub current: bool,
    pub selected: bool,
    /// The key that opens it, when keys pick.
    pub key: Option<&'a str>,
}

pub(crate) struct Painter {
    factory: ID2D1Factory,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    title: IDWriteTextFormat,
    subtitle: IDWriteTextFormat,
    small: IDWriteTextFormat,
    header: IDWriteTextFormat,
    body: IDWriteTextFormat,
    /// The DPI the text formats were made for.
    dpi: u32,
    dwrite: IDWriteFactory,
}

/// A ratio (0 to 1) or aspect for drawing; well inside `f32`'s range.
#[allow(clippy::cast_possible_truncation)]
fn f(v: f64) -> f32 {
    v as f32
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: l,
        top: t,
        right: r,
        bottom: b,
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Windows 11's UI font when present, else Windows 10's.
fn ui_font(dwrite: &IDWriteFactory) -> windows::core::PCWSTR {
    let variable = w!("Segoe UI Variable Text");
    let mut collection = None;
    // SAFETY: reads the system font collection; out parameters are locals.
    let found = unsafe {
        dwrite.GetSystemFontCollection(&mut collection, false).is_ok()
            && collection.is_some_and(|c| {
                let mut index = 0u32;
                let mut exists = windows::core::BOOL(0);
                c.FindFamilyName(variable, &mut index, &mut exists).is_ok() && exists.as_bool()
            })
    };
    if found { variable } else { w!("Segoe UI") }
}

fn format(
    dwrite: &IDWriteFactory,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    wrap: bool,
    align: DWRITE_TEXT_ALIGNMENT,
) -> windows::core::Result<IDWriteTextFormat> {
    // SAFETY: plain DirectWrite object creation with static strings.
    unsafe {
        let f = dwrite.CreateTextFormat(
            ui_font(dwrite),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        f.SetTextAlignment(align)?;
        if wrap {
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        } else {
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            // Long names end with an ellipsis instead of running under the key hint.
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            let sign = dwrite.CreateEllipsisTrimmingSign(&f)?;
            f.SetTrimming(&trimming, &sign)?;
        }
        Ok(f)
    }
}

impl Painter {
    pub fn new(dpi: u32) -> windows::core::Result<Self> {
        // SAFETY: factory and render-target creation with valid, fully initialised parameters.
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                // Device-independent pixels map 1:1 to physical pixels; sizes are scaled by hand.
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let target = factory.CreateDCRenderTarget(&props)?;
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target.CreateSolidColorBrush(&Theme::DARK.text.d2d(), None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let [title, subtitle, small, header, body] = Self::formats(&dwrite, dpi)?;
            Ok(Self {
                title,
                subtitle,
                small,
                header,
                body,
                factory,
                target,
                brush,
                dpi,
                dwrite,
            })
        }
    }

    /// The text formats at `dpi`: title, subtitle, small (key caps and hints, centred in their
    /// boxes), header, and body (the empty list's explanation, wrapped and centred under its
    /// title). One place for them, so every DPI gets the same alignment.
    fn formats(dwrite: &IDWriteFactory, dpi: u32) -> windows::core::Result<[IDWriteTextFormat; 5]> {
        let s = dpi as f32 / 96.0;
        let (normal, semi_bold) = (DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD);
        let (leading, centre) = (DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_CENTER);
        Ok([
            format(dwrite, 15.0 * s, semi_bold, false, leading)?,
            format(dwrite, 12.5 * s, normal, false, leading)?,
            format(dwrite, 11.5 * s, normal, false, centre)?,
            format(dwrite, 15.0 * s, normal, false, leading)?,
            format(dwrite, 13.0 * s, normal, true, centre)?,
        ])
    }

    /// Text formats are made for one DPI; remake them when it changes.
    pub fn set_dpi(&mut self, dpi: u32) -> windows::core::Result<()> {
        if dpi == self.dpi {
            return Ok(());
        }
        [self.title, self.subtitle, self.small, self.header, self.body] = Self::formats(&self.dwrite, dpi)?;
        self.dpi = dpi;
        Ok(())
    }

    fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    fn color(&self, c: Rgba) -> &ID2D1SolidColorBrush {
        // SAFETY: changes the colour of the painter's own brush.
        unsafe { self.brush.SetColor(&c.d2d()) };
        &self.brush
    }

    fn text(&self, s: &str, f: &IDWriteTextFormat, r: D2D_RECT_F, c: Rgba) {
        let text = wide(s);
        let brush = self.color(c);
        // SAFETY: draws inside BeginDraw/EndDraw on the bound target.
        unsafe {
            self.target.DrawText(
                &text,
                f,
                &r,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    fn fill_round(&self, r: D2D_RECT_F, radius: f32, c: Rgba) {
        let rr = D2D1_ROUNDED_RECT {
            rect: r,
            radiusX: radius,
            radiusY: radius,
        };
        let brush = self.color(c);
        // SAFETY: draws inside BeginDraw/EndDraw on the bound target.
        unsafe { self.target.FillRoundedRectangle(&rr, brush) };
    }

    fn stroke_round(&self, r: D2D_RECT_F, radius: f32, c: Rgba, width: f32) {
        let rr = D2D1_ROUNDED_RECT {
            rect: r,
            radiusX: radius,
            radiusY: radius,
        };
        let brush = self.color(c);
        // SAFETY: draws inside BeginDraw/EndDraw on the bound target.
        unsafe { self.target.DrawRoundedRectangle(&rr, brush, width, None) };
    }

    fn fill(&self, r: D2D_RECT_F, c: Rgba) {
        let brush = self.color(c);
        // SAFETY: draws inside BeginDraw/EndDraw on the bound target.
        unsafe { self.target.FillRectangle(&r, brush) };
    }

    /// Runs `draw` on the device context `hdc` limited to `area`.
    fn on(&self, hdc: HDC, area: RECT, draw: impl FnOnce()) -> windows::core::Result<()> {
        // SAFETY: binds the DC render target to a DC that is valid for this paint cycle.
        unsafe {
            self.target.BindDC(hdc, &area)?;
            self.target.BeginDraw();
            draw();
            self.target.EndDraw(None, None)
        }
    }

    /// The whole window except the list: background, header with the filter, footer with keys.
    pub fn window(
        &self,
        hdc: HDC,
        client: RECT,
        theme: &Theme,
        model: &Model,
        shortcut: &str,
    ) -> windows::core::Result<()> {
        let s = self.scale();
        let w = (client.right - client.left) as f32;
        let h = (client.bottom - client.top) as f32;
        self.on(hdc, client, || {
            // SAFETY: inside BeginDraw on the bound target.
            unsafe { self.target.Clear(Some(&theme.panel.d2d())) };
            let header = size::HEADER * s;
            let footer = size::FOOTER * s;
            // Header: what is typed, or what to do.
            let filter = model.filter();
            let naming = model.naming().is_some();
            let (label, color) = if naming {
                ("Save as workspace", theme.muted)
            } else if filter.is_empty() {
                ("Switch workspace", theme.muted)
            } else {
                (filter, theme.text)
            };
            self.text(
                label,
                &self.header,
                rect(16.0 * s, 0.0, w - 140.0 * s, header),
                color,
            );
            if !filter.is_empty() && !naming {
                // A caret after the typed text, so it reads as a field.
                let caret_x = 16.0 * s + self.measure(filter, &self.header) + 2.0 * s;
                self.fill(
                    rect(
                        caret_x,
                        header / 2.0 - 9.0 * s,
                        caret_x + 1.5 * s,
                        header / 2.0 + 9.0 * s,
                    ),
                    theme.accent,
                );
            }
            if !naming {
                self.keys(shortcut.split('+'), w - 16.0 * s, header / 2.0, theme, false);
            }
            self.fill(rect(0.0, header - s, w, header), theme.line);
            // Footer.
            self.fill(rect(0.0, h - footer, w, h - footer + s), theme.line);
            let mut x = 16.0 * s;
            let hints: &[(&str, &str)] = if naming {
                &[("Enter", "save"), ("Esc", "back")]
            } else {
                &[("Enter", "open"), ("S", "save as workspace"), ("Esc", "close")]
            };
            for &(key, what) in hints {
                let kw = self.measure(key, &self.title) * 0.8 + 2.0 * s;
                self.text(key, &self.small, rect(x, h - footer, x + kw, h), theme.text);
                x += kw + 4.0 * s;
                let ww = self.measure(what, &self.small) + 2.0 * s;
                self.text(what, &self.small, rect(x, h - footer, x + ww, h), theme.muted);
                x += ww + 18.0 * s;
            }
            if !naming && (model.is_empty() || model.shown().is_empty()) {
                let (title, body) = if model.is_empty() {
                    (
                        "No workspaces yet",
                        "Arrange your windows, then press S to save the layout: it opens again in one key.",
                    )
                } else {
                    (
                        "No workspace matches",
                        "Try another name, or press Esc to clear the filter.",
                    )
                };
                let top = header + 24.0 * s;
                self.title_centered(title, rect(24.0 * s, top, w - 24.0 * s, top + 24.0 * s), theme);
                self.text(
                    body,
                    &self.body,
                    rect(32.0 * s, top + 28.0 * s, w - 32.0 * s, h - footer - 16.0 * s),
                    theme.muted,
                );
            }
        })
    }

    fn title_centered(&self, s: &str, r: D2D_RECT_F, theme: &Theme) {
        // SAFETY: changes the alignment of the painter's own format for one draw, then back.
        unsafe {
            let _ = self.title.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        }
        self.text(s, &self.title, r, theme.text);
        // SAFETY: as above.
        unsafe {
            let _ = self.title.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
        }
    }

    /// Width of `s` in `f`, in pixels.
    fn measure(&self, s: &str, f: &IDWriteTextFormat) -> f32 {
        let text = wide(s);
        // SAFETY: a temporary layout to read metrics from; out struct is a local.
        unsafe {
            self.dwrite
                .CreateTextLayout(&text, f, 10_000.0, 1_000.0)
                .and_then(|layout| {
                    let mut m = windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                    layout
                        .GetMetrics(&mut m)
                        .map(|()| m.widthIncludingTrailingWhitespace)
                })
                .unwrap_or(0.0)
        }
    }

    /// Key caps, right-aligned so the last one ends at `right`, centred on `middle`.
    fn keys<'a>(
        &self,
        keys: impl DoubleEndedIterator<Item = &'a str>,
        right: f32,
        middle: f32,
        theme: &Theme,
        dim: bool,
    ) {
        let s = self.scale();
        let mut x = right;
        for k in keys.rev() {
            let w = self.measure(k, &self.small) + 12.0 * s;
            let r = rect(x - w, middle - 10.0 * s, x, middle + 10.0 * s);
            self.stroke_round(r, 6.0 * s, theme.line_strong(), s);
            self.text(k, &self.small, r, if dim { theme.muted } else { theme.text });
            x -= w + 6.0 * s;
        }
    }

    /// One row of the list: its layout in small, its name (marked when it is open) and its
    /// apps, and the key that opens it.
    pub fn row(&self, hdc: HDC, area: RECT, theme: &Theme, row: &Row<'_>) -> windows::core::Result<()> {
        let Row {
            entry,
            current,
            selected,
            key,
        } = *row;
        let s = self.scale();
        let w = (area.right - area.left) as f32;
        let h = (area.bottom - area.top) as f32;
        self.on(hdc, area, || {
            // SAFETY: inside BeginDraw on the bound target.
            unsafe { self.target.Clear(Some(&theme.panel.d2d())) };
            let text = if selected && theme.high_contrast {
                theme.selection_text
            } else {
                theme.text
            };
            if selected {
                self.fill_round(
                    rect(4.0 * s, 2.0 * s, w - 4.0 * s, h - 2.0 * s),
                    10.0 * s,
                    theme.selection,
                );
                self.fill_round(
                    rect(4.0 * s, 12.0 * s, 7.0 * s, h - 12.0 * s),
                    1.5 * s,
                    theme.accent,
                );
            }
            // The preview: the first monitor in miniature, zones in their colours.
            let th = 40.0 * s;
            let tw = (th * f(entry.preview.aspect)).clamp(48.0 * s, 72.0 * s);
            let tx = 16.0 * s;
            let ty = (h - th) / 2.0;
            let thumb = rect(tx, ty, tx + tw, ty + th);
            self.fill_round(thumb, 6.0 * s, theme.thumb);
            for (area, color) in &entry.preview.zones {
                let z = rect(
                    tx + f(area.left()) * tw + 1.5 * s,
                    ty + f(area.top()) * th + 1.5 * s,
                    tx + f(area.right()) * tw - 1.5 * s,
                    ty + f(area.bottom()) * th - 1.5 * s,
                );
                if theme.high_contrast {
                    self.stroke_round(z, 2.0 * s, text, s);
                } else {
                    self.fill_round(z, 2.0 * s, Rgba::of(color.fill()));
                }
            }
            self.stroke_round(thumb, 6.0 * s, theme.line, s);
            // Name, "On screen" after it when it is the one open, and its apps.
            let x = tx + tw + 16.0 * s;
            let key_room = if key.is_some() { 56.0 * s } else { 16.0 * s };
            let open_room = if current {
                self.measure(OPEN, &self.small) + 14.0 * s
            } else {
                0.0
            };
            let name_right =
                (x + self.measure(&entry.name, &self.title) + 2.0 * s).min(w - key_room - open_room);
            self.text(
                &entry.name,
                &self.title,
                rect(x, h / 2.0 - 20.0 * s, name_right, h / 2.0 + 2.0 * s),
                text,
            );
            if current {
                let accent = if selected && theme.high_contrast {
                    theme.selection_text
                } else {
                    theme.accent
                };
                self.text(
                    OPEN,
                    &self.small,
                    rect(
                        name_right + 8.0 * s,
                        h / 2.0 - 20.0 * s,
                        name_right + open_room,
                        h / 2.0 + 2.0 * s,
                    ),
                    accent,
                );
            }
            self.text(
                &entry.apps_line(),
                &self.subtitle,
                rect(x, h / 2.0 + 2.0 * s, w - key_room, h / 2.0 + 20.0 * s),
                if selected && theme.high_contrast {
                    theme.selection_text
                } else {
                    theme.muted
                },
            );
            if let Some(k) = key {
                self.keys(std::iter::once(k), w - 16.0 * s, h / 2.0, theme, true);
            }
        })
    }

    /// The name field shown while saving: the name (highlighted while it is the untouched
    /// suggestion), a caret, and what saving will do.
    pub fn naming_row(
        &self,
        hdc: HDC,
        area: RECT,
        theme: &Theme,
        name: &str,
        selected: bool,
        replaces: Option<&str>,
    ) -> windows::core::Result<()> {
        let s = self.scale();
        let w = (area.right - area.left) as f32;
        let h = (area.bottom - area.top) as f32;
        self.on(hdc, area, || {
            // SAFETY: inside BeginDraw on the bound target.
            unsafe { self.target.Clear(Some(&theme.panel.d2d())) };
            let field = rect(4.0 * s, 2.0 * s, w - 4.0 * s, h - 2.0 * s);
            self.fill_round(field, 10.0 * s, theme.selection);
            self.stroke_round(field, 10.0 * s, theme.accent, 1.5 * s);
            let x = 16.0 * s;
            let top = rect(x, h / 2.0 - 20.0 * s, w - 16.0 * s, h / 2.0 + 2.0 * s);
            let text_w = self.measure(name, &self.title);
            if selected && !name.is_empty() {
                let mut highlight = theme.accent;
                highlight.3 = if theme.high_contrast { 1.0 } else { 0.28 };
                self.fill_round(
                    rect(
                        x - 2.0 * s,
                        top.top + 1.0 * s,
                        x + text_w + 2.0 * s,
                        top.bottom - 1.0 * s,
                    ),
                    3.0 * s,
                    highlight,
                );
            }
            let ink = if selected && theme.high_contrast {
                theme.selection_text
            } else {
                theme.text
            };
            self.text(name, &self.title, top, ink);
            if !selected {
                let caret = x + text_w + 1.0 * s;
                self.fill(
                    rect(caret, top.top + 2.0 * s, caret + 1.5 * s, top.bottom - 2.0 * s),
                    theme.accent,
                );
            }
            let note = match replaces {
                Some(existing) => {
                    format!("Updates \u{201c}{existing}\u{201d} with the windows on screen now")
                }
                None => "A new workspace with the windows on screen now".to_owned(),
            };
            self.text(
                &note,
                &self.subtitle,
                rect(x, h / 2.0 + 2.0 * s, w - 16.0 * s, h / 2.0 + 20.0 * s),
                theme.muted,
            );
        })
    }

    /// Recreates the render target and brush after Direct2D reports a lost device (a driver
    /// update or a GPU reset), so the next paint works again.
    pub fn recreate(&mut self) -> windows::core::Result<()> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: render-target and brush creation from the painter's own factory.
        unsafe {
            self.target = self.factory.CreateDCRenderTarget(&props)?;
            self.target
                .SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            self.brush = self.target.CreateSolidColorBrush(&Theme::DARK.text.d2d(), None)?;
        }
        Ok(())
    }
}

impl Theme {
    fn line_strong(&self) -> Rgba {
        Rgba(
            self.line.0,
            self.line.1,
            self.line.2,
            (self.line.3 * 1.5).min(1.0),
        )
    }
}
