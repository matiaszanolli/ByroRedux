//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::bs_geometry::BSGeometry;
use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
use crate::blocks::node::{
    BsDistantObjectInstancedNode, BsMultiBoundNode, BsOrderedNode, BsRangeNode, BsTreeNode,
    BsValueNode, BsWeakReferenceNode, NiBillboardNode, NiLODNode, NiNode, NiRangeLODData,
    NiSortAdjustNode, NiSwitchNode,
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
use super::{
    ImportedCollision, ImportedLight, ImportedMesh, ImportedNode, ImportedScene, LightKind,
    LodGroupData, MeshResolver, TreeBones,
};
use crate::blocks::extra_data::BsPackedCombinedGeomDataExtra;
use crate::blocks::node::BsRangeKind;
use byroredux_core::string::StringPool;

/// Extract the sprite texture, blend factors and `BSEffectShaderProperty`
/// payload attached to a particle system. Particle systems carry the same
/// shader/property references as geometry, so route them through the shared
/// material walker instead of silently replacing the authored sprite with
/// bindless slot zero.
///
/// #2610 — the walker already builds the full [`MaterialInfo`] here, so the
/// authored BGEM effect payload (`effect_soft` / `effect_lit` / the two
/// greyscale-palette bits / `lighting_influence`) comes for free. It used to
/// be dropped on the floor, which is why every particle `DrawCommand`
/// hardcoded `effect_shader_flags: 0`.
///
/// [`MaterialInfo`]: super::material::MaterialInfo
fn extract_particle_material(
    scene: &NifScene,
    ps: &crate::blocks::particle::NiParticleSystem,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> ParticleMaterial {
    let info = super::material::extract_material_info_from_refs(
        scene,
        ps.shader_property_ref,
        ps.alpha_property_ref,
        &ps.properties,
        inherited_props,
        pool,
    );
    let texture_path = info
        .texture_path
        .and_then(|path| pool.resolve(path).map(str::to_owned));
    let authored_blend = info.alpha_blend || info.alpha_blend_authored;
    ParticleMaterial {
        texture_path,
        src_blend: authored_blend.then_some(info.src_blend_mode),
        dst_blend: authored_blend.then_some(info.dst_blend_mode),
        effect_shader: info.effect_shader,
    }
}

/// Return of [`extract_particle_material`] — the authored material state a
/// particle system contributes to its spawned emitter.
struct ParticleMaterial {
    texture_path: Option<String>,
    src_blend: Option<u8>,
    dst_blend: Option<u8>,
    effect_shader: Option<crate::import::BsEffectShaderData>,
}

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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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

/// Walk a `NiParticleSystem.modifier_refs` chain and collect every
/// `NiPSys{Gravity,Vortex,Drag,Turbulence,Air,Radial}FieldModifier`
/// into an `ImportedParticleForceField` list. Inactive modifiers
/// (per [`NiPSysModifierBase::active`]) and stale refs are skipped.
/// See #984 / NIF-D5-ORPHAN-A2.
pub(super) fn collect_force_fields(
    scene: &NifScene,
    modifier_refs: &[crate::types::BlockRef],
) -> Vec<crate::import::ImportedParticleForceField> {
    use crate::blocks::particle::{
        NiPSysAirFieldModifier, NiPSysDragFieldModifier, NiPSysGravityFieldModifier,
        NiPSysRadialFieldModifier, NiPSysTurbulenceFieldModifier, NiPSysVortexFieldModifier,
    };
    use crate::import::ImportedParticleForceField as F;

    let mut out = Vec::new();
    for r in modifier_refs {
        let Some(idx) = r.index() else { continue };
        let Some(block) = scene.blocks.get(idx) else {
            continue;
        };
        let any = block.as_any();
        if let Some(g) = any.downcast_ref::<NiPSysGravityFieldModifier>() {
            if !g.modifier_base.active {
                continue;
            }
            out.push(F::Gravity {
                direction: g.direction,
                strength: g.field_base.magnitude,
                decay: g.field_base.attenuation,
            });
        } else if let Some(v) = any.downcast_ref::<NiPSysVortexFieldModifier>() {
            if !v.modifier_base.active {
                continue;
            }
            out.push(F::Vortex {
                axis: v.direction,
                strength: v.field_base.magnitude,
                decay: v.field_base.attenuation,
            });
        } else if let Some(d) = any.downcast_ref::<NiPSysDragFieldModifier>() {
            if !d.modifier_base.active {
                continue;
            }
            out.push(F::Drag {
                strength: d.field_base.magnitude,
                direction: d.direction,
                use_direction: d.use_direction,
            });
        } else if let Some(t) = any.downcast_ref::<NiPSysTurbulenceFieldModifier>() {
            if !t.modifier_base.active {
                continue;
            }
            out.push(F::Turbulence {
                frequency: t.frequency,
                scale: t.field_base.magnitude,
            });
        } else if let Some(a) = any.downcast_ref::<NiPSysAirFieldModifier>() {
            if !a.modifier_base.active {
                continue;
            }
            out.push(F::Air {
                direction: a.direction,
                strength: a.field_base.magnitude,
                falloff: a.field_base.attenuation,
            });
        } else if let Some(rd) = any.downcast_ref::<NiPSysRadialFieldModifier>() {
            if !rd.modifier_base.active {
                continue;
            }
            out.push(F::Radial {
                strength: rd.field_base.magnitude,
                falloff: rd.field_base.attenuation,
            });
        }
    }
    out
}

/// Scan the parsed NIF scene for the first authored particle colour ramp.
/// Two sources, in priority order:
///   1. `NiPSysColorModifier` → `NiColorData` keyframe stream (Oblivion /
///      Skyrim-era + the modern reference-based modifier). Returns the
///      t=0 and t=last RGBA keys.
///   2. `BSPSysSimpleColorModifier` (#1345) — the dominant FO3/FNV-era
///      modifier, which embeds its 3-key ramp INLINE rather than
///      referencing a `NiColorData` block. Returns `Colors[0]` (birth) and
///      `Colors[2]` (death).
///
/// `None` when neither is present (→ fall back to the heuristic preset).
///
/// First-pass scope per the issue body — this is a scene-level scan
/// rather than per-emitter, which is exact for the dominant single-
/// emitter-per-NIF case (every Bethesda hearth / torch / spell-cast /
/// geyser NIF). Multi-emitter NIFs would need to walk each
/// `NiParticleSystem.modifiers` list to attribute curves to specific
/// emitters; deferred until a multi-emitter regression surfaces. See
/// #707 / FX-2 + #1345 + #1402.
pub(super) fn extract_first_color_curve(
    scene: &NifScene,
) -> Option<crate::import::ParticleColorCurve> {
    use crate::blocks::interpolator::NiColorData;
    use crate::blocks::particle::{BSPSysSimpleColorModifier, NiPSysColorModifier};

    // Reject NaN, ±Inf, and the nif.xml FLT_MAX sentinel (≥ 3.0e38) in any
    // component.  Analogous to `sane()` in `extract_emitter_rate` — authored
    // RGBA values are always in a normal float range; a sentinel or corrupt
    // parse would otherwise reach the shader as a wildly out-of-range colour.
    fn is_valid_color(c: [f32; 4]) -> bool {
        c.iter().all(|&x| x.is_finite() && x < 3.0e38)
    }

    // 1. Legacy reference-based modifier → NiColorData keyframe stream.
    if let Some(modifier) = scene
        .blocks
        .iter()
        .find_map(|b| b.as_any().downcast_ref::<NiPSysColorModifier>())
    {
        if let Some(data_idx) = modifier.color_data_ref.index() {
            if let Some(data) = scene.get_as::<NiColorData>(data_idx) {
                let keys = &data.keys.keys;
                if !keys.is_empty() {
                    let start = keys[0].value;
                    let end = keys.last().expect("non-empty checked above").value;
                    if is_valid_color(start) && is_valid_color(end) {
                        return Some(crate::import::ParticleColorCurve { start, end });
                    }
                }
            }
        }
    }

    // 2. #1345 — fall back to the inline BSPSysSimpleColorModifier ramp.
    if let Some(scm) = scene
        .blocks
        .iter()
        .find_map(|b| b.as_any().downcast_ref::<BSPSysSimpleColorModifier>())
    {
        let start = scm.colors[0];
        let end = scm.colors[2];
        if is_valid_color(start) && is_valid_color(end) {
            return Some(crate::import::ParticleColorCurve { start, end });
        }
    }

    None
}

/// Scan the parsed NIF scene for the first `NiPSysEmitter` and return its
/// decoded base spawn parameters. `None` when the scene has no emitter
/// block (→ fall back to the heuristic preset). Same scene-level first-
/// match scope as [`extract_first_color_curve`] — exact for the dominant
/// single-emitter-per-NIF case; multi-emitter NIFs would need per-system
/// attribution (deferred until a regression surfaces). See
/// `docs/engine/nifal.md` — particles slice.
pub(super) fn extract_emitter_params(
    scene: &NifScene,
) -> Option<crate::import::ImportedEmitterParams> {
    use crate::blocks::particle::NiPSysEmitter;

    let emitter = scene
        .blocks
        .iter()
        .find_map(|b| b.as_any().downcast_ref::<NiPSysEmitter>())?;
    let p = &emitter.params;
    // Pair the emitter base with the first grow/fade modifier's
    // base_scale (size multiplier), if any. NIFAL-S5 (#1434) — reject a
    // non-finite or non-positive raw scale here: it feeds `initial_radius ×
    // base_scale` (systems/particle.rs), so 0.0/negative spawns zero-or-
    // inverted-size particles and NaN/Inf poisons the product. Dropping just
    // the modifier to `None` falls back to the ×1.0 default rather than
    // rejecting the whole (otherwise valid) emitter — sibling of the
    // NIFAL-S3 finite filter below.
    let base_scale = scene
        .blocks
        .iter()
        .find_map(|b| {
            b.as_any()
                .downcast_ref::<crate::blocks::particle::NiPSysGrowFadeModifier>()
        })
        .and_then(|m| m.base_scale)
        .filter(|s| s.is_finite() && *s > 0.0);
    // NIFAL-S3 (#1411) — reject corrupt emitter scalars before they reach
    // `apply_emitter_params`, which copies every one straight into the
    // particle preset. A single non-finite value (NaN/Inf from a malformed
    // NIF) poisons every spawned particle's per-frame integration, and a
    // non-positive `life_span` spawns already-dead particles. Fall back to
    // the heuristic preset (`None`) rather than leak garbage. Sibling of
    // `extract_emitter_rate`'s `sane()` finite filter; the positivity
    // checks match the issue's `life_span > 0` / `initial_radius >= 0`.
    let all_finite = p.speed.is_finite()
        && p.speed_variation.is_finite()
        && p.declination.is_finite()
        && p.declination_variation.is_finite()
        // #1445 — planar_angle / planar_angle_variation were lifted into
        // EmitterBaseParams but omitted from this sweep. Harmless today
        // (apply_emitter_params doesn't read them yet) but a latent NaN trap
        // the moment planar angle is wired into the spawn cone; include them
        // now so the guard can't be silently outrun by a future consumer.
        && p.planar_angle.is_finite()
        && p.planar_angle_variation.is_finite()
        && p.initial_radius.is_finite()
        // #1775 — radius_variation is now forwarded + consumed as per-particle
        // size jitter, so it joins the finite sweep (a NaN/Inf spread would
        // poison every spawned particle's start_size). Negative is tolerated:
        // the consumer (apply_emitter_params) takes its magnitude.
        && p.radius_variation.is_finite()
        && p.life_span.is_finite()
        && p.life_span_variation.is_finite();
    if !(all_finite && p.life_span > 0.0 && p.initial_radius >= 0.0) {
        log::debug!(
            "Rejecting NiPSysEmitter params (non-finite or non-positive): \
             speed={} speed_var={} decl={} decl_var={} radius={} life_span={} \
             life_var={} base_scale={:?} — falling back to heuristic preset",
            p.speed,
            p.speed_variation,
            p.declination,
            p.declination_variation,
            p.initial_radius,
            p.life_span,
            p.life_span_variation,
            base_scale,
        );
        return None;
    }
    Some(crate::import::ImportedEmitterParams {
        speed: p.speed,
        speed_variation: p.speed_variation,
        declination: p.declination,
        declination_variation: p.declination_variation,
        initial_color: p.initial_color,
        initial_radius: p.initial_radius,
        life_span: p.life_span,
        life_span_variation: p.life_span_variation,
        base_scale,
        radius_variation: p.radius_variation,
    })
}

/// Scan for the emitter's authored birth rate (particles/sec) and return
/// a single representative scalar. Modern path: the first
/// `NiPSysEmitterCtlr`'s interpolator → `NiFloatData` first key value, or
/// the `NiFloatInterpolator`'s constant value. Legacy fallback: the first
/// `NiPSysEmitterCtlrData` birth-rate key. `None` when no controller is
/// present (→ keep the preset's rate). Non-finite, negative, exactly `0.0`
/// (a ramp-up emitter's t=0 key → keep the preset rate, #1771), and
/// `FLT_MAX`-sentinel (`>= 3.0e38`) values are rejected: the sentinel on
/// `NiFloatInterpolator.value` is nif.xml's "use the keyed data" marker, so
/// even when the keyed `data_ref` is NULL it must not leak through the
/// constant-value branch as a ~3.4e38 spawn rate (cap-spawning every frame,
/// #1364). Scene-level first-match, same scope caveat as
/// [`extract_first_color_curve`] — secondary emitters in a multi-emitter NIF
/// share the first emitter's rate; deferred until a regression surfaces (#1402).
/// See `docs/engine/nifal.md` — particles spawn-rate follow-up.
/// Authored particle budget from the scene's `NiPSysData` block — nif.xml
/// `NiParticlesData.Num Vertices`, *"the maximum number of particles"*, which
/// on Bethesda `#BS202#` streams is the `BS Max Vertices` upper bound (#3344).
/// `None` when the scene has no particle-data block or it authored `0`.
///
/// Scene-level first-match, the same scope caveat carried by
/// [`extract_emitter_params`] and [`extract_emitter_rate`]: exact for the
/// dominant single-emitter-per-NIF case, and secondary emitters in a
/// multi-emitter NIF share the first block's budget. Following the particle
/// system's own `data_ref` would be exact, but that ref is only serialized on
/// the pre-SSE branch — `BS_GTE_SSE` streams replace it with a bounding
/// sphere + skin ref — so a per-system link would work on FO3/FNV/Oblivion
/// and silently fall back to this same scan everywhere else.
pub(super) fn extract_emitter_max_particles(scene: &NifScene) -> Option<u32> {
    // Find the first block that actually *carries* a budget, not the first
    // `NiPSysBlock`: 27 other `NiPSys*` types deserialise to that same marker
    // struct with `max_particles: None`, so `find_map(downcast).and_then(..)`
    // stops at whichever marker happens to come first in block order and
    // reports no budget at all. Caught by the #3343 magnitude floors, which
    // read 0/346 on FNV against a measured 1,262 budget-bearing blocks.
    scene
        .blocks
        .iter()
        .find_map(|b| {
            b.as_any()
                .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
                .and_then(|d| d.max_particles)
        })
        .filter(|m| *m > 0)
}

pub(super) fn extract_emitter_rate(scene: &NifScene) -> Option<f32> {
    use crate::anim::resolve_blend_interpolator_target;
    use crate::blocks::interpolator::{NiBlendFloatInterpolator, NiFloatData, NiFloatInterpolator};
    use crate::blocks::particle::{NiPSysEmitterCtlr, NiPSysEmitterCtlrData};

    fn sane(r: f32) -> Option<f32> {
        // Reject non-finite, negative, the FLT_MAX sentinel (`>= 3.0e38`, the
        // shader rimlight/backlight threshold + nif.xml's "use the keyed data"
        // marker — blocks/shader.rs), AND an exact 0.0. A zero first-key is a
        // ramp-up emitter (rate climbs from 0 over the clip — geyser/steam/
        // ignition FX); taking it as a permanent-zero constant rate makes the
        // spawn guard (`em.rate > 0.0`) kill the emitter for the whole clip, so
        // fall back to the preset spawn rate instead (#1771). Rate-curve
        // sampling over time is the fuller fix (#1402).
        (r.is_finite() && 0.0 < r && r < 3.0e38).then_some(r)
    }

    // NiFloatInterpolator → (keyed data | constant). Shared by the direct
    // case below and the NiBlendFloatInterpolator sub-interpolator case
    // (#2548), so the two chains can't silently diverge.
    fn float_interpolator_rate(
        scene: &NifScene,
        interp_idx: usize,
        curves: CurveTier,
    ) -> Option<f32> {
        let interp = scene.get_as::<NiFloatInterpolator>(interp_idx)?;
        if let Some(data_idx) = interp.data_ref.index() {
            if let Some(keys) = scene.get_as::<NiFloatData>(data_idx).map(|d| &d.keys.keys) {
                if let Some(r) = keys.first().and_then(|k| sane(k.value)) {
                    return Some(r);
                }
                // #3754 — the first key was rejected (a `0.0` ramp-up start,
                // per `sane`'s note), but the rest of the curve is right
                // here and used to be discarded whole: the interpolator's
                // own `value` on this shape is the `-FLT_MAX` "use the keyed
                // data" sentinel, which `sane` also rejects, so the emitter
                // fell all the way through to `fog.rs::particle_preset`'s
                // name-heuristic guess.
                if curves == CurveTier::Allowed {
                    if let Some(r) = curve_mean_rate(keys) {
                        return Some(r);
                    }
                }
            }
        }
        sane(interp.value)
    }

    /// The constant spawn rate that reproduces an authored rate curve's
    /// particle count — its **time-weighted mean**, by trapezoid over the
    /// authored keys (#3754).
    ///
    /// Why the mean and not the peak: `ParticleEmitter::rate` is particles
    /// *per second*, applied continuously and forever, so the faithful
    /// scalar reduction of a curve is the one that emits the same number of
    /// particles per clip cycle. The curves this reaches are not the
    /// ramp-to-plateau shape they were assumed to be — measured off the
    /// three meshes the report cites:
    ///
    /// ```text
    /// tenpengate01     `Close` 2 s  : 0,0,600,0,0     — a 0.13 s spike
    /// fxfallingrocks01 `Idle` 20 s  : two 300 spikes + two 120 spikes
    /// fxbubblestall01  `Idle` 16.7 s: ramp to a 30 plateau, then holds
    /// ```
    ///
    /// Only the third is a plateau. Taking each curve's maximum would run
    /// Tenpenny's gate at 600 /s forever against an authored average of
    /// ~20 /s, and the falling-rock ambient at 300 /s against ~19 /s — 30×
    /// and 16× overshoots, i.e. *further* from the file than the 35 /s
    /// preset this replaces. The mean is within a factor of two on all
    /// three.
    ///
    /// Sampling the curve over time is still the fuller fix (#1402); this is
    /// the best constant the file supports until an emitter can hold one.
    ///
    /// Returns `None` — leaving the existing fallbacks intact — when the
    /// curve carries no positive area, spans no time, or contains a
    /// non-finite / sentinel key. Negative key values are clamped to zero
    /// rather than subtracting area: a negative spawn rate has no meaning,
    /// and letting one cancel real emission would silently mute the emitter.
    fn curve_mean_rate(keys: &[crate::blocks::interpolator::FloatKey]) -> Option<f32> {
        let mut area = 0.0f64;
        let mut span = 0.0f64;
        for w in keys.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            // A sentinel or garbage key poisons the whole average, so bail
            // to the caller's fallbacks rather than averaging it in.
            if !a.value.is_finite()
                || !b.value.is_finite()
                || a.value.abs() >= 3.0e38
                || b.value.abs() >= 3.0e38
            {
                return None;
            }
            let dt = f64::from(b.time) - f64::from(a.time);
            if !dt.is_finite() || dt <= 0.0 {
                continue;
            }
            area += 0.5 * (f64::from(a.value.max(0.0)) + f64::from(b.value.max(0.0))) * dt;
            span += dt;
        }
        if span <= 0.0 {
            return None;
        }
        sane((area / span) as f32)
    }

    /// #3329 tier (d) — recover the authored rate from the scene's embedded
    /// `NiControllerSequence` blocks when the emitter controller's own
    /// interpolator is a manager-driven blend with no items.
    ///
    /// Walks every sequence's `controlled_blocks` for one whose resolved
    /// `controller_type` names an emitter controller, and runs its
    /// `interpolator_ref` through the same `float_interpolator_rate` the
    /// direct tiers use — so the four chains cannot diverge.
    ///
    /// Steady-state sequences win over transient ones. A single NIF commonly
    /// carries several (`Idle`, `Forward`, `OFF`, `Open`, …) and they author
    /// *different* rates: an ignition ramp or a one-shot burst is not the
    /// density the emitter runs at while the player is looking at it. Names
    /// are ranked, and a tie falls back to block order.
    fn sequence_emitter_rate(scene: &NifScene, curves: CurveTier) -> Option<f32> {
        use crate::anim::{resolve_cb_string, CbString};
        use crate::blocks::controller::NiControllerSequence;

        /// Lower is preferred. `Idle`/`SpecialIdle` are the steady-state
        /// loops; everything else is a transition whose rate is only correct
        /// for the moment it plays.
        fn sequence_rank(name: Option<&str>) -> u8 {
            match name.map(str::to_ascii_lowercase).as_deref() {
                Some("idle") => 0,
                Some(n) if n.ends_with("idle") => 1,
                Some(_) => 2,
                None => 3,
            }
        }

        let mut best: Option<(u8, f32)> = None;
        for seq in scene
            .blocks
            .iter()
            .filter_map(|b| b.as_any().downcast_ref::<NiControllerSequence>())
        {
            let rank = sequence_rank(seq.name.as_deref());
            // Nothing here can beat an already-found better-ranked hit.
            if best.as_ref().is_some_and(|(r, _)| *r <= rank) {
                continue;
            }
            for cb in &seq.controlled_blocks {
                let Some(ctype) = resolve_cb_string(scene, cb, CbString::ControllerType) else {
                    continue;
                };
                if !ctype.contains("EmitterCtlr") {
                    continue;
                }
                let Some(idx) = cb.interpolator_ref.index() else {
                    continue;
                };
                if let Some(r) = float_interpolator_rate(scene, idx, curves) {
                    best = Some((rank, r));
                    break;
                }
            }
        }
        best.map(|(_, r)| r)
    }

    /// Whether this pass may fall back to a rate curve's time-weighted mean
    /// (#3754). The whole tier chain runs once with [`Self::Rejected`] and,
    /// only if that finds nothing at all, again with [`Self::Allowed`].
    ///
    /// Two passes rather than one because the tiers resolve on the *first*
    /// emitter controller that yields a rate, so enabling the curve tier
    /// inline does not merely fill gaps — it lets an earlier controller win
    /// a mesh that already resolved. Measured over `Fallout - Meshes.bsa`:
    /// inline, 10 meshes gained a rate but 5 that already had one changed
    /// (`fxharoldfire` 90 → 41 /s, `ppurityfxtankfog01` 7.5 → 86.5 /s).
    /// Those five are not the defect being fixed, and re-ranking a mesh's
    /// emitters is #1402's business, not this fix's. Split into passes, the
    /// change is exactly additive: same 10 gained, zero changed.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum CurveTier {
        Rejected,
        Allowed,
    }

    fn resolve(scene: &NifScene, curves: CurveTier) -> Option<f32> {
        // Modern: controller → interpolator → (keyed data | constant).
        if let Some(ctlr) = scene
            .blocks
            .iter()
            .find_map(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlr>())
        {
            if let Some(interp_idx) = ctlr.interpolator_ref.index() {
                if let Some(r) = float_interpolator_rate(scene, interp_idx, curves) {
                    return Some(r);
                }
                // #2548 — 78% of real FO3 NiPSysEmitterCtlr.interpolator_ref
                // targets are NiBlendFloatInterpolator (most of FO3's fire/
                // explosion/dust/blood/gore VFX library), a weighted-array
                // wrapper this branch never followed at all — only the bare
                // NiFloatInterpolator case above, on 22% of real targets.
                // `resolve_blend_interpolator_target` (#334 / AR-08, already
                // used by the KF channel-extraction path) picks the highest-
                // `normalized_weight` sub-interpolator; `None` for the
                // manager-controlled case (no items to pick from — those are
                // driven externally and don't apply to a particle emitter
                // rate anyway). Fall back to the blend interpolator's own
                // constant `value` if no item resolves.
                if let Some(sub_idx) = resolve_blend_interpolator_target(scene, interp_idx) {
                    if let Some(r) = float_interpolator_rate(scene, sub_idx, curves) {
                        return Some(r);
                    }
                }
                if let Some(blend) = scene.get_as::<NiBlendFloatInterpolator>(interp_idx) {
                    if let Some(r) = sane(blend.value) {
                        return Some(r);
                    }
                }
                // #3329 — the manager-controlled residual #2548 left behind. When
                // the controller's interpolator is a `NiBlendFloatInterpolator`
                // with an EMPTY `items` array, tier (b) above cannot resolve a
                // sub-interpolator (`resolve_blend_interpolator_target` returns
                // `None` by design for that shape — those blends are driven
                // externally by a `NiControllerManager`, not from their own array)
                // and tier (c) reads a non-positive `value`. On vanilla FNV that
                // is 168 of the 307 emitter-bearing meshes: every single affected
                // file's blend has `items.len() == 0`.
                //
                // The authored rate is still in the file — it lives on the
                // sibling `NiControllerSequence`'s controlled block for the same
                // emitter controller. Scanning those recovers 155 of the 168
                // (`fxambdust*` 25/s, snowglobes 6/s, Lucky 38 reactor, the Strip
                // fountain, Helios steam, `dlc04fxcrashthroughfloor` 510/s).
                // Without it `apply_emitter_overlays` leaves the density at
                // `fog.rs::particle_preset`'s name-heuristic guess — off by ~6×
                // for snowglobes and ~15× for the DLC04 crash FX.
                if scene
                    .get_as::<NiBlendFloatInterpolator>(interp_idx)
                    .is_some()
                {
                    if let Some(r) = sequence_emitter_rate(scene, curves) {
                        return Some(r);
                    }
                }
            }
        }
        // Legacy: NiPSysEmitterCtlrData first birth-rate key.
        scene
            .blocks
            .iter()
            .find_map(|b| b.as_any().downcast_ref::<NiPSysEmitterCtlrData>())
            .and_then(|d| d.birth_rate_first)
            .and_then(sane)
    }

    resolve(scene, CurveTier::Rejected).or_else(|| resolve(scene, CurveTier::Allowed))
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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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
        if shape.av.flags & 0x01 != 0 {
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

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for idx in active_children {
            walk_node_lights(scene, idx, &world_transform, out);
        }
        return;
    }

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
    inherited_props: &mut Vec<BlockRef>,
    pool: &mut StringPool,
    out: &mut Vec<crate::import::ImportedParticleEmitterFlat>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        let new_parent_name = node.av.net.name.clone().or(parent_node_name);
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_particle_emitters_flat(
                scene,
                idx,
                &world_transform,
                new_parent_name.clone(),
                inherited_props,
                pool,
                out,
            );
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        // Pass this node's name down so descendant emitters inherit a
        // sensible host name even when the emitter block itself is
        // unnamed (the common case in vanilla content).
        let new_parent_name = node.av.net.name.clone().or(parent_node_name);
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_particle_emitters_flat(
                    scene,
                    idx,
                    &world_transform,
                    new_parent_name.clone(),
                    inherited_props,
                    pool,
                    out,
                );
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    // Mirror the hierarchical-walk dispatch (#984): handle the typed
    // `NiParticleSystem` (carries `modifier_refs`). The legacy controller
    // / particle types dispatch to `legacy_particle::*`, not `NiPSysBlock`,
    // so the old `NiPSysBlock` fall-through never matched them — that dead
    // arm was removed in #1327.
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiParticleSystem>()
    {
        // Compose the particle block's own local TRS onto the host-node
        // world transform (#1333). Pre-fix only `parent_transform` (the
        // host world) was used, zeroing any authored emitter offset —
        // smoke spawned inside the fire instead of above it.
        let world_transform = compose_transforms(parent_transform, &ps.transform);
        let pmat = extract_particle_material(scene, ps, inherited_props, pool);
        out.push(crate::import::ImportedParticleEmitterFlat {
            local_position: zup_point_to_yup(&world_transform.translation),
            host_name: parent_node_name,
            original_type: ps.original_type.clone(),
            texture_path: pmat.texture_path,
            src_blend: pmat.src_blend,
            dst_blend: pmat.dst_blend,
            effect_shader: pmat.effect_shader,
            color_curve: extract_first_color_curve(scene),
            force_fields: collect_force_fields(scene, &ps.modifier_refs),
            emitter_params: extract_emitter_params(scene),
            emitter_rate: extract_emitter_rate(scene),
            max_particles: extract_emitter_max_particles(scene),
        });
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
    // Extract local -Z column (light points along -Z in Gamebryo), then
    // convert Z-up to Y-up via the same [x, z, -y] swap that zup_point_to_yup uses.
    let [dx, dy, dz] = [-rot.rows[0][2], -rot.rows[1][2], -rot.rows[2][2]];
    let direction = byroredux_core::math::coord::zup_to_yup_pos([dx, dy, dz]);

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
        // #983 — surface the light's NIF block name so the cell
        // loader can spawn a matching `Name` component; the
        // animation system resolves NiLight*Controller channels by
        // that name. `None` for anonymous lights (rare).
        name: base.av.net.name.clone(),
    }
}

