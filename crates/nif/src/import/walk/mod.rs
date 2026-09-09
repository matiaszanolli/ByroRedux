//! Scene graph walking — hierarchical and flat traversal.
//!
//! #3856 — the three independent satellite walkers (`walk_node_lights`,
//! `walk_node_texture_effects`, `walk_node_particle_emitters_flat`) are
//! entry points called only from `import/mod.rs`, never from
//! `walk_node_hierarchical` or `walk_node_flat`. That made them free to
//! extract with no shared-state threading, so this file now holds just the
//! two scene-graph walkers and their immediate helpers; the satellites and
//! the small per-node attribute readers live in the siblings below.

mod emitter;
mod lights;
mod node_attrs;
mod texture_effect;

pub(super) use emitter::{
    collect_force_fields, extract_emitter_max_particles, extract_emitter_params,
    extract_emitter_rate, extract_first_color_curve, walk_node_particle_emitters_flat,
};
#[cfg(test)]
pub(super) use lights::attenuation_radius;
pub(super) use lights::walk_node_lights;
pub(super) use node_attrs::{
    extract_bs_ordered_node, extract_bs_value_node, extract_lod_group, extract_range_kind,
    extract_tree_bones,
};
pub(super) use texture_effect::walk_node_texture_effects;

use emitter::extract_particle_material;
use node_attrs::{extract_billboard_mode, is_editor_marker};

use crate::blocks::bs_geometry::BSGeometry;
use crate::blocks::node::{
    BsDistantObjectInstancedNode, BsMultiBoundNode, BsOrderedNode, BsRangeNode, BsTreeNode,
    BsValueNode, BsWeakReferenceNode, NiBillboardNode, NiLODNode, NiNode, NiSortAdjustNode,
    NiSwitchNode,
};
use crate::blocks::tri_shape::{BsTriShape, NiLodTriShape, NiTriShape};
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
use super::{ImportedCollision, ImportedMesh, ImportedNode, ImportedScene, MeshResolver};
use crate::blocks::extra_data::BsPackedCombinedGeomDataExtra;
use byroredux_core::string::StringPool;

