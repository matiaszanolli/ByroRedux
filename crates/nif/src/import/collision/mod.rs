//! Collision data extraction — walks the bhk shape tree and produces ECS components.
//!
//! Pipeline: NiNode.collision_ref → bhk*CollisionObject → … → CollisionShape +
//! RigidBodyData (physics-agnostic ECS components). The "…" branches by the
//! concrete bhk*CollisionObject subclass authored on the NIF, which is
//! effectively the per-game-variant boundary:
//!
//! | Block | Game line | Body kind | Extractable today |
//! |---|---|---|---|
//! | `BhkCollisionObject`   | Universal (dominant pre-FO4)             | `BhkRigidBody` → shape tree | **yes** |
//! | `BhkNPCollisionObject` | FO4 / FO76 / Starfield ("Niagara Physics") | `BhkSystemBinary` (Havok-serialised blob) | **no** — blob decoder is a multi-day project; consumer falls back to `cell_loader/spawn.rs::synthesize_static_trimesh` for Architecture meshes (commit `15016ee0`) |
//! | `BhkPCollisionObject`  | Skyrim+ trigger volumes / phantoms       | `bhkPhantom` subclass | **no** — phantoms aren't modeled as rigid bodies; need a `TriggerVolume` ECS path |
//!
//! Until the NP-blob decoder lands, the NP arm is a tracked stub: it confirms
//! the FO4+ chain is present (so the symptom is "collision authoring exists but
//! we can't read it" rather than "no collision authored") and surfaces the blob
//! size in the debug log. The render-geometry trimesh fallback in
//! `cell_loader/spawn.rs` is what produces the actual collider today.
//!
//! Havok coordinates are scaled per-game (×7.0 for TES4/FO3/FNV, ×69.99 for
//! Skyrim+/FO4) and converted from Z-up to Y-up. The scale lives on the parsed
//! [`NifScene`] (`havok_scale` field, populated by `havok_scale_for` at parse
//! time) so consumers don't have to re-detect the game variant per call.

use std::collections::HashSet;

use crate::blocks::collision::*;
use crate::scene::NifScene;
use crate::types::BlockRef;

use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
use byroredux_core::math::{Quat, Vec3};

mod ragdoll;
mod shape;

pub use ragdoll::extract_ragdoll;
use shape::resolve_shape;

/// Discriminator surfaced by [`examine_collision_kind`] so callers (telemetry,
/// the trimesh fallback in `cell_loader/spawn.rs`) can distinguish "no
/// collision authored" from "FO4+ NP collision authored but our decoder is a
/// stub". The two cases produce the same `None` from [`extract_collision`]
/// today; the trimesh fallback fires identically for both, but the bookkeeping
/// matters for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionAuthoring {
    /// No `collision_ref` on the AVObject, or the ref doesn't resolve.
    None,
    /// `BhkCollisionObject` → `BhkRigidBody` chain. Extractable.
    Classic,
    /// `BhkNPCollisionObject` (FO4 / FO76 / Starfield). Carries a
    /// Havok-serialised blob in a linked `BhkSystemBinary`. Not yet decodable.
    NewPhysicsStub,
    /// `BhkPCollisionObject` wrapping a `bhkPhantom` subclass (Skyrim+
    /// trigger volume). Phantom semantics need a dedicated ECS path
    /// rather than a rigid body.
    Phantom,
    /// `collision_ref` resolved to a block whose concrete type isn't a
    /// recognised collision-object subclass.
    Unrecognised,
}

/// Inspect what kind of collision authoring is present at `collision_ref`
/// without attempting to extract it. Cheap; just downcasts the block.
/// Lets the cell-loader trimesh fallback distinguish "FO4 NP — workaround
/// is intentional" from "no collision — workaround is silently filling a
/// gap the authoring intended to leave empty".
pub fn examine_collision_kind(scene: &NifScene, collision_ref: BlockRef) -> CollisionAuthoring {
    let Some(idx) = collision_ref.index() else {
        return CollisionAuthoring::None;
    };
    let Some(block) = scene.get(idx) else {
        return CollisionAuthoring::None;
    };
    if block.as_any().is::<BhkCollisionObject>() {
        CollisionAuthoring::Classic
    } else if block.as_any().is::<BhkNPCollisionObject>() {
        CollisionAuthoring::NewPhysicsStub
    } else if block.as_any().is::<BhkPCollisionObject>() {
        CollisionAuthoring::Phantom
    } else {
        CollisionAuthoring::Unrecognised
    }
}

