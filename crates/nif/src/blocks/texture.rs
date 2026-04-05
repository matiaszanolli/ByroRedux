//! NiSourceTexture — texture file reference.
//! NiPixelData — embedded pixel data (used by some Oblivion NIFs).

use super::base::NiObjectNETData;
use super::NiObject;
use crate::stream::NifStream;
use crate::types::BlockRef;
use crate::version::NifVersion;
use std::any::Any;
use std::io;

/// Reference to an external texture file or embedded pixel data.
#[derive(Debug)]
pub struct NiSourceTexture {
    pub net: NiObjectNETData,
    pub use_external: bool,
    pub filename: Option<String>,
    pub pixel_data_ref: BlockRef,
    pub pixel_layout: u32,
    pub use_mipmaps: u32,
    pub alpha_format: u32,
    pub is_static: bool,
}

impl NiObject for NiSourceTexture {
    fn block_type_name(&self) -> &'static str {
        "NiSourceTexture"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiSourceTexture {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        let use_external = stream.read_u8()? != 0;
        let use_string_table = stream.version() >= crate::version::NifVersion::V20_2_0_7;

        let (filename, pixel_data_ref) = if use_external {
            let fname = if use_string_table {
                stream.read_string()?
            } else {
                Some(stream.read_sized_string()?)
            };
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                let _unknown_ref = stream.read_block_ref()?;
            }
            (fname, BlockRef::NULL)
        } else {
            if stream.version() >= crate::version::NifVersion(0x0A010000) {
                if use_string_table {
                    let _unknown = stream.read_string()?;
                } else {
                    let _unknown = stream.read_sized_string()?;
                }
            }
            let pix_ref = stream.read_block_ref()?;
            (None, pix_ref)
        };

        let pixel_layout = stream.read_u32_le()?;
        let use_mipmaps = stream.read_u32_le()?;
        let alpha_format = stream.read_u32_le()?;
        let is_static = stream.read_u8()? != 0;

        // nif.xml: Direct Render since 10.1.0.103 (0x0A010067), NOT 10.1.0.6.
        if stream.version() >= crate::version::NifVersion(0x0A010067) {
            let _direct_render = stream.read_byte_bool()?;
        }

        if stream.version() >= crate::version::NifVersion::V20_2_0_7 {
            let _persist_render_data = stream.read_byte_bool()?;
        }

        Ok(Self {
            net,
            use_external,
            filename,
            pixel_data_ref,
            pixel_layout,
            use_mipmaps,
            alpha_format,
            is_static,
        })
    }
}

// ── NiPixelData ────────────────────────────────────────────────────

/// Pixel format channel descriptor (4 per NiPixelFormat).
#[derive(Debug, Clone)]
pub struct PixelFormatComponent {
    pub component_type: u32,
    pub convention: u32,
    pub bits_per_channel: u8,
    pub is_signed: bool,
}

/// Mipmap level descriptor.
#[derive(Debug, Clone)]
pub struct MipMapInfo {
    pub width: u32,
    pub height: u32,
    pub offset: u32,
}

/// Embedded pixel data block — inlines texture pixels directly in the NIF.
///
/// Uncommon but occurs in some Oblivion NIFs where textures are baked in.
/// The pixel format fields (NiPixelFormat) are read inline at the start,
/// followed by mipmap descriptors and the raw pixel bytes.
#[derive(Debug)]
pub struct NiPixelData {
    /// Pixel format enum (0=RGB, 1=RGBA, etc.)
    pub pixel_format: u32,
    pub bits_per_pixel: u8,
    pub renderer_hint: u32,
    pub extra_data: u32,
    pub flags: u8,
    pub tiling: u32,
    pub channels: [PixelFormatComponent; 4],
    /// Reference to NiPalette (usually -1/NULL).
    pub palette_ref: BlockRef,
    pub num_mipmaps: u32,
    pub bytes_per_pixel: u32,
    pub mipmaps: Vec<MipMapInfo>,
    pub num_faces: u32,
    /// Raw pixel data (all mipmaps, all faces, contiguous).
    pub pixel_data: Vec<u8>,
}

impl NiObject for NiPixelData {
    fn block_type_name(&self) -> &'static str {
        "NiPixelData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPixelData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiPixelFormat fields (inline, not inherited).
        let pixel_format = stream.read_u32_le()?;

        // Version split at 10.4.0.2 — Oblivion/FO3+ use the "new" layout.
        let old_layout = stream.version() < NifVersion(0x0A040002);

        if old_layout {
            // Pre-10.4.0.2: color masks, old bits per pixel, fast compare, tiling.
            let _red_mask = stream.read_u32_le()?;
            let _green_mask = stream.read_u32_le()?;
            let _blue_mask = stream.read_u32_le()?;
            let _alpha_mask = stream.read_u32_le()?;
            let bits_per_pixel_u32 = stream.read_u32_le()?;
            let _fast_compare = stream.read_bytes(8)?;

            let tiling = if stream.version() >= NifVersion(0x0A010000) {
                stream.read_u32_le()?
            } else {
                0
            };

            // Old layout NiPixelData fields
            let palette_ref = stream.read_block_ref()?;
            let num_mipmaps = stream.read_u32_le()?;
            let bytes_per_pixel = stream.read_u32_le()?;
            let mut mipmaps = Vec::with_capacity(num_mipmaps as usize);
            for _ in 0..num_mipmaps {
                let width = stream.read_u32_le()?;
                let height = stream.read_u32_le()?;
                let offset = stream.read_u32_le()?;
                mipmaps.push(MipMapInfo { width, height, offset });
            }
            let num_pixels = stream.read_u32_le()? as usize;
            let pixel_data = stream.read_bytes(num_pixels)?;

            let default_channel = PixelFormatComponent {
                component_type: 0,
                convention: 0,
                bits_per_channel: 0,
                is_signed: false,
            };

            return Ok(Self {
                pixel_format,
                bits_per_pixel: bits_per_pixel_u32 as u8,
                renderer_hint: 0,
                extra_data: 0,
                flags: 0,
                tiling,
                channels: [
                    default_channel.clone(),
                    default_channel.clone(),
                    default_channel.clone(),
                    default_channel,
                ],
                palette_ref,
                num_mipmaps,
                bytes_per_pixel,
                mipmaps,
                num_faces: 1,
                pixel_data,
            });
        }

