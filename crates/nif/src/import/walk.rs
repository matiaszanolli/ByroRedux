//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
use crate::blocks::node::{
    BsMultiBoundNode, BsOrderedNode, BsRangeNode, BsTreeNode, BsValueNode, NiBillboardNode,
    NiLODNode, NiNode, NiSortAdjustNode, NiSwitchNode,
};
use crate::blocks::bs_geometry::BSGeometry;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::blocks::NiObject;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiTransform};

use super::collision::extract_collision;
use super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::mesh::{
    extract_bs_geometry, extract_bs_geometry_local, extract_bs_tri_shape,
    extract_bs_tri_shape_local, extract_mesh, extract_mesh_local,
};
use super::transform::compose_transforms;
use super::{
    ImportedCollision, ImportedLight, ImportedMesh, ImportedNode, ImportedScene, LightKind,
    MeshResolver, TreeBones,
};
use crate::blocks::extra_data::BsPackedCombinedGeomDataExtra;
use crate::blocks::node::BsRangeKind;
use byroredux_core::string::StringPool;

/// SK-D4-04 / #564 — return `true` when any of `node`'s extra_data refs
/// resolves to a `BSPackedCombinedGeomDataExtra` (or its Shared
/// variant). Used by the walkers to skip the host BSMultiBoundNode
/// subtree until the M35 terrain-streaming milestone consumes the
/// packed-extra payload.
fn has_packed_combined_geom_extra(scene: &NifScene, node: &NiNode) -> bool {
    for &ref_idx in &node.av.net.extra_data_refs {
        let Some(idx) = ref_idx.index() else { continue };
        let Some(block) = scene.blocks.get(idx) else {
            continue;
        };
        if block
            .as_any()
            .downcast_ref::<BsPackedCombinedGeomDataExtra>()
            .is_some()
        {
            return true;
        }
    }
    false
}

/// Downcast a `NiObject` to its underlying `NiNode` representation,
/// unwrapping any known subclass that wraps a `base: NiNode` (directly
/// or transitively). Returns `None` for non-node blocks.
///
/// This exists because every NiNode subclass gets its own concrete
/// Rust type, not a runtime alias — a plain `downcast_ref::<NiNode>()`
/// check would miss every subclass the parser grew dedicated structs
/// for (#142, #148, and the BSOrderedNode / BSValueNode cases from
/// issue #150 that this helper unblocks). Walkers should call this
/// instead of hand-rolling the downcast chain so that future NiNode
/// subtypes get picked up in one place.
pub(super) fn as_ni_node(block: &dyn NiObject) -> Option<&NiNode> {
    let any = block.as_any();
    if let Some(n) = any.downcast_ref::<NiNode>() {
        return Some(n);
    }
    // Direct NiNode wrappers (single `base: NiNode` field).
    if let Some(n) = any.downcast_ref::<BsOrderedNode>() {
        return Some(&n.base);
    }
    if let Some(n) = any.downcast_ref::<BsValueNode>() {
        return Some(&n.base);
    }
    if let Some(n) = any.downcast_ref::<BsMultiBoundNode>() {
        return Some(&n.base);
    }
    if let Some(n) = any.downcast_ref::<BsTreeNode>() {
        return Some(&n.base);
    }
    if let Some(n) = any.downcast_ref::<NiBillboardNode>() {
        return Some(&n.base);
    }
    // NiSwitchNode and NiLODNode are NOT unwrapped here — they need
    // child-filtering logic (active_index / LOD 0 only) which the generic
    // NiNode path doesn't provide. Handled explicitly in the walk
    // functions. See #212.
    if let Some(n) = any.downcast_ref::<NiSortAdjustNode>() {
        return Some(&n.base);
    }
    if let Some(n) = any.downcast_ref::<BsRangeNode>() {
        return Some(&n.base);
    }
    None
}