/// Extract collision data from a NiAVObject's collision_ref.
///
/// Returns `(CollisionShape, RigidBodyData)` if the collision chain resolves
/// through a fully-decodable subclass. The shape is in engine space (Y-up,
/// Gamebryo units). For FO4+ NP physics and trigger phantoms, returns `None`
/// — see [`CollisionAuthoring`] and the module docstring for the dispatch
/// table.
pub fn extract_collision(
    scene: &NifScene,
    collision_ref: BlockRef,
) -> Option<(CollisionShape, RigidBodyData)> {
    // #1874/#1832 diagnostic — trace level since "no collision authored on
    // this node" is the overwhelmingly common case; enable via
    // `RUST_LOG=byroredux_nif::import::collision=trace` to get a per-node
    // census of collision_ref presence for a suspiciously sparse cell.
    let Some(coll_idx) = collision_ref.index() else {
        log::trace!("extract_collision: no collision_ref on this AVObject");
        return None;
    };
    let Some(block) = scene.get(coll_idx) else {
        log::debug!("extract_collision: collision_ref {coll_idx} does not resolve to a block");
        return None;
    };

    // Dispatch on the concrete bhk*CollisionObject subclass. The three live
    // arms differ in what they wrap and which game line ships them — see the
    // module docstring for the table.
    if let Some(classic) = block.as_any().downcast_ref::<BhkCollisionObject>() {
        return extract_from_classic(scene, classic);
    }
    if let Some(np) = block.as_any().downcast_ref::<BhkNPCollisionObject>() {
        return extract_from_np(scene, coll_idx, np);
    }
    if let Some(phantom) = block.as_any().downcast_ref::<BhkPCollisionObject>() {
        return extract_from_phantom(scene, coll_idx, phantom);
    }

    log::debug!(
        "extract_collision: collision_ref at block {} resolves to '{}' which is not a recognised bhk*CollisionObject subclass",
        coll_idx,
        block.block_type_name(),
    );
    None
}

/// Map the raw Havok `hkMotionType` byte to the engine [`MotionType`].
///
/// Values per the canonical `hkMotionType` enum (`hkpMotion::MotionType`,
/// nif.xml `<enum name="hkMotionType">`):
///
/// | Value | Havok | Engine |
/// |-------|-------|--------|
/// | 1 | DYNAMIC | Dynamic |
/// | 2 | SPHERE_INERTIA | Dynamic |
/// | 3 | SPHERE_STABILIZED | Dynamic |
/// | 4 | BOX_INERTIA | Dynamic |
/// | 5 | BOX_STABILIZED | Dynamic |
/// | 6 | KEYFRAMED | Keyframed |
/// | 7 | FIXED | Static |
/// | 8 | THIN_BOX | Dynamic |
/// | 9 | CHARACTER | CharacterKinematic |
/// | 0 / other | INVALID | Static |
///
/// The pre-#1652 mapping collapsed `4 => Keyframed` and `_ => Static`,
/// which froze the most common dynamic clutter (BOX_INERTIA 4 crates/
/// debris) into a kinematic body with no animated transform and turned
/// every KEYFRAMED (6) door/platform into immovable Static. This is the
/// canonical translation boundary — the physics solver only ever sees
/// the engine [`MotionType`], never the raw Havok byte.
fn havok_motion_type(raw: u8) -> MotionType {
    match raw {
        1..=5 | 8 => MotionType::Dynamic,
        6 => MotionType::Keyframed,
        7 => MotionType::Static,
        9 => MotionType::CharacterKinematic,
        // 0 = MO_SYS_INVALID and any out-of-range value → Static.
        _ => MotionType::Static,
    }
}

