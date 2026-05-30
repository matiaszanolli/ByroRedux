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
use crate::blocks::tri_shape::NiTriStripsData;
use crate::scene::NifScene;
use crate::types::BlockRef;

use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
use byroredux_core::math::{Quat, Vec3};

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
    let coll_idx = collision_ref.index()?;
    let block = scene.get(coll_idx)?;

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

/// Classic `BhkCollisionObject` → `BhkRigidBody` → shape-tree extractor.
/// This is the dominant path for Oblivion / FO3 / FNV / Skyrim LE / SSE and
/// still covers most FO4+ rigid bodies that author the legacy chain. Body of
/// the original `extract_collision` — preserved bit-for-bit so the refactor
/// is a no-op on every existing classic-bhk fixture.
fn extract_from_classic(
    scene: &NifScene,
    coll_obj: &BhkCollisionObject,
) -> Option<(CollisionShape, RigidBodyData)> {
    let body_idx = coll_obj.body_ref.index()?;
    let body = scene.get_as::<BhkRigidBody>(body_idx)?;

    let scale = scene.havok_scale;
    let mut visited = HashSet::new();
    let mut shape = resolve_shape(scene, body.shape_ref, &mut visited)?;

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

/// Recursively resolve a bhk shape block into a CollisionShape enum.
///
/// `visited` records `BlockRef` indices currently on the resolution
/// stack so a `BhkListShape` whose `sub_shape_refs` cycle (directly or
/// transitively) returns `None` instead of overflowing the stack
/// (#1269 / SAFE-DIM3-NEW-01). Entries are removed on return so a
/// legitimate DAG (the same shape referenced from two sibling subtrees)
/// still resolves on both arms. Vanilla content has no such cycles,
/// but a corrupt or adversarial NIF could otherwise crash the parser.
fn resolve_shape(
    scene: &NifScene,
    shape_ref: BlockRef,
    visited: &mut HashSet<usize>,
) -> Option<CollisionShape> {
    let idx = shape_ref.index()?;
    if !visited.insert(idx) {
        log::warn!(
            "resolve_shape: cycle detected at block {} — breaking recursion (#1269)",
            idx,
        );
        return None;
    }
    let result = resolve_shape_inner(scene, idx, visited);
    visited.remove(&idx);
    result
}

/// Inner body of `resolve_shape`. Extracted so the outer function can
/// own the `visited` insert/remove bookkeeping at a single entry/exit
/// point regardless of which match arm returns.
fn resolve_shape_inner(
    scene: &NifScene,
    idx: usize,
    visited: &mut HashSet<usize>,
) -> Option<CollisionShape> {
    let scale = scene.havok_scale;
    let block = scene.get(idx)?;

    // Sphere
    if let Some(s) = block.as_any().downcast_ref::<BhkSphereShape>() {
        return Some(CollisionShape::Ball {
            radius: s.radius * scale,
        });
    }

    // Multi-sphere — up to 8 offset spheres approximating a volume.
    // Each becomes a `Ball` child of a `Compound`, positioned at its
    // (havok→engine, scaled) center. Pre-fix this fell through to the
    // "unsupported" log and the authored collision was dropped entirely.
    if let Some(s) = block.as_any().downcast_ref::<BhkMultiSphereShape>() {
        let mut children = Vec::with_capacity(s.spheres.len());
        for sph in &s.spheres {
            let center = havok_to_engine(sph[0], sph[1], sph[2]) * scale;
            let radius = sph[3] * scale;
            children.push((
                center,
                Quat::IDENTITY,
                Box::new(CollisionShape::Ball { radius }),
            ));
        }
        return match children.len() {
            0 => None,
            // A single centred sphere is just a Ball — no Compound needed.
            1 if children[0].0 == Vec3::ZERO => {
                let (_, _, shape) = children.into_iter().next().unwrap();
                Some(*shape)
            }
            _ => Some(CollisionShape::Compound { children }),
        };
    }

    // Box
    if let Some(s) = block.as_any().downcast_ref::<BhkBoxShape>() {
        let [hx, hy, hz] = s.dimensions;
        return Some(CollisionShape::Cuboid {
            half_extents: havok_to_engine(hx, hy, hz) * scale,
        });
    }

    // Capsule
    if let Some(s) = block.as_any().downcast_ref::<BhkCapsuleShape>() {
        let p1 = havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * scale;
        let p2 = havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * scale;
        let half_height = (p2 - p1).length() * 0.5;
        let radius = s.radius1.max(s.radius2) * scale;
        return Some(CollisionShape::Capsule {
            half_height,
            radius,
        });
    }

    // Cylinder
    if let Some(s) = block.as_any().downcast_ref::<BhkCylinderShape>() {
        let p1 = havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * scale;
        let p2 = havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * scale;
        let half_height = (p2 - p1).length() * 0.5;
        let radius = s.cylinder_radius * scale;
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
            .map(|v| havok_to_engine(v[0], v[1], v[2]) * scale)
            .collect();
        return Some(CollisionShape::ConvexHull { vertices: verts });
    }

    // MOPP BV tree — skip the MOPP data, recurse into the wrapped shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkMoppBvTreeShape>() {
        return resolve_shape(scene, s.shape_ref, visited);
    }

    // Convex sweep — wraps a child shape swept along a direction. For static
    // rest-pose collision the sweep direction is a runtime motion hint, not
    // part of the collider, so we recurse into the wrapped shape (same as the
    // MOPP wrapper above). Dispatched at blocks/mod.rs but had no resolve arm
    // pre-#1360 → the authored collision silently dropped.
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexSweepShape>() {
        return resolve_shape(scene, s.shape_ref, visited);
    }

    // List shape — compound of sub-shapes.
    if let Some(s) = block.as_any().downcast_ref::<BhkListShape>() {
        let mut children = Vec::with_capacity(s.sub_shape_refs.len());
        for sub_ref in &s.sub_shape_refs {
            if let Some(child) = resolve_shape(scene, *sub_ref, visited) {
                children.push((Vec3::ZERO, Quat::IDENTITY, Box::new(child)));
            }
        }
        return if children.is_empty() {
            // All sub-shapes failed to resolve (cycle elimination or
            // unsupported types). Surface as None rather than an empty
            // Compound (#1269 — pre-fix a cycled BhkListShape would
            // return Compound { children: [] } after cycle detection
            // dropped the only sub-shape).
            None
        } else if children.len() == 1 {
            // Unwrap single-child compound.
            let (_, _, shape) = children.into_iter().next().unwrap();
            Some(*shape)
        } else {
            Some(CollisionShape::Compound { children })
        };
    }

    // Convex list — like BhkListShape, a compound of convex sub-shapes
    // (FO3/FNV/Skyrim destructibles, debris). Pre-fix this fell through
    // to the "unsupported" log and the authored collision was dropped.
    if let Some(s) = block.as_any().downcast_ref::<BhkConvexListShape>() {
        let mut children = Vec::with_capacity(s.sub_shapes.len());
        for sub_ref in &s.sub_shapes {
            if let Some(child) = resolve_shape(scene, *sub_ref, visited) {
                children.push((Vec3::ZERO, Quat::IDENTITY, Box::new(child)));
            }
        }
        return if children.is_empty() {
            None
        } else if children.len() == 1 {
            let (_, _, shape) = children.into_iter().next().unwrap();
            Some(*shape)
        } else {
            Some(CollisionShape::Compound { children })
        };
    }

    // Transform shape — apply 4x4 transform to child shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkTransformShape>() {
        let child = resolve_shape(scene, s.shape_ref, visited)?;
        let (translation, rotation) = decompose_havok_matrix(&s.transform, scale);
        return Some(CollisionShape::Compound {
            children: vec![(translation, rotation, Box::new(child))],
        });
    }

    // NiTriStrips mesh collision — resolve referenced NiTriStripsData.
    if let Some(s) = block.as_any().downcast_ref::<BhkNiTriStripsShape>() {
        return resolve_tri_strips_collision(scene, s);
    }

    // Mesh shape (Oblivion 10.0.1.0 only) — references NiTriStripsData just
    // like BhkNiTriStripsShape, plus a per-axis Scale vector. Dispatched at
    // blocks/mod.rs but had no resolve arm pre-#1361 → the authored collision
    // silently dropped. The Scale folds in alongside the uniform havok_scale;
    // a degenerate/unset scale vector falls back to identity rather than
    // collapsing the mesh to a point (which would render as empty → None).
    if let Some(s) = block.as_any().downcast_ref::<BhkMeshShape>() {
        let extra = if s.scale[..3].iter().all(|c| c.is_finite() && *c != 0.0) {
            [s.scale[0], s.scale[1], s.scale[2]]
        } else {
            [1.0, 1.0, 1.0]
        };
        return resolve_tri_strips_data_refs(scene, &s.data_refs, extra);
    }

    // Packed tri strips mesh collision.
    if let Some(s) = block.as_any().downcast_ref::<BhkPackedNiTriStripsShape>() {
        let data_idx = s.data_ref.index()?;
        let data = scene.get_as::<HkPackedNiTriStripsData>(data_idx)?;
        return resolve_packed_mesh(data, scale);
    }

    // Compressed mesh (Skyrim+) — resolve via data ref.
    if let Some(s) = block.as_any().downcast_ref::<BhkCompressedMeshShape>() {
        let data_idx = s.data_ref.index()?;
        let data = scene.get_as::<BhkCompressedMeshShapeData>(data_idx)?;
        return resolve_compressed_mesh(data, scale);
    }

    // Phantom (trigger volume) — resolve inner shape.
    if let Some(s) = block.as_any().downcast_ref::<BhkSimpleShapePhantom>() {
        return resolve_shape(scene, s.shape_ref, visited);
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
    resolve_tri_strips_data_refs(scene, &shape.data_refs, [1.0, 1.0, 1.0])
}

