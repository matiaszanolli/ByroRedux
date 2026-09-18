//! CPU rasterizer: draws the layout's [`DrawItem`] list into an RGBA8
//! frame for the engine's UI-overlay upload.
//!
//! Source-over alpha blending, nearest-neighbour sampling (the HUD's
//! authored coordinates land at 1:1 or integer scales; the one heavily
//! scaled element — the 2048-px compass face — reads fine nearest at
//! these sizes, matching the period hardware's unfiltered menu path).

use crate::font::Font;
use crate::layout::{tint_u8, DrawItem, Rect};
use crate::tex::Rgba8;

/// A top-down RGBA8 frame, non-premultiplied alpha.
#[derive(Debug, Clone)]
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn clear(&mut self, rgba: [u8; 4]) {
        for px in self.pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&rgba);
        }
    }

    /// Blend one pixel (source-over). `src_a` is the source alpha after
    /// tint/alpha modulation.
    #[inline]
    fn blend(&mut self, x: i64, y: i64, rgb: [u8; 3], src_a: u8) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 || src_a == 0 {
            return;
        }
        let o = (y as usize * self.width as usize + x as usize) * 4;
        let dst_a = self.pixels[o + 3] as u32;
        let out_a = src_a as u32 + dst_a * (255 - src_a as u32) / 255;
        if out_a == 0 {
            return;
        }
        for c in 0..3 {
            let src_c = rgb[c] as u32 * src_a as u32;
            let dst_c = self.pixels[o + c] as u32 * dst_a;
            self.pixels[o + c] = ((src_c + dst_c * (255 - src_a as u32) / 255) / out_a)
                .clamp(0, 255) as u8;
        }
        self.pixels[o + 3] = out_a.clamp(0, 255) as u8;
    }

    /// Blit a texture sub-rect to a destination rect, scaled.
    ///
    /// `crop` is the authored `cropx`/`cropy` in *display* pixels;
    /// per the wiki it applies after zoom, so the texel-space source
    /// offset is `crop * (src_extent / dst_extent)` — with the default
    /// stretch zoom (`-1`) that is `crop * (tex_size / rect_size)`,
    /// which reproduces the compass icon atlas math (32-px cells at
    /// 32-px tiles → 1:1).
    pub fn blit(
        &mut self,
        tex: &Rgba8,
        mut dst: Rect,
        crop: (f32, f32),
        tint: [f32; 3],
        alpha: f32,
        clip: Option<Rect>,
    ) {
        // Zero extents mean "natural texture size" (TiImage default when
        // the XML authored no width/height).
        if dst.w <= 0.0 {
            dst.w = tex.width as f32;
        }
        if dst.h <= 0.0 {
            dst.h = tex.height as f32;
        }
        if dst.w <= 0.0 || dst.h <= 0.0 || alpha <= 0.0 {
            return;
        }
        let clip = clip.unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: self.width as f32,
            h: self.height as f32,
        });
        let [tr, tg, tb] = tint_u8(tint);
        let a_mod = alpha.clamp(0.0, 255.0) / 255.0;

        let x0 = (dst.x.max(clip.x)).floor() as i64;
        let y0 = (dst.y.max(clip.y)).floor() as i64;
        let x1 = ((dst.x + dst.w).min(clip.x + clip.w)).ceil() as i64;
        let y1 = ((dst.y + dst.h).min(clip.y + clip.h)).ceil() as i64;

        // Source rect in texels: full texture minus the display-pixel
        // crop converted through the dst→src scale.
        let sx = tex.width as f32 / dst.w;
        let sy = tex.height as f32 / dst.h;
        let src_x0 = (crop.0 * sx).clamp(0.0, tex.width as f32 - 1.0);
        let src_y0 = (crop.1 * sy).clamp(0.0, tex.height as f32 - 1.0);
        let src_w = (tex.width as f32 - src_x0).max(0.01);
        let src_h = (tex.height as f32 - src_y0).max(0.01);

        for py in y0..y1 {
            let v = (py as f32 - dst.y) / dst.h;
            let ty = (src_y0 + v * src_h) as i64;
            for px in x0..x1 {
                let u = (px as f32 - dst.x) / dst.w;
                let tx = (src_x0 + u * src_w) as i64;
                let p = tex.pixel(tx, ty);
                // Tint: menu art carries its own colour; authored tint
                // replaces RGB only when it is not the default white.
                let rgb = if tr == 255 && tg == 255 && tb == 255 {
                    [p[0], p[1], p[2]]
                } else {
                    [
                        ((p[0] as u16 * tr as u16) / 255) as u8,
                        ((p[1] as u16 * tg as u16) / 255) as u8,
                        ((p[2] as u16 * tb as u16) / 255) as u8,
                    ]
                };
                let src_a = (p[3] as f32 * a_mod) as u8;
                self.blend(px, py, rgb, src_a);
            }
        }
    }

    pub fn fill(&mut self, rect: Rect, tint: [f32; 3], alpha: f32, clip: Option<Rect>) {
        self.blit(white_tex(), rect, (0.0, 0.0), tint, alpha, clip);
    }

    /// Draw one line of text with a bitmap font. Returns the line's
    /// advance width.
    ///
    /// `justify` shifts each *line* relative to `x`: left aligns the
    /// line's leading edge at `x`, right aligns its trailing edge,
    /// center centres it (CS Wiki `justify` trait).
    pub fn text_line(
        &mut self,
        font: &Font,
        line: &str,
        x: f32,
        y: f32,
        justify: u8,
        tint: [f32; 3],
        alpha: f32,
        clip: Option<Rect>,
    ) -> f32 {
        let width = font.measure_width(line);
        let x = match justify {
            1 => x - width / 2.0,
            2 => x - width,
            _ => x,
        };
        let mut pen = x;
        for &b in line.as_bytes() {
            let g = font.glyph(b);
            if g.inked {
                // Glyphs draw at 1:1 texel scale — the authored font
                // sizes are pixel metrics.
                let dst = Rect {
                    x: pen,
                    y: y + g.y_offset,
                    w: g.width,
                    h: g.height,
                };
                let sub = texel_rect(font, g);
                blit_sub(self, &font.atlas, sub, dst, tint, alpha, clip);
            }
            pen += g.advance;
        }
        width
    }
}

