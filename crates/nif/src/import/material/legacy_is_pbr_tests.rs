//! Regression tests for #2570 (OBL-D4-04) — the NIF import path must
//! never mark a material PBR.
//!
//! `MAT_FLAG_PBR_BSDF` selects the Disney lobe in `lighting.glsl` and
//! `triangle.frag`; everything else takes the legacy Lambert arm. Every
//! writer that sets `is_pbr = true` sits behind an **external material
//! file** merge (`.mat` / BGSM / BGEM), and Oblivion authors none of
//! those — so today the flag is provably 0 for 100% of Oblivion content
//! and the Disney lobe is unreachable there.
//!
//! That is a property of the code, not of the format, and nothing pinned
//! it. A future "promote legacy specular / glossiness to PBR" heuristic
//! added to the classifier would silently flip every Oblivion surface
//! onto a lobe it was never authored for — a whole-game shading change
//! with no test-level tripwire. These tests are that tripwire: the
//! translation boundary is where the decision must be made (see
//! `docs/engine/nifal.md`), so this is where it is pinned.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::properties::{NiMaterialProperty, NiTexturingProperty, TexDesc};
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

/// An Oblivion-shaped property pair: `NiMaterialProperty` first (the
/// order vanilla ships), then `NiTexturingProperty` with a base slot.
/// Deliberately *glossy* — high shininess and a bright specular are the
/// signals a "promote legacy specular to PBR" heuristic would key on.
fn oblivion_shape_properties() -> (NiMaterialProperty, NiTexturingProperty) {
    let mat = NiMaterialProperty {
        net: empty_net(),
        ambient: NiColor::default(),
        diffuse: NiColor {
            r: 0.8,
            g: 0.8,
            b: 0.8,
        },
        specular: NiColor {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        },
        emissive: NiColor::default(),
        shininess: 120.0,
        alpha: 1.0,
        emissive_mult: 1.0,
    };
    let tex = NiTexturingProperty {
        net: empty_net(),
        flags: 0,
        texture_count: 1,
        base_texture: Some(TexDesc {
            source_ref: BlockRef::NULL,
            flags: 0,
            transform: None,
        }),
        dark_texture: None,
        detail_texture: None,
        gloss_texture: None,
        glow_texture: None,
        bump_texture: None,
        normal_texture: None,
        parallax_texture: None,
        parallax_offset: 0.0,
        decal_textures: Vec::new(),
    };
    (mat, tex)
}

#[test]
fn legacy_material_and_texturing_properties_never_yield_a_pbr_material() {
    let (mat, tex) = oblivion_shape_properties();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat), Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);

    let mut pool = StringPool::new();
    let info = extract_material_info(&scene, &shape, &[], &mut pool);
    let imported = info.into_imported_material(&mut pool, Some("TestShape"));

    assert!(
        !imported.is_pbr,
        "a NiMaterialProperty + NiTexturingProperty shape authors no external \
         material file, so it must not reach the Disney lobe — this is the \
         invariant that makes MAT_FLAG_PBR_BSDF provably 0 for Oblivion (#2570)"
    );
    // The three sibling flags that also gate on an external material file.
    // A heuristic that flipped `is_pbr` would most likely flip these too, and
    // `bgsm_pbr_scalars_authored` in particular is what tells consumers the
    // PBR scalars are *authored* rather than the keyword classifier's guess
    // (#2609).
    assert!(!imported.from_bgsm);
    assert!(!imported.bgsm_pbr_scalars_authored);
    assert!(!imported.bgem_glass);
}

/// The classifier still runs on legacy input — it populates
/// `metalness_override` / `roughness_override` for the legacy Lambert
/// path. This pins that producing those overrides is *not* the same
/// thing as being PBR: the overrides are the keyword classifier's guess,
/// and promoting them to `is_pbr` is exactly the regression #2570 guards.
#[test]
fn legacy_pbr_scalar_overrides_do_not_promote_the_material_to_pbr() {
    let (mat, tex) = oblivion_shape_properties();
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(mat), Box::new(tex)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = make_tri_shape_with_props(vec![BlockRef(0), BlockRef(1)]);

    let mut pool = StringPool::new();
    let mut info = extract_material_info(&scene, &shape, &[], &mut pool);
    // A metal-keyword path: the strongest "this looks like a conductor"
    // signal the legacy classifier has.
    info.texture_path = Some(pool.intern(r"Textures\Weapons\Iron\IronCuirass.dds"));
    let imported = info.into_imported_material(&mut pool, Some("TestShape"));

    assert!(
        imported.metalness_override.is_some(),
        "test premise: the legacy classifier does derive PBR scalars here"
    );
    assert!(
        !imported.is_pbr,
        "derived scalars are a guess for the LEGACY lobe; they must not \
         select the Disney BSDF (#2570)"
    );
    assert!(
        !imported.bgsm_pbr_scalars_authored,
        "and they must not claim to be authored (#2609)"
    );
}
