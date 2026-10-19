//! Havok ragdoll articulation extraction — assembles the rigid bodies +
//! constraints of a skeletal NIF into an engine-native [`ImportedRagdoll`].
//! Split out of the original `import/collision.rs` (#1876).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::blocks::collision::*;
use crate::import::types::{
    ImportedJointKind, ImportedRagdoll, ImportedRagdollBody, ImportedRagdollConstraint,
};
use crate::scene::NifScene;

use byroredux_core::ecs::components::collision::CollisionShape;
use byroredux_core::math::{Mat3, Quat, Vec3};

use super::shape::{resolve_shape, segment_shape};
use super::{finite, finite_vec, havok_quat_to_engine, havok_to_engine};

/// Scene-level extractor: assemble the Havok ragdoll articulation (the
/// rigid bodies + the constraints linking them) into an engine-native
/// [`ImportedRagdoll`]. Returns `None` unless the scene carries a real
/// articulation — ≥2 bodies and ≥1 decoded Ragdoll/LimitedHinge joint;
/// a lone static collider isn't a ragdoll.
///
/// Bodies are reached via the classic `BhkCollisionObject → BhkRigidBody`
/// chain; FO4+ NP-blob ragdolls (`bhkRagdollSystem`) take
/// [`extract_np_ragdoll`] when the scene authors no discrete constraints. Each body's bone name is its host
/// NiNode's name. Constraint entity refs are rigid-body *block* indices,
/// remapped here to body-array indices. All geometry is Y-up + havok-
/// scaled, reusing the same helpers as [`extract_from_classic`].
pub fn extract_ragdoll(scene: &NifScene) -> Option<ImportedRagdoll> {
    let scale = scene.havok_scale;
    // `import_nif_scene` probes every NIF, including ordinary architecture
    // with one or more rigid bodies. Rejection diagnostics are useful only
    // when the file actually authored an articulation edge; otherwise an
    // unhosted/static body is normal content and warning about it would flood
    // cell-load logs. Breakable constraints are included so their existing
    // unsupported-edge diagnostic still fires.
    let has_constraint_authoring = scene.blocks.iter().any(|block| {
        block.as_any().is::<BhkConstraint>() || block.as_any().is::<BhkBreakableConstraint>()
    });
    if !has_constraint_authoring {
        // FO4+ keeps the whole articulation in a `bhkRagdollSystem` blob
        // instead of discrete `bhkRigidBody` / `bhkConstraint` blocks (#5015).
        return extract_np_ragdoll(scene);
    }
    let body_to_bone = build_body_to_bone(scene);

    // Collect skeletal rigid bodies in block order; map block idx → array idx
    // so the constraints below can translate their entity refs.
    let mut bodies = Vec::new();
    let mut block_to_body: HashMap<usize, usize> = HashMap::new();
    for (idx, block) in scene.blocks.iter().enumerate() {
        let Some(body) = block.as_any().downcast_ref::<BhkRigidBody>() else {
            continue;
        };
        // Only bodies hosted on a named bone take part in the ragdoll — a
        // stray rigid body with no host bone can't be driven from / written
        // back to a bone transform.
        let Some(bone_name) = body_to_bone.get(&idx).cloned() else {
            log::warn!(
                "extract_ragdoll: dropping rigid body block {idx} — no named NiNode hosts it"
            );
            continue;
        };
        let mut visited = HashSet::new();
        let Some(shape) = resolve_shape(scene, body.shape_ref, &mut visited) else {
            log::warn!(
                "extract_ragdoll: dropping rigid body block {idx} for bone '{bone_name}' — \
                 shape ref {:?} did not resolve to a supported finite shape",
                body.shape_ref,
            );
            continue;
        };
        // #1534 — finite guards on the body CInfo, mirroring the shape path
        // (#1409) two functions below. A non-finite mass / translation /
        // rotation from a corrupt or truncated Havok CInfo decode would seed
        // a NaN Rapier body and propagate through `ragdoll_writeback_system`
        // into `GlobalTransform` → bone palette → GPU skinning (NaN-on-GPU is
        // UB; NaN pixels stick through SVGF/TAA history). Drop the body — the
        // `bodies.len() < 2` guard below then drops the whole ragdoll if too
        // few survive, exactly like the shape-resolution failure path above.
        let Some(mass) = finite(body.mass) else {
            log::warn!(
                "extract_ragdoll: dropping rigid body block {idx} for bone '{bone_name}' — \
                 non-finite mass {}",
                body.mass,
            );
            continue;
        };
        let Some(translation) = finite_vec(
            havok_to_engine(
                body.translation[0],
                body.translation[1],
                body.translation[2],
            ) * scale,
        ) else {
            log::warn!(
                "extract_ragdoll: dropping rigid body block {idx} for bone '{bone_name}' — \
                 non-finite translation {:?}",
                body.translation,
            );
            continue;
        };
        let rotation = havok_quat_to_engine(body.rotation);
        if !rotation.is_finite() {
            log::warn!(
                "extract_ragdoll: dropping rigid body block {idx} for bone '{bone_name}' — \
                 non-finite rotation {:?}",
                body.rotation,
            );
            continue;
        }
        block_to_body.insert(idx, bodies.len());
        bodies.push(ImportedRagdollBody {
            bone_name,
            mass,
            linear_damping: body.linear_damping,
            angular_damping: body.angular_damping,
            friction: body.friction,
            restitution: body.restitution,
            shape,
            translation,
            rotation,
            is_t: body.is_t,
            pose_bone_id: None,
        });
    }
    if bodies.len() < 2 {
        return None;
    }

    let mut constraints = Vec::new();
    for (constraint_idx, block) in scene.blocks.iter().enumerate() {
        let Some(c) = block.as_any().downcast_ref::<BhkConstraint>() else {
            // #1850/#3792 — a `bhkBreakableConstraint` decodes into its OWN
            // struct (`BhkBreakableConstraint`), never a `BhkConstraint`, so it
            // falls through here. Since #3792 the wrapped geometry is decoded
            // for the four types a canonical joint exists for (Ragdoll/
            // LimitedHinge/Prismatic/Hinge — #1850's original gap), so try to
            // build the joint the same way a bare constraint would first.
            // Only a still-unmapped wrapped type (BallAndSocket/StiffSpring/
            // Malleable), an unresolved/self-looped endpoint, or a non-finite
            // decode falls through to the drop diagnostic below.
            if let Some(bc) = block.as_any().downcast_ref::<BhkBreakableConstraint>() {
                if let Some((body_a, body_b, kind)) = try_breakable_joint(bc, &block_to_body, scale)
                {
                    constraints.push(ImportedRagdollConstraint {
                        body_a,
                        body_b,
                        kind,
                    });
                    continue;
                }
                if let Some((bone_a, bone_b)) = breakable_dropped_edge(bc, &block_to_body, &bodies)
                {
                    log::warn!(
                        "extract_ragdoll: dropping bhkBreakableConstraint \
                         (wrapped_type={wt}) linking bones '{bone_a}' <-> '{bone_b}' — \
                         either its wrapped type has no canonical joint mapping yet \
                         (BallAndSocket / StiffSpring / nested Malleable) or its decoded \
                         geometry was non-finite. The ragdoll edge is lost; if it was the \
                         sole link to a limb, that limb will detach and free-fall (#1850).",
                        wt = bc.wrapped_type,
                    );
                }
            }
            continue;
        };
        // Constraint entity refs point at the rigid-body blocks; remap to
        // body-array indices FIRST so a dropped joint can name the two bones
        // it would have linked. A ref to a body we skipped (no bone / shape
        // failed) drops the joint gracefully.
        let (Some(body_a), Some(body_b)) = (
            c.entity_a
                .index()
                .and_then(|i| block_to_body.get(&i).copied()),
            c.entity_b
                .index()
                .and_then(|i| block_to_body.get(&i).copied()),
        ) else {
            log::warn!(
                "extract_ragdoll: dropping constraint block {constraint_idx} ({}) — \
                 endpoints {:?} / {:?} do not both resolve to retained ragdoll bodies",
                c.type_name,
                c.entity_a,
                c.entity_b,
            );
            continue;
        };
        if body_a == body_b {
            log::warn!(
                "extract_ragdoll: dropping constraint block {constraint_idx} ({}) — \
                 both endpoints resolve to bone '{}'",
                c.type_name,
                bodies[body_a].bone_name,
            );
            continue; // a joint must link two distinct bodies
        }
        // A non-finite limit angle / pivot / axis (#1534) drops the joint —
        // `[NaN, NaN]` limits handed to the Rapier solver destabilize it.
        let kind = match &c.data {
            BhkConstraintData::Ragdoll(r) => ragdoll_joint(r, scale),
            BhkConstraintData::LimitedHinge(h) => limited_hinge_joint(h, scale),
            // #3792 — the `creatures\protectron\skeleton.nif` edges this
            // arm used to fall through to `Other` for.
            BhkConstraintData::Prismatic(p) => prismatic_joint(p, scale),
            // #1539 — point-to-point (`bhkBallAndSocketConstraint`),
            // distance (`bhkStiffSpringConstraint`) and chain
            // (`bhkBallSocketConstraintChain`) constraints have no canonical
            // joint kind yet. #4212 decodes their CInfos, so the geometry is
            // available when one lands, but decoding is not mapping: they
            // are dropped here exactly as an undecoded `Other` is.
            //
            // Dropping one that links two ragdoll bones silently
            // disconnects the articulation: `orient_tree`
            // (`crates/physics/src/ragdoll.rs`) then yields a forest and
            // `build_ragdoll` builds the detached limb as an independent
            // free-floating multibody that free-falls. Every other
            // block-drop in this file logs (the FO4-NP / phantom arms
            // `log::debug!`); this one warns — louder, because unlike those
            // benign out-of-scope drops it can visibly break the ragdoll.
            BhkConstraintData::BallAndSocket(_)
            | BhkConstraintData::StiffSpring(_)
            | BhkConstraintData::BallSocketChain(_)
            | BhkConstraintData::Other => {
                log::warn!(
                    "extract_ragdoll: dropping constraint block {constraint_idx} ({ty}) \
                     linking bones '{a}' <-> '{b}' — no canonical joint kind exists for \
                     this constraint type. The ragdoll edge is lost; if it was the sole \
                     link to a limb, that limb will detach and free-fall (#1539).",
                    ty = c.type_name,
                    a = bodies[body_a].bone_name,
                    b = bodies[body_b].bone_name,
                );
                continue;
            }
        };
        let Some(kind) = kind else {
            log::warn!(
                "extract_ragdoll: dropping constraint block {constraint_idx} ({}) linking \
                 bones '{}' <-> '{}' — non-finite pivot, axis, or limit data",
                c.type_name,
                bodies[body_a].bone_name,
                bodies[body_b].bone_name,
            );
            continue;
        };
        constraints.push(ImportedRagdollConstraint {
            body_a,
            body_b,
            kind,
        });
    }
    if constraints.is_empty() {
        return None;
    }

    Some(ImportedRagdoll {
        bodies,
        constraints,
    })
}

