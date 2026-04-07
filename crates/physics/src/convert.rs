//! glam ↔ nalgebra conversions and `CollisionShape` → Rapier shape mapping.
//!
//! Engine code speaks glam. Rapier speaks nalgebra. Keep the adapter
//! confined here so the rest of the crate never sees nalgebra types.

use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::math::{Quat, Vec3};
use nalgebra::{Isometry3, Point3, UnitQuaternion, Vector3};
use rapier3d::prelude::SharedShape;

// ── glam ↔ nalgebra ─────────────────────────────────────────────────────

#[inline]
pub fn vec3_to_na(v: Vec3) -> Vector3<f32> {
    Vector3::new(v.x, v.y, v.z)
}

#[inline]
pub fn vec3_to_point(v: Vec3) -> Point3<f32> {
    Point3::new(v.x, v.y, v.z)
}

#[inline]
pub fn vec3_from_na(v: Vector3<f32>) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[inline]
pub fn vec3_from_translation(t: nalgebra::Translation3<f32>) -> Vec3 {
    Vec3::new(t.x, t.y, t.z)
}

#[inline]
pub fn quat_to_na(q: Quat) -> UnitQuaternion<f32> {
    // glam stores quats as (x, y, z, w); nalgebra's Quaternion::new takes (w, i, j, k).
    UnitQuaternion::new_normalize(nalgebra::Quaternion::new(q.w, q.x, q.y, q.z))
}

#[inline]
pub fn quat_from_na(q: UnitQuaternion<f32>) -> Quat {
    let c = q.into_inner().coords; // (i, j, k, w)
    Quat::from_xyzw(c.x, c.y, c.z, c.w)
}

#[inline]
pub fn iso_from_trs(translation: Vec3, rotation: Quat) -> Isometry3<f32> {
    Isometry3::from_parts(vec3_to_na(translation).into(), quat_to_na(rotation))
}

// ── CollisionShape → Rapier ─────────────────────────────────────────────

