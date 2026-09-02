//! Regression: #3640 (FO4-2026-08-30-D4-02). `APP_CULLED` (`flags &
//! 0x01`) geometry with a live `NiVisController` targeting it must still
//! reach the importer — dropping it unconditionally means the controller
//! never has anything to toggle at runtime. A culled shape with NO
//! visibility controller must still be dropped (unchanged behavior,
//! SIBLING check against a regression that imports everything).
//!
//! Both `import_nif` (flat walker) and `import_nif_scene` (hierarchical
//! walker) are covered — the fix touches the shape-culling sites in both
//! (`crates/nif/src/import/walk/mod.rs`).

use super::super::*;
use crate::blocks::controller::{
    NiPreSplitDataController, NiSingleInterpController, NiTimeControllerBase,
};
use crate::blocks::interpolator::{BoolInterpolatorKind, NiBoolInterpolator};
use crate::types::BlockRef;

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
};

/// `NiPreSplitDataController` typed as `NiVisController` (RTTI-preserved
/// since #2562/#2563) — the same block shape a real embedded visibility
/// controller parses into.
fn make_ni_vis_controller(interpolator_ref: BlockRef) -> NiPreSplitDataController {
    NiPreSplitDataController {
        type_name: "NiVisController",
        base: NiSingleInterpController {
            base: NiTimeControllerBase {
                next_controller_ref: BlockRef::NULL,
                flags: 0,
                frequency: 1.0,
                phase: 0.0,
                start_time: 0.0,
                stop_time: 1.0,
                target_ref: BlockRef::NULL,
            },
            interpolator_ref,
        },
        data_ref: BlockRef::NULL,
    }
}

fn make_ni_bool_interpolator(value: bool) -> NiBoolInterpolator {
    NiBoolInterpolator {
        value,
        data_ref: BlockRef::NULL,
        kind: BoolInterpolatorKind::Plain,
    }
}

#[test]
fn flat_walker_imports_culled_shape_with_live_visibility_controller() {
    let mut shape = make_ni_tri_shape("Prop", identity_transform(), 2, Vec::new());
    shape.av.flags = 1; // APP_CULLED
    shape.av.net.controller_ref = BlockRef(3);

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
        Box::new(make_tri_shape_data()),
        Box::new(make_ni_vis_controller(BlockRef(4))),
        Box::new(make_ni_bool_interpolator(false)),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert_eq!(
        meshes.len(),
        1,
        "a live NiVisController must save the culled shape from the drop"
    );
    assert_eq!(meshes[0].name, Some(Arc::from("Prop")));
    assert_eq!(
        meshes[0].flags & 0x01,
        1,
        "APP_CULLED must still ride through on ImportedMesh::flags — the \
         shape isn't un-culled, just no longer dropped outright"
    );
}

#[test]
fn flat_walker_still_drops_culled_shape_with_no_visibility_controller() {
    let mut shape = make_ni_tri_shape("Prop", identity_transform(), 2, Vec::new());
    shape.av.flags = 1; // APP_CULLED, no controller_ref at all.

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    assert!(
        meshes.is_empty(),
        "unconditional drop must be unchanged for culled geometry with \
         nothing to ever un-hide it"
    );
}

#[test]
fn hierarchical_walker_imports_culled_shape_with_live_visibility_controller() {
    let mut shape = make_ni_tri_shape("Prop", identity_transform(), 2, Vec::new());
    shape.av.flags = 1; // APP_CULLED
    shape.av.net.controller_ref = BlockRef(3);

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
        Box::new(make_tri_shape_data()),
        Box::new(make_ni_vis_controller(BlockRef(4))),
        Box::new(make_ni_bool_interpolator(false)),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert_eq!(
        imported.meshes.len(),
        1,
        "a live NiVisController must save the culled shape from the drop \
         (hierarchical walker)"
    );
}

#[test]
fn hierarchical_walker_still_drops_culled_shape_with_no_visibility_controller() {
    let mut shape = make_ni_tri_shape("Prop", identity_transform(), 2, Vec::new());
    shape.av.flags = 1; // APP_CULLED, no controller_ref.

    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(make_ni_node(identity_transform(), vec![BlockRef(1)])),
        Box::new(shape),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert!(
        imported.meshes.is_empty(),
        "unconditional drop must be unchanged (hierarchical walker)"
    );
}
