//! NiSourceTexture — texture file reference.
//! NiPixelData — embedded pixel data (used by some Oblivion NIFs).
//! NiTextureEffect — projected texture effect (env map, gobo, fog).

use super::base::{NiAVObjectData, NiObjectNETData};
use super::traits::{HasAVObject, HasObjectNET};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiMatrix3, NiTransform};
use crate::version::NifVersion;
use std::any::Any;
use std::io;
use std::sync::Arc;

/// Reference to an external texture file or embedded pixel data.
#[derive(Debug)]
pub struct NiSourceTexture {
    pub net: NiObjectNETData,
    pub use_external: bool,
    pub filename: Option<Arc<str>>,
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
            let fname: Option<Arc<str>> = if use_string_table {
                stream.read_string()?
            } else {
                Some(Arc::from(stream.read_sized_string()?))
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
        // is_static only present in v >= 5.0.0.1 (not in Morrowind-era NIFs).
        let is_static = if stream.version() >= NifVersion(0x05000001) {
            stream.read_u8()? != 0
        } else {
            true
        };

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
                mipmaps.push(MipMapInfo {
                    width,
                    height,
                    offset,
                });
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
            mipmaps.push(MipMapInfo {
                width,
                height,
                offset,
            });
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
        data.extend_from_slice(&[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 128, 128, 128, 255,
        ]);

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

// ── NiTextureEffect ────────────────────────────────────────────────────
//
// Inherits NiDynamicEffect (which in turn inherits NiAVObject). Describes
// a projected texture — sphere/env maps, gobos, fog maps, projected
// shadows. Used by Oblivion magic FX meshes and various projected-shadow
// setups. See issue #163.
//
// Wire layout (up to Skyrim — FO4 removes NiDynamicEffect from the chain):
//
//   NiAVObject base
//   [NiDynamicEffect] switch_state:u8 (since 10.1.0.106, < BSVER 130)
//                     num_affected_nodes:u32 (since 10.1.0.0, < BSVER 130)
//                     affected_nodes:u32[n]
//   model_projection_matrix: Matrix33
//   model_projection_translation: Vector3
//   texture_filtering: u32 (TexFilterMode enum)
//   max_anisotropy: u16 (since 20.5.0.4)
//   texture_clamping: u32 (TexClampMode enum)
//   texture_type: u32 (TextureType enum)
//   coordinate_generation_type: u32 (CoordGenType enum)
//   source_texture_ref: Ref<NiSourceTexture> (since 3.1 — always for us)
//   enable_plane: u8 (byte bool)
//   plane: NiPlane { normal:Vec3, constant:f32 } = 16 bytes
//   ps2_l: i16 (until 10.2.0.0 — present in Oblivion v20.0.0.5... wait,
//              nif.xml says "until 10.2.0.0"; Oblivion is 20.0.0.5 which is
//              AFTER that, so PS2 fields are ABSENT for Oblivion)
//   ps2_k: i16 (until 10.2.0.0 — same)

/// NiTextureEffect — projected texture effect (env map, gobo, fog, etc.).
#[derive(Debug)]
pub struct NiTextureEffect {
    pub av: NiAVObjectData,
    pub switch_state: bool,
    pub affected_nodes: Vec<u32>,
    pub model_projection_matrix: NiMatrix3,
    pub model_projection_translation: [f32; 3],
    pub texture_filtering: u32,
    pub max_anisotropy: u16,
    pub texture_clamping: u32,
    pub texture_type: u32,
    pub coordinate_generation_type: u32,
    pub source_texture_ref: BlockRef,
    pub enable_plane: bool,
    /// Clipping plane: (normal_x, normal_y, normal_z, constant).
    pub plane: [f32; 4],
    pub ps2_l: i16,
    pub ps2_k: i16,
}

impl NiObject for NiTextureEffect {
    fn block_type_name(&self) -> &'static str {
        "NiTextureEffect"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(self)
    }
}

impl HasObjectNET for NiTextureEffect {
    fn name(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.av.net.controller_ref
    }
}

impl HasAVObject for NiTextureEffect {
    fn flags(&self) -> u32 {
        self.av.flags
    }
    fn transform(&self) -> &NiTransform {
        &self.av.transform
    }
    fn properties(&self) -> &[BlockRef] {
        &self.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.av.collision_ref
    }
}

impl NiTextureEffect {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiDynamicEffect base fields — same version gates as NiLight.
        // See crates/nif/src/blocks/light.rs for the full rationale.
        let switch_state = if stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u8()? != 0
        } else {
            true
        };
        let affected_nodes = if stream.version() >= NifVersion(0x0A010000) {
            let count = stream.read_u32_le()? as usize;
            let mut nodes = Vec::with_capacity(count);
            for _ in 0..count {
                nodes.push(stream.read_u32_le()?);
            }
            nodes
        } else {
            Vec::new()
        };

        let model_projection_matrix = stream.read_ni_matrix3()?;
        let p = stream.read_ni_point3()?;
        let model_projection_translation = [p.x, p.y, p.z];

        let texture_filtering = stream.read_u32_le()?;
        let max_anisotropy = if stream.version() >= NifVersion(0x14050004) {
            stream.read_u16_le()?
        } else {
            0
        };
        let texture_clamping = stream.read_u32_le()?;
        let texture_type = stream.read_u32_le()?;
        let coordinate_generation_type = stream.read_u32_le()?;
        let source_texture_ref = stream.read_block_ref()?;

        let enable_plane = stream.read_u8()? != 0;
        // NiPlane: vec3 normal + f32 constant = 16 bytes.
        let pn = stream.read_ni_point3()?;
        let pc = stream.read_f32_le()?;
        let plane = [pn.x, pn.y, pn.z, pc];

        // PS2 L/K: only present up to and including 10.2.0.0. Oblivion
        // is 20.0.0.5 — AFTER 10.2.0.0 — so these fields are ABSENT.
        let (ps2_l, ps2_k) = if stream.version() <= NifVersion(0x0A020000) {
            // No i16 reader in NifStream; sign-reinterpret the u16.
            let l = stream.read_u16_le()? as i16;
            let k = stream.read_u16_le()? as i16;
            (l, k)
        } else {
            (0, 0)
        };

        Ok(Self {
            av,
            switch_state,
            affected_nodes,
            model_projection_matrix,
            model_projection_translation,
            texture_filtering,
            max_anisotropy,
            texture_clamping,
            texture_type,
            coordinate_generation_type,
            source_texture_ref,
            enable_plane,
            plane,
            ps2_l,
            ps2_k,
        })
    }
}