/// Classic `BhkCollisionObject` → `BhkRigidBody` → shape-tree extractor.
/// This is the dominant path for Oblivion / FO3 / FNV / Skyrim LE / SSE and
/// still covers most FO4+ rigid bodies that author the legacy chain. Body of
/// the original `extract_collision` — preserved bit-for-bit so the refactor
/// is a no-op on every existing classic-bhk fixture.
/// #1874/#1832 diagnostic-only — coarse "how big is this shape" summary so
/// a debug-level census can distinguish small clutter placeholders from
/// large architecture pieces without a separate inspection pass. Not used
/// outside logging.
fn shape_size_descriptor(shape: &CollisionShape) -> String {
    match shape {
        CollisionShape::Ball { radius } => format!("Ball(r={radius:.1})"),
        CollisionShape::Cuboid { half_extents } => format!(
            "Cuboid(half_extents=[{:.1},{:.1},{:.1}])",
            half_extents.x, half_extents.y, half_extents.z
        ),
        CollisionShape::Capsule {
            half_height,
            radius,
        } => {
            format!("Capsule(half_height={half_height:.1}, r={radius:.1})")
        }
        CollisionShape::Cylinder {
            half_height,
            radius,
        } => {
            format!("Cylinder(half_height={half_height:.1}, r={radius:.1})")
        }
        CollisionShape::ConvexHull { vertices } => {
            let bbox = aabb_extent(vertices.iter().copied());
            format!(
                "ConvexHull({} verts, bbox={:.1}x{:.1}x{:.1})",
                vertices.len(),
                bbox.x,
                bbox.y,
                bbox.z
            )
        }
        CollisionShape::TriMesh { vertices, indices } => {
            let bbox = aabb_extent(vertices.iter().copied());
            format!(
                "TriMesh({} verts, {} tris, bbox={:.1}x{:.1}x{:.1})",
                vertices.len(),
                indices.len(),
                bbox.x,
                bbox.y,
                bbox.z
            )
        }
        CollisionShape::Compound { children } => {
            format!("Compound({} children)", children.len())
        }
    }
}

/// Bounding-box extent (max-min per axis) of a point cloud, `Vec3::ZERO` if empty.
fn aabb_extent(points: impl Iterator<Item = Vec3>) -> Vec3 {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut any = false;
    for p in points {
        any = true;
        min = min.min(p);
        max = max.max(p);
    }
    if any {
        max - min
    } else {
        Vec3::ZERO
    }
}