/// Blit an explicit texel sub-rect (glyph path — source rect comes from
/// the font's UV metrics, not a crop).
fn blit_sub(
    fb: &mut Framebuffer,
    tex: &Rgba8,
    src: Rect,
    dst: Rect,
    tint: [f32; 3],
    alpha: f32,
    clip: Option<Rect>,
) {
    if dst.w <= 0.0 || dst.h <= 0.0 || src.w <= 0.0 || src.h <= 0.0 || alpha <= 0.0 {
        return;
    }
    let clip = clip.unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: fb.width as f32,
        h: fb.height as f32,
    });
    let [tr, tg, tb] = tint_u8(tint);
    let a_mod = alpha.clamp(0.0, 255.0) / 255.0;

    let x0 = (dst.x.max(clip.x)).floor() as i64;
    let y0 = (dst.y.max(clip.y)).floor() as i64;
    let x1 = ((dst.x + dst.w).min(clip.x + clip.w)).ceil() as i64;
    let y1 = ((dst.y + dst.h).min(clip.y + clip.h)).ceil() as i64;

    for py in y0..y1 {
        let v = (py as f32 - dst.y) / dst.h;
        let ty = (src.y + v * src.h) as i64;
        for px in x0..x1 {
            let u = (px as f32 - dst.x) / dst.w;
            let tx = (src.x + u * src.w) as i64;
            let p = tex.pixel(tx, ty);
            let rgb = [
                ((p[0] as u16 * tr as u16) / 255) as u8,
                ((p[1] as u16 * tg as u16) / 255) as u8,
                ((p[2] as u16 * tb as u16) / 255) as u8,
            ];
            let src_a = (p[3] as f32 * a_mod) as u8;
            fb.blend(px, py, rgb, src_a);
        }
    }
}

/// Convert a glyph's UV metrics to an atlas texel rect.
fn texel_rect(font: &Font, g: &crate::font::Glyph) -> Rect {
    let w = font.atlas.width as f32;
    let h = font.atlas.height as f32;
    Rect {
        x: g.u0 * w,
        y: g.v0 * h,
        w: (g.u1 - g.u0).max(0.0) * w,
        h: (g.v1 - g.v0).max(0.0) * h,
    }
}

/// 1×1 white source for fill rects.
static WHITE_TEX: once_white::White = once_white::White;

mod once_white {
    use crate::tex::Rgba8;
    pub struct White;
    impl White {
        pub fn get(&self) -> &Rgba8 {
            static INSTANCE: std::sync::OnceLock<Rgba8> = std::sync::OnceLock::new();
            INSTANCE.get_or_init(|| Rgba8 {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            })
        }
    }
}

/// Draw-list dispatch used by [`crate::menu::MenuRenderer`]: resolves
/// image paths is *not* this module's job — the caller maps
/// [`DrawItem::Image`] filenames to decoded textures first.
impl Framebuffer {
    pub fn draw_text_item(
        &mut self,
        item: &DrawItem,
        font: &Font,
    ) {
        let DrawItem::Text {
            x,
            y,
            string,
            justify,
            tint,
            alpha,
            wrap_width,
            wrap_lines,
            clip,
            ..
        } = item
        else {
            return;
        };
        let lines = wrap(string, font, *wrap_width, *wrap_lines);
        let mut ly = *y;
        for line in lines {
            self.text_line(font, &line, *x, ly, *justify, *tint, *alpha, *clip);
            ly += font.line_height();
        }
    }
}

/// Word-wrap `text` to `wrap_width` texels (0 = no wrap), capped at
/// `wrap_lines` (0 = unlimited). Wraps on spaces; a single word longer
/// than the width is not broken (vanilla HUD strings are short).
pub fn wrap(text: &str, font: &Font, wrap_width: f32, wrap_lines: u32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if wrap_width <= 0.0 {
        lines.push(text.to_string());
    } else {
        let mut current = String::new();
        for word in text.split(' ') {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if font.measure_width(&candidate) > wrap_width && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        lines.push(current);
    }
    if wrap_lines > 0 && lines.len() > wrap_lines as usize {
        lines.truncate(wrap_lines as usize);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// The shared 1×1 white texture used by [`Framebuffer::fill`].
pub fn white_tex() -> &'static Rgba8 {
    WHITE_TEX.get()
}
