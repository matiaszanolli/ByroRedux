//! Minimal DDS header parser — maps DDS pixel formats to Vulkan image formats.

use anyhow::{bail, ensure, Result};
use ash::vk;

const DDS_MAGIC: u32 = 0x20534444; // "DDS "
const HEADER_SIZE: usize = 128; // 4 magic + 124 DDS_HEADER
const DX10_EXT_SIZE: usize = 20;

// DDS_PIXELFORMAT flags
const DDPF_FOURCC: u32 = 0x4;
const DDPF_RGB: u32 = 0x40;

// FourCC values
const FOURCC_DXT1: u32 = u32::from_le_bytes(*b"DXT1");
const FOURCC_DXT3: u32 = u32::from_le_bytes(*b"DXT3");
const FOURCC_DXT5: u32 = u32::from_le_bytes(*b"DXT5");
const FOURCC_ATI2: u32 = u32::from_le_bytes(*b"ATI2");
const FOURCC_BC5S: u32 = u32::from_le_bytes(*b"BC5S");
const FOURCC_DX10: u32 = u32::from_le_bytes(*b"DX10");

// DXGI format codes (subset we care about)
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;
// Single-channel uncompressed (#1074 / FO4-D2-008)
const DXGI_FORMAT_R16_UNORM: u32 = 56; // 2 bytes/px — heightmaps, mono masks
const DXGI_FORMAT_R8_UNORM: u32 = 61; // 1 byte/px  — single-channel masks
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const DXGI_FORMAT_BC1_UNORM_SRGB: u32 = 72;
const DXGI_FORMAT_BC2_UNORM: u32 = 74;
const DXGI_FORMAT_BC2_UNORM_SRGB: u32 = 75;
const DXGI_FORMAT_BC3_UNORM: u32 = 77;
const DXGI_FORMAT_BC3_UNORM_SRGB: u32 = 78;
const DXGI_FORMAT_BC4_UNORM: u32 = 80;
const DXGI_FORMAT_BC4_SNORM: u32 = 81; // 8 B/block — signed normal channel (#1074)
const DXGI_FORMAT_BC5_UNORM: u32 = 83;
const DXGI_FORMAT_BC5_SNORM: u32 = 84;
// BGRA uncompressed (#1074 / FO4-D2-008) — FO4 normal maps ship as B8G8R8A8_UNORM
const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87; // 4 bytes/px
const DXGI_FORMAT_B8G8R8X8_UNORM: u32 = 88; // 4 bytes/px — BGRX, X ignored (UFO4P + mods)
const DXGI_FORMAT_B8G8R8A8_UNORM_SRGB: u32 = 91; // 4 bytes/px — sRGB variant
                                                 // BC6H HDR (#1074 / FO4-D2-008) — Starfield env maps; requires textureCompressionBC
const DXGI_FORMAT_BC6H_UF16: u32 = 95; // 16 B/block — unsigned half-float
const DXGI_FORMAT_BC6H_SF16: u32 = 96; // 16 B/block — signed half-float
const DXGI_FORMAT_BC7_UNORM: u32 = 98;
const DXGI_FORMAT_BC7_UNORM_SRGB: u32 = 99;

/// Parsed DDS metadata — everything needed for Vulkan image creation.
#[derive(Debug, Clone)]
pub struct DdsMetadata {
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub format: vk::Format,
    /// Bytes per block (8 for BC1/BC4, 16 for BC2/BC3/BC5/BC7) or bytes per pixel for uncompressed.
    pub block_size: u32,
    /// Whether the format is block-compressed (BC).
    pub compressed: bool,
    /// Byte offset where pixel data begins (128 standard, 148 for DX10 extended header).
    pub data_offset: usize,
    /// Set for an uncompressed `DDPF_RGB` DDS that is NOT 32-bpp
    /// R8G8B8A8 and must be CPU-expanded to R8G8B8A8 before upload
    /// (#1542). When `Some`, `format`/`block_size` already describe the
    /// *post-expansion* R8G8B8A8 target, and the caller must run
    /// [`expand_uncompressed_rgb`] on the raw bytes rather than uploading
    /// them directly. `None` for every zero-copy format (BC, 32-bpp RGBA).
    pub expand: Option<RgbExpand>,
}

/// Source pixel layout for a 16- or 24-bpp uncompressed `DDPF_RGB` DDS
/// that needs CPU expansion to R8G8B8A8 (#1542). The channel masks come
/// straight from DDS_PIXELFORMAT, so any ordering — 16-bpp A1R5G5B5 /
/// A4R4G4B4 / R5G6B5, 24-bpp R8G8B8 / B8G8R8 — decodes from the masks
/// alone without enumerating named formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbExpand {
    /// Source bits per pixel — 16 or 24.
    pub src_bpp: u32,
    pub r_mask: u32,
    pub g_mask: u32,
    pub b_mask: u32,
    /// `0` when the source carries no alpha channel (opaque → A=255).
    pub a_mask: u32,
}

/// Whether a loaded DDS format carries a usable alpha channel. Skyrim (and
/// Gamebryo) author the specular/gloss mask in the normal-map ALPHA, so this
/// gates the normal-alpha-as-spec path: BC1(RGB)/BC4/BC5/BC6H and single-
/// channel formats have no alpha (sampling `.a` returns 1.0), and must fall
/// back to the scalar `specular_strength` rather than read garbage gloss.
pub fn format_has_alpha(format: vk::Format) -> bool {
    matches!(
        format,
        vk::Format::B8G8R8A8_SRGB
            | vk::Format::B8G8R8A8_UNORM
            | vk::Format::R8G8B8A8_SRGB
            | vk::Format::BC2_SRGB_BLOCK
            | vk::Format::BC3_SRGB_BLOCK
            | vk::Format::BC7_SRGB_BLOCK
    )
}

