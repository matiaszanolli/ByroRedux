//! Tests for `effect_shader_capture_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`effect_shader_capture_tests::FOO`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::BSEffectShaderProperty;
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;

/// Build a fully-populated FO4-style `BSEffectShaderProperty` with
/// every field set to a distinct, recognisable value.
fn fully_populated_fo4_shader() -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        material_reference: false,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        source_texture: "fx/glow.dds".to_string(),
        texture_clamp_mode: 3,
        lighting_influence: 200,
        env_map_min_lod: 4,
        falloff_start_angle: 0.95,
        falloff_stop_angle: 0.30,
        falloff_start_opacity: 1.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0, // pre-FO76 default
        base_color: [0.0; 4],
        base_color_scale: 1.0,
        soft_falloff_depth: 8.0,
        greyscale_texture: "fx/grad.dds".to_string(),
        env_map_texture: "fx/env.dds".to_string(),
        normal_texture: "fx/n.dds".to_string(),
        env_mask_texture: "fx/mask.dds".to_string(),
        env_map_scale: 1.5,
        reflectance_texture: String::new(),
        lighting_texture: String::new(),
        emittance_color: [0.0; 3],
        emit_gradient_texture: String::new(),
        luminance: None,
        starfield_tail: Vec::new(),
    }
}

#[test]
fn capture_lifts_every_rich_field() {
    let shader = fully_populated_fo4_shader();
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.falloff_start_angle, 0.95);
    assert_eq!(captured.falloff_stop_angle, 0.30);
    assert_eq!(captured.falloff_start_opacity, 1.0);
    assert_eq!(captured.falloff_stop_opacity, 0.0);
    assert_eq!(captured.soft_falloff_depth, 8.0);
    assert_eq!(captured.lighting_influence, 200);
    assert_eq!(captured.env_map_min_lod, 4);
    assert_eq!(captured.texture_clamp_mode, 3);
    assert_eq!(captured.env_map_scale, 1.5);
    assert_eq!(captured.greyscale_texture.as_deref(), Some("fx/grad.dds"));
    assert_eq!(captured.env_map_texture.as_deref(), Some("fx/env.dds"));
    assert_eq!(captured.normal_texture.as_deref(), Some("fx/n.dds"));
    assert_eq!(captured.env_mask_texture.as_deref(), Some("fx/mask.dds"));
    // Pre-FO76: refraction_power = 0.0 surfaces as None.
    assert_eq!(captured.refraction_power, None);
}

#[test]
fn capture_collapses_empty_texture_strings_to_none() {
    let mut shader = fully_populated_fo4_shader();
    shader.greyscale_texture.clear();
    shader.env_map_texture.clear();
    shader.normal_texture.clear();
    shader.env_mask_texture.clear();
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.greyscale_texture, None);
    assert_eq!(captured.env_map_texture, None);
    assert_eq!(captured.normal_texture, None);
    assert_eq!(captured.env_mask_texture, None);
}

#[test]
fn capture_surfaces_fo76_refraction_power() {
    let mut shader = fully_populated_fo4_shader();
    shader.refraction_power = 0.5;
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.refraction_power, Some(0.5));
}

/// A material-reference stub, hand-built with the exact placeholder
/// values `BSEffectShaderProperty::material_reference_stub` writes on
/// the real Starfield/FO76 parse path (that constructor is private, so
/// tests mirror its literal field values here — same pattern
/// `fully_populated_fo4_shader` already uses for the non-stub case).
fn material_reference_stub(name: &str) -> BSEffectShaderProperty {
    BSEffectShaderProperty {
        net: NiObjectNETData {
            name: Some(std::sync::Arc::from(name)),
            extra_data_refs: Vec::new(),
            controller_ref: BlockRef::NULL,
        },
        material_reference: true,
        shader_flags_1: 0,
        shader_flags_2: 0,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        source_texture: String::new(),
        texture_clamp_mode: 3,
        lighting_influence: 0,
        env_map_min_lod: 0,
        falloff_start_angle: 1.0,
        falloff_stop_angle: 1.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: [1.0, 1.0, 1.0, 1.0],
        base_color_scale: 1.0,
        soft_falloff_depth: 100.0,
        greyscale_texture: String::new(),
        env_map_texture: String::new(),
        normal_texture: String::new(),
        env_mask_texture: String::new(),
        env_map_scale: 1.0,
        reflectance_texture: String::new(),
        lighting_texture: String::new(),
        emittance_color: [0.0, 0.0, 0.0],
        emit_gradient_texture: String::new(),
        luminance: None,
        starfield_tail: Vec::new(),
    }
}

