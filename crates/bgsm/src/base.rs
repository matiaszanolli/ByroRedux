//! Common prefix shared by BGSM and BGEM — everything before the
//! subtype-specific texture list. Matches
//! `BaseMaterialFile.Deserialize` at `Material-Editor:BaseMaterialFile.cs:179`.

use crate::reader::Reader;
use crate::Result;

/// RGB color as three floats in `[0.0, 1.0]`. Serialized as 12 bytes
/// (3×f32, little-endian). The reference implementation converts to/
/// from a 24-bit packed RGB on disk but we preserve float precision.
pub type ColorRgb = [f32; 3];

/// `BaseMaterialFile.MaskWriteFlags` — 6 bit flags packed into one u8
/// (version >= 6). Controls which G-buffer channels the shader writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaskWriteFlags(pub u8);

impl MaskWriteFlags {
    pub const ALBEDO: u8 = 1;
    pub const NORMAL: u8 = 2;
    pub const SPECULAR: u8 = 4;
    pub const AMBIENT_OCCLUSION: u8 = 8;
    pub const EMISSIVE: u8 = 16;
    pub const GLOSS: u8 = 32;

    pub const ALL: u8 = Self::ALBEDO
        | Self::NORMAL
        | Self::SPECULAR
        | Self::AMBIENT_OCCLUSION
        | Self::EMISSIVE
        | Self::GLOSS;

    pub fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }
}

/// Alpha blend mode — serialized as a 3-field tuple `(u8, u32, u32)`
/// and converted to an enum by the reference impl's
/// `ConvertAlphaBlendMode`. We preserve the raw tuple so callers can
/// decode exactly what CreationKit wrote; the common "Standard" mode
/// is `(1, 6, 7)` meaning `function=1, src_blend=6, dst_blend=7`.
///
/// Values from UESP + the reference source:
/// - `function`: 0 = None, 1 = Standard, 2 = Additive, 3 = Multiplicative.
/// - `src_blend` / `dst_blend`: GL-style enum (Zero=0, One=1,
///   SrcColor=2, SrcAlpha=6, InvSrcAlpha=7, ...).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AlphaBlendMode {
    pub function: u8,
    pub src_blend: u32,
    pub dst_blend: u32,
}

/// Common header + state fields shared by BGSM (lit) and BGEM (effect).
///
/// Layout matches the ReferenceImpl `BaseMaterialFile.Deserialize` at
/// `Material-Editor:BaseMaterialFile.cs:179-234`. Field order matters —
/// the serializer at `:236-293` writes them back in this sequence.
#[derive(Debug, Clone)]
pub struct BaseMaterial {
    /// Format version — FO4 vanilla = 2, Skyrim SE = 20, FO76 = higher.
    pub version: u32,

    /// Tile flags byte — `(bit 1 << 1) = tile_v`, `(bit 0 << 1) = tile_u`
    /// per the reference (`tileFlags & 2 = TileU`, `& 1 = TileV`).
    pub tile_u: bool,
    pub tile_v: bool,

    /// UV transform. `offset` is pre-sample bias, `scale` multiplies
    /// before lookup. All four default to 0.0/1.0 (identity).
    pub u_offset: f32,
    pub v_offset: f32,
    pub u_scale: f32,
    pub v_scale: f32,

    /// Material alpha multiplier (0..=1). Composes with per-texel alpha.
    pub alpha: f32,

    /// Blend mode tuple. See [`AlphaBlendMode`].
    pub alpha_blend_mode: AlphaBlendMode,

    /// Alpha-test threshold (0..=255). `AlphaTest` below gates its use.
    pub alpha_test_ref: u8,
    pub alpha_test: bool,

    pub z_buffer_write: bool,
    pub z_buffer_test: bool,
    pub screen_space_reflections: bool,
    pub wetness_control_ssr: bool,
    pub decal: bool,
    pub two_sided: bool,
    pub decal_no_fade: bool,
    pub non_occluder: bool,

    pub refraction: bool,
    pub refraction_falloff: bool,
    pub refraction_power: f32,

