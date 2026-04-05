//! Collision data extraction — walks the bhk shape tree and produces ECS components.
//!
//! Pipeline: NiNode.collision_ref → bhkCollisionObject → bhkRigidBody → shape tree
//! → CollisionShape + RigidBodyData (physics-agnostic ECS components).
//!
//! Havok coordinates are scaled (typically ×7.0 for Oblivion, ×7.0 for FO3+)
//! and converted from Z-up to Y-up.

use crate::blocks::collision::*;
use crate::blocks::tri_shape::NiTriStripsData;
use crate::scene::NifScene;
use crate::types::BlockRef;

use byroredux_core::ecs::components::collision::{
    CollisionShape, MotionType, RigidBodyData,
};
use byroredux_core::math::{Quat, Vec3};

/// Default Havok-to-Gamebryo scale factor (7.0 for all Bethesda games).
const HAVOK_SCALE: f32 = 7.0;

/// Extract collision data from a NiAVObject's collision_ref.
///
/// Returns `(CollisionShape, RigidBodyData)` if the collision chain resolves.
/// The shape is in engine space (Y-up, Gamebryo units).
pub fn extract_collision(
    scene: &NifScene,
    collision_ref: BlockRef,
) -> Option<(CollisionShape, RigidBodyData)> {
    let coll_idx = collision_ref.index()?;
    let coll_obj = scene.get_as::<BhkCollisionObject>(coll_idx)?;
    let body_idx = coll_obj.body_ref.index()?;
    let body = scene.get_as::<BhkRigidBody>(body_idx)?;

    let shape = resolve_shape(scene, body.shape_ref)?;

    let motion_type = match body.motion_type {
        1 | 2 | 3 => MotionType::Dynamic,
        4 => MotionType::Keyframed,
        _ => MotionType::Static,
    };

    let body_data = RigidBodyData {
        motion_type,
        mass: body.mass,
        friction: body.friction,
        restitution: body.restitution,
        linear_damping: body.linear_damping,
        angular_damping: body.angular_damping,
    };

    Some((shape, body_data))
}

/// Recursively resolve a bhk shape block into a CollisionShape enum.
fn resolve_shape(scene: &NifScene, shape_ref: BlockRef) -> Option<CollisionShape> {
    let idx = shape_ref.index()?;
    let block = scene.get(idx)?;

    // Sphere
    if let Some(s) = block.as_any().downcast_ref::<BhkSphereShape>() {
        return Some(CollisionShape::Ball {
            radius: s.radius * HAVOK_SCALE,
        });
    }

    // Box
    if let Some(s) = block.as_any().downcast_ref::<BhkBoxShape>() {
        let [hx, hy, hz] = s.dimensions;
        return Some(CollisionShape::Cuboid {
            half_extents: havok_to_engine(hx, hy, hz) * HAVOK_SCALE,
        });
    }

    // Capsule
    if let Some(s) = block.as_any().downcast_ref::<BhkCapsuleShape>() {
        let p1 = havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * HAVOK_SCALE;
        let p2 = havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * HAVOK_SCALE;
        let half_height = (p2 - p1).length() * 0.5;
        let radius = s.radius1.max(s.radius2) * HAVOK_SCALE;
        return Some(CollisionShape::Capsule {
            half_height,
            radius,
        });
    }

    // Cylinder
    if let Some(s) = block.as_any().downcast_ref::<BhkCylinderShape>() {
        let p1 = havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * HAVOK_SCALE;
        let p2 = havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * HAVOK_SCALE;
        let half_height = (p2 - p1).length() * 0.5;
        let radius = s.cylinder_radius * HAVOK_SCALE;
        return Some(CollisionShape::Cylinder {
            half_height,
            radius,
        });
    }

    // Convex hull
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexVerticesShape>() {
        let verts: Vec<Vec3> = s
            .vertices
            .iter()
            .map(|v| havok_to_engine(v[0], v[1], v[2]) * HAVOK_SCALE)
            .collect();
        return Some(CollisionShape::ConvexHull { vertices: verts });
    }

    // MOPP BV tree — skip the MOPP data, recurse into the wrapped shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkMoppBvTreeShape>() {
        return resolve_shape(scene, s.shape_ref);
    }

    // List shape — compound of sub-shapes.
    if let Some(s) = block.as_any().downcast_ref::<BhkListShape>() {
        let mut children = Vec::with_capacity(s.sub_shape_refs.len());
        for sub_ref in &s.sub_shape_refs {
            if let Some(child) = resolve_shape(scene, *sub_ref) {
                children.push((Vec3::ZERO, Quat::IDENTITY, Box::new(child)));
            }
        }
        return if children.len() == 1 {
            // Unwrap single-child compound.
            let (_, _, shape) = children.into_iter().next().unwrap();
            Some(*shape)
        } else {
            Some(CollisionShape::Compound { children })
        };
    }

    // Transform shape — apply 4x4 transform to child shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkTransformShape>() {
        let child = resolve_shape(scene, s.shape_ref)?;
        let (translation, rotation) = decompose_havok_matrix(&s.transform);
        return Some(CollisionShape::Compound {
            children: vec![(translation, rotation, Box::new(child))],
        });
    }

    // NiTriStrips mesh collision — resolve referenced NiTriStripsData.
    if let Some(s) = block.as_any().downcast_ref::<BhkNiTriStripsShape>() {
        return resolve_tri_strips_collision(scene, s);
    }

    // Packed tri strips mesh collision.
    if let Some(s) = block.as_any().downcast_ref::<BhkPackedNiTriStripsShape>() {
        let data_idx = s.data_ref.index()?;
        let data = scene.get_as::<HkPackedNiTriStripsData>(data_idx)?;
        return resolve_packed_mesh(data);
    }

    // Phantom (trigger volume) — resolve inner shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkSimpleShapePhantom>() {
        return resolve_shape(scene, s.shape_ref);
    }

    log::debug!(
        "Unsupported collision shape type at block {}: {}",
        idx,
        block.block_type_name()
    );
    None
}