/// Extract the active child indices for NiSwitchNode (and NiLODNode).
///
/// NiSwitchNode: walk only child at `active_index` (furniture states,
/// weapon sheaths, destruction stages). If index is 0xFFFFFFFF (-1 as
/// u32) or out of range, walk all children (fallback).
///
/// NiLODNode: walk child 0 only (highest LOD). Proper distance-based
/// selection requires camera distance, which isn't available at import
/// time. LOD 0 is always the most detailed mesh. See #212.
fn switch_active_children(block: &dyn NiObject) -> Option<(&NiNode, Vec<usize>)> {
    let any = block.as_any();
    // NiLODNode check first (it wraps NiSwitchNode).
    if let Some(lod) = any.downcast_ref::<NiLODNode>() {
        let node = &lod.base.base;
        let active = if node.children.is_empty() {
            vec![]
        } else {
            // LOD 0 = highest detail.
            node.children[0].index().into_iter().collect()
        };
        return Some((node, active));
    }
    if let Some(sw) = any.downcast_ref::<NiSwitchNode>() {
        let node = &sw.base;
        let idx = sw.index as usize;
        let active = if idx < node.children.len() {
            node.children[idx].index().into_iter().collect()
        } else {
            // Fallback: walk all children (index out of range or 0xFFFFFFFF).
            node.children.iter().filter_map(|r| r.index()).collect()
        };
        return Some((node, active));
    }
    None
}

