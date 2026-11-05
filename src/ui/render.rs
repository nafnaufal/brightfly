use std::ffi::c_void;

use windows::core::w;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_DRAW_TEXT_OPTIONS_NONE,
    ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD,
};

pub struct Row {
    pub pct: u8,
    pub enabled: bool,
    pub name: String,
}

pub const CARD_WIDTH_DIP: f32 = 192.0;
pub const ROW_HEIGHT_DIP: f32 = 48.0;
pub const CARD_RADIUS_DIP: f32 = 8.0;

const PAD_X_DIP: f32 = 12.0;
const ICON_SIZE_DIP: f32 = 20.0;
const GAP_DIP: f32 = 10.0;
const NUMBER_WIDTH_DIP: f32 = 28.0;
const TRACK_HEIGHT_DIP: f32 = 4.0;
const THUMB_RADIUS_DIP: f32 = 7.0;
const LABEL_HEIGHT_DIP: f32 = 16.0;
const NAME_FONT_DIP: f32 = 12.0;

pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub pad: f32,
    pub row_height: f32,
    pub label_height: f32,
    pub track_left: f32,
    pub track_right: f32,
    pub track_height: f32,
}

pub fn layout(rows: usize, scale: f32) -> Layout {
    let width = CARD_WIDTH_DIP * scale;
    let pad = PAD_X_DIP * scale;
    let label_height = if rows > 1 {
        LABEL_HEIGHT_DIP * scale
    } else {
        0.0
    };
    Layout {
        width,
        height: (ROW_HEIGHT_DIP * scale + label_height) * rows as f32,
        pad,
        row_height: ROW_HEIGHT_DIP * scale + label_height,
        label_height,
        track_left: pad + (ICON_SIZE_DIP + GAP_DIP) * scale,
        track_right: width - pad - (NUMBER_WIDTH_DIP + GAP_DIP) * scale,
        track_height: TRACK_HEIGHT_DIP * scale,
    }
}

impl Layout {
    pub fn row_top(&self, index: usize) -> f32 {
        self.row_height * index as f32
    }

    pub fn slider_top(&self, index: usize) -> f32 {
        self.row_top(index) + self.label_height
    }

    pub fn row_center(&self, index: usize) -> f32 {
        self.slider_top(index) + (self.row_height - self.label_height) / 2.0
    }

    pub fn thumb_x(&self, pct: u8) -> f32 {
        self.track_left + (self.track_right - self.track_left) * (pct.min(100) as f32 / 100.0)
    }
}

const ICON_BRIGHTNESS: &str = "\u{E706}";

pub struct Theme {
    pub dark: bool,
    pub accent: [f32; 3],
}

impl Theme {
    pub fn system() -> Self {
        let dark = system_uses_dark();
        Self {
            dark,
            accent: system_accent(dark),
        }
    }

    fn card(&self) -> D2D1_COLOR_F {
        if self.dark {
            color(0.173, 0.173, 0.173, 1.0)
        } else {
            color(0.957, 0.957, 0.957, 1.0)
        }
    }

    fn text(&self) -> D2D1_COLOR_F {
        if self.dark {
            color(1.0, 1.0, 1.0, 1.0)
        } else {
            color(0.10, 0.10, 0.10, 1.0)
        }
    }

    fn secondary_text(&self) -> D2D1_COLOR_F {
        if self.dark {
            color(1.0, 1.0, 1.0, 0.72)
        } else {
            color(0.0, 0.0, 0.0, 0.60)
        }
    }

    fn track(&self) -> D2D1_COLOR_F {
        if self.dark {
            color(1.0, 1.0, 1.0, 0.541)
        } else {
            color(0.0, 0.0, 0.0, 0.447)
        }
    }

    fn fill(&self) -> D2D1_COLOR_F {
        color(self.accent[0], self.accent[1], self.accent[2], 1.0)
    }

    fn disabled(&self) -> D2D1_COLOR_F {
        if self.dark {
            color(1.0, 1.0, 1.0, 0.35)
        } else {
            color(0.0, 0.0, 0.0, 0.35)
        }
    }
}

pub fn system_uses_dark() -> bool {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok() && value == 0
}

