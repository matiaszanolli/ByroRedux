//! BGSM (lit material) — `0x4d534742` = "BGSM" little-endian.
//!
//! Layout after the 4-byte magic matches `BGSM.Deserialize` at
//! `Material-Editor:BGSM.cs:321`.

use crate::base::{BaseMaterial, ColorRgb};
use crate::reader::Reader;
use crate::{Error, Result};

pub(crate) const SIGNATURE: u32 = 0x4D534742; // "BGSM"

/// Fallout 4 / Skyrim SE / FO76 lit material file.
///
/// Every field present in the reference impl is parsed; version-gated
/// fields default to the reference-impl default when their gate is
/// not met. See `Material-Editor:BGSM.cs:321-488` for the canonical
/// deserialization order.
#[derive(Debug, Clone, Default)]
pub struct BgsmFile {
    pub base: BaseMaterial,

    // --- Texture slots (version-gated) ---
    /// Always present. Diffuse / albedo.
    pub diffuse_texture: String,
    pub normal_texture: String,
    /// "Smooth spec" on disk — smoothness in alpha, specular RGB.
    pub smooth_spec_texture: String,
    pub greyscale_texture: String,

    // v > 2:
    pub glow_texture: String,
    pub wrinkles_texture: String,
    /// Standalone specular (PBR-style separate).
    pub specular_texture: String,
    pub lighting_texture: String,
    pub flow_texture: String,
    // v >= 17:
    pub distance_field_alpha_texture: String,

    // v <= 2 legacy alternate texture list. Preserved for completeness.
    pub envmap_texture: String,
    pub inner_layer_texture: String,
    pub displacement_texture: String,

    // --- Flags / scalars ---
    pub enable_editor_alpha_ref: bool,

    // v >= 8: translucency suite
    pub translucency: bool,
    pub translucency_thick_object: bool,
    pub translucency_mix_albedo_with_subsurface_color: bool,
    pub translucency_subsurface_color: ColorRgb,
    pub translucency_transmissive_scale: f32,
    pub translucency_turbulence: f32,

    // v < 8: rim + subsurface branch
    pub rim_lighting: bool,
    pub rim_power: f32,
    pub back_light_power: f32,
    pub subsurface_lighting: bool,
    pub subsurface_lighting_rolloff: f32,

    // always
    pub specular_enabled: bool,
    pub specular_color: ColorRgb,
    pub specular_mult: f32,
    pub smoothness: f32,

    pub fresnel_power: f32,
    pub wetness_control_spec_scale: f32,
    pub wetness_control_spec_power_scale: f32,
    pub wetness_control_spec_min_var: f32,
    /// v < 10: wetness env map scale (later dropped).
    pub wetness_control_env_map_scale: f32,
    pub wetness_control_fresnel_power: f32,
    pub wetness_control_metalness: f32,

    // v > 2
    pub pbr: bool,
    // v >= 9
    pub custom_porosity: bool,
    pub porosity_value: f32,

    /// Template parent — path to another BGSM whose fields provide
    /// defaults that this one overrides. Empty when no template.
    pub root_material_path: Option<String>,

    pub aniso_lighting: bool,
    pub emit_enabled: bool,
    /// Only set when `emit_enabled`.
    pub emittance_color: ColorRgb,

    pub emittance_mult: f32,
    pub model_space_normals: bool,
    pub external_emittance: bool,

    // v >= 12
    pub lum_emittance: f32,
    // v >= 13
    pub use_adaptive_emissive: bool,
    pub adaptive_emissive_exposure_offset: f32,
    pub adaptive_emissive_final_exposure_min: f32,
    pub adaptive_emissive_final_exposure_max: f32,

    // v < 8
    pub back_lighting: bool,

    pub receive_shadows: bool,
    pub hide_secret: bool,
    pub cast_shadows: bool,
    pub dissolve_fade: bool,
    pub assume_shadowmask: bool,

    pub glowmap: bool,

    // v < 7
    pub environment_mapping_window: bool,
    pub environment_mapping_eye: bool,

    pub hair: bool,
    pub hair_tint_color: ColorRgb,

    pub tree: bool,
    pub facegen: bool,
    pub skin_tint: bool,
    pub tessellate: bool,

