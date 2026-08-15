//! Regression tests for #2553 (FNV-D2-01) — unauthored-vs-authored-off
//! for legacy `NiMaterialProperty.specular`.
//!
//! On the FO3/FNV lit pipeline (`BSShaderPPLightingProperty` /
//! `BSShaderNoLightingProperty`) the specular term came from the shader /
//! normal-map alpha, not from `NiMaterialProperty`, so vanilla content
//! leaves that field black essentially everywhere. Forwarding the zero as
//! *authored* collapsed the whole direct-specular lobe downstream
//! (`triangle.frag` multiplies the GGX term by `specStrength * specColor`
//! with no floor).
//!
//! The distinction is made ONCE here at the NIFAL boundary and never
//! re-derived at render time.

use super::*;
use crate::blocks::base::BSShaderPropertyData;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::properties::{NiFlagProperty, NiMaterialProperty};
use crate::blocks::shader::{BSShaderNoLightingProperty, BSShaderPPLightingProperty};
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiColor, NiTransform};
use byroredux_core::string::StringPool;
use std::sync::Arc;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn make_tri_shape_with_props(properties: Vec<BlockRef>) -> NiTriShape {
    NiTriShape {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TestShape")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties,
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

/// `NiMaterialProperty` with a caller-chosen specular colour; every other
/// field is the vanilla-typical neutral so only specular is under test.
fn material_with_specular(specular: [f32; 3]) -> NiMaterialProperty {
    NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        diffuse: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        specular: NiColor {
            r: specular[0],
            g: specular[1],
            b: specular[2],
        },
        emissive: NiColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        },
        shininess: 80.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    }
}

fn pp_lighting() -> BSShaderPPLightingProperty {
    BSShaderPPLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: 0,
            shader_flags_2: 0,
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

fn extract(scene: &NifScene, shape: &NiTriShape) -> MaterialInfo {
    let mut pool = StringPool::new();
    extract_material_info(scene, shape, &[], &mut pool)
}

/// The core #2553 case: vanilla FNV shape — black `NiMaterialProperty.
/// specular` co-bound with `BSShaderPPLightingProperty`. The zero is
/// vestigial, so it must be restored to the unauthored neutral rather
/// than forwarded as an authored black.
#[test]
fn black_specular_with_pp_lighting_is_treated_as_unauthored() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(material_with_specular([0.0, 0.0, 0.0])),
        Box::new(pp_lighting()),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let info = extract(&scene, &shape);

    assert_eq!(
        info.specular_color,
        [1.0, 1.0, 1.0],
        "a vestigial black specular on the FO3/FNV lit pipeline must fall \
         back to the unauthored neutral, not zero the GGX lobe"
    );
    assert!(
        !info.specular_authored,
        "specular_authored must be false so classify_pbr_keyword does not \
         read the neutral as a real Gamebryo specular tint (#1873)"
    );
    // The strength is untouched — this path is not the authored disable.
    assert!(info.specular_strength > 0.0);
}

/// Same, via `BSShaderNoLightingProperty` — the sibling legacy arm.
#[test]
fn black_specular_with_no_lighting_shader_is_treated_as_unauthored() {
    let shader = BSShaderNoLightingProperty {
        net: empty_net(),
        shader: BSShaderPropertyData {
            shade_flags: 0,
            shader_type: 0,
            shader_flags_1: 0,
            shader_flags_2: 0,
            env_map_scale: 0.0,
        },
        texture_clamp_mode: 0,
        file_name: "textures\\foo.dds".to_string(),
        falloff_start_angle: 0.0,
        falloff_stop_angle: 0.0,
        falloff_start_opacity: 0.0,
        falloff_stop_opacity: 0.0,
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(material_with_specular([0.0, 0.0, 0.0])),
        Box::new(shader),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let info = extract(&scene, &shape);

    assert_eq!(info.specular_color, [1.0, 1.0, 1.0]);
    assert!(!info.specular_authored);
}

/// The authored-disable path must survive untouched: an explicit
/// `NiSpecularProperty { flags: 0 }` still zeroes BOTH fields even
/// though the unauthored reset runs first. This is the ordering pin —
/// swap the two post-passes and this test fails.
#[test]
fn explicit_nispecular_disable_still_zeroes_under_pp_lighting() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(material_with_specular([0.0, 0.0, 0.0])),
        Box::new(pp_lighting()),
        Box::new(NiFlagProperty::for_test(0, "NiSpecularProperty")),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1), BlockRef(2)]);
    let info = extract(&scene, &shape);

    assert!(!info.specular_enabled);
    assert_eq!(
        info.specular_color,
        [0.0, 0.0, 0.0],
        "an explicit NiSpecularProperty{{flags:0}} is a genuine authored \
         disable and must still win over the #2553 unauthored reset"
    );
    assert_eq!(info.specular_strength, 0.0);
}

/// A genuinely authored non-black specular is forwarded verbatim — the
/// reset keys on the all-zero value, not on the pipeline alone.
#[test]
fn authored_specular_with_pp_lighting_is_preserved() {
    let blocks: Vec<Box<dyn NiObject>> = vec![
        Box::new(material_with_specular([0.8, 0.7, 0.6])),
        Box::new(pp_lighting()),
    ];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);
    let info = extract(&scene, &shape);

    assert_eq!(info.specular_color, [0.8, 0.7, 0.6]);
    assert!(info.specular_authored);
}

/// Blast-radius guard: with NO legacy BS shader bound (the Oblivion
/// fixed-function case, where `NiMaterialProperty.specular` really did
/// drive the lit pipeline) an authored black stays black.
#[test]
fn black_specular_without_legacy_shader_stays_authored() {
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(material_with_specular([0.0, 0.0, 0.0]))];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0)]);
    let info = extract(&scene, &shape);

    assert_eq!(
        info.specular_color,
        [0.0, 0.0, 0.0],
        "Oblivion-era fixed-function content DID source specular from \
         NiMaterialProperty — the reset must not reach it"
    );
    assert!(info.specular_authored);
}
