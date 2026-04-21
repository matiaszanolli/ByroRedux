//! BGEM (effect material) — `0x4d454742` = "BGEM" little-endian.
//!
//! Layout after the 4-byte magic matches `BGEM.Deserialize` at
//! `Material-Editor:BGEM.cs:178`. BGEM has no `root_material_path`
//! (template inheritance is BGSM-only).

use crate::base::{BaseMaterial, ColorRgb};
use crate::reader::Reader;
use crate::{Error, Result};

pub(crate) const SIGNATURE: u32 = 0x4D454742; // "BGEM"

/// Effect material file — alpha-blended particles, decals, glass,
/// force-field overlays, etc.
#[derive(Debug, Clone, Default)]
pub struct BgemFile {
    pub base: BaseMaterial,

    // Always present.
    pub base_texture: String,
    pub grayscale_texture: String,
    pub envmap_texture: String,
    pub normal_texture: String,
    pub envmap_mask_texture: String,

    // v >= 11
    pub specular_texture: String,
    pub lighting_texture: String,
    pub glow_texture: String,

    // v >= 21 glass overlay suite
    pub glass_roughness_scratch: String,
    pub glass_dirt_overlay: String,
    pub glass_enabled: bool,
    pub glass_fresnel_color: ColorRgb,
    pub glass_blur_scale_base: f32,
    /// v >= 22
    pub glass_blur_scale_factor: f32,
    pub glass_refraction_scale_base: f32,

    // v >= 10 — BGEM re-reads these in its subclass section
    pub environment_mapping: bool,
    pub environment_mapping_mask_scale: f32,

    pub blood_enabled: bool,
    pub effect_lighting_enabled: bool,
    pub falloff_enabled: bool,
    pub falloff_color_enabled: bool,
    pub grayscale_to_palette_alpha: bool,
    pub soft_enabled: bool,

    pub base_color: ColorRgb,
    pub base_color_scale: f32,

    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,

    pub lighting_influence: f32,
    pub envmap_min_lod: u8,
    pub soft_depth: f32,

    // v >= 11
    pub emittance_color: ColorRgb,

    // v >= 15
    pub adaptive_emissive_exposure_offset: f32,
    pub adaptive_emissive_final_exposure_min: f32,
    pub adaptive_emissive_final_exposure_max: f32,

    // v >= 16
    pub glowmap: bool,

    // v >= 20
    pub effect_pbr_specular: bool,
}

impl BgemFile {
    pub(crate) fn parse(r: &mut Reader<'_>) -> Result<Self> {
        let magic = r.read_u32()?;
        if magic != SIGNATURE {
            return Err(Error::BadMagic { got: magic });
        }
        let base = BaseMaterial::parse_after_magic(r)?;
        let version = base.version;

        let mut out = Self {
            base,
            base_color: [1.0, 1.0, 1.0],
            base_color_scale: 1.0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            lighting_influence: 1.0,
            soft_depth: 100.0,
            emittance_color: [1.0, 1.0, 1.0],
            glass_fresnel_color: [1.0, 1.0, 1.0],
            glass_refraction_scale_base: 0.05,
            glass_blur_scale_base: 0.4,
            glass_blur_scale_factor: 1.0,
            ..Default::default()
        };

        out.base_texture = r.read_string()?;
        out.grayscale_texture = r.read_string()?;
        out.envmap_texture = r.read_string()?;
        out.normal_texture = r.read_string()?;
        out.envmap_mask_texture = r.read_string()?;

        if version >= 11 {
            out.specular_texture = r.read_string()?;
            out.lighting_texture = r.read_string()?;
            out.glow_texture = r.read_string()?;
        }

        if version >= 21 {
            out.glass_roughness_scratch = r.read_string()?;
            out.glass_dirt_overlay = r.read_string()?;
            out.glass_enabled = r.read_bool()?;
            if out.glass_enabled {
                out.glass_fresnel_color = r.read_color()?;
                // Order matches the reference's // FIXME note.
                out.glass_blur_scale_base = r.read_f32()?;
                if version >= 22 {
                    out.glass_blur_scale_factor = r.read_f32()?;
                }
                out.glass_refraction_scale_base = r.read_f32()?;
            }
        }

        if version >= 10 {
            out.environment_mapping = r.read_bool()?;
            out.environment_mapping_mask_scale = r.read_f32()?;
        }

        out.blood_enabled = r.read_bool()?;
        out.effect_lighting_enabled = r.read_bool()?;
        out.falloff_enabled = r.read_bool()?;
        out.falloff_color_enabled = r.read_bool()?;
        out.grayscale_to_palette_alpha = r.read_bool()?;
        out.soft_enabled = r.read_bool()?;

        out.base_color = r.read_color()?;
        out.base_color_scale = r.read_f32()?;

        out.falloff_start_angle = r.read_f32()?;
        out.falloff_stop_angle = r.read_f32()?;
        out.falloff_start_opacity = r.read_f32()?;
        out.falloff_stop_opacity = r.read_f32()?;

        out.lighting_influence = r.read_f32()?;
        out.envmap_min_lod = r.read_u8()?;
        out.soft_depth = r.read_f32()?;

        if version >= 11 {
            out.emittance_color = r.read_color()?;
        }

        if version >= 15 {
            out.adaptive_emissive_exposure_offset = r.read_f32()?;
            out.adaptive_emissive_final_exposure_min = r.read_f32()?;
            out.adaptive_emissive_final_exposure_max = r.read_f32()?;
        }

        if version >= 16 {
            out.glowmap = r.read_bool()?;
        }

        if version >= 20 {
            out.effect_pbr_specular = r.read_bool()?;
        }

        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::base::tests::append_base_v2;

    fn append_string(buf: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            buf.extend_from_slice(&0u32.to_le_bytes());
            return;
        }
        let bytes = s.as_bytes();
        let len = bytes.len() as u32 + 1;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(bytes);
        buf.push(0);
    }

