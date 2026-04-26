//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: apply_shader_type_data, capture_shader_type_fields, ShaderTypeFields, capture_effect_shader_data.

use super::*;

/// Lift a `BSEffectShaderProperty` into the importer's
/// [`BsEffectShaderData`] capture struct. Empty string fields collapse
/// to `None`. Pre-FO76 inputs leave `refraction_power = None`. See
/// #345 / audit S4-01.
pub(crate) fn capture_effect_shader_data(shader: &BSEffectShaderProperty) -> BsEffectShaderData {
    fn opt(s: &str) -> Option<String> {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }
    BsEffectShaderData {
        falloff_start_angle: shader.falloff_start_angle,
        falloff_stop_angle: shader.falloff_stop_angle,
        falloff_start_opacity: shader.falloff_start_opacity,
        falloff_stop_opacity: shader.falloff_stop_opacity,
        soft_falloff_depth: shader.soft_falloff_depth,
        greyscale_texture: opt(&shader.greyscale_texture),
        env_map_texture: opt(&shader.env_map_texture),
        normal_texture: opt(&shader.normal_texture),
        env_mask_texture: opt(&shader.env_mask_texture),
        env_map_scale: shader.env_map_scale,
        // refraction_power is FO76-only; the parser fills it with 0.0
        // on pre-FO76. Surface as `None` so the shader-side dispatch
        // can branch on `Some(p)` instead of guessing whether 0.0
        // means "off" or "FO76 with literal 0".
        refraction_power: (shader.refraction_power != 0.0).then_some(shader.refraction_power),
        lighting_influence: shader.lighting_influence,
        env_map_min_lod: shader.env_map_min_lod,
        texture_clamp_mode: shader.texture_clamp_mode,
    }
}

/// Decode an NiAlphaProperty onto a `MaterialInfo`. `NiAlphaProperty.flags`
/// packs both alpha-blend (bit 0) and alpha-test (bit 9, mask 0x200) per
/// nif.xml; `threshold` is a u8 in [0, 255]. See issue #152.
///
/// When a material sets both bits (common on Gamebryo foliage, hair,
/// chain-link fences) we prefer alpha-test over alpha-blend — the
/// discard + opaque-depth path gives clean cutouts without the z-sort
/// artifacts that plague back-to-front blend on statics. `alpha_blend`
/// is intentionally set to `false` in that case so the renderer binds
/// the opaque pipeline.
/// Write every `ShaderTypeData` variant's trailing fields onto
/// `MaterialInfo`. Previously only `EnvironmentMap` was consumed; the
/// remaining 8 variants (SkinTint, Fo76SkinTint, HairTint, ParallaxOcc,
/// MultiLayerParallax, SparkleSnow, EyeEnvmap, and the `None`
/// pass-through for types that carry no trailing data) were pattern-
/// matched out and dropped. Issue #343 / SK-D3-01.
///
/// Renderer-side dispatch on `MaterialInfo.material_kind` is tracked
/// separately (SK-D3-02). Until that lands these values ride unused on
/// the `Material` component; the purpose here is to ensure no variant
/// is silently discarded at the import boundary.
pub(crate) fn apply_shader_type_data(info: &mut MaterialInfo, data: &ShaderTypeData) {
    // Env-map scale lives on its own field for backwards compatibility with
    // pre-#343 readers; the other variants copy through `ShaderTypeFields`.
    if let ShaderTypeData::EnvironmentMap { env_map_scale } = *data {
        info.env_map_scale = env_map_scale;
    }
    // FO76 BSShaderType155 numbers SkinTint as 4 (Color4), but the
    // legacy BSLightingShaderType + the renderer's `materialKind == 5u`
    // branch use 5. The Color4 alpha and Color3 RGB are both written
    // into the same `skin_tint_color` + `skin_tint_alpha` slots
    // upstream (Skyrim's Color3 path leaves alpha defaulted to 1.0,
    // FO76's Color4 path supplies a real value), and the shader's
    // `mix(albedo, albedo*tint, alpha)` formula handles both. Remap
    // here so every NPC/creature reaches the same shader branch.
    // See #612 / SK-D3-04.
    if matches!(data, ShaderTypeData::Fo76SkinTint { .. }) {
        info.material_kind = 5;
    }
    let fields = capture_shader_type_fields(data);
    // #623 / SK-D3-07: GpuInstance packs `multi_layer_envmap_strength`
    // into the `w` slot of the `hair_tint_{r,g,b,_}` vec4 to save
    // alignment padding (scene_buffer.rs:240-263). The packing is
    // safe only because `ShaderTypeData` is a single-tag enum, so
    // `capture_shader_type_fields` populates one or the other but
    // never both. If a future variant or refactor breaks that, the
    // assert fires before we silently render hair-tinted meshes
    // with a stray multi-layer envmap strength (or vice versa).
    debug_assert!(
        fields.hair_tint_color.is_none() || fields.multi_layer_envmap_strength.is_none(),
        "GpuInstance vec4 share is broken: hair_tint and multi_layer_envmap_strength \
         must never appear together in a single ShaderTypeData capture"
    );
    info.skin_tint_color = fields.skin_tint_color.or(info.skin_tint_color);
    info.skin_tint_alpha = fields.skin_tint_alpha.or(info.skin_tint_alpha);
    info.hair_tint_color = fields.hair_tint_color.or(info.hair_tint_color);
    info.eye_cubemap_scale = fields.eye_cubemap_scale.or(info.eye_cubemap_scale);
    info.eye_left_reflection_center = fields
        .eye_left_reflection_center
        .or(info.eye_left_reflection_center);
    info.eye_right_reflection_center = fields
        .eye_right_reflection_center
        .or(info.eye_right_reflection_center);
    info.parallax_max_passes = fields.parallax_max_passes.or(info.parallax_max_passes);
    info.parallax_height_scale = fields
        .parallax_height_scale
        .or(info.parallax_height_scale);
    info.multi_layer_inner_thickness = fields
        .multi_layer_inner_thickness
        .or(info.multi_layer_inner_thickness);
    info.multi_layer_refraction_scale = fields
        .multi_layer_refraction_scale
        .or(info.multi_layer_refraction_scale);
    info.multi_layer_inner_layer_scale = fields
        .multi_layer_inner_layer_scale
        .or(info.multi_layer_inner_layer_scale);
    info.multi_layer_envmap_strength = fields
        .multi_layer_envmap_strength
        .or(info.multi_layer_envmap_strength);
    info.sparkle_parameters = fields.sparkle_parameters.or(info.sparkle_parameters);
}

