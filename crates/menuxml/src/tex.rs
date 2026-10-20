//! CPU-side texture decoding for menu assets.
//!
//! Two container formats carry UI pixels:
//!
//! * menu art — DDS (DXT1/DXT3/DXT5 and uncompressed masked RGB/RGBA —
//!   vanilla `textures\menus…` mixes DXT3/DXT5 with raw 32-bit BGRA,
//!   24-bit BGR and 16-bpp A4R4G4B4/R5G6B5 files, a wider surface than
//!   the `image` crate's DDS decoder accepts, so this module implements
//!   the format directly). The same decoder serves engine textures on the
//!   CPU (`tex.dump`, terrain height generation #6195), so it also reads
//!   BC4/BC5 (`ATI1`/`ATI2`/`BC4U`/`BC5U`, Fallout 4's two-channel normal
//!   maps) and the DX10 extended header for BC1–BC5 and 8-bit RGBA/BGRA/R.
//!   BC6H/BC7 are not decoded;
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

    /// The RGBA byte count [`Self::decode_dds`] would allocate for `bytes`,
    /// read from the header alone — `None` for a header it would reject.
    /// Lets a caller hold a memory budget before paying for a decode
    /// (#5658).
    pub fn decoded_len(bytes: &[u8]) -> Option<usize> {
        dds_dimensions(bytes).map(|(width, height)| width as usize * height as usize * 4)
    }
}

/// The magic and dimension checks every decode starts with, shared with the
/// pre-decode budget probe so the two can never disagree.
fn dds_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
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
    Some((width, height))
}