    // version < 10 serializes env mapping here; version >= 10 drops
    // them (BGEM re-adds them later in its own section) and writes
    // depth_bias instead.
    /// Only set on BGSM version < 10 + BGEM version >= 10. The
    /// base-file layer treats it as "if Version < 10, read here" per
    /// the reference; higher-version BGEM re-reads it in its subclass
    /// section.
    pub environment_mapping: bool,
    pub environment_mapping_mask_scale: f32,
    /// Only set on version >= 10 (replaces env mapping in the base
    /// prefix).
    pub depth_bias: bool,

    pub grayscale_to_palette_color: bool,

    /// Version >= 6 only — all bits set by default.
    pub mask_writes: MaskWriteFlags,
}

impl Default for BaseMaterial {
    fn default() -> Self {
        Self {
            version: 2,
            tile_u: true,
            tile_v: true,
            u_offset: 0.0,
            v_offset: 0.0,
            u_scale: 1.0,
            v_scale: 1.0,
            alpha: 1.0,
            alpha_blend_mode: AlphaBlendMode::default(),
            alpha_test_ref: 128,
            alpha_test: false,
            z_buffer_write: true,
            z_buffer_test: true,
            screen_space_reflections: false,
            wetness_control_ssr: false,
            decal: false,
            two_sided: false,
            decal_no_fade: false,
            non_occluder: false,
            refraction: false,
            refraction_falloff: false,
            refraction_power: 0.0,
            environment_mapping: false,
            environment_mapping_mask_scale: 1.0,
            depth_bias: false,
            grayscale_to_palette_color: false,
            mask_writes: MaskWriteFlags(MaskWriteFlags::ALL),
        }
    }
}

