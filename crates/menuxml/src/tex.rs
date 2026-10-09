//! CPU-side texture decoding for menu assets.
//!
//! Two container formats carry UI pixels:
//!
//! * menu art — DDS (DXT1/DXT3/DXT5 and uncompressed masked RGB/RGBA —
//!   vanilla `textures\menus…` mixes DXT3/DXT5 with raw 32-bit BGRA,
//!   24-bit BGR and 16-bpp A4R4G4B4/R5G6B5 files, a wider surface than
//!   the `image` crate's DDS decoder accepts, so this module implements
//!   the format directly);
//! * font atlases — Bethesda's raw `.tex` (see [`Rgba8::parse_font_tex`]).
//!
//! Only the top mip is decoded — menu art authors single-level textures.

/// A decoded RGBA8 image, top-down rows, non-premultiplied alpha.
#[derive(Debug, Clone)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl Rgba8 {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    /// Fetch a pixel (r, g, b, a) with clamp-to-edge addressing.
    #[inline]
    pub fn pixel(&self, x: i64, y: i64) -> [u8; 4] {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        let o = (y * self.width as usize + x) * 4;
        [
            self.pixels[o],
            self.pixels[o + 1],
            self.pixels[o + 2],
            self.pixels[o + 3],
        ]
    }

    /// Parse a font `.tex`: `u32 width, u32 height, w*h*4` raw RGBA.
    /// Vanilla files carry white ink on transparent; the background's
    /// odd blue=1 constant is preserved (effectively transparent since
    /// alpha is 0).
    pub fn parse_font_tex(bytes: &[u8]) -> Result<Self, TexError> {
        if bytes.len() < 8 {
            return Err(TexError::TruncatedHeader(bytes.len()));
        }
        let width = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let expect = width as usize * height as usize * 4;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(TexError::AbsurdDimensions(width, height));
        }
        if bytes.len() < 8 + expect {
            return Err(TexError::TruncatedPixels {
                have: bytes.len() - 8,
                expect,
            });
        }
        Ok(Self {
            width,
            height,
            pixels: bytes[8..8 + expect].to_vec(),
        })
    }

    /// Decode a menu DDS: DXT1 / DXT3 / DXT5 or uncompressed
    /// (masked 32/24/16-bit, A8L8/L8-style luminance). Returns `None` with
    /// a one-line debug log for anything else.
    pub fn decode_dds(bytes: &[u8]) -> Option<Self> {
        decode_dds_impl(bytes)
    }
}

fn decode_dds_impl(bytes: &[u8]) -> Option<Rgba8> {
    if bytes.len() < 4 + 124 || &bytes[0..4] != b"DDS " {
        log::debug!("menuxml: not a DDS (len {})", bytes.len());
        return None;
    }
    let h = |off: usize| u32::from_le_bytes(bytes[4 + off..4 + off + 4].try_into().unwrap());
    let height = h(8);
    let width = h(12);
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        log::debug!("menuxml: DDS absurd dimensions {width}x{height}");
        return None;
    }
    // DDS_PIXELFORMAT lives at header offset 72.
    let pf_flags = h(76);
    let fourcc = &bytes[4 + 80..4 + 84];
    let bit_count = h(84);
    let r_mask = h(88);
    let g_mask = h(92);
    let b_mask = h(96);
    let a_mask = h(100);

    const DDPF_FOURCC: u32 = 0x4;
    let out = if pf_flags & DDPF_FOURCC != 0 {
        match fourcc {
            b"DXT1" => decode_dxt1(bytes, width, height),
            b"DXT3" => decode_dxt3(bytes, width, height),
            b"DXT5" => decode_dxt5(bytes, width, height),
            other => {
                log::debug!("menuxml: DDS fourcc {:?} unsupported", other);
                return None;
            }
        }
    } else {
        decode_uncompressed(
            bytes, width, height, bit_count, pf_flags, r_mask, g_mask, b_mask, a_mask,
        )
    };
    if out.is_none() {
        log::debug!(
            "menuxml: DDS payload truncated ({width}x{height}, {} bytes)",
            bytes.len()
        );
    }
    out
}