/// #1850 — decide whether a `bhkBreakableConstraint` that the joint loop is
/// about to drop actually bridges two distinct ragdoll bodies (i.e. is a real
/// lost articulation edge worth warning about). Returns the two host bone
/// names when it does; `None` when either entity isn't a mapped ragdoll body
/// (constraint to a non-skeletal body / a body we skipped) or both entities
/// map to the same body (not an inter-limb edge). Pure so the drop is unit-
/// testable without a logger — the caller `log::warn!`s the returned names.
fn breakable_dropped_edge<'a>(
    bc: &BhkBreakableConstraint,
    block_to_body: &HashMap<usize, usize>,
    bodies: &'a [ImportedRagdollBody],
) -> Option<(&'a str, &'a str)> {
    let body_a = bc
        .entity_a
        .index()
        .and_then(|i| block_to_body.get(&i).copied())?;
    let body_b = bc
        .entity_b
        .index()
        .and_then(|i| block_to_body.get(&i).copied())?;
    if body_a == body_b {
        return None;
    }
    Some((
        bodies[body_a].bone_name.as_ref(),
        bodies[body_b].bone_name.as_ref(),
    ))
}

/// #3792 — try to build a joint for a breakable-wrapped constraint whose
/// geometry decoded (Ragdoll/LimitedHinge/Prismatic — #1850's long-
/// standing gap; a bare Hinge would also surface here via
/// `BhkConstraintData::LimitedHinge`, mirroring #3330). Mirrors the
/// bare-`BhkConstraint` resolve/validate/build steps in `extract_ragdoll`
/// but collapses every failure reason (unresolved endpoint, self-loop,
/// `Other` data, non-finite decode) to one opaque `None` — the caller
/// falls back to the same `breakable_dropped_edge` #1850 diagnostic
/// either way, so a finer-grained split isn't needed here.
fn try_breakable_joint(
    bc: &BhkBreakableConstraint,
    block_to_body: &HashMap<usize, usize>,
    scale: f32,
) -> Option<(usize, usize, ImportedJointKind)> {
    let body_a = bc
        .entity_a
        .index()
        .and_then(|i| block_to_body.get(&i).copied())?;
    let body_b = bc
        .entity_b
        .index()
        .and_then(|i| block_to_body.get(&i).copied())?;
    if body_a == body_b {
        return None;
    }
    let kind = match &bc.data {
        BhkConstraintData::Ragdoll(r) => ragdoll_joint(r, scale)?,
        BhkConstraintData::LimitedHinge(h) => limited_hinge_joint(h, scale)?,
        BhkConstraintData::Prismatic(p) => prismatic_joint(p, scale)?,
        // #4212 — decoded, but with no canonical joint kind to build (see
        // the sibling arm in `extract_ragdoll`), so they decline here the
        // same way undecoded data does.
        BhkConstraintData::BallAndSocket(_)
        | BhkConstraintData::StiffSpring(_)
        | BhkConstraintData::BallSocketChain(_)
        | BhkConstraintData::Other => return None,
    };
    Some((body_a, body_b, kind))
}

/// The values every FO4 ragdoll body takes for what its blob stores in rows
/// whose fields are not identified (material and motion properties — see
/// [`crate::blocks::collision::np_ragdoll`]): the ones the Skyrim humanoid
/// ragdoll authors on every body but the head. Mass, shape, pose and joints
/// are decoded; these four are not.
const NP_LINEAR_DAMPING: f32 = 0.099_609_375;
const NP_ANGULAR_DAMPING: f32 = 0.049_804_688;
const NP_FRICTION: f32 = 0.3;
const NP_RESTITUTION: f32 = 0.8;

/// A body whose bind pose is within this of its host bone's rest transform
/// (NIF units / quaternion dot) is coincident with it and needs no offset.
const NP_COINCIDENT_DISTANCE: f32 = 0.01;
const NP_COINCIDENT_DOT: f32 = 1.0 - 1.0e-6;