    // v < 3 tessellation suite
    pub displacement_texture_bias: f32,
    pub displacement_texture_scale: f32,
    pub tessellation_pn_scale: f32,
    pub tessellation_base_factor: f32,
    pub tessellation_fade_distance: f32,

    pub grayscale_to_palette_scale: f32,

    // v >= 1
    pub skew_specular_alpha: bool,

    // v >= 3 terrain
    pub terrain: bool,
    pub terrain_unk_int1: u32,
    pub terrain_threshold_falloff: f32,
    pub terrain_tiling_distance: f32,
    pub terrain_rotation_angle: f32,
}

impl BgsmFile {
    pub(crate) fn parse(r: &mut Reader<'_>) -> Result<Self> {
        let magic = r.read_u32()?;
        if magic != SIGNATURE {
            return Err(Error::BadMagic { got: magic });
        }
        let base = BaseMaterial::parse_after_magic(r)?;
        let version = base.version;

        let mut out = Self {
            base,
            rim_power: 2.0,
            subsurface_lighting_rolloff: 0.3,
            specular_color: [1.0, 1.0, 1.0],
            specular_mult: 1.0,
            smoothness: 1.0,
            fresnel_power: 5.0,
            wetness_control_spec_scale: -1.0,
            wetness_control_spec_power_scale: -1.0,
            wetness_control_spec_min_var: -1.0,
            wetness_control_env_map_scale: -1.0,
            wetness_control_fresnel_power: -1.0,
            wetness_control_metalness: -1.0,
            emittance_color: [1.0, 1.0, 1.0],
            emittance_mult: 1.0,
            hair_tint_color: [128.0 / 255.0; 3],
            displacement_texture_bias: -0.5,
            displacement_texture_scale: 10.0,
            tessellation_pn_scale: 1.0,
            tessellation_base_factor: 1.0,
            grayscale_to_palette_scale: 1.0,
            ..Default::default()
        };

        // Texture slots — layout forks at version > 2.
        out.diffuse_texture = r.read_string()?;
        out.normal_texture = r.read_string()?;
        out.smooth_spec_texture = r.read_string()?;
        out.greyscale_texture = r.read_string()?;

        if version > 2 {
            out.glow_texture = r.read_string()?;
            out.wrinkles_texture = r.read_string()?;
            out.specular_texture = r.read_string()?;
            out.lighting_texture = r.read_string()?;
            out.flow_texture = r.read_string()?;
            if version >= 17 {
                out.distance_field_alpha_texture = r.read_string()?;
            }
        } else {
            out.envmap_texture = r.read_string()?;
            out.glow_texture = r.read_string()?;
            out.inner_layer_texture = r.read_string()?;
            out.wrinkles_texture = r.read_string()?;
            out.displacement_texture = r.read_string()?;
        }

        out.enable_editor_alpha_ref = r.read_bool()?;

        if version >= 8 {
            out.translucency = r.read_bool()?;
            out.translucency_thick_object = r.read_bool()?;
            out.translucency_mix_albedo_with_subsurface_color = r.read_bool()?;
            out.translucency_subsurface_color = r.read_color()?;
            out.translucency_transmissive_scale = r.read_f32()?;
            out.translucency_turbulence = r.read_f32()?;
        } else {
            out.rim_lighting = r.read_bool()?;
            out.rim_power = r.read_f32()?;
            out.back_light_power = r.read_f32()?;
            out.subsurface_lighting = r.read_bool()?;
            out.subsurface_lighting_rolloff = r.read_f32()?;
        }

        out.specular_enabled = r.read_bool()?;
        out.specular_color = r.read_color()?;
        out.specular_mult = r.read_f32()?;
        out.smoothness = r.read_f32()?;

        out.fresnel_power = r.read_f32()?;
        out.wetness_control_spec_scale = r.read_f32()?;
        out.wetness_control_spec_power_scale = r.read_f32()?;
        out.wetness_control_spec_min_var = r.read_f32()?;

        if version < 10 {
            out.wetness_control_env_map_scale = r.read_f32()?;
        }

        out.wetness_control_fresnel_power = r.read_f32()?;
        out.wetness_control_metalness = r.read_f32()?;

        if version > 2 {
            out.pbr = r.read_bool()?;
            if version >= 9 {
                out.custom_porosity = r.read_bool()?;
                out.porosity_value = r.read_f32()?;
            }
        }

        let root_path = r.read_string()?;
        out.root_material_path = if root_path.is_empty() {
            None
        } else {
            Some(root_path)
        };

        out.aniso_lighting = r.read_bool()?;
        out.emit_enabled = r.read_bool()?;
        if out.emit_enabled {
            out.emittance_color = r.read_color()?;
        }

        out.emittance_mult = r.read_f32()?;
        out.model_space_normals = r.read_bool()?;
        out.external_emittance = r.read_bool()?;

        if version >= 12 {
            out.lum_emittance = r.read_f32()?;
        }

        if version >= 13 {
            out.use_adaptive_emissive = r.read_bool()?;
            out.adaptive_emissive_exposure_offset = r.read_f32()?;
            out.adaptive_emissive_final_exposure_min = r.read_f32()?;
            out.adaptive_emissive_final_exposure_max = r.read_f32()?;
        }

        if version < 8 {
            out.back_lighting = r.read_bool()?;
        }

        out.receive_shadows = r.read_bool()?;
        out.hide_secret = r.read_bool()?;
        out.cast_shadows = r.read_bool()?;
        out.dissolve_fade = r.read_bool()?;
        out.assume_shadowmask = r.read_bool()?;

        out.glowmap = r.read_bool()?;

        if version < 7 {
            out.environment_mapping_window = r.read_bool()?;
            out.environment_mapping_eye = r.read_bool()?;
        }

        out.hair = r.read_bool()?;
        out.hair_tint_color = r.read_color()?;

        out.tree = r.read_bool()?;
        out.facegen = r.read_bool()?;
        out.skin_tint = r.read_bool()?;
        out.tessellate = r.read_bool()?;

        if version < 3 {
            out.displacement_texture_bias = r.read_f32()?;
            out.displacement_texture_scale = r.read_f32()?;
            out.tessellation_pn_scale = r.read_f32()?;
            out.tessellation_base_factor = r.read_f32()?;
            out.tessellation_fade_distance = r.read_f32()?;
        }

        out.grayscale_to_palette_scale = r.read_f32()?;

        if version >= 1 {
            out.skew_specular_alpha = r.read_bool()?;
        }

        if version >= 3 {
            out.terrain = r.read_bool()?;
            if out.terrain {
                if version == 3 {
                    out.terrain_unk_int1 = r.read_u32()?;
                }
                out.terrain_threshold_falloff = r.read_f32()?;
                out.terrain_tiling_distance = r.read_f32()?;
                out.terrain_rotation_angle = r.read_f32()?;
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::base::tests::append_base_v2;

    /// Append a length-prefixed string per the BGSM convention
    /// (length counts the trailing NUL).
    fn append_string(buf: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            buf.extend_from_slice(&0u32.to_le_bytes());
            return;
        }
        let bytes = s.as_bytes();
        let len = bytes.len() as u32 + 1;
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(bytes);
        buf.push(0); // trailing NUL
    }

    /// Minimal valid FO4 v2 BGSM — identity UV, empty textures, no
    /// template, no translucency. Used by lib.rs::parse_dispatches_on_magic.
    pub(crate) fn minimal_v2_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SIGNATURE.to_le_bytes());
        append_base_v2(&mut buf, 2);

        // Texture slots (v <= 2 uses legacy 5-texture layout).
        append_string(&mut buf, ""); // diffuse
        append_string(&mut buf, ""); // normal
        append_string(&mut buf, ""); // smooth_spec
        append_string(&mut buf, ""); // greyscale
                                     // v <= 2: envmap, glow, inner_layer, wrinkles, displacement
        append_string(&mut buf, "");
        append_string(&mut buf, "");
        append_string(&mut buf, "");
        append_string(&mut buf, "");
        append_string(&mut buf, "");

        buf.push(0); // enable_editor_alpha_ref

        // v < 8 branch: rim + subsurface
        buf.push(0); // rim_lighting
        buf.extend_from_slice(&2.0f32.to_le_bytes()); // rim_power
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // back_light_power
        buf.push(0); // subsurface_lighting
        buf.extend_from_slice(&0.3f32.to_le_bytes()); // subsurface_rolloff

        // specular
        buf.push(0); // specular_enabled
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // r
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // g
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // b
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // specular_mult
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // smoothness

        buf.extend_from_slice(&5.0f32.to_le_bytes()); // fresnel_power
        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_spec_scale
        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_spec_power_scale
        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_spec_min_var

        // v < 10
        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_env_map_scale

        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_fresnel_power
        buf.extend_from_slice(&(-1.0f32).to_le_bytes()); // wetness_metalness

        // v > 2: pbr + custom_porosity — NOT written for v=2
        // root_material_path
        append_string(&mut buf, "");

        buf.push(0); // aniso_lighting
        buf.push(0); // emit_enabled — no emittance_color follows
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // emittance_mult
        buf.push(0); // model_space_normals
        buf.push(0); // external_emittance
                     // v < 12 → no lum_emittance
                     // v < 13 → no adaptive_emissive
                     // v < 8 → back_lighting
        buf.push(0);

        // 5 receive/hide/cast/dissolve/assume
        for _ in 0..5 {
            buf.push(0);
        }
        buf.push(0); // glowmap
                     // v < 7 → env_mapping_window + eye
        buf.push(0);
        buf.push(0);

        buf.push(0); // hair
        buf.extend_from_slice(&0.5f32.to_le_bytes()); // hair_tint r
        buf.extend_from_slice(&0.5f32.to_le_bytes()); // g
        buf.extend_from_slice(&0.5f32.to_le_bytes()); // b

        // tree, facegen, skin_tint, tessellate
        for _ in 0..4 {
            buf.push(0);
        }

        // v < 3 → 5 tessellation floats
        for _ in 0..5 {
            buf.extend_from_slice(&0.0f32.to_le_bytes());
        }

        buf.extend_from_slice(&1.0f32.to_le_bytes()); // grayscale_to_palette_scale

        // v >= 1 → skew_specular_alpha
        buf.push(0);

        // v < 3 → NO terrain section
        buf
    }

