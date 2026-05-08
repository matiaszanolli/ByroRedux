//! Regression tests for #337 (D4-NEW-01) — `NiStencilProperty` test/write
//! state must round-trip into `MaterialInfo.stencil_state`. Pre-fix the
//! walker only consumed `is_two_sided()` and silently dropped
//! `stencil_enabled`, `stencil_function`, `stencil_ref`, `stencil_mask`,
//! `fail_action`, `z_fail_action`, `pass_action` — closing the silent-drop
//! is the parser-side half of the fix; renderer pipeline variants are
//! deferred. See [`StencilState`] docs.
//!
//! Same fixture pattern as `double_sided_tests` — synthetic
//! `NiStencilProperty` block, attach to a `NiTriShape` via the inherited
//! `properties` list, run `extract_material_info` against a fresh
//! `StringPool`, assert on `MaterialInfo.stencil_state`.

use super::*;
use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
use crate::blocks::properties::NiStencilProperty;
use crate::blocks::tri_shape::NiTriShape;
use crate::blocks::NiObject;
use crate::types::{BlockRef, NiTransform};
use byroredux_core::string::StringPool;

fn empty_net() -> NiObjectNETData {
    NiObjectNETData {
        name: None,
        extra_data_refs: Vec::new(),
        controller_ref: BlockRef::NULL,
    }
}

fn shape_with_property(ref_idx: u32) -> NiTriShape {
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

fn extract_with_pool(scene: &NifScene, shape: &NiTriShape) -> MaterialInfo {
    let mut pool = StringPool::new();
    extract_material_info(scene, shape, &[], &mut pool)
}

/// Stencil-active property (FO3 `dust_off_pew_pew_decals` shape — a
/// stencil-masked decal): all seven test/write fields populated, plus
/// `draw_mode = 1` (CCW, single-sided). The walker must capture all
/// seven values verbatim; `two_sided` stays unset (not the 95% case).
#[test]
fn stencil_active_property_round_trips_all_fields() {
    let stencil = NiStencilProperty {
        net: empty_net(),
        flags: 0,
        stencil_enabled: true,
        stencil_function: 2,  // EQUAL
        stencil_ref: 0x42,
        stencil_mask: 0xFF,
        fail_action: 0,    // KEEP
        z_fail_action: 0,  // KEEP
        pass_action: 2,    // REPLACE
        draw_mode: 1,      // CCW (not two-sided)
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(stencil)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_property(0);
    let info = extract_with_pool(&scene, &shape);

    let state = info
        .stencil_state
        .expect("NiStencilProperty present must populate stencil_state (#337)");
    assert!(state.enabled, "stencil_enabled must round-trip");
    assert_eq!(state.function, 2, "EQUAL function must round-trip");
    assert_eq!(state.reference, 0x42);
    assert_eq!(state.mask, 0xFF);
    assert_eq!(state.fail_action, 0, "KEEP on stencil fail must round-trip");
    assert_eq!(state.z_fail_action, 0);
    assert_eq!(state.pass_action, 2, "REPLACE on pass must round-trip");
    // CCW (1) is single-sided — `is_two_sided()` returns false.
    assert!(
        !info.two_sided,
        "draw_mode=1 (CCW) must not promote two_sided"
    );
}

/// 95% case: a stencil-disabled property used purely for two-sided
/// rendering (`draw_mode = 3 = BOTH`). The walker must still capture
/// the (empty) state on `stencil_state` — the renderer-side gate is
/// `state.enabled`, but capturing the `Some(...)` reflects what the
/// NIF authored. `two_sided` is set via the existing `is_two_sided()`
/// path that already worked before this fix.
#[test]
fn two_sided_only_property_captures_state_with_enabled_false() {
    let stencil = NiStencilProperty {
        net: empty_net(),
        flags: 0,
        stencil_enabled: false,
        stencil_function: 7, // ALWAYS — parser default
        stencil_ref: 0,
        stencil_mask: 0xFFFF_FFFF,
        fail_action: 0,
        z_fail_action: 0,
        pass_action: 0,
        draw_mode: 3, // BOTH (two-sided)
    };
    let blocks: Vec<Box<dyn NiObject>> = vec![Box::new(stencil)];
    let scene = NifScene {
        blocks,
        ..NifScene::default()
    };
    let shape = shape_with_property(0);
    let info = extract_with_pool(&scene, &shape);

    let state = info
        .stencil_state
        .expect("two-sided NiStencilProperty must still populate stencil_state");
    assert!(
        !state.enabled,
        "stencil_enabled=false must round-trip (no spurious enable)"
    );
    assert!(
        info.two_sided,
        "draw_mode=3 (BOTH) must promote two_sided via is_two_sided()"
    );
}

/// Materials without a `NiStencilProperty` keep `stencil_state = None`
/// — the importer must not synthesise a default state when the NIF
/// has no authoring source.
#[test]
fn material_without_stencil_property_leaves_stencil_state_none() {
    let scene = NifScene::default();
    let shape = NiTriShape {
        av: NiAVObjectData {
            net: empty_net(),
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        data_ref: BlockRef::NULL,
        skin_instance_ref: BlockRef::NULL,
        shader_property_ref: BlockRef::NULL,
        alpha_property_ref: BlockRef::NULL,
        num_materials: 0,
        active_material_index: 0,
    };
    let info = extract_with_pool(&scene, &shape);
    assert!(
        info.stencil_state.is_none(),
        "no NiStencilProperty → stencil_state must stay None"
    );
}