/// Decode an RGB565-packed colour to normalised `[r, g, b]` in `[0, 1]`,
/// in the raw stored (sRGB-encoded) value space — no linearisation.
fn rgb565(c: u16) -> [f32; 3] {
    [
        ((c >> 11) & 0x1F) as f32 / 31.0,
        ((c >> 5) & 0x3F) as f32 / 63.0,
        (c & 0x1F) as f32 / 31.0,
    ]
}

/// Average RGB colour of a DDS texture, in the raw stored (monitor /
/// sRGB-encoded) value space — the SAME space `Material::diffuse_color`
/// lives in, so the two compose by a straight component multiply with no
/// sRGB linearisation (see the `feedback_color_space` rule).
///
/// Used as the GI bounce-albedo texel-mean (#1628): a textured surface
/// bleeds its average texel colour into the one-bounce GI rather than the
/// flat material tint. Computed ONCE at texture upload and cached per
/// handle — never re-derived per frame.
///
/// Reads the SMALLEST mip (already a maximally-downsampled whole-image
/// average), and strides the block/pixel scan to a fixed cap, so the cost
/// is bounded regardless of texture size or mip count. Handles the colour
/// formats Bethesda authors diffuse maps in — BC1/BC2/BC3 (RGB565 endpoint
/// mean) and uncompressed RGBA8/BGRA8. Returns `None` for non-colour
/// formats (BC4/BC5 masks + normals, BC6H HDR, single-channel) and BC7
/// (variable-mode block — not worth a CPU decoder for a 1×1 average); the
/// caller then keeps the material tint unchanged.
pub fn average_rgb(meta: &DdsMetadata, data: &[u8]) -> Option<[f32; 3]> {
    // Byte offset of the smallest mip level.
    let target = meta.mip_count.saturating_sub(1);
    let mut offset = meta.data_offset;
    for m in 0..target {
        offset += mip_size(meta.width, meta.height, m, meta.block_size, meta.compressed) as usize;
    }
    let w = (meta.width >> target).max(1) as usize;
    let h = (meta.height >> target).max(1) as usize;
    let bs = meta.block_size as usize;

    // Cap the number of samples so a single-mip 4K texture doesn't pay a
    // million-block scan; striding keeps a representative spread.
    const MAX_SAMPLES: usize = 4096;

    if meta.compressed {
        // Colour sub-block offset inside each block: BC1 carries colour at
        // byte 0, BC2/BC3 prefix an 8-byte alpha block.
        let color_off = match meta.format {
            vk::Format::BC1_RGB_SRGB_BLOCK | vk::Format::BC1_RGBA_SRGB_BLOCK => 0,
            vk::Format::BC2_SRGB_BLOCK | vk::Format::BC3_SRGB_BLOCK => 8,
            _ => return None, // BC4/BC5/BC6H/BC7 — not a diffuse-colour format
        };
        let blocks = w.div_ceil(4) * h.div_ceil(4);
        if blocks == 0 {
            return None;
        }
        let stride = (blocks / MAX_SAMPLES).max(1);
        let mut acc = [0.0f32; 3];
        let mut n = 0u32;
        let mut b = 0;
        while b < blocks {
            let base = offset + b * bs + color_off;
            if base + 4 > data.len() {
                break;
            }
            let c0 = rgb565(u16::from_le_bytes([data[base], data[base + 1]]));
            let c1 = rgb565(u16::from_le_bytes([data[base + 2], data[base + 3]]));
            for i in 0..3 {
                acc[i] += 0.5 * (c0[i] + c1[i]);
            }
            n += 1;
            b += stride;
        }
        (n > 0).then(|| [acc[0] / n as f32, acc[1] / n as f32, acc[2] / n as f32])
    } else {
        // Uncompressed 4-byte colour formats only; single-channel masks
        // (R8/R16) are not albedo.
        let swap_rb = match meta.format {
            vk::Format::R8G8B8A8_SRGB => false,
            vk::Format::B8G8R8A8_SRGB | vk::Format::B8G8R8A8_UNORM => true,
            _ => return None,
        };
        let pixels = w * h;
        if pixels == 0 {
            return None;
        }
        let stride = (pixels / MAX_SAMPLES).max(1);
        let mut acc = [0.0f32; 3];
        let mut n = 0u32;
        let mut p = 0;
        while p < pixels {
            let o = offset + p * bs;
            if o + 3 > data.len() {
                break;
            }
            let (r, g, b) = if swap_rb {
                (data[o + 2], data[o + 1], data[o])
            } else {
                (data[o], data[o + 1], data[o + 2])
            };
            acc[0] += r as f32;
            acc[1] += g as f32;
            acc[2] += b as f32;
            n += 1;
            p += stride;
        }
        let denom = n as f32 * 255.0;
        (n > 0).then(|| [acc[0] / denom, acc[1] / denom, acc[2] / denom])
    }
}

