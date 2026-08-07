//! `BS*` NiNode-subclass payload tests: `BsOrderedNode`, `BsValueNode`,
//! `BsRangeNode` variants, `BsTreeNode` bone lists, `BsMultiBoundNode`
//! packed-geometry skip, `BSConnectPoint::{Parents,Children}`,
//! `BsBound`, and `NiLodTriShape` (BSLODTriShape).

use super::super::*;
use crate::types::BlockRef;

use super::{
    identity_transform, make_ni_node, make_ni_tri_shape, make_tri_shape_data, scene_from_blocks,
};

/// Regression test for issue #150 — `BsOrderedNode` (and every other
/// NiNode subclass with a `base: NiNode` field) must unwrap cleanly
/// during scene-graph walks. Previously the walker only downcast to
/// plain `NiNode`, so children of BSOrderedNode (FO3/FNV weapons,
/// effects, architecture) were silently dropped.
#[test]
fn bs_ordered_node_children_are_walked() {
    use crate::blocks::node::BsOrderedNode;

    // Root BsOrderedNode with a single NiTriShape child.
    let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let ordered = BsOrderedNode {
        base: inner_node,
        alpha_sort_bound: [0.0, 0.0, 0.0, 10.0],
        is_static_bound: false,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(ordered),
        Box::new(make_ni_tri_shape(
            "OrderedChild",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);

    // Flat path — would return zero meshes before the fix.
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert_eq!(
        meshes.len(),
        1,
        "BsOrderedNode subtree must yield 1 mesh in flat import"
    );
    assert_eq!(meshes[0].name, Some(Arc::from("OrderedChild")));

    // Hierarchical path — must register the parent node AND the mesh.
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert_eq!(imported.meshes.len(), 1);
    assert_eq!(imported.meshes[0].parent_node, Some(0));
}

/// Regression test for issue #150 — `BsValueNode` is a NiNode
/// subclass carrying numeric metadata; its children must also be
/// walked.
#[test]
fn bs_value_node_children_are_walked() {
    use crate::blocks::node::BsValueNode;

    let inner_node = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let value_node = BsValueNode {
        base: inner_node,
        value: 42,
        value_flags: 0,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(value_node),
        Box::new(make_ni_tri_shape(
            "ValueChild",
            identity_transform(),
            2,
            Vec::new(),
        )),
        Box::new(make_tri_shape_data()),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].name, Some(Arc::from("ValueChild")));
}

/// Regression for #625 / SK-D4-02: the BSValueNode `(value,
/// value_flags)` pair survives the `as_ni_node` unwrap and lands
/// on the matching `ImportedNode.bs_value_node`. Pre-fix the
/// walker dropped these alongside the type identity, hiding LOD-
/// distance overrides + billboard hints from the scene builder.
#[test]
fn bs_value_node_value_and_flags_are_surfaced_on_imported_node() {
    use crate::blocks::node::BsValueNode;

    let inner_node = make_ni_node(identity_transform(), vec![]);
    let value_node = BsValueNode {
        base: inner_node,
        value: 0xCAFEBABE,
        value_flags: 0x07,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![Box::new(value_node)];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    let payload = imported.nodes[0]
        .bs_value_node
        .expect("BsValueNode must surface bs_value_node payload (#625 / SK-D4-02)");
    assert_eq!(payload.value, 0xCAFEBABE);
    assert_eq!(payload.flags, 0x07);
    // Plain NiNode siblings stay None — the field is only populated
    // for the matching subclass.
    assert!(imported.nodes[0].bs_ordered_node.is_none());
}

/// Regression for #625 / SK-D4-03: BSOrderedNode `alpha_sort_bound`
/// + `is_static_bound` survive the walker unwrap. Renderer-side
///
/// consumption (a `RenderOrderHint` component on each child + a
/// sort-key tweak in `build_render_data`) is deferred per the
/// no-speculative-Vulkan-fixes policy — this test pins the data-
/// plumbing half so the eventual renderer fix has the source
/// material to read.
///
/// #2008 — the center is a point, so it must go through the same
/// Z-up → Y-up conversion (`(x, y, z) → (x, z, -y)`) as every other
/// node-local position on `ImportedNode` and its siblings; `radius`
/// is a magnitude and is unaffected. A non-trivial, asymmetric center
/// is used specifically so a "copied verbatim" regression would fail
/// this assertion (an all-zero or symmetric center couldn't).
#[test]
fn bs_ordered_node_alpha_sort_bound_is_surfaced_on_imported_node() {
    use crate::blocks::node::BsOrderedNode;

    let inner_node = make_ni_node(identity_transform(), vec![]);
    let ordered = BsOrderedNode {
        base: inner_node,
        alpha_sort_bound: [1.0, 2.0, 3.0, 7.5],
        is_static_bound: true,
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![Box::new(ordered)];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    let payload = imported.nodes[0]
        .bs_ordered_node
        .expect("BsOrderedNode must surface bs_ordered_node payload (#625 / SK-D4-03)");
    assert_eq!(
        payload.alpha_sort_bound,
        [1.0, 3.0, -2.0, 7.5],
        "center must convert Z-up (x,y,z) -> Y-up (x,z,-y); radius (last element) unchanged"
    );
    assert!(payload.is_static_bound);
    assert!(imported.nodes[0].bs_value_node.is_none());
}

/// Plain `NiNode` (no subclass payload) keeps both fields `None`.
/// Guards against a future regression where the walker
/// inadvertently fabricates default payloads on every node.
#[test]
fn plain_ni_node_has_no_bs_subclass_payloads() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(make_ni_node(identity_transform(), vec![]))];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert!(imported.nodes[0].bs_value_node.is_none());
    assert!(imported.nodes[0].bs_ordered_node.is_none());
}

/// Helper for the #364 test: build a `BsRangeNode` block with the
/// given discriminator + the canonical (min, max, current) triple.
fn ni_range_node(
    kind: crate::blocks::node::BsRangeKind,
    min: u8,
    max: u8,
    current: u8,
) -> crate::blocks::node::BsRangeNode {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    let inner_node = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("RangeHost")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    crate::blocks::node::BsRangeNode {
        base: inner_node,
        min,
        max,
        current,
        kind,
    }
}

/// Regression: #364 — BSRangeNode subclasses (BSBlastNode /
/// BSDamageStage / BSDebrisNode) must surface their wire-type
/// discriminator on the resulting `ImportedNode.range_kind`.
/// Pre-fix all four collapsed into a `BsRangeNode` with no
/// surviving discriminator and the walker stripped them down to
/// plain NiNode — gameplay-side systems couldn't tell apart
/// "switch the visible damage stage" from "spawn debris on
/// detach" from "fire the blast effect".
#[test]
fn import_surfaces_bs_range_kind_for_each_subclass() {
    for kind in [
        crate::blocks::node::BsRangeKind::Range,
        crate::blocks::node::BsRangeKind::DamageStage,
        crate::blocks::node::BsRangeKind::Blast,
        crate::blocks::node::BsRangeKind::Debris,
    ] {
        let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
            vec![Box::new(ni_range_node(kind, 0, 5, 2))];
        let scene = scene_from_blocks(blocks);
        let mut pool = StringPool::new();
        let imported = import_nif_scene(&scene, &mut pool);
        assert_eq!(imported.nodes.len(), 1, "{:?}", kind);
        assert_eq!(
            imported.nodes[0].range_kind,
            Some(kind),
            "range_kind should round-trip the dispatcher discriminator for {:?}",
            kind,
        );
    }
}

/// Regression: #364 — plain NiNode produces `range_kind: None`.
/// Catches a regression that defaults the discriminator to
/// `Some(BsRangeKind::Range)` for every node.
#[test]
fn import_plain_ninode_has_no_range_kind() {
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> =
        vec![Box::new(make_ni_node(identity_transform(), Vec::new()))];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert_eq!(imported.nodes.len(), 1);
    assert!(imported.nodes[0].range_kind.is_none());
}

/// Regression: #363 — `BSTreeNode` bone-list metadata must surface
/// on `ImportedNode.tree_bones` resolved to the targets'
/// `NiObjectNET.name` (mirrors the `#335` affected-node-names
/// pattern). Pre-fix the walker stripped the BSTreeNode down to
/// plain NiNode and dropped both bone lists, blocking any future
/// SpeedTree wind / bend simulation from finding what to animate.
#[test]
fn import_surfaces_bs_tree_node_bones_by_name() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    // Build three bone targets (NiNodes with names) at indices 1, 2, 3.
    // Then a BSTreeNode at index 0 whose:
    //   bones_1 = [1, 3]  (branch roots)
    //   bones_2 = [2]     (trunk)
    let bone = |name: &str| -> Box<dyn crate::blocks::NiObject> {
        Box::new(crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: identity_transform(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        })
    };
    let host = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("TreeRoot")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let tree = crate::blocks::node::BsTreeNode {
        base: host,
        bones_1: vec![BlockRef(1), BlockRef(3)],
        bones_2: vec![BlockRef(2)],
    };
    let blocks: Vec<Box<dyn crate::blocks::NiObject>> = vec![
        Box::new(tree),
        bone("Branch_A"),
        bone("Trunk_0"),
        bone("Branch_B"),
    ];
    let scene = scene_from_blocks(blocks);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    let host_node = &imported.nodes[0];
    let bones = host_node
        .tree_bones
        .as_ref()
        .expect("BSTreeNode should surface tree_bones");
    let branch: Vec<&str> = bones.branch_roots.iter().map(|s| s.as_ref()).collect();
    let trunk: Vec<&str> = bones.trunk.iter().map(|s| s.as_ref()).collect();
    assert_eq!(branch, vec!["Branch_A", "Branch_B"]);
    assert_eq!(trunk, vec!["Trunk_0"]);
}

