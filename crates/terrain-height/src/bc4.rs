//! BC4 encoding and the DX10 DDS container for a height-map mip chain. Decoding
//! is `menuxml::tex`'s (the workspace's CPU DDS codec) — see
//! [`crate::HeightField::from_red_channel`].

use crate::field::HeightField;

/// `DXGI_FORMAT_BC4_UNORM`.
const DXGI_FORMAT_BC4_UNORM: u32 = 80;

/// Encode one 4×4 block of `[0, 1]` values (row-major, 16 entries) as BC4.
///
/// Uses the eight-value mode (`a0 > a1`) with the block's min and max as
/// endpoints and picks the nearest of the eight levels per texel — the
/// straightforward encoder. A flat block stores equal endpoints, where every
/// selector decodes to the same value.
pub fn encode_block(values: &[f32; 16]) -> [u8; 8] {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let lo = values.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let (a0, a1) = (q(hi), q(lo));
    let mut out = [0u8; 8];
    out[0] = a0;
    out[1] = a1;
    if a0 == a1 {
        return out;
    }
    // Palette in selector order: 0 → a0, 1 → a1, 2..7 → (8-s)/7·a0 + (s-1)/7·a1.
    let mut palette = [0f32; 8];
    palette[0] = a0 as f32;
    palette[1] = a1 as f32;
    for s in 2..8u16 {
        palette[s as usize] = ((a0 as u16 * (8 - s) + a1 as u16 * (s - 1)) / 7) as f32;
    }
    let mut bits = 0u64;
    for (i, &v) in values.iter().enumerate() {
        let target = v.clamp(0.0, 1.0) * 255.0;
        let sel = (0..8)
            .min_by(|&a, &b| {
                (palette[a] - target)
                    .abs()
                    .total_cmp(&(palette[b] - target).abs())
            })
            .unwrap() as u64;
        bits |= sel << (i * 3);
    }
    out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    out
}

/// BC4 payload for one mip level. Blocks overhanging the edge of a level
/// smaller than 4 texels repeat its texels with wrap addressing.
pub fn encode_level(level: &HeightField) -> Vec<u8> {
    let bw = level.width.div_ceil(4);
    let bh = level.height.div_ceil(4);
    let mut out = Vec::with_capacity(bw * bh * 8);
    for by in 0..bh {
        for bx in 0..bw {
            let mut block = [0f32; 16];
            for (i, v) in block.iter_mut().enumerate() {
                *v = level.at((bx * 4 + i % 4) as isize, (by * 4 + i / 4) as isize);
            }
            out.extend_from_slice(&encode_block(&block));
        }
    }
    out
}

/// A complete `.dds` (classic header + DX10 extension, `BC4_UNORM`) holding
/// `mips`, largest first.
pub fn write_dds(mips: &[HeightField]) -> Vec<u8> {
    let top = &mips[0];
    let top_size = top.width.div_ceil(4) * top.height.div_ceil(4) * 8;
    let mut out = Vec::with_capacity(4 + 124 + 20 + top_size * 4 / 3 + 64);
    let u32le = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());

    out.extend_from_slice(b"DDS ");
    u32le(&mut out, 124); // dwSize
                          // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT | LINEARSIZE
    u32le(&mut out, 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000 | 0x8_0000);
    u32le(&mut out, top.height as u32);
    u32le(&mut out, top.width as u32);
    u32le(&mut out, top_size as u32); // dwPitchOrLinearSize
    u32le(&mut out, 0); // dwDepth
    u32le(&mut out, mips.len() as u32);
    for _ in 0..11 {
        u32le(&mut out, 0); // dwReserved1
    }
    // DDS_PIXELFORMAT
    u32le(&mut out, 32);
    u32le(&mut out, 0x4); // DDPF_FOURCC
    out.extend_from_slice(b"DX10");
    for _ in 0..5 {
        u32le(&mut out, 0);
    }
    // CAPS_COMPLEX | CAPS_TEXTURE | CAPS_MIPMAP
    u32le(&mut out, 0x8 | 0x1000 | 0x40_0000);
    for _ in 0..4 {
        u32le(&mut out, 0); // dwCaps2..4, dwReserved2
    }
    // DDS_HEADER_DXT10
    u32le(&mut out, DXGI_FORMAT_BC4_UNORM);
    u32le(&mut out, 3); // D3D10_RESOURCE_DIMENSION_TEXTURE2D
    u32le(&mut out, 0); // miscFlag
    u32le(&mut out, 1); // arraySize
    u32le(&mut out, 0); // miscFlags2
    for level in mips {
        out.extend_from_slice(&encode_level(level));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode one block through the workspace's CPU DDS codec (`menuxml::tex`),
    /// wrapped in a 4×4 single-mip BC4 file: this crate keeps only the encoder.
    fn decode_block(block: [u8; 8]) -> [u8; 16] {
        let dds = write_dds(&[HeightField::new(4, 4)]);
        let mut dds = dds[..148].to_vec();
        dds.extend_from_slice(&block);
        let image = byroredux_menuxml::tex::Rgba8::decode_dds(&dds).expect("a 4x4 BC4 file decodes");
        std::array::from_fn(|i| image.pixels[i * 4])
    }

    #[test]
    fn block_round_trip_error_is_within_one_palette_step() {
        let values: [f32; 16] = std::array::from_fn(|i| (i as f32 * 0.37).sin() * 0.4 + 0.5);
        let decoded = decode_block(encode_block(&values));
        let lo = values.iter().copied().fold(1.0f32, f32::min);
        let hi = values.iter().copied().fold(0.0f32, f32::max);
        let step = (hi - lo) * 255.0 / 7.0;
        for (v, d) in values.iter().zip(decoded) {
            assert!((v * 255.0 - d as f32).abs() <= step / 2.0 + 1.0);
        }
        let flat = decode_block(encode_block(&[0.25; 16]));
        assert!(flat.iter().all(|&v| v == 64));
    }

    #[test]
    fn dds_carries_every_mip_after_the_dx10_header() {
        let mips = HeightField::new(8, 4).mip_chain();
        let dds = write_dds(&mips);
        assert_eq!(&dds[0..4], b"DDS ");
        assert_eq!(&dds[84..88], b"DX10");
        assert_eq!(u32::from_le_bytes(dds[28..32].try_into().unwrap()), 4); // mip count
        assert_eq!(u32::from_le_bytes(dds[128..132].try_into().unwrap()), 80);
        // 8×4 → 2 blocks, 4×2 / 2×1 / 1×1 → 1 block each.
        assert_eq!(dds.len(), 148 + (2 + 1 + 1 + 1) * 8);
    }
}