/// Parse a DDS file header and return metadata.
pub fn parse_dds(data: &[u8]) -> Result<DdsMetadata> {
    ensure!(
        data.len() >= HEADER_SIZE,
        "DDS file too small ({} bytes)",
        data.len()
    );

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    ensure!(
        magic == DDS_MAGIC,
        "Not a DDS file (bad magic: {:#010x})",
        magic
    );

    // DDS_HEADER starts at offset 4
    let height = read_u32(data, 12);
    let width = read_u32(data, 16);
    let mip_count = read_u32(data, 28).max(1);

    // DDS_PIXELFORMAT at offset 76 within file (offset 72 within DDS_HEADER + 4 magic)
    let pf_flags = read_u32(data, 80);
    let pf_fourcc = read_u32(data, 84);
    let pf_rgb_bit_count = read_u32(data, 88);

    if pf_flags & DDPF_FOURCC != 0 {
        if pf_fourcc == FOURCC_DX10 {
            // DX10 extended header
            ensure!(
                data.len() >= HEADER_SIZE + DX10_EXT_SIZE,
                "DDS DX10 extended header truncated"
            );
            let dxgi_format = read_u32(data, HEADER_SIZE);
            let (format, block_size, compressed) = map_dxgi_format(dxgi_format)?;
            Ok(DdsMetadata {
                width,
                height,
                mip_count,
                format,
                block_size,
                compressed,
                data_offset: HEADER_SIZE + DX10_EXT_SIZE,
                expand: None,
            })
        } else {
            let (format, block_size) = map_fourcc(pf_fourcc)?;
            Ok(DdsMetadata {
                width,
                height,
                mip_count,
                format,
                block_size,
                compressed: true,
                data_offset: HEADER_SIZE,
                expand: None,
            })
        }
    } else if pf_flags & DDPF_RGB != 0 {
        let bpp = pf_rgb_bit_count;
        if bpp == 32 {
            // 32-bpp R8G8B8A8 uploads directly — zero-copy.
            Ok(DdsMetadata {
                width,
                height,
                mip_count,
                format: vk::Format::R8G8B8A8_SRGB,
                block_size: 4, // bytes per pixel
                compressed: false,
                data_offset: HEADER_SIZE,
                expand: None,
            })
        } else if bpp == 16 || bpp == 24 {
            // #1542 — FO3/FNV-era font glyph atlases (16-bpp, e.g.
            // A1R5G5B5 / A4R4G4B4) and the HUD compass (24-bpp R8G8B8)
            // carry DDPF_RGB but no 32-bpp layout and no sampled Vulkan
            // format of their own. Decode them from the DDS_PIXELFORMAT
            // channel masks and CPU-expand to R8G8B8A8 at upload time.
            let r_mask = read_u32(data, 92);
            let g_mask = read_u32(data, 96);
            let b_mask = read_u32(data, 100);
            let a_mask = read_u32(data, 104);
            ensure!(
                r_mask | g_mask | b_mask != 0,
                "Uncompressed {bpp}-bpp DDS has empty RGB channel masks — cannot decode",
            );
            Ok(DdsMetadata {
                width,
                height,
                mip_count,
                // Post-expansion target — the raw bytes are `bpp`-bpp until
                // `expand_uncompressed_rgb` runs.
                format: vk::Format::R8G8B8A8_SRGB,
                block_size: 4,
                compressed: false,
                data_offset: HEADER_SIZE,
                expand: Some(RgbExpand {
                    src_bpp: bpp,
                    r_mask,
                    g_mask,
                    b_mask,
                    a_mask,
                }),
            })
        } else {
            bail!("Unsupported uncompressed DDS: {bpp} bpp (RGB masks; expected 16/24/32)");
        }
    } else {
        bail!("Unsupported DDS pixel format (flags={:#x})", pf_flags);
    }
}

/// Expand a 16- or 24-bpp uncompressed `DDPF_RGB` DDS to a contiguous
/// R8G8B8A8 buffer covering all mips, laid out mip-0-first to match what
/// the upload path expects for `block_size = 4` (#1542).
///
/// `meta.expand` must be `Some`; returns an empty `Vec` otherwise. Source
/// pixels are read starting at `meta.data_offset`, one `src_bpp/8`-byte
/// little-endian value per pixel, then each channel is extracted via its
/// mask and bit-expanded to 8 bits. A zero alpha mask yields an opaque
/// (255) alpha.
pub fn expand_uncompressed_rgb(meta: &DdsMetadata, data: &[u8]) -> Vec<u8> {
    let Some(ex) = meta.expand else {
        return Vec::new();
    };
    let src_bpp = (ex.src_bpp / 8) as usize; // source bytes per pixel
    let mut total_pixels = 0usize;
    for m in 0..meta.mip_count {
        let w = (meta.width >> m).max(1) as usize;
        let h = (meta.height >> m).max(1) as usize;
        total_pixels += w * h;
    }
    let mut out = Vec::with_capacity(total_pixels * 4);
    let mut src_off = meta.data_offset;
    for m in 0..meta.mip_count {
        let w = (meta.width >> m).max(1) as usize;
        let h = (meta.height >> m).max(1) as usize;
        for _ in 0..(w * h) {
            let mut val = 0u32;
            for b in 0..src_bpp {
                val |= (data.get(src_off + b).copied().unwrap_or(0) as u32) << (8 * b);
            }
            src_off += src_bpp;
            out.push(scale_channel(val, ex.r_mask));
            out.push(scale_channel(val, ex.g_mask));
            out.push(scale_channel(val, ex.b_mask));
            out.push(if ex.a_mask != 0 {
                scale_channel(val, ex.a_mask)
            } else {
                255
            });
        }
    }
    out
}

/// The R8G8B8A8-ready pixel bytes for `meta` (all mips): the expanded
/// buffer for a 16/24-bpp `DDPF_RGB` source (#1542), or a zero-copy borrow
/// of `data[data_offset..]` for every directly-uploadable format. Every
/// DDS upload site funnels through here so the expansion can't be missed
/// on one path.
pub fn upload_pixels<'a>(meta: &DdsMetadata, data: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
    if meta.expand.is_some() {
        std::borrow::Cow::Owned(expand_uncompressed_rgb(meta, data))
    } else {
        std::borrow::Cow::Borrowed(&data[meta.data_offset..])
    }
}