/// Recursively walk the scene graph, preserving hierarchy.
/// NiNodes become ImportedNode entries; geometry becomes ImportedMesh with parent_node set.
///
/// `inherited_props` accumulates property BlockRefs from ancestor NiNodes via
/// push/truncate stack discipline — no per-node Vec clone. Gamebryo propagates
/// properties down the scene graph; children inherit parent properties unless
/// they override with their own. See #208, #276.
pub(super) fn walk_node_hierarchical(
    scene: &NifScene,
    block_idx: usize,
    parent_node_idx: Option<usize>,
    inherited_props: &mut Vec<BlockRef>,
    out: &mut ImportedScene,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active child, not all
    // children. Must be checked BEFORE as_ni_node() since these types
    // are no longer unwrapped there. See #212.
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let t = &node.av.transform.translation;
        let quat = zup_matrix_to_yup_quat(&node.av.transform.rotation);
        let collision = extract_collision(scene, node.av.collision_ref);
        let billboard_mode = extract_billboard_mode(block, node.av.flags);
        // BSRangeKind / BSTreeNode metadata — populated only when the
        // source block was the matching subclass. NiSwitchNode /
        // NiLODNode (the only types reaching this branch) are never
        // BSRangeNode or BSTreeNode in shipped content, so both stay
        // None here. See #363 / #364.
        let range_kind = extract_range_kind(block);
        let tree_bones = extract_tree_bones(scene, block);

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: zup_point_to_yup(t),
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
            collision,
            billboard_mode,
            tree_bones,
            range_kind,
            flags: node.av.flags,
        });

        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_hierarchical(scene, idx, Some(this_node_idx), inherited_props, out, pool, resolver);
        }
        inherited_props.truncate(prev_len);
        return;
    }

    // `BsMultiBoundNode` culling-mode guard (#355, partial): Skyrim+
    // large-interior cells (Dragonsreach, College of Winterhold) use
    //   0 = normal (default)
    //   1 = all children visible regardless of bounds
    //   2 = always-hidden
    //   3 = force-culled
    // on BsMultiBoundNode to flag unreachable / invisible subtrees.
    // Honor 2 and 3 by dropping the subtree at import time — the
    // subtree wouldn't render anyway but skipping it avoids uploading
    // its meshes and building their BLAS entries. Full AABB
    // consumption for culling_mode == 1 (and feeding the renderer's
    // culling structure) is the remaining half of the issue and is
    // mid-scope plumbing.
    if let Some(mbn) = block.as_any().downcast_ref::<BsMultiBoundNode>() {
        if mbn.culling_mode == 2 || mbn.culling_mode == 3 {
            return;
        }
        // SK-D4-04 / #564 — FO4+ distant-LOD merged-geometry hosts.
        // A BSMultiBoundNode whose extra_data carries a
        // BSPackedCombinedGeomDataExtra (or its Shared variant) is a
        // dedicated LOD-batch root: the geometry lives entirely in
        // the packed-extra block and the M35 terrain-streaming
        // milestone owns its consumption. Walking the subtree
        // produces empty `ImportedNode` entries that contribute no
        // meshes today — skip the host so the cell's ECS doesn't
        // pick up dead nodes. The packed-extra block stays available
        // on the scene's block table for the future LOD importer.
        if has_packed_combined_geom_extra(scene, &mbn.base) {
            log::debug!(
                "Skipping BSMultiBoundNode LOD-batch subtree (SK-D4-04 / #564) — \
                 packed-combined-geom consumer is M35 terrain-streaming work"
            );
            return;
        }
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }

        // Convert this node's LOCAL transform to Y-up.
        let t = &node.av.transform.translation;
        let quat = zup_matrix_to_yup_quat(&node.av.transform.rotation);

        // Extract collision data if this node has a collision_ref.
        let collision = extract_collision(scene, node.av.collision_ref);

        // Detect NiBillboardNode (or the pre-10.1.0.0 form where the mode
        // is packed into NiAVObject flags bits 5-6). See #225 / nif.xml
        // `BillboardMode`. The importer hands the raw u16 to the consumer
        // which maps it to the `Billboard` ECS component.
        let billboard_mode = extract_billboard_mode(block, node.av.flags);
        // BSRangeKind discriminator (#364) and BSTreeNode bone lists
        // (#363). Both default to None for plain NiNode.
        let range_kind = extract_range_kind(block);
        let tree_bones = extract_tree_bones(scene, block);

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: zup_point_to_yup(t),
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
            collision,
            billboard_mode,
            tree_bones,
            range_kind,
            flags: node.av.flags,
        });

        // Merge this node's properties with the inherited set via stack
        // discipline. Child shapes see the union; their own properties
        // take priority inside extract_material_info because shape props
        // are iterated before inherited props.
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(scene, idx, Some(this_node_idx), inherited_props, out, pool, resolver);
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        // bit 0 = APP_CULLED (hidden). Editor-marker filtering runs
        // as a sibling check below so shape-level editor markers
        // (common on Skyrim+ MapMarker geometry where the flag rides
        // on the shape, not the containing node) don't render as
        // untextured debug pyramids. See #165 / audit N26-4-06.
        //
        // Pre-#332 the mask was `0x21` (APP_CULLED + bit 5). Bit 5 is
        // DISPLAY_OBJECT_MASK per Gamebryo `NiAVObject.h` — the
        // occlusion-display helper that SHOULD still render. The
        // conflation was harmless on vanilla Bethesda content (which
        // doesn't set that bit) but dropped modded geometry and
        // anything authored with a Gamebryo-native tool.
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_mesh_local(scene, shape, inherited_props, pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        // bit 0 = APP_CULLED (hidden). Editor-marker filtering runs
        // as a sibling check below so shape-level editor markers
        // (common on Skyrim+ MapMarker geometry where the flag rides
        // on the shape, not the containing node) don't render as
        // untextured debug pyramids. See #165 / audit N26-4-06.
        //
        // Pre-#332 the mask was `0x21` (APP_CULLED + bit 5). Bit 5 is
        // DISPLAY_OBJECT_MASK per Gamebryo `NiAVObject.h` — the
        // occlusion-display helper that SHOULD still render. The
        // conflation was harmless on vanilla Bethesda content (which
        // doesn't set that bit) but dropped modded geometry and
        // anything authored with a Gamebryo-native tool.
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_bs_tri_shape_local(scene, shape, pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BSGeometry>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        if let Some(mesh) = extract_bs_geometry_local(scene, shape, pool, resolver) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    // Particle systems — see #401 / `ImportedParticleEmitter`.
    // The parser keeps every NiPSys* variant opaque (`NiPSysBlock`), so
    // we identify the renderable subset by the original_type string.
    // Only the top-level emitter blocks produce an ImportedParticleEmitter;
    // modifier blocks ride along inside the same scene and are walked but
    // not surfaced (the heuristic preset on the host node carries the
    // visual config until the parsers retain real emitter fields).
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
    {
        match ps.original_type.as_str() {
            "NiParticleSystem"
            | "NiMeshParticleSystem"
            | "NiParticles"
            | "NiParticleSystemController"
            | "NiBSPArrayController"
            | "NiAutoNormalParticles"
            | "NiRotatingParticles" => {
                out.particle_emitters
                    .push(crate::import::ImportedParticleEmitter {
                        parent_node: parent_node_idx,
                        original_type: ps.original_type.clone(),
                    });
            }
            _ => {}
        }
    }
}

/// Recursively walk the scene graph, accumulating world-space transforms (flat, no hierarchy).
///
/// When `collisions` is `Some`, also extracts collision data from NiNodes
/// and stores them in world space.
pub(super) fn walk_node_flat(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    inherited_props: &mut Vec<BlockRef>,
    out: &mut Vec<ImportedMesh>,
    mut collisions: Option<&mut Vec<ImportedCollision>>,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active child (#212).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        if let Some(ref mut coll_out) = collisions {
            if let Some((shape, body)) = extract_collision(scene, node.av.collision_ref) {
                let t = &world_transform.translation;
                let quat = zup_matrix_to_yup_quat(&world_transform.rotation);
                coll_out.push(ImportedCollision {
                    translation: zup_point_to_yup(t),
                    rotation: quat,
                    scale: world_transform.scale,
                    shape,
                    body,
                });
            }
        }
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_flat(
                scene,
                idx,
                &world_transform,
                inherited_props,
                out,
                collisions.as_deref_mut(),
                pool,
                resolver,
            );
        }
        inherited_props.truncate(prev_len);
        return;
    }

    // BsMultiBoundNode culling-mode guard (#355, partial) — sibling of
    // the hierarchical walker above. Same SK-D4-04 / #564 LOD-batch
    // skip applies on the flat path so loose-NIF imports (`scene.rs`)
    // honor the M35 deferral the same way cell-loader imports do.
    if let Some(mbn) = block.as_any().downcast_ref::<BsMultiBoundNode>() {
        if mbn.culling_mode == 2 || mbn.culling_mode == 3 {
            return;
        }
        if has_packed_combined_geom_extra(scene, &mbn.base) {
            log::debug!(
                "Skipping BSMultiBoundNode LOD-batch subtree on flat walk \
                 (SK-D4-04 / #564) — packed-combined-geom consumer is M35"
            );
            return;
        }
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);

        // Extract collision data if requested and this node has a collision_ref.
        if let Some(ref mut coll_out) = collisions {
            if let Some((shape, body)) = extract_collision(scene, node.av.collision_ref) {
                let t = &world_transform.translation;
                let quat = zup_matrix_to_yup_quat(&world_transform.rotation);
                coll_out.push(ImportedCollision {
                    translation: zup_point_to_yup(t),
                    rotation: quat,
                    scale: world_transform.scale,
                    shape,
                    body,
                });
            }
        }

        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_flat(
                    scene,
                    idx,
                    &world_transform,
                    inherited_props,
                    out,
                    collisions.as_deref_mut(),
                    pool,
                    resolver,
                );
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        // bit 0 = APP_CULLED (hidden). Editor-marker filtering runs
        // as a sibling check below so shape-level editor markers
        // (common on Skyrim+ MapMarker geometry where the flag rides
        // on the shape, not the containing node) don't render as
        // untextured debug pyramids. See #165 / audit N26-4-06.
        //
        // Pre-#332 the mask was `0x21` (APP_CULLED + bit 5). Bit 5 is
        // DISPLAY_OBJECT_MASK per Gamebryo `NiAVObject.h` — the
        // occlusion-display helper that SHOULD still render. The
        // conflation was harmless on vanilla Bethesda content (which
        // doesn't set that bit) but dropped modded geometry and
        // anything authored with a Gamebryo-native tool.
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_mesh(scene, shape, &world_transform, inherited_props, pool) {
            out.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        // bit 0 = APP_CULLED (hidden). Editor-marker filtering runs
        // as a sibling check below so shape-level editor markers
        // (common on Skyrim+ MapMarker geometry where the flag rides
        // on the shape, not the containing node) don't render as
        // untextured debug pyramids. See #165 / audit N26-4-06.
        //
        // Pre-#332 the mask was `0x21` (APP_CULLED + bit 5). Bit 5 is
        // DISPLAY_OBJECT_MASK per Gamebryo `NiAVObject.h` — the
        // occlusion-display helper that SHOULD still render. The
        // conflation was harmless on vanilla Bethesda content (which
        // doesn't set that bit) but dropped modded geometry and
        // anything authored with a Gamebryo-native tool.
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_bs_tri_shape(scene, shape, &world_transform, pool) {
            out.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BSGeometry>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);
        if let Some(mesh) = extract_bs_geometry(scene, shape, &world_transform, pool, resolver) {
            out.push(mesh);
        }
    }
}