impl BaseMaterial {
    /// Parse the common prefix starting AFTER the 4-byte magic. Caller
    /// (in `bgsm.rs` / `bgem.rs`) reads and validates the magic first,
    /// then calls this to consume the rest of the base fields.
    pub(crate) fn parse_after_magic(r: &mut Reader<'_>) -> Result<Self> {
        let version = r.read_u32()?;

        let tile_flags = r.read_u32()?;
        let tile_u = tile_flags & 2 != 0;
        let tile_v = tile_flags & 1 != 0;

        let u_offset = r.read_f32()?;
        let v_offset = r.read_f32()?;
        let u_scale = r.read_f32()?;
        let v_scale = r.read_f32()?;

        let alpha = r.read_f32()?;

        let function = r.read_u8()?;
        let src_blend = r.read_u32()?;
        let dst_blend = r.read_u32()?;
        let alpha_blend_mode = AlphaBlendMode {
            function,
            src_blend,
            dst_blend,
        };

        let alpha_test_ref = r.read_u8()?;
        let alpha_test = r.read_bool()?;

        let z_buffer_write = r.read_bool()?;
        let z_buffer_test = r.read_bool()?;
        let screen_space_reflections = r.read_bool()?;
        let wetness_control_ssr = r.read_bool()?;
        let decal = r.read_bool()?;
        let two_sided = r.read_bool()?;
        let decal_no_fade = r.read_bool()?;
        let non_occluder = r.read_bool()?;

        let refraction = r.read_bool()?;
        let refraction_falloff = r.read_bool()?;
        let refraction_power = r.read_f32()?;

        let (environment_mapping, environment_mapping_mask_scale, depth_bias) = if version < 10 {
            (r.read_bool()?, r.read_f32()?, false)
        } else {
            (false, 1.0, r.read_bool()?)
        };

        let grayscale_to_palette_color = r.read_bool()?;

        let mask_writes = if version >= 6 {
            MaskWriteFlags(r.read_u8()?)
        } else {
            MaskWriteFlags(MaskWriteFlags::ALL)
        };

        Ok(Self {
            version,
            tile_u,
            tile_v,
            u_offset,
            v_offset,
            u_scale,
            v_scale,
            alpha,
            alpha_blend_mode,
            alpha_test_ref,
            alpha_test,
            z_buffer_write,
            z_buffer_test,
            screen_space_reflections,
            wetness_control_ssr,
            decal,
            two_sided,
            decal_no_fade,
            non_occluder,
            refraction,
            refraction_falloff,
            refraction_power,
            environment_mapping,
            environment_mapping_mask_scale,
            depth_bias,
            grayscale_to_palette_color,
            mask_writes,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Append a BGSM v2 common prefix (everything after the 4-byte
    /// magic) with identity UV + alpha=1.0 + MaskWrites::ALL.
    pub(crate) fn append_base_v2(buf: &mut Vec<u8>, version: u32) {
        buf.extend_from_slice(&version.to_le_bytes());
        // tile_flags: bit 1 = TileV, bit 2 = TileU → 3 = both.
        buf.extend_from_slice(&3u32.to_le_bytes());
        // UV offset/scale
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        // alpha
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        // alpha blend mode: function=1 "Standard", src=6 SrcAlpha, dst=7 InvSrcAlpha
        buf.push(1);
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(&7u32.to_le_bytes());
        // alpha_test_ref + alpha_test
        buf.push(128);
        buf.push(0); // alpha_test = false
                     // 8 bools
        for _ in 0..8 {
            buf.push(0);
        }
        // refraction + refraction_falloff + refraction_power
        buf.push(0);
        buf.push(0);
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        // version < 10: env_mapping + scale; else: depth_bias
        if version < 10 {
            buf.push(0);
            buf.extend_from_slice(&1.0f32.to_le_bytes());
        } else {
            buf.push(0); // depth_bias
        }
        // grayscale_to_palette_color
        buf.push(0);
        // version >= 6: mask_writes = ALL
        if version >= 6 {
            buf.push(MaskWriteFlags::ALL);
        }
    }

    #[test]
    fn parse_common_prefix_v2_matches_defaults() {
        let mut bytes = Vec::new();
        append_base_v2(&mut bytes, 2);
        let mut r = Reader::new(&bytes);
        let base = BaseMaterial::parse_after_magic(&mut r).unwrap();
        assert_eq!(base.version, 2);
        assert!(base.tile_u && base.tile_v);
        assert_eq!(base.u_scale, 1.0);
        assert_eq!(base.alpha, 1.0);
        assert_eq!(base.alpha_blend_mode.function, 1);
        assert_eq!(base.alpha_blend_mode.src_blend, 6);
        assert_eq!(base.alpha_blend_mode.dst_blend, 7);
        assert_eq!(base.alpha_test_ref, 128);
        assert!(!base.alpha_test);
        // v2 < 10 → env_mapping path (disabled by fixture)
        assert!(!base.environment_mapping);
        assert_eq!(base.environment_mapping_mask_scale, 1.0);
        // v2 >= 6 → mask_writes was written
        assert_eq!(base.mask_writes.0, MaskWriteFlags::ALL);
        // Exact byte consumption
        assert_eq!(r.pos(), bytes.len());
    }

    #[test]
    fn parse_common_prefix_v10_uses_depth_bias_branch() {
        let mut bytes = Vec::new();
        append_base_v2(&mut bytes, 10);
        let mut r = Reader::new(&bytes);
        let base = BaseMaterial::parse_after_magic(&mut r).unwrap();
        assert_eq!(base.version, 10);
        // v >= 10 → env_mapping not read here, depth_bias used instead.
        assert!(!base.depth_bias);
        assert!(!base.environment_mapping); // default, never set
        assert_eq!(r.pos(), bytes.len());
    }

    #[test]
    fn parse_common_prefix_v5_omits_mask_writes_byte() {
        // v < 6 → no mask_writes byte; default remains ALL per default().
        let mut bytes = Vec::new();
        append_base_v2(&mut bytes, 5);
        let mut r = Reader::new(&bytes);
        let base = BaseMaterial::parse_after_magic(&mut r).unwrap();
        assert_eq!(base.version, 5);
        assert_eq!(base.mask_writes.0, MaskWriteFlags::ALL);
        assert_eq!(r.pos(), bytes.len());
    }
}