/// Convert bhkNiTriStripsShape into a TriMesh by merging all referenced NiTriStripsData.
fn resolve_tri_strips_collision(
    scene: &NifScene,
    shape: &BhkNiTriStripsShape,
) -> Option<CollisionShape> {
    let mut all_verts = Vec::new();
    let mut all_indices = Vec::new();

    for data_ref in &shape.data_refs {
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) else {
            continue;
        };

        let base_idx = all_verts.len() as u32;
        for v in &data.vertices {
            all_verts.push(havok_to_engine(v.x, v.y, v.z) * HAVOK_SCALE);
        }
        // Convert triangle strips to triangles.
        for strip in &data.strips {
            for i in 2..strip.len() {
                let (a, b, c) = if i % 2 == 0 {
                    (strip[i - 2], strip[i - 1], strip[i])
                } else {
                    (strip[i - 1], strip[i - 2], strip[i])
                };
                // Skip degenerate triangles.
                if a != b && b != c && a != c {
                    all_indices.push([
                        a as u32 + base_idx,
                        b as u32 + base_idx,
                        c as u32 + base_idx,
                    ]);
                }
            }
        }
    }

    if all_verts.is_empty() {
        return None;
    }

    Some(CollisionShape::TriMesh {
        vertices: all_verts,
        indices: all_indices,
    })
}

/// Convert hkPackedNiTriStripsData into a TriMesh.
fn resolve_packed_mesh(data: &HkPackedNiTriStripsData) -> Option<CollisionShape> {
    if data.vertices.is_empty() {
        return None;
    }

    let vertices: Vec<Vec3> = data
        .vertices
        .iter()
        .map(|v| havok_to_engine(v[0], v[1], v[2]) * HAVOK_SCALE)
        .collect();

    let indices: Vec<[u32; 3]> = data
        .triangles
        .iter()
        .map(|t| [t.v0 as u32, t.v1 as u32, t.v2 as u32])
        .collect();

    Some(CollisionShape::TriMesh { vertices, indices })
}

/// Convert Havok Z-up coordinates to engine Y-up: (x, z, -y).
fn havok_to_engine(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, z, -y)
}

/// Decompose a Havok 4x4 matrix into (translation, rotation) in engine space.
fn decompose_havok_matrix(m: &[[f32; 4]; 4]) -> (Vec3, Quat) {
    // Translation from column 3 (row-major: m[3][0..3]).
    let tx = m[3][0] * HAVOK_SCALE;
    let ty = m[3][1] * HAVOK_SCALE;
    let tz = m[3][2] * HAVOK_SCALE;
    let translation = havok_to_engine(tx, ty, tz);

    // Rotation from upper 3x3, converted to engine space.
    // Build a glam Mat3 from the Havok rotation, apply Z-up→Y-up basis change.
    let r00 = m[0][0];
    let r01 = m[0][1];
    let r02 = m[0][2];
    let r10 = m[1][0];
    let r11 = m[1][1];
    let r12 = m[1][2];
    let r20 = m[2][0];
    let r21 = m[2][1];
    let r22 = m[2][2];

    // Z-up → Y-up basis change: swap Y↔Z, negate new Z.
    // R_engine = P * R_havok * P^-1 where P swaps Y,Z and negates.
    let mat = byroredux_core::math::Mat3::from_cols(
        byroredux_core::math::Vec3::new(r00, r02, -r01),
        byroredux_core::math::Vec3::new(r20, r22, -r21),
        byroredux_core::math::Vec3::new(-r10, -r12, r11),
    );
    let rotation = Quat::from_mat3(&mat);

    (translation, rotation)
}
