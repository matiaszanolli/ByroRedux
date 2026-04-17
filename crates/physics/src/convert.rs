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

/// Convert an engine `CollisionShape` into one or more Rapier shape
/// parts, each with its parent-space isometry.
///
/// The shape is flattened — any `CollisionShape::Compound` tree is
/// walked depth-first and its leaves are emitted as individual parts.
/// Each part is either a primitive (Ball, Cuboid, Capsule, Cylinder,
/// ConvexHull) or a composite mesh (TriMesh).
///
/// Parry / Rapier forbid **composite-inside-compound** (TriMesh /
/// HeightField / Polyline / Compound). Returning a `Vec<(Isometry3,
/// SharedShape)>` instead of a single `SharedShape::compound` lets the
/// physics sync attach one `Collider` per part, which is the idiomatic
/// Rapier pattern and works for every valid mix of primitives and
/// meshes. See #373.
///
/// Mapping per variant:
/// - `Ball` / `Cuboid` / `Capsule` / `Cylinder` → 1 primitive part.
/// - `ConvexHull` → `SharedShape::convex_hull` (falls back to a tiny
///   ball if the hull is degenerate — Rapier rejects fewer than 4
///   non-coplanar points).
/// - `TriMesh` → `SharedShape::trimesh` (falls back to a tiny ball on
///   empty mesh or if trimesh construction fails).
/// - `Compound` → depth-first flatten, composing transforms.
///
/// An empty compound with no viable leaves emits a single tiny-ball
/// part so the caller can still register a collider.
pub fn collision_shape_to_parts(shape: &CollisionShape) -> Vec<(Isometry3<f32>, SharedShape)> {
    let mut out: Vec<(Isometry3<f32>, SharedShape)> = Vec::new();
    flatten_to_parts(shape, Isometry3::identity(), &mut out);
    if out.is_empty() {
        out.push((Isometry3::identity(), SharedShape::ball(1e-3)));
    }
    out
}