/// Extract the channel selected by `mask` from a packed pixel value and
/// bit-expand it to a full 8-bit range (e.g. a 5-bit `0x1F` → `255`,
/// a 1-bit `0x1` → `255`). A zero mask reads as `0`.
fn scale_channel(val: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift; // largest raw value the channel can hold
    let raw = (val & mask) >> shift;
    // Round-to-nearest expansion of [0, max] onto [0, 255].
    ((raw * 255 + max / 2) / max) as u8
}

/// Compute byte size of a single mip level.
///
/// For block-compressed: dimensions are rounded up to block boundaries (4×4).
/// For uncompressed: width × height × bytes_per_pixel.
pub fn mip_size(width: u32, height: u32, mip_level: u32, block_size: u32, compressed: bool) -> u32 {
    let w = (width >> mip_level).max(1);
    let h = (height >> mip_level).max(1);
    if compressed {
        let blocks_x = w.div_ceil(4);
        let blocks_y = h.div_ceil(4);
        blocks_x * blocks_y * block_size
    } else {
        w * h * block_size
    }
}

/// Total byte size of all mip levels.
pub fn total_data_size(meta: &DdsMetadata) -> u64 {
    let mut total = 0u64;
    for mip in 0..meta.mip_count {
        total += mip_size(
            meta.width,
            meta.height,
            mip,
            meta.block_size,
            meta.compressed,
        ) as u64;
    }
    total
}

fn map_fourcc(fourcc: u32) -> Result<(vk::Format, u32)> {
    match fourcc {
        // BC1/DXT1 carries an optional 1-bit punch-through alpha (the
        // "color0 <= color1" 3-colour block mode encodes index-3 as
        // transparent). Decode as BC1_RGBA so that bit reaches the shader:
        // FO4 alpha-test cutout textures (groundtrash, leaf/vine cards,
        // grates) are authored as BC1 with 1-bit alpha, and BC1_RGB samples
        // `.a == 1.0` everywhere — the alpha test never discards and the
        // whole opaque quad renders. RGB is byte-identical between the two
        // formats (same endpoints, same 4-colour blocks), so meshes that
        // don't alpha-test/blend are visually unchanged.
        FOURCC_DXT1 => Ok((vk::Format::BC1_RGBA_SRGB_BLOCK, 8)),
        FOURCC_DXT3 => Ok((vk::Format::BC2_SRGB_BLOCK, 16)),
        FOURCC_DXT5 => Ok((vk::Format::BC3_SRGB_BLOCK, 16)),
        FOURCC_ATI2 | FOURCC_BC5S => Ok((vk::Format::BC5_UNORM_BLOCK, 16)),
        _ => {
            let bytes = fourcc.to_le_bytes();
            bail!(
                "Unsupported DDS FourCC: {:?} ({:#010x})",
                std::str::from_utf8(&bytes).unwrap_or("????"),
                fourcc
            );
        }
    }
}

