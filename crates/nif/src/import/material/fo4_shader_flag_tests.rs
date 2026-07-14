//! #1592 (FO4-D5-MEDIUM-01) — FO4 `BSLightingShaderProperty` shader-flag
//! bits OR'd into `MaterialInfo` at import.
//!
//! For an FO4 (BSVER >= 130) `BSLightingShaderProperty` the walker now
//! consumes `F4SF1::Model_Space_Normals` and `F4SF2::Alpha_Test` (plus the
//! FO76+ `MODELSPACENORMALS` CRC) when no companion BGSM overrides them.
//! The capture is gated on the FO4 BSVER so a Skyrim property — same block,
//! different flag vocabulary — isn't read with FO4 semantics (`F4SF2` bit
//! 25 is not alpha-test on Skyrim, which routes that via `NiAlphaProperty`).

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::shader::{BSLightingShaderProperty, ShaderTypeData};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::shader_flags::{bs_shader_crc32, fo4_slsf1, fo4_slsf2};
use crate::types::{BlockRef, NiTransform};
use crate::version::bsver;
use byroredux_core::string::StringPool;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn make_bslsp(flags1: u32, flags2: u32, sf1_crcs: Vec<u32>) -> BSLightingShaderProperty {
    BSLightingShaderProperty {
        shader_type: 0,
        net: empty_net(),
        material_reference: false,
        shader_flags_1: flags1,
        shader_flags_2: flags2,
        sf1_crcs,
        sf2_crcs: Vec::new(),
        uv_offset: [0.0, 0.0],
        uv_scale: [1.0, 1.0],
        texture_set_ref: BlockRef::NULL,
        emissive_color: [0.0; 3],
        emissive_multiple: 1.0,
        root_material_path: None,
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
        starfield_tail: Vec::new(),
    }
}

/// Shape carrying the shader via `shader_property_ref` (the Skyrim+/FO4
/// attach path), not the inherited `properties` list.
fn shape_with_shader_ref(ref_idx: u32) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef(ref_idx),
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    }
}

fn extract(shader: BSLightingShaderProperty, bsver_val: u32) -> MaterialInfo {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(shader)];
    let scene = NifScene {
        blocks,
        bsver: bsver_val,
        ..NifScene::default()
    };
    let shape = shape_with_shader_ref(0);
    let mut pool = StringPool::new();
    extract_material_info(&scene, &shape, &[], &mut pool)
}

/// FO4 BSVER + `F4SF1::Model_Space_Normals` → `model_space_normals`.
/// Pre-fix the bit was parsed but never consumed (BGSM-only path).
#[test]
fn fo4_model_space_normals_flag_sets_field() {
    let shader = make_bslsp(fo4_slsf1::MODEL_SPACE_NORMALS, 0, Vec::new());
    let info = extract(shader, bsver::FALLOUT4);
    assert!(
        info.model_space_normals,
        "FO4 F4SF1 bit 12 must OR model_space_normals into MaterialInfo (#1592)"
    );
    assert!(!info.alpha_test, "alpha-test bit was not set");
}

/// FO4 BSVER + `F4SF2::Alpha_Test` → `alpha_test`. The NiAlphaProperty
/// path is absent here, so the shader flag is the only signal.
#[test]
fn fo4_alpha_test_flag_sets_field() {
    let shader = make_bslsp(0, fo4_slsf2::ALPHA_TEST, Vec::new());
    let info = extract(shader, bsver::FALLOUT4);
    assert!(
        info.alpha_test,
        "FO4 F4SF2 bit 25 must OR alpha_test into MaterialInfo (#1592)"
    );
    // #1985 (FO4-D5-01): the flag alone must also seed a shader-usable
    // threshold. `triangle.frag` gates the discard on `alphaThreshold > 0.0`,
    // so the default 0.0 would leave the cutout inert (a solid opaque quad).
    // With no NiAlphaProperty present, the walker seeds Bethesda's 128/255.
    assert!(
        info.alpha_threshold > 0.0,
        "FO4 shader-flag-only alpha test must seed a usable threshold, not leave it 0.0 (#1985)"
    );
    assert_eq!(
        info.alpha_threshold,
        128.0 / 255.0,
        "expected Bethesda's conventional 128/255 cutout threshold (#1985)"
    );
    assert!(!info.model_space_normals, "MSN bit was not set");
}

/// The gate is exclusive: a Skyrim (BSVER 100) property with the SAME
/// numeric bits set must NOT pick up FO4 semantics. On Skyrim F4SF2 bit 25
/// is not alpha-test (routed via NiAlphaProperty) — reading it here would
/// spuriously cut out opaque Skyrim meshes.
#[test]
fn skyrim_bsver_does_not_read_fo4_f2_alpha_test() {
    let shader = make_bslsp(0, fo4_slsf2::ALPHA_TEST, Vec::new());
    let info = extract(shader, bsver::SKYRIM_SE);
    assert!(
        !info.alpha_test,
        "Skyrim BSVER must not read F4SF2 bit 25 as alpha-test (#1592 gate)"
    );
}

/// FO4 BSVER with no shader flags set leaves both at their defaults — the
/// capture is additive, never fabricated.
#[test]
fn fo4_no_flags_leaves_defaults() {
    let info = extract(make_bslsp(0, 0, Vec::new()), bsver::FALLOUT4);
    assert!(!info.model_space_normals);
    assert!(!info.alpha_test);
}

/// FO76 / Starfield (BSVER >= 132) store the typed flag words as zero and
/// carry the identifiers in the CRC arrays instead. The MODELSPACENORMALS
/// CRC on `sf1_crcs` must still set `model_space_normals`.
#[test]
fn fo76_crc_model_space_normals_sets_field() {
    let shader = make_bslsp(0, 0, vec![bs_shader_crc32::MODELSPACENORMALS]);
    let info = extract(shader, bsver::FO76);
    assert!(
        info.model_space_normals,
        "FO76+ MODELSPACENORMALS CRC must OR model_space_normals (#1592)"
    );
}