// ---------------------------------------------------------------------------
// Block-compressed decoders (BC1/BC2/BC3, aka DXT1/3/5)
// ---------------------------------------------------------------------------

/// 4×4-pixel block iteration: yields (block_x, block_y, block_offset).
fn blocks(width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    let bw = width.div_ceil(4);
    let bh = height.div_ceil(4);
    (0..bh).flat_map(move |by| (0..bw).map(move |bx| (bx, by)))
}

/// Decode the BC1 colour half of a block into `out` at pixel (px, py).
/// `has_alpha` selects the 3-colour + transparent mode when c0 <= c1.
fn bc1_colors(
    c0: u16,
    c1: u16,
    indices: u32,
    has_alpha: bool,
    out: &mut Rgba8,
    px: u32,
    py: u32,
) {
    let expand = |c: u16| -> [u8; 3] {
        let r = ((c >> 11) & 0x1f) as u8;
        let g = ((c >> 5) & 0x3f) as u8;
        let b = (c & 0x1f) as u8;
        [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
    };
    let a = expand(c0);
    let b = expand(c1);
    // Interpolate in u16 — the endpoint sums reach 3×255 and a premature
    // `as u8` wraps before the divide (a real bug this module shipped
    // with on first pass: 651 → 139 → 46 instead of 217).
    let mix = |wa: u16, wb: u16| -> [u8; 3] {
        [
            ((a[0] as u16 * wa + b[0] as u16 * wb) / (wa + wb)) as u8,
            ((a[1] as u16 * wa + b[1] as u16 * wb) / (wa + wb)) as u8,
            ((a[2] as u16 * wa + b[2] as u16 * wb) / (wa + wb)) as u8,
        ]
    };
    let (c2, c3) = if c0 > c1 {
        (mix(2, 1), mix(1, 2))
    } else {
        (mix(1, 1), [0, 0, 0])
    };
    for i in 0..16u32 {
        let x = px + i % 4;
        let y = py + i / 4;
        if x >= out.width || y >= out.height {
            continue;
        }
        let idx = ((indices >> (i * 2)) & 0x3) as usize;
        let (rgb, alpha) = match idx {
            0 => (a, 255),
            1 => (b, 255),
            2 => (c2, 255),
            _ => (c3, if has_alpha && c0 <= c1 { 0 } else { 255 }),
        };
        let o = (y as usize * out.width as usize + x as usize) * 4;
        out.pixels[o..o + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
    }
}

fn decode_dxt1(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = Rgba8::new(width, height);
    for (bx, by) in blocks(width, height) {
        let off = 128 + ((by as usize * width.div_ceil(4) as usize + bx as usize) * 8);
        if off + 8 > bytes.len() {
            return None;
        }
        let c0 = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap());
        let c1 = u16::from_le_bytes(bytes[off + 2..off + 4].try_into().unwrap());
        let indices = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
        bc1_colors(c0, c1, indices, true, &mut out, bx * 4, by * 4);
    }
    Some(out)
}

fn decode_dxt3(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = Rgba8::new(width, height);
    for (bx, by) in blocks(width, height) {
        let off = 128 + ((by as usize * width.div_ceil(4) as usize + bx as usize) * 16);
        if off + 16 > bytes.len() {
            return None;
        }
        // 8 bytes of 4-bit explicit alpha, row-major within the block.
        let alpha: [u8; 8] = bytes[off..off + 8].try_into().unwrap();
        let c0 = u16::from_le_bytes(bytes[off + 8..off + 10].try_into().unwrap());
        let c1 = u16::from_le_bytes(bytes[off + 10..off + 12].try_into().unwrap());
        let indices = u32::from_le_bytes(bytes[off + 12..off + 16].try_into().unwrap());
        bc1_colors(c0, c1, indices, false, &mut out, bx * 4, by * 4);
        for i in 0..16u32 {
            let x = bx * 4 + i % 4;
            let y = by * 4 + i / 4;
            if x >= out.width || y >= out.height {
                continue;
            }
            let nibble = (alpha[(i / 2) as usize] >> ((i % 2) * 4)) & 0xf;
            let o = (y as usize * out.width as usize + x as usize) * 4 + 3;
            out.pixels[o] = nibble * 17;
        }
    }
    Some(out)
}

fn decode_dxt5(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = Rgba8::new(width, height);
    for (bx, by) in blocks(width, height) {
        let off = 128 + ((by as usize * width.div_ceil(4) as usize + bx as usize) * 16);
        if off + 16 > bytes.len() {
            return None;
        }
        let a0 = bytes[off];
        let a1 = bytes[off + 1];
        // 16 3-bit selectors packed little-endian into 6 bytes.
        let mut bits = 0u64;
        for (i, &b) in bytes[off + 2..off + 8].iter().enumerate() {
            bits |= (b as u64) << (i * 8);
        }
        let c0 = u16::from_le_bytes(bytes[off + 8..off + 10].try_into().unwrap());
        let c1 = u16::from_le_bytes(bytes[off + 10..off + 12].try_into().unwrap());
        let indices = u32::from_le_bytes(bytes[off + 12..off + 16].try_into().unwrap());
        bc1_colors(c0, c1, indices, false, &mut out, bx * 4, by * 4);
        for i in 0..16u32 {
            let x = bx * 4 + i % 4;
            let y = by * 4 + i / 4;
            if x >= out.width || y >= out.height {
                continue;
            }
            let sel = ((bits >> (i * 3)) & 0x7) as u8;
            let alpha = match sel {
                0 => a0,
                1 => a1,
                _ if a0 > a1 => {
                    ((a0 as u16 * (8 - sel as u16) + a1 as u16 * (sel as u16 - 1)) / 7) as u8
                }
                _ if sel < 5 => ((a0 as u16 * (5 - sel as u16) + a1 as u16 * (sel as u16 - 1)) / 5) as u8,
                5 => 0,
                6 => 255,
                _ => a1,
            };
            let o = (y as usize * out.width as usize + x as usize) * 4 + 3;
            out.pixels[o] = alpha;
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Uncompressed: masked channels (BGRA byte order on disk), 24/32-bit,
// plus single-channel luminance/alpha forms.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)] // the masks mirror the DDS BITMAPINFOHEADER channel layout 1:1 — a struct would not make the decode clearer
fn decode_uncompressed(
    bytes: &[u8],
    width: u32,
    height: u32,
    bit_count: u32,
    pf_flags: u32,
    r_mask: u32,
    g_mask: u32,
    b_mask: u32,
    a_mask: u32,
) -> Option<Rgba8> {
    let bytes_per_px = match bit_count {
        8 => 1usize,
        16 => 2,
        24 => 3,
        32 => 4,
        _ => {
            log::debug!("menuxml: DDS uncompressed {bit_count}-bit unsupported");
            return None;
        }
    };
    let pitch = (width as usize * bytes_per_px).max(1);
    let data = &bytes[128..];
    // #5377: size the payload before allocating the RGBA canvas — a
    // 128-byte file claiming 8192² used to reserve 256 MiB before the
    // first bounds check rejected it.
    if data.len() < pitch * height as usize {
        return None;
    }
    let mut out = Rgba8::new(width, height);
    let shift_of = |mask: u32| -> (u32, u32) {
        if mask == 0 {
            return (0, 0); // absent channel — `chan` guards the use
        }
        let shift = mask.trailing_zeros();
        let bits = 32 - mask.leading_zeros() - shift;
        (shift, bits)
    };
    let (rs, rb) = shift_of(r_mask);
    let (gs, gb) = shift_of(g_mask);
    let (bs, bb) = shift_of(b_mask);
    let (as_, ab) = shift_of(a_mask);
    // Normalize a `bits`-wide sample to 8 bits. `bits < 8` replicates the
    // pattern (0b1 -> 0xFF, 0b10 -> 0xAA) via shift-OR doubling — the
    // closed form `v >> (2*bits - 8)` underflowed for bits < 4 (#5377).
    let scale = |v: u32, bits: u32| -> u8 {
        debug_assert!(bits > 0 && bits <= 32);
        if bits >= 8 {
            return (v >> (bits - 8)).clamp(0, 255) as u8;
        }
        let mut v8 = v << (8 - bits);
        let mut shift = bits;
        while shift < 8 {
            v8 |= v8 >> shift;
            shift *= 2;
        }
        v8.clamp(0, 255) as u8
    };
    // A zero mask is an absent channel (L8's G/B, A8's RGB), not a 1-bit
    // channel: decode as 0 and let the luminance/alpha paths fill the
    // real slots.
    let chan = |raw: u32, mask: u32, (shift, bits): (u32, u32)| -> u8 {
        if mask == 0 {
            0
        } else {
            scale((raw & mask) >> shift, bits)
        }
    };
    // DDPF_LUMINANCE (0x20000): 8-bit files carry grey in the lone R
    // mask slot (`0xFF, 0, 0`). Keying off all-equal masks missed the
    // standard header and decoded L8 as red-only.
    const DDPF_LUMINANCE: u32 = 0x2_0000;
    let luminance = r_mask != 0
        && g_mask == 0
        && b_mask == 0
        && (pf_flags & DDPF_LUMINANCE != 0 || bit_count == 8);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let o = y * pitch + x * bytes_per_px;
            let raw = match bytes_per_px {
                4 => u32::from_le_bytes(data[o..o + 4].try_into().unwrap()),
                3 => u32::from_le_bytes([data[o], data[o + 1], data[o + 2], 0]),
                2 => u32::from_le_bytes([data[o], data[o + 1], 0, 0]),
                _ => data[o] as u32,
            };
            let p = (y * out.width as usize + x) * 4;
            let r = chan(raw, r_mask, (rs, rb));
            let g = chan(raw, g_mask, (gs, gb));
            let b = chan(raw, b_mask, (bs, bb));
            let a = if a_mask == 0 { 255 } else { scale((raw & a_mask) >> as_, ab) };
            if luminance {
                out.pixels[p..p + 4].copy_from_slice(&[r, r, r, a]);
            } else {
                out.pixels[p..p + 4].copy_from_slice(&[r, g, b, a]);
            }
        }
    }
    Some(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TexError {
    #[error("tex header truncated: {0} bytes")]
    TruncatedHeader(usize),
    #[error("tex absurd dimensions {0}x{1}")]
    AbsurdDimensions(u32, u32),
    #[error("tex pixel payload truncated: {have} of {expect} bytes")]
    TruncatedPixels { have: usize, expect: usize },
}

#[cfg(test)]
mod uncompressed_decode_tests {
    use super::Rgba8;

    /// Hand-roll an uncompressed DDS header (128 B) + payload. The masks
    /// are [R, G, B, A] in pixel-format order (abs 92..108).
    fn dds(
        width: u32,
        height: u32,
        flags: u32,
        bit_count: u32,
        masks: [u32; 4],
        payload: &[u8],
    ) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&height.to_le_bytes());
        b[16..20].copy_from_slice(&width.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&flags.to_le_bytes());
        // dwFourCC (abs 84) stays zero; the masks start at abs 92.
        b[88..92].copy_from_slice(&bit_count.to_le_bytes());
        for (i, m) in masks.iter().enumerate() {
            b[92 + i * 4..96 + i * 4].copy_from_slice(&m.to_le_bytes());
        }
        b.extend_from_slice(payload);
        b
    }

    fn px(img: &Rgba8) -> [u8; 4] {
        [img.pixels[0], img.pixels[1], img.pixels[2], img.pixels[3]]
    }

    /// #5377 — the standard L8 header (lone R mask, G/B zero) must decode
    /// as grey, not `(L, 0, 0)`; a zero colour mask is an absent channel,
    /// not a 1-bit one, so nothing may underflow either.
    #[test]
    fn l8_grey_decodes_as_luminance_not_red() {
        let img = Rgba8::decode_dds(&dds(1, 1, 0x2_0000, 8, [0xFF, 0, 0, 0], &[0x80])).unwrap();
        assert_eq!(px(&img), [128, 128, 128, 255]);
    }

    /// #5377 — A8 (all colour masks zero) decodes black + alpha.
    #[test]
    fn a8_decodes_alpha_only() {
        let img = Rgba8::decode_dds(&dds(1, 1, 0x2, 8, [0, 0, 0, 0xFF], &[0x80])).unwrap();
        assert_eq!(px(&img), [0, 0, 0, 128]);
    }

    /// #5377 — a 2-bit alpha channel replicates to full range (0b11 ->
    /// 255), not the old `3 << 6 = 192` truncation.
    #[test]
    fn a2r10g10b10_two_bit_alpha_replicates_to_opaque() {
        let img = Rgba8::decode_dds(&dds(
            1,
            1,
            0x41,
            32,
            [0x0000_03FF, 0x000F_FC00, 0x3FF0_0000, 0xC000_0000],
            &0xFFFF_FFFFu32.to_le_bytes(),
        ))
        .unwrap();
        assert_eq!(px(&img), [255, 255, 255, 255]);
    }

    /// The vanilla 32-bpp BGRA layout still decodes through its masks.
    #[test]
    fn bgra_32bpp_decodes_through_masks() {
        let img = Rgba8::decode_dds(&dds(
            1,
            1,
            0x41,
            32,
            [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000],
            &[1, 2, 3, 255],
        ))
        .unwrap();
        assert_eq!(px(&img), [3, 2, 1, 255]);
    }

    /// #5377 — the payload is sized against `width * height * bpp` before
    /// the RGBA canvas is allocated, so a header claiming 8192² over a
    /// 132-byte file is rejected without reserving 256 MiB.
    #[test]
    fn truncated_payload_is_rejected_before_allocation() {
        assert!(Rgba8::decode_dds(&dds(
            8192,
            8192,
            0x41,
            32,
            [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000],
            &[0, 0, 0, 255],
        ))
        .is_none());
    }

    /// #5400 — the vanilla Oblivion A4R4G4B4 layout (flags 0x41, masks
    /// R 0x0F00 / G 0x00F0 / B 0x000F / A 0xF000; 48 menu textures under
    /// `textures\menus\`) decodes through the masked path with nibble
    /// replication: raw 0x842F → A 0x8, R 0x4, G 0x2, B 0xF →
    /// (0x44, 0x22, 0xFF, 0x88).
    #[test]
    fn a4r4g4b4_16bpp_decodes_through_masks() {
        let img = Rgba8::decode_dds(&dds(
            1,
            1,
            0x41,
            16,
            [0x0F00, 0x00F0, 0x000F, 0xF000],
            &[0x2F, 0x84],
        ))
        .unwrap();
        assert_eq!(px(&img), [0x44, 0x22, 0xFF, 0x88]);
    }

    /// #5400 — the vanilla Oblivion R5G6B5 layout (flags 0x40, the
    /// `healthbar3dbw.dds` header): no alpha mask ⇒ opaque, and the
    /// 5/6-bit channels replicate to full range.
    #[test]
    fn r5g6b5_16bpp_decodes_opaque() {
        let img = Rgba8::decode_dds(&dds(
            2,
            1,
            0x40,
            16,
            [0xF800, 0x07E0, 0x001F, 0],
            &[0x00, 0xF8, 0xE0, 0x07],
        ))
        .unwrap();
        assert_eq!(px(&img), [255, 0, 0, 255]);
        assert_eq!(
            [img.pixels[4], img.pixels[5], img.pixels[6], img.pixels[7]],
            [0, 255, 0, 255]
        );
    }
}