/// The 13 shader-type-specific fields pulled off `BSLightingShaderProperty`'s
/// `shader_type_data` variant. Mirrors the flat fields on `MaterialInfo` so
/// both the NiTriShape path (via `MaterialInfo`) and the BsTriShape path
/// (direct) can populate the same `ImportedMesh` fields without duplication.
/// See #430 / NIF-D4-N01.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShaderTypeFields {
    pub skin_tint_color: Option<[f32; 3]>,
    pub skin_tint_alpha: Option<f32>,
    pub hair_tint_color: Option<[f32; 3]>,
    pub eye_cubemap_scale: Option<f32>,
    pub eye_left_reflection_center: Option<[f32; 3]>,
    pub eye_right_reflection_center: Option<[f32; 3]>,
    pub parallax_max_passes: Option<f32>,
    pub parallax_height_scale: Option<f32>,
    pub multi_layer_inner_thickness: Option<f32>,
    pub multi_layer_refraction_scale: Option<f32>,
    pub multi_layer_inner_layer_scale: Option<[f32; 2]>,
    pub multi_layer_envmap_strength: Option<f32>,
    pub sparkle_parameters: Option<[f32; 4]>,
}

/// Pull the shader-type-specific trailing fields out of a `ShaderTypeData`
/// into a flat `ShaderTypeFields` bundle. Complements
/// [`apply_shader_type_data`] — both are exhaustive on the 9 variants so
/// any future addition fails compilation here.
pub(crate) fn capture_shader_type_fields(data: &ShaderTypeData) -> ShaderTypeFields {
    let mut f = ShaderTypeFields::default();
    match *data {
        ShaderTypeData::None | ShaderTypeData::EnvironmentMap { .. } => {}
        ShaderTypeData::SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some(skin_tint_color);
        }
        ShaderTypeData::Fo76SkinTint { skin_tint_color } => {
            f.skin_tint_color = Some([skin_tint_color[0], skin_tint_color[1], skin_tint_color[2]]);
            f.skin_tint_alpha = Some(skin_tint_color[3]);
        }
        ShaderTypeData::HairTint { hair_tint_color } => {
            f.hair_tint_color = Some(hair_tint_color);
        }
        ShaderTypeData::ParallaxOcc { max_passes, scale } => {
            f.parallax_max_passes = Some(max_passes);
            f.parallax_height_scale = Some(scale);
        }
        ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness,
            refraction_scale,
            inner_layer_texture_scale,
            envmap_strength,
        } => {
            f.multi_layer_inner_thickness = Some(inner_layer_thickness);
            f.multi_layer_refraction_scale = Some(refraction_scale);
            f.multi_layer_inner_layer_scale = Some(inner_layer_texture_scale);
            f.multi_layer_envmap_strength = Some(envmap_strength);
        }
        ShaderTypeData::SparkleSnow { sparkle_parameters } => {
            f.sparkle_parameters = Some(sparkle_parameters);
        }
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale,
            left_eye_reflection_center,
            right_eye_reflection_center,
        } => {
            f.eye_cubemap_scale = Some(eye_cubemap_scale);
            f.eye_left_reflection_center = Some(left_eye_reflection_center);
            f.eye_right_reflection_center = Some(right_eye_reflection_center);
        }
    }
    f
}

impl ShaderTypeFields {
    /// `true` when every slot is empty — equivalent to
    /// `*self == ShaderTypeFields::default()`. Used to skip attaching
    /// a heap-allocated `Box<ShaderTypeFields>` on the ECS `Material`
    /// component for the 99% of meshes that use no Skyrim+ variant.
    pub fn is_empty(&self) -> bool {
        *self == ShaderTypeFields::default()
    }

    /// Convert into the ECS-side [`byroredux_core::ecs::components::material::ShaderTypeFields`]
    /// mirror type. Field-by-field copy — both shapes are intentionally
    /// identical. See #562.
    pub fn to_core(&self) -> byroredux_core::ecs::components::material::ShaderTypeFields {
        byroredux_core::ecs::components::material::ShaderTypeFields {
            skin_tint_color: self.skin_tint_color,
            skin_tint_alpha: self.skin_tint_alpha,
            hair_tint_color: self.hair_tint_color,
            eye_cubemap_scale: self.eye_cubemap_scale,
            eye_left_reflection_center: self.eye_left_reflection_center,
            eye_right_reflection_center: self.eye_right_reflection_center,
            parallax_max_passes: self.parallax_max_passes,
            parallax_height_scale: self.parallax_height_scale,
            multi_layer_inner_thickness: self.multi_layer_inner_thickness,
            multi_layer_refraction_scale: self.multi_layer_refraction_scale,
            multi_layer_inner_layer_scale: self.multi_layer_inner_layer_scale,
            multi_layer_envmap_strength: self.multi_layer_envmap_strength,
            sparkle_parameters: self.sparkle_parameters,
        }
    }
}