/// Regression: #363 — when every bone ref in a BSTreeNode is null
/// or unresolvable, surface `tree_bones: None` rather than a
/// `Some(TreeBones { empty, empty })` so the consumer doesn't have
/// to filter empty payloads downstream.
#[test]
fn import_drops_bs_tree_node_with_only_unresolvable_bones() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    let host = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(Arc::from("EmptyTree")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: identity_transform(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let tree = crate::blocks::node::BsTreeNode {
        base: host,
        bones_1: vec![BlockRef::NULL, BlockRef(99)], // null + out-of-range
        bones_2: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(tree)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);
    assert!(imported.nodes[0].tree_bones.is_none());
}

/// SK-D4-04 / #564 — distant-LOD `BSMultiBoundNode` hosts whose
/// extra_data carries a `BSPackedCombinedGeomDataExtra` are
/// skipped wholesale. The packed-extra block is renderer-side
/// deferred (M35 terrain-streaming) and the host subtree carries
/// no other geometry, so walking it would only produce empty
/// `ImportedNode` entries.
#[test]
fn bs_multi_bound_node_with_packed_geom_extra_subtree_is_skipped() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::{BsPackedCombinedGeomDataExtra, BsPackedCombinedPayload};
    use crate::blocks::node::BsMultiBoundNode;

    // [0] BSMultiBoundNode root with extra_data → block 1.
    // [1] BSPackedCombinedGeomDataExtra (the LOD batch).
    let packed = BsPackedCombinedGeomDataExtra {
        type_name: "BSPackedCombinedGeomDataExtra",
        name: None,
        vertex_desc: 0,
        num_vertices: 0,
        num_triangles: 0,
        unknown_flags_1: 0,
        unknown_flags_2: 0,
        num_data: 0,
        payload: BsPackedCombinedPayload::Baked(Vec::new()),
    };
    let host = BsMultiBoundNode {
        base: crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from("LODHost")),
                    extra_data_refs: vec![BlockRef(1)],
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        },
        multi_bound_ref: BlockRef::NULL,
        culling_mode: 0,
    };
    let scene = scene_from_blocks(vec![Box::new(host), Box::new(packed)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert!(
        imported.nodes.is_empty(),
        "LOD-batch host must be skipped — no ImportedNode entries should leak"
    );
    assert!(imported.meshes.is_empty());
}

/// Sanity: a plain `BSMultiBoundNode` with no packed-extra
/// extra_data still produces an `ImportedNode` so non-LOD scenes
/// (Dragonsreach interior, College of Winterhold) keep working.
/// Pre-#564 the skip applied unconditionally, which would have
/// broken these.
#[test]
fn plain_bs_multi_bound_node_without_packed_geom_extra_still_imports() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::BsMultiBoundNode;

    let host = BsMultiBoundNode {
        base: crate::blocks::node::NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(std::sync::Arc::from("DragonsreachInterior")),
                    // No extra_data_refs — the packed-extra detector
                    // returns false and the walker falls through to
                    // the normal NiNode path.
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        },
        multi_bound_ref: BlockRef::NULL,
        culling_mode: 0,
    };
    let scene = scene_from_blocks(vec![Box::new(host)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert_eq!(
        imported.nodes.len(),
        1,
        "Plain BSMultiBoundNode (no packed-extra) must still produce a node"
    );
}

// ── #985 / NIF-D5-ORPHAN-A3 — FO4 weapon-mod attach graph consumer ──

/// `BSConnectPoint::Parents` extra-data on the root node lifts every
/// authored attach point into `ImportedScene::attach_points`. Without
/// this routing, every FO4 modular weapon imports with no discoverable
/// attach surface — the OMOD / weapon-mod system can't function.
#[test]
fn bs_connect_point_parents_lifts_to_imported_scene() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::{BsConnectPointParents, ConnectPointData};

    // FO4 10mm-pistol-style attach graph: receiver bone exposes
    // a magazine slot and a scope rail.
    let parents = BsConnectPointParents {
        name: None,
        connect_points: vec![
            ConnectPointData {
                parent: "GunBoneReceiver".to_string(),
                name: "CON_Magazine".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, -1.5, 0.0],
                scale: 1.0,
            },
            ConnectPointData {
                parent: "GunBoneReceiver".to_string(),
                name: "CON_Scope".to_string(),
                rotation: [1.0, 0.0, 0.0, 0.0],
                translation: [0.0, 0.0, 2.0],
                scale: 1.0,
            },
        ],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("10mmPistolRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(parents)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let points = imported
        .attach_points
        .as_ref()
        .expect("BSConnectPoint::Parents must reach ImportedScene.attach_points");
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].name, "CON_Magazine");
    assert_eq!(points[0].parent, "GunBoneReceiver");
    // #1594 — the lift now converts the attach transform Z-up → Y-up
    // (`(x,y,z) → (x,z,-y)`), matching BsBound and the `AttachPoint`
    // component's documented Y-up frame. Authored Z-up `[0,-1.5,0]` → `[0,0,1.5]`.
    assert_eq!(points[0].translation, [0.0, 0.0, 1.5]);
    // Identity rotation stays identity through the conversion (WXYZ).
    assert_eq!(points[0].rotation, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(points[0].scale, 1.0);
    assert_eq!(points[1].name, "CON_Scope");
    // Authored Z-up `[0,0,2]` → Y-up `[0,2,0]`.
    assert_eq!(points[1].translation, [0.0, 2.0, 0.0]);
    // Child connections were not authored on this NIF; field stays None.
    assert!(imported.child_attach_connections.is_none());
}