/// SK-D4-04 / #564 — return `true` when any of `node`'s extra_data refs
/// resolves to a `BSPackedCombinedGeomDataExtra` (or its Shared
/// variant). Used by the walkers to skip the host BSMultiBoundNode
/// subtree until the M35 terrain-streaming milestone consumes the
/// packed-extra payload.
///
/// Both variants currently warrant the skip:
/// - **Baked** (distant-LOD batches): geometry lives inline in the
///   packed-extra block; the subtree's NiNode children are empty
///   shells. Skipping avoids spawning empty `ImportedNode` entries.
/// - **Shared** (FO4+ interior precombines, `_oc.nif`): the BSTriShape
///   children carry `num_vertices = 0` / empty vertex+triangle arrays;
///   the real data lives in a companion `.csg`/`.psg` blob (Bethesda
///   ships these as `Fallout4 - Geometry.csg` next to the BA2s).
///   Walking the subtree still produces zero meshes because each
///   BSTriShape's inline buffers are empty (#1188, Diamond City Dugout
///   Inn 2026-05-19). Future CSG-loader work will populate these from
///   the companion file; until then the cell-loader falls back to
///   per-REFR rendering via the conditional absorption gate in
///   `load_cell_with_masters`.
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
    // #942 — FO76 BSDistantObjectInstancedNode wraps BsMultiBoundNode
    // which wraps NiNode. Without this arm the walker would never descend
    // through it and the host LOD subtree would import as an empty
    // ImportedNode (no children, no meshes).
    if let Some(n) = any.downcast_ref::<BsDistantObjectInstancedNode>() {
        return Some(&n.base.base);
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
    if let Some(n) = any.downcast_ref::<BsWeakReferenceNode>() {
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

/// Whether `controller_ref`'s chain contains a live `NiVisController`
/// (RTTI-preserved since #2562/#2563, so `block_type_name()` reports it
/// correctly whether the block parsed as a bare `NiSingleInterpController`
/// pre-fix era or its own type today). #3640 — a shape with `APP_CULLED`
/// (`flags & 0x01`) set is normally dropped at import outright; when
/// something is actually going to drive its visibility at runtime, the
/// shape needs to exist in the ECS for that controller to have anything
/// to toggle. Shape-scoped only (SHAPE sites: NiTriShape / BsTriShape /
/// NiLodTriShape / BSGeometry in both `walk_node_hierarchical` and
/// `walk_node_flat`) — the sibling NODE-level `flags & 0x01` checks
/// (`switch_active_children` / `as_ni_node` branches) are deliberately
/// untouched: dropping a culled node also drops its whole subtree, and
/// nothing in this ECS propagates a parent's `AnimatedVisibility` to its
/// children, so "stop dropping the node" would not by itself reproduce
/// "subtree starts hidden" the way it does for a single shape. The
/// audit's own measured evidence (581 APP_CULLED BSTriShapes, 13 files)
/// is shape-scoped too — no node-level count was measured. A node-level
/// fix needs visibility propagation this codebase doesn't have yet; left
/// for a follow-up rather than silently making the wrong subtree visible.
fn has_live_visibility_controller(scene: &NifScene, controller_ref: BlockRef) -> bool {
    if controller_ref.is_null() {
        return false;
    }
    let mut found = false;
    crate::anim::walk_controller_chain(scene, controller_ref, |_idx, block, _base| {
        if !found && block.block_type_name() == "NiVisController" {
            found = true;
        }
    });
    found
}

/// Maximum recursion depth for `walk_node_hierarchical` and
/// `walk_node_flat`. Bethesda-shipped NIFs nest at most a few dozen
/// nodes deep; the cap stops a malformed or adversarial file from
/// crashing the parser via stack overflow (#1269 / SAFE-DIM3-NEW-01).
pub(crate) const MAX_NIF_NODE_DEPTH: u32 = 128;

/// Long-lived context threaded through [`walk_node_hierarchical`]'s
/// recursion: the read-only scene + resolver and the mutable output /
/// string-pool / inherited-property accumulators. Bundling these keeps
/// the per-call recursion signature small (block index / parent / depth).
pub(super) struct HierWalkCtx<'a> {
    pub scene: &'a NifScene,
    /// Accumulates property BlockRefs from ancestor NiNodes via
    /// push/truncate stack discipline — no per-node Vec clone. Gamebryo
    /// propagates properties down the scene graph; children inherit
    /// parent properties unless they override with their own (#208/#276).
    pub inherited_props: &'a mut Vec<BlockRef>,
    pub out: &'a mut ImportedScene,
    pub pool: &'a mut StringPool,
    pub resolver: Option<&'a dyn MeshResolver>,
    /// #2527 / NIF-D4-2026-08-07-01 — mirror of `FlatWalkCtx`'s field of
    /// the same name. The flat walker (`walk_node_flat`) has threaded
    /// this since #2206; the hierarchical walker never did, so a
    /// `NiBillboardNode` wrapping child geometry imported through
    /// `import_nif_scene` (the loose-NIF viewer AND the real object/
    /// terrain LOD spawn paths) spawned but never rotated to face the
    /// camera. Save/restore around recursion in the `as_ni_node` branch,
    /// same stack discipline as `inherited_props`.
    pub inherited_billboard: Option<u16>,
}

/// Recursively walk the scene graph, preserving hierarchy.
/// NiNodes become ImportedNode entries; geometry becomes ImportedMesh with parent_node set.
pub(super) fn walk_node_hierarchical(
    ctx: &mut HierWalkCtx,
    block_idx: usize,
    parent_node_idx: Option<usize>,
    depth: u32,
) {
    let scene = ctx.scene;
    let resolver = ctx.resolver;
    if depth > MAX_NIF_NODE_DEPTH {
        log::warn!(
            "walk_node_hierarchical: depth cap {} hit at block {} — \
             aborting subtree (#1269)",
            MAX_NIF_NODE_DEPTH,
            block_idx,
        );
        return;
    }
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
        // BSValueNode + BSOrderedNode subclass-specific fields (#625).
        // NiSwitchNode / NiLODNode never overlap with these, so both
        // stay None here.
        let bs_value_node = extract_bs_value_node(block);
        let bs_ordered_node = extract_bs_ordered_node(block);

        let this_node_idx = ctx.out.nodes.len();
        let lod_group = extract_lod_group(scene, block);
        ctx.out.nodes.push(ImportedNode {
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
            bs_value_node,
            bs_ordered_node,
            // NiLODNode → its NiRangeLODData ranges (surfaced for a future
            // distance-switch; import still walks child 0 only). `None` for
            // NiSwitchNode. In-cell-LOD foundation.
            lod_group,
        });

        let prev_len = ctx.inherited_props.len();
        ctx.inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_hierarchical(ctx, idx, Some(this_node_idx), depth + 1);
        }
        ctx.inherited_props.truncate(prev_len);
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
        // BSValueNode value+flags (#625 / SK-D4-02) and BSOrderedNode
        // alpha_sort_bound (#625 / SK-D4-03). Both default to None for
        // plain NiNode and non-matching subclasses.
        let bs_value_node = extract_bs_value_node(block);
        let bs_ordered_node = extract_bs_ordered_node(block);

        let this_node_idx = ctx.out.nodes.len();
        ctx.out.nodes.push(ImportedNode {
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
            bs_value_node,
            bs_ordered_node,
            // Plain NiNode / non-LOD subclasses — NiLODNode is handled in the
            // switch_active_children branch above and never reaches here.
            lod_group: None,
        });

        // Merge this node's properties with the inherited set via stack
        // discipline. Child shapes see the union; their own properties
        // take priority inside extract_material_info because shape props
        // are iterated before inherited props.
        let prev_len = ctx.inherited_props.len();
        ctx.inherited_props.extend_from_slice(&node.av.properties);
        // #2527 / NIF-D4-2026-08-07-01 — same save/restore stack
        // discipline as `inherited_props`, mirroring `walk_node_flat`'s
        // pattern (#2206) so a billboard subtree's mode doesn't leak to
        // its siblings. `billboard_mode` (computed above) already ran
        // `extract_billboard_mode` for this node's own `ImportedNode`;
        // reuse it here instead of re-downcasting.
        let prev_billboard = ctx.inherited_billboard;
        if let Some(mode) = billboard_mode {
            ctx.inherited_billboard = Some(mode);
        }
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(ctx, idx, Some(this_node_idx), depth + 1);
            }
        }
        ctx.inherited_props.truncate(prev_len);
        ctx.inherited_billboard = prev_billboard;
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
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        // Surface shape-level collision onto the parent NiNode's
        // collision slot if the parent didn't already author one. The
        // hierarchical walker stores collisions on `ImportedNode`
        // (it has no separate `collisions` out-list like the flat
        // path), so a shape-bound `bhkCollisionObject` flows into
        // the same field as a node-bound one. See NIF-D4-NEW-04
        // (audit 2026-05-12). Oblivion + some FO3 modded content
        // attaches collision to the shape directly.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = ctx.out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        ctx.out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_mesh_local(scene, shape, ctx.inherited_props, ctx.pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            // #2527 / NIF-D4-2026-08-07-01 — mirror of `walk_node_flat`'s
            // per-mesh billboard-mode stamp (#2206).
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.meshes.push(mesh);
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
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        // Mirror of the NiTriShape branch above — see NIF-D4-NEW-04.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = ctx.out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        ctx.out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_bs_tri_shape_local(scene, shape, ctx.pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            // #2527 / NIF-D4-2026-08-07-01 — mirror of `walk_node_flat`'s
            // per-mesh billboard-mode stamp (#2206).
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.meshes.push(mesh);
        }
    }

    // BSLODTriShape (Skyrim/SSE distant-LOD geometry) — parsed as
    // NiLodTriShape since #838. Body is an NiTriShape with three
    // trailing LOD-size u32s. Delegate to the NiTriShape extraction
    // path via the `.base` field; the LOD sizes are rendered at
    // whatever detail the camera sees (future: expose lod*_size as a
    // draw-distance hint for an LOD selector). Pre-#988 these shapes
    // had no import arm and were silently dropped (#988 / SK-D5-NEW-09).
    if let Some(lod) = block.as_any().downcast_ref::<NiLodTriShape>() {
        let shape = &lod.base;
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        // Mirror of the NiTriShape branch above — see NIF-D4-NEW-04.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = ctx.out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        ctx.out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_mesh_local(scene, shape, ctx.inherited_props, ctx.pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            // #2283 — `NiLodTriShape` carries its own LOD triangle-count
            // cutoffs (a separate wire type from `BSMeshLODTriShape`'s
            // `BsTriShapeKind::MeshLOD`); `extract_mesh_local` runs the
            // classic-`NiTriShape` extractor and always hardcodes `None`,
            // so thread them through here instead.
            mesh.bs_lod_cutoffs = Some([lod.lod0_size, lod.lod1_size, lod.lod2_size]);
            // #2527 / NIF-D4-2026-08-07-01 — mirror of `walk_node_flat`'s
            // per-mesh billboard-mode stamp (#2206).
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.meshes.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BSGeometry>() {
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        // Mirror of the NiTriShape branch above — see NIF-D4-NEW-04.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = ctx.out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        ctx.out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_bs_geometry_local(scene, shape, ctx.pool, resolver) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            // #2527 / NIF-D4-2026-08-07-01 — mirror of `walk_node_flat`'s
            // per-mesh billboard-mode stamp (#2206).
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.meshes.push(mesh);
        }
    }

    // Particle systems — see #401 / `ImportedParticleEmitter` and
    // #984 / `ImportedParticleForceField`. Modern emitter blocks
    // (`NiParticleSystem` / `NiMeshParticleSystem` / `NiParticles` /
    // `BSStripParticleSystem`) deserialise to a typed
    // `NiParticleSystem` whose `modifier_refs` we walk into the field-
    // modifier blocks. (The legacy NiParticleSystemController /
    // NiAutoNormalParticles / NiRotatingParticles types dispatch to
    // `legacy_particle::*`, NOT `NiPSysBlock`, so a NiPSysBlock downcast
    // never matched them — that dead arm was removed in #1327. The target
    // games all author the modern NiParticleSystem stack.)
    //
    // #2568 — that last sentence was challenged as false for Oblivion and is
    // now **measured true for every target game**: zero legacy particle
    // blocks across 54 202 vanilla NIFs (Oblivion + Shivering Isles + FO3 +
    // FNV + Skyrim SE), against 3 635 `NiParticleSystem`. The per-archive
    // counts are in `blocks/mod.rs`'s dispatch arm for those types. An
    // emission arm for the legacy stack would be unreachable code.
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiParticleSystem>()
    {
        let pmat = extract_particle_material(scene, ps, ctx.inherited_props, ctx.pool);
        // Retain the block's own local TRS (#1333). The host node's
        // world transform reaches us as the host entity's GlobalTransform
        // in the scene builder; these fields carry the offset *within*
        // that host so the emitter anchors at host-world × block-local
        // instead of the host node origin.
        ctx.out
            .particle_emitters
            .push(crate::import::ImportedParticleEmitter {
                parent_node: parent_node_idx,
                original_type: ps.original_type.clone(),
                texture_path: pmat.texture_path,
                src_blend: pmat.src_blend,
                dst_blend: pmat.dst_blend,
                effect_shader: pmat.effect_shader,
                greyscale_lut_map: pmat.greyscale_lut_map,
                local_translation: zup_point_to_yup(&ps.transform.translation),
                local_rotation: zup_matrix_to_yup_quat(&ps.transform.rotation),
                local_scale: ps.transform.scale,
                color_curve: extract_first_color_curve(scene),
                force_fields: collect_force_fields(scene, &ps.modifier_refs),
                emitter_params: extract_emitter_params(scene),
                emitter_rate: extract_emitter_rate(scene),
                max_particles: extract_emitter_max_particles(scene),
            });
    }
}