/// Convert an engine `CollisionShape` into a Rapier `SharedShape`.
///
/// Mapping follows the doc comments on [`CollisionShape`]:
/// - `Ball` → `SharedShape::ball`
/// - `Cuboid` → `SharedShape::cuboid`
/// - `Capsule` → `SharedShape::capsule_y`
/// - `Cylinder` → `SharedShape::cylinder`
/// - `ConvexHull` → `SharedShape::convex_hull` (falls back to a tiny ball if
///   the hull is degenerate — Rapier rejects fewer than 4 non-coplanar points)
/// - `TriMesh` → `SharedShape::trimesh` (falls back to a tiny ball on empty
///   mesh or if trimesh construction fails — corrupt NIF collision data)
/// - `Compound` → `SharedShape::compound` with recursive child conversion.
///   Empty compounds fall back to a tiny ball.
pub fn collision_shape_to_shared_shape(shape: &CollisionShape) -> SharedShape {
    match shape {
        CollisionShape::Ball { radius } => SharedShape::ball((*radius).max(1e-3)),
        CollisionShape::Cuboid { half_extents } => SharedShape::cuboid(
            half_extents.x.max(1e-3),
            half_extents.y.max(1e-3),
            half_extents.z.max(1e-3),
        ),
        CollisionShape::Capsule {
            half_height,
            radius,
        } => SharedShape::capsule_y((*half_height).max(1e-3), (*radius).max(1e-3)),
        CollisionShape::Cylinder {
            half_height,
            radius,
        } => SharedShape::cylinder((*half_height).max(1e-3), (*radius).max(1e-3)),
        CollisionShape::ConvexHull { vertices } => {
            let pts: Vec<Point3<f32>> = vertices.iter().copied().map(vec3_to_point).collect();
            SharedShape::convex_hull(&pts).unwrap_or_else(|| {
                log::warn!(
                    "convex hull with {} pts rejected by Rapier; falling back to ball",
                    pts.len()
                );
                SharedShape::ball(1e-3)
            })
        }
        CollisionShape::TriMesh { vertices, indices } => {
            if vertices.is_empty() || indices.is_empty() {
                return SharedShape::ball(1e-3);
            }
            let pts: Vec<Point3<f32>> = vertices.iter().copied().map(vec3_to_point).collect();
            let idx: Vec<[u32; 3]> = indices.clone();
            SharedShape::trimesh(pts, idx)
        }
        CollisionShape::Compound { children } => {
            if children.is_empty() {
                return SharedShape::ball(1e-3);
            }
            let parts: Vec<(Isometry3<f32>, SharedShape)> = children
                .iter()
                .map(|(t, r, child)| {
                    (
                        iso_from_trs(*t, *r),
                        collision_shape_to_shared_shape(child),
                    )
                })
                .collect();
            SharedShape::compound(parts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::ShapeType;

    #[test]
    fn vec3_roundtrip() {
        let v = Vec3::new(1.0, -2.5, 3.25);
        assert_eq!(vec3_from_na(vec3_to_na(v)), v);
    }

    #[test]
    fn quat_roundtrip_identity() {
        let q = Quat::IDENTITY;
        let back = quat_from_na(quat_to_na(q));
        assert!((back.w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn quat_roundtrip_rotation() {
        let q = Quat::from_rotation_y(1.234);
        let back = quat_from_na(quat_to_na(q));
        // Component-wise with a loose tolerance (normalize may flip sign).
        let same = (back.x - q.x).abs() < 1e-5
            && (back.y - q.y).abs() < 1e-5
            && (back.z - q.z).abs() < 1e-5
            && (back.w - q.w).abs() < 1e-5;
        let flipped = (back.x + q.x).abs() < 1e-5
            && (back.y + q.y).abs() < 1e-5
            && (back.z + q.z).abs() < 1e-5
            && (back.w + q.w).abs() < 1e-5;
        assert!(same || flipped, "quat roundtrip mismatch: {q:?} -> {back:?}");
    }

    #[test]
    fn ball_maps_to_rapier_ball() {
        let s = collision_shape_to_shared_shape(&CollisionShape::Ball { radius: 2.0 });
        assert_eq!(s.shape_type(), ShapeType::Ball);
        assert_eq!(s.as_ball().unwrap().radius, 2.0);
    }

    #[test]
    fn cuboid_maps_to_rapier_cuboid() {
        let s = collision_shape_to_shared_shape(&CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        });
        assert_eq!(s.shape_type(), ShapeType::Cuboid);
        let he = s.as_cuboid().unwrap().half_extents;
        assert_eq!((he.x, he.y, he.z), (1.0, 2.0, 3.0));
    }

    #[test]
    fn capsule_maps_to_rapier_capsule() {
        let s = collision_shape_to_shared_shape(&CollisionShape::Capsule {
            half_height: 5.0,
            radius: 1.5,
        });
        assert_eq!(s.shape_type(), ShapeType::Capsule);
        let c = s.as_capsule().unwrap();
        assert_eq!(c.radius, 1.5);
        assert_eq!(c.half_height(), 5.0);
    }

    #[test]
    fn compound_recursively_maps_with_two_children() {
        let child_a = CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 1.0, 1.0),
        };
        let child_b = CollisionShape::Ball { radius: 0.5 };
        let compound = CollisionShape::Compound {
            children: vec![
                (Vec3::ZERO, Quat::IDENTITY, Box::new(child_a)),
                (Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, Box::new(child_b)),
            ],
        };
        let s = collision_shape_to_shared_shape(&compound);
        assert_eq!(s.shape_type(), ShapeType::Compound);
        assert_eq!(s.as_compound().unwrap().shapes().len(), 2);
    }

    #[test]
    fn trimesh_preserves_vertex_count() {
        // Small tetrahedron — enough for Rapier's BVH builder.
        let verts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let idx = vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
        let s = collision_shape_to_shared_shape(&CollisionShape::TriMesh {
            vertices: verts,
            indices: idx,
        });
        assert_eq!(s.shape_type(), ShapeType::TriMesh);
        let tm = s.as_trimesh().unwrap();
        assert_eq!(tm.vertices().len(), 4);
        assert_eq!(tm.indices().len(), 4);
    }

    #[test]
    fn empty_trimesh_falls_back_to_ball() {
        let s = collision_shape_to_shared_shape(&CollisionShape::TriMesh {
            vertices: vec![],
            indices: vec![],
        });
        assert_eq!(s.shape_type(), ShapeType::Ball);
    }
}