/// Recursively walk the scene graph accumulating world-space transforms
/// and collecting any `NiTextureEffect` block encountered. Mirrors
/// [`walk_node_lights`] one-for-one — the only difference is the
/// downcast type and the data captured at the leaf. See #891.
pub(super) fn walk_node_texture_effects(
    scene: &NifScene,
    block_idx: usize,
    parent_transform: &NiTransform,
    pool: &mut byroredux_core::string::StringPool,
    out: &mut Vec<crate::import::ImportedTextureEffect>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active children (#718).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &node.av.transform);
        for idx in active_children {
            walk_node_texture_effects(scene, idx, &world_transform, pool, out);
        }
        return;
    }

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
                walk_node_texture_effects(scene, idx, &world_transform, pool, out);
            }
        }
        return;
    }

    // NiTextureEffect leaf — extract using the world transform composed
    // from the parent chain plus the effect's own local transform.
    if let Some(eff) = block
        .as_any()
        .downcast_ref::<crate::blocks::texture::NiTextureEffect>()
    {
        let world = compose_transforms(parent_transform, &eff.av.transform);
        let translation = zup_point_to_yup(&world.translation);
        let rotation = zup_matrix_to_yup_quat(&world.rotation);
        let scale = world.scale;

        // Resolve source_texture_ref → NiSourceTexture → filename →
        // interned FixedString. Same `tex_desc_source_path` shape used
        // by material slots (#609 / D6-NEW-01); centralised here rather
        // than re-importing the helper because that one takes a TexDesc.
        let texture_path = eff
            .source_texture_ref
            .index()
            .and_then(|idx| scene.get_as::<crate::blocks::texture::NiSourceTexture>(idx))
            .and_then(|src| src.filename.as_deref())
            .and_then(|name| {
                if name.is_empty() {
                    None
                } else {
                    Some(pool.intern(name))
                }
            });

        let affected_node_names = resolve_affected_node_names(scene, &eff.affected_nodes);

        out.push(crate::import::ImportedTextureEffect {
            translation,
            rotation,
            scale,
            texture_path,
            texture_type: eff.texture_type,
            coordinate_generation_type: eff.coordinate_generation_type,
            affected_node_names,
        });
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

/// Extract the [`LodGroupData`] (LOD center + per-level near/far ranges) when
/// `block` is a [`NiLODNode`] whose `lod_level_data` resolves to a
/// [`NiRangeLODData`]. Center is converted NIF-Z-up → engine-Y-up. Returns
/// `None` for any other node type, a NULL/legacy ref, or empty ranges.
/// In-cell-LOD foundation: surfaced for a future distance-switch consumer;
/// the walker still imports only child 0 (highest detail).
pub(super) fn extract_lod_group(scene: &NifScene, block: &dyn NiObject) -> Option<LodGroupData> {
    let lod = block.as_any().downcast_ref::<NiLODNode>()?;
    let data_idx = lod.lod_level_data.index()?;
    let data = scene.get_as::<NiRangeLODData>(data_idx)?;
    if data.lod_levels.is_empty() {
        return None;
    }
    Some(LodGroupData {
        center: zup_point_to_yup(&data.lod_center),
        levels: data.lod_levels.clone(),
    })
}

/// Extract a `BSValueNode`'s `(value, value_flags)` pair. Pre-#625
/// `as_ni_node` unwrapped the wrapper to plain `NiNode`, dropping
/// these fields. Returns `None` for any block that isn't a
/// `BsValueNode`. See #625 (SK-D4-02).
pub(super) fn extract_bs_value_node(block: &dyn NiObject) -> Option<super::BsValueNodeData> {
    block
        .as_any()
        .downcast_ref::<crate::blocks::node::BsValueNode>()
        .map(|n| super::BsValueNodeData {
            value: n.value,
            flags: n.value_flags,
        })
}

/// Extract a `BSOrderedNode`'s draw-order metadata. Pre-#625
/// `as_ni_node` unwrapped the wrapper to plain `NiNode`, dropping
/// `alpha_sort_bound` + `is_static_bound`. Returns `None` for any
/// block that isn't a `BsOrderedNode`. See #625 (SK-D4-03).
///
/// #2008 — `alpha_sort_bound`'s `[x, y, z]` center is a point (unlike
/// `BsBound.dimensions`, an unsigned half-extent), so it gets the full
/// Z-up → Y-up swap-and-negate like every other node-local position on
/// `ImportedNode` and its siblings, not the magnitude-only axis reorder
/// `BsBound.dimensions` uses. `radius` is a magnitude and is unaffected
/// by the rotation.
pub(super) fn extract_bs_ordered_node(block: &dyn NiObject) -> Option<super::BsOrderedNodeData> {
    block.as_any().downcast_ref::<BsOrderedNode>().map(|n| {
        let [x, y, z, radius] = n.alpha_sort_bound;
        let center = byroredux_core::math::coord::zup_to_yup_pos([x, y, z]);
        super::BsOrderedNodeData {
            alpha_sort_bound: [center[0], center[1], center[2], radius],
            is_static_bound: n.is_static_bound,
        }
    })
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
    // #2210 (NIFAL-D3-02) — no attenuation → effectively infinite. This is
    // the OPERATIVE case, not a rare fallback: FNV/FO3 spawnable point
    // lights ship a zero-only attenuation triple (radius control is
    // deferred to the ESM LIGH record instead — see
    // `cell_loader/spawn.rs::spawn_nif_lights`'s ESM-radius preference),
    // so this branch is what 82/82 measured FNV lights actually hit.
    //
    // Pre-fix this returned a bare, uncited `2048.0`. `EXTERIOR_CELL_UNITS`
    // is a genuine citation instead of a nicer-looking guess: it's the
    // spec-defined Bethesda exterior cell size (4096 units on a side,
    // every Gamebryo/Creation title Oblivion→Starfield — see its own doc
    // comment) and `cell_loader/spawn.rs::light_radius_or_default` already
    // uses it as the sibling "we don't know this light's true radius, make
    // it at least visible across a cell" fallback for the exact same
    // semantic gap. Reusing it here (rather than an unrelated magic
    // number) keeps both "no radius info" fallbacks in this engine
    // grounded in the same cited constant instead of two independent
    // guesses that happen to differ.
    byroredux_core::math::coord::EXTERIOR_CELL_UNITS
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
mod tests;
