//! glam ↔ nalgebra conversions and `CollisionShape` → Rapier shape mapping.
//!
//! Engine code speaks glam. Rapier speaks nalgebra. Keep the adapter
//! confined here so the rest of the crate never sees nalgebra types.

use crate::config::ContactConfig;
use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::math::{Quat, Vec3};
use nalgebra::{Isometry3, Point3, UnitQuaternion, Vector3};
use rapier3d::prelude::SharedShape;

/// Upper bound on a single shape-primitive extent/half-extent this crate
/// will hand to Rapier. Mirrors the magnitude of
/// `byroredux::cell_loader::references::RT_ABSOLUTE_PRECISION_CEILING`
/// (2^20, #1495) — this crate can't depend on the binary crate that
/// constant lives in, but the two exist for the same reason: a producer
/// upstream (NIF import, ESM-driven proxy synthesis) can hand this crate a
/// finite-but-corrupt magnitude, and nothing downstream should have to
/// trust it. #2543.
const MAX_SANE_SHAPE_EXTENT: f32 = 1_048_576.0;

/// Sanitise a caller-supplied uniform scale (#2860).
///
/// Mirrors `RagdollJointSpec::scaled_pivots`' `factor` closure so both
/// scale-consuming boundaries in this crate reject the same inputs the same
/// way: a non-finite or non-positive scale is meaningless for a collider
/// (zero collapses the shape, negative mirrors it) and resolves to `1.0`
/// rather than propagating into Rapier.
#[inline]
fn sanitize_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

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
///
/// `scale` is the uniform world scale the shape is placed at — a REFR's
/// `XSCL` composed with any node scale, i.e. `GlobalTransform::scale`
/// (#2860). Every emitted primitive dimension, vertex set **and** composed
/// child translation is multiplied by it, so a `2.0` placement produces a
/// collider twice the size *and* twice as far apart in its parts.
///
/// It is a required parameter rather than an `Option` or a field read
/// downstream precisely because it used to be dropped silently: Rapier's
/// body isometry carries translation and rotation only, so nothing after
/// this boundary can recover the scale, and a collider that is simply the
/// wrong size looks like correct code. Pass `1.0` when the shape is already
/// in world units. The engine's `Transform`/`GlobalTransform` scale is a
/// scalar `f32`, so the non-uniform case this guidance usually warns about
/// cannot arise here.
///
/// This makes the bhk path agree with the other two collider producers,
/// which already pre-bake scale: `synthesize_static_trimesh` multiplies
/// every vertex by `world_scale`, and `spawn_packed_havok_proxy` passes
/// `ref_scale` through (see `docs/engine/physics.md`).
///
/// `cfg` carries the engine-wide TriMesh flags. The default
/// (`ContactConfig::DEFAULT`) preserves the pre-unification behaviour
/// (`FIX_INTERNAL_EDGES`, which transitively ORs in `ORIENTED |
/// MERGE_DUPLICATE_VERTICES`); callers that need to override per-shape
/// can pass a `ContactConfig` with a different `trimesh_flags`.
pub fn collision_shape_to_parts(
    shape: &CollisionShape,
    scale: f32,
    cfg: &ContactConfig,
) -> Vec<(Isometry3<f32>, SharedShape)> {
    let mut out: Vec<(Isometry3<f32>, SharedShape)> = Vec::new();
    flatten_to_parts(
        shape,
        Isometry3::identity(),
        sanitize_scale(scale),
        cfg,
        &mut out,
    );
    if out.is_empty() {
        out.push((Isometry3::identity(), SharedShape::ball(1e-3)));
    }
    out
}