/// Recursively walk a `CollisionShape` tree, composing transforms as
/// we descend. Non-compound variants are emitted as a single part at
/// `parent_iso`; compounds recurse without emitting anything
/// themselves.
fn flatten_to_parts(
    shape: &CollisionShape,
    parent_iso: Isometry3<f32>,
    out: &mut Vec<(Isometry3<f32>, SharedShape)>,
) {
    match shape {
        CollisionShape::Compound { children } => {
            for (t, r, child) in children {
                let composed = parent_iso * iso_from_trs(*t, *r);
                flatten_to_parts(child, composed, out);
            }
        }
        CollisionShape::Ball { radius } => {
            out.push((parent_iso, SharedShape::ball((*radius).max(1e-3))));
        }
        CollisionShape::Cuboid { half_extents } => {
            out.push((
                parent_iso,
                SharedShape::cuboid(
                    half_extents.x.max(1e-3),
                    half_extents.y.max(1e-3),
                    half_extents.z.max(1e-3),
                ),
            ));
        }
        CollisionShape::Capsule {
            half_height,
            radius,
        } => {
            out.push((
                parent_iso,
                SharedShape::capsule_y((*half_height).max(1e-3), (*radius).max(1e-3)),
            ));
        }
        CollisionShape::Cylinder {
            half_height,
            radius,
        } => {
            out.push((
                parent_iso,
                SharedShape::cylinder((*half_height).max(1e-3), (*radius).max(1e-3)),
            ));
        }
        CollisionShape::ConvexHull { vertices } => {
            let pts: Vec<Point3<f32>> = vertices.iter().copied().map(vec3_to_point).collect();
            let shape = SharedShape::convex_hull(&pts).unwrap_or_else(|| {
                log::warn!(
                    "convex hull with {} pts rejected by Rapier; falling back to ball",
                    pts.len()
                );
                SharedShape::ball(1e-3)
            });
            out.push((parent_iso, shape));
        }
        CollisionShape::TriMesh { vertices, indices } => {
            if vertices.is_empty() || indices.is_empty() {
                out.push((parent_iso, SharedShape::ball(1e-3)));
                return;
            }
            let pts: Vec<Point3<f32>> = vertices.iter().copied().map(vec3_to_point).collect();
            let idx: Vec<[u32; 3]> = indices.clone();
            out.push((parent_iso, SharedShape::trimesh(pts, idx)));
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
        assert!(
            same || flipped,
            "quat roundtrip mismatch: {q:?} -> {back:?}"
        );
    }

    fn shape_type_of(parts: &[(Isometry3<f32>, SharedShape)], i: usize) -> ShapeType {
        parts[i].1.shape_type()
    }

    #[test]
    fn ball_maps_to_one_rapier_ball() {
        let parts = collision_shape_to_parts(&CollisionShape::Ball { radius: 2.0 });
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
        assert_eq!(parts[0].1.as_ball().unwrap().radius, 2.0);
        assert_eq!(parts[0].0.translation.vector, Vector3::zeros());
    }

    #[test]
    fn cuboid_maps_to_one_rapier_cuboid() {
        let parts = collision_shape_to_parts(&CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        });
        assert_eq!(parts.len(), 1);
        let he = parts[0].1.as_cuboid().unwrap().half_extents;
        assert_eq!((he.x, he.y, he.z), (1.0, 2.0, 3.0));
    }

    #[test]
    fn capsule_maps_to_one_rapier_capsule() {
        let parts = collision_shape_to_parts(&CollisionShape::Capsule {
            half_height: 5.0,
            radius: 1.5,
        });
        assert_eq!(parts.len(), 1);
        let c = parts[0].1.as_capsule().unwrap();
        assert_eq!(c.radius, 1.5);
        assert_eq!(c.half_height(), 5.0);
    }

    #[test]
    fn nested_compound_flattens_to_part_list() {
        // Oblivion bhkListShape chains commonly produce nested
        // CollisionShape::Compound. Parry panics
        // ("Nested composite shapes are not allowed") if we hand it a
        // Compound containing any CompositeShape — verify the new API
        // returns a flat Vec<(Iso, SharedShape)> with no nesting and
        // the expected composed isometries. See #373.
        let inner = CollisionShape::Compound {
            children: vec![
                (
                    Vec3::new(1.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Ball { radius: 0.5 }),
                ),
                (
                    Vec3::new(-1.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Ball { radius: 0.5 }),
                ),
            ],
        };
        let outer = CollisionShape::Compound {
            children: vec![(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, Box::new(inner))],
        };

        let parts = collision_shape_to_parts(&outer);
        assert_eq!(parts.len(), 2, "both inner balls should survive");
        for (_, s) in &parts {
            assert_ne!(
                s.shape_type(),
                ShapeType::Compound,
                "flatten_to_parts left a nested compound in place"
            );
        }
        let translations: Vec<_> = parts
            .iter()
            .map(|(iso, _)| (iso.translation.x, iso.translation.y, iso.translation.z))
            .collect();
        assert!(translations.contains(&(1.0, 2.0, 0.0)));
        assert!(translations.contains(&(-1.0, 2.0, 0.0)));
    }

    #[test]
    fn deeply_nested_compound_composes_transforms() {
        // 3 levels deep: Compound → Compound → Compound → Ball.
        // Each level adds +1 Y. Final translation must be +3.
        let level3 = CollisionShape::Compound {
            children: vec![(
                Vec3::new(0.0, 1.0, 0.0),
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius: 1.0 }),
            )],
        };
        let level2 = CollisionShape::Compound {
            children: vec![(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Box::new(level3))],
        };
        let level1 = CollisionShape::Compound {
            children: vec![(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Box::new(level2))],
        };
        let parts = collision_shape_to_parts(&level1);
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
        assert!((parts[0].0.translation.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn empty_nested_compound_falls_back_to_ball_part() {
        // A compound whose only child is an empty compound has no
        // viable leaves — the API emits a tiny-ball placeholder so the
        // caller can still register a collider rather than skipping.
        let inner = CollisionShape::Compound { children: vec![] };
        let outer = CollisionShape::Compound {
            children: vec![(Vec3::ZERO, Quat::IDENTITY, Box::new(inner))],
        };
        let parts = collision_shape_to_parts(&outer);
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
    }

    #[test]
    fn compound_mixing_trimesh_and_primitive_produces_two_parts() {
        // This is the case that tripped parry's Compound::new panic.
        // TriMesh is a CompositeShape; placing it inside a
        // SharedShape::compound used to fire
        // "Nested composite shapes are not allowed."
        // The new API returns a flat Vec — the caller (physics sync)
        // builds one collider per part, which Rapier permits.
        let mesh = CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            indices: vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        };
        let ball = CollisionShape::Ball { radius: 0.5 };
        let compound = CollisionShape::Compound {
            children: vec![
                (Vec3::ZERO, Quat::IDENTITY, Box::new(mesh)),
                (Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY, Box::new(ball)),
            ],
        };
        let parts = collision_shape_to_parts(&compound);
        assert_eq!(parts.len(), 2);
        let types: Vec<ShapeType> = parts.iter().map(|(_, s)| s.shape_type()).collect();
        assert!(types.contains(&ShapeType::TriMesh));
        assert!(types.contains(&ShapeType::Ball));
    }

    #[test]
    fn trimesh_preserves_vertex_count() {
        let verts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ];
        let idx = vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
        let parts = collision_shape_to_parts(&CollisionShape::TriMesh {
            vertices: verts,
            indices: idx,
        });
        assert_eq!(parts.len(), 1);
        let tm = parts[0].1.as_trimesh().unwrap();
        assert_eq!(tm.vertices().len(), 4);
        assert_eq!(tm.indices().len(), 4);
    }

    #[test]
    fn empty_trimesh_falls_back_to_ball_part() {
        let parts = collision_shape_to_parts(&CollisionShape::TriMesh {
            vertices: vec![],
            indices: vec![],
        });
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
    }
}
