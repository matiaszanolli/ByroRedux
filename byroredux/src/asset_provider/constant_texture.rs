//! A single-channel material Constant as a texture (SMM-GPU-001; the role
//! contract in `docs/engine/nifal.md`).
//!
//! A Constant for opacity travels as a generated 1×1 texture holding the
//! value, bound in the role's lane where a Map would be, so no GPU record or
//! draw field exists for it. The texture has a reserved path: it resolves,
//! caches, reference-counts and unloads like any other, with one entry per
//! distinct 8-bit value. [`super::TextureProvider::extract`] builds the file
//! instead of reading an archive.

/// Reserved path stem. It is already in canonical texture-key form.
const PREFIX: &str = "textures\\byro_constant\\r8_";
const SUFFIX: &str = ".dds";

/// The path that holds `value` in its red channel.
pub(crate) fn constant_texture_path(value: u8) -> String {
    format!("{PREFIX}{value}{SUFFIX}")
}

/// The value of a [`constant_texture_path`], or `None` for any other path.
pub(crate) fn constant_texture_value(path: &str) -> Option<u8> {
    path.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.parse().ok()
}

/// A 1×1 `R8G8B8A8_UNORM` DDS holding `value`. Linear, so a sample returns
/// `value / 255` exactly (SMM-SRC-004 reads a replacement as linear).
pub(crate) fn constant_texture_dds(value: u8) -> Vec<u8> {
    const DDSD_CAPS_HEIGHT_WIDTH_PITCH_PIXELFORMAT: u32 = 0x1 | 0x2 | 0x4 | 0x8 | 0x1000;
    const DDPF_FOURCC: u32 = 0x4;
    const DDSCAPS_TEXTURE: u32 = 0x1000;
    const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
    const D3D10_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;

    // The 128-byte DDS header, its 20-byte DX10 extension, and one texel.
    let mut file = vec![0u8; 148];
    let mut put = |offset: usize, bytes: [u8; 4]| {
        file[offset..offset + 4].copy_from_slice(&bytes);
    };
    put(0, *b"DDS ");
    put(4, 124u32.to_le_bytes()); // header size
    put(8, DDSD_CAPS_HEIGHT_WIDTH_PITCH_PIXELFORMAT.to_le_bytes());
    put(12, 1u32.to_le_bytes()); // height
    put(16, 1u32.to_le_bytes()); // width
    put(20, 4u32.to_le_bytes()); // pitch
    put(76, 32u32.to_le_bytes()); // pixel format size
    put(80, DDPF_FOURCC.to_le_bytes());
    put(84, *b"DX10");
    put(108, DDSCAPS_TEXTURE.to_le_bytes());
    put(128, DXGI_FORMAT_R8G8B8A8_UNORM.to_le_bytes());
    put(132, D3D10_RESOURCE_DIMENSION_TEXTURE2D.to_le_bytes());
    put(140, 1u32.to_le_bytes()); // array size
    file.extend_from_slice(&[value, value, value, 255]);
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_renderer::vulkan::dds::{parse_dds_with_color_space, TextureColorSpace};

    #[test]
    fn a_constant_path_round_trips_every_value() {
        for value in 0..=u8::MAX {
            let path = constant_texture_path(value);
            assert_eq!(constant_texture_value(&path), Some(value));
            assert_eq!(super::super::canonical_texture_key(&path), path);
        }
        assert_eq!(constant_texture_value("textures\\a_opacity.dds"), None);
        assert_eq!(constant_texture_value("textures\\byro_constant\\r8_256.dds"), None);
    }

    #[test]
    fn the_generated_file_is_a_one_texel_linear_dds() {
        let file = constant_texture_dds(128);
        assert_eq!(file.len(), 148 + 4);
        let meta = parse_dds_with_color_space(&file, TextureColorSpace::AsAuthored)
            .expect("parses");
        assert_eq!((meta.width, meta.height, meta.mip_count), (1, 1, 1));
        assert_eq!(meta.format, ash::vk::Format::R8G8B8A8_UNORM);
        assert_eq!(meta.data_offset, 148);
        assert_eq!(&file[meta.data_offset..], &[128, 128, 128, 255]);
    }
}
