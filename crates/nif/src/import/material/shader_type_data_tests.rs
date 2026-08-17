//! Tests for `shader_type_data_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`shader_type_data_tests::FOO`).

use super::*;

#[test]
fn none_variant_leaves_all_shader_type_fields_at_defaults() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(&mut info, &ShaderTypeData::None);
    assert_eq!(info.env_map_scale, 0.0);
    assert_eq!(info.skin_tint_color, None);
    assert_eq!(info.hair_tint_color, None);
    assert_eq!(info.parallax_max_passes, None);
    assert_eq!(info.multi_layer_inner_thickness, None);
    assert_eq!(info.sparkle_parameters, None);
    assert_eq!(info.eye_cubemap_scale, None);
}

#[test]
fn environment_map_writes_scale() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
    );
    assert_eq!(info.env_map_scale, 2.5);
}

/// #562 — the importer writes the core-owned payload directly. This
/// fully-populated value keeps every variant field available without a
/// second mirror type or field-by-field conversion.
#[test]
fn core_shader_type_fields_carries_every_variant_field() {
    let f = ShaderTypeFields {
        skin_tint_color: Some([0.9, 0.8, 0.7]),
        skin_tint_alpha: Some(0.5),
        hair_tint_color: Some([0.3, 0.15, 0.05]),
        eye_cubemap_scale: Some(1.25),
        eye_left_reflection_center: Some([0.1, 0.2, 0.3]),
        eye_right_reflection_center: Some([0.4, 0.5, 0.6]),
        parallax_max_passes: Some(8.0),
        parallax_height_scale: Some(0.04),
        multi_layer_inner_thickness: Some(0.1),
        multi_layer_refraction_scale: Some(0.5),
        multi_layer_inner_layer_scale: Some([2.0, 3.0]),
        multi_layer_envmap_strength: Some(1.5),
        sparkle_parameters: Some([1.0, 0.5, 0.25, 2.0]),
    };
    let core: byroredux_core::ecs::components::material::ShaderTypeFields = f.clone();
    assert_eq!(core, f);
    assert!(!core.is_empty());
}

/// Empty ShaderTypeFields must report `is_empty() == true` so the
/// spawn path can skip the Box allocation for the 99% of meshes
/// that don't carry a Skyrim+ variant payload.
#[test]
fn is_empty_returns_true_for_default_fields() {
    assert!(ShaderTypeFields::default().is_empty());
    let skin = ShaderTypeFields {
        skin_tint_color: Some([1.0, 1.0, 1.0]),
        ..Default::default()
    };
    assert!(!skin.is_empty());
}

/// MaterialInfo.
#[test]
fn capture_helper_parity_with_apply() {
    for data in &[
        ShaderTypeData::None,
        ShaderTypeData::EnvironmentMap { env_map_scale: 2.5 },
        ShaderTypeData::SkinTint {
            skin_tint_color: [0.8, 0.6, 0.5],
            skin_tint_alpha: None,
        },
        ShaderTypeData::Fo76SkinTint {
            skin_tint_color: [0.9, 0.7, 0.55, 0.25],
        },
        ShaderTypeData::HairTint {
            hair_tint_color: [0.3, 0.15, 0.05],
        },
        ShaderTypeData::ParallaxOcc {
            max_passes: 16.0,
            scale: 0.05,
        },
        ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness: 0.1,
            refraction_scale: 0.5,
            inner_layer_texture_scale: [2.0, 2.0],
            envmap_strength: 1.25,
        },
        ShaderTypeData::SparkleSnow {
            sparkle_parameters: [1.0, 0.5, 0.25, 2.0],
        },
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale: 1.5,
            left_eye_reflection_center: [0.1, 0.2, 0.3],
            right_eye_reflection_center: [0.4, 0.5, 0.6],
        },
    ] {
        let mut info = MaterialInfo::default();
        apply_shader_type_data(&mut info, data);
        assert_eq!(
            info.shader_type_fields(),
            capture_shader_type_fields(data),
            "variant {:?} must produce identical fields via apply and capture",
            data
        );
    }
}