/// FO4+ counterpart of [`extract_ragdoll`]: assemble the articulation from a
/// skeleton's `bhkRagdollSystem` blob (#5015).
///
/// The blob lists bodies and joints in its own order; the skeleton's
/// `bhkNPCollisionObject`s name each body's host bone by body id. A body's
/// bind pose is stored in skeleton-root space, so it is expressed relative to
/// its bone's rest world transform — an exact zero offset for the 59 of 75
/// vanilla ragdoll skeletons whose bodies sit on their bones (`is_t` false,
/// like a classic non-`T` body), a bone-local offset for the rest (turrets,
/// the vertibird, a stray body or two on the deathclaw and mirelurk hunter).
/// Bodies with no host bone or an undecoded shape (convex hulls, compounds)
/// are dropped, with their joints.
pub(super) fn extract_np_ragdoll(scene: &NifScene) -> Option<ImportedRagdoll> {
    let (system_idx, blob) = scene.blocks.iter().enumerate().find_map(|(i, b)| {
        b.as_any()
            .downcast_ref::<BhkSystemBinary>()
            .filter(|s| s.type_name == "bhkRagdollSystem")
            .map(|s| (i, &s.data))
    })?;
    let decoded = match decode_np_ragdoll(blob) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("extract_np_ragdoll: bhkRagdollSystem block {system_idx} not decoded: {e}");
            return None;
        }
    };
    let scale = scene.havok_scale;

    // Host bone block per body id.
    let mut host: HashMap<usize, usize> = HashMap::new();
    for block in &scene.blocks {
        if let Some(c) = block.as_any().downcast_ref::<BhkNPCollisionObject>() {
            if c.data_ref.index() == Some(system_idx) {
                if let Some(target) = c.target_ref.index() {
                    host.insert(c.body_id as usize, target);
                }
            }
        }
    }
    let parents = node_parents(scene);

    let mut bodies = Vec::new();
    let mut blob_to_body: HashMap<usize, usize> = HashMap::new();
    for body in &decoded.bodies {
        let Some(&node) = host.get(&body.index) else {
            log::warn!("extract_np_ragdoll: dropping body {} — no bone hosts it", body.index);
            continue;
        };
        let Some(bone_name) = scene.blocks.get(node).and_then(|b| b.as_av_object()?.name_arc().cloned())
        else {
            log::warn!("extract_np_ragdoll: dropping body {} — its host bone has no name", body.index);
            continue;
        };
        let Some(shape) = body.shape.and_then(|s| np_shape(s, scale)) else {
            log::warn!(
                "extract_np_ragdoll: dropping body {} ('{bone_name}') — shape class not decoded",
                body.index
            );
            continue;
        };
        let Some(mass) = finite(body.mass) else { continue };
        let (is_t, translation, rotation) = match body_offset(scene, &parents, node, body, scale) {
            Some(offset) => offset,
            None => {
                log::warn!(
                    "extract_np_ragdoll: dropping body {} ('{bone_name}') — non-finite pose",
                    body.index
                );
                continue;
            }
        };
        blob_to_body.insert(body.index, bodies.len());
        bodies.push(ImportedRagdollBody {
            bone_name,
            mass,
            linear_damping: NP_LINEAR_DAMPING,
            angular_damping: NP_ANGULAR_DAMPING,
            friction: NP_FRICTION,
            restitution: NP_RESTITUTION,
            shape,
            translation,
            rotation,
            pose_bone_id: Some(body.bone_id),
            is_t,
        });
    }
    if bodies.len() < 2 {
        return None;
    }

    let mut constraints = Vec::new();
    for c in &decoded.constraints {
        let (Some(&body_a), Some(&body_b)) =
            (blob_to_body.get(&c.body_a), blob_to_body.get(&c.body_b))
        else {
            log::warn!(
                "extract_np_ragdoll: dropping joint between blob bodies {} and {} — an endpoint \
                 body was dropped. If it was the sole link to a limb, that limb will detach.",
                c.body_a,
                c.body_b,
            );
            continue;
        };
        let kind = match &c.data {
            NpConstraintData::Ragdoll(r) => ragdoll_joint(r, scale),
            NpConstraintData::LimitedHinge(h) => limited_hinge_joint(h, scale),
        };
        let Some(kind) = kind else { continue };
        constraints.push(ImportedRagdollConstraint {
            body_a,
            body_b,
            kind,
        });
    }
    if decoded.skipped_constraints > 0 {
        log::warn!(
            "extract_np_ragdoll: {} joint(s) of block {system_idx} use a constraint class or atom \
             layout that is not decoded and were dropped",
            decoded.skipped_constraints
        );
    }
    if constraints.is_empty() {
        return None;
    }
    Some(ImportedRagdoll {
        bodies,
        constraints,
    })
}

/// A blob shape in engine space (Y-up, havok-scaled), placed in the body's
/// frame like the classic capsule (`segment_shape`).
fn np_shape(shape: NpShape, scale: f32) -> Option<CollisionShape> {
    match shape {
        NpShape::Capsule { a, b, radius } => {
            let p1 = finite_vec(havok_to_engine(a[0], a[1], a[2]) * scale)?;
            let p2 = finite_vec(havok_to_engine(b[0], b[1], b[2]) * scale)?;
            let half_height = (p2 - p1).length() * 0.5;
            Some(segment_shape(
                p1,
                p2,
                CollisionShape::Capsule {
                    half_height,
                    radius: finite(radius * scale)?,
                },
            ))
        }
        NpShape::Sphere { radius } => Some(CollisionShape::Ball {
            radius: finite(radius * scale)?,
        }),
    }
}

/// Block index → parent `NiNode` block index.
fn node_parents(scene: &NifScene) -> HashMap<usize, usize> {
    let mut parents = HashMap::new();
    for (i, block) in scene.blocks.iter().enumerate() {
        if let Some(node) = block.as_any().downcast_ref::<crate::blocks::node::NiNode>() {
            for child in &node.children {
                if let Some(c) = child.index() {
                    parents.insert(c, i);
                }
            }
        }
    }
    parents
}

/// A node's rest world transform in NIF space: `(rotation, translation, scale)`.
fn node_world(
    scene: &NifScene,
    parents: &HashMap<usize, usize>,
    mut idx: usize,
) -> (Mat3, Vec3, f32) {
    let mut chain = Vec::new();
    // A parent chain is a tree; the bound only stops a corrupt cycle.
    for _ in 0..256 {
        if let Some(node) = scene.blocks.get(idx).and_then(|b| b.as_any().downcast_ref::<crate::blocks::node::NiNode>())
        {
            chain.push(&node.av.transform);
        }
        match parents.get(&idx) {
            Some(&p) => idx = p,
            None => break,
        }
    }
    let (mut r, mut t, mut s) = (Mat3::IDENTITY, Vec3::ZERO, 1.0f32);
    for local in chain.into_iter().rev() {
        let rows = &local.rotation.rows;
        let lr = Mat3::from_cols(
            Vec3::new(rows[0][0], rows[1][0], rows[2][0]),
            Vec3::new(rows[0][1], rows[1][1], rows[2][1]),
            Vec3::new(rows[0][2], rows[1][2], rows[2][2]),
        );
        t += r * (Vec3::new(local.translation.x, local.translation.y, local.translation.z) * s);
        r *= lr;
        s *= local.scale;
    }
    (r, t, s)
}

/// `(is_t, translation, rotation)` for a blob body hosted on `node`: the body's
/// bind pose relative to the bone's rest world transform, in engine space.
/// `None` for a non-finite pose.
fn body_offset(
    scene: &NifScene,
    parents: &HashMap<usize, usize>,
    node: usize,
    body: &NpRagdollBody,
    scale: f32,
) -> Option<(bool, Vec3, Quat)> {
    let body_rotation = Quat::from_xyzw(
        body.rotation[0],
        body.rotation[1],
        body.rotation[2],
        body.rotation[3],
    );
    let body_position = Vec3::from_array(body.position) * scale;
    if !body_rotation.is_finite() || !body_position.is_finite() || body_rotation.length_squared() < 1e-6 {
        return None;
    }
    let body_rotation = body_rotation.normalize();
    let (bone_rotation, bone_position, bone_scale) = node_world(scene, parents, node);
    if !bone_scale.is_finite() || bone_scale.abs() < f32::EPSILON {
        return None;
    }
    let bone_inverse = bone_rotation.transpose();
    let local_translation = bone_inverse * (body_position - bone_position) / bone_scale;
    let local_rotation = Quat::from_mat3(&bone_inverse) * body_rotation;
    if !local_translation.is_finite() || !local_rotation.is_finite() {
        return None;
    }
    let coincident = local_translation.length() < NP_COINCIDENT_DISTANCE
        && local_rotation.w.abs() > NP_COINCIDENT_DOT;
    if coincident {
        return Some((false, Vec3::ZERO, Quat::IDENTITY));
    }
    let q = local_rotation.normalize();
    Some((
        true,
        havok_to_engine(local_translation.x, local_translation.y, local_translation.z),
        havok_quat_to_engine([q.x, q.y, q.z, q.w]),
    ))
}

/// Map each `BhkRigidBody` block index → the name of the bone NiNode that
/// hosts it (via `NiNode.collision_ref → BhkCollisionObject.body_ref`).
/// The reverse link doesn't exist in the NIF, so we scan AVObjects.
fn build_body_to_bone(scene: &NifScene) -> HashMap<usize, Arc<str>> {
    let mut map = HashMap::new();
    for block in scene.blocks.iter() {
        let Some(av) = block.as_av_object() else {
            continue;
        };
        let Some(coll_idx) = av.collision_ref().index() else {
            continue;
        };
        let Some(coll) = scene.get_as::<BhkCollisionObject>(coll_idx) else {
            continue;
        };
        let Some(body_idx) = coll.body_ref.index() else {
            continue;
        };
        if let Some(name) = av.name_arc() {
            map.insert(body_idx, name.clone());
        }
    }
    map
}