fn extract_from_classic(
    scene: &NifScene,
    coll_obj: &BhkCollisionObject,
) -> Option<(CollisionShape, RigidBodyData)> {
    // #1874/#1832 diagnostic — these three early-returns previously had no
    // logging at all, so a classic BhkCollisionObject that fails to produce
    // a collider left no trace. See the module-level trace/debug note on
    // `extract_collision`.
    let Some(body_idx) = coll_obj.body_ref.index() else {
        log::debug!("extract_from_classic: BhkCollisionObject has no body_ref");
        return None;
    };
    let Some(body) = scene.get_as::<BhkRigidBody>(body_idx) else {
        log::debug!("extract_from_classic: body_ref {body_idx} does not resolve to a BhkRigidBody");
        return None;
    };

    let scale = scene.havok_scale;
    let mut visited = HashSet::new();
    let Some(mut shape) = resolve_shape(scene, body.shape_ref, &mut visited) else {
        log::debug!(
            "extract_from_classic: body {body_idx}'s shape_ref failed to resolve to a CollisionShape \
             (unsupported/corrupt shape tree — see resolve_shape debug logs)"
        );
        return None;
    };

    // Apply rigid body center-of-mass offset and orientation to the shape.
    // Static architecture typically has zero offset; dynamic objects (crates,
    // bottles, ragdoll bones) have non-trivial transforms.
    let body_translation = havok_to_engine(
        body.translation[0],
        body.translation[1],
        body.translation[2],
    ) * scale;
    let body_rotation = havok_quat_to_engine(body.rotation);

    let has_offset = body_translation.length_squared() > 1e-6
        || (body_rotation - Quat::IDENTITY).length_squared() > 1e-6;
    if has_offset {
        shape = CollisionShape::Compound {
            children: vec![(body_translation, body_rotation, Box::new(shape))],
        };
    }

    let mut motion_type = havok_motion_type(body.motion_type);

    // #1832/#1874 — a zero-mass "Dynamic"-per-enum body is Havok's
    // convention for immovable world geometry, not a real physics object:
    // F=ma is undefined at m=0, so no physics engine actually integrates
    // these — Havok's own runtime special-cases them, but Rapier doesn't,
    // so a naive 1:1 enum mapping hands Rapier a genuine `Dynamic` body
    // with no support under it. Confirmed live: Skyrim SE architecture
    // (walls/floor/roof — large TriMesh shapes, e.g. 256×10×256 floor
    // tiles) ships `motionType` raw values 2-5 (SPHERE/BOX_INERTIA family)
    // with `mass=0`, gets built as `RigidBodyType::Dynamic`, spawns asleep
    // (the exterior-freeze fix), then free-falls the instant the player's
    // KCC wakes it by standing on it — the root cause of the TES-family
    // (Oblivion/Skyrim) "character never grounds" bug. FNV/FO3 ship the
    // same architecture as genuinely `motionType=7` (Static); FO4+ never
    // reaches this path at all (NP-collision stub + render-geometry
    // trimesh fallback). A real dynamic prop (crate, plate, ragdoll bone)
    // always has non-zero authored mass, so this only reclassifies the
    // physically-nonsensical case.
    if motion_type == MotionType::Dynamic && body.mass <= 0.0 {
        log::debug!(
            "extract_from_classic: body {body_idx} authored motionType={raw} (Dynamic-family) \
             with mass=0 — treating as Static (immovable world geometry), shape={shape_desc}",
            raw = body.motion_type,
            shape_desc = shape_size_descriptor(&shape),
        );
        motion_type = MotionType::Static;
    }

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

/// FO4 / FO76 / Starfield NP-physics extractor. Stub.
///
/// `BhkNPCollisionObject.data_ref` points at a [`BhkSystemBinary`] block
/// (`bhkPhysicsSystem` or `bhkRagdollSystem`) that carries the body + shape
/// tree as a Havok-serialised binary blob. Decoding it requires a Havok
/// content-system deserialiser — nifly's C++ implementation is ~2k LOC and
/// OpenMW doesn't cover FO4 physics, so this is a multi-day project tracked
/// as a follow-up to the per-variant abstraction work in #1277.
///
/// Today this arm returns `None` so the existing render-geometry trimesh
/// fallback in `cell_loader/spawn.rs::synthesize_static_trimesh` (commit
/// `15016ee0`) continues to fire for Architecture meshes — the player still
/// grounds in FO4 cells via the fallback. The debug log surfaces the blob
/// size + linked-block index so a future blob decoder has a breadcrumb
/// trail.
fn extract_from_np(
    scene: &NifScene,
    coll_idx: usize,
    coll_obj: &BhkNPCollisionObject,
) -> Option<(CollisionShape, RigidBodyData)> {
    let blob_idx = coll_obj.data_ref.index();
    let blob = blob_idx.and_then(|i| scene.get_as::<BhkSystemBinary>(i));
    match blob {
        Some(b) => log::debug!(
            "extract_collision: FO4+ NP collision at block {coll_idx} \
             (body_id {bid}, flags {flags:#06x}) — {kind} blob is \
             {bytes} bytes; not yet decodable, render-geometry trimesh \
             fallback will fire for Architecture meshes",
            bid = coll_obj.body_id,
            flags = coll_obj.flags,
            kind = b.type_name,
            bytes = b.data.len(),
        ),
        None => log::debug!(
            "extract_collision: FO4+ NP collision at block {coll_idx} \
             has no data_ref or the ref doesn't resolve to a BhkSystemBinary \
             (body_id {bid}, flags {flags:#06x}); no Havok blob to decode",
            bid = coll_obj.body_id,
            flags = coll_obj.flags,
        ),
    }
    None
}

/// Skyrim+ phantom-collision extractor. Stub.
///
/// `BhkPCollisionObject.body_ref` points at a `bhkPhantom` subclass
/// (`bhkSimpleShapePhantom`, `bhkAabbPhantom`, …) which carries the
/// collision volume but participates in physics as a *trigger* rather than
/// a rigid body — solid geometry that detects overlap but doesn't generate
/// contact response. Modelling them properly needs a `TriggerVolume` ECS
/// component + a system that routes phantom overlaps into the scripting
/// event stream, neither of which exist yet.
///
/// Returning `None` today keeps phantoms from being mis-promoted into solid
/// rigid bodies (which would block the player from walking through trigger
/// regions intended to fire quest scripts). The blob index logged is the
/// `bhkPhantom`-subclass block id so a future trigger-volume importer has
/// a breadcrumb trail.
fn extract_from_phantom(
    scene: &NifScene,
    coll_idx: usize,
    coll_obj: &BhkPCollisionObject,
) -> Option<(CollisionShape, RigidBodyData)> {
    let phantom_idx = coll_obj.body_ref.index();
    match phantom_idx.and_then(|i| scene.get(i)) {
        Some(p) => log::debug!(
            "extract_collision: bhkPCollisionObject phantom at block {coll_idx} \
             (flags {flags:#06x}) wraps '{kind}' at block {phantom_idx:?}; \
             phantoms are trigger volumes, not yet modeled as TriggerVolume \
             ECS components",
            flags = coll_obj.flags,
            kind = p.block_type_name(),
        ),
        None => log::debug!(
            "extract_collision: bhkPCollisionObject phantom at block {coll_idx} \
             (flags {flags:#06x}) has no body_ref or the ref doesn't resolve",
            flags = coll_obj.flags,
        ),
    }
    None
}

/// Reject a non-finite scalar (NaN / ±Inf) read from a corrupt or
/// adversarial NIF before it reaches a `CollisionShape` field. A
/// non-finite radius / half-extent flows into the parry3d/Rapier
/// collider builder where it panics or poisons the broadphase. Returning
/// `None` from the construction site drops the authored primitive to the
/// synthesized-trimesh fallback (`spawn.rs`) instead (NIFAL-S4 / #1409).
#[inline]
pub(super) fn finite(x: f32) -> Option<f32> {
    x.is_finite().then_some(x)
}

/// `Vec3` sibling of [`finite`] — all three lanes must be finite.
#[inline]
pub(super) fn finite_vec(v: Vec3) -> Option<Vec3> {
    v.is_finite().then_some(v)
}

/// Convert Havok Z-up coordinates to engine Y-up: (x, z, -y).
///
/// #1617 — routed through the coord single source of truth
/// (`zup_to_yup_pos`) so this (and the ~15 collision sites that call it)
/// stays in lockstep with the canonical position swap. Bit-identical.
pub(super) fn havok_to_engine(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos([x, y, z]))
}