/// Regression for #2617 / SF-D8-2026-08-07-01: a material-reference stub
/// must contribute its external path only — every fabricated scalar
/// default (`base_color`, `falloff_start/stop_opacity`, …) must NOT be
/// copied into `MaterialInfo`. Pre-fix, `falloff_start_opacity =
/// falloff_stop_opacity = 0.0` (the stub's placeholder pair) landed
/// straight in `info.effect_shader`, and with `triangle.frag`'s cone-fade
/// math assuming the identity default `1.0`/`1.0`, every externally-
/// referenced Starfield effect-shader surface rendered fully transparent.
/// Mirrors `bslighting_material_reference_stub_does_not_claim_material_data`
/// (#2353) on the sibling `BSLightingShaderProperty` guard.
#[test]
fn effect_shader_material_reference_stub_does_not_claim_material_data() {
    let shader = material_reference_stub("materials\\sf\\fx_glow.mat");
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: vec![],
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(0),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };
    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    assert_eq!(
        info.material_path.and_then(|s| pool.resolve(s)),
        Some("materials\\sf\\fx_glow.mat"),
        "the external material path must survive the stub guard"
    );
    // The engine-synthesized "this is an effect shader" tag is real
    // information (not placeholder), independent of whether the actual
    // material body was resolved — kept.
    assert_eq!(info.material_kind, 101);
    assert!(
        !info.has_material_data,
        "stub must not claim inline material authorship"
    );
    assert!(
        matches!(
            info.emissive_source,
            byroredux_core::ecs::components::material::EmissiveSource::None
        ),
        "stub's placeholder base_color must not be tagged as authored Effect emissive"
    );
    assert!(
        info.effect_shader.is_none(),
        "stub's placeholder falloff/greyscale/env fields must not populate \
         effect_shader — this is exactly what fed 0.0/0.0 falloff opacity \
         into the renderer's cone-fade math and rendered the surface invisible"
    );
    assert!(info.texture_path.is_none());
    assert!(info.normal_map.is_none());
    assert!(!info.has_uv_transform);
}

#[test]
fn material_info_default_has_no_effect_shader() {
    // Sibling check — the new field defaults to `None` so non-effect
    // materials don't get spurious capture data.
    let info = MaterialInfo::default();
    assert!(info.effect_shader.is_none());
}

/// Regression for #719 / NIF-D4-03: BSEffectShaderProperty on FO4+ carries
/// `env_map_texture` / `env_mask_texture` (BSVER >= 130 fields).  Pre-fix
/// these were only stored in `effect_shader.env_map_texture` but never
/// forwarded to `MaterialInfo.env_map` / `env_mask`.  The renderer checks
/// `mat.env_map`, so FO4+ effect-shader env reflections silently dropped.
#[test]
fn fo4_effect_shader_env_map_texture_forwards_to_material_info() {
    let mut shader = fully_populated_fo4_shader();
    shader.env_map_texture = "fx/env_cube.dds".to_string();
    shader.env_mask_texture = "fx/env_mask.dds".to_string();

    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };

    // BSEffectShaderProperty is a Skyrim+ shader; bind via shader_property_ref
    // (not the legacy NiProperty chain). Properties chain stays empty.
    let shape = NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: vec![],
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(0),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);

    let env = info.env_map.and_then(|s| pool.resolve(s));
    let mask = info.env_mask.and_then(|s| pool.resolve(s));

    assert_eq!(
        env,
        Some("fx/env_cube.dds"),
        "pre-#719: env_map_texture was captured in effect_shader but never \
         forwarded to MaterialInfo.env_map — renderer env branch stayed dark"
    );
    assert_eq!(
        mask,
        Some("fx/env_mask.dds"),
        "pre-#719: env_mask_texture not forwarded to MaterialInfo.env_mask"
    );
}