/// Direction-only Z-up→Y-up swap: axis swap with no translation and no
/// havok scale (a unit direction's length must be preserved).
fn havok_dir_to_engine(v: [f32; 4]) -> Vec3 {
    havok_to_engine(v[0], v[1], v[2])
}

/// Returns `None` (dropping the joint) when any pivot, axis, or limit angle
/// is non-finite — a corrupt/truncated Havok CInfo decode (#1534). The
/// limit angles flow into `GenericJointBuilder::limits`, where `[NaN, NaN]`
/// destabilizes the Rapier solver.
fn ragdoll_joint(r: &RagdollCInfo, scale: f32) -> Option<ImportedJointKind> {
    Some(ImportedJointKind::Ragdoll {
        twist_a: finite_vec(havok_dir_to_engine(r.twist_a))?,
        plane_a: finite_vec(havok_dir_to_engine(r.plane_a))?,
        pivot_a: finite_vec(havok_to_engine(r.pivot_a[0], r.pivot_a[1], r.pivot_a[2]) * scale)?,
        twist_b: finite_vec(havok_dir_to_engine(r.twist_b))?,
        plane_b: finite_vec(havok_dir_to_engine(r.plane_b))?,
        pivot_b: finite_vec(havok_to_engine(r.pivot_b[0], r.pivot_b[1], r.pivot_b[2]) * scale)?,
        cone_max: finite(r.cone_max_angle)?,
        plane_min: finite(r.plane_min_angle)?,
        plane_max: finite(r.plane_max_angle)?,
        twist_min: finite(r.twist_min_angle)?,
        twist_max: finite(r.twist_max_angle)?,
    })
}

/// `LimitedHinge` sibling of [`ragdoll_joint`] — same non-finite drop (#1534).
///
/// #2448 / PHYS-02 — threads the authored perp-axis zero-reference
/// (`Perp Axis In A1`/`Perp Axis In B1`) through so the solver boundary
/// (`crates/physics/src/ragdoll.rs::build_joint`) can build the hinge's
/// angle-limit frame from the content author's own reference instead of
/// synthesizing an arbitrary one. Oblivion/Morrowind limited hinges derive
/// their missing B1 as `Perp B2 × Axis B` in
/// `LimitedHingeCInfo::parse_oblivion`. The separate plain-hinge parser still
/// zero-fills both absent B-side perpendiculars; those zeros pass through
/// `finite_vec`, and `build_joint` synthesizes a perpendicular for that
/// plain-hinge case.
fn limited_hinge_joint(h: &LimitedHingeCInfo, scale: f32) -> Option<ImportedJointKind> {
    Some(ImportedJointKind::LimitedHinge {
        axis_a: finite_vec(havok_dir_to_engine(h.axis_a))?,
        perp_a: finite_vec(havok_dir_to_engine(h.perp_axis_in_a1))?,
        pivot_a: finite_vec(havok_to_engine(h.pivot_a[0], h.pivot_a[1], h.pivot_a[2]) * scale)?,
        axis_b: finite_vec(havok_dir_to_engine(h.axis_b))?,
        perp_b: finite_vec(havok_dir_to_engine(h.perp_axis_in_b1))?,
        pivot_b: finite_vec(havok_to_engine(h.pivot_b[0], h.pivot_b[1], h.pivot_b[2]) * scale)?,
        min_angle: finite(h.min_angle)?,
        max_angle: finite(h.max_angle)?,
    })
}