/// Recursively walk the scene graph accumulating world-space transforms
/// and collecting any NiLight subclass encountered.
pub(super) fn walk_node_lights(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    out: &mut Vec<ImportedLight>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_lights(scene, idx, &world_transform, out);
            }
        }
        return;
    }

    // NiLight subclasses — extract using the world transform composed from
    // the parent chain plus the light's own local transform.
    if let Some(l) = block.as_any().downcast_ref::<NiPointLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        let radius = attenuation_radius(
            l.constant_attenuation,
            l.linear_attenuation,
            l.quadratic_attenuation,
        );
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Point,
            radius,
            0.0,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiSpotLight>() {
        let world = compose_transforms(parent_transform, &l.point.base.av.transform);
        let radius = attenuation_radius(
            l.point.constant_attenuation,
            l.point.linear_attenuation,
            l.point.quadratic_attenuation,
        );
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.point.base,
            LightKind::Spot,
            radius,
            l.outer_spot_angle,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiAmbientLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Ambient,
            0.0,
            0.0,
        ));
        return;
    }
    if let Some(l) = block.as_any().downcast_ref::<NiDirectionalLight>() {
        let world = compose_transforms(parent_transform, &l.base.av.transform);
        out.push(imported_light_from_base(
            scene,
            &world,
            &l.base,
            LightKind::Directional,
            0.0,
            0.0,
        ));
        // no return — directional lights are leaves
    }
}

