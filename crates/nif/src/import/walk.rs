//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::node::NiNode;
use crate::blocks::tri_shape::{BsTriShape, NiTriShape};
use crate::scene::NifScene;
use crate::types::NiTransform;

use super::collision::extract_collision;
use super::coord::zup_matrix_to_yup_quat;
use super::mesh::{
    extract_bs_tri_shape, extract_bs_tri_shape_local, extract_mesh, extract_mesh_local,
};
use super::transform::compose_transforms;
use super::{ImportedCollision, ImportedMesh, ImportedNode, ImportedScene};

/// Recursively walk the scene graph, preserving hierarchy.
/// NiNodes become ImportedNode entries; geometry becomes ImportedMesh with parent_node set.
pub(super) fn walk_node_hierarchical(
    scene: &NifScene,
    block_idx: usize,
    parent_node_idx: Option<usize>,
    out: &mut ImportedScene,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
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

        let this_node_idx = out.nodes.len();
        out.nodes.push(ImportedNode {
            name: node.av.net.name.clone(),
            translation: [t.x, t.z, -t.y],
            rotation: quat,
            scale: node.av.transform.scale,
            parent_node: parent_node_idx,
            collision,
        });

        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_hierarchical(scene, idx, Some(this_node_idx), out);
            }
        }
        return;
    }

    if let Some(shape) = block.as_any().downcast_ref::<NiTriShape>() {
        if shape.av.flags & 0x01 != 0 {
            return;
        }
        if is_editor_marker(shape.av.net.name.as_deref()) {
            return;
        }

        if let Some(mesh) = extract_mesh_local(scene, shape) {
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
    out: &mut Vec<ImportedMesh>,
    mut collisions: Option<&mut Vec<ImportedCollision>>,
) {
    let Some(block) = scene.get(block_idx) else {
        return;
    };

    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
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
                    translation: [t.x, t.z, -t.y],
                    rotation: quat,
                    scale: world_transform.scale,
                    shape,
                    body,
                });
            }
        }

        for child_ref in &node.children {
            if let Some(idx) = child_ref.index() {
                walk_node_flat(
                    scene,
                    idx,
                    &world_transform,
                    out,
                    collisions.as_deref_mut(),
                );
            }
        }
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

        if let Some(mesh) = extract_mesh(scene, shape, &world_transform) {
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

/// Check if a node name is an editor marker that should be skipped.
fn is_editor_marker(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    let lower = name.to_ascii_lowercase();
    lower.starts_with("editormarker")
        || lower.starts_with("marker_")
        || lower == "markerx"
        || lower.starts_with("marker:")
}