/// `Prismatic` sibling of [`limited_hinge_joint`] — same non-finite drop
/// (#1534). `Sliding A`/`Sliding B` become `axis_a`/`axis_b` (the free
/// translation axis); `Rotation A`/`Rotation B` become `perp_a`/`perp_b`
/// (the zero-reference the solver boundary orthogonalises against the
/// axis, same role `Perp Axis In A1`/`B1` play for `LimitedHinge`). #3792.
fn prismatic_joint(p: &PrismaticCInfo, scale: f32) -> Option<ImportedJointKind> {
    Some(ImportedJointKind::Prismatic {
        axis_a: finite_vec(havok_dir_to_engine(p.sliding_a))?,
        perp_a: finite_vec(havok_dir_to_engine(p.rotation_a))?,
        pivot_a: finite_vec(havok_to_engine(p.pivot_a[0], p.pivot_a[1], p.pivot_a[2]) * scale)?,
        axis_b: finite_vec(havok_dir_to_engine(p.sliding_b))?,
        perp_b: finite_vec(havok_dir_to_engine(p.rotation_b))?,
        pivot_b: finite_vec(havok_to_engine(p.pivot_b[0], p.pivot_b[1], p.pivot_b[2]) * scale)?,
        // Distances are body-local displacements along the sliding axis —
        // same authored-unit-then-scaled treatment as a pivot, not a
        // direction.
        min_distance: finite(p.min_distance * scale)?,
        max_distance: finite(p.max_distance * scale)?,
    })
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // Synthetic scenes are assembled block-by-block.
mod ragdoll_extract_tests {
    //! CI-runnable coverage for [`extract_ragdoll`] (M41.x Phase 2). The
    //! real-data path (18-body FNV skeleton) is `crates/nif/tests/
    //! ragdoll_import.rs`, `#[ignore]`; these synthetic scenes lock the
    //! body-collection, host-bone mapping, constraint-remap, and
    //! not-a-ragdoll gates without game data.
    use super::*;
    // The zero-mass-Dynamic reclassification tests exercise the classic
    // collision-object path (`extract_collision`, in the parent module),
    // reusing this module's `coll_obj`/`sphere` fixtures. #1876.
    use super::super::extract_collision;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::collision::{
        BhkBreakableConstraint, BhkCollisionObject, BhkConstraint, BhkConstraintData, BhkRigidBody,
        BhkSphereShape, LimitedHingeCInfo, PrismaticCInfo, RagdollCInfo,
    };
    use crate::blocks::node::NiNode;
    use crate::blocks::NiObject;
    use crate::types::{BlockRef, NiTransform};
    use byroredux_core::ecs::components::collision::{CollisionShape, MotionType};

    fn bone(name: &str, collision_ref: usize) -> Box<dyn NiObject> {
        Box::new(NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform::default(),
                properties: Vec::new(),
                collision_ref: BlockRef(collision_ref as u32),
            },
            children: Vec::new(),
            effects: Vec::new(),
        })
    }

    fn coll_obj(body_ref: usize) -> Box<dyn NiObject> {
        Box::new(BhkCollisionObject {
            target_ref: BlockRef::NULL,
            flags: 0,
            body_ref: BlockRef(body_ref as u32),
        })
    }

    fn rigid_body(shape_ref: usize) -> Box<dyn NiObject> {
        Box::new(BhkRigidBody {
            shape_ref: BlockRef(shape_ref as u32),
            havok_filter: 0,
            is_t: false,
            translation: [0.0; 4],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 4],
            angular_velocity: [0.0; 4],
            inertia_tensor: [0.0; 12],
            center_of_mass: [0.0; 4],
            mass: 5.0,
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.3,
            restitution: 0.4,
            max_linear_velocity: 0.0,
            max_angular_velocity: 0.0,
            penetration_depth: 0.0,
            motion_type: 1,
            deactivator_type: 0,
            solver_deactivation: 0,
            quality_type: 0,
            constraint_refs: Vec::new(),
            body_flags: 0,
        })
    }

    fn sphere(radius: f32) -> Box<dyn NiObject> {
        Box::new(BhkSphereShape {
            material: 0,
            radius,
        })
    }

    fn ragdoll_constraint(entity_a: usize, entity_b: usize, pivot_a_x: f32) -> Box<dyn NiObject> {
        Box::new(BhkConstraint {
            type_name: "bhkRagdollConstraint",
            entity_a: BlockRef(entity_a as u32),
            entity_b: BlockRef(entity_b as u32),
            priority: 1,
            data: BhkConstraintData::Ragdoll(RagdollCInfo {
                twist_a: [0.0, 0.0, 1.0, 0.0],
                plane_a: [1.0, 0.0, 0.0, 0.0],
                motor_a: [0.0; 4],
                pivot_a: [pivot_a_x, 0.0, 0.0, 1.0],
                twist_b: [0.0, 0.0, 1.0, 0.0],
                plane_b: [1.0, 0.0, 0.0, 0.0],
                motor_b: [0.0; 4],
                pivot_b: [-pivot_a_x, 0.0, 0.0, 1.0],
                cone_max_angle: 0.5,
                plane_min_angle: -0.5,
                plane_max_angle: 0.5,
                twist_min_angle: -0.3,
                twist_max_angle: 0.3,
                max_friction: 0.0,
            }),
        })
    }

    /// Two bones, each with a capsule rigid body, joined by one Ragdoll
    /// constraint → a 2-body / 1-joint graph with the right bone names,
    /// remapped indices, and Y-up pivot.
    #[test]
    fn two_bone_ragdoll_extracts_graph() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(ragdoll_constraint(2, 6, 10.0)); // [8] refs rigid-body blocks

        let r = extract_ragdoll(&scene).expect("two bodies + one joint must yield a ragdoll");
        assert_eq!(r.bodies.len(), 2);
        assert_eq!(r.bodies[0].bone_name.as_ref(), "Bip01 Pelvis");
        assert_eq!(r.bodies[1].bone_name.as_ref(), "Bip01 Spine");
        assert_eq!(r.bodies[0].mass, 5.0);

        assert_eq!(r.constraints.len(), 1);
        let c = &r.constraints[0];
        // Block 2 → body 0, block 6 → body 1.
        assert_eq!(c.body_a, 0);
        assert_eq!(c.body_b, 1);
        match &c.kind {
            ImportedJointKind::Ragdoll { pivot_a, .. } => {
                // Havok pivot (10,0,0) → engine (x,z,-y) = (10,0,0) at scale 1.
                assert_eq!(*pivot_a, Vec3::new(10.0, 0.0, 0.0));
            }
            other => panic!("expected Ragdoll joint, got {other:?}"),
        }
    }

    /// #2447 / PHYS-01 — `extract_ragdoll` must propagate each body's
    /// source-block `is_t` flag onto `ImportedRagdollBody::is_t` verbatim
    /// (mirroring `extract_from_classic`'s `is_t` gate for architecture
    /// colliders, #2316), so the NIF→ECS boundary downstream
    /// (`template_from_imported`) knows whether the body's CInfo
    /// translation/rotation are real or must be treated as identity.
    #[test]
    fn extract_ragdoll_propagates_is_t_per_body() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene
            .blocks
            .push(rigid_body_at(3, [1.0, 2.0, 3.0, 1.0], true)); // [2] bhkRigidBodyT
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene
            .blocks
            .push(rigid_body_at(7, [4.0, 5.0, 6.0, 1.0], false)); // [6] plain bhkRigidBody
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(ragdoll_constraint(2, 6, 10.0)); // [8]

        let r = extract_ragdoll(&scene).expect("two bodies + one joint must yield a ragdoll");
        assert!(r.bodies[0].is_t, "block 2 was bhkRigidBodyT");
        assert!(!r.bodies[1].is_t, "block 6 was plain bhkRigidBody");
    }

    fn limited_hinge_constraint(entity_a: usize, entity_b: usize) -> Box<dyn NiObject> {
        Box::new(BhkConstraint {
            type_name: "bhkLimitedHingeConstraint",
            entity_a: BlockRef(entity_a as u32),
            entity_b: BlockRef(entity_b as u32),
            priority: 1,
            data: BhkConstraintData::LimitedHinge(LimitedHingeCInfo {
                axis_a: [0.0, 0.0, 1.0, 0.0],
                // Havok Y (0,1,0) → engine (x,z,-y) = (0,0,-1).
                perp_axis_in_a1: [0.0, 1.0, 0.0, 0.0],
                perp_axis_in_a2: [1.0, 0.0, 0.0, 0.0],
                pivot_a: [10.0, 0.0, 0.0, 1.0],
                axis_b: [0.0, 0.0, 1.0, 0.0],
                // Havok X (1,0,0) → engine (x,z,-y) = (1,0,0).
                perp_axis_in_b1: [1.0, 0.0, 0.0, 0.0],
                perp_axis_in_b2: [0.0, 1.0, 0.0, 0.0],
                pivot_b: [-10.0, 0.0, 0.0, 1.0],
                min_angle: -0.5,
                max_angle: 0.5,
                max_friction: 0.0,
            }),
        })
    }

    /// #2448 / PHYS-02 — `extract_ragdoll` must thread the authored
    /// `Perp Axis In A1`/`Perp Axis In B1` zero-reference vectors through to
    /// `ImportedJointKind::LimitedHinge::perp_a`/`perp_b`, converted Z-up →
    /// Y-up like every other direction in this module (not silently
    /// discarded, the pre-fix behavior this regresses).
    #[test]
    fn limited_hinge_perp_axis_survives_extraction() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 LUpperArm", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 LForearm", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(limited_hinge_constraint(2, 6)); // [8]

        let r = extract_ragdoll(&scene).expect("two bodies + one joint must yield a ragdoll");
        match &r.constraints[0].kind {
            ImportedJointKind::LimitedHinge { perp_a, perp_b, .. } => {
                assert_eq!(*perp_a, Vec3::new(0.0, 0.0, -1.0));
                assert_eq!(*perp_b, Vec3::new(1.0, 0.0, 0.0));
            }
            other => panic!("expected LimitedHinge joint, got {other:?}"),
        }
    }

    fn prismatic_constraint(entity_a: usize, entity_b: usize) -> Box<dyn NiObject> {
        Box::new(BhkConstraint {
            type_name: "bhkPrismaticConstraint",
            entity_a: BlockRef(entity_a as u32),
            entity_b: BlockRef(entity_b as u32),
            priority: 1,
            data: BhkConstraintData::Prismatic(PrismaticCInfo {
                sliding_a: [1.0, 0.0, 0.0, 0.0],
                rotation_a: [0.0, 1.0, 0.0, 0.0],
                plane_a: [0.0, 0.0, 1.0, 0.0],
                pivot_a: [10.0, 0.0, 0.0, 1.0],
                sliding_b: [1.0, 0.0, 0.0, 0.0],
                rotation_b: [0.0, 1.0, 0.0, 0.0],
                plane_b: [0.0, 0.0, 1.0, 0.0],
                pivot_b: [-10.0, 0.0, 0.0, 1.0],
                min_distance: -2.0,
                max_distance: 2.0,
                friction: 0.1,
            }),
        })
    }

    /// #3792 — bare `bhkPrismaticConstraint` extraction, mirroring
    /// [`limited_hinge_perp_axis_survives_extraction`]: the two
    /// `creatures\protectron\skeleton.nif` edges this closes.
    #[test]
    fn prismatic_constraint_extracts_as_a_joint() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Head", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Head Dome", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(prismatic_constraint(2, 6)); // [8]

        let r = extract_ragdoll(&scene).expect("two bodies + one joint must yield a ragdoll");
        assert_eq!(r.constraints.len(), 1);
        match &r.constraints[0].kind {
            ImportedJointKind::Prismatic {
                axis_a,
                pivot_a,
                min_distance,
                max_distance,
                ..
            } => {
                // Havok +X (1,0,0) → engine (x,z,-y) = (1,0,0).
                assert_eq!(*axis_a, Vec3::new(1.0, 0.0, 0.0));
                assert_eq!(*pivot_a, Vec3::new(10.0, 0.0, 0.0));
                assert_eq!(*min_distance, -2.0);
                assert_eq!(*max_distance, 2.0);
            }
            other => panic!("expected Prismatic joint, got {other:?}"),
        }
    }

    /// A single body (no second body, no joint) is not a ragdoll.
    #[test]
    fn single_body_is_not_a_ragdoll() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        assert!(extract_ragdoll(&scene).is_none());
    }

    /// Two bodies but no constraint linking them → not a ragdoll.
    #[test]
    fn bodies_without_joints_is_not_a_ragdoll() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        assert!(extract_ragdoll(&scene).is_none());
    }

    // ── #1534 — non-finite CInfo finite guards ─────────────────────────

    /// A `rigid_body` with a caller-set translation, so the body-extraction
    /// finite guard can be exercised through `extract_ragdoll`.
    fn rigid_body_at(shape_ref: usize, translation: [f32; 4], is_t: bool) -> Box<dyn NiObject> {
        Box::new(BhkRigidBody {
            shape_ref: BlockRef(shape_ref as u32),
            havok_filter: 0,
            is_t,
            translation,
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 4],
            angular_velocity: [0.0; 4],
            inertia_tensor: [0.0; 12],
            center_of_mass: [0.0; 4],
            mass: 5.0,
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.3,
            restitution: 0.4,
            max_linear_velocity: 0.0,
            max_angular_velocity: 0.0,
            penetration_depth: 0.0,
            motion_type: 1,
            deactivator_type: 0,
            solver_deactivation: 0,
            quality_type: 0,
            constraint_refs: Vec::new(),
            body_flags: 0,
        })
    }

    /// A ragdoll constraint with a caller-set twist-max limit (poison it to
    /// NaN to exercise the joint finite guard).
    fn ragdoll_constraint_twist_max(
        entity_a: usize,
        entity_b: usize,
        twist_max_angle: f32,
    ) -> Box<dyn NiObject> {
        Box::new(BhkConstraint {
            type_name: "bhkRagdollConstraint",
            entity_a: BlockRef(entity_a as u32),
            entity_b: BlockRef(entity_b as u32),
            priority: 1,
            data: BhkConstraintData::Ragdoll(RagdollCInfo {
                twist_a: [0.0, 0.0, 1.0, 0.0],
                plane_a: [1.0, 0.0, 0.0, 0.0],
                motor_a: [0.0; 4],
                pivot_a: [10.0, 0.0, 0.0, 1.0],
                twist_b: [0.0, 0.0, 1.0, 0.0],
                plane_b: [1.0, 0.0, 0.0, 0.0],
                motor_b: [0.0; 4],
                pivot_b: [-10.0, 0.0, 0.0, 1.0],
                cone_max_angle: 0.5,
                plane_min_angle: -0.5,
                plane_max_angle: 0.5,
                twist_min_angle: -0.3,
                twist_max_angle,
                max_friction: 0.0,
            }),
        })
    }

    /// A NaN body translation (corrupt/truncated Havok CInfo) drops that
    /// body. With only one finite body left, the `< 2` gate drops the whole
    /// ragdoll rather than seeding a NaN Rapier body that would propagate
    /// through writeback into the GPU bone palette. See #1534.
    #[test]
    fn non_finite_body_translation_drops_the_body() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene
            .blocks
            .push(rigid_body_at(3, [f32::NAN, 0.0, 0.0, 0.0], false)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(ragdoll_constraint(2, 6, 10.0)); // [8]

        // One body dropped ⇒ fewer than 2 survive ⇒ no ragdoll.
        assert!(
            extract_ragdoll(&scene).is_none(),
            "a NaN-translation body must be dropped, collapsing the ragdoll",
        );
    }

    /// A NaN joint limit angle drops the joint; with no constraints left the
    /// ragdoll is rejected (`[NaN, NaN]` limits would destabilize the Rapier
    /// solver). See #1534.
    #[test]
    fn non_finite_joint_limit_drops_the_joint() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene
            .blocks
            .push(ragdoll_constraint_twist_max(2, 6, f32::NAN)); // [8]

        assert!(
            extract_ragdoll(&scene).is_none(),
            "a NaN joint limit must drop the joint, leaving no constraints",
        );
    }

    /// Sanity: the same graphs WITH finite values do build — proves the
    /// guards reject only the poisoned field, not the healthy path.
    #[test]
    fn finite_graph_still_builds() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1));
        scene.blocks.push(coll_obj(2));
        scene.blocks.push(rigid_body(3));
        scene.blocks.push(sphere(1.0));
        scene.blocks.push(bone("Bip01 Spine", 5));
        scene.blocks.push(coll_obj(6));
        scene.blocks.push(rigid_body(7));
        scene.blocks.push(sphere(1.0));
        scene.blocks.push(ragdoll_constraint(2, 6, 10.0));
        assert!(extract_ragdoll(&scene).is_some());
    }

    // ── #1850 — bhkBreakableConstraint is invisible to the joint loop ──

    fn breakable_constraint(
        entity_a: usize,
        entity_b: usize,
        wrapped_type: u32,
    ) -> BhkBreakableConstraint {
        breakable_constraint_with_data(entity_a, entity_b, wrapped_type, BhkConstraintData::Other)
    }

    /// #3792 — sibling of [`breakable_constraint`] that lets a test supply
    /// the (now-decoded) wrapped geometry, so a breakable-wrapped
    /// Ragdoll/LimitedHinge/Prismatic can exercise the surfaced-joint path.
    fn breakable_constraint_with_data(
        entity_a: usize,
        entity_b: usize,
        wrapped_type: u32,
        data: BhkConstraintData,
    ) -> BhkBreakableConstraint {
        BhkBreakableConstraint {
            entity_a: BlockRef(entity_a as u32),
            entity_b: BlockRef(entity_b as u32),
            priority: 1,
            wrapped_type,
            threshold: 100.0,
            remove_when_broken: true,
            data,
        }
    }

    fn body_named(name: &str) -> ImportedRagdollBody {
        ImportedRagdollBody {
            bone_name: Arc::from(name),
            mass: 5.0,
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.3,
            restitution: 0.4,
            shape: CollisionShape::Ball { radius: 1.0 },
            translation: Vec3::ZERO,
            rotation: byroredux_core::math::Quat::IDENTITY,
            pose_bone_id: None,
            is_t: true,
        }
    }

    /// The `breakable_dropped_edge` helper (which drives the #1850 warn) names
    /// the two host bones when a `bhkBreakableConstraint` bridges two distinct
    /// ragdoll bodies — a wrapped Ragdoll(7)/LimitedHinge(2) that would
    /// otherwise vanish silently. It reports `None` when the edge isn't a real
    /// inter-limb link (self-loop or a ref to an unmapped body).
    #[test]
    fn breakable_dropped_edge_names_the_two_bones() {
        let bodies = vec![body_named("Bip01 Pelvis"), body_named("Bip01 Spine")];
        // block idx 2 → body 0, block idx 6 → body 1 (mirrors the remap in
        // `extract_ragdoll`).
        let block_to_body: HashMap<usize, usize> = [(2, 0), (6, 1)].into_iter().collect();

        // wrapped Ragdoll linking the two distinct bodies → a real dropped edge.
        let bc = breakable_constraint(2, 6, 7);
        assert_eq!(
            breakable_dropped_edge(&bc, &block_to_body, &bodies),
            Some(("Bip01 Pelvis", "Bip01 Spine")),
        );
        // wrapped LimitedHinge is equally a real edge.
        let hinge = breakable_constraint(6, 2, 2);
        assert_eq!(
            breakable_dropped_edge(&hinge, &block_to_body, &bodies),
            Some(("Bip01 Spine", "Bip01 Pelvis")),
        );
        // self-loop (both entities map to the same body) → not an inter-limb edge.
        let self_loop = breakable_constraint(2, 2, 7);
        assert_eq!(
            breakable_dropped_edge(&self_loop, &block_to_body, &bodies),
            None
        );
        // ref to an unmapped body (block 99 was never collected) → dropped quietly.
        let dangling = breakable_constraint(2, 99, 7);
        assert_eq!(
            breakable_dropped_edge(&dangling, &block_to_body, &bodies),
            None
        );
    }

    /// #3792 — end-to-end: a 2-body scene whose only articulation link is a
    /// `bhkBreakableConstraint` wrapping a Ragdoll (the dominant vanilla FNV
    /// form) now surfaces a real joint, since the wrapped geometry is
    /// decoded rather than skipped. Pre-#3792 this dropped unconditionally
    /// (#1850) regardless of whether the wrapped geometry existed; this is
    /// the regression #3792 exists to fix — a Protectron-class skeleton
    /// whose only link to a limb is a breakable-wrapped joint must not
    /// fragment.
    #[test]
    fn breakable_wrapped_ragdoll_now_surfaces_as_a_joint() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(Box::new(breakable_constraint_with_data(
            2,
            6,
            7, // Ragdoll
            BhkConstraintData::Ragdoll(RagdollCInfo {
                twist_a: [0.0, 0.0, 1.0, 0.0],
                plane_a: [1.0, 0.0, 0.0, 0.0],
                motor_a: [0.0; 4],
                pivot_a: [10.0, 0.0, 0.0, 1.0],
                twist_b: [0.0, 0.0, 1.0, 0.0],
                plane_b: [1.0, 0.0, 0.0, 0.0],
                motor_b: [0.0; 4],
                pivot_b: [-10.0, 0.0, 0.0, 1.0],
                cone_max_angle: 0.5,
                plane_min_angle: -0.5,
                plane_max_angle: 0.5,
                twist_min_angle: -0.3,
                twist_max_angle: 0.3,
                max_friction: 0.0,
            }),
        ))); // [8]

        let r = extract_ragdoll(&scene)
            .expect("a breakable-wrapped Ragdoll with decoded geometry must surface a joint");
        assert_eq!(r.constraints.len(), 1);
        assert!(matches!(
            r.constraints[0].kind,
            ImportedJointKind::Ragdoll { .. }
        ));
    }

    /// Sibling of the above: a breakable-wrapped type with no canonical
    /// joint kind yet (BallAndSocket = 0) still drops — #3792 only closes
    /// the gap for the four decoded types, not every wrapped type.
    #[test]
    fn breakable_wrapped_ball_and_socket_still_drops() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(bone("Bip01 Pelvis", 1)); // [0]
        scene.blocks.push(coll_obj(2)); // [1]
        scene.blocks.push(rigid_body(3)); // [2]
        scene.blocks.push(sphere(1.0)); // [3]
        scene.blocks.push(bone("Bip01 Spine", 5)); // [4]
        scene.blocks.push(coll_obj(6)); // [5]
        scene.blocks.push(rigid_body(7)); // [6]
        scene.blocks.push(sphere(1.0)); // [7]
        scene.blocks.push(Box::new(breakable_constraint(2, 6, 0))); // [8] BallAndSocket, data: Other

        assert!(
            extract_ragdoll(&scene).is_none(),
            "a breakable-wrapped type with no canonical joint kind must still drop (#1850)",
        );
    }

    /// #1832/#1874 — a `BhkRigidBody` authored with a Dynamic-family
    /// `motionType` (SPHERE/BOX_INERTIA, raw 2-5) but `mass == 0.0` must be
    /// treated as immovable world geometry (`MotionType::Static`), not a
    /// real Rapier `Dynamic` body. Confirmed live against vanilla Skyrim SE
    /// architecture (WhiterunBanneredMare): 139 of 240 successfully-parsed
    /// collision bodies in that cell match exactly this pattern — large
    /// TriMesh floor/wall/roof shapes (e.g. 256×10×256 floor tiles), raw
    /// motionType 2-5, mass=0 — which built as sleeping Dynamic bodies that
    /// free-fell the instant the player's KCC woke them by standing on the
    /// floor. This is the root cause of the TES-family (Oblivion/Skyrim)
    /// "character never grounds" bug (RT-2 / #1832), which also manifests
    /// as a ghosted-camera artifact (#1874) via the character-controller's
    /// camera-follow path never signalling a temporal discontinuity.
    #[test]
    fn zero_mass_dynamic_body_becomes_static() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(coll_obj(1)); // [0]
        scene.blocks.push(Box::new(BhkRigidBody {
            shape_ref: BlockRef(2),
            havok_filter: 0,
            is_t: false,
            translation: [0.0; 4],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 4],
            angular_velocity: [0.0; 4],
            inertia_tensor: [0.0; 12],
            center_of_mass: [0.0; 4],
            mass: 0.0,
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.3,
            restitution: 0.4,
            max_linear_velocity: 0.0,
            max_angular_velocity: 0.0,
            penetration_depth: 0.0,
            motion_type: 4, // BOX_INERTIA — Dynamic family per havok_motion_type
            deactivator_type: 0,
            solver_deactivation: 0,
            quality_type: 0,
            constraint_refs: Vec::new(),
            body_flags: 0,
        })); // [1]
        scene.blocks.push(sphere(50.0)); // [2]

        let (_, body_data) = extract_collision(&scene, BlockRef(0))
            .expect("classic BhkCollisionObject chain must resolve");
        assert_eq!(
            body_data.motion_type,
            MotionType::Static,
            "zero-mass Dynamic-family body must be reclassified as immovable"
        );
    }

    /// Sibling guard: the same Dynamic-family `motionType` with REAL
    /// authored mass (movable clutter — crates, plates, ragdoll bones)
    /// must NOT be reclassified — only the physically-nonsensical
    /// mass=0 case is special-cased.
    #[test]
    fn nonzero_mass_dynamic_body_stays_dynamic() {
        let mut scene = NifScene::default();
        scene.havok_scale = 1.0;
        scene.blocks.push(coll_obj(1)); // [0]
        scene.blocks.push(Box::new(BhkRigidBody {
            shape_ref: BlockRef(2),
            havok_filter: 0,
            is_t: false,
            translation: [0.0; 4],
            rotation: [0.0, 0.0, 0.0, 1.0],
            linear_velocity: [0.0; 4],
            angular_velocity: [0.0; 4],
            inertia_tensor: [0.0; 12],
            center_of_mass: [0.0; 4],
            mass: 5.0,
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.3,
            restitution: 0.4,
            max_linear_velocity: 0.0,
            max_angular_velocity: 0.0,
            penetration_depth: 0.0,
            motion_type: 4, // BOX_INERTIA — Dynamic family per havok_motion_type
            deactivator_type: 0,
            solver_deactivation: 0,
            quality_type: 0,
            constraint_refs: Vec::new(),
            body_flags: 0,
        })); // [1]
        scene.blocks.push(sphere(1.0)); // [2]

        let (_, body_data) = extract_collision(&scene, BlockRef(0))
            .expect("classic BhkCollisionObject chain must resolve");
        assert_eq!(
            body_data.motion_type,
            MotionType::Dynamic,
            "real authored mass must keep the body Dynamic — only mass=0 is reclassified"
        );
    }

    #[test]
    fn non_t_body_ignores_non_identity_cinfo_transform() {
        let mut scene = NifScene {
            havok_scale: 1.0,
            ..NifScene::default()
        };
        scene.blocks.push(coll_obj(1));
        scene
            .blocks
            .push(rigid_body_at(2, [10.0, 20.0, 30.0, 0.0], false));
        scene.blocks.push(sphere(1.0));

        let (shape, _) = extract_collision(&scene, BlockRef(0))
            .expect("classic non-T rigid-body chain must resolve");
        assert!(
            matches!(shape, CollisionShape::Ball { .. }),
            "plain bhkRigidBody must ignore its stored CInfo transform"
        );
    }

    #[test]
    fn t_body_applies_non_identity_cinfo_transform() {
        let mut scene = NifScene {
            havok_scale: 1.0,
            ..NifScene::default()
        };
        scene.blocks.push(coll_obj(1));
        scene
            .blocks
            .push(rigid_body_at(2, [10.0, 20.0, 30.0, 0.0], true));
        scene.blocks.push(sphere(1.0));

        let (shape, _) = extract_collision(&scene, BlockRef(0))
            .expect("classic T rigid-body chain must resolve");
        let CollisionShape::Compound { children } = shape else {
            panic!("bhkRigidBodyT must apply its active CInfo transform");
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(
            children[0].2.as_ref(),
            CollisionShape::Ball { .. }
        ));
    }
}