/// Flat counterpart to the particle-emitter detection in
/// `walk_node_hierarchical`: walks the scene graph accumulating world-
/// space transforms and emits one [`crate::import::ImportedParticleEmitterFlat`]
/// per renderable particle block (`NiParticleSystem` and friends). Used
/// by the cell loader, which spawns one entity per emitter at the
/// composed REFR-times-host-NIF-local world position. See #401.
pub(super) fn walk_node_particle_emitters_flat(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    parent_node_name: Option<std::sync::Arc<str>>,
    out: &mut Vec<crate::import::ImportedParticleEmitterFlat>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        // Pass this node's name down so descendant emitters inherit a
        // sensible host name even when the emitter block itself is
        // unnamed (the common case in vanilla content).
        let new_parent_name = node.av.net.name.clone().or(parent_node_name);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_particle_emitters_flat(
                    scene,
                    idx,
                    &world_transform,
                    new_parent_name.clone(),
                    out,
                );
            }
        }
        return;
    }

    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
    {
        match ps.original_type.as_str() {
            "NiParticleSystem"
            | "NiMeshParticleSystem"
            | "NiParticles"
            | "NiParticleSystemController"
            | "NiBSPArrayController"
            | "NiAutoNormalParticles"
            | "NiRotatingParticles" => {
                let t = &parent_transform.translation;
                out.push(crate::import::ImportedParticleEmitterFlat {
                    local_position: zup_point_to_yup(t),
                    host_name: parent_node_name,
                    original_type: ps.original_type.clone(),
                });
            }
            _ => {}
        }
    }
}