/// #3186: the file's BSVER selects texture-slot semantics even when the mesh
/// has only a BSEffectShaderProperty. Before the shared walker seeded this
/// context, only BSLightingShaderProperty updated the layout and this FO4
/// mesh incorrectly escaped with the MaterialInfo default (`Skyrim`).
#[test]
fn fo4_effect_only_mesh_preserves_scene_texture_slot_layout() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(fully_populated_fo4_shader())];
    let scene = NifScene {
        blocks,
        bsver: crate::version::bsver::FALLOUT4,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info =
        extract_material_info_from_refs(&scene, BlockRef(0), BlockRef::NULL, &[], &[], &mut pool);
    assert_eq!(info.texture_slot_layout, TextureSlotLayout::Fallout4);
}

// ── #890 / SK-D4-NEW-04 — BSEffect flag-bit capture ──────────────
//
// Pre-fix the four BSEffect-relevant flag bits (Soft_Effect,
// Greyscale_To_Palette_Color / _Alpha, Effect_Lighting) were parsed on
// the wire but never lifted into `BsEffectShaderData`. These tests pin
// both lift paths: the typed-flag word (Skyrim / FO4 / pre-FO76) AND
// the FO76 / Starfield CRC32 list (BSVER >= 132 — typed words zero,
// CRCs carry the signal).

#[test]
fn capture_default_effect_shader_has_no_flag_bits() {
    let shader = fully_populated_fo4_shader();
    let captured = capture_effect_shader_data(&shader);
    assert!(!captured.effect_soft);
    assert!(!captured.effect_palette_color);
    assert!(!captured.effect_palette_alpha);
    assert!(!captured.effect_lit);
}

#[test]
fn capture_soft_effect_typed_flag() {
    let mut shader = fully_populated_fo4_shader();
    shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::SOFT_EFFECT;
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_soft);
    assert!(!captured.effect_palette_color);
    assert!(!captured.effect_palette_alpha);
    assert!(!captured.effect_lit);
}

#[test]
fn capture_soft_effect_crc_fallback() {
    // FO76 / Starfield path — typed flag is zero, CRC array carries the
    // signal. nif.xml writes the CRC into sf1_crcs OR sf2_crcs; the
    // capture must consult the union of both.
    let mut shader = fully_populated_fo4_shader();
    shader.sf1_crcs = vec![crate::shader_flags::bs_shader_crc32::SOFT_EFFECT];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_soft);

    let mut shader = fully_populated_fo4_shader();
    shader.sf2_crcs = vec![crate::shader_flags::bs_shader_crc32::SOFT_EFFECT];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_soft);
}

#[test]
fn capture_palette_color_typed_flag() {
    let mut shader = fully_populated_fo4_shader();
    shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::GREYSCALE_TO_PALETTE_COLOR;
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_color);
    assert!(!captured.effect_soft);
    assert!(!captured.effect_palette_alpha);
}

#[test]
fn capture_palette_color_crc_fallback() {
    let mut shader = fully_populated_fo4_shader();
    shader.sf1_crcs = vec![crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_COLOR];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_color);

    let mut shader = fully_populated_fo4_shader();
    shader.sf2_crcs = vec![crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_COLOR];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_color);
}

#[test]
fn capture_palette_alpha_typed_flag() {
    let mut shader = fully_populated_fo4_shader();
    shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::GREYSCALE_TO_PALETTE_ALPHA;
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_alpha);
    assert!(!captured.effect_palette_color);
}

#[test]
fn capture_palette_alpha_crc_fallback() {
    let mut shader = fully_populated_fo4_shader();
    shader.sf1_crcs = vec![crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_ALPHA];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_alpha);

    let mut shader = fully_populated_fo4_shader();
    shader.sf2_crcs = vec![crate::shader_flags::bs_shader_crc32::GRAYSCALE_TO_PALETTE_ALPHA];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_palette_alpha);
}

#[test]
fn capture_effect_lit_typed_flag() {
    // Effect_Lighting lives on `shader_flags_2` (SLSF2), distinct from
    // the three SLSF1 bits above.
    let mut shader = fully_populated_fo4_shader();
    shader.shader_flags_2 = crate::shader_flags::skyrim_slsf2::EFFECT_LIGHTING;
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_lit);
    assert!(!captured.effect_soft);
}

#[test]
fn capture_effect_lit_crc_fallback() {
    let mut shader = fully_populated_fo4_shader();
    shader.sf1_crcs = vec![crate::shader_flags::bs_shader_crc32::EFFECT_LIGHTING];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_lit);

    let mut shader = fully_populated_fo4_shader();
    shader.sf2_crcs = vec![crate::shader_flags::bs_shader_crc32::EFFECT_LIGHTING];
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_lit);
}