        // New layout (10.4.0.2+, covers Oblivion and FO3+).
        let bits_per_pixel = stream.read_u8()?;
        let renderer_hint = stream.read_u32_le()?;
        let extra_data = stream.read_u32_le()?;
        let flags = stream.read_u8()?;
        let tiling = stream.read_u32_le()?;

        // sRGB Space — only since 20.3.0.4 (NOT Oblivion, NOT FO3).
        if stream.version() >= NifVersion(0x14030004) {
            let _srgb = stream.read_byte_bool()?;
        }

        // 4 pixel format channels.
        let mut channels = Vec::with_capacity(4);
        for _ in 0..4 {
            let component_type = stream.read_u32_le()?;
            let convention = stream.read_u32_le()?;
            let bits_per_channel = stream.read_u8()?;
            let is_signed = stream.read_byte_bool()?;
            channels.push(PixelFormatComponent {
                component_type,
                convention,
                bits_per_channel,
                is_signed,
            });
        }
        let channels_arr = [
            channels[0].clone(),
            channels[1].clone(),
            channels[2].clone(),
            channels[3].clone(),
        ];

        // NiPixelData fields.
        let palette_ref = stream.read_block_ref()?;
        let num_mipmaps = stream.read_u32_le()?;
        let bytes_per_pixel = stream.read_u32_le()?;

        let mut mipmaps = Vec::with_capacity(num_mipmaps as usize);
        for _ in 0..num_mipmaps {
            let width = stream.read_u32_le()?;
            let height = stream.read_u32_le()?;
            let offset = stream.read_u32_le()?;
            mipmaps.push(MipMapInfo { width, height, offset });
        }

        let num_pixels = stream.read_u32_le()? as usize;
        let num_faces = stream.read_u32_le()?;
        let total_bytes = num_pixels * num_faces as usize;
        let pixel_data = stream.read_bytes(total_bytes)?;

        Ok(Self {
            pixel_format,
            bits_per_pixel,
            renderer_hint,
            extra_data,
            flags,
            tiling,
            channels: channels_arr,
            palette_ref,
            num_mipmaps,
            bytes_per_pixel,
            mipmaps,
            num_faces,
            pixel_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;

    fn make_oblivion_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    #[test]
    fn parse_ni_pixel_data_oblivion() {
        let header = make_oblivion_header();
        let mut data = Vec::new();

        // NiPixelFormat: pixel_format
        data.extend_from_slice(&1u32.to_le_bytes()); // RGBA
        // New layout (v20.0.0.5 >= 10.4.0.2): bits_per_pixel(u8), renderer_hint(u32),
        // extra_data(u32), flags(u8), tiling(u32)
        data.push(32u8); // bits_per_pixel
        data.extend_from_slice(&0u32.to_le_bytes()); // renderer_hint
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data
        data.push(0u8); // flags
        data.extend_from_slice(&0u32.to_le_bytes()); // tiling
        // No sRGB (v20.0.0.5 < 20.3.0.4)
        // 4 channels: each is (type:u32, convention:u32, bits:u8, signed:bool=u8)
        for _ in 0..4 {
            data.extend_from_slice(&0u32.to_le_bytes()); // component type
            data.extend_from_slice(&0u32.to_le_bytes()); // convention
            data.push(8u8); // bits per channel
            data.push(0u8); // is_signed (bool as u8 via read_byte_bool)
        }
        // NiPixelData fields
        data.extend_from_slice(&(-1i32).to_le_bytes()); // palette_ref (NULL)
        data.extend_from_slice(&1u32.to_le_bytes()); // num_mipmaps
        data.extend_from_slice(&4u32.to_le_bytes()); // bytes_per_pixel
        // MipMap[0]: width, height, offset
        data.extend_from_slice(&2u32.to_le_bytes()); // width
        data.extend_from_slice(&2u32.to_le_bytes()); // height
        data.extend_from_slice(&0u32.to_le_bytes()); // offset
        // num_pixels (total bytes)
        data.extend_from_slice(&16u32.to_le_bytes()); // 2×2×4 = 16 bytes
        // num_faces
        data.extend_from_slice(&1u32.to_le_bytes());
        // pixel_data: 16 bytes of RGBA
        data.extend_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255]);

        let mut stream = NifStream::new(&data, &header);
        let pix = NiPixelData::parse(&mut stream).unwrap();

        assert_eq!(pix.pixel_format, 1); // RGBA
        assert_eq!(pix.bits_per_pixel, 32);
        assert_eq!(pix.num_mipmaps, 1);
        assert_eq!(pix.bytes_per_pixel, 4);
        assert_eq!(pix.mipmaps.len(), 1);
        assert_eq!(pix.mipmaps[0].width, 2);
        assert_eq!(pix.mipmaps[0].height, 2);
        assert_eq!(pix.num_faces, 1);
        assert_eq!(pix.pixel_data.len(), 16);
        assert_eq!(pix.pixel_data[0], 255); // first pixel R
        assert_eq!(stream.position() as usize, data.len());
    }
}