fn map_dxgi_format(dxgi: u32) -> Result<(vk::Format, u32, bool)> {
    match dxgi {
        // ── Uncompressed ─────────────────────────────────────────────────────
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => {
            Ok((vk::Format::R8G8B8A8_SRGB, 4, false))
        }
        // FO4 normal maps ship as B8G8R8A8_UNORM (ba2.rs:808). No special
        // Vulkan feature required — universally supported on Vulkan 1.0 desktop.
        DXGI_FORMAT_B8G8R8A8_UNORM => Ok((vk::Format::B8G8R8A8_UNORM, 4, false)),
        // B8G8R8X8_UNORM (88): same 4-byte BGRX layout; the X channel is
        // "ignore", so read it as B8G8R8A8_UNORM (alpha sampled but unused by
        // the shader on color textures). #1595.
        DXGI_FORMAT_B8G8R8X8_UNORM => Ok((vk::Format::B8G8R8A8_UNORM, 4, false)),
        DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => Ok((vk::Format::B8G8R8A8_SRGB, 4, false)),
        // Single-channel uncompressed — heightmaps and mono masks.
        DXGI_FORMAT_R16_UNORM => Ok((vk::Format::R16_UNORM, 2, false)),
        DXGI_FORMAT_R8_UNORM => Ok((vk::Format::R8_UNORM, 1, false)),
        // ── Block-compressed ─────────────────────────────────────────────────
        // All BC formats below require the `textureCompressionBC` Vulkan feature,
        // already assumed by BC1-BC5/BC7 handling above. RTX 4070 Ti exposes it.
        // BC1 1-bit punch-through alpha — decode as BC1_RGBA (see the
        // DXT1 arm in `map_fourcc` for the full rationale). FO4 ships most
        // alpha-test cutout diffuse maps as DX10 BC1_UNORM_SRGB.
        DXGI_FORMAT_BC1_UNORM | DXGI_FORMAT_BC1_UNORM_SRGB => {
            Ok((vk::Format::BC1_RGBA_SRGB_BLOCK, 8, true))
        }
        DXGI_FORMAT_BC2_UNORM | DXGI_FORMAT_BC2_UNORM_SRGB => {
            Ok((vk::Format::BC2_SRGB_BLOCK, 16, true))
        }
        DXGI_FORMAT_BC3_UNORM | DXGI_FORMAT_BC3_UNORM_SRGB => {
            Ok((vk::Format::BC3_SRGB_BLOCK, 16, true))
        }
        DXGI_FORMAT_BC4_UNORM => Ok((vk::Format::BC4_UNORM_BLOCK, 8, true)),
        DXGI_FORMAT_BC4_SNORM => Ok((vk::Format::BC4_SNORM_BLOCK, 8, true)),
        DXGI_FORMAT_BC5_UNORM => Ok((vk::Format::BC5_UNORM_BLOCK, 16, true)),
        DXGI_FORMAT_BC5_SNORM => Ok((vk::Format::BC5_SNORM_BLOCK, 16, true)),
        // BC6H — HDR half-float env maps (Starfield). Signed and unsigned variants.
        // Requires `textureCompressionBC` (same as BC1-BC7).
        DXGI_FORMAT_BC6H_UF16 => Ok((vk::Format::BC6H_UFLOAT_BLOCK, 16, true)),
        DXGI_FORMAT_BC6H_SF16 => Ok((vk::Format::BC6H_SFLOAT_BLOCK, 16, true)),
        DXGI_FORMAT_BC7_UNORM | DXGI_FORMAT_BC7_UNORM_SRGB => {
            Ok((vk::Format::BC7_SRGB_BLOCK, 16, true))
        }
        _ => bail!("Unsupported DXGI format: {}", dxgi),
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DDS_PIXELFORMAT alpha-pixels flag (test header construction only).
    const DDPF_ALPHAPIXELS: u32 = 0x1;

    /// Build a minimal valid DDS header for testing.
    fn make_dds_header(width: u32, height: u32, mip_count: u32, fourcc: &[u8; 4]) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE + 256]; // header + some fake pixel data
                                                    // Magic
        buf[0..4].copy_from_slice(b"DDS ");
        // DDS_HEADER.dwSize = 124
        buf[4..8].copy_from_slice(&124u32.to_le_bytes());
        // DDS_HEADER.dwFlags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT | DDSD_MIPMAPCOUNT
        buf[8..12].copy_from_slice(&0x0002_100Fu32.to_le_bytes());
        // height
        buf[12..16].copy_from_slice(&height.to_le_bytes());
        // width
        buf[16..20].copy_from_slice(&width.to_le_bytes());
        // mipMapCount at offset 28
        buf[28..32].copy_from_slice(&mip_count.to_le_bytes());
        // DDS_PIXELFORMAT at offset 76: dwSize=32
        buf[76..80].copy_from_slice(&32u32.to_le_bytes());
        // dwFlags = DDPF_FOURCC
        buf[80..84].copy_from_slice(&DDPF_FOURCC.to_le_bytes());
        // fourCC
        buf[84..88].copy_from_slice(fourcc);
        buf
    }

    /// Build a DDS header with the DX10 extended header for a given DXGI format.
    /// Used for the BA2 DX10 path which always emits the 148-byte extended header.
    fn make_dx10_header(width: u32, height: u32, mip_count: u32, dxgi_format: u32) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE + DX10_EXT_SIZE + 256];
        buf[0..4].copy_from_slice(b"DDS ");
        buf[4..8].copy_from_slice(&124u32.to_le_bytes()); // DDS_HEADER.dwSize
        buf[8..12].copy_from_slice(&0x0002_100Fu32.to_le_bytes()); // dwFlags
        buf[12..16].copy_from_slice(&height.to_le_bytes());
        buf[16..20].copy_from_slice(&width.to_le_bytes());
        buf[28..32].copy_from_slice(&mip_count.to_le_bytes());
        buf[76..80].copy_from_slice(&32u32.to_le_bytes()); // DDS_PIXELFORMAT.dwSize
        buf[80..84].copy_from_slice(&DDPF_FOURCC.to_le_bytes());
        buf[84..88].copy_from_slice(b"DX10"); // FourCC = "DX10"
                                              // DX10 extended header at offset 128:
        buf[128..132].copy_from_slice(&dxgi_format.to_le_bytes()); // dxgiFormat
        buf[132..136].copy_from_slice(&3u32.to_le_bytes()); // resourceDimension = TEXTURE2D
        buf[136..140].copy_from_slice(&0u32.to_le_bytes()); // miscFlag
        buf[140..144].copy_from_slice(&1u32.to_le_bytes()); // arraySize
        buf[144..148].copy_from_slice(&0u32.to_le_bytes()); // miscFlags2
        buf
    }

    fn make_uncompressed_header(width: u32, height: u32) -> Vec<u8> {
        let pixel_data_size = (width * height * 4) as usize;
        let mut buf = vec![0u8; HEADER_SIZE + pixel_data_size];
        buf[0..4].copy_from_slice(b"DDS ");
        buf[4..8].copy_from_slice(&124u32.to_le_bytes());
        buf[8..12].copy_from_slice(&0x0000_100Fu32.to_le_bytes());
        buf[12..16].copy_from_slice(&height.to_le_bytes());
        buf[16..20].copy_from_slice(&width.to_le_bytes());
        buf[28..32].copy_from_slice(&1u32.to_le_bytes());
        buf[76..80].copy_from_slice(&32u32.to_le_bytes());
        // dwFlags = DDPF_RGB | DDPF_ALPHAPIXELS
        buf[80..84].copy_from_slice(&(DDPF_RGB | DDPF_ALPHAPIXELS).to_le_bytes());
        // rgbBitCount = 32
        buf[88..92].copy_from_slice(&32u32.to_le_bytes());
        buf
    }

    #[test]
    fn parse_bc1_dxt1() {
        let data = make_dds_header(256, 256, 9, b"DXT1");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.width, 256);
        assert_eq!(meta.height, 256);
        assert_eq!(meta.mip_count, 9);
        // DXT1/BC1 decodes as BC1_RGBA so the 1-bit punch-through alpha
        // reaches the shader (FO4 alpha-test cutout diffuse maps).
        assert_eq!(meta.format, vk::Format::BC1_RGBA_SRGB_BLOCK);
        assert_eq!(meta.block_size, 8);
        assert!(meta.compressed);
        assert_eq!(meta.data_offset, 128);
    }

    #[test]
    fn parse_bc3_dxt5() {
        let data = make_dds_header(512, 512, 10, b"DXT5");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC3_SRGB_BLOCK);
        assert_eq!(meta.block_size, 16);
    }

    /// #1653 — the implicit blend-discard gate hinges on a two-part
    /// invariant that this test pins together:
    ///   1. BC1/DXT1 decodes as `BC1_RGBA` so its 1-bit punch-through
    ///      alpha reaches alpha-test cutouts (2aac5351), AND
    ///   2. `format_has_alpha` still reports `false` for that format —
    ///      BC1 punch-through is NOT an authored alpha channel.
    /// Together these keep `INSTANCE_FLAG_DIFFUSE_ALPHA` clear for BC1
    /// blend meshes (so the pure-blend implicit discard is suppressed and
    /// 3-colour-block opaque texels survive) while it stays set for
    /// BC2/BC3/BC7/RGBA (so the FNV picture/table discard is preserved).
    /// Genuine BC1 cutouts are unaffected: they discard via the explicit
    /// alpha test (`aThresh > 0`), not this gate.
    #[test]
    fn bc1_decodes_rgba_but_reports_no_authored_alpha() {
        let (fmt, _) = map_fourcc(FOURCC_DXT1).unwrap();
        assert_eq!(fmt, vk::Format::BC1_RGBA_SRGB_BLOCK);
        // The gate bit is driven by `handle_has_alpha` → `format_has_alpha`.
        assert!(
            !format_has_alpha(fmt),
            "BC1_RGBA must report no authored alpha so the #1653 gate bit \
             stays clear and BC1 blend meshes don't spuriously discard"
        );
        // The authored-alpha formats keep the discard (FNV picture/table).
        assert!(format_has_alpha(vk::Format::BC2_SRGB_BLOCK));
        assert!(format_has_alpha(vk::Format::BC3_SRGB_BLOCK));
        assert!(format_has_alpha(vk::Format::BC7_SRGB_BLOCK));
        assert!(format_has_alpha(vk::Format::R8G8B8A8_SRGB));
    }

    #[test]
    fn parse_bc5_ati2() {
        let data = make_dds_header(128, 128, 1, b"ATI2");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC5_UNORM_BLOCK);
        assert_eq!(meta.block_size, 16);
    }

    #[test]
    fn parse_uncompressed_rgba() {
        let data = make_uncompressed_header(16, 16);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.width, 16);
        assert_eq!(meta.height, 16);
        assert_eq!(meta.format, vk::Format::R8G8B8A8_SRGB);
        assert_eq!(meta.block_size, 4);
        assert!(!meta.compressed);
        assert!(meta.expand.is_none(), "32-bpp RGBA uploads zero-copy");
    }

    /// Single-mip uncompressed `DDPF_RGB` header at `bpp` with the given
    /// channel masks, followed by `pixels` of raw source bytes (#1542).
    fn make_rgb_header(
        width: u32,
        height: u32,
        bpp: u32,
        masks: [u32; 4],
        pixels: &[u8],
    ) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE + pixels.len()];
        buf[0..4].copy_from_slice(b"DDS ");
        buf[4..8].copy_from_slice(&124u32.to_le_bytes());
        buf[8..12].copy_from_slice(&0x0000_100Fu32.to_le_bytes());
        buf[12..16].copy_from_slice(&height.to_le_bytes());
        buf[16..20].copy_from_slice(&width.to_le_bytes());
        buf[28..32].copy_from_slice(&1u32.to_le_bytes());
        buf[76..80].copy_from_slice(&32u32.to_le_bytes());
        let flags = if masks[3] != 0 {
            DDPF_RGB | DDPF_ALPHAPIXELS
        } else {
            DDPF_RGB
        };
        buf[80..84].copy_from_slice(&flags.to_le_bytes());
        buf[88..92].copy_from_slice(&bpp.to_le_bytes());
        buf[92..96].copy_from_slice(&masks[0].to_le_bytes()); // R
        buf[96..100].copy_from_slice(&masks[1].to_le_bytes()); // G
        buf[100..104].copy_from_slice(&masks[2].to_le_bytes()); // B
        buf[104..108].copy_from_slice(&masks[3].to_le_bytes()); // A
        buf[HEADER_SIZE..].copy_from_slice(pixels);
        buf
    }

    #[test]
    fn parse_uncompressed_24bpp_marks_expand() {
        // 24-bpp R8G8B8 (masks like FO3's HUD compass). Parses to the
        // post-expansion R8G8B8A8 target with an `expand` descriptor.
        let data = make_rgb_header(4, 4, 24, [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0], &[]);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::R8G8B8A8_SRGB);
        assert_eq!(meta.block_size, 4);
        assert!(!meta.compressed);
        let ex = meta.expand.expect("24-bpp must set expand");
        assert_eq!(ex.src_bpp, 24);
        assert_eq!(ex.a_mask, 0);
    }

    #[test]
    fn parse_uncompressed_16bpp_a1r5g5b5_marks_expand() {
        // 16-bpp A1R5G5B5 (FO3 font glyph atlas era).
        let data = make_rgb_header(2, 2, 16, [0x7C00, 0x03E0, 0x001F, 0x8000], &[]);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::R8G8B8A8_SRGB);
        let ex = meta.expand.expect("16-bpp must set expand");
        assert_eq!(ex.src_bpp, 16);
        assert_eq!(ex.r_mask, 0x7C00);
        assert_eq!(ex.a_mask, 0x8000);
    }

    #[test]
    fn reject_uncompressed_empty_masks() {
        // DDPF_RGB with no channel masks can't be decoded — reject, not 0.
        let data = make_rgb_header(1, 1, 24, [0, 0, 0, 0], &[0, 0, 0]);
        assert!(parse_dds(&data).is_err());
    }

    #[test]
    fn expand_24bpp_pixel_to_rgba8() {
        // One 24-bpp pixel, memory order [B, G, R] per the masks.
        let data = make_rgb_header(
            1,
            1,
            24,
            [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0],
            &[0x10, 0x20, 0x30],
        );
        let meta = parse_dds(&data).unwrap();
        let rgba = expand_uncompressed_rgb(&meta, &data);
        // R=0x30, G=0x20, B=0x10, A=opaque.
        assert_eq!(rgba, vec![0x30, 0x20, 0x10, 0xFF]);
    }

    #[test]
    fn expand_16bpp_a1r5g5b5_pixel_to_rgba8() {
        // 0xFC00 = alpha bit + full red (A1R5G5B5). LE bytes [0x00, 0xFC].
        let data = make_rgb_header(1, 1, 16, [0x7C00, 0x03E0, 0x001F, 0x8000], &[0x00, 0xFC]);
        let meta = parse_dds(&data).unwrap();
        let rgba = expand_uncompressed_rgb(&meta, &data);
        // 5-bit full → 255; 1-bit alpha → 255.
        assert_eq!(rgba, vec![255, 0, 0, 255]);
    }

    #[test]
    fn scale_channel_bit_expands_to_full_range() {
        assert_eq!(scale_channel(0x1F, 0x1F), 255); // 5-bit max → 255
        assert_eq!(scale_channel(0x00, 0x1F), 0); // 5-bit min → 0
        assert_eq!(scale_channel(0x01, 0x01), 255); // 1-bit set → 255
        assert_eq!(scale_channel(0xFF00, 0xFF00), 255); // 8-bit high byte
        assert_eq!(scale_channel(0, 0), 0); // empty mask
    }

    #[test]
    fn reject_too_small() {
        let data = vec![0u8; 64];
        assert!(parse_dds(&data).is_err());
    }

    #[test]
    fn reject_bad_magic() {
        let mut data = make_dds_header(64, 64, 1, b"DXT1");
        data[0..4].copy_from_slice(b"PNG ");
        assert!(parse_dds(&data).is_err());
    }

    #[test]
    fn mip_count_zero_becomes_one() {
        let data = make_dds_header(64, 64, 0, b"DXT1");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.mip_count, 1);
    }

    #[test]
    fn mip_size_bc1() {
        // 256x256 BC1: 64x64 blocks * 8 bytes = 32768
        assert_eq!(mip_size(256, 256, 0, 8, true), 32768);
        // mip1: 128x128 = 32x32 blocks * 8 = 8192
        assert_eq!(mip_size(256, 256, 1, 8, true), 8192);
        // mip7: 2x2 -> 1x1 blocks * 8 = 8
        assert_eq!(mip_size(256, 256, 7, 8, true), 8);
        // mip8: 1x1 -> still 1x1 block * 8 = 8
        assert_eq!(mip_size(256, 256, 8, 8, true), 8);
    }

    #[test]
    fn mip_size_bc3() {
        // 512x512 BC3: 128x128 blocks * 16 = 262144
        assert_eq!(mip_size(512, 512, 0, 16, true), 262144);
        // mip1: 256x256 = 64x64 * 16 = 65536
        assert_eq!(mip_size(512, 512, 1, 16, true), 65536);
    }

    #[test]
    fn mip_size_uncompressed() {
        assert_eq!(mip_size(256, 256, 0, 4, false), 256 * 256 * 4);
        assert_eq!(mip_size(256, 256, 1, 4, false), 128 * 128 * 4);
    }

    #[test]
    fn total_data_size_bc1_256() {
        let meta = DdsMetadata {
            width: 256,
            height: 256,
            mip_count: 9,
            format: vk::Format::BC1_RGB_SRGB_BLOCK,
            block_size: 8,
            compressed: true,
            data_offset: 128,
            expand: None,
        };
        let total = total_data_size(&meta);
        // Sum: 32768 + 8192 + 2048 + 512 + 128 + 32 + 8 + 8 + 8 = 43704
        assert_eq!(total, 43704);
    }

    /// Regression for #730: `Texture::from_dds_with_mip_chain` now
    /// handles uncompressed RGBA DDS files too (pre-fix the
    /// uncompressed branch hard-coded `mip_levels(1)` and dropped the
    /// authored mip chain). The mip-aware upload path uses
    /// `total_data_size` to size the staging buffer; this test pins the
    /// byte total for a typical 256×256 RGBA mip chain so a future
    /// drift in `mip_size` for `compressed=false` surfaces here rather
    /// than as a buffer-overrun assert at runtime.
    #[test]
    fn total_data_size_rgba_256_full_mip_chain() {
        let meta = DdsMetadata {
            width: 256,
            height: 256,
            mip_count: 9, // 256 → 128 → 64 → 32 → 16 → 8 → 4 → 2 → 1
            format: vk::Format::R8G8B8A8_SRGB,
            block_size: 4, // bytes per pixel
            compressed: false,
            data_offset: 128,
            expand: None,
        };
        let total = total_data_size(&meta);
        // Geometric sum over mips:
        //   256² + 128² + 64² + 32² + 16² + 8² + 4² + 2² + 1²
        //   = 65536 + 16384 + 4096 + 1024 + 256 + 64 + 16 + 4 + 1
        //   = 87381 pixels × 4 bytes = 349_524.
        assert_eq!(total, 349_524);
    }

    // ── #1074 / FO4-D2-008 — 7 previously-unsupported DXGI formats ───────────
    //
    // Before this fix, all 7 fell through to `bail!("Unsupported DXGI format: N")`
    // and crashed texture upload from BA2 DX10 archives. Each test exercises the
    // DX10 extended-header path (BA2-extracted textures always use DX10).

    #[test]
    fn dxgi_b8g8r8a8_unorm_maps_correctly() {
        // FO4 normal maps commonly ship as B8G8R8A8_UNORM (DXGI 87).
        let data = make_dx10_header(256, 256, 1, DXGI_FORMAT_B8G8R8A8_UNORM);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(meta.block_size, 4);
        assert!(!meta.compressed);
        assert_eq!(meta.data_offset, HEADER_SIZE + DX10_EXT_SIZE);
    }

    #[test]
    fn dxgi_b8g8r8a8_unorm_srgb_maps_correctly() {
        let data = make_dx10_header(128, 128, 1, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::B8G8R8A8_SRGB);
        assert_eq!(meta.block_size, 4);
        assert!(!meta.compressed);
    }

    #[test]
    fn dxgi_b8g8r8x8_unorm_maps_to_bgra8() {
        // B8G8R8X8_UNORM (DXGI 88) — BGRX, X ignored. Same 4-byte layout as
        // B8G8R8A8_UNORM, which it reads as. UFO4P + mods. #1595.
        let data = make_dx10_header(256, 256, 1, DXGI_FORMAT_B8G8R8X8_UNORM);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(meta.block_size, 4);
        assert!(!meta.compressed);
    }

    #[test]
    fn dxgi_r16_unorm_maps_correctly() {
        // Heightmaps and single-channel 16-bit masks.
        let data = make_dx10_header(512, 512, 1, DXGI_FORMAT_R16_UNORM);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::R16_UNORM);
        assert_eq!(meta.block_size, 2);
        assert!(!meta.compressed);
    }

    #[test]
    fn dxgi_r8_unorm_maps_correctly() {
        // Single-channel 8-bit masks.
        let data = make_dx10_header(64, 64, 1, DXGI_FORMAT_R8_UNORM);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::R8_UNORM);
        assert_eq!(meta.block_size, 1);
        assert!(!meta.compressed);
    }

    #[test]
    fn dxgi_bc4_snorm_maps_correctly() {
        // Signed single-channel BC4 — used for signed normal map channels.
        let data = make_dx10_header(256, 256, 1, DXGI_FORMAT_BC4_SNORM);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC4_SNORM_BLOCK);
        assert_eq!(meta.block_size, 8);
        assert!(meta.compressed);
        // Verify mip_size uses BC block layout (same as BC4_UNORM).
        assert_eq!(
            mip_size(256, 256, 0, meta.block_size, meta.compressed),
            32768
        );
    }

    #[test]
    fn dxgi_bc6h_uf16_maps_correctly() {
        // Starfield HDR environment maps — unsigned half-float.
        let data = make_dx10_header(128, 128, 1, DXGI_FORMAT_BC6H_UF16);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC6H_UFLOAT_BLOCK);
        assert_eq!(meta.block_size, 16);
        assert!(meta.compressed);
        // 128×128 → 32×32 blocks × 16 bytes = 16384
        assert_eq!(
            mip_size(128, 128, 0, meta.block_size, meta.compressed),
            16384
        );
    }

    #[test]
    fn dxgi_bc6h_sf16_maps_correctly() {
        // Starfield HDR environment maps — signed half-float.
        let data = make_dx10_header(128, 128, 1, DXGI_FORMAT_BC6H_SF16);
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC6H_SFLOAT_BLOCK);
        assert_eq!(meta.block_size, 16);
        assert!(meta.compressed);
    }

    fn approx(a: [f32; 3], b: [f32; 3]) {
        for i in 0..3 {
            assert!(
                (a[i] - b[i]).abs() < 1e-3,
                "channel {i}: {} vs {}",
                a[i],
                b[i]
            );
        }
    }

    #[test]
    fn average_rgb_bc1_endpoint_mean() {
        // 4×4 single-mip BC1 block: endpoint0 = pure red (RGB565 0xF800),
        // endpoint1 = pure blue (0x001F). The texel-mean averages the two
        // endpoints → [0.5, 0, 0.5].
        let mut data = make_dds_header(4, 4, 1, b"DXT1");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.data_offset, 128);
        data[128..130].copy_from_slice(&0xF800u16.to_le_bytes()); // color0 = red
        data[130..132].copy_from_slice(&0x001Fu16.to_le_bytes()); // color1 = blue
        approx(average_rgb(&meta, &data).unwrap(), [0.5, 0.0, 0.5]);
    }

    #[test]
    fn average_rgb_uncompressed_rgba() {
        // 2×2 RGBA8: red, green, blue, white → per-channel mean 0.5.
        let mut data = make_uncompressed_header(2, 2);
        let meta = parse_dds(&data).unwrap();
        assert!(!meta.compressed);
        let px = [
            [255u8, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ];
        for (i, p) in px.iter().enumerate() {
            data[128 + i * 4..132 + i * 4].copy_from_slice(p);
        }
        approx(average_rgb(&meta, &data).unwrap(), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn average_rgb_bc5_normal_map_is_none() {
        // BC5 is a two-channel normal map, not a diffuse-colour format —
        // the GI bounce must keep the material tint, not fold in garbage.
        let data = make_dds_header(4, 4, 1, b"ATI2");
        let meta = parse_dds(&data).unwrap();
        assert_eq!(meta.format, vk::Format::BC5_UNORM_BLOCK);
        assert!(average_rgb(&meta, &data).is_none());
    }
}