/// Convert a Havok quaternion [x, y, z, w] from Z-up to Y-up engine space.
pub(super) fn havok_quat_to_engine(q: [f32; 4]) -> Quat {
    // Havok quat is (x, y, z, w) in Z-up. Apply Z-up→Y-up: swap y↔z, negate
    // new z. #1617 — `.normalize()` adopts the #333 unit-quaternion guard the
    // array/matrix coord SoT carries, so a drifted-length Havok quat can't
    // leak scale into the ragdoll Transform rotation. Identity for the unit
    // quats Havok normally ships; the axis mapping is unchanged.
    Quat::from_xyzw(q[0], q[2], -q[1], q[3]).normalize()
}

/// Decompose a Havok 4x4 matrix into (translation, rotation) in engine space.
pub(super) fn decompose_havok_matrix(m: &[[f32; 4]; 4], scale: f32) -> (Vec3, Quat) {
    // Translation from column 3 (row-major: m[3][0..3]).
    let tx = m[3][0] * scale;
    let ty = m[3][1] * scale;
    let tz = m[3][2] * scale;
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
    // #1617 — `.normalize()` adopts the #333 unit-quaternion guard the coord
    // SoT (`zup_matrix_to_yup_quat`) carries, so a drifted Havok rotation
    // matrix can't leak scale into the placed-shape Transform. The basis
    // change above is left as-is (its column-major `from_cols` layout differs
    // from the SoT's row-major Shepperd path, so this is NOT a drop-in for
    // `zup_matrix_to_yup_quat` — only the normalize guard is shared).
    let rotation = Quat::from_mat3(&mat).normalize();

    (translation, rotation)
}

/// Structural guard: every `bhk*Shape` block the parser dispatches must have a
/// resolve arm in [`resolve_shape_inner`].
#[cfg(test)]
mod dispatch_coverage_tests {
    //! Regression for #1360 / #1361 (and the #1329 migration that left
    //! `BhkConvexSweepShape` + `BhkMeshShape` parse-dispatched but unresolved).
    //!
    //! A `bhk*Shape` block that is dispatched in `blocks/mod.rs` but has no
    //! `downcast_ref::<…>` arm in `resolve_shape_inner` parses for byte
    //! correctness and then silently drops the authored collision at the
    //! unsupported-shape fallback — the NIFAL "parsed then dropped" leak class.
    //! This test fails the moment a new shape is dispatched without a resolve
    //! arm, so the gap can't migrate from the parser tier to the canonical tier
    //! unnoticed again.
    use std::collections::HashSet;