fn imported_light_from_base(
    scene: &NifScene,
    world: &NiTransform,
    base: &crate::blocks::light::NiLightBase,
    kind: LightKind,
    radius: f32,
    outer_angle: f32,
) -> ImportedLight {
    let translation = zup_point_to_yup(&world.translation);

    // Gamebryo lights point down the local -Z axis in their own space.
    // Transform that via the world rotation, then convert to Y-up.
    let rot = &world.rotation;
    let dir_zup = [-rot.rows[0][2], -rot.rows[1][2], -rot.rows[2][2]];
    let direction = [dir_zup[0], dir_zup[2], -dir_zup[1]];

    // Dimmer scales the diffuse contribution — the only channel the
    // engine currently consumes. Ambient/specular are stored for later.
    // Gamebryo stores light colors as raw floats in "monitor space" —
    // effectively sRGB values used as-is with no gamma conversion.  We
    // pass them through unchanged because the legacy content was
    // authored for this non-linear-aware pipeline.
    let d = base.dimmer;
    let diffuse = base.diffuse_color;
    let color = [diffuse.r * d, diffuse.g * d, diffuse.b * d];

    let affected_node_names = resolve_affected_node_names(scene, &base.affected_nodes);

    ImportedLight {
        translation,
        direction,
        color,
        radius,
        kind,
        outer_angle,
        affected_node_names,
    }
}

/// Resolve the `NiDynamicEffect.Affected Nodes` Ptr list to a list of
/// node names. The on-disk values are 4-byte `Ptr<NiAVObject>` entries:
/// `u32::MAX` = null pointer, otherwise a block index. Names are
/// pulled from each target's `NiObjectNET.name`. Null entries and
/// targets that fail to resolve to a named scene-graph block are
/// dropped silently — empty list = "no restriction" by convention,
/// so partial restrictions stay meaningful even with unresolvable
/// pointers (corrupt content). See #335.
fn resolve_affected_node_names(scene: &NifScene, ptrs: &[u32]) -> Vec<std::sync::Arc<str>> {
    let mut out: Vec<std::sync::Arc<str>> = Vec::with_capacity(ptrs.len());
    for &p in ptrs {
        if p == u32::MAX {
            continue;
        }
        let Some(block) = scene.get(p as usize) else {
            continue;
        };
        let Some(net) = block.as_object_net() else {
            continue;
        };
        let Some(name) = net.name() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        out.push(std::sync::Arc::from(name));
    }
    out
}

/// Resolve a list of `BlockRef`s to scene-graph node names, dropping
/// null refs and refs that don't resolve to a named NiObjectNET-bearing
/// block. Mirrors [`resolve_affected_node_names`] but operates on
/// `BlockRef` (the type [`BSTreeNode`] uses for its bone lists).
fn resolve_block_ref_names(scene: &NifScene, refs: &[BlockRef]) -> Vec<std::sync::Arc<str>> {
    let mut out: Vec<std::sync::Arc<str>> = Vec::with_capacity(refs.len());
    for r in refs {
        let Some(idx) = r.index() else { continue };
        let Some(block) = scene.get(idx) else {
            continue;
        };
        let Some(net) = block.as_object_net() else {
            continue;
        };
        let Some(name) = net.name() else { continue };
        if name.is_empty() {
            continue;
        }
        out.push(std::sync::Arc::from(name));
    }
    out
}

/// Extract the [`crate::import::TreeBones`] payload when `block` is a
/// [`BSTreeNode`]. Returns `None` for any other block type (including
/// the regular `NiNode` and its non-tree subclasses). See #363.
pub(super) fn extract_tree_bones(scene: &NifScene, block: &dyn NiObject) -> Option<TreeBones> {
    let tree = block.as_any().downcast_ref::<BsTreeNode>()?;
    let branch_roots = resolve_block_ref_names(scene, &tree.bones_1);
    let trunk = resolve_block_ref_names(scene, &tree.bones_2);
    if branch_roots.is_empty() && trunk.is_empty() {
        // No surviving bones — treat as if the wire data was absent so
        // the consumer doesn't have to filter out empty-payload tree
        // nodes downstream.
        None
    } else {
        Some(TreeBones {
            branch_roots,
            trunk,
        })
    }
}

/// Extract the [`BsRangeKind`] discriminator when `block` is a
/// [`BsRangeNode`] (or one of its dispatcher-aliased subclasses
/// `BSDamageStage` / `BSBlastNode` / `BSDebrisNode`). Returns `None`
/// for any other block type. See #364.
pub(super) fn extract_range_kind(block: &dyn NiObject) -> Option<BsRangeKind> {
    block.as_any().downcast_ref::<BsRangeNode>().map(|n| n.kind)
}

