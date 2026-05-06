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

/// #430 — `capture_shader_type_fields` is the shared helper the
/// BsTriShape import path uses. Exhaustive per-variant check that the
/// returned bundle matches what `apply_shader_type_data` writes into
/// #562 — `ShaderTypeFields::to_core()` must mirror every field
/// byte-for-byte so the ECS `Material` component carries the
/// same Skyrim+ variant payload as the NIF importer captured.
/// Silent drift would desync the fragment-shader variant ladder
/// from the NIF-authored data.
#[test]
fn to_core_round_trips_every_field() {
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
    let c = f.to_core();
    assert_eq!(c.skin_tint_color, f.skin_tint_color);
    assert_eq!(c.skin_tint_alpha, f.skin_tint_alpha);
    assert_eq!(c.hair_tint_color, f.hair_tint_color);
    assert_eq!(c.eye_cubemap_scale, f.eye_cubemap_scale);
    assert_eq!(c.eye_left_reflection_center, f.eye_left_reflection_center);
    assert_eq!(c.eye_right_reflection_center, f.eye_right_reflection_center);
    assert_eq!(c.parallax_max_passes, f.parallax_max_passes);
    assert_eq!(c.parallax_height_scale, f.parallax_height_scale);
    assert_eq!(c.multi_layer_inner_thickness, f.multi_layer_inner_thickness);
    assert_eq!(
        c.multi_layer_refraction_scale,
        f.multi_layer_refraction_scale
    );
    assert_eq!(
        c.multi_layer_inner_layer_scale,
        f.multi_layer_inner_layer_scale
    );
    assert_eq!(c.multi_layer_envmap_strength, f.multi_layer_envmap_strength);
    assert_eq!(c.sparkle_parameters, f.sparkle_parameters);
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
        },
    );
    assert_eq!(info.skin_tint_color, Some([0.8, 0.6, 0.5]));
    assert_eq!(info.skin_tint_alpha, None);
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
/// `apply_shader_type_data` must remap so every FO76 NPC reaches
/// the SkinTint shader path. Pre-fix the simulated upstream
/// `info.material_kind = 4` survived and the shader gate skipped
/// the multiply silently.
#[test]
fn fo76_skin_tint_remaps_material_kind_to_skyrim_constant() {
    let mut info = MaterialInfo::default();
    // Simulate the upstream write at material.rs:606:
    // `info.material_kind = shader.shader_type as u8` — for FO76
    // bsver==155 this is the BSShaderType155 value `4`.
    info.material_kind = 4;
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