fn system_accent(dark: bool) -> [f32; 3] {
    let mut palette = [0u8; 32];
    let mut size = palette.len() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent"),
            w!("AccentPalette"),
            RRF_RT_REG_BINARY,
            None,
            Some(palette.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };

    if status.is_ok() && size >= 32 {
        let index = if dark { 1 } else { 4 };
        let base = index * 4;
        return [
            palette[base] as f32 / 255.0,
            palette[base + 1] as f32 / 255.0,
            palette[base + 2] as f32 / 255.0,
        ];
    }

    if dark {
        [0.741, 0.729, 0.906]
    } else {
        [0.271, 0.278, 0.714]
    }
}

fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

pub struct Renderer {
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    dwrite: IDWriteFactory,
    text_format: Option<(f32, IDWriteTextFormat, IDWriteTextFormat, IDWriteTextFormat)>,
    size: (u32, u32),
}

impl Renderer {
    pub fn new() -> Option<Self> {
        macro_rules! step {
            ($expr:expr, $name:literal) => {
                match $expr {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("renderer: {} gagal: {:?}", $name, error);
                        return None;
                    }
                }
            };
        }

        unsafe {
            let factory: ID2D1Factory =
                step!(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None), "D2D1CreateFactory");

            let properties = D2D1_RENDER_TARGET_PROPERTIES {
                // DC render targets are always software; stating it keeps
                // Direct2D from loading a D3D11 driver for one small card.
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let target = step!(factory.CreateDCRenderTarget(&properties), "CreateDCRenderTarget");
            let brush = step!(
                target.CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 1.0), None),
                "CreateSolidColorBrush"
            );
            let dwrite: IDWriteFactory =
                step!(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED), "DWriteCreateFactory");

            Some(Self {
                target,
                brush,
                dwrite,
                text_format: None,
                size: (0, 0),
            })
        }
    }

    pub fn measure(rows: usize, scale: f32) -> (i32, i32) {
        let l = layout(rows, scale);
        (l.width.round() as i32, l.height.round() as i32)
    }

    fn ensure_formats(&mut self, scale: f32) {
        if let Some((current, _, _, _)) = &self.text_format {
            if (*current - scale).abs() < 0.01 {
                return;
            }
        }
        unsafe {
            let family = w!("Segoe UI Variable Text");
            let locale = w!("en-us");

            let Ok(value) = self.dwrite.CreateTextFormat(
                family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                15.0 * scale,
                locale,
            ) else {
                return;
            };
            let _ = value.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
            let _ = value.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = value.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);

            let Ok(icon) = self.dwrite.CreateTextFormat(
                w!("Segoe Fluent Icons"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                16.0 * scale,
                locale,
            ) else {
                return;
            };
            let _ = icon.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = icon.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

            let Ok(name) = self.dwrite.CreateTextFormat(
                family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                NAME_FONT_DIP * scale,
                locale,
            ) else {
                return;
            };
            let _ = name.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
            let _ = name.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = name.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);

            self.text_format = Some((scale, value, icon, name));
        }
    }

    pub fn draw(
        &mut self,
        hdc: HDC,
        rows: &[Row],
        hover: Option<usize>,
        dragging: Option<usize>,
        theme: &Theme,
        scale: f32,
    ) {
        let (width, height) = Self::measure(rows.len(), scale);
        if width <= 0 || height <= 0 {
            return;
        }
        self.size = (width as u32, height as u32);
        self.ensure_formats(scale);

        let bounds = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };

        unsafe {
            if let Err(error) = self.target.BindDC(hdc, &bounds) {
                eprintln!("renderer: BindDC gagal: {error:?}");
                return;
            }
            self.target.BeginDraw();

            let background = theme.card();
            let _ = self.target.Clear(Some(&background));

            self.draw_card(rows, hover, dragging, theme, scale);

            if let Err(error) = self.target.EndDraw(None, None) {
                eprintln!("renderer: EndDraw gagal: {error:?}");
            }
        }
    }

    fn draw_card(
        &mut self,
        rows: &[Row],
        hover: Option<usize>,
        dragging: Option<usize>,
        theme: &Theme,
        scale: f32,
    ) {
        unsafe {
            let l = layout(rows.len(), scale);
            let pad = l.pad;
            let row_height = l.row_height;
            let label_height = l.label_height;
            let band_height = row_height - label_height;
            let icon_size = ICON_SIZE_DIP * scale;
            let text_width = NUMBER_WIDTH_DIP * scale;
            let track_height = l.track_height;
            let width = self.size.0 as f32;

            let track_left = l.track_left;
            let track_right = l.track_right;

            for (index, row) in rows.iter().enumerate() {
                let top = l.row_top(index);
                let band_top = top + label_height;
                let center_y = l.row_center(index);

                if label_height > 0.0 {
                    if let Some((_, _, _, name_format)) = &self.text_format {
                        self.brush.SetColor(&theme.secondary_text());
                        let text: Vec<u16> = row.name.encode_utf16().collect();
                        let rect = D2D_RECT_F {
                            left: pad,
                            top,
                            right: width - pad,
                            bottom: top + label_height,
                        };
                        self.target.DrawText(
                            &text,
                            name_format,
                            &rect,
                            &self.brush,
                            D2D1_DRAW_TEXT_OPTIONS_NONE,
                            DWRITE_MEASURING_MODE_NATURAL,
                        );
                    }
                }

                if let Some((_, _, icon, _)) = &self.text_format {
                    self.brush.SetColor(&theme.text());
                    let glyph: Vec<u16> = ICON_BRIGHTNESS.encode_utf16().collect();
                    let rect = D2D_RECT_F {
                        left: pad,
                        top: band_top,
                        right: pad + icon_size,
                        bottom: band_top + band_height,
                    };
                    self.target.DrawText(
                        &glyph,
                        icon,
                        &rect,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }

                let track = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: track_left,
                        top: center_y - track_height / 2.0,
                        right: track_right,
                        bottom: center_y + track_height / 2.0,
                    },
                    radiusX: track_height / 2.0,
                    radiusY: track_height / 2.0,
                };
                let base = if row.enabled { theme.track() } else { theme.disabled() };
                self.brush.SetColor(&base);
                self.target.FillRoundedRectangle(&track, &self.brush);

                let ratio = row.pct as f32 / 100.0;
                let filled_right = track_left + (track_right - track_left) * ratio;
                if row.enabled && filled_right > track_left {
                    let filled = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: track_left,
                            top: track.rect.top,
                            right: filled_right,
                            bottom: track.rect.bottom,
                        },
                        radiusX: track_height / 2.0,
                        radiusY: track_height / 2.0,
                    };
                    self.brush.SetColor(&theme.fill());
                    self.target.FillRoundedRectangle(&filled, &self.brush);
                }

                let active = hover == Some(index) || dragging == Some(index);
                if active {
                    let radius = THUMB_RADIUS_DIP * scale;
                    let thumb = D2D1_ELLIPSE {
                        point: windows_numerics::Vector2 {
                            X: filled_right.max(track_left).min(track_right),
                            Y: center_y,
                        },
                        radiusX: radius,
                        radiusY: radius,
                    };
                    self.brush.SetColor(if row.enabled {
                        &theme.text()
                    } else {
                        &theme.disabled()
                    });
                    self.target.FillEllipse(&thumb, &self.brush);
                }

                if let Some((_, format, _, _)) = &self.text_format {
                    self.brush.SetColor(&theme.text());
                    let text: Vec<u16> = format!("{}", row.pct).encode_utf16().collect();
                    let rect = D2D_RECT_F {
                        left: width - pad - text_width,
                        top: band_top,
                        right: width - pad,
                        bottom: band_top + band_height,
                    };
                    self.target.DrawText(
                        &text,
                        format,
                        &rect,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_row_has_no_label_band() {
        let l = layout(1, 1.0);
        assert_eq!(l.height, 48.0);
        assert_eq!(l.row_height, 48.0);
        assert_eq!(l.label_height, 0.0);
        assert_eq!(l.row_center(0), 24.0);
    }

    #[test]
    fn multiple_rows_reserve_label_band() {
        let l = layout(2, 1.0);
        assert_eq!(l.label_height, 16.0);
        assert_eq!(l.row_height, 64.0);
        assert_eq!(l.height, 128.0);
        assert_eq!(l.slider_top(0), 16.0);
        assert_eq!(l.row_center(0), 40.0);
        assert_eq!(l.row_center(1), 104.0);
    }

    #[test]
    fn label_band_scales_with_dpi() {
        let l = layout(2, 1.5);
        assert_eq!(l.width, 288.0);
        assert_eq!(l.label_height, 24.0);
        assert_eq!(l.row_height, 96.0);
        assert_eq!(l.height, 192.0);
    }
}