/// FO4+ `bhkRagdollSystem` extraction (#5015) on a synthetic scene: a root
/// node, two bone nodes, their `bhkNPCollisionObject`s and one blob.
#[cfg(test)]
mod np_ragdoll_extract_tests {
    use super::*;
    use crate::blocks::base::{NiAVObjectData, NiObjectNETData};
    use crate::blocks::node::NiNode;
    use crate::blocks::NiObject;
    use crate::types::{BlockRef, NiPoint3, NiTransform};

    fn node(name: &str, translation: [f32; 3], children: Vec<u32>) -> Box<dyn NiObject> {
        Box::new(NiNode {
            av: NiAVObjectData {
                net: NiObjectNETData {
                    name: Some(Arc::from(name)),
                    extra_data_refs: Vec::new(),
                    controller_ref: BlockRef::NULL,
                },
                flags: 0,
                transform: NiTransform {
                    translation: NiPoint3 {
                        x: translation[0],
                        y: translation[1],
                        z: translation[2],
                    },
                    ..NiTransform::default()
                },
                properties: Vec::new(),
                collision_ref: BlockRef::NULL,
            },
            children: children.into_iter().map(BlockRef).collect(),
            effects: Vec::new(),
        })
    }

    fn np_collision(target: u32, system: u32, body_id: u32) -> Box<dyn NiObject> {
        Box::new(BhkNPCollisionObject {
            target_ref: BlockRef(target),
            flags: 0,
            data_ref: BlockRef(system),
            body_id,
        })
    }