/// Solve `1 / (const + lin·d + quad·d²) = THRESHOLD` for distance.
/// A light's "effective radius" is the distance at which its contribution
/// drops below a small fraction of its peak. We use 1/256 (~0.4%) which
/// matches what Bethesda shaders use as a cull threshold.
fn attenuation_radius(k_const: f32, k_lin: f32, k_quad: f32) -> f32 {
    const THRESHOLD: f32 = 1.0 / 256.0;
    // Find distance d where k_quad·d² + k_lin·d + k_const = 1/THRESHOLD
    let target = 1.0 / THRESHOLD;
    if k_quad > 1e-6 {
        // Quadratic: d = (-b + sqrt(b² - 4a(c - target))) / 2a
        let a = k_quad;
        let b = k_lin;
        let c = k_const - target;
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            return ((-b + disc.sqrt()) / (2.0 * a)).max(0.0);
        }
    }
    if k_lin > 1e-6 {
        return ((target - k_const) / k_lin).max(0.0);
    }
    // No attenuation → effectively infinite. Clamp to a sane default so
    // the renderer doesn't get a garbage value.
    2048.0
}

/// Extract a NiBillboardNode mode from a block, if any.
///
/// From 10.1.0.0 onward (all Bethesda games) the mode is a trailing u16
/// field on the block. Pre-10.1.0.0 the mode is packed into NiAVObject
/// flags bits 5-6 — we translate that back out so the consumer always
/// sees the modern `BillboardMode` value regardless of source version.
///
/// Returns `None` for non-billboard nodes.
fn extract_billboard_mode(block: &dyn NiObject, av_flags: u32) -> Option<u16> {
    if let Some(bb) = block.as_any().downcast_ref::<NiBillboardNode>() {
        if bb.billboard_mode != 0 {
            return Some(bb.billboard_mode);
        }
        // 10.1.0.0+ NIF with mode 0 is still a valid "always face camera"
        // billboard — preserve the fact that this is a billboard.
        // Fall through to the legacy flags check in case the parser
        // defaulted to 0 for a pre-10.1.0.0 NIF.
        let legacy = (av_flags >> 5) & 0x3;
        return Some(legacy as u16);
    }
    None
}

/// Check if a node name is an editor marker that should be skipped.
///
/// Matches the NiNode name prefixes Bethesda uses for editor-only
/// geometry across Oblivion / FO3 / FNV / Skyrim / FO4 / FO76 /
/// Starfield:
///
/// - `EditorMarker*` — catch-all Bethesda placeholder (every game).
/// - `marker_*` / `marker:*` / `MarkerX` — Gamebryo editor pins
///   (quest / patrol / navmesh markers).
/// - `MapMarker` — exterior-cell world map pin. Skyrim+ ships one
///   of these per settlement / POI; without the match they render
///   as untextured pyramids scattered across the overworld
///   (audit N26-4-06 / #165).
fn is_editor_marker(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    fn starts_with_ci(s: &str, prefix: &str) -> bool {
        s.len() >= prefix.len()
            && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    }
    starts_with_ci(name, "editormarker")
        || starts_with_ci(name, "marker_")
        || name.eq_ignore_ascii_case("markerx")
        || starts_with_ci(name, "marker:")
        || starts_with_ci(name, "mapmarker")
}

#[cfg(test)]
mod affected_nodes_tests {
    //! Regression tests for issue #335 — `NiDynamicEffect.Affected
    //! Nodes` Ptr list must surface on `ImportedLight` so the
    //! renderer's per-light filter can later restrict the light's
    //! effect to the named subtrees.
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::types::BlockRef;
    use std::sync::Arc;

