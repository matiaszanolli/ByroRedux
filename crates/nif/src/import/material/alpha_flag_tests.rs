//! Tests for `alpha_flag_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`alpha_flag_tests::FOO`).

//! Regression tests for issue #152 — NiAlphaProperty bit extraction.
//! Verify the cutout-vs-blend precedence and threshold scaling.
use super::*;
use crate::blocks::base::NiObjectNETData;

fn alpha_prop(flags: u16, threshold: u8) -> NiAlphaProperty {
    NiAlphaProperty {
        net: NiObjectNETData {
            name: None,
            extra_data_refs: Vec::new(),
            controller_ref: crate::types::BlockRef::NULL,
        },
        flags,
        threshold,
    }
}

#[test]
fn alpha_blend_only_sets_blend() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0001, 128));
    assert!(info.alpha_blend);
    assert!(!info.alpha_test);
    assert_eq!(info.alpha_threshold, 0.0);
}

#[test]
fn alpha_test_only_sets_test_and_scales_threshold() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
    assert!(!info.alpha_blend);
    assert!(info.alpha_test);
    assert!((info.alpha_threshold - (128.0 / 255.0)).abs() < 1e-5);
}

#[test]
fn alpha_test_and_blend_prefers_test() {
    // Foliage with both bits set: alpha-test wins because the
    // discard + depth-write path sorts cleanly without back-to-front
    // pre-sort of the alpha-blend pipeline.
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0201, 200));
    assert!(!info.alpha_blend, "alpha_blend should yield to alpha_test");
    assert!(info.alpha_test);
    assert!((info.alpha_threshold - (200.0 / 255.0)).abs() < 1e-5);
}

#[test]
fn neither_bit_leaves_defaults() {
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0000, 255));
    assert!(!info.alpha_blend);
    assert!(!info.alpha_test);
    assert_eq!(info.alpha_threshold, 0.0);
}

#[test]
fn threshold_extremes_clamp_expected_range() {
    let mut info_min = MaterialInfo::default();
    apply_alpha_flags(&mut info_min, &alpha_prop(0x0200, 0));
    assert_eq!(info_min.alpha_threshold, 0.0);

    let mut info_max = MaterialInfo::default();
    apply_alpha_flags(&mut info_max, &alpha_prop(0x0200, 255));
    assert!((info_max.alpha_threshold - 1.0).abs() < 1e-5);
}

/// #263: alpha test function bits 10-12 are extracted.
#[test]
fn alpha_test_func_greaterequal_default() {
    // flags = 0x1A00: test enable (0x200) + GREATEREQUAL (6 << 10 = 0x1800)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x1A00, 128));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL
}

#[test]
fn alpha_test_func_less() {
    // flags = 0x0600: test enable (0x200) + LESS (1 << 10 = 0x400)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0600, 64));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 1); // LESS
}

#[test]
fn alpha_test_func_always() {
    // flags = 0x0200: test enable (0x200) + ALWAYS (0 << 10 = 0x000)
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0200, 128));
    assert!(info.alpha_test);
    assert_eq!(info.alpha_test_func, 0); // ALWAYS
}

#[test]
fn alpha_test_func_default_when_no_test() {
    // When alpha test is disabled, func should stay at default (6).
    let info = MaterialInfo::default();
    assert_eq!(info.alpha_test_func, 6); // GREATEREQUAL default
}

// ── #1201 — cascade gate honours `alpha_property_consumed` ─────────
//
// Pre-#1201 the cascade gate at `walker.rs:494` read
// `!alpha_blend && !alpha_test`. A direct shape NiAlphaProperty with
// `flags=0` (explicit "no blend, no test") left both bits false and
// admitted the inherited parent NiNode's NiAlphaProperty, silently
// overwriting the shape's intent. #982 added `alpha_property_consumed`
// but the consumer change was never made.

#[test]
fn flags_zero_alpha_property_marks_consumption() {
    // Defensive baseline: even `flags=0` must mark the property as
    // consumed so the cascade gate closes.
    let mut info = MaterialInfo::default();
    apply_alpha_flags(&mut info, &alpha_prop(0x0000, 128));
    assert!(!info.alpha_blend);
    assert!(!info.alpha_test);
    assert!(
        info.alpha_property_consumed,
        "apply_alpha_flags must mark the property consumed even when \
         flags=0 (#1201 — gates the cascade in walker.rs)",
    );
}