#[test]
fn skin_tint_writes_rgb() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::SkinTint {
            skin_tint_color: [0.8, 0.6, 0.5],
            skin_tint_alpha: None,
        },
    );
    assert_eq!(info.skin_tint_color, Some([0.8, 0.6, 0.5]));
    assert_eq!(info.skin_tint_alpha, None);
}

#[test]
fn fo4_skin_tint_writes_authored_alpha() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::SkinTint {
            skin_tint_color: [0.0, 0.0, 0.0],
            skin_tint_alpha: Some(0.0),
        },
    );
    assert_eq!(info.skin_tint_color, Some([0.0, 0.0, 0.0]));
    assert_eq!(info.skin_tint_alpha, Some(0.0));
}

#[test]
fn fo76_skin_tint_splits_rgba_into_rgb_plus_alpha() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::Fo76SkinTint {
            skin_tint_color: [0.9, 0.7, 0.55, 0.25],
        },
    );
    assert_eq!(info.skin_tint_color, Some([0.9, 0.7, 0.55]));
    assert_eq!(info.skin_tint_alpha, Some(0.25));
}

/// Regression for #612 / SK-D3-04 — FO76 BSShaderType155 numbers
/// SkinTint as 4, but the renderer's `materialKind == 5u` branch
/// dispatches on the legacy BSLightingShaderType numbering.
/// The dedicated-shader boundary must remap so every FO76 NPC reaches
/// the SkinTint shader path. Payload application must then leave that
/// canonical kind intact.
#[test]
fn fo76_skin_tint_remaps_material_kind_to_skyrim_constant() {
    let mut info = MaterialInfo::default();
    info.material_kind = canonical_shader_type(TextureSlotLayout::Fallout76, 4);
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::Fo76SkinTint {
            skin_tint_color: [0.9, 0.7, 0.55, 0.25],
        },
    );
    assert_eq!(
        info.material_kind, 5,
        "FO76 SkinTint must remap to the legacy SkinTint constant \
             so `materialKind == 5u` in triangle.frag fires"
    );
}

/// #2579 sibling — FO76 raw type 5 means HairTint, while canonical type 5
/// means SkinTint. The shared boundary must translate it to 6 before the
/// payload reaches the renderer's `materialKind == 6u` branch.
#[test]
fn fo76_hair_tint_remaps_material_kind_to_skyrim_constant() {
    let mut info = MaterialInfo::default();
    info.material_kind = canonical_shader_type(TextureSlotLayout::Fallout76, 5);
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::HairTint {
            hair_tint_color: [0.3, 0.15, 0.05],
        },
    );
    assert_eq!(info.material_kind, 6);
}

/// Skyrim/FO4 `SkinTint` (legacy enum value 5) must not be touched
/// by the FO76 remap — it already arrives as 5 and the shader
/// branch fires correctly. Guards against an over-eager remap that
/// would clobber other paths.
#[test]
fn skyrim_skin_tint_preserves_material_kind() {
    let mut info = MaterialInfo::default();
    info.material_kind = 5;
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::SkinTint {
            skin_tint_color: [0.8, 0.6, 0.5],
            skin_tint_alpha: None,
        },
    );
    assert_eq!(info.material_kind, 5);
}

/// Other variants must not be affected by the FO76 SkinTint remap.
/// Spot-checks `HairTint` (legacy enum value 6) — its material_kind
/// must reach the shader unchanged so `materialKind == 6u` fires.
#[test]
fn hair_tint_does_not_remap_material_kind() {
    let mut info = MaterialInfo::default();
    info.material_kind = 6;
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::HairTint {
            hair_tint_color: [0.3, 0.15, 0.05],
        },
    );
    assert_eq!(info.material_kind, 6);
}