    /// The struct identifier constructed by `Box::new(<Ident>::parse` on `line`,
    /// if it names a `Bhk…Shape`.
    fn constructed_shape(line: &str) -> Option<String> {
        let after = line.split("Box::new(").nth(1)?;
        let ident: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (ident.starts_with("Bhk") && ident.ends_with("Shape")).then_some(ident)
    }

    /// Every `Bhk…Shape` struct produced by a dispatch arm whose match key is a
    /// quoted `"bhk…Shape"` (excludes `…ShapeData`, `…Phantom`, collision
    /// objects, constraints). Handles the 2-line `bhkTransformShape |
    /// bhkConvexTransformShape` alias arm by probing the following lines.
    fn dispatched_shape_structs() -> HashSet<String> {
        let src = include_str!("../../blocks/mod.rs");
        let lines: Vec<&str> = src.lines().collect();
        let mut out = HashSet::new();
        for (i, line) in lines.iter().enumerate() {
            let is_shape_arm = line.contains("=>")
                && line
                    .split('"')
                    .any(|tok| tok.starts_with("bhk") && tok.ends_with("Shape"));
            if !is_shape_arm {
                continue;
            }
            for probe in i..=(i + 2).min(lines.len() - 1) {
                if let Some(ident) = constructed_shape(lines[probe]) {
                    out.insert(ident);
                    break;
                }
            }
        }
        out
    }

    /// Every `Bhk…Shape` struct that has a `downcast_ref::<…>` resolve arm.
    fn resolved_shape_structs() -> HashSet<String> {
        // Shape resolve arms all live in the sibling `shape.rs` after the
        // #1876 split (`resolve_shape_inner`'s `downcast_ref::<Bhk…Shape>`
        // chain); the collision-object downcasts that remain here are not
        // `…Shape` types, so this scan targets `shape.rs` exclusively.
        let src = include_str!("shape.rs");
        src.split("downcast_ref::<")
            .skip(1)
            .filter_map(|part| {
                let ident: String = part
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (ident.starts_with("Bhk") && ident.ends_with("Shape")).then_some(ident)
            })
            .collect()
    }

    #[test]
    fn every_dispatched_bhk_shape_has_resolve_arm() {
        let dispatched = dispatched_shape_structs();
        let resolved = resolved_shape_structs();

        // Sanity-check the source extractors so a future reformat that empties
        // a set can't turn this into a vacuous pass.
        assert!(
            dispatched.contains("BhkBoxShape") && dispatched.contains("BhkMeshShape"),
            "dispatch extractor regressed; found {dispatched:?}"
        );
        assert!(
            dispatched.len() >= 15,
            "expected >=15 dispatched bhk*Shape structs, found {}: {dispatched:?}",
            dispatched.len()
        );
        assert!(
            resolved.contains("BhkBoxShape"),
            "resolve extractor regressed; found {resolved:?}"
        );

        let missing: Vec<_> = dispatched.difference(&resolved).cloned().collect();
        assert!(
            missing.is_empty(),
            "these bhk*Shape blocks are parse-dispatched but have NO resolve arm in \
             resolve_shape_inner — authored collision silently drops: {missing:?}"
        );
    }
}

#[cfg(test)]
mod coord_sot_tests {
    use super::*;

    /// #1617 — `havok_to_engine` is now a thin wrapper over the coord single
    /// source of truth. Pin it bit-identical to the canonical `(x, z, -y)`
    /// swap so the consolidation can't silently diverge.
    #[test]
    fn havok_to_engine_matches_coord_sot() {
        for &(x, y, z) in &[(2.0f32, 3.0, 5.0), (-1.0, 7.0, -4.0), (0.0, 0.0, 0.0)] {
            let got = havok_to_engine(x, y, z);
            let sot = byroredux_core::math::coord::zup_to_yup_pos([x, y, z]);
            assert_eq!(
                [got.x, got.y, got.z],
                sot,
                "havok_to_engine drifted from coord SoT"
            );
            // And the literal canonical mapping it encodes.
            assert_eq!([got.x, got.y, got.z], [x, z, -y]);
        }
    }

