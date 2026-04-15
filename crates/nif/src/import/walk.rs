//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
use crate::blocks::node::{
    BsMultiBoundNode, BsOrderedNode, BsRangeNode, BsTreeNode, BsValueNode, NiBillboardNode,
    NiLODNode, NiNode, NiSortAdjustNode, NiSwitchNode,
};
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::blocks::NiObject;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiTransform};

use super::collision::extract_collision;
use super::coord::zup_matrix_to_yup_quat;
use super::mesh::{
    extract_bs_tri_shape, extract_bs_tri_shape_local, extract_mesh, extract_mesh_local,
};
use super::transform::compose_transforms;
use super::{
    ImportedCollision, ImportedLight, ImportedMesh, ImportedNode, ImportedScene, LightKind,
};

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
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active child, not all
    // children. Must be checked BEFORE as_ni_node() since these types
    // are no longer unwrapped there. See #212.
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x21 != 0 {
            return;
        }
        if is_editor_marker(node.av.net.name.as_deref()) {
            return;
        }
        let t = &node.av.transform.translation;
        let quat = zup_matrix_to_yup_quat(&node.av.transform.rotation);
        let collision = extract_collision(scene, node.av.collision_ref);
        let billboard_mode = extract_billboard_mode(block, node.av.flags);

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: [t.x, t.z, -t.y],
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
            collision,
            billboard_mode,
        });

        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for idx in active_children {
            walk_node_hierarchical(scene, idx, Some(this_node_idx), inherited_props, out);
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x21 != 0 {
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

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: [t.x, t.z, -t.y],
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
            collision,
            billboard_mode,
        });

        // Merge this node's properties with the inherited set via stack
        // discipline. Child shapes see the union; their own properties
        // take priority inside extract_material_info because shape props
        // are iterated before inherited props.
        let prev_len = inherited_props.len();
        inherited_props.extend_from_slice(&node.av.properties);
        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(scene, idx, Some(this_node_idx), inherited_props, out);
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_mesh_local(scene, shape, inherited_props) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_bs_tri_shape_local(scene, shape) {
            let mut mesh = mesh;
            mesh.parent_node = parent_node_idx;
            out.meshes.push(mesh);
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
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    // NiSwitchNode / NiLODNode: only walk the active child (#212).
    if let Some((node, active_children)) = switch_active_children(block) {
        if node.av.flags & 0x21 != 0 {
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
                    translation: [t.x, t.z, -t.y],
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
            );
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(node) = as_ni_node(block) {
        if node.av.flags & 0x21 != 0 {
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
                    translation: [t.x, t.z, -t.y],
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
                );
            }
        }
        inherited_props.truncate(prev_len);
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_mesh(scene, shape, &world_transform, inherited_props) {
            out.push(mesh);
        }
    }

    if let Some(shape) = block.as_any().downcast_ref::<BsTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }
        let world_transform = compose_transforms(parent_transform, &shape.av.transform);

        if let Some(mesh) = extract_bs_tri_shape(scene, shape, &world_transform) {
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
        if node.av.flags & 0x21 != 0 {
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
            &world,
            &l.base,
            LightKind::Directional,
            0.0,
            0.0,
        ));
        // no return — directional lights are leaves
    }
}

fn imported_light_from_base(
    world: &NiTransform,
    base: &crate::blocks::light::NiLightBase,
    kind: LightKind,
    radius: f32,
    outer_angle: f32,
) -> ImportedLight {
    // Z-up → Y-up: (x, y, z) → (x, z, -y).
    let t = &world.translation;
    let translation = [t.x, t.z, -t.y];

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

    ImportedLight {
        translation,
        direction,
        color,
        radius,
        kind,
        outer_angle,
    }
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
}
