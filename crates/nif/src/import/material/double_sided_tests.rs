//! Tests for `double_sided_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`double_sided_tests::FOO`).

use super::*;
use crate::blocks::base::{BSShaderPropertyData, NiObjectNETData};
use crate::blocks::shader::{
    BSLightingShaderProperty, BSShaderNoLightingProperty, BSShaderPPLightingProperty,
    ShaderTypeData,
};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;

/// Test helper — runs the walker against a fresh per-call `StringPool`.
/// Tests in this file only assert on flag fields, never on path
/// strings, so the pool's lifetime can stay scoped to one assertion.
/// See #609 / D6-NEW-01 for the FixedString plumbing motivation.
fn extract_with_pool(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited: &[BlockRef],
) -> MaterialInfo {
    let mut pool = StringPool::new();
    extract_material_info(scene, shape, inherited, &mut pool)
}

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn make_pp_lighting(flags1: u32, flags2: u32) -> BSShaderPPLightingProperty {
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: flags1,
            shader_flags_2: flags2,
            env_map_scale: 0.0,
        },
        texture_clamp_mode: 0,
        texture_set_ref: BlockRef::NULL,
        refraction_strength: 0.0,
        refraction_fire_period: 0,
        parallax_max_passes: 4.0,
        parallax_scale: 0.04,
        emissive_color: [0.0, 0.0, 0.0, 1.0],
    }
}

fn make_no_lighting(flags1: u32) -> BSShaderNoLightingProperty {
    BSShaderNoLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: flags1,
            shader_flags_2: 0,
            env_map_scale: 0.0,
        },
        texture_clamp_mode: 0,
        file_name: String::new(),
        falloff_start_angle: 0.0,
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
    }
}

fn make_bs_lighting(flags2: u32) -> BSLightingShaderProperty {
    make_bs_lighting_with_flags(0, flags2)
}

/// Variant of [`make_bs_lighting`] with both flag words overridable
/// — used by #414's FO4 decal regression test so
/// `shader_flags_2 = Anisotropic_Lighting` can be tested without
/// unrelated bits.
fn make_bs_lighting_with_flags(flags1: u32, flags2: u32) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net(),
        material_reference: false,
        shader_flags_1: flags1,
        shader_flags_2: flags2,
        sf1_crcs: Vec::new(),
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_set_ref: BlockRef::NULL,
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        texture_clamp_mode: 0,
        alpha: 1.0,
        refraction_strength: 0.0,
        glossiness: 80.0,
        specular_color: [1.0; 3],
        specular_strength: 1.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        grayscale_to_palette_scale: 0.0,
        fresnel_power: 0.0,
        wetness: None,
        luminance: None,
        do_translucency: false,
        translucency: None,
        texture_arrays: Vec::new(),
        shader_type_data: ShaderTypeData::None,
    }
}

fn shape_with_shader_ref(ref_idx: u32) -> NiTriShape {
    use crate::blocks::base::NiAVObjectData;
    NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: vec![BlockRef(ref_idx)],
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

/// FO3/FNV: flags1 bit 12 is `Unknown_3`, NOT Double_Sided.
/// Pre-fix this came back as `two_sided = true`; now it must not.
#[test]
fn fo3_pp_lighting_flags1_bit12_is_not_double_sided() {
    let shader = make_pp_lighting(0x1000, 0); // Unknown_3 set on its own.
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        !info.two_sided,
        "FO3 PPLighting flags1 bit 12 (Unknown_3) must NOT mark two_sided (#441)"
    );
}

/// Same for BSShaderNoLightingProperty — the pre-fix #441 site at
/// the `NoLighting` branch applied the same wrong mask.
#[test]
fn fo3_no_lighting_flags1_bit12_is_not_double_sided() {
    let shader = make_no_lighting(0x1000);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        !info.two_sided,
        "FO3 NoLighting flags1 bit 12 (Unknown_3) must NOT mark two_sided (#441)"
    );
}

/// FO3/FNV: flags2 bit 4 is `Refraction_Tint` per the
/// `Fallout3ShaderPropertyFlags2` enum in nif.xml — also NOT
/// Double_Sided. The PPLighting branch must not test this bit on
/// the FO3 path either.
#[test]
fn fo3_pp_lighting_flags2_bit4_refraction_tint_is_not_double_sided() {
    let shader = make_pp_lighting(0, 0x10); // Refraction_Tint
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        !info.two_sided,
        "FO3 PPLighting flags2 bit 4 (Refraction_Tint) must NOT mark two_sided (#441)"
    );
}