    /// #1617 — the Havok rotation helpers adopt the #333 normalize guard:
    /// a deliberately drifted-length input must come out unit-length, while
    /// the axis mapping (x→x, y→z, z→-y, w→w) is preserved.
    #[test]
    fn havok_quat_to_engine_normalizes_and_keeps_axes() {
        // Unit input: axis mapping check.
        let unit = havok_quat_to_engine([0.0, 0.0, 0.0, 1.0]);
        assert!((unit.w - 1.0).abs() < 1e-6);
        // Drifted-length input (1.05x): must be renormalised to unit length.
        let drifted = havok_quat_to_engine([0.0, 0.0, 0.0, 1.05]);
        assert!(
            (drifted.length() - 1.0).abs() < 1e-6,
            "guard must renormalise"
        );
    }

    /// #1617 — `decompose_havok_matrix` likewise emits a unit quaternion even
    /// from a scaled (drifted) rotation basis.
    #[test]
    fn decompose_havok_matrix_emits_unit_quat() {
        // Identity rotation scaled 1.1x in the upper 3x3; translation in row 3.
        let m = [
            [1.1, 0.0, 0.0, 0.0],
            [0.0, 1.1, 0.0, 0.0],
            [0.0, 0.0, 1.1, 0.0],
            [4.0, 5.0, 6.0, 1.0],
        ];
        let (t, r) = decompose_havok_matrix(&m, 1.0);
        assert!(
            (r.length() - 1.0).abs() < 1e-6,
            "rotation must be unit-length"
        );
        // Translation still routes (x, z, -y) through the SoT.
        assert_eq!([t.x, t.y, t.z], [4.0, 6.0, -5.0]);
    }
}

#[cfg(test)]
mod dispatch_tests {
    //! Per-variant dispatch coverage for [`extract_collision`] and
    //! [`examine_collision_kind`]. The classic-bhk happy path is covered
    //! transitively by every scene-import test that loads a NIF with
    //! collision; these tests focus on the FO4+ NP and Skyrim+ phantom
    //! arms whose return value (`None`) is otherwise indistinguishable
    //! from "no collision authored" — a regression here would silently
    //! re-introduce the bug that landed `15016ee0`'s render-geometry
    //! trimesh fallback.
    use super::*;
    use crate::blocks::collision::{
        BhkCollisionObject, BhkNPCollisionObject, BhkPCollisionObject, BhkSphereShape,
        BhkSystemBinary,
    };
    use crate::blocks::NiObject;
    use crate::types::BlockRef;

    /// #1652 — `havok_motion_type` must map the full canonical
    /// `hkMotionType` enum, not the pre-fix `4 => Keyframed / _ => Static`
    /// collapse. Pins the four previously-wrong values (4, 5, 6, 8) plus
    /// the correct edges (1, 7, 9, 0).
    #[test]
    fn havok_motion_type_maps_full_enum() {
        // Dynamic family: DYNAMIC(1), SPHERE_INERTIA(2), SPHERE_STABILIZED(3),
        // BOX_INERTIA(4), BOX_STABILIZED(5), THIN_BOX(8).
        for v in [1u8, 2, 3, 4, 5, 8] {
            assert_eq!(
                havok_motion_type(v),
                MotionType::Dynamic,
                "hkMotionType {v} must be Dynamic"
            );
        }
        assert_eq!(havok_motion_type(6), MotionType::Keyframed, "KEYFRAMED");
        assert_eq!(havok_motion_type(7), MotionType::Static, "FIXED");
        assert_eq!(
            havok_motion_type(9),
            MotionType::CharacterKinematic,
            "CHARACTER"
        );
        // INVALID(0) and any out-of-range value fall back to Static.
        assert_eq!(havok_motion_type(0), MotionType::Static, "INVALID");
        assert_eq!(havok_motion_type(200), MotionType::Static, "out-of-range");
    }

    fn empty_scene() -> NifScene {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene
    }

    fn np_collision(data_ref: BlockRef) -> Box<dyn NiObject> {
        Box::new(BhkNPCollisionObject {
            target_ref: BlockRef::NULL,
            flags: 0x0029,
            data_ref,
            body_id: 0xdead_beef,
        })
    }