/// Recursively walk a `CollisionShape` tree, composing transforms as
/// we descend. Non-compound variants are emitted as a single part at
/// `parent_iso`; compounds recurse without emitting anything
/// themselves.
///
/// `scale` is already sanitised by [`collision_shape_to_parts`] and applies
/// uniformly at every depth. Scaling each child's *local* translation on the
/// way down is equivalent to scaling the fully-composed offset, because the
/// rotations in between are unchanged: `s·t₁ + R₁·(s·t₂) = s·(t₁ + R₁·t₂)`.
/// Doing it per-level keeps the leaf arms free of any depth bookkeeping.
///
/// Pre-#2860 the compound arm scaled *neither* — but `GlobalTransform::
/// compose_trs` on the producer side did scale each part's placement, so a
/// multi-part assembly on a scaled REFR had its parts spread apart while each
/// kept its authored size, opening literal gaps between adjacent colliders.
fn flatten_to_parts(
    shape: &CollisionShape,
    parent_iso: Isometry3<f32>,
    scale: f32,
    cfg: &ContactConfig,
    out: &mut Vec<(Isometry3<f32>, SharedShape)>,
) {
    match shape {
        CollisionShape::Compound { children } => {
            for (t, r, child) in children {
                let composed = parent_iso * iso_from_trs(*t * scale, *r);
                flatten_to_parts(child, composed, scale, cfg, out);
            }
        }
        CollisionShape::Ball { radius } => {
            out.push((parent_iso, SharedShape::ball((*radius * scale).max(1e-3))));
        }
        CollisionShape::Cuboid { half_extents } => {
            debug_assert!(
                half_extents.is_finite()
                    && half_extents.x >= 0.0
                    && half_extents.y >= 0.0
                    && half_extents.z >= 0.0,
                "canonical Cuboid half-extents must be finite non-negative magnitudes, got \
                 {half_extents:?}"
            );
            // #2543 — the debug_assert! above is compiled out of release
            // builds, so it can't be the only guard: a producer that
            // slips a non-finite or astronomically large half-extent
            // through (e.g. an unclamped REFR scale feeding
            // `synthesize_packed_havok_proxy`) would otherwise hand
            // Rapier's broad-phase an effectively-infinite AABB that
            // overlaps the entire scene. Replace non-finite lanes with
            // the same degenerate-avoidance floor the other arms already
            // use, and clamp the ceiling to Rapier's own extent limit so
            // a corrupt-but-finite input can't do the same thing.
            let clamp_lane = |v: f32| {
                if v.is_finite() {
                    v.clamp(1e-3, MAX_SANE_SHAPE_EXTENT)
                } else {
                    1e-3
                }
            };
            // Scale BEFORE the clamp so the #2543 sanity ceiling bounds the
            // value Rapier actually receives, not the pre-scale authored one.
            out.push((
                parent_iso,
                SharedShape::cuboid(
                    clamp_lane(half_extents.x * scale),
                    clamp_lane(half_extents.y * scale),
                    clamp_lane(half_extents.z * scale),
                ),
            ));
        }
        CollisionShape::Capsule {
            half_height,
            radius,
        } => {
            out.push((
                parent_iso,
                SharedShape::capsule_y(
                    (*half_height * scale).max(1e-3),
                    (*radius * scale).max(1e-3),
                ),
            ));
        }
        CollisionShape::Cylinder {
            half_height,
            radius,
        } => {
            out.push((
                parent_iso,
                SharedShape::cylinder(
                    (*half_height * scale).max(1e-3),
                    (*radius * scale).max(1e-3),
                ),
            ));
        }
        CollisionShape::ConvexHull { vertices } => {
            let pts: Vec<Point3<f32>> =
                vertices.iter().map(|v| vec3_to_point(*v * scale)).collect();
            let fallback = || {
                log::warn!(
                    "convex hull with {} pts rejected by Rapier; falling back to ball",
                    pts.len()
                );
                SharedShape::ball(1e-3)
            };
            // #3066 — parry panics rather than returning `None` below its
            // documented three-point precondition, so the fallback must guard
            // the call itself for untrusted NIF collision data.
            let shape = if pts.len() < 3 {
                fallback()
            } else {
                SharedShape::convex_hull(&pts).unwrap_or_else(fallback)
            };
            out.push((parent_iso, shape));
        }
        CollisionShape::TriMesh { vertices, indices } => {
            // #1779 — reject empty *or* non-finite meshes at the single choke
            // point every TriMesh source passes through (incl. the synth
            // fallback). A NaN/±Inf vertex makes `trimesh_with_flags` build a
            // Qbvh with NaN AABB bounds, corrupting the broadphase; fall back
            // to a tiny ball instead.
            if vertices.is_empty() || indices.is_empty() || vertices.iter().any(|v| !v.is_finite())
            {
                out.push((parent_iso, SharedShape::ball(1e-3)));
                return;
            }
            let pts: Vec<Point3<f32>> =
                vertices.iter().map(|v| vec3_to_point(*v * scale)).collect();
            let idx: Vec<[u32; 3]> = indices.clone();
            // M28.5 follow-up — TriMesh contact-normal treatment is
            // owned by `ContactConfig::trimesh_flags`. The default
            // (`FIX_INTERNAL_EDGES`, which transitively ORs in
            // `ORIENTED | MERGE_DUPLICATE_VERTICES`) fixes per-edge
            // normal flips at shared triangle seams — without it a
            // kinematic capsule sliding across a closed interior shell
            // can pick up a normal pointing the wrong way at the seam
            // and get pushed *through* the wall instead of along it.
            // See parry3d-0.17.6/src/shape/trimesh.rs:270-276 and the
            // pin tests in `crate::config`.
            use rapier3d::parry::shape::TriMeshFlags;
            let flags = TriMeshFlags::from_bits_truncate(cfg.trimesh_flags.0);
            out.push((parent_iso, SharedShape::trimesh_with_flags(pts, idx, flags)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::prelude::ShapeType;

    /// Test helper — call `collision_shape_to_parts` with the default
    /// `ContactConfig` so existing assertions stay shape-agnostic.
    fn parts(shape: &CollisionShape) -> Vec<(Isometry3<f32>, SharedShape)> {
        collision_shape_to_parts(shape, 1.0, &ContactConfig::DEFAULT)
    }

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
        let parts = parts(&CollisionShape::Ball { radius: 2.0 });
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
        assert_eq!(parts[0].1.as_ball().unwrap().radius, 2.0);
        assert_eq!(parts[0].0.translation.vector, Vector3::zeros());
    }

    #[test]
    fn cuboid_maps_to_one_rapier_cuboid() {
        let parts = parts(&CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        });
        assert_eq!(parts.len(), 1);
        let he = parts[0].1.as_cuboid().unwrap().half_extents;
        assert_eq!((he.x, he.y, he.z), (1.0, 2.0, 3.0));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "canonical Cuboid half-extents")]
    fn negative_cuboid_extent_trips_canonical_boundary_assertion() {
        let _ = parts(&CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, -3.0),
        });
    }

    /// Regression for #2543: an astronomically large but finite
    /// half-extent (e.g. from an unclamped upstream scale) must clamp to
    /// `MAX_SANE_SHAPE_EXTENT` rather than reaching Rapier unbounded.
    /// Non-negative and finite, so it passes the `debug_assert!` above and
    /// exercises the same clamp path in every build profile.
    #[test]
    fn huge_finite_cuboid_extent_clamps_to_sane_ceiling() {
        let parts = parts(&CollisionShape::Cuboid {
            half_extents: Vec3::splat(1.0e30),
        });
        let he = parts[0].1.as_cuboid().unwrap().half_extents;
        assert_eq!(
            (he.x, he.y, he.z),
            (
                MAX_SANE_SHAPE_EXTENT,
                MAX_SANE_SHAPE_EXTENT,
                MAX_SANE_SHAPE_EXTENT,
            )
        );
    }

    /// Regression for #2543: the release-build backstop. With
    /// `debug_assert!` compiled out, non-finite half-extents (`Infinity`/
    /// `NaN`) must still be replaced with the degenerate-avoidance floor
    /// instead of reaching Rapier's `cuboid` constructor as-is — this is
    /// the exact scenario the bare `debug_assert!` alone couldn't guard in
    /// release.
    #[cfg(not(debug_assertions))]
    #[test]
    fn non_finite_cuboid_extent_clamps_instead_of_reaching_rapier() {
        let parts = parts(&CollisionShape::Cuboid {
            half_extents: Vec3::new(f32::INFINITY, f32::NAN, f32::NEG_INFINITY),
        });
        let he = parts[0].1.as_cuboid().unwrap().half_extents;
        assert!(
            he.x.is_finite() && he.y.is_finite() && he.z.is_finite(),
            "non-finite half-extents must not reach Rapier: {he:?}"
        );
    }

    #[test]
    fn capsule_maps_to_one_rapier_capsule() {
        let parts = parts(&CollisionShape::Capsule {
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

        let parts = parts(&outer);
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
        let parts = parts(&level1);
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
        assert!((parts[0].0.translation.y - 3.0).abs() < 1e-6);
    }

    // ── #2860 — uniform placement scale must reach the collider ──────

    /// The headline regression: a `ref_scale = 2.0` cuboid must emit doubled
    /// half-extents. Pre-fix `GlobalTransform::scale` was read by nobody in
    /// this crate, so a 2× rock got a collider sized for the 1× mesh.
    #[test]
    fn scale_multiplies_cuboid_half_extents() {
        let shape = CollisionShape::Cuboid {
            half_extents: Vec3::new(1.0, 2.0, 3.0),
        };
        let scaled = collision_shape_to_parts(&shape, 2.0, &ContactConfig::DEFAULT);
        let he = scaled[0].1.as_cuboid().unwrap().half_extents;
        assert_eq!((he.x, he.y, he.z), (2.0, 4.0, 6.0));
    }

    /// Every primitive arm scales, not just the cuboid the issue named.
    #[test]
    fn scale_multiplies_every_primitive_dimension() {
        let cfg = &ContactConfig::DEFAULT;

        let ball = collision_shape_to_parts(&CollisionShape::Ball { radius: 2.0 }, 3.0, cfg);
        assert_eq!(ball[0].1.as_ball().unwrap().radius, 6.0);

        let capsule = collision_shape_to_parts(
            &CollisionShape::Capsule {
                half_height: 10.0,
                radius: 4.0,
            },
            0.5,
            cfg,
        );
        let c = capsule[0].1.as_capsule().unwrap();
        assert_eq!(c.radius, 2.0);
        assert!((c.half_height() - 5.0).abs() < 1e-6);

        let cylinder = collision_shape_to_parts(
            &CollisionShape::Cylinder {
                half_height: 10.0,
                radius: 4.0,
            },
            0.25,
            cfg,
        );
        let cy = cylinder[0].1.as_cylinder().unwrap();
        assert_eq!(cy.radius, 1.0);
        assert_eq!(cy.half_height, 2.5);
    }

    /// Vertex-set shapes scale too — a convex hull or trimesh collider on a
    /// scaled REFR must bound the scaled geometry, not the authored mesh.
    #[test]
    fn scale_multiplies_vertex_sets() {
        let cfg = &ContactConfig::DEFAULT;
        let hull = CollisionShape::ConvexHull {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(0.0, 10.0, 0.0),
                Vec3::new(0.0, 0.0, 10.0),
            ],
        };
        let parts = collision_shape_to_parts(&hull, 2.0, cfg);
        let aabb = parts[0].1.compute_local_aabb();
        assert!(
            (aabb.maxs.x - 20.0).abs() < 1e-4,
            "hull must span the scaled extent, got {:?}",
            aabb.maxs
        );

        let mesh = CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 10.0),
            ],
            indices: vec![[0, 1, 2]],
        };
        let parts = collision_shape_to_parts(&mesh, 3.0, cfg);
        let aabb = parts[0].1.compute_local_aabb();
        assert!(
            (aabb.maxs.x - 30.0).abs() < 1e-4,
            "trimesh must span the scaled extent, got {:?}",
            aabb.maxs
        );
    }

    /// Regression for #3066: parry panics when asked to construct a convex
    /// hull from fewer than three points. The PHYSAL boundary must reject the
    /// knowable bad precondition before entering the library.
    #[test]
    fn undersized_convex_hulls_fall_back_to_a_ball_without_panicking() {
        for count in 0..3 {
            let shape = CollisionShape::ConvexHull {
                vertices: (0..count).map(|x| Vec3::new(x as f32, 0.0, 0.0)).collect(),
            };
            let parts = parts(&shape);
            assert_eq!(parts.len(), 1);
            assert_eq!(
                shape_type_of(&parts, 0),
                ShapeType::Ball,
                "{count}-point hull must use the safe fallback"
            );
        }
    }

    /// The multi-part case the issue calls out as worse than the sizing bug:
    /// `compose_trs` already scaled each part's *placement* on the producer
    /// side, so leaving the shapes unscaled spread a scaled assembly apart
    /// while each part kept its authored size — opening gaps a KCC falls
    /// through. Size and separation must scale together.
    #[test]
    fn scale_moves_and_resizes_compound_parts_together() {
        let compound = CollisionShape::Compound {
            children: vec![
                (
                    Vec3::new(0.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Cuboid {
                        half_extents: Vec3::splat(10.0),
                    }),
                ),
                (
                    // Placed exactly touching the first box.
                    Vec3::new(20.0, 0.0, 0.0),
                    Quat::IDENTITY,
                    Box::new(CollisionShape::Cuboid {
                        half_extents: Vec3::splat(10.0),
                    }),
                ),
            ],
        };
        let parts = collision_shape_to_parts(&compound, 2.0, &ContactConfig::DEFAULT);
        assert_eq!(parts.len(), 2);

        let half = parts[0].1.as_cuboid().unwrap().half_extents.x;
        assert_eq!(half, 20.0, "part half-extent must scale");
        let separation = parts[1].0.translation.x - parts[0].0.translation.x;
        assert_eq!(separation, 40.0, "part separation must scale");
        assert_eq!(
            separation,
            2.0 * half,
            "the two boxes must still touch after scaling — any mismatch is \
             the gap dynamic bodies fall through"
        );
    }

    /// Depth doesn't compound the scale: applying it per level is equivalent
    /// to scaling the fully-composed offset, so a 3-deep chain of +1 Y at
    /// scale 2 lands at +6, not +8.
    #[test]
    fn scale_applies_once_across_nesting_depth() {
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
        let parts = collision_shape_to_parts(&level1, 2.0, &ContactConfig::DEFAULT);
        assert!((parts[0].0.translation.y - 6.0).abs() < 1e-6);
    }

    /// A rotated child proves the per-level scaling is a true uniform scale
    /// and not a translation hack: `s·t₁ + R₁·(s·t₂) == s·(t₁ + R₁·t₂)`.
    #[test]
    fn scale_commutes_with_rotation_in_a_compound() {
        // Inner ball offset +10 X, rotated a quarter turn about Y at the
        // outer level so that offset becomes -10 Z (or +10 Z, sign per
        // convention) — either way the magnitude must be 10 × scale.
        let inner = CollisionShape::Compound {
            children: vec![(
                Vec3::new(10.0, 0.0, 0.0),
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius: 1.0 }),
            )],
        };
        let outer = CollisionShape::Compound {
            children: vec![(
                Vec3::ZERO,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                Box::new(inner),
            )],
        };
        let parts = collision_shape_to_parts(&outer, 3.0, &ContactConfig::DEFAULT);
        let t = parts[0].0.translation.vector;
        assert!(
            (t.norm() - 30.0).abs() < 1e-4,
            "rotated child must sit at 10 × 3 from the origin, got {t:?}"
        );
    }

    /// A meaningless scale must not reach Rapier: zero would collapse the
    /// shape, negative would mirror it, NaN would poison the broad-phase.
    #[test]
    fn degenerate_scales_fall_back_to_unity() {
        let shape = CollisionShape::Cuboid {
            half_extents: Vec3::splat(5.0),
        };
        for bad in [0.0_f32, -2.0, f32::NAN, f32::INFINITY] {
            let parts = collision_shape_to_parts(&shape, bad, &ContactConfig::DEFAULT);
            let he = parts[0].1.as_cuboid().unwrap().half_extents;
            assert_eq!(
                (he.x, he.y, he.z),
                (5.0, 5.0, 5.0),
                "scale {bad} must resolve to 1.0, not reach Rapier"
            );
        }
    }

    /// #2543's sanity ceiling must bound the value Rapier receives, so a
    /// corrupt scale on a large authored extent cannot slip past it.
    #[test]
    fn scale_is_applied_before_the_sanity_ceiling() {
        let shape = CollisionShape::Cuboid {
            half_extents: Vec3::splat(MAX_SANE_SHAPE_EXTENT * 0.75),
        };
        let parts = collision_shape_to_parts(&shape, 1000.0, &ContactConfig::DEFAULT);
        let he = parts[0].1.as_cuboid().unwrap().half_extents;
        assert_eq!(he.x, MAX_SANE_SHAPE_EXTENT);
    }

    /// Unit scale must be byte-identical to the pre-#2860 output, so the
    /// overwhelmingly common unscaled placement is provably untouched.
    #[test]
    fn unit_scale_is_a_no_op() {
        let shape = CollisionShape::Compound {
            children: vec![(
                Vec3::new(1.0, 2.0, 3.0),
                Quat::from_rotation_z(0.5),
                Box::new(CollisionShape::Capsule {
                    half_height: 7.0,
                    radius: 2.0,
                }),
            )],
        };
        let parts = collision_shape_to_parts(&shape, 1.0, &ContactConfig::DEFAULT);
        let c = parts[0].1.as_capsule().unwrap();
        assert_eq!(c.radius, 2.0);
        assert!((c.half_height() - 7.0).abs() < 1e-6);
        assert_eq!(parts[0].0.translation.vector, Vector3::new(1.0, 2.0, 3.0));
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
        let parts = parts(&outer);
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
        let parts = parts(&compound);
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
        let parts = parts(&CollisionShape::TriMesh {
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
        let parts = parts(&CollisionShape::TriMesh {
            vertices: vec![],
            indices: vec![],
        });
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
    }

    #[test]
    fn nonfinite_trimesh_falls_back_to_ball_part() {
        // #1779 — a non-finite vertex must not reach `trimesh_with_flags`
        // (it would build a Qbvh with NaN AABB bounds); the choke point
        // drops it to a tiny ball, same as the empty case.
        let parts = parts(&CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(f32::NAN, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            indices: vec![[0, 1, 2]],
        });
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
    }
}