/// `BSConnectPoint::Children` extra-data on the root node lifts the
/// child-side of the attach graph (the names this accessory connects
/// back to on its parent host) into
/// `ImportedScene::child_attach_connections`.
#[test]
fn bs_connect_point_children_lifts_to_imported_scene() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsConnectPointChildren;

    // A reflex-sight accessory mesh mounting to a parent's CON_Scope.
    let children = BsConnectPointChildren {
        name: None,
        skinned: false,
        point_names: vec!["CON_Scope".to_string()],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("ReflexSightRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(children)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let conn = imported
        .child_attach_connections
        .as_ref()
        .expect("BSConnectPoint::Children must reach ImportedScene.child_attach_connections");
    assert_eq!(conn.point_names, vec!["CON_Scope".to_string()]);
    assert!(!conn.skinned);
    // Parents not authored on this accessory; field stays None.
    assert!(imported.attach_points.is_none());
}

/// `skinned: true` from `BSConnectPoint::Children` round-trips into
/// `ImportedChildAttachConnections.skinned` — drives the equip-side
/// "rigid attach vs bone-weighted attach" decision.
#[test]
fn bs_connect_point_children_skinned_flag_round_trips() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsConnectPointChildren;

    let children = BsConnectPointChildren {
        name: None,
        skinned: true,
        point_names: vec!["CON_Cape".to_string()],
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("CapeAccessoryRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(children)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let conn = imported.child_attach_connections.as_ref().unwrap();
    assert!(conn.skinned);
}

/// Sibling check: a NIF with neither `BSConnectPoint::Parents` nor
/// `BSConnectPoint::Children` in its root extra-data leaves both
/// fields at `None`. Defends against an unconditional default
/// `Some(empty)` initialization that would mislead consumers into
/// "this entity has an explicitly-empty attach graph" (vs the truth:
/// "no graph authored").
#[test]
fn scene_without_connect_point_extras_leaves_fields_none() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};

    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("PlainStatic")),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    assert!(imported.attach_points.is_none());
    assert!(imported.child_attach_connections.is_none());
}