/// Skyrim+ `BSLightingShaderProperty`: flags2 bit 4 IS Double_Sided
/// per `SkyrimShaderPropertyFlags2`. The per-game dispatch preserves
/// this path.
#[test]
fn skyrim_bs_lighting_flags2_bit4_marks_double_sided() {
    let shader = make_bs_lighting(0x10);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    // BSLightingShaderProperty attaches via shader_property_ref, not
    // the inherited `properties` list.
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        info.two_sided,
        "Skyrim BSLightingShaderProperty flags2 bit 4 MUST mark two_sided (#441)"
    );
}

/// Regression: #454 — `BSShaderNoLightingProperty` decal detection
/// was missing the `ALPHA_DECAL_F2` (flags2 bit 21) check. A
/// blood-splat NoLighting mesh that marks itself decal-only via
/// flag2 bit 21 (no flag1 bits set) must still be classified as a
/// decal. The shared `is_decal_from_shader_flags` helper keeps the
/// PPLighting and NoLighting paths in lockstep.
#[test]
fn no_lighting_alpha_decal_flag2_marks_is_decal() {
    use crate::blocks::shader::BSShaderNoLightingProperty;
    let shader = BSShaderNoLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: 0,           // no flag1 bits
            shader_flags_2: 0x0020_0000, // ALPHA_DECAL_F2 only
            env_map_scale: 0.0,
        },
        texture_clamp_mode: 0,
        file_name: String::new(),
        falloff_start_angle: 0.0,
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        info.is_decal,
        "NoLighting flags2 bit 21 (ALPHA_DECAL_F2) MUST mark is_decal (#454)"
    );
}

/// Legacy (FO3/FNV) helper sanity — both flag1 decal bits and the
/// FO3/FNV-specific flag2 `Alpha_Decal` path classify as decal.
#[test]
fn is_decal_legacy_helper_matches_both_flag_sources() {
    use super::is_decal_from_legacy_shader_flags;
    // DECAL_SINGLE_PASS (flag1 bit 26 = 0x0400_0000).
    assert!(is_decal_from_legacy_shader_flags(0x0400_0000, 0));
    // DYNAMIC_DECAL (flag1 bit 27 = 0x0800_0000).
    assert!(is_decal_from_legacy_shader_flags(0x0800_0000, 0));
    // ALPHA_DECAL_F2 (flag2 bit 21 = 0x0020_0000) — FO3/FNV only.
    assert!(is_decal_from_legacy_shader_flags(0, 0x0020_0000));
    // Unrelated bits — not a decal.
    assert!(!is_decal_from_legacy_shader_flags(0x1000, 0x0010));
    assert!(!is_decal_from_legacy_shader_flags(0, 0));
}

/// #414 / FO4-D3-M1 regression — the modern (Skyrim+/FO4) decal
/// helper MUST NOT test flag2 bit 21. On Skyrim that bit is
/// `Cloud_LOD`; on FO4 it's `Anisotropic_Lighting`. Neither is a
/// decal flag, and the pre-fix `is_decal_from_shader_flags` helper
/// misclassified those meshes as decals → unwanted depth-bias.
#[test]
fn is_decal_modern_helper_ignores_flag2_bit_21() {
    use super::is_decal_from_modern_shader_flags;
    // SLSF1 / F4SF1 bit 26 — shared with legacy, must classify.
    assert!(is_decal_from_modern_shader_flags(0x0400_0000, 0, &[], &[]));
    assert!(is_decal_from_modern_shader_flags(0x0800_0000, 0, &[], &[]));
    // Flag2 bit 21 — Cloud_LOD on Skyrim / Anisotropic_Lighting on
    // FO4. MUST NOT classify as decal.
    assert!(!is_decal_from_modern_shader_flags(0, 0x0020_0000, &[], &[]));
    // Sanity: unrelated bits, all zeros.
    assert!(!is_decal_from_modern_shader_flags(0, 0, &[], &[]));
    assert!(!is_decal_from_modern_shader_flags(0x1000, 0x0010, &[], &[]));
}

/// #414 end-to-end: a FO4-shaped `BSLightingShaderProperty` with
/// `Anisotropic_Lighting` set (F4SF2 bit 21) must parse through
/// `extract_material_info` with `is_decal == false`. Pre-fix the
/// shared `is_decal_from_shader_flags` helper read that bit as
/// FO3/FNV `Alpha_Decal` and flipped `is_decal`.
#[test]
fn fo4_anisotropic_lighting_does_not_trigger_decal_classification() {
    let shader = make_bs_lighting_with_flags(0, 0x0020_0000);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        !info.is_decal,
        "FO4 Anisotropic_Lighting (F4SF2 bit 21) MUST NOT mark is_decal (#414)"
    );
}

/// Skyrim+ shader with flags2 = 0 must NOT mark two-sided either —
/// pins the semantic from the opposite direction.
#[test]
fn skyrim_bs_lighting_flags2_zero_leaves_default_culling() {
    let shader = make_bs_lighting(0);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(!info.two_sided);
}