    /// `bone_b_x` is where the second bone's node sits in X; the blob's
    /// second body sits at 0.5 m = 5 units with `havok_scale` 10.
    fn scene(bone_b_x: f32, system_type: &'static str) -> NifScene {
        let mut scene = NifScene {
            havok_scale: 10.0,
            ..NifScene::default()
        };
        scene.blocks.push(node("Root", [0.0, 0.0, 10.0], vec![1, 2])); // [0]
        scene.blocks.push(node("BoneA", [0.0, 0.0, 0.0], vec![])); // [1]
        scene.blocks.push(node("BoneB", [bone_b_x, 0.0, 0.0], vec![])); // [2]
        scene.blocks.push(np_collision(1, 5, 0)); // [3]
        scene.blocks.push(np_collision(2, 5, 1)); // [4]
        scene.blocks.push(Box::new(BhkSystemBinary {
            type_name: system_type,
            data: crate::blocks::collision::synthetic_two_body_blob(),
        })); // [5]
        scene
    }

    #[test]
    fn a_ragdoll_system_blob_becomes_an_imported_ragdoll() {
        let r = extract_ragdoll(&scene(5.0, "bhkRagdollSystem")).expect("two bodies and a joint");
        assert_eq!(r.bodies.len(), 2);
        assert_eq!(&*r.bodies[0].bone_name, "BoneA");
        assert_eq!(&*r.bodies[1].bone_name, "BoneB");
        assert!((r.bodies[0].mass - 4.0).abs() < 1e-4 && (r.bodies[1].mass - 2.0).abs() < 1e-4);
        // Bodies that sit on their bones need no offset (like a plain
        // `bhkRigidBody`).
        assert!(r.bodies.iter().all(|b| !b.is_t && b.translation == Vec3::ZERO));
        assert_eq!(r.constraints.len(), 1);
        let c = &r.constraints[0];
        assert_eq!((c.body_a, c.body_b), (0, 1));
        let ImportedJointKind::Ragdoll { pivot_a, cone_max, twist_min, twist_max, .. } = c.kind else {
            panic!("a ragdoll joint")
        };
        // Pivot A is 0.5 m along Havok +X → 5 engine units along +X.
        assert!((pivot_a - Vec3::new(5.0, 0.0, 0.0)).length() < 1e-4, "{pivot_a:?}");
        assert_eq!((cone_max, twist_min, twist_max), (0.5, -0.25, 0.3));
    }