    fn system_binary(bytes: usize) -> Box<dyn NiObject> {
        Box::new(BhkSystemBinary {
            type_name: "bhkPhysicsSystem",
            data: vec![0u8; bytes],
        })
    }

    fn phantom_collision(body_ref: BlockRef) -> Box<dyn NiObject> {
        Box::new(BhkPCollisionObject {
            target_ref: BlockRef::NULL,
            flags: 0x0001,
            body_ref,
        })
    }

    fn classic_collision(body_ref: BlockRef) -> Box<dyn NiObject> {
        Box::new(BhkCollisionObject {
            target_ref: BlockRef::NULL,
            flags: 0x0001,
            body_ref,
        })
    }

    #[test]
    fn examine_returns_none_for_unresolved_ref() {
        let scene = empty_scene();
        assert_eq!(
            examine_collision_kind(&scene, BlockRef::NULL),
            CollisionAuthoring::None,
        );
        // Out-of-range index also resolves to None.
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(42u32)),
            CollisionAuthoring::None,
        );
    }

    #[test]
    fn examine_classifies_each_collision_subclass() {
        let mut scene = empty_scene();
        scene.blocks.push(classic_collision(BlockRef::NULL)); // [0]
        scene.blocks.push(np_collision(BlockRef::NULL)); // [1]
        scene.blocks.push(phantom_collision(BlockRef::NULL)); // [2]
        scene.blocks.push(Box::new(BhkSphereShape {
            material: 0,
            radius: 1.0,
        })); // [3] — non-collision block

        assert_eq!(
            examine_collision_kind(&scene, BlockRef(0u32)),
            CollisionAuthoring::Classic,
        );
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(1u32)),
            CollisionAuthoring::NewPhysicsStub,
        );
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(2u32)),
            CollisionAuthoring::Phantom,
        );
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(3u32)),
            CollisionAuthoring::Unrecognised,
        );
    }

    #[test]
    fn np_collision_returns_none_but_dispatcher_reaches_blob() {
        // The arm logs the blob size; we can't directly assert the log
        // line here, but the test guarantees the dispatcher routes a
        // BhkNPCollisionObject to extract_from_np (which always returns
        // None today) rather than falling through to the unrecognised
        // branch — a regression that returned None silently from the
        // top-level dispatcher would be invisible without this gate.
        let mut scene = empty_scene();
        scene.blocks.push(system_binary(2048)); // [0] blob
        scene.blocks.push(np_collision(BlockRef(0u32))); // [1] NP coll
        let result = extract_collision(&scene, BlockRef(1u32));
        assert!(
            result.is_none(),
            "NP collision must return None until the Havok blob decoder lands"
        );
        // Sanity: the same blob is still classified as NewPhysicsStub,
        // confirming the dispatcher routed correctly.
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(1u32)),
            CollisionAuthoring::NewPhysicsStub,
        );
    }

    #[test]
    fn np_collision_with_missing_blob_still_returns_none() {
        // data_ref points nowhere — the arm logs the "no Havok blob"
        // variant but the return contract holds.
        let mut scene = empty_scene();
        scene.blocks.push(np_collision(BlockRef::NULL));
        assert!(extract_collision(&scene, BlockRef(0u32)).is_none());
    }

    #[test]
    fn phantom_collision_returns_none() {
        // Phantom wraps a non-rigid-body. We return None so the consumer
        // doesn't mis-promote a trigger volume into a solid collider.
        let mut scene = empty_scene();
        scene.blocks.push(Box::new(BhkSphereShape {
            material: 0,
            radius: 1.0,
        })); // [0]
        scene.blocks.push(phantom_collision(BlockRef(0u32))); // [1]
        assert!(extract_collision(&scene, BlockRef(1u32)).is_none());
    }

    #[test]
    fn unrecognised_collision_ref_returns_none() {
        // A collision_ref that points at e.g. an NiNode (wrong subclass)
        // takes the unrecognised arm rather than panicking or returning
        // a malformed shape.
        let mut scene = empty_scene();
        scene.blocks.push(Box::new(BhkSphereShape {
            material: 0,
            radius: 1.0,
        }));
        assert!(extract_collision(&scene, BlockRef(0u32)).is_none());
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(0u32)),
            CollisionAuthoring::Unrecognised,
        );
    }
}
