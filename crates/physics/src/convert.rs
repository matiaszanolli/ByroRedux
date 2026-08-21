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
/// - `TriMesh` → `SharedShape::trimesh` (falls back to a tiny ball on an
///   empty vertex/index buffer, a non-finite vertex, or an index buffer
///   with no in-range triangle left after filtering). There is no
///   "construction failed" fallback and there cannot be one:
///   `SharedShape::trimesh_with_flags` returns `Self`, not a `Result`, and
///   `TriMesh::with_flags` *panics* on an empty index buffer and indexes
///   `vertices[idx]` unchecked thereafter — so every condition that would
///   "fail construction" has to be rejected before the call (#2878).
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
                debug_assert!(
                    t.is_finite() && r.is_finite(),
                    "canonical Compound child transform must be finite, got t={t:?} r={r:?}"
                );
                // #2862 — release-profile backstop: the debug_assert!
                // above is compiled out of release builds, so it can't
                // be the only guard. Every current producer of a
                // Compound child transform already rejects a non-finite
                // (t, r) at its own construction site (BhkTransformShape
                // — #2862), but this is the single choke point every
                // producer's output passes through before reaching
                // Rapier, so it holds even for a future producer that
                // forgets to. NaN/±Inf here would poison the composed
                // isometry and give Rapier's broadphase a NaN AABB for
                // the whole island. Neutralize to identity rather than
                // drop the child outright — same "clamp to a safe value,
                // don't discard the shape" posture as the Cuboid extent
                // clamp above (#2543).
                let safe_t = if t.is_finite() { *t } else { Vec3::ZERO };
                let safe_r = if r.is_finite() { *r } else { Quat::IDENTITY };
                let composed = parent_iso * iso_from_trs(safe_t * scale, safe_r);
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
            // #3066 / #2551 — classify the point set BEFORE parry sees it.
            // `convex_hull` has two distinct failure modes on untrusted NIF
            // data, and only one of them is recoverable by the caller:
            // a set with no extent at all unwraps an internal
            // `MissingSupportPoint` (a *panic*, not a `None`), while a merely
            // collinear set returns `None`. Coplanar sets are not a failure
            // mode at all — parry builds a flat polyhedron from them.
            let shape = match hull_degeneracy(&pts) {
                HullDegeneracy::Pointlike => {
                    log::warn!(
                        "convex hull with {} pts has no extent; falling back to ball",
                        pts.len()
                    );
                    SharedShape::ball(1e-3)
                }
                HullDegeneracy::Collinear { start, end } => {
                    // The authored extent is real even though the volume
                    // isn't: span it instead of collapsing to a millimetre
                    // ball the player walks straight through (#2551).
                    log::debug!(
                        "convex hull with {} pts is collinear; falling back to a \
                         capsule spanning its authored extent",
                        pts.len()
                    );
                    SharedShape::capsule(start, end, DEGENERATE_HULL_RADIUS)
                }
                HullDegeneracy::Buildable => {
                    SharedShape::convex_hull(&pts).unwrap_or_else(|| {
                        log::warn!(
                            "convex hull with {} pts rejected by Rapier; falling back to ball",
                            pts.len()
                        );
                        SharedShape::ball(1e-3)
                    })
                }
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
            // #2878 — the index-range guard belongs HERE, at the choke point
            // the comment above claims, not only in each producer. Both of
            // today's producers carry their own copy (`finish_trimesh`,
            // `synthesize_static_trimesh`), so this is defense-in-depth for
            // them — but a future producer that skips it would otherwise
            // panic *inside* `TriMesh::with_flags`' unchecked
            // `self.vertices[idx as usize]`, mid-frame in
            // `physics_sync_system`. `spawn_collision_shapes`' `catch_unwind`
            // does not cover that: it wraps only the shape `Clone`, and the
            // conversion happens later in the sync system.
            let vertex_count = pts.len() as u32;
            let idx: Vec<[u32; 3]> = indices
                .iter()
                .copied()
                .filter(|[a, b, c]| *a < vertex_count && *b < vertex_count && *c < vertex_count)
                .collect();
            // Filtering can empty a non-empty buffer — that reaches the same
            // `assert!` as an originally-empty one, so it takes the same
            // degrade-to-a-ball exit rather than the panic.
            if idx.is_empty() {
                out.push((parent_iso, SharedShape::ball(1e-3)));
                return;
            }
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

/// Radius given to a collision shape that has extent but no volume.
///
/// Matches the `ball(1e-3)` convention the no-volume fallbacks in this file
/// already use — a degenerate hull recovers its authored *extent*, which is
/// real data, and inherits its thickness from that existing convention
/// rather than from an invented number.
const DEGENERATE_HULL_RADIUS: f32 = 1e-3;

/// How degenerate a `ConvexHull` point set is, as far as `parry` cares.
enum HullDegeneracy {
    /// No extent — every point is (near) coincident. `parry` *panics* on
    /// these: `convex_hull` unwraps a `MissingSupportPoint`.
    Pointlike,
    /// Extent along one axis only. `parry` returns `None` for these.
    Collinear { start: Point3<f32>, end: Point3<f32> },
    /// Anything `convex_hull` can actually work with — including coplanar
    /// sets, which build a flat `ConvexPolyhedron` without complaint.
    Buildable,
}

/// Classify a `ConvexHull` point set ahead of `parry::convex_hull` (#2551).
///
/// Bethesda content authors `bhkConvexVerticesShape` blocks that collapse to
/// a line or a point (17 across the FO3 base+DLC corpus). Handing those to
/// `convex_hull` either panics the engine mid-frame or yields `None`, and the
/// old answer for the `None` case — `SharedShape::ball(1e-3)` — is a
/// millimetre ball at the shape origin, so an authored solid silently stopped
/// colliding anywhere near where it was placed.
///
/// Note this is *not* the "fewer than four non-coplanar points" precondition
/// parry documents: measured against parry 0.17.6, coplanar triangles, quads
/// and n-gons all build fine. Only extent is decisive.
fn hull_degeneracy(pts: &[Point3<f32>]) -> HullDegeneracy {
    let Some(origin) = pts.first().copied() else {
        return HullDegeneracy::Pointlike;
    };
    // Longest chord from `origin` fixes both the extent test and the axis the
    // collinearity test measures against. Taking the farthest point rather
    // than `pts[1]` keeps that axis well-conditioned when the first two
    // vertices happen to be near-coincident.
    let (mut far, mut span) = (origin, 0.0f32);
    for p in pts {
        let len = (p - origin).norm();
        if len > span {
            (far, span) = (*p, len);
        }
    }
    if span <= f32::EPSILON {
        return HullDegeneracy::Pointlike;
    }
    let axis = (far - origin) / span;
    // `|axis × d|` is the perpendicular distance from the chord. Scaling the
    // floor to the chord keeps the test size-independent: a 1 mm bow in a
    // 10 m shape is collinear, the same 1 mm in a 5 mm shape is not.
    let off_axis = pts
        .iter()
        .map(|p| axis.cross(&(p - origin)).norm())
        .fold(0.0f32, f32::max);
    if off_axis > span * 1e-4 {
        return HullDegeneracy::Buildable;
    }
    // Collinear: the segment runs between the two extremes along `axis`,
    // which `origin` and `far` are by construction only if `origin` is an
    // endpoint — it need not be, so take the real projection range.
    let (mut lo, mut hi) = (0.0f32, 0.0f32);
    for p in pts {
        let t = (p - origin).dot(&axis);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    HullDegeneracy::Collinear {
        start: origin + axis * lo,
        end: origin + axis * hi,
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

    /// Regression for #2862: `flatten_to_parts`'s `Compound` arm is the
    /// single choke point every collision producer's child transform
    /// passes through before reaching Rapier. In debug builds a
    /// non-finite (t, r) must trip the canonical-boundary assertion —
    /// same posture as the Cuboid extent guard above.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "canonical Compound child transform")]
    fn non_finite_compound_child_transform_trips_canonical_boundary_assertion() {
        let shape = CollisionShape::Compound {
            children: vec![(
                Vec3::new(f32::NAN, 0.0, 0.0),
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius: 0.5 }),
            )],
        };
        let _ = parts(&shape);
    }

    /// Regression for #2862: the release-build backstop. With
    /// `debug_assert!` compiled out, a non-finite Compound child
    /// translation/rotation (e.g. from `BhkTransformShape`'s corrupt-4x4
    /// case, or any future Compound producer) must be neutralized to
    /// identity rather than reaching Rapier's isometry composition —
    /// the exact scenario the bare `debug_assert!` alone couldn't guard
    /// in release. The child shape itself must still come through
    /// (neutralize the offset, don't drop the shape).
    #[cfg(not(debug_assertions))]
    #[test]
    fn non_finite_compound_child_transform_neutralizes_instead_of_reaching_rapier() {
        let shape = CollisionShape::Compound {
            children: vec![(
                Vec3::new(f32::NAN, f32::INFINITY, 0.0),
                Quat::from_xyzw(f32::NAN, 0.0, 0.0, f32::NAN),
                Box::new(CollisionShape::Ball { radius: 0.5 }),
            )],
        };
        let parts = parts(&shape);
        assert_eq!(parts.len(), 1, "the child ball must still come through");
        let iso = parts[0].0;
        assert!(
            iso.translation.vector.x.is_finite()
                && iso.translation.vector.y.is_finite()
                && iso.translation.vector.z.is_finite(),
            "non-finite compound child translation must not reach Rapier: {:?}",
            iso.translation
        );
        assert!(
            iso.rotation
                .into_inner()
                .coords
                .iter()
                .all(|c| c.is_finite()),
            "non-finite compound child rotation must not reach Rapier: {:?}",
            iso.rotation
        );
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

    /// Regression for #2877. The two tests above build every level with
    /// `Quat::IDENTITY`, and composition of pure translations **commutes** —
    /// so `child * parent` produces byte-identical results and both pass
    /// under a reversed/transposed compose. That is the exact failure this
    /// area is most exposed to ("collider in the wrong place, visually
    /// invisible, physically fatal"), and nothing in the crate exercised a
    /// non-identity rotation through a nested compound.
    ///
    /// A 90° parent yaw with an off-axis child offset makes the two orders
    /// disagree: the correct `parent * child` puts the child at
    /// `parent_rot * child_t + parent_t`, while `child * parent` would put it
    /// at `child_rot * parent_t + child_t` — a different point.
    #[test]
    fn rotated_compound_composes_parent_then_child() {
        use std::f32::consts::FRAC_PI_2;

        let parent_t = Vec3::new(0.0, 5.0, 0.0);
        let parent_r = Quat::from_rotation_y(FRAC_PI_2);
        let child_t = Vec3::new(4.0, 0.0, 0.0);

        let inner = CollisionShape::Compound {
            children: vec![(
                child_t,
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius: 1.0 }),
            )],
        };
        let outer = CollisionShape::Compound {
            children: vec![(parent_t, parent_r, Box::new(inner))],
        };

        let parts = parts(&outer);
        assert_eq!(parts.len(), 1);
        let actual = parts[0].0.translation.vector;

        // parent-then-child: a +90° yaw maps +X to −Z, so (4,0,0) lands at
        // (0,5,−4).
        let expected = parent_r * child_t + parent_t;
        assert!(
            (actual.x - expected.x).abs() < 1e-5
                && (actual.y - expected.y).abs() < 1e-5
                && (actual.z - expected.z).abs() < 1e-5,
            "compose must be parent_iso * child; expected {expected:?}, got {actual:?}"
        );

        // And it must NOT be the reversed product — pinned explicitly so the
        // assertion above can't be satisfied by an accidental symmetry.
        let reversed = Quat::IDENTITY * parent_t + child_t;
        let delta = Vec3::new(actual.x, actual.y, actual.z) - reversed;
        assert!(
            delta.length() > 1.0,
            "composed translation coincides with the reversed product \
             ({reversed:?}) — this test no longer distinguishes compose order"
        );

        // The child's own orientation is the parent's, since the child
        // authored identity: a reversed compose would leave it identity.
        let rot = parts[0].0.rotation;
        let expected_rot = quat_to_na(parent_r);
        assert!(
            (rot.i - expected_rot.i).abs() < 1e-5
                && (rot.j - expected_rot.j).abs() < 1e-5
                && (rot.k - expected_rot.k).abs() < 1e-5
                && (rot.w - expected_rot.w).abs() < 1e-5,
            "composed rotation must carry the parent yaw; got {rot:?}"
        );
    }

    /// Regression for #2878. The `#1779` comment calls the `TriMesh` arm
    /// "the single choke point every TriMesh source passes through", but the
    /// index-range guard lived only in each producer — so a shape reaching
    /// here with an out-of-range index panicked inside
    /// `TriMesh::with_flags`' unchecked `self.vertices[idx as usize]`,
    /// mid-frame in `physics_sync_system`. Out-of-range triangles are now
    /// filtered here; a mesh with nothing left degrades to the same tiny
    /// ball an originally-empty one gets.
    #[test]
    fn out_of_range_trimesh_indices_are_filtered_at_the_choke_point() {
        // Three vertices, two triangles: one valid, one referencing index 9.
        let mesh = CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            indices: vec![[0, 1, 2], [0, 1, 9]],
        };
        let parts = parts(&mesh);
        assert_eq!(parts.len(), 1);
        assert_eq!(
            shape_type_of(&parts, 0),
            ShapeType::TriMesh,
            "the valid triangle must survive as a real TriMesh"
        );
        assert_eq!(
            parts[0].1.as_trimesh().unwrap().indices().len(),
            1,
            "only the in-range triangle may reach Rapier"
        );
    }

    /// Companion: when *every* triangle is out of range the filtered index
    /// buffer is empty, which is precisely the input `TriMesh::with_flags`
    /// asserts on. It must take the tiny-ball exit, not panic.
    #[test]
    fn all_out_of_range_trimesh_degrades_to_a_ball_instead_of_panicking() {
        let mesh = CollisionShape::TriMesh {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
            indices: vec![[7, 8, 9]],
        };
        let parts = parts(&mesh);
        assert_eq!(parts.len(), 1);
        assert_eq!(shape_type_of(&parts, 0), ShapeType::Ball);
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

    /// Regression for #3066, corrected under #2551: parry panics when asked
    /// for a hull it cannot find a support point for. The PHYSAL boundary
    /// must reject that before entering the library — but "fewer than three
    /// points" was never the real precondition (a two-point set has an
    /// authored extent), so the guard is on extent, not count.
    #[test]
    fn extentless_convex_hulls_fall_back_to_a_ball_without_panicking() {
        for vertices in [
            vec![],
            vec![Vec3::ZERO],
            vec![Vec3::ZERO; 4],
            // Distinct floats, but closer together than parry can find a
            // support point across — the case that actually panicked.
            vec![Vec3::ZERO, Vec3::splat(f32::EPSILON * 0.5)],
        ] {
            let count = vertices.len();
            let shape = CollisionShape::ConvexHull { vertices };
            let parts = parts(&shape);
            assert_eq!(parts.len(), 1);
            assert_eq!(
                shape_type_of(&parts, 0),
                ShapeType::Ball,
                "{count}-point extentless hull must use the safe fallback"
            );
        }
    }

    /// Regression for #2551: a collinear convex vertex set still carries an
    /// authored *extent*. `convex_hull` returns `None` for these, and the old
    /// answer — `ball(1e-3)` at the shape origin — is a millimetre ball the
    /// player walks straight through, so 17 FO3 `bhkConvexVerticesShape`
    /// blocks silently stopped colliding. Span the extent instead.
    #[test]
    fn collinear_convex_hull_spans_its_authored_extent() {
        let shape = CollisionShape::ConvexHull {
            vertices: (0..5).map(|x| Vec3::new(x as f32 * 5.0, 0.0, 0.0)).collect(),
        };
        let parts = parts(&shape);
        assert_eq!(parts.len(), 1);
        assert_eq!(
            shape_type_of(&parts, 0),
            ShapeType::Capsule,
            "a collinear hull must span its extent, not collapse to a ball"
        );
        let aabb = parts[0].1.compute_local_aabb();
        assert!(
            (aabb.maxs.x - aabb.mins.x - 20.0).abs() < 1e-2,
            "the fallback must span the authored 20-unit segment, got {aabb:?}"
        );
    }

    /// The segment must take the same uniform scale every other shape gets —
    /// the classifier runs on already-scaled points, so this pins that it
    /// does not silently reconstruct at authored size.
    #[test]
    fn collinear_hull_fallback_respects_scale() {
        let shape = CollisionShape::ConvexHull {
            vertices: (0..5).map(|x| Vec3::new(x as f32 * 5.0, 0.0, 0.0)).collect(),
        };
        let parts = collision_shape_to_parts(&shape, 3.0, &ContactConfig::DEFAULT);
        let aabb = parts[0].1.compute_local_aabb();
        assert!(
            (aabb.maxs.x - aabb.mins.x - 60.0).abs() < 1e-2,
            "scaled segment must span the scaled extent, got {aabb:?}"
        );
    }

    /// The collinearity floor scales with the shape: a bow that is negligible
    /// on a 10-unit shape is a real hull on a millimetre one. Pins that the
    /// classifier is size-independent rather than using a bare epsilon.
    #[test]
    fn collinearity_floor_is_relative_to_shape_extent() {
        // 1e-4 of bow across a 10-unit span — an order of magnitude inside
        // the 1e-3 floor, so still a line.
        let flat = CollisionShape::ConvexHull {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(5.0, 1e-4, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
            ],
        };
        assert_eq!(shape_type_of(&parts(&flat), 0), ShapeType::Capsule);

        // The same absolute bow across a 0.01-unit span is 1% of the shape —
        // a hundred times the floor there, so a real (if thin) hull.
        let bowed = CollisionShape::ConvexHull {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.005, 1e-4, 0.0),
                Vec3::new(0.01, 0.0, 0.0),
            ],
        };
        assert_eq!(shape_type_of(&parts(&bowed), 0), ShapeType::ConvexPolyhedron);
    }

    /// Coplanar sets are NOT a parry failure mode — measured against parry
    /// 0.17.6, a flat quad builds a `ConvexPolyhedron` without complaint. The
    /// classifier must not divert them into a fallback (the premise #2551 was
    /// filed on; kept as a pin so a future parry bump that changes it fails
    /// here rather than silently degrading 
    /// every flat plate in the corpus).
    #[test]
    fn coplanar_hulls_still_build_a_real_polyhedron() {
        let plate = CollisionShape::ConvexHull {
            vertices: vec![
                Vec3::new(-10.0, 0.0, -10.0),
                Vec3::new(10.0, 0.0, -10.0),
                Vec3::new(10.0, 0.0, 10.0),
                Vec3::new(-10.0, 0.0, 10.0),
            ],
        };
        assert_eq!(
            shape_type_of(&parts(&plate), 0),
            ShapeType::ConvexPolyhedron,
            "coplanar input is buildable; diverting it would lose the real hull"
        );
    }

    /// A well-formed hull must still take the real `convex_hull` path — the
    /// fallbacks are only reachable through degeneracy.
    #[test]
    fn non_degenerate_hulls_still_build_a_convex_polyhedron() {
        let tetra = CollisionShape::ConvexHull {
            vertices: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(0.0, 10.0, 0.0),
                Vec3::new(0.0, 0.0, 10.0),
            ],
        };
        assert_eq!(shape_type_of(&parts(&tetra), 0), ShapeType::ConvexPolyhedron);
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