// ── #712 / NIF-D4-01 — FO76/Starfield CRC32 shader-flag fallback ──────

/// Inject CRC arrays into a `BSLightingShaderProperty` whose legacy
/// flag pair is zero — the FO76/Starfield shape on disk per
/// `shader.rs:604-608`.
fn make_bs_lighting_with_crcs(sf1: Vec<u32>, sf2: Vec<u32>) -> BSLightingShaderProperty {
    let mut shader = make_bs_lighting_with_flags(0, 0);
    shader.sf1_crcs = sf1;
    shader.sf2_crcs = sf2;
    shader
}

/// Pre-#712 a Starfield decal mesh imports with `is_decal == false`
/// because the legacy `shader_flags_1` field is zero on BSVER >= 132 and
/// nothing read the CRC arrays. Now: the CRC `BSShaderCRC32::DECAL`
/// (3849131744) found anywhere in `sf1_crcs` or `sf2_crcs` flips
/// `is_decal` to true.
#[test]
fn starfield_decal_crc_flips_is_decal_when_legacy_flags_are_zero() {
    use crate::shader_flags::bs_shader_crc32;

    let shader = make_bs_lighting_with_crcs(vec![bs_shader_crc32::DECAL], Vec::new());
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        info.is_decal,
        "Starfield decal CRC in sf1_crcs must flip is_decal"
    );
}

/// `Dynamic_Decal` CRC found via the SF2 (second) array also flips
/// `is_decal`. The split between SF1 and SF2 is purely a wire detail —
/// the same `BSShaderCRC32` enum populates both. See nif.xml lines
/// 6590–6591.
#[test]
fn starfield_dynamic_decal_crc_in_sf2_array_flips_is_decal() {
    use crate::shader_flags::bs_shader_crc32;

    let shader = make_bs_lighting_with_crcs(Vec::new(), vec![bs_shader_crc32::DYNAMIC_DECAL]);
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(info.is_decal);
}

/// `BSShaderCRC32::TWO_SIDED` flips `info.two_sided` on the modern path.
/// Pre-fix every Starfield grass / hair / cloth mesh rendered with
/// backface culling on regardless of the authored Two_Sided flag.
#[test]
fn starfield_two_sided_crc_flips_two_sided_when_legacy_flags_are_zero() {
    use crate::shader_flags::bs_shader_crc32;

    let shader = make_bs_lighting_with_crcs(vec![bs_shader_crc32::TWO_SIDED], Vec::new());
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(
        info.two_sided,
        "Starfield Two_Sided CRC must flip info.two_sided"
    );
}

/// CRC arrays carrying unrelated flags (e.g. `Skinned`, `Cast_Shadows`)
/// do NOT flip `is_decal` or `two_sided`. Guards against the obvious
/// "any CRC means decal" miswire.
#[test]
fn starfield_unrelated_crcs_do_not_trigger_decal_or_two_sided() {
    use crate::shader_flags::bs_shader_crc32;

    let shader = make_bs_lighting_with_crcs(
        vec![bs_shader_crc32::SKINNED, bs_shader_crc32::CAST_SHADOWS],
        vec![bs_shader_crc32::ZBUFFER_TEST],
    );
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let mut shape = shape_with_shader_ref(0);
    shape.av.properties.clear();
    shape.shader_property_ref = BlockRef(0);
    let info = extract_with_pool(&scene, &shape, &[]);
    assert!(!info.is_decal, "Skinned/CastShadows/ZBuffer CRCs are not decal");
    assert!(!info.two_sided);
}

/// Helper-level regression: when both legacy flags AND the CRC arrays
/// are empty, neither is_decal nor two_sided fires. Anchors the
/// "no false positives on empty input" invariant.
#[test]
fn modern_helpers_return_false_on_empty_input() {
    use super::{is_decal_from_modern_shader_flags, is_two_sided_from_modern_shader_flags};
    assert!(!is_decal_from_modern_shader_flags(0, 0, &[], &[]));
    assert!(!is_two_sided_from_modern_shader_flags(0, 0, &[], &[]));
}

/// Helper-level regression: legacy bits still classify (BSVER < 132
/// content where `shader_flags_*` is non-zero and CRC arrays are
/// empty). Pin the back-compat invariant so the CRC fallback addition
/// can't accidentally regress legacy meshes.
#[test]
fn modern_helpers_still_honour_legacy_bits_when_crcs_empty() {
    use super::{is_decal_from_modern_shader_flags, is_two_sided_from_modern_shader_flags};
    // SLSF1 / F4SF1 bit 26 == Decal — same numeric value across games.
    assert!(is_decal_from_modern_shader_flags(0x0400_0000, 0, &[], &[]));
    // SLSF2 / F4SF2 bit 4 == Double_Sided.
    assert!(is_two_sided_from_modern_shader_flags(0, 0x0000_0010, &[], &[]));
}
