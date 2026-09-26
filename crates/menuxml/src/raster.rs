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

#[cfg(test)]
thread_local! {
    /// Texels `blend_texel` has been asked to blend on this thread — the
    /// probe #4715's regression tests read to pin the raster loop bound.
    static TEXEL_BLENDS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// A top-down RGBA8 frame, non-premultiplied alpha.
#[derive(Debug, Clone)]
pub struct Framebuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Shared trailing style of every raster draw (#4570): authored tint,
/// alpha fade, and the optional clip window. Groups the three arguments
/// that always travel together so the raster entry points stay under the
/// arg-count lint while mirroring the XML trait grouping.
pub struct BlitStyle {
    pub tint: [f32; 3],
    pub alpha: f32,
    pub clip: Option<Rect>,
}

/// Whether every value is a finite number.
fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|v| v.is_finite())
}

/// Whether a rect is fully finite.
fn rect_finite(r: Rect) -> bool {
    all_finite(&[r.x, r.y, r.w, r.h])
}

/// Whether an optional clip window is absent or fully finite.
fn clip_finite(clip: Option<Rect>) -> bool {
    clip.is_none_or(rect_finite)
}

/// The pixels of one axis a draw covers, `lo..hi` in framebuffer space,
/// clamped to `0..limit` (#4715).
///
/// Menu XML is untrusted, and the loop bounds used to come straight from the
/// authored tile rect: a `16000 x 16000` extent ran 256M iterations per HUD
/// render, and the float→int cast saturates, so an extent past the `i64`
/// range was a loop to `i64::MAX`. The framebuffer is the outermost clip, so
/// nothing outside it can be drawn and the loop never needs to leave it.
/// Clamping in `f32` first keeps the cast in range; a `NaN` bound collapses
/// to the near edge (`f32::max`/`min` return the non-`NaN` operand), and
/// callers reject non-finite geometry before it gets here.
fn axis_span(lo: f32, hi: f32, limit: u32) -> std::ops::Range<i64> {
    let limit = limit as f32;
    let lo = lo.floor().max(0.0).min(limit) as i64;
    let hi = hi.ceil().max(0.0).min(limit) as i64;
    lo..hi
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
        for (c, &src_c) in rgb.iter().enumerate() {
            let src_c = src_c as u32 * src_a as u32;
            let dst_c = self.pixels[o + c] as u32 * dst_a;
            self.pixels[o + c] = ((src_c + dst_c * (255 - src_a as u32) / 255) / out_a)
                .clamp(0, 255) as u8;
        }
        self.pixels[o + 3] = out_a.clamp(0, 255) as u8;
    }

    /// Resolve the shared tint/alpha pair, or `None` when the whole draw
    /// fades to nothing (#4570 — one home for the style math that `blit`,
    /// `text_line` and `blit_sub` each used to repeat).
    fn resolve_style(tint: [f32; 3], alpha: f32) -> Option<([u8; 3], f32)> {
        if alpha <= 0.0 {
            return None;
        }
        Some((tint_u8(tint), alpha.clamp(0.0, 255.0) / 255.0))
    }

    /// Tint + fade one texel and blend it — the per-pixel tail `blit` and
    /// `blit_sub` both carried byte-identical copies of (#4570). The white
    /// fast path replaces RGB only when the tint is not default-white, so
    /// `blit_sub`'s glyph path takes it too (multiply-by-255/255 is
    /// identical to the raw pass-through).
    fn blend_texel(&mut self, px: i64, py: i64, p: [u8; 4], tint: [u8; 3], a_mod: f32) {
        #[cfg(test)]
        TEXEL_BLENDS.with(|n| n.set(n.get() + 1));
        let [tr, tg, tb] = tint;
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

    /// Blit a texture into a tile rect.
    ///
    /// Zoom semantics per the CS Wiki (`Oblivion XML/Traits`):
    ///
    /// * `zoom < 0` (`&scale;`) — the texture stretches (non-uniformly) to
    ///   fill the tile rect. The only mode where the rect scales the art.
    /// * `zoom == 0` (unauthored) or `100` — the texture draws at natural
    ///   size, top-left aligned at the tile origin, **clipped to the tile
    ///   rect**. This is the mode the HUD ribbons depend on: their art is
    ///   power-of-2 padded (256 px wide, ~164 px of ink) and the authored
    ///   width is a *fill fraction* of the ink width, not a stretch target —
    ///   stretching squeezed the transparent pad into the bar and rendered a
    ///   constant ~63%-of-width bar at every health value.
    /// * `zoom > 0` — natural size scaled by `zoom / 100`, clipped the same
    ///   way.
    ///
    /// `crop` is the authored `cropx`/`cropy` in display pixels; per the
    /// wiki it applies after zoom, so the texel-space source offset is
    /// `crop * (tex_extent / draw_extent)`.
    ///
    /// `tiled` implements the `tile` trait (FO3-era): repeat the texture
    /// at 1:1 texel scale across the tile rect (the HUD's tick-mark
    /// meters are a repeated 8×20 tick; the draw width is the fill
    /// fraction). `crop` becomes a direct texel-space scroll offset
    /// wrapping at the texture edges — the compass strip scrolls by
    /// overriding `cropx` with no visible seam. Zoom is ignored in tile
    /// mode (the corpus never combines them).
    pub fn blit(
        &mut self,
        tex: &Rgba8,
        mut dst: Rect,
        crop: (f32, f32),
        zoom: f32,
        tiled: bool,
        style: BlitStyle,
    ) {
        let BlitStyle { tint, alpha, clip } = style;
        // Authored values are untrusted (#4715): non-finite geometry draws
        // nothing rather than reaching the loop bounds as `i64::MAX`.
        if !(rect_finite(dst) && all_finite(&[crop.0, crop.1, zoom]) && clip_finite(clip)) {
            return;
        }
        // Zero extents mean "natural texture size" (TiImage default when
        // the XML authored no width/height) — the tile rect, which clips
        // the drawn image, then equals the full draw.
        if dst.w <= 0.0 {
            dst.w = tex.width as f32;
        }
        if dst.h <= 0.0 {
            dst.h = tex.height as f32;
        }
        if dst.w <= 0.0 || dst.h <= 0.0 {
            return;
        }
        let Some(([tr, tg, tb], a_mod)) = Self::resolve_style(tint, alpha) else {
            return;
        };
        // Draw extent: stretch and tile fill the tile; natural/zoom draws
        // the (scaled) texture, which the tile rect then clips.
        let (draw_w, draw_h) = if tiled || zoom < 0.0 {
            (dst.w, dst.h)
        } else {
            let scale = if zoom <= 0.0 { 1.0 } else { zoom / 100.0 };
            (tex.width as f32 * scale, tex.height as f32 * scale)
        };
        // Clip chain: outer clip window ∩ the tile's own rect.
        let tile = Rect {
            x: dst.x,
            y: dst.y,
            w: dst.w,
            h: dst.h,
        };
        let clip = match clip {
            Some(c) => Rect {
                x: c.x.max(tile.x),
                y: c.y.max(tile.y),
                w: (c.x + c.w).min(tile.x + tile.w) - c.x.max(tile.x),
                h: (c.y + c.h).min(tile.y + tile.h) - c.y.max(tile.y),
            },
            None => tile,
        };
        if clip.w <= 0.0 || clip.h <= 0.0 {
            return;
        }

        // The draw's pixel span, clamped to the framebuffer (#4715). `px`/`py`
        // stay framebuffer coordinates, so the texel mapping below is unchanged.
        let cols = axis_span(
            dst.x.max(clip.x),
            (dst.x + draw_w).min(clip.x + clip.w),
            self.width,
        );
        let rows = axis_span(
            dst.y.max(clip.y),
            (dst.y + draw_h).min(clip.y + clip.h),
            self.height,
        );

        // Source rect in texels: full texture minus the display-pixel
        // crop converted through the draw→src scale.
        let sx = tex.width as f32 / draw_w;
        let sy = tex.height as f32 / draw_h;
        let src_x0 = (crop.0 * sx).clamp(0.0, tex.width as f32 - 1.0);
        let src_y0 = (crop.1 * sy).clamp(0.0, tex.height as f32 - 1.0);
        let src_w = (tex.width as f32 - src_x0).max(0.01);
        let src_h = (tex.height as f32 - src_y0).max(0.01);
        let tw = tex.width as i64;
        let th = tex.height as i64;
        let scroll_x = crop.0 as i64;
        let scroll_y = crop.1 as i64;

        for py in rows {
            let v = (py as f32 - dst.y) / draw_h;
            // Wrapping: a huge finite `crop`/`dst` saturates its `as i64` to
            // `i64::MIN/MAX`, and a plain `+`/`-` on that overflows (a panic
            // in a debug build). A wrapped index is still a valid
            // `rem_euclid` operand.
            let ty = if tiled {
                scroll_y
                    .wrapping_add(py)
                    .wrapping_sub(dst.y as i64)
                    .rem_euclid(th)
            } else {
                (src_y0 + v * src_h) as i64
            };
            for px in cols.clone() {
                let u = (px as f32 - dst.x) / draw_w;
                let tx = if tiled {
                    scroll_x
                        .wrapping_add(px)
                        .wrapping_sub(dst.x as i64)
                        .rem_euclid(tw)
                } else {
                    (src_x0 + u * src_w) as i64
                };
                let p = tex.pixel(tx, ty);
                // Tint: menu art carries its own colour; authored tint
                // replaces RGB only when it is not the default white.
                self.blend_texel(px, py, p, [tr, tg, tb], a_mod);
            }
        }
    }

    pub fn fill(&mut self, rect: Rect, tint: [f32; 3], alpha: f32, clip: Option<Rect>) {
        self.blit(
            white_tex(),
            rect,
            (0.0, 0.0),
            -1.0,
            false,
            BlitStyle { tint, alpha, clip },
        );
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
        style: BlitStyle,
    ) -> f32 {
        let BlitStyle { tint, alpha, clip } = style;
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
                blit_sub(
                    self,
                    &font.atlas,
                    sub,
                    dst,
                    BlitStyle { tint, alpha, clip },
                );
            }
            pen += g.advance;
        }
        width
    }
}