fn decode_dds_impl(bytes: &[u8]) -> Option<Rgba8> {
    let (width, height) = dds_dimensions(bytes)?;
    let h = |off: usize| u32::from_le_bytes(bytes[4 + off..4 + off + 4].try_into().unwrap());
    // DDS_PIXELFORMAT lives at header offset 72.
    let pf_flags = h(76);
    let fourcc = &bytes[4 + 80..4 + 84];
    let bit_count = h(84);
    let r_mask = h(88);
    let g_mask = h(92);
    let b_mask = h(96);
    let a_mask = h(100);
    let payload = &bytes[128..];

    const DDPF_FOURCC: u32 = 0x4;
    let out = if pf_flags & DDPF_FOURCC != 0 {
        match fourcc {
            b"DXT1" => decode_dxt1(payload, width, height),
            b"DXT3" => decode_dxt3(payload, width, height),
            b"DXT5" => decode_dxt5(payload, width, height),
            b"ATI1" | b"BC4U" => decode_bc4(payload, width, height),
            b"ATI2" | b"BC5U" => decode_bc5(payload, width, height),
            b"DX10" => decode_dx10(bytes, width, height),
            other => {
                log::debug!("menuxml: DDS fourcc {:?} unsupported", other);
                return None;
            }
        }
    } else {
        decode_uncompressed(
            payload, width, height, bit_count, pf_flags, r_mask, g_mask, b_mask, a_mask,
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

/// Validate the complete compressed payload before reserving the RGBA canvas.
fn bc_output(bytes: &[u8], width: u32, height: u32, bytes_per_block: usize) -> Option<Rgba8> {
    let block_count = usize::try_from(width.div_ceil(4))
        .ok()?
        .checked_mul(usize::try_from(height.div_ceil(4)).ok()?)?;
    let payload_len = block_count.checked_mul(bytes_per_block)?;
    if bytes.len() < payload_len {
        return None;
    }
    Some(Rgba8::new(width, height))
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
    let mut out = bc_output(bytes, width, height, 8)?;
    for (bx, by) in blocks(width, height) {
        let off = (by as usize * width.div_ceil(4) as usize + bx as usize) * 8;
        let c0 = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap());
        let c1 = u16::from_le_bytes(bytes[off + 2..off + 4].try_into().unwrap());
        let indices = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
        bc1_colors(c0, c1, indices, true, &mut out, bx * 4, by * 4);
    }
    Some(out)
}

fn decode_dxt3(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = bc_output(bytes, width, height, 16)?;
    for (bx, by) in blocks(width, height) {
        let off = (by as usize * width.div_ceil(4) as usize + bx as usize) * 16;
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
    let mut out = bc_output(bytes, width, height, 16)?;
    for (bx, by) in blocks(width, height) {
        let off = (by as usize * width.div_ceil(4) as usize + bx as usize) * 16;
        let alpha = bc4_block(&bytes[off..off + 8]);
        let c0 = u16::from_le_bytes(bytes[off + 8..off + 10].try_into().unwrap());
        let c1 = u16::from_le_bytes(bytes[off + 10..off + 12].try_into().unwrap());
        let indices = u32::from_le_bytes(bytes[off + 12..off + 16].try_into().unwrap());
        bc1_colors(c0, c1, indices, false, &mut out, bx * 4, by * 4);
        write_channel(&mut out, bx, by, 3, &alpha);
    }
    Some(out)
}

/// One 8-byte BC4 block (also BC3's alpha half and each BC5 channel): two
/// endpoints and 16 3-bit selectors, decoded to 16 values in row-major
/// block order.
fn bc4_block(block: &[u8]) -> [u8; 16] {
    let a0 = block[0];
    let a1 = block[1];
    // 16 3-bit selectors packed little-endian into 6 bytes.
    let mut bits = 0u64;
    for (i, &b) in block[2..8].iter().enumerate() {
        bits |= (b as u64) << (i * 8);
    }
    let mut out = [0u8; 16];
    for (i, v) in out.iter_mut().enumerate() {
        let sel = ((bits >> (i * 3)) & 0x7) as u8;
        *v = match sel {
            0 => a0,
            1 => a1,
            _ if a0 > a1 => {
                ((a0 as u16 * (8 - sel as u16) + a1 as u16 * (sel as u16 - 1)) / 7) as u8
            }
            // Six-value mode (`a0 <= a1`): selectors 2..=5 interpolate in
            // fifths, 6 is 0 and 7 is 255. Selector 2 is (4·a0 + a1)/5.
            _ if sel < 6 => ((a0 as u16 * (6 - sel as u16) + a1 as u16 * (sel as u16 - 1)) / 5) as u8,
            6 => 0,
            _ => 255,
        };
    }
    out
}

/// Store a decoded 4×4 block's values into channel `c` of `out`, clipping
/// the block to the image edge.
fn write_channel(out: &mut Rgba8, bx: u32, by: u32, c: usize, values: &[u8; 16]) {
    for (i, &v) in values.iter().enumerate() {
        let x = bx * 4 + i as u32 % 4;
        let y = by * 4 + i as u32 / 4;
        if x >= out.width || y >= out.height {
            continue;
        }
        out.pixels[(y as usize * out.width as usize + x as usize) * 4 + c] = v;
    }
}

/// BC4 (`ATI1`): one channel, decoded as `(r, 0, 0, 255)` — what a GPU
/// sample of a `BC4_UNORM` texture returns.
fn decode_bc4(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = bc_output(bytes, width, height, 8)?;
    for chunk in out.pixels.chunks_exact_mut(4) {
        chunk[3] = 255;
    }
    for (bx, by) in blocks(width, height) {
        let off = (by as usize * width.div_ceil(4) as usize + bx as usize) * 8;
        write_channel(&mut out, bx, by, 0, &bc4_block(&bytes[off..off + 8]));
    }
    Some(out)
}

/// BC5 (`ATI2`): two BC4 channels, decoded as `(r, g, 0, 255)` — Fallout
/// 4's tangent-space normal maps, whose `z` the consumer reconstructs.
fn decode_bc5(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let mut out = bc_output(bytes, width, height, 16)?;
    for chunk in out.pixels.chunks_exact_mut(4) {
        chunk[3] = 255;
    }
    for (bx, by) in blocks(width, height) {
        let off = (by as usize * width.div_ceil(4) as usize + bx as usize) * 16;
        write_channel(&mut out, bx, by, 0, &bc4_block(&bytes[off..off + 8]));
        write_channel(&mut out, bx, by, 1, &bc4_block(&bytes[off + 8..off + 16]));
    }
    Some(out)
}

/// The DX10 extended header: a 20-byte `DDS_HEADER_DXT10` after the
/// classic header, whose first word is the `DXGI_FORMAT`. BA2 archives
/// reconstruct this header for every DX10 texture they store.
fn decode_dx10(bytes: &[u8], width: u32, height: u32) -> Option<Rgba8> {
    let dxgi = u32::from_le_bytes(bytes.get(128..132)?.try_into().unwrap());
    let payload = bytes.get(148..)?;
    match dxgi {
        // BC1..BC5, TYPELESS / UNORM / UNORM_SRGB each.
        70..=72 => decode_dxt1(payload, width, height),
        73..=75 => decode_dxt3(payload, width, height),
        76..=78 => decode_dxt5(payload, width, height),
        79 | 80 => decode_bc4(payload, width, height),
        82 | 83 => decode_bc5(payload, width, height),
        // R8G8B8A8 (TYPELESS / UNORM / UNORM_SRGB).
        27..=29 => decode_uncompressed(
            payload, width, height, 32, 0x41, 0xFF, 0xFF00, 0xFF_0000, 0xFF00_0000,
        ),
        // B8G8R8A8 UNORM / TYPELESS / UNORM_SRGB.
        87 | 90 | 91 => decode_uncompressed(
            payload, width, height, 32, 0x41, 0xFF_0000, 0xFF00, 0xFF, 0xFF00_0000,
        ),
        // R8_UNORM.
        61 => decode_uncompressed(payload, width, height, 8, 0, 0xFF, 0, 0, 0)
            .map(|mut img| {
                // Decoded through the L8 path as (r, r, r); a GPU sample of
                // R8 is (r, 0, 0).
                for px in img.pixels.chunks_exact_mut(4) {
                    px[1] = 0;
                    px[2] = 0;
                }
                img
            }),
        other => {
            log::debug!("menuxml: DDS DX10 DXGI format {other} unsupported");
            None
        }
    }
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
    let data = bytes;
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

    fn dxt_dds(width: u32, height: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&height.to_le_bytes());
        b[16..20].copy_from_slice(&width.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&0x4u32.to_le_bytes());
        b[84..88].copy_from_slice(fourcc);
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

    /// BC payload size is checked for every decoder before allocating the
    /// full RGBA canvas, including the 8192² case that would reserve 256 MiB.
    #[test]
    fn truncated_bc_payload_is_rejected_before_allocation() {
        for fourcc in [b"DXT1", b"DXT3", b"DXT5"] {
            assert!(Rgba8::decode_dds(&dxt_dds(8192, 8192, fourcc)).is_none());
        }
    }

    /// #6195 — one BC4 block, endpoints 0/255 (`a0 < a1`, the 6-value
    /// mode): selector 0 → 0, 1 → 255, 2 → 51. The value lands in R as a
    /// GPU sample would return it; BC5 decodes the second block into G.
    fn bc4_block_bytes() -> [u8; 8] {
        // Selectors for pixels 0..3: 0, 1, 2, 7 (3 bits each, LE).
        let bits: u64 = (1 << 3) | (2 << 6) | (7 << 9);
        let mut b = [0u8; 8];
        b[0] = 0;
        b[1] = 255;
        b[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
        b
    }

    /// The BC3-alpha / BC4 six-value mode (`a0 <= a1`) against the D3D
    /// table, with endpoints that are not 0/255 so the interpolation
    /// weights are actually exercised: selectors 2..=5 are
    /// (4a0+a1)/5 … (a0+4a1)/5, 6 is 0 and 7 is 255. The decoder used to
    /// weight selector 2 as (3a0+a1)/5 and map 5/6/7 to 0/255/a1.
    #[test]
    fn bc4_six_value_mode_matches_the_d3d_table() {
        let mut bits = 0u64;
        for (i, sel) in (0u64..8).enumerate() {
            bits |= sel << (i * 3);
        }
        let mut block = [0u8; 8];
        block[0] = 40;
        block[1] = 200;
        block[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
        let mut dds = dxt_dds(4, 4, b"ATI1");
        dds.extend_from_slice(&block);
        let img = Rgba8::decode_dds(&dds).unwrap();
        let reds: Vec<u8> = img.pixels.chunks_exact(4).take(8).map(|p| p[0]).collect();
        assert_eq!(reds, [40, 200, 72, 104, 136, 168, 0, 255]);
    }

    #[test]
    fn bc4_and_bc5_decode_into_red_and_green() {
        let mut bc4 = dxt_dds(4, 4, b"ATI1");
        bc4.extend_from_slice(&bc4_block_bytes());
        let img = Rgba8::decode_dds(&bc4).unwrap();
        let reds: Vec<u8> = img.pixels.chunks_exact(4).take(4).map(|p| p[0]).collect();
        assert_eq!(reds, [0, 255, 51, 255]);
        assert_eq!(&img.pixels[4..8], &[255, 0, 0, 255]);

        let mut bc5 = dxt_dds(4, 4, b"ATI2");
        bc5.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // R: all endpoint 0
        bc5.extend_from_slice(&bc4_block_bytes());
        let img = Rgba8::decode_dds(&bc5).unwrap();
        assert_eq!(&img.pixels[4..8], &[0, 255, 0, 255]);
    }

    /// The DX10 extended header (`DXT10` fourcc + 20-byte DXGI header) moves
    /// the payload to byte 148; BC5_UNORM (83) and R8G8B8A8_UNORM (28) both
    /// read from there, and BC7 (98) is declined rather than misread.
    #[test]
    fn dx10_header_formats_decode_from_the_extended_offset() {
        let dx10 = |dxgi: u32, w: u32, h: u32, payload: &[u8]| {
            let mut b = dxt_dds(w, h, b"DX10");
            let mut ext = [0u8; 20];
            ext[0..4].copy_from_slice(&dxgi.to_le_bytes());
            b.extend_from_slice(&ext);
            b.extend_from_slice(payload);
            b
        };
        let mut bc5 = [0u8; 16];
        bc5[8..16].copy_from_slice(&bc4_block_bytes());
        let img = Rgba8::decode_dds(&dx10(83, 4, 4, &bc5)).unwrap();
        assert_eq!(&img.pixels[4..8], &[0, 255, 0, 255]);

        let img = Rgba8::decode_dds(&dx10(28, 1, 1, &[10, 20, 30, 40])).unwrap();
        assert_eq!(px(&img), [10, 20, 30, 40]);
        let img = Rgba8::decode_dds(&dx10(87, 1, 1, &[10, 20, 30, 40])).unwrap();
        assert_eq!(px(&img), [30, 20, 10, 40]);

        assert!(Rgba8::decode_dds(&dx10(98, 4, 4, &[0u8; 16])).is_none());
        // Truncated extended header.
        assert!(Rgba8::decode_dds(&dxt_dds(4, 4, b"DX10")).is_none());
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