/// Long-lived context threaded through [`walk_node_flat`]'s recursion:
/// the read-only scene + resolver, the mutable mesh/collision out-lists,
/// the inherited-property accumulator, and the string pool. Bundling
/// these keeps the per-call recursion signature focused on what varies
/// between nodes (block index / parent transform / depth).
///
/// When `collisions` is `Some`, the walker also extracts collision data
/// from NiNodes and stores it in world space.
pub(super) struct FlatWalkCtx<'a> {
    pub scene: &'a NifScene,
    pub inherited_props: &'a mut Vec<BlockRef>,
    pub out: &'a mut Vec<ImportedMesh>,
    pub collisions: Option<&'a mut Vec<ImportedCollision>>,
    pub pool: &'a mut StringPool,
    pub resolver: Option<&'a dyn MeshResolver>,
    /// #2206 / NIFAL-D4-02 — nearest-ancestor `NiBillboardNode` mode on
    /// the current path from the walk root, or `None` if no ancestor
    /// (including the current node) is a billboard node. Set on every
    /// leaf mesh pushed to `out` while this is `Some`. `Copy`, so callers
    /// save/restore it around a recursive descent exactly like
    /// `inherited_props`'s push/truncate, just without the `Vec`.
    pub inherited_billboard: Option<u16>,
}

