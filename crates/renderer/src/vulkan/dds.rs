//! Minimal DDS header parser — maps DDS pixel formats to Vulkan image formats.

use anyhow::{bail, ensure, Result};
use ash::vk;

const DDS_MAGIC: u32 = 0x20534444; // "DDS "
const HEADER_SIZE: usize = 128; // 4 magic + 124 DDS_HEADER
const DX10_EXT_SIZE: usize = 20;

// DDS_PIXELFORMAT flags
const DDPF_FOURCC: u32 = 0x4;
const DDPF_RGB: u32 = 0x40;
const DDPF_ALPHAPIXELS: u32 = 0x1;

// FourCC values
const FOURCC_DXT1: u32 = u32::from_le_bytes(*b"DXT1");
const FOURCC_DXT3: u32 = u32::from_le_bytes(*b"DXT3");
const FOURCC_DXT5: u32 = u32::from_le_bytes(*b"DXT5");
const FOURCC_ATI2: u32 = u32::from_le_bytes(*b"ATI2");
const FOURCC_BC5S: u32 = u32::from_le_bytes(*b"BC5S");
const FOURCC_DX10: u32 = u32::from_le_bytes(*b"DX10");

// DXGI format codes (subset we care about)
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const DXGI_FORMAT_BC1_UNORM_SRGB: u32 = 72;
const DXGI_FORMAT_BC2_UNORM: u32 = 74;
const DXGI_FORMAT_BC2_UNORM_SRGB: u32 = 75;
const DXGI_FORMAT_BC3_UNORM: u32 = 77;
const DXGI_FORMAT_BC3_UNORM_SRGB: u32 = 78;
const DXGI_FORMAT_BC4_UNORM: u32 = 80;
const DXGI_FORMAT_BC5_UNORM: u32 = 83;
const DXGI_FORMAT_BC5_SNORM: u32 = 84;
const DXGI_FORMAT_BC7_UNORM: u32 = 98;
const DXGI_FORMAT_BC7_UNORM_SRGB: u32 = 99;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;

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
            })
        }
    } else if pf_flags & DDPF_RGB != 0 {
        // Uncompressed RGBA
        let bpp = pf_rgb_bit_count;
        ensure!(
            bpp == 32,
            "Unsupported uncompressed DDS: {} bpp (only 32-bit RGBA supported)",
            bpp
        );
        let format = if pf_flags & DDPF_ALPHAPIXELS != 0 {
            vk::Format::R8G8B8A8_SRGB
        } else {
            vk::Format::R8G8B8A8_SRGB
        };
        Ok(DdsMetadata {
            width,
            height,
            mip_count,
            format,
            block_size: 4, // bytes per pixel
            compressed: false,
            data_offset: HEADER_SIZE,
        })
    } else {
        bail!("Unsupported DDS pixel format (flags={:#x})", pf_flags);
    }
}

/// Compute byte size of a single mip level.
///
/// For block-compressed: dimensions are rounded up to block boundaries (4×4).
/// For uncompressed: width × height × bytes_per_pixel.
pub fn mip_size(width: u32, height: u32, mip_level: u32, block_size: u32, compressed: bool) -> u32 {
    let w = (width >> mip_level).max(1);
    let h = (height >> mip_level).max(1);
    if compressed {
        let blocks_x = (w + 3) / 4;
        let blocks_y = (h + 3) / 4;
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
        FOURCC_DXT1 => Ok((vk::Format::BC1_RGB_SRGB_BLOCK, 8)),
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
        DXGI_FORMAT_BC1_UNORM | DXGI_FORMAT_BC1_UNORM_SRGB => {
            Ok((vk::Format::BC1_RGB_SRGB_BLOCK, 8, true))
        }
        DXGI_FORMAT_BC2_UNORM | DXGI_FORMAT_BC2_UNORM_SRGB => {
            Ok((vk::Format::BC2_SRGB_BLOCK, 16, true))
        }
        DXGI_FORMAT_BC3_UNORM | DXGI_FORMAT_BC3_UNORM_SRGB => {
            Ok((vk::Format::BC3_SRGB_BLOCK, 16, true))
        }
        DXGI_FORMAT_BC4_UNORM => Ok((vk::Format::BC4_UNORM_BLOCK, 8, true)),
        DXGI_FORMAT_BC5_UNORM => Ok((vk::Format::BC5_UNORM_BLOCK, 16, true)),
        DXGI_FORMAT_BC5_SNORM => Ok((vk::Format::BC5_SNORM_BLOCK, 16, true)),
        DXGI_FORMAT_BC7_UNORM | DXGI_FORMAT_BC7_UNORM_SRGB => {
            Ok((vk::Format::BC7_SRGB_BLOCK, 16, true))
        }
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => {
            Ok((vk::Format::R8G8B8A8_SRGB, 4, false))
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
        assert_eq!(meta.format, vk::Format::BC1_RGB_SRGB_BLOCK);
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
        };
        let total = total_data_size(&meta);
        // Geometric sum over mips:
        //   256² + 128² + 64² + 32² + 16² + 8² + 4² + 2² + 1²
        //   = 65536 + 16384 + 4096 + 1024 + 256 + 64 + 16 + 4 + 1
        //   = 87381 pixels × 4 bytes = 349_524.
        assert_eq!(total, 349_524);
    }
}