/// Regression for #570 / SK-D3-03: `material_kind` is `u32` end-to-
/// end (parser's `BSLightingShaderProperty.shader_type` is `u32`,
/// `GpuMaterial.material_kind` is `u32`). Pre-fix the importer
/// narrowed through `MaterialInfo.material_kind: u8` and re-widened
/// at scene-builder time, silently masking any `shader_type ≥ 256`.
/// All known Bethesda values today are 0–20 + engine 100/101, but
/// any future Starfield / FO4 DLC variant in the high-byte range
/// would have routed silently to the wrong shader branch.
///
/// Assert that values 256 and 0x10001 (a third-byte set, beyond
/// what `as u8` would have masked to 0) round-trip verbatim.
#[test]
fn material_kind_round_trips_values_above_u8_max() {
    let mut info = MaterialInfo::default();
    info.material_kind = 256;
    assert_eq!(
        info.material_kind, 256,
        "post-#570 material_kind must accept values ≥ 256 verbatim",
    );
    info.material_kind = 0x10001; // bit 16 + bit 0
    assert_eq!(
        info.material_kind, 0x10001,
        "post-#570 material_kind must accept values ≥ 65536 verbatim",
    );
    info.material_kind = u32::MAX;
    assert_eq!(info.material_kind, u32::MAX);
}

#[test]
fn hair_tint_writes_rgb() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::HairTint {
            hair_tint_color: [0.3, 0.15, 0.05],
        },
    );
    assert_eq!(info.hair_tint_color, Some([0.3, 0.15, 0.05]));
}

#[test]
fn parallax_occ_writes_passes_and_scale() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::ParallaxOcc {
            max_passes: 16.0,
            scale: 0.04,
        },
    );
    assert_eq!(info.parallax_max_passes, Some(16.0));
    assert_eq!(info.parallax_height_scale, Some(0.04));
}

#[test]
fn multi_layer_parallax_writes_all_four_fields() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness: 0.1,
            refraction_scale: 1.2,
            inner_layer_texture_scale: [2.0, 3.0],
            envmap_strength: 0.75,
        },
    );
    assert_eq!(info.multi_layer_inner_thickness, Some(0.1));
    assert_eq!(info.multi_layer_refraction_scale, Some(1.2));
    assert_eq!(info.multi_layer_inner_layer_scale, Some([2.0, 3.0]));
    assert_eq!(info.multi_layer_envmap_strength, Some(0.75));
}

#[test]
fn sparkle_snow_writes_all_four_parameters() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::SparkleSnow {
            sparkle_parameters: [1.0, 0.5, 0.25, 2.0],
        },
    );
    assert_eq!(info.sparkle_parameters, Some([1.0, 0.5, 0.25, 2.0]));
}

#[test]
fn eye_envmap_writes_scale_and_both_reflection_centers() {
    let mut info = MaterialInfo::default();
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale: 1.5,
            left_eye_reflection_center: [-0.03, 0.05, 0.0],
            right_eye_reflection_center: [0.03, 0.05, 0.0],
        },
    );
    assert_eq!(info.eye_cubemap_scale, Some(1.5));
    assert_eq!(info.eye_left_reflection_center, Some([-0.03, 0.05, 0.0]));
    assert_eq!(info.eye_right_reflection_center, Some([0.03, 0.05, 0.0]));
}

#[test]
fn environment_map_does_not_touch_other_variants_fields() {
    // Sanity: a mesh with env-map shader leaves skin/hair/eye/etc.
    // fields at None. Previous behavior was an if-let that matched
    // only EnvironmentMap, so this test would have passed before too
    // — but it's a guard against a future "clear all variants"
    // regression where the match arm accidentally stomps fields.
    let mut info = MaterialInfo::default();
    info.hair_tint_color = Some([0.1, 0.2, 0.3]); // pretend something else set this first
    apply_shader_type_data(
        &mut info,
        &ShaderTypeData::EnvironmentMap { env_map_scale: 1.0 },
    );
    assert_eq!(info.hair_tint_color, Some([0.1, 0.2, 0.3]));
}