#[test]
fn capture_all_four_bits_together() {
    // Confirm the four helpers don't interfere with each other when
    // multiple bits are set on the same shader.
    let mut shader = fully_populated_fo4_shader();
    shader.shader_flags_1 = crate::shader_flags::skyrim_slsf1::SOFT_EFFECT
        | crate::shader_flags::skyrim_slsf1::GREYSCALE_TO_PALETTE_COLOR
        | crate::shader_flags::skyrim_slsf1::GREYSCALE_TO_PALETTE_ALPHA;
    shader.shader_flags_2 = crate::shader_flags::skyrim_slsf2::EFFECT_LIGHTING;
    let captured = capture_effect_shader_data(&shader);
    assert!(captured.effect_soft);
    assert!(captured.effect_palette_color);
    assert!(captured.effect_palette_alpha);
    assert!(captured.effect_lit);
}

// ── #1205 — FO76 quintet (BSVER == 155) capture-side plumbing ──────
//
// Pre-fix the BSEffectShaderProperty parser read these five fields off
// the wire but `capture_effect_shader_data` had no destination for them
// on `BsEffectShaderData`. They're additive data plumbing — the
// renderer-side dispatch is follow-up work. Tests pin the
// capture round-trip + empty-collapse semantics.

#[test]
fn fo76_defaults_collapse_to_none() {
    // The `fully_populated_fo4_shader` fixture writes empty strings +
    // `[0.0; 3]` + `None` for the FO76 quintet — verify capture
    // collapses to all-None.
    let shader = fully_populated_fo4_shader();
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.reflectance_texture, None);
    assert_eq!(captured.lighting_texture, None);
    assert_eq!(captured.emit_gradient_texture, None);
    assert_eq!(captured.emittance_color, None);
    assert!(captured.luminance.is_none());
}

#[test]
fn fo76_quintet_round_trips_through_capture() {
    let mut shader = fully_populated_fo4_shader();
    shader.reflectance_texture = "fx/refl.dds".to_string();
    shader.lighting_texture = "fx/lit.dds".to_string();
    shader.emit_gradient_texture = "fx/emit_grad.dds".to_string();
    shader.emittance_color = [1.5, 0.5, 2.0];
    shader.luminance = Some(crate::blocks::shader::LuminanceParams {
        lum_emittance: 100.0,
        exposure_offset: -2.0,
        final_exposure_min: 0.01,
        final_exposure_max: 8.0,
    });

    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.reflectance_texture.as_deref(), Some("fx/refl.dds"));
    assert_eq!(captured.lighting_texture.as_deref(), Some("fx/lit.dds"));
    assert_eq!(
        captured.emit_gradient_texture.as_deref(),
        Some("fx/emit_grad.dds")
    );
    assert_eq!(captured.emittance_color, Some([1.5, 0.5, 2.0]));
    let lum = captured.luminance.expect("luminance must round-trip");
    assert_eq!(lum.lum_emittance, 100.0);
    assert_eq!(lum.exposure_offset, -2.0);
    assert_eq!(lum.final_exposure_min, 0.01);
    assert_eq!(lum.final_exposure_max, 8.0);
}

#[test]
fn fo76_emittance_color_zero_treated_as_sentinel() {
    // `[0, 0, 0]` is Bethesda's pre-FO76 default — the capture surfaces
    // it as `None` so consumers can distinguish "FO76 set black" (very
    // rare) from "non-FO76 with zero-fill" (the common case). Documented
    // sentinel-pattern; if a real corpus turns up a literal-black FO76
    // emittance this test must change deliberately.
    let mut shader = fully_populated_fo4_shader();
    shader.emittance_color = [0.0, 0.0, 0.0];
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.emittance_color, None);
}

#[test]
fn fo76_partial_texture_population() {
    // Only some of the FO76 texture slots populated — every present one
    // surfaces; absent ones stay None. Mirrors the pre-existing FO4
    // env-map / env-mask partial-population behaviour.
    let mut shader = fully_populated_fo4_shader();
    shader.reflectance_texture = "fx/refl.dds".to_string();
    let captured = capture_effect_shader_data(&shader);
    assert_eq!(captured.reflectance_texture.as_deref(), Some("fx/refl.dds"));
    assert_eq!(captured.lighting_texture, None);
    assert_eq!(captured.emit_gradient_texture, None);
}
