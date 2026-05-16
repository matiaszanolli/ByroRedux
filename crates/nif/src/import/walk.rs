//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::bs_geometry::BSGeometry;
use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
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
        // BSValueNode + BSOrderedNode subclass-specific fields (#625).
        // NiSwitchNode / NiLODNode never overlap with these, so both
        // stay None here.
        let bs_value_node = extract_bs_value_node(block);
        let bs_ordered_node = extract_bs_ordered_node(block);

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
            bs_value_node,
            bs_ordered_node,
        });

        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_hierarchical(
                scene,
                idx,
                Some(this_node_idx),
                inherited_props,
                out,
                pool,
                resolver,
            );
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
        // BSValueNode value+flags (#625 / SK-D4-02) and BSOrderedNode
        // alpha_sort_bound (#625 / SK-D4-03). Both default to None for
        // plain NiNode and non-matching subclasses.
        let bs_value_node = extract_bs_value_node(block);
        let bs_ordered_node = extract_bs_ordered_node(block);

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
            bs_value_node,
            bs_ordered_node,
        });

        // Merge this node's properties with the inherited set via stack
        // discipline. Child shapes see the union; their own properties
        // take priority inside extract_material_info because shape props
        // are iterated before inherited props.
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(
                    scene,
                    idx,
                    Some(this_node_idx),
                    inherited_props,
                    out,
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

        // Surface shape-level collision onto the parent NiNode's
        // collision slot if the parent didn't already author one. The
        // hierarchical walker stores collisions on `ImportedNode`
        // (it has no separate `collisions` out-list like the flat
        // path), so a shape-bound `bhkCollisionObject` flows into
        // the same field as a node-bound one. See NIF-D4-NEW-04
        // (audit 2026-05-12). Oblivion + some FO3 modded content
        // attaches collision to the shape directly.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
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

        // Mirror of the NiTriShape branch above — see NIF-D4-NEW-04.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_bs_tri_shape_local(scene, shape, pool) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
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
            if let Some(parent) = out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_mesh_local(scene, shape, inherited_props, pool) {
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
        // Mirror of the NiTriShape branch above — see NIF-D4-NEW-04.
        if let Some(parent_idx) = parent_node_idx {
            if let Some(parent) = out.nodes.get(parent_idx) {
                if parent.collision.is_none() {
                    if let Some(collision) = extract_collision(scene, shape.av.collision_ref) {
                        out.nodes[parent_idx].collision = Some(collision);
                    }
                }
            }
        }
        if let Some(mesh) = extract_bs_geometry_local(scene, shape, pool, resolver) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    // Particle systems — see #401 / `ImportedParticleEmitter` and
    // #984 / `ImportedParticleForceField`. Modern emitter blocks
    // (`NiParticleSystem` / `NiMeshParticleSystem` / `NiParticles` /
    // `BSStripParticleSystem`) deserialise to a typed
    // `NiParticleSystem` whose `modifier_refs` we walk into the field-
    // modifier blocks. Legacy emitter / controller types stay on the
    // opaque `NiPSysBlock` fallback because they have no modifier list.
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiParticleSystem>()
    {
        out.particle_emitters
            .push(crate::import::ImportedParticleEmitter {
                parent_node: parent_node_idx,
                original_type: ps.original_type.clone(),
                color_curve: extract_first_color_curve(scene),
                force_fields: collect_force_fields(scene, &ps.modifier_refs),
            });
        return;
    }
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
    {
        match ps.original_type.as_str() {
            "NiParticleSystemController"
            | "NiBSPArrayController"
            | "NiAutoNormalParticles"
            | "NiRotatingParticles" => {
                out.particle_emitters
                    .push(crate::import::ImportedParticleEmitter {
                        parent_node: parent_node_idx,
                        original_type: ps.original_type.clone(),
                        color_curve: extract_first_color_curve(scene),
                        // Legacy controller path — no NiPSysModifier
                        // chain on the wire, so no authored force fields.
                        force_fields: Vec::new(),
                    });
            }
            _ => {}
        }
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

/// Scan the parsed NIF scene for the first `NiPSysColorModifier` and
/// resolve its `color_data_ref` to a `NiColorData` keyframe stream.
/// Returns `Some(curve)` with the t=0 and t=last RGBA keys when both
/// the modifier and the referenced data block are present and the
/// keyframe array is non-empty; `None` otherwise (no modifier in
/// scene → fall back to the heuristic preset).
///
/// First-pass scope per the issue body — this is a scene-level scan
/// rather than per-emitter, which is exact for the dominant single-
/// emitter-per-NIF case (every Bethesda hearth / torch / spell-cast
/// NIF). Multi-emitter NIFs would need to walk each
/// `NiParticleSystem.modifiers` list to attribute curves to specific
/// emitters; deferred until a multi-emitter regression surfaces. See
/// #707 / FX-2.
pub(super) fn extract_first_color_curve(
    scene: &NifScene,
) -> Option<crate::import::ParticleColorCurve> {
    use crate::blocks::interpolator::NiColorData;
    use crate::blocks::particle::NiPSysColorModifier;

    let modifier = scene
        .blocks
        .iter()
        .find_map(|b| b.as_any().downcast_ref::<NiPSysColorModifier>())?;
    let data_idx = modifier.color_data_ref.index()?;
    let data = scene.get_as::<NiColorData>(data_idx)?;
    let keys = &data.keys.keys;
    if keys.is_empty() {
        return None;
    }
    Some(crate::import::ParticleColorCurve {
        start: keys[0].value,
        end: keys.last().expect("non-empty checked above").value,
    })
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
        push_shape_collision(scene, &mut collisions, shape.av.collision_ref, &world_transform);
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
        push_shape_collision(scene, &mut collisions, shape.av.collision_ref, &world_transform);
        if let Some(mesh) = extract_bs_tri_shape(scene, shape, &world_transform, pool) {
            out.push(mesh);
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
        push_shape_collision(scene, &mut collisions, shape.av.collision_ref, &world_transform);
        if let Some(mesh) = extract_mesh(scene, shape, &world_transform, inherited_props, pool) {
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
        push_shape_collision(scene, &mut collisions, shape.av.collision_ref, &world_transform);
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
        for idx in active_children {
            walk_node_particle_emitters_flat(
                scene,
                idx,
                &world_transform,
                new_parent_name.clone(),
                out,
            );
        }
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

    // Mirror the hierarchical-walk dispatch (#984): try typed
    // `NiParticleSystem` first (carries `modifier_refs`); fall through
    // to opaque `NiPSysBlock` for legacy controller / particle types.
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiParticleSystem>()
    {
        let t = &parent_transform.translation;
        out.push(crate::import::ImportedParticleEmitterFlat {
            local_position: zup_point_to_yup(t),
            host_name: parent_node_name,
            original_type: ps.original_type.clone(),
            color_curve: extract_first_color_curve(scene),
            force_fields: collect_force_fields(scene, &ps.modifier_refs),
        });
        return;
    }
    if let Some(ps) = block
        .as_any()
        .downcast_ref::<crate::blocks::particle::NiPSysBlock>()
    {
        match ps.original_type.as_str() {
            "NiParticleSystemController"
            | "NiBSPArrayController"
            | "NiAutoNormalParticles"
            | "NiRotatingParticles" => {
                let t = &parent_transform.translation;
                out.push(crate::import::ImportedParticleEmitterFlat {
                    local_position: zup_point_to_yup(t),
                    host_name: parent_node_name,
                    original_type: ps.original_type.clone(),
                    color_curve: extract_first_color_curve(scene),
                    force_fields: Vec::new(),
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
pub(super) fn extract_bs_ordered_node(block: &dyn NiObject) -> Option<super::BsOrderedNodeData> {
    block
        .as_any()
        .downcast_ref::<BsOrderedNode>()
        .map(|n| super::BsOrderedNodeData {
            alpha_sort_bound: n.alpha_sort_bound,
            is_static_bound: n.is_static_bound,
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

    /// Regression for #872 / NIF-PERF-08. Both resolvers must take the
    /// `name_arc()` fast path (`Arc::clone` ⇒ refcount bump) instead of
    /// `Arc::from(&str)` (fresh heap alloc + byte copy). On cell-load
    /// critical paths — many lights' affected_nodes, every BSTreeNode's
    /// trunk + branch bone lists — that's the difference between
    /// `O(refs)` allocations and zero. We pin the contract via
    /// `Arc::ptr_eq`: the returned Arc must alias the source Arc on
    /// the underlying NiObjectNET, not a freshly minted copy.
    #[test]
    fn resolvers_refcount_bump_instead_of_realloc() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(node_with_name("TrunkBone")));
        scene.blocks.push(Box::new(node_with_name("BranchBoneA")));

        let original_trunk_arc = scene
            .get(0)
            .and_then(|b| b.as_object_net())
            .and_then(|n| n.name_arc())
            .expect("seed Arc must exist on the NiObjectNET")
            .clone();
        let original_branch_arc = scene
            .get(1)
            .and_then(|b| b.as_object_net())
            .and_then(|n| n.name_arc())
            .expect("seed Arc must exist")
            .clone();

        // Path 1: BSTreeNode bone-list resolution (the SpeedTree case
        // called out in #872). BlockRef-typed.
        let bone_refs = [BlockRef(0), BlockRef(1)];
        let bone_names = resolve_block_ref_names(&scene, &bone_refs);
        assert_eq!(bone_names.len(), 2);
        assert!(
            std::sync::Arc::ptr_eq(&bone_names[0], &original_trunk_arc),
            "BSTreeNode bone-list resolver must Arc::clone, not Arc::from(&str)"
        );
        assert!(
            std::sync::Arc::ptr_eq(&bone_names[1], &original_branch_arc),
            "all entries take the refcount-bump fast path"
        );

        // Path 2: NiDynamicEffect.affected_nodes resolution (the lights
        // case bundled in the same fix). Ptr-typed (u32).
        let affected = [0u32, 1u32];
        let lit_names = resolve_affected_node_names(&scene, &affected);
        assert_eq!(lit_names.len(), 2);
        assert!(
            std::sync::Arc::ptr_eq(&lit_names[0], &original_trunk_arc),
            "affected_nodes resolver shares the same fast path"
        );
        assert!(std::sync::Arc::ptr_eq(&lit_names[1], &original_branch_arc));

        // Strong-count sanity: the seed clone above + 2 entries each
        // from the two resolvers ⇒ ≥ 4 references to the trunk Arc.
        // Pre-fix the resolvers minted fresh allocations, leaving
        // strong_count == 2 (block storage + our seed clone) and the
        // returned Arcs would each be strong_count == 1.
        assert!(
            std::sync::Arc::strong_count(&original_trunk_arc) >= 4,
            "post-fix every resolved entry shares the seed Arc — \
             strong_count must reflect the refcount bump (was {})",
            std::sync::Arc::strong_count(&original_trunk_arc)
        );
    }
}

#[cfg(test)]
mod texture_effect_import_tests {
    //! Regression tests for #891 / LC-D2-NEW-01 — `NiTextureEffect`
    //! blocks must surface as `ImportedTextureEffect` after the import
    //! walk, with world-space pose, interned texture path, and
    //! resolved affected-node names. Pre-fix the parser captured all
    //! 12 wire fields but no consumer read them, so vanilla Oblivion
    //! sun gobos / FO3 / FNV light cookies parsed and were silently
    //! discarded.
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::blocks::texture::{NiSourceTexture, NiTextureEffect};
    use crate::import::ImportedTextureEffect;
    use crate::types::{BlockRef, NiMatrix3, NiTransform};
    use byroredux_core::string::StringPool;
    use std::sync::Arc;

    fn node_named(name: &str, children: Vec<BlockRef>) -> NiNode {
        NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children,
            effects: Vec::new(),
        }
    }

    fn make_texture_effect(
        affected: Vec<u32>,
        source_ref: BlockRef,
        texture_type: u32,
        coord_gen: u32,
    ) -> NiTextureEffect {
        NiTextureEffect {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from("SunGobo")),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            switch_state: true,
            affected_nodes: affected,
            model_projection_matrix: NiMatrix3::default(),
            model_projection_translation: [0.0; 3],
            texture_filtering: 0,
            max_anisotropy: 1,
            texture_clamping: 0,
            texture_type,
            coordinate_generation_type: coord_gen,
            source_texture_ref: source_ref,
            enable_plane: false,
            plane: [0.0; 4],
            ps2_l: 0,
            ps2_k: 0,
        }
    }

    fn make_source_texture(filename: &str) -> NiSourceTexture {
        NiSourceTexture {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            use_external: true,
            filename: Some(Arc::from(filename)),
            pixel_data_ref: BlockRef::NULL,
            pixel_layout: 0,
            use_mipmaps: 1,
            alpha_format: 0,
            is_static: true,
        }
    }

    /// Happy path: a NIF with one NiNode root + one NiTextureEffect
    /// child that references a NiSourceTexture and lists two affected
    /// nodes. The walker must produce one `ImportedTextureEffect`
    /// with: interned texture path, both texture-type and
    /// coord-gen-type fields preserved, and both affected-node names
    /// resolved through the same `resolve_affected_node_names` path
    /// `ImportedLight` uses.
    #[test]
    fn import_texture_effect_round_trips_path_and_affected_nodes() {
        // Build the scene blocks:
        //   0 = root NiNode (has child #1)
        //   1 = NiTextureEffect (refs source #2, affects nodes #3, #4)
        //   2 = NiSourceTexture (filename = "textures\\sun_gobo.dds")
        //   3 = NiNode "SunDiscBone"
        //   4 = NiNode "CloudsBone"
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", vec![BlockRef(1)])));
        scene.blocks.push(Box::new(make_texture_effect(
            vec![3, 4],
            BlockRef(2),
            0, // ProjectedLight
            1, // WorldPerspective
        )));
        scene
            .blocks
            .push(Box::new(make_source_texture("textures\\sun_gobo.dds")));
        scene
            .blocks
            .push(Box::new(node_named("SunDiscBone", Vec::new())));
        scene
            .blocks
            .push(Box::new(node_named("CloudsBone", Vec::new())));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects = crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert_eq!(
            effects.len(),
            1,
            "one NiTextureEffect → one ImportedTextureEffect"
        );
        let eff = &effects[0];

        // Texture path interned through the pool — resolve back for
        // the comparison; the pool lower-cases on intern.
        let path = eff
            .texture_path
            .and_then(|fs| pool.resolve(fs).map(str::to_owned));
        assert_eq!(path.as_deref(), Some("textures\\sun_gobo.dds"));

        assert_eq!(eff.texture_type, 0, "ProjectedLight roundtrip");
        assert_eq!(
            eff.coordinate_generation_type, 1,
            "WorldPerspective roundtrip"
        );

        assert_eq!(eff.affected_node_names.len(), 2);
        assert_eq!(&*eff.affected_node_names[0], "SunDiscBone");
        assert_eq!(&*eff.affected_node_names[1], "CloudsBone");
    }

    /// A NiTextureEffect whose `source_texture_ref` is null leaves
    /// `texture_path` as `None` — empty paths must drop rather than
    /// intern an empty string into the pool. Same convention the
    /// material walker uses for empty texture slots (#609).
    #[test]
    fn texture_effect_with_null_source_ref_leaves_path_none() {
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", vec![BlockRef(1)])));
        scene.blocks.push(Box::new(make_texture_effect(
            Vec::new(),
            BlockRef::NULL,
            2, // Environment
            2, // SphereMap
        )));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects: Vec<ImportedTextureEffect> =
            crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert_eq!(effects.len(), 1);
        assert!(
            effects[0].texture_path.is_none(),
            "null source_texture_ref must produce no path"
        );
        assert_eq!(effects[0].texture_type, 2);
        assert_eq!(effects[0].coordinate_generation_type, 2);
    }

    /// A NIF without any `NiTextureEffect` blocks must produce an
    /// empty result. NO_REGRESSION check from the issue's
    /// completeness checklist — non-texture-effect scenes must not
    /// be perturbed by the new walker.
    #[test]
    fn scene_without_texture_effects_returns_empty() {
        let mut scene = NifScene::default();
        scene
            .blocks
            .push(Box::new(node_named("Scene Root", Vec::new())));
        scene.root_index = Some(0);

        let mut pool = StringPool::new();
        let effects = crate::import::import_nif_texture_effects(&scene, &mut pool);
        assert!(effects.is_empty());
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

#[cfg(test)]
mod switch_node_walker_tests {
    //! Regression tests for #718 / NIF-D4-02: `walk_node_lights` and
    //! `walk_node_particle_emitters_flat` must walk through
    //! `NiSwitchNode` subtrees (previously they only called
    //! `as_ni_node`, which returns `None` for NiSwitchNode/NiLODNode,
    //! silently dropping any lights/emitters inside them).
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::light::{NiLightBase, NiPointLight};
    use crate::blocks::node::{NiLODNode, NiNode, NiSwitchNode};
    use crate::types::{BlockRef, NiColor, NiTransform};
    use std::sync::Arc;

    fn blank_av(name: Option<&str>) -> NiAVObjectData {
        NiAVObjectData {
            net: NiObjectNETData {
                name: name.map(Arc::from),
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            flags: 0,
            transform: NiTransform::default(),
            properties: Vec::new(),
            collision_ref: BlockRef::NULL,
        }
    }

    fn blank_node(name: Option<&str>, children: Vec<BlockRef>) -> NiNode {
        NiNode {
            av: blank_av(name),
            children,
            effects: Vec::new(),
        }
    }

    fn point_light_block() -> Box<dyn NiObject> {
        Box::new(NiPointLight {
            base: NiLightBase {
                av: blank_av(Some("TestLight")),
                switch_state: true,
                affected_nodes: Vec::new(),
                dimmer: 1.0,
                ambient_color: NiColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                },
                diffuse_color: NiColor {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                },
                specular_color: NiColor {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                },
            },
            constant_attenuation: 0.0,
            linear_attenuation: 0.0,
            quadratic_attenuation: 1.0,
        })
    }

    /// Regression for #718: a NiSwitchNode wrapping a NiPointLight child
    /// must yield the light from `walk_node_lights`.  Pre-fix the walker
    /// went straight to `as_ni_node`, which returns `None` for
    /// NiSwitchNode, silently dropping the light.
    #[test]
    fn walk_node_lights_traverses_ni_switch_node() {
        // Scene layout:
        //   [0] NiSwitchNode  { active_index=0, children=[1] }
        //   [1] NiPointLight  { diffuse=(1,0,0) }
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(NiSwitchNode {
            base: blank_node(None, vec![BlockRef(1)]),
            switch_flags: 0,
            index: 0,
        }));
        scene.blocks.push(point_light_block());
        scene.root_index = Some(0);

        let mut lights = Vec::new();
        walk_node_lights(&scene, 0, &NiTransform::default(), &mut lights);

        assert_eq!(
            lights.len(),
            1,
            "pre-#718: NiSwitchNode was invisible to walk_node_lights — light lost"
        );
        assert_eq!(lights[0].color, [1.0, 0.0, 0.0]);
    }

    /// Regression for #718: a NiLODNode wrapping a NiPointLight child
    /// must also yield the light (LOD 0 = highest detail is always walked).
    #[test]
    fn walk_node_lights_traverses_ni_lod_node() {
        let mut scene = NifScene::default();
        scene.blocks.push(Box::new(NiLODNode {
            base: NiSwitchNode {
                base: blank_node(None, vec![BlockRef(1), BlockRef::NULL]),
                switch_flags: 0,
                index: 0,
            },
            lod_level_data: BlockRef::NULL,
        }));
        scene.blocks.push(point_light_block());
        scene.root_index = Some(0);

        let mut lights = Vec::new();
        walk_node_lights(&scene, 0, &NiTransform::default(), &mut lights);

        assert_eq!(lights.len(), 1, "NiLODNode must expose its LOD-0 light");
    }
}