/// Blit an explicit texel sub-rect (glyph path — source rect comes from
/// the font's UV metrics, not a crop).
fn blit_sub(fb: &mut Framebuffer, tex: &Rgba8, src: Rect, dst: Rect, style: BlitStyle) {
    let BlitStyle { tint, alpha, clip } = style;
    // Same untrusted-input rule as `Framebuffer::blit` (#4715): glyph metrics
    // and clip windows can be non-finite or enormous.
    if !(rect_finite(src) && rect_finite(dst) && clip_finite(clip)) {
        return;
    }
    if dst.w <= 0.0 || dst.h <= 0.0 || src.w <= 0.0 || src.h <= 0.0 || alpha <= 0.0 {
        return;
    }
    let clip = clip.unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: fb.width as f32,
        h: fb.height as f32,
    });
    let Some(([tr, tg, tb], a_mod)) = Framebuffer::resolve_style(tint, alpha) else {
        return;
    };

    let cols = axis_span(
        dst.x.max(clip.x),
        (dst.x + dst.w).min(clip.x + clip.w),
        fb.width,
    );
    let rows = axis_span(
        dst.y.max(clip.y),
        (dst.y + dst.h).min(clip.y + clip.h),
        fb.height,
    );

    for py in rows {
        let v = (py as f32 - dst.y) / dst.h;
        let ty = (src.y + v * src.h) as i64;
        for px in cols.clone() {
            let u = (px as f32 - dst.x) / dst.w;
            let tx = (src.x + u * src.w) as i64;
            let p = tex.pixel(tx, ty);
            fb.blend_texel(px, py, p, [tr, tg, tb], a_mod);
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
            self.text_line(
                font,
                &line,
                *x,
                ly,
                *justify,
                BlitStyle {
                    tint: *tint,
                    alpha: *alpha,
                    clip: *clip,
                },
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    const FB_W: u32 = 32;
    const FB_H: u32 = 16;

    fn red_tex() -> Rgba8 {
        let mut tex = Rgba8::new(2, 2);
        for px in tex.pixels.chunks_exact_mut(4) {
            px.copy_from_slice(&[255, 0, 0, 255]);
        }
        tex
    }

    fn style() -> BlitStyle {
        BlitStyle {
            tint: [255.0, 255.0, 255.0],
            alpha: 255.0,
            clip: None,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Run `draw` on a fresh framebuffer and report how many texels the raster
    /// loop handed to `blend_texel`, plus the framebuffer.
    fn blended(draw: impl FnOnce(&mut Framebuffer)) -> (u64, Framebuffer) {
        TEXEL_BLENDS.with(|n| n.set(0));
        let mut fb = Framebuffer::new(FB_W, FB_H);
        draw(&mut fb);
        (TEXEL_BLENDS.with(|n| n.get()), fb)
    }

    const FB_PIXELS: u64 = FB_W as u64 * FB_H as u64;

    fn untouched(fb: &Framebuffer) -> bool {
        fb.pixels.iter().all(|&b| b == 0)
    }

    /// #4715 — the loop bound came from the authored tile rect, never the
    /// framebuffer: a `<width>16000</width><height>16000</height>` stretch
    /// tile ran 256M iterations on every HUD render. The loop must visit at
    /// most the framebuffer's own pixels, and still draw them.
    #[test]
    fn a_huge_finite_extent_visits_at_most_the_framebuffer() {
        let tex = red_tex();
        let (count, fb) = blended(|fb| {
            fb.blit(
                &tex,
                rect(0.0, 0.0, 16000.0, 16000.0),
                (0.0, 0.0),
                -1.0,
                false,
                style(),
            );
        });
        assert_eq!(count, FB_PIXELS, "one blend per framebuffer pixel, no more");
        assert!(fb.pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));

        // A tile that starts far off-screen but spans the framebuffer.
        let (count, _) = blended(|fb| {
            fb.blit(
                &tex,
                rect(-8000.0, -8000.0, 16000.0, 16000.0),
                (0.0, 0.0),
                -1.0,
                false,
                style(),
            );
        });
        assert_eq!(count, FB_PIXELS);

        // Tiled and natural-size-with-zoom take the same loop.
        let (count, _) = blended(|fb| {
            fb.blit(
                &tex,
                rect(0.0, 0.0, 16000.0, 16000.0),
                (0.0, 0.0),
                0.0,
                true,
                style(),
            );
        });
        assert_eq!(count, FB_PIXELS);
        let (count, _) = blended(|fb| {
            fb.blit(
                &tex,
                rect(0.0, 0.0, 16000.0, 16000.0),
                (0.0, 0.0),
                1.0e7,
                false,
                style(),
            );
        });
        assert!(count <= FB_PIXELS, "zoomed natural draw visited {count}");

        // And through a clip window as wide as the tile.
        let (count, _) = blended(|fb| {
            let clip = Some(rect(-5000.0, -5000.0, 20000.0, 20000.0));
            let style = BlitStyle { clip, ..style() };
            fb.blit(
                &tex,
                rect(0.0, 0.0, 16000.0, 16000.0),
                (0.0, 0.0),
                -1.0,
                false,
                style,
            );
        });
        assert_eq!(count, FB_PIXELS);
    }

    /// #4715 — `inf`, `-inf` and `NaN` (`str::parse::<f32>` accepts all three,
    /// and `1e39` parses to `inf`) saturate to `i64::MAX` at the float→int
    /// cast, so a non-finite extent was an effective hang. Non-finite geometry
    /// draws nothing.
    #[test]
    fn non_finite_geometry_draws_nothing() {
        let tex = red_tex();
        let bad = [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            "1e39".parse().unwrap(),
        ];
        for v in bad {
            for field in 0..7 {
                let mut dst = rect(0.0, 0.0, 8.0, 8.0);
                let mut crop = (0.0, 0.0);
                let mut zoom = -1.0;
                match field {
                    0 => dst.x = v,
                    1 => dst.y = v,
                    2 => dst.w = v,
                    3 => dst.h = v,
                    4 => crop.0 = v,
                    5 => crop.1 = v,
                    _ => zoom = v,
                }
                let (count, fb) = blended(|fb| fb.blit(&tex, dst, crop, zoom, false, style()));
                assert_eq!(count, 0, "blit field {field} = {v}");
                assert!(untouched(&fb), "blit field {field} = {v} drew");
            }
            let clip = Some(rect(0.0, 0.0, v, 8.0));
            let (count, _) = blended(|fb| {
                fb.blit(
                    &tex,
                    rect(0.0, 0.0, 8.0, 8.0),
                    (0.0, 0.0),
                    -1.0,
                    false,
                    BlitStyle { clip, ..style() },
                );
            });
            assert_eq!(count, 0, "non-finite clip {v}");
            // `fill` is a blit of the 1x1 white texture.
            let (count, _) = blended(|fb| fb.fill(rect(0.0, 0.0, v, v), [255.0; 3], 255.0, None));
            assert_eq!(count, 0, "fill {v}");
        }
    }

    /// #4715 SIBLING — `blit_sub`, the glyph path, has the same loop shape.
    #[test]
    fn the_glyph_path_is_bounded_and_rejects_non_finite_geometry_too() {
        let tex = red_tex();
        let src = rect(0.0, 0.0, 2.0, 2.0);
        // A glyph rect (or its clip window) far larger than the framebuffer.
        let huge = Some(rect(-5000.0, -5000.0, 20000.0, 20000.0));
        let (count, _) = blended(|fb| {
            blit_sub(
                fb,
                &tex,
                src,
                rect(-8000.0, -8000.0, 16000.0, 16000.0),
                BlitStyle {
                    clip: huge,
                    ..style()
                },
            );
        });
        assert_eq!(count, FB_PIXELS);
        let (count, _) = blended(|fb| {
            blit_sub(fb, &tex, src, rect(0.0, 0.0, 16000.0, 16000.0), style());
        });
        assert_eq!(count, FB_PIXELS, "the default clip is the framebuffer");

        for v in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            for field in 0..4 {
                let mut dst = rect(0.0, 0.0, 8.0, 8.0);
                match field {
                    0 => dst.x = v,
                    1 => dst.y = v,
                    2 => dst.w = v,
                    _ => dst.h = v,
                }
                let (count, fb) = blended(|fb| blit_sub(fb, &tex, src, dst, style()));
                assert_eq!(count, 0, "blit_sub dst field {field} = {v}");
                assert!(untouched(&fb));
            }
            let (count, _) = blended(|fb| {
                blit_sub(
                    fb,
                    &tex,
                    rect(0.0, 0.0, v, 2.0),
                    rect(0.0, 0.0, 8.0, 8.0),
                    style(),
                );
            });
            assert_eq!(count, 0, "non-finite source rect {v}");
            let clip = Some(rect(0.0, 0.0, 8.0, v));
            let (count, _) = blended(|fb| {
                blit_sub(
                    fb,
                    &tex,
                    src,
                    rect(0.0, 0.0, 8.0, 8.0),
                    BlitStyle { clip, ..style() },
                );
            });
            assert_eq!(count, 0, "non-finite clip {v}");
        }
    }

    /// Huge but finite values saturate the float→int casts to `i64::MIN/MAX`;
    /// the tiled texel index arithmetic must not overflow on them (a debug
    /// build panics, a release build silently wraps).
    #[test]
    fn saturated_casts_do_not_overflow_the_tiled_index_math() {
        let tex = red_tex();
        for (x, crop) in [(-3.0e38, 0.0), (3.0e38, 0.0), (0.0, 1.0e30), (0.0, -1.0e30)] {
            let (count, _) = blended(|fb| {
                fb.blit(
                    &tex,
                    rect(x, 0.0, 3.0e38, 8.0),
                    (crop, crop),
                    0.0,
                    true,
                    style(),
                );
            });
            assert!(count <= FB_PIXELS, "x={x} crop={crop}: {count} texels");
        }
    }

    /// Clamping the loop to the framebuffer must not move the texel mapping:
    /// a tile hanging off the left edge still samples the texels its
    /// on-screen part covers, exactly as before.
    #[test]
    fn clamping_the_loop_does_not_shift_the_sampled_texels() {
        // 8x1 ramp: texel i has red = i * 10.
        let mut tex = Rgba8::new(8, 1);
        for (i, px) in tex.pixels.chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[i as u8 * 10, 0, 0, 255]);
        }
        let mut fb = Framebuffer::new(4, 1);
        // Stretch 8 texels over an 8-wide tile starting at x = -2: framebuffer
        // column 0 is tile column 2, so it must show texel 2.
        fb.blit(
            &tex,
            rect(-2.0, 0.0, 8.0, 1.0),
            (0.0, 0.0),
            -1.0,
            false,
            style(),
        );
        let reds: Vec<u8> = fb.pixels.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(reds, [20, 30, 40, 50]);
    }
}