    #[test]
    fn parse_minimal_v2_bgsm() {
        let bytes = minimal_v2_bytes();
        let mut r = Reader::new(&bytes);
        let m = BgsmFile::parse(&mut r).expect("parse minimal v2");
        assert_eq!(m.base.version, 2);
        assert_eq!(m.diffuse_texture, "");
        assert_eq!(m.smoothness, 1.0);
        assert_eq!(m.root_material_path, None);
        // v < 8 rim branch was taken — rim_power default landed.
        assert_eq!(m.rim_power, 2.0);
        // exact byte consumption
        assert_eq!(r.pos(), bytes.len());
    }

    #[test]
    fn parse_v2_bgsm_with_textures_and_template() {
        // Same as minimal but with a few real texture paths + a template.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE.to_le_bytes());
        append_base_v2(&mut bytes, 2);

        append_string(&mut bytes, "textures/clutter/diffuse.dds");
        append_string(&mut bytes, "textures/clutter/normal.dds");
        append_string(&mut bytes, ""); // smooth_spec
        append_string(&mut bytes, ""); // greyscale
        append_string(&mut bytes, ""); // envmap
        append_string(&mut bytes, ""); // glow
        append_string(&mut bytes, ""); // inner_layer
        append_string(&mut bytes, ""); // wrinkles
        append_string(&mut bytes, ""); // displacement

        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(&2.0f32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&0.3f32.to_le_bytes());

        bytes.push(0);
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());

        bytes.extend_from_slice(&5.0f32.to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());
        bytes.extend_from_slice(&(-1.0f32).to_le_bytes());

        // TEMPLATE!
        append_string(&mut bytes, "Materials/template/parent.bgsm");

        bytes.push(0);
        bytes.push(0); // emit_enabled
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.push(0); // back_lighting

        for _ in 0..5 {
            bytes.push(0);
        }
        bytes.push(0); // glowmap
        bytes.push(0);
        bytes.push(0);

        bytes.push(0);
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());

        for _ in 0..4 {
            bytes.push(0);
        }
        for _ in 0..5 {
            bytes.extend_from_slice(&0.0f32.to_le_bytes());
        }

        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        bytes.push(0);

        let mut r = Reader::new(&bytes);
        let m = BgsmFile::parse(&mut r).unwrap();
        assert_eq!(m.diffuse_texture, "textures/clutter/diffuse.dds");
        assert_eq!(m.normal_texture, "textures/clutter/normal.dds");
        assert_eq!(
            m.root_material_path.as_deref(),
            Some("Materials/template/parent.bgsm"),
        );
        assert_eq!(r.pos(), bytes.len());
    }
}