    /// Minimum FO4 v2 BGEM fixture.
    pub(crate) fn minimal_v2_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SIGNATURE.to_le_bytes());
        append_base_v2(&mut buf, 2);

        // 5 texture strings (v < 11 stops here).
        for _ in 0..5 {
            append_string(&mut buf, "");
        }

        // v < 10 → no env_mapping section here.
        // 6 flag bools: blood, effect_lighting, falloff, falloff_color, grayscale_to_palette_alpha, soft
        for _ in 0..6 {
            buf.push(0);
        }

        // base_color (3×f32) + base_color_scale
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());

        // 4 falloff floats
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        // lighting_influence, envmap_min_lod, soft_depth
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(&100.0f32.to_le_bytes());

        // v < 11 → no emittance color
        // v < 15 → no adaptive_emissive
        // v < 16 → no glowmap
        // v < 20 → no effect_pbr_specular
        buf
    }

    #[test]
    fn parse_minimal_v2_bgem() {
        let bytes = minimal_v2_bytes();
        let mut r = Reader::new(&bytes);
        let m = BgemFile::parse(&mut r).expect("parse minimal v2");
        assert_eq!(m.base.version, 2);
        assert_eq!(m.base_color, [1.0, 1.0, 1.0]);
        assert_eq!(m.base_color_scale, 1.0);
        assert_eq!(m.soft_depth, 100.0);
        assert_eq!(r.pos(), bytes.len());
    }

    #[test]
    fn parse_v2_bgem_with_base_texture() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE.to_le_bytes());
        append_base_v2(&mut bytes, 2);

        append_string(&mut bytes, "textures/effects/forcefield.dds");
        append_string(&mut bytes, "");
        append_string(&mut bytes, "");
        append_string(&mut bytes, "");
        append_string(&mut bytes, "");

        for _ in 0..6 {
            bytes.push(0);
        }
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&0.8f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&2.0f32.to_le_bytes());

        for _ in 0..4 {
            bytes.extend_from_slice(&1.0f32.to_le_bytes());
        }

        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&100.0f32.to_le_bytes());

        let mut r = Reader::new(&bytes);
        let m = BgemFile::parse(&mut r).unwrap();
        assert_eq!(m.base_texture, "textures/effects/forcefield.dds");
        assert_eq!(m.base_color, [0.5, 0.8, 1.0]);
        assert_eq!(m.base_color_scale, 2.0);
        assert_eq!(r.pos(), bytes.len());
    }
}
