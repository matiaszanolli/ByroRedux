//! Scene graph walking — hierarchical and flat traversal.

use crate::blocks::light::{NiAmbientLight, NiDirectionalLight, NiPointLight, NiSpotLight};
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
use super::{ImportedCollision, ImportedLight, ImportedMesh, ImportedNode, ImportedScene, LightKind};

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
            name: node.av.net.name.as_deref().map(str::to_string),
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
                walk_node_flat(scene, idx, &world_transform, out, collisions.as_deref_mut());
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

    if let Some(node) = block.as_any().downcast_ref::<NiNode>() {
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

/// Check if a node name is an editor marker that should be skipped.
fn is_editor_marker(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    let lower = name.to_ascii_lowercase();
    lower.starts_with("editormarker")
        || lower.starts_with("marker_")
        || lower == "markerx"
        || lower.starts_with("marker:")
}
