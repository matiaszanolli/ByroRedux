//! Oblivion bitmap fonts: `.fnt` metrics + `.tex` glyph atlas.
//!
//! Binary layout (reverse-engineered against the five vanilla fonts in
//! `Oblivion - Misc.bsa`; all five are exactly 14 632 bytes, which pins
//! the glyph table at 256 × 56 bytes after a 296-byte header):
//!
//! ```text
//! offset  size  field
//! 0       4     f32  point size (28.0 for Kingthings Regular)
//! 4       4     u32  version (1)
//! 8       4     u32  texture count (1)
//! 12      284   char texture name ("Kingthings_Regular_0_Lod_A"),
//!               NUL-padded — 296-byte header total
//! 296     56×256 glyph records, 14 f32s each:
//!   [0]  left bearing (0 across the vanilla corpus)
//!   [1]  v0 (atlas top edge, top-down V — verified against atlas ink)
//!   [2]  u0
//!   [3]  v1
//!   [4]  u0 (corner repetition; [1]==[5], [2]==[4], [3]==[7], [6]==[8])
//!   [5]  v0
//!   [6]  u1
//!   [7]  v1
//!   [8]  u1
//!   [9]  glyph height, texels
//!   [10] glyph width, texels
//!   [11] 0 across the corpus
//!   [12] y offset (−1 for inked glyphs; the space glyph parks its
//!        advance here — 11 — with [13] left at 0)
//!   [13] horizontal advance (0 for control chars / space)
//! ```
//!
//! The companion `.tex` is `u32 width, u32 height, width*height*4` raw
//! RGBA — white glyph ink on transparent, tinted at draw time by the
//! text tile's red/green/blue traits (see [`crate::raster`]).

use crate::tex::Rgba8;

/// Header size before the glyph table.
const HEADER_LEN: usize = 296;
/// Glyph record size (14 f32s).
const GLYPH_LEN: usize = 56;
/// Glyphs per font — the full extended-ASCII page, fixed-size table.
const GLYPH_COUNT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Atlas rectangle in texel space (top-down V).
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    /// Inked size in texels.
    pub width: f32,
    pub height: f32,
    /// Horizontal pen advance in texels.
    pub advance: f32,
    /// Vertical draw offset (typically −1).
    pub y_offset: f32,
    /// Whether the glyph has any ink (false for space/control chars).
    pub inked: bool,
}

#[derive(Debug, Clone)]
pub struct Font {
    pub point_size: f32,
    pub texture_name: String,
    pub glyphs: Vec<Glyph>,
    /// Glyph atlas, decoded from the companion `.tex`.
    pub atlas: Rgba8,
}

impl Font {
    /// Parse a `.fnt` and its companion `.tex` bytes.
    pub fn parse(fnt: &[u8], tex: &[u8]) -> Result<Self, FontError> {
        if fnt.len() < HEADER_LEN {
            return Err(FontError::TruncatedHeader(fnt.len()));
        }
        let point_size = f32::from_le_bytes(fnt[0..4].try_into().unwrap());
        let name_end = fnt[12..HEADER_LEN]
            .iter()
            .position(|&b| b == 0)
            .map(|p| 12 + p)
            .unwrap_or(HEADER_LEN);
        let texture_name = String::from_utf8_lossy(&fnt[12..name_end]).to_string();
        let atlas = Rgba8::parse_font_tex(tex).map_err(FontError::BadAtlas)?;

        let expect = HEADER_LEN + GLYPH_COUNT * GLYPH_LEN;
        if fnt.len() < expect {
            return Err(FontError::TruncatedGlyphTable {
                have: fnt.len(),
                expect,
            });
        }
        let mut glyphs = Vec::with_capacity(GLYPH_COUNT);
        for c in 0..GLYPH_COUNT {
            let base = HEADER_LEN + c * GLYPH_LEN;
            let f = |i: usize| {
                f32::from_le_bytes(fnt[base + i * 4..base + i * 4 + 4].try_into().unwrap())
            };
            let v0 = f(1);
            let u0 = f(2);
            let v1 = f(3);
            let u1 = f(6);
            let width = f(10);
            let height = f(9);
            let y_offset = f(12);
            // Advance lives at [13]; the space glyph alone parks it at
            // [12] with [13] zero (verified across the vanilla corpus:
            // every inked glyph has [12] == −1 and [13] > 0, space has
            // [12] == 11 and [13] == 0).
            let advance = if f(13) != 0.0 { f(13) } else if c == b' ' as usize { y_offset } else { 0.0 };
            glyphs.push(Glyph {
                u0,
                v0,
                u1,
                v1,
                width,
                height,
                advance,
                y_offset: if c == b' ' as usize { 0.0 } else { y_offset },
                inked: width > 0.0 && height > 0.0,
            });
        }
        Ok(Self {
            point_size,
            texture_name,
            glyphs,
            atlas,
        })
    }

    pub fn glyph(&self, ch: u8) -> &Glyph {
        &self.glyphs[ch as usize]
    }

    /// Measured advance width of `text` in texels.
    pub fn measure_width(&self, text: &str) -> f32 {
        let mut w = 0.0;
        for &b in text.as_bytes() {
            w += self.glyph(b).advance;
        }
        w
    }

    /// Line height: the authored point size doubles as the row pitch
    /// (28 pt font, 28 px rows in the HUD's authored coordinates).
    pub fn line_height(&self) -> f32 {
        self.point_size
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FontError {
    #[error("fnt header truncated: {0} bytes")]
    TruncatedHeader(usize),
    #[error("fnt glyph table truncated: {have} of {expect} bytes")]
    TruncatedGlyphTable { have: usize, expect: usize },
    #[error("glyph atlas unusable: {0:?}")]
    BadAtlas(crate::tex::TexError),
}