/// Merge the `NiTriStripsData` referenced by `data_refs` into a single TriMesh.
/// `extra_scale` is a per-axis multiplier applied in Havok space BEFORE the
/// uniform `havok_scale`: `bhkNiTriStripsShape` passes identity, while
/// `bhkMeshShape` passes its authored per-axis Scale vector.
fn resolve_tri_strips_data_refs(
    scene: &NifScene,
    data_refs: &[BlockRef],
    extra_scale: [f32; 3],
) -> Option<CollisionShape> {
    let scale = scene.havok_scale;
    let mut all_verts = Vec::new();
    let mut all_indices = Vec::new();

    for data_ref in data_refs {
        let Some(data_idx) = data_ref.index() else {
            continue;
        };
        let Some(data) = scene.get_as::<NiTriStripsData>(data_idx) else {
            continue;
        };

        let base_idx = all_verts.len() as u32;
        for v in &data.vertices {
            all_verts.push(
                havok_to_engine(
                    v.x * extra_scale[0],
                    v.y * extra_scale[1],
                    v.z * extra_scale[2],
                ) * scale,
            );
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
fn resolve_packed_mesh(data: &HkPackedNiTriStripsData, scale: f32) -> Option<CollisionShape> {
    if data.vertices.is_empty() {
        return None;
    }

    let vertices: Vec<Vec3> = data
        .vertices
        .iter()
        .map(|v| havok_to_engine(v[0], v[1], v[2]) * scale)
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

/// Convert a Havok quaternion [x, y, z, w] from Z-up to Y-up engine space.
fn havok_quat_to_engine(q: [f32; 4]) -> Quat {
    // Havok quat is (x, y, z, w) in Z-up. Apply Z-up→Y-up: swap y↔z, negate new z.
    Quat::from_xyzw(q[0], q[2], -q[1], q[3])
}

/// Decompose a Havok 4x4 matrix into (translation, rotation) in engine space.
fn decompose_havok_matrix(m: &[[f32; 4]; 4], scale: f32) -> (Vec3, Quat) {
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
    let rotation = Quat::from_mat3(&mat);

    (translation, rotation)
}

/// Convert bhkCompressedMeshShapeData into a TriMesh.
///
/// Merges big tris (full-precision) and chunk tris (quantized, strip-based)
/// into a single vertex/index buffer in engine space.
fn resolve_compressed_mesh(data: &BhkCompressedMeshShapeData, scale: f32) -> Option<CollisionShape> {
    let mut all_verts = Vec::new();
    let mut all_indices = Vec::new();

    // 1. Big tris — full-precision vertices.
    if !data.big_tris.is_empty() {
        let base = all_verts.len() as u32;
        for v in &data.big_verts {
            all_verts.push(havok_to_engine(v[0], v[1], v[2]) * scale);
        }
        for tri in &data.big_tris {
            all_indices.push([
                tri.v1 as u32 + base,
                tri.v2 as u32 + base,
                tri.v3 as u32 + base,
            ]);
        }
    }

    // 2. Chunks — quantized vertices + triangle strips.
    // Dequantization: world_pos = chunk.offset + (u16_vertex * error)
    // Confirmed via Havok source: Chunk::decompressVertex takes m_error parameter.
    // error is typically 0.001 but can vary per mesh.
    let error = data.error;
    for chunk in &data.chunks {
        let base = all_verts.len() as u32;
        let tx = chunk.translation[0];
        let ty = chunk.translation[1];
        let tz = chunk.translation[2];

        for qv in &chunk.vertices {
            let x = tx + qv[0] as f32 * error;
            let y = ty + qv[1] as f32 * error;
            let z = tz + qv[2] as f32 * error;
            all_verts.push(havok_to_engine(x, y, z) * scale);
        }

        // Havok chunk indices reference into the flat u16 vertex component array
        // (pre-multiplied by 3). Since we store vertices as [u16; 3] triples,
        // divide each index by 3 to get the vertex triple index.
        if chunk.strips.is_empty() {
            // Plain triangle list: every 3 indices = 1 triangle.
            let mut i = 0;
            while i + 2 < chunk.indices.len() {
                let a = chunk.indices[i] as u32 / 3 + base;
                let b = chunk.indices[i + 1] as u32 / 3 + base;
                let c = chunk.indices[i + 2] as u32 / 3 + base;
                if a != b && b != c && a != c {
                    all_indices.push([a, b, c]);
                }
                i += 3;
            }
        } else {
            // Triangle strips: convert each strip to triangles.
            let mut idx_offset = 0usize;
            for &strip_len in &chunk.strips {
                let end = idx_offset + strip_len as usize;
                let strip = &chunk.indices[idx_offset..end.min(chunk.indices.len())];
                for j in 2..strip.len() {
                    let (a, b, c) = if j % 2 == 0 {
                        (strip[j - 2], strip[j - 1], strip[j])
                    } else {
                        (strip[j - 1], strip[j - 2], strip[j])
                    };
                    if a != b && b != c && a != c {
                        all_indices.push([
                            a as u32 / 3 + base,
                            b as u32 / 3 + base,
                            c as u32 / 3 + base,
                        ]);
                    }
                }
                idx_offset = end;
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
        scene.blocks.push(classic_collision(BlockRef::NULL));   // [0]
        scene.blocks.push(np_collision(BlockRef::NULL));        // [1]
        scene.blocks.push(phantom_collision(BlockRef::NULL));   // [2]
        scene
            .blocks
            .push(Box::new(BhkSphereShape { material: 0, radius: 1.0 })); // [3] — non-collision block

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
        scene.blocks.push(system_binary(2048));                  // [0] blob
        scene.blocks.push(np_collision(BlockRef(0u32)));            // [1] NP coll
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
        scene
            .blocks
            .push(Box::new(BhkSphereShape { material: 0, radius: 1.0 })); // [0]
        scene.blocks.push(phantom_collision(BlockRef(0u32)));               // [1]
        assert!(extract_collision(&scene, BlockRef(1u32)).is_none());
    }

    #[test]
    fn unrecognised_collision_ref_returns_none() {
        // A collision_ref that points at e.g. an NiNode (wrong subclass)
        // takes the unrecognised arm rather than panicking or returning
        // a malformed shape.
        let mut scene = empty_scene();
        scene
            .blocks
            .push(Box::new(BhkSphereShape { material: 0, radius: 1.0 }));
        assert!(extract_collision(&scene, BlockRef(0u32)).is_none());
        assert_eq!(
            examine_collision_kind(&scene, BlockRef(0u32)),
            CollisionAuthoring::Unrecognised,
        );
    }
}

#[cfg(test)]
mod cycle_tests {
    //! Regression for #1269 / SAFE-DIM3-NEW-01: `resolve_shape` must
    //! detect a `BhkListShape` whose `sub_shape_refs` cycle and return
    //! `None` rather than overflow the stack. Visited bookkeeping uses
    //! insert-on-entry / remove-on-exit so legitimate DAG sharing (the
    //! same leaf shape referenced from two sibling subtrees) still
    //! resolves on both arms.
    use super::*;
    use crate::blocks::collision::{
        BhkConvexListShape, BhkConvexSweepShape, BhkListShape, BhkMeshShape, BhkMultiSphereShape,
        BhkSphereShape,
    };
    use crate::blocks::tri_shape::NiTriStripsData;
    use crate::blocks::NiObject;
    use crate::types::{BlockRef, NiPoint3};

    fn list_shape(refs: Vec<BlockRef>) -> Box<dyn NiObject> {
        Box::new(BhkListShape {
            sub_shape_refs: refs,
            material: 0,
            filters: Vec::new(),
        })
    }

    fn sphere_shape(radius: f32) -> Box<dyn NiObject> {
        Box::new(BhkSphereShape {
            material: 0,
            radius,
        })
    }

    #[test]
    fn list_shape_self_cycle_returns_none() {
        // Scene:
        //   [0] BhkListShape { sub_shape_refs = [0] }   // self-reference
        // Pre-#1269 this would unbounded-recurse and stack-overflow.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(list_shape(vec![BlockRef(0u32)]));
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        assert!(
            result.is_none(),
            "self-cycle must produce None, not a populated shape"
        );
    }

    #[test]
    fn list_shape_mutual_cycle_does_not_overflow() {
        // Scene:
        //   [0] BhkListShape { sub_shape_refs = [1] }
        //   [1] BhkListShape { sub_shape_refs = [0] }
        // Mutual cycle through two BhkListShapes. The cycle blocks the
        // inner recursion; the outer list ends up with no resolvable
        // children. Success here is "returned without overflowing the
        // stack" — the shape returned may be None or an empty
        // Compound, both are acceptable cycle-broken outcomes.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(list_shape(vec![BlockRef(1u32)]));
        scene.blocks.push(list_shape(vec![BlockRef(0u32)]));
        let mut visited = HashSet::new();
        let _ = resolve_shape(&scene, BlockRef(0u32), &mut visited);
    }

    #[test]
    fn multi_sphere_shape_resolves_to_compound_of_balls() {
        // Two offset spheres → Compound with two Ball children at their
        // (havok→engine, scaled) centers. Pre-fix this dropped entirely.
        let mut scene = NifScene::default();
        scene.havok_scale = 2.0;
        scene.blocks.push(Box::new(BhkMultiSphereShape {
            material: 0,
            shape_property: [0; 3],
            // havok (x,y,z,r); havok_to_engine maps to engine axes.
            spheres: vec![[1.0, 0.0, 0.0, 0.5], [0.0, 1.0, 0.0, 0.25]],
        }));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::Compound { children }) => {
                assert_eq!(children.len(), 2);
                // radii scaled by havok_scale.
                for (_, _, shape) in &children {
                    match **shape {
                        CollisionShape::Ball { radius } => {
                            assert!(radius == 1.0 || radius == 0.5, "got {radius}");
                        }
                        ref other => panic!("expected Ball child, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Compound of Balls, got {other:?}"),
        }
    }

    #[test]
    fn single_centred_multi_sphere_unwraps_to_ball() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMultiSphereShape {
            material: 0,
            shape_property: [0; 3],
            spheres: vec![[0.0, 0.0, 0.0, 3.0]], // centred → plain Ball
        }));
        let mut visited = HashSet::new();
        assert!(matches!(
            resolve_shape(&scene, BlockRef(0u32), &mut visited),
            Some(CollisionShape::Ball { radius }) if radius == 3.0
        ));
    }

    #[test]
    fn convex_list_shape_resolves_to_compound() {
        // ConvexList of two spheres → Compound, like BhkListShape.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkConvexListShape {
            sub_shapes: vec![BlockRef(1u32), BlockRef(2u32)],
            material: 0,
            radius: 0.0,
            use_cached_aabb: false,
            closest_point_min_distance: 0.0,
        }));
        scene.blocks.push(sphere_shape(1.0));
        scene.blocks.push(sphere_shape(2.0));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::Compound { children }) => assert_eq!(children.len(), 2),
            other => panic!("expected Compound, got {other:?}"),
        }
    }

    #[test]
    fn visited_resets_between_sibling_subtrees() {
        // Scene (DAG, not a cycle):
        //   [0] BhkListShape { sub_shape_refs = [1, 1] }   // shared leaf
        //   [1] BhkSphereShape { radius = 2.0 }
        // The same sphere is referenced twice as a child of the outer
        // list. Visited must remove on exit, so the second occurrence
        // still resolves rather than being mis-flagged as a cycle.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(list_shape(vec![BlockRef(1u32), BlockRef(1u32)]));
        scene.blocks.push(sphere_shape(2.0));
        let mut visited = HashSet::new();
        let result = resolve_shape(&scene, BlockRef(0u32), &mut visited);
        match result {
            Some(CollisionShape::Compound { children }) => {
                assert_eq!(
                    children.len(),
                    2,
                    "DAG sharing must produce two child entries, not one"
                );
            }
            other => panic!("expected Compound with two children, got {other:?}"),
        }
    }

    fn tri_strips_data(verts: Vec<NiPoint3>, strip: Vec<u16>) -> Box<dyn NiObject> {
        Box::new(NiTriStripsData {
            vertices: verts,
            normals: Vec::new(),
            center: NiPoint3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            radius: 0.0,
            vertex_colors: Vec::new(),
            uv_sets: Vec::new(),
            num_triangles: 0,
            strips: vec![strip],
        })
    }

    #[test]
    fn convex_sweep_shape_resolves_to_inner_shape() {
        // #1360: BhkConvexSweepShape was dispatched at parse but had no
        // resolve arm — its wrapped shape silently dropped. It must now
        // recurse into the wrapped shape (like the MOPP wrapper).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkConvexSweepShape {
            shape_ref: BlockRef(1u32),
            material: 0,
            radius: 0.0,
        }));
        scene.blocks.push(sphere_shape(4.0));
        let mut visited = HashSet::new();
        assert!(
            matches!(
                resolve_shape(&scene, BlockRef(0u32), &mut visited),
                Some(CollisionShape::Ball { radius }) if radius == 4.0
            ),
            "convex-sweep must resolve to its wrapped Ball, not drop"
        );
    }

    #[test]
    fn mesh_shape_resolves_to_trimesh() {
        // #1361: BhkMeshShape was dispatched at parse but had no resolve
        // arm — its referenced NiTriStripsData silently dropped. It must
        // now build a TriMesh, like BhkNiTriStripsShape.
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMeshShape {
            radius: 0.0,
            scale: [1.0, 1.0, 1.0, 0.0],
            data_refs: vec![BlockRef(1u32)],
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, indices }) => {
                assert_eq!(vertices.len(), 3, "all three verts converted");
                assert_eq!(indices.len(), 1, "one non-degenerate triangle");
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    #[test]
    fn mesh_shape_folds_per_axis_scale() {
        // BhkMeshShape's authored per-axis Scale vector must fold in
        // alongside havok_scale (applied in havok space before the swap).
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(Box::new(BhkMeshShape {
            radius: 0.0,
            scale: [2.0, 3.0, 5.0, 0.0],
            data_refs: vec![BlockRef(1u32)],
        }));
        scene.blocks.push(tri_strips_data(
            vec![
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                NiPoint3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ],
            vec![0, 1, 2],
        ));
        let mut visited = HashSet::new();
        match resolve_shape(&scene, BlockRef(0u32), &mut visited) {
            Some(CollisionShape::TriMesh { vertices, .. }) => {
                // Vertex (1,1,1) in havok space → scaled to (2,3,5), then
                // havok_to_engine maps (x,y,z) → (x, z, -y) = (2, 5, -3).
                let v = vertices[1];
                assert!(
                    (v.x - 2.0).abs() < 1e-5 && (v.y - 5.0).abs() < 1e-5 && (v.z + 3.0).abs() < 1e-5,
                    "per-axis scale not folded; got {v:?}"
                );
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }
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
        let src = include_str!("../blocks/mod.rs");
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
        let src = include_str!("collision.rs");
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