    /// A body whose bind pose is off its bone's rest transform is expressed
    /// relative to the bone (`is_t`), in engine space: bone B 2 units further
    /// along +X than the body leaves the body 2 units behind it.
    #[test]
    fn a_body_off_its_bone_gets_a_bone_local_offset() {
        let r = extract_ragdoll(&scene(7.0, "bhkRagdollSystem")).expect("ragdoll");
        assert!(!r.bodies[0].is_t, "body A still sits on its bone");
        assert!(r.bodies[1].is_t, "body B is 2 units off its bone");
        assert!(
            (r.bodies[1].translation - Vec3::new(-2.0, 0.0, 0.0)).length() < 1e-4,
            "{:?}",
            r.bodies[1].translation
        );
    }

    /// Only a `bhkRagdollSystem` is a skeleton's articulation; a
    /// `bhkPhysicsSystem` (character bumper, props) is not one.
    #[test]
    fn a_physics_system_blob_is_not_a_ragdoll() {
        assert!(extract_ragdoll(&scene(5.0, "bhkPhysicsSystem")).is_none());
    }

    /// A body no bone hosts is dropped with its joints, leaving too few for a
    /// ragdoll.
    #[test]
    fn a_body_without_a_host_bone_drops_the_ragdoll() {
        let mut s = scene(5.0, "bhkRagdollSystem");
        s.blocks[4] = np_collision(2, 5, 7); // body id 7 does not exist
        assert!(extract_ragdoll(&s).is_none());
    }
}

#[cfg(test)]
mod drop_site_diagnostics_tests {
    /// #2339 / FNV-D7-04 — every articulation-edge drop in `extract_ragdoll`
    /// must stay loud.
    ///
    /// The finding was that four drop sites `continue`d silently while two
    /// neighbours (#1539 `Other`, #1850 breakable) warned for the same class
    /// of lost edge, so an auditor reading a log saw warnings for some
    /// dropped edges and not others. All six now warn; this pins that they
    /// keep doing so.
    ///
    /// **What this does and does not prove.** It asserts each drop site still
    /// carries its diagnostic, which is exactly what a regression here looks
    /// like — someone refactors the bodies loop and a `log::warn!` goes with
    /// it. It does *not* assert the warning fires at runtime: that needs a
    /// `log` capture implementation, and this workspace has none (only
    /// `env_logger`, a writer). Adding a capture crate is a new dependency,
    /// which the project gates on explicit approval, so it is deliberately
    /// not done here for a telemetry-only LOW.
    ///
    /// A behavioural sibling would also need a synthetic `NifScene` carrying
    /// a hand-built `BhkRigidBody` (22 fields, no `Default`), a hosting
    /// `NiNode`, a resolvable shape and a `BhkConstraint` — worth writing the
    /// day this function grows real logic to test, but it would still not
    /// assert the log line, which is the thing the issue is about.
    #[test]
    fn every_ragdoll_drop_site_still_logs_its_reason() {
        const SRC: &str = include_str!("ragdoll.rs");
        let body = SRC
            .split("pub fn extract_ragdoll")
            .nth(1)
            .expect("extract_ragdoll must exist in this file");

        // One distinctive fragment per drop site. Keyed on the *reason* text
        // rather than the issue number so a reworded-but-present diagnostic
        // still passes and a deleted one fails.
        for (site, needle) in [
            ("unhosted body (#2339)", "no named NiNode hosts it"),
            (
                "shape did not resolve (#2339)",
                "did not resolve to a supported finite shape",
            ),
            ("non-finite mass (#1534/#2339)", "non-finite mass"),
            (
                "non-finite translation (#1534/#2339)",
                "non-finite translation",
            ),
            ("non-finite rotation (#1534/#2339)", "non-finite rotation"),
            (
                "unresolved constraint endpoint (#2339)",
                "do not both resolve to retained ragdoll bodies",
            ),
            ("self-linked constraint", "both endpoints resolve to bone"),
            (
                "breakable constraint (#1850)",
                "dropping bhkBreakableConstraint",
            ),
            // #4212 reworded this one: the arm now covers decoded-but-unmapped
            // kinds (ball-and-socket, stiff spring, chain) alongside undecoded
            // `Other`, and names the block and type instead of listing the two
            // types in prose. Keyed on the reason that survived the reword.
            (
                "unsupported constraint kind (#1539/#4212)",
                "no canonical joint kind exists for",
            ),
        ] {
            assert!(
                body.contains(needle),
                "extract_ragdoll lost the diagnostic for its {site} drop site \
                 (expected text containing {needle:?}). Every dropped articulation \
                 edge must stay diagnosable from the log alone — that is the whole \
                 point of #1539/#1850's warnings, and #2339 was filed because four \
                 siblings were silent."
            );
        }
    }
}