/// Recursively walk the scene graph, accumulating world-space transforms (flat, no hierarchy).
pub(super) fn walk_node_flat(
    ctx: &mut FlatWalkCtx,
    block_idx: usize,
    parent_transform: &NiTransform,
    depth: u32,
) {
    let scene = ctx.scene;
    let resolver = ctx.resolver;
    if depth > MAX_NIF_NODE_DEPTH {
        log::warn!(
            "walk_node_flat: depth cap {} hit at block {} — \
             aborting subtree (#1269)",
            MAX_NIF_NODE_DEPTH,
            block_idx,
        );
        return;
    }
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
        if let Some(ref mut coll_out) = ctx.collisions {
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
        let prev_len = ctx.inherited_props.len();
        ctx.inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_flat(ctx, idx, &world_transform, depth + 1);
        }
        ctx.inherited_props.truncate(prev_len);
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
        if let Some(ref mut coll_out) = ctx.collisions {
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

        let prev_len = ctx.inherited_props.len();
        ctx.inherited_props.extend_from_slice(&node.av.properties);
        // #2206 / NIFAL-D4-02 — `as_ni_node` unwraps `NiBillboardNode` to
        // its plain `NiNode` base, so `block` (the untyped original) is
        // the only way left to tell this node was a billboard.
        // `extract_billboard_mode` already does this downcast for the
        // hierarchical walker; reuse it here so both walkers agree on
        // the pre/post-10.1.0.0 mode normalization. Save/restore around
        // the recursion the same way `inherited_props` does, so a
        // billboard subtree's mode doesn't leak to its siblings.
        let prev_billboard = ctx.inherited_billboard;
        if let Some(mode) = extract_billboard_mode(block, node.av.flags) {
            ctx.inherited_billboard = Some(mode);
        }
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_flat(ctx, idx, &world_transform, depth + 1);
            }
        }
        ctx.inherited_props.truncate(prev_len);
        ctx.inherited_billboard = prev_billboard;
        return;
    }

    // Helper: surface shape-level collision into the `collisions`
    // out-list, mirroring the NiNode pattern at lines 491 / 549.
    // Most Bethesda content attaches `bhkCollisionObject` to a parent
    // NiNode, but Oblivion + some FO3 modded content attaches it
    // directly to the NiTriShape / BsTriShape / BSGeometry. Pre-fix
    // these shape-level collisions silently disappeared because the
    // walker only checked nodes. See NIF-D4-NEW-04 (audit 2026-05-12).
    fn push_shape_collision(
        scene: &NifScene,
        collisions: &mut Option<&mut Vec<ImportedCollision>>,
        collision_ref: BlockRef,
        world_transform: &NiTransform,
    ) {
        let Some(coll_out) = collisions else {
            return;
        };
        let Some((shape, body)) = extract_collision(scene, collision_ref) else {
            return;
        };
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
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);
        push_shape_collision(
            scene,
            &mut ctx.collisions,
            shape.av.collision_ref,
            &world_transform,
        );
        if let Some(mut mesh) = extract_mesh(
            scene,
            shape,
            &world_transform,
            ctx.inherited_props,
            ctx.pool,
        ) {
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.push(mesh);
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
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);
        push_shape_collision(
            scene,
            &mut ctx.collisions,
            shape.av.collision_ref,
            &world_transform,
        );
        if let Some(mut mesh) = extract_bs_tri_shape(scene, shape, &world_transform, ctx.pool) {
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.push(mesh);
        }
    }

    // BSLODTriShape (Skyrim/SSE distant-LOD) — see walk_node_local arm above.
    // Flat-walk path identical to NiTriShape but delegating via .base (#988).
    if let Some(lod) = block.as_any().downcast_ref::<NiLodTriShape>() {
        let shape = &lod.base;
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);
        push_shape_collision(
            scene,
            &mut ctx.collisions,
            shape.av.collision_ref,
            &world_transform,
        );
        if let Some(mut mesh) = extract_mesh(
            scene,
            shape,
            &world_transform,
            ctx.inherited_props,
            ctx.pool,
        ) {
            mesh.billboard_mode = ctx.inherited_billboard;
            // #2283 — see the mirrored hierarchical-walk arm above.
            mesh.bs_lod_cutoffs = Some([lod.lod0_size, lod.lod1_size, lod.lod2_size]);
            ctx.out.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BSGeometry>() {
        // #3640 — a live NiVisController targeting this shape gets a
        // chance to un-hide it at runtime instead of the shape never
        // existing for it to act on. See `has_live_visibility_controller`'s
        // doc for why this is shape-scoped only.
        if shape.av.flags & 0x01 != 0
            && !has_live_visibility_controller(scene, shape.av.net.controller_ref)
        {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);
        push_shape_collision(
            scene,
            &mut ctx.collisions,
            shape.av.collision_ref,
            &world_transform,
        );
        if let Some(mut mesh) =
            extract_bs_geometry(scene, shape, &world_transform, ctx.pool, resolver)
        {
            mesh.billboard_mode = ctx.inherited_billboard;
            ctx.out.push(mesh);
        }
    }
}