    fn node_with_name(name: &str) -> NiNode {
        NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: crate::types::NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: Vec::new(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn resolve_skips_null_pointer() {
        let scene = NifScene::default();
        let names = resolve_affected_node_names(&scene, &[u32::MAX]);
        assert!(names.is_empty());
    }

    #[test]
    fn resolve_skips_out_of_range_pointer() {
        // Empty scene — index 0 is out of range. Must be silently
        // dropped rather than panic.
        let scene = NifScene::default();
        let names = resolve_affected_node_names(&scene, &[0u32]);
        assert!(names.is_empty());
    }

    #[test]
    fn resolve_extracts_node_name() {
        // Regression: pre-#335 the `affected_nodes` Vec was parsed
        // (light.rs:48) but never read. Now the importer surfaces the
        // names on `ImportedLight` for the renderer's per-light filter.
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_with_name("HandLanternBone")));
        scene.blocks.push(Box::new(node_with_name("BipedHead")));
        let names = resolve_affected_node_names(&scene, &[0u32, 1u32]);
        assert_eq!(names.len(), 2);
        assert_eq!(&*names[0], "HandLanternBone");
        assert_eq!(&*names[1], "BipedHead");
    }

    #[test]
    fn resolve_drops_unnamed_target() {
        // Sibling check — a target block that exists but has no name
        // (`net.name == None`) must drop out of the result rather
        // than emitting an empty string. Empty names break consumer
        // hash-set lookups silently.
        let mut scene = NifScene::default();
        let mut anon = node_with_name("");
        anon.av.net.name = None;
        scene.blocks.push(Box::new(anon));
        scene.blocks.push(Box::new(node_with_name("Named")));
        let names = resolve_affected_node_names(&scene, &[0u32, 1u32]);
        assert_eq!(names.len(), 1);
        assert_eq!(&*names[0], "Named");
    }

    #[test]
    fn resolve_partial_failure_keeps_recoverable_entries() {
        // A mix of [valid, null, out-of-range] must yield exactly the
        // one valid entry — the null-as-no-restriction convention
        // means we'd lose meaning if a single bad pointer collapsed
        // the whole list to empty.
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(node_with_name("OnlyValid")));
        let names = resolve_affected_node_names(&scene, &[0u32, u32::MAX, 99u32]);
        assert_eq!(names.len(), 1);
        assert_eq!(&*names[0], "OnlyValid");
    }
}

#[cfg(test)]
mod editor_marker_tests {
    //! Regression tests for `is_editor_marker` (#165 / audit N26-4-06).
    //! Exhaustive list of the prefixes the walker must filter across
    //! Gamebryo-lineage games. Missed patterns render as untextured
    //! debug geometry in the live scene (map pins, quest targets,
    //! patrol route markers, editor bounding pyramids).
    use super::is_editor_marker;

    #[test]
    fn matches_known_editor_marker_prefixes() {
        // Gamebryo editor / quest / patrol markers — every game.
        assert!(is_editor_marker(Some("EditorMarker")));
        assert!(is_editor_marker(Some("EDITORMARKER")));
        assert!(is_editor_marker(Some("EditorMarker_QuestNode")));
        assert!(is_editor_marker(Some("Marker_01")));
        assert!(is_editor_marker(Some("marker:patrol")));
        assert!(is_editor_marker(Some("MarkerX")));
        assert!(is_editor_marker(Some("markerx")));
    }

    /// Regression: #165 — Skyrim+ exterior-cell world map pins
    /// ("MapMarker") were rendering as untextured pyramids in the
    /// overworld. The match now catches the prefix (case-insensitive).
    #[test]
    fn matches_skyrim_map_marker() {
        assert!(is_editor_marker(Some("MapMarker")));
        assert!(is_editor_marker(Some("mapmarker")));
        assert!(is_editor_marker(Some("MapMarker_Whiterun")));
        assert!(is_editor_marker(Some("MAPMARKER")));
    }

    #[test]
    fn does_not_match_legitimate_names() {
        // False-positive regression guards — these are real NIF node
        // names that must NOT be filtered.
        assert!(!is_editor_marker(None));
        assert!(!is_editor_marker(Some("")));
        assert!(!is_editor_marker(Some("Bip01 Head")));
        assert!(!is_editor_marker(Some("NPC Torso [Tors]")));
        // "MapMarkerMesh" does get filtered — that's correct, any
        // prefix match is intentional (vanilla doesn't author non-
        // marker nodes starting with these prefixes).
        assert!(is_editor_marker(Some("MapMarkerMesh")));
    }
}