#[test]
fn explicit_opaque_shape_blocks_inherited_blend_cascade() {
    // The bug case: shape authors `NiAlphaProperty { flags: 0 }`
    // (block 0); parent NiNode authors `NiAlphaProperty { flags: 1 }`
    // (block 1). Walk the inherited-property loop in
    // `extract_material_info_from_refs` with the shape's property at
    // [BlockRef(0)] (direct) and the parent's at [BlockRef(1)]
    // (inherited). Expect alpha_blend == false (shape intent wins).
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(alpha_prop(0x0000, 128)), // shape — explicit opaque
        Box::new(alpha_prop(0x0001, 128)), // parent — alpha blend
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef::NULL,
        BlockRef::NULL,
        &[BlockRef(0)],
        &[BlockRef(1)],
        &mut pool,
    );
    assert!(
        !info.alpha_blend,
        "explicit-opaque shape NiAlphaProperty must close the cascade \
         gate against the inherited parent NiAlphaProperty (#1201)",
    );
    assert!(!info.alpha_test);
    assert!(info.alpha_property_consumed);
}

/// #1202 — BSEffectShader-backed shape with explicit-opaque
/// `NiAlphaProperty { flags: 0 }` bound via `alpha_property_ref` must
/// suppress the BSEffectShader implicit alpha-blend. Pre-fix the
/// implicit `alpha_blend = true` at walker.rs:427 ran inside the
/// shader-property block (line 298+) before `apply_alpha_flags` saw
/// the NiAlphaProperty at line 480; the gate `!alpha_blend &&
/// !alpha_test` admitted the implicit write, and the subsequent
/// `apply_alpha_flags(flags=0)` had nothing to clear.
#[test]
fn bs_effect_shader_explicit_opaque_blocks_implicit_blend() {
    use crate::blocks::shader::BSEffectShaderProperty;
    let blocks: Vec<Box<dyn NiObject>> = vec![
        // block 0 — BSEffectShader (would imply alpha_blend = true)
        Box::new(BSEffectShaderProperty {
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
            lighting_influence: 0,
            env_map_min_lod: 0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 0.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0,
            base_color: [0.0; 4],
            base_color_scale: 1.0,
            soft_falloff_depth: 0.0,
            greyscale_texture: String::new(),
            env_map_texture: String::new(),
            normal_texture: String::new(),
            env_mask_texture: String::new(),
            env_map_scale: 1.0,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0; 3],
            emit_gradient_texture: String::new(),
            luminance: None,
        }),
        // block 1 — explicit-opaque NiAlphaProperty
        Box::new(alpha_prop(0x0000, 128)),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef(0),       // shader_property_ref → BSEffectShader
        BlockRef(1),       // alpha_property_ref → flags=0
        &[],
        &[],
        &mut pool,
    );
    assert!(
        !info.alpha_blend,
        "BSEffectShader implicit blend must yield to explicit \
         NiAlphaProperty(flags=0) (#1202)",
    );
    assert!(!info.alpha_test);
    assert!(info.alpha_property_consumed);
}

/// Non-regression: BSEffectShader-backed shape with NO
/// `alpha_property_ref` still gets the implicit blend (the common case
/// for `meshes/effects/*.nif` glow rings / magic flares / smoke cards).
#[test]
fn bs_effect_shader_without_alpha_property_still_gets_implicit_blend() {
    use crate::blocks::shader::BSEffectShaderProperty;
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(BSEffectShaderProperty {
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
        lighting_influence: 0,
        env_map_min_lod: 0,
        falloff_start_angle: 1.0,
        falloff_stop_angle: 1.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
        refraction_power: 0.0,
        base_color: [0.0; 4],
        base_color_scale: 1.0,
        soft_falloff_depth: 0.0,
        greyscale_texture: String::new(),
        env_map_texture: String::new(),
        normal_texture: String::new(),
        env_mask_texture: String::new(),
        env_map_scale: 1.0,
        reflectance_texture: String::new(),
        lighting_texture: String::new(),
        emittance_color: [0.0; 3],
        emit_gradient_texture: String::new(),
        luminance: None,
    })];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef(0),
        BlockRef::NULL,    // no explicit alpha property
        &[],
        &[],
        &mut pool,
    );
    assert!(
        info.alpha_blend,
        "BSEffectShader without explicit NiAlphaProperty must keep \
         implicit blend (non-regression on #354)",
    );
}

#[test]
fn no_direct_alpha_property_still_consumes_inherited() {
    // Non-regression: when the shape has no direct NiAlphaProperty,
    // the inherited one must still apply. This is the pre-#1201
    // behaviour for any shape without a direct property.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(alpha_prop(0x0001, 128))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut pool = StringPool::new();
    let info = walker::extract_material_info_from_refs(
        &scene,
        BlockRef::NULL,
        BlockRef::NULL,
        &[],                // no direct properties
        &[BlockRef(0)],     // inherited only
        &mut pool,
    );
    assert!(
        info.alpha_blend,
        "no direct property → inherited alpha-blend must reach info",
    );
    assert!(info.alpha_property_consumed);
}