#[cfg(test)]
mod tests;

// #3856 — `resolve_affected_node_names` / `resolve_block_ref_names` live here
// rather than in `texture_effect.rs` (where the issue's table put them)
// because `imported_light_from_base` needs them too: a light's
// `affected_nodes` list resolves through exactly the same block-ref-to-name
// walk. One shared home beats a cross-sibling import in one direction or a
// duplicate in both.
/// Resolve the `NiDynamicEffect.Affected Nodes` Ptr list to a list of
/// node names. The on-disk values are 4-byte `Ptr<NiAVObject>` entries:
/// `u32::MAX` = null pointer, otherwise a block index. Names are
/// pulled from each target's `NiObjectNET.name`. Null entries and
/// targets that fail to resolve to a named scene-graph block are
/// dropped silently — empty list = "no restriction" by convention,
/// so partial restrictions stay meaningful even with unresolvable
/// pointers (corrupt content). See #335.
pub(crate) fn resolve_affected_node_names(
    scene: &NifScene,
    ptrs: &[u32],
) -> Vec<std::sync::Arc<str>> {
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
        // Refcount-bump the existing `Arc<str>` storage when the
        // implementor exposes it (every NiObjectNET-backed block
        // does — default trait impl returns None as the safety
        // hatch). Falls back to allocating a fresh `Arc<str>` from
        // the `&str` accessor only for impls that don't override.
        // #872.
        if let Some(arc) = net.name_arc() {
            if !arc.is_empty() {
                out.push(std::sync::Arc::clone(arc));
            }
            continue;
        }
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
pub(crate) fn resolve_block_ref_names(
    scene: &NifScene,
    refs: &[BlockRef],
) -> Vec<std::sync::Arc<str>> {
    let mut out: Vec<std::sync::Arc<str>> = Vec::with_capacity(refs.len());
    for r in refs {
        let Some(idx) = r.index() else { continue };
        let Some(block) = scene.get(idx) else {
            continue;
        };
        let Some(net) = block.as_object_net() else {
            continue;
        };
        // Same Arc<str> refcount-bump path as
        // `resolve_affected_node_names` above. #872.
        if let Some(arc) = net.name_arc() {
            if !arc.is_empty() {
                out.push(std::sync::Arc::clone(arc));
            }
            continue;
        }
        let Some(name) = net.name() else { continue };
        if name.is_empty() {
            continue;
        }
        out.push(std::sync::Arc::from(name));
    }
    out
}

/// #3856 — pin the property that made the satellite extraction safe.
///
/// `walk_node_lights`, `walk_node_texture_effects` and
/// `walk_node_particle_emitters_flat` are *independent entry points*: they are
/// invoked from `import/mod.rs` and never from `walk_node_hierarchical` or
/// `walk_node_flat`. That is the whole reason they could move to sibling files
/// verbatim, with no shared traversal state to thread.
///
/// If a future edit calls one of them from inside a scene-graph walker, the
/// files stop being independent and the next person to move code between them
/// inherits a coupling nothing announced. A source-shape check is the right
/// shape here for the same reason the light-dispatch sibling above uses one:
/// the property is about *call structure*, and a behavioural test would pass
/// just as happily with the call present.
#[cfg(test)]
mod satellite_independence_tests {
    /// The two scene-graph walkers, as source text.
    fn scene_graph_walkers() -> String {
        let src = include_str!("mod.rs");
        let mut out = String::new();
        for name in ["fn walk_node_hierarchical(", "fn walk_node_flat("] {
            let start = src
                .find(name)
                .unwrap_or_else(|| panic!("{name} must exist in walk/mod.rs"));
            // Body ends at the first column-0 closing brace after the start.
            let end = src[start..]
                .find("\n}")
                .map(|i| start + i)
                .expect("walker body must terminate at column 0");
            out.push_str(&src[start..end]);
        }
        out
    }

    #[test]
    fn the_scene_graph_walkers_never_call_a_satellite_walker() {
        let walkers = scene_graph_walkers();
        assert!(
            walkers.contains("switch_active_children"),
            "sanity: the extracted walker bodies must be non-empty — if this fires, \
             the slicing above broke and the assertions below are vacuous"
        );
        for satellite in [
            "walk_node_lights",
            "walk_node_texture_effects",
            "walk_node_particle_emitters_flat",
        ] {
            assert!(
                !walkers.contains(satellite),
                "`{satellite}` is now called from a scene-graph walker. It lives in a \
                 sibling file precisely because it was an independent entry point \
                 invoked only from `import/mod.rs` (#3856) — calling it from here \
                 reintroduces the shared-state coupling the split removed. Either \
                 keep the call out, or move the satellite back and say so."
            );
        }
    }
}