/// #986 / NIF-D5-ORPHAN-B2 — `BSBound` extra-data on the root node lifts
/// onto `ImportedScene::bs_bound` with the center/dimensions rotated
/// from NIF Z-up to renderer Y-up so the downstream
/// `BSBound` ECS component agrees with `Transform` / `GlobalTransform`.
/// Pre-fix the captured value was raw Z-up, leaving any future culling
/// or spatial-query consumer 90° out of plane with the scene graph.
#[test]
fn bs_bound_lifts_to_imported_scene_in_y_up() {
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::extra_data::BsBound;

    // Asymmetric center + half-extents so the y/z permutation is
    // observable separately from the y-sign flip.
    let bound = BsBound {
        name: None,
        center: [1.0, 2.0, 3.0],     // Z-up
        dimensions: [4.0, 5.0, 6.0], // half-extents (Z-up labels)
    };
    let root = crate::blocks::node::NiNode {
        av: NiAVObjectData {
            net: NiObjectNETData {
                name: Some(std::sync::Arc::from("BoundedRoot")),
                extra_data_refs: vec![BlockRef(1)],
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        },
        children: Vec::new(),
        effects: Vec::new(),
    };
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(bound)]);
    let mut pool = StringPool::new();
    let imported = import_nif_scene(&scene, &mut pool);

    let (center, half_extents) = imported
        .bs_bound
        .expect("BsBound on the root node must reach ImportedScene.bs_bound");
    // Z-up [1, 2, 3] → Y-up [x, z, -y] = [1, 3, -2]. Same rule as
    // every other point in the importer (zup_point_to_yup).
    assert_eq!(center, [1.0, 3.0, -2.0]);
    // Half-extents are unsigned magnitudes — the Z-up→Y-up rotation
    // around X is a 90° relabel, so the new-Y half-extent equals the
    // old Z half-extent and vice versa. No sign flip.
    assert_eq!(half_extents, [4.0, 6.0, 5.0]);
}

/// #988 / SK-D5-NEW-09 — BSLODTriShape geometry was silently dropped by both
/// import walkers because no NiLodTriShape downcast arm existed. The #838 parser
/// fix added the type but the import path was never wired up.
///
/// Regression: a scene containing a BSLODTriShape (NiLodTriShape) under a root
/// NiNode must import exactly one mesh (from lod.base, the inner NiTriShape),
/// not zero.
#[test]
fn bs_lod_tri_shape_imports_geometry_not_dropped() {
    use crate::blocks::tri_shape::NiLodTriShape;

    // Root NiNode → BSLODTriShape (NiLodTriShape) → NiTriShapeData
    let root = make_ni_node(identity_transform(), vec![BlockRef(1)]);
    let lod = NiLodTriShape {
        base: make_ni_tri_shape("LODTree", identity_transform(), 2, Vec::new()),
        lod0_size: 100,
        lod1_size: 50,
        lod2_size: 25,
    };
    let data = make_tri_shape_data();
    let scene = scene_from_blocks(vec![Box::new(root), Box::new(lod), Box::new(data)]);
    let mut pool = StringPool::new();
    let meshes = import_nif(&scene, &mut pool);

    // Pre-#988: meshes.len() == 0 (silently dropped).
    assert_eq!(
        meshes.len(),
        1,
        "BSLODTriShape must produce 1 ImportedMesh, not be silently dropped"
    );
    let m = &meshes[0];
    assert_eq!(m.name, Some(std::sync::Arc::from("LODTree")));
    assert_eq!(m.positions.len(), 3, "triangle mesh has 3 positions");
    // #2283 — NiLodTriShape's own lod{0,1,2}_size fields must reach
    // ImportedMesh.bs_lod_cutoffs; pre-fix the classic-NiTriShape
    // extractor this walks through hardcoded `None`, so the #988 test
    // above never actually pinned this despite constructing a fixture
    // with non-zero LOD sizes.
    assert_eq!(
        m.bs_lod_cutoffs,
        Some([100, 50, 25]),
        "NiLodTriShape's lod0/1/2_size must thread through to bs_lod_cutoffs"
    );
}
