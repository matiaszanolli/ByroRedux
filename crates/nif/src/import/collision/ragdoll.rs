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

use byroredux_core::math::Vec3;

use super::shape::resolve_shape;
use super::{finite, finite_vec, havok_quat_to_engine, havok_to_engine};

/// Scene-level extractor: assemble the Havok ragdoll articulation (the
/// rigid bodies + the constraints linking them) into an engine-native
/// [`ImportedRagdoll`]. Returns `None` unless the scene carries a real
/// articulation — ≥2 bodies and ≥1 decoded Ragdoll/LimitedHinge joint;
/// a lone static collider isn't a ragdoll.
///
/// Bodies are reached via the classic `BhkCollisionObject → BhkRigidBody`
/// chain (the only decodable path; FO4+ NP-blob ragdolls are out of
/// scope — see the module table). Each body's bone name is its host
/// NiNode's name. Constraint entity refs are rigid-body *block* indices,
/// remapped here to body-array indices. All geometry is Y-up + havok-
/// scaled, reusing the same helpers as [`extract_from_classic`].
pub fn extract_ragdoll(scene: &NifScene) -> Option<ImportedRagdoll> {
    let scale = scene.havok_scale;
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
            continue;
        };
        let mut visited = HashSet::new();
        let Some(shape) = resolve_shape(scene, body.shape_ref, &mut visited) else {
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
            continue;
        };
        let Some(translation) = finite_vec(
            havok_to_engine(
                body.translation[0],
                body.translation[1],
                body.translation[2],
            ) * scale,
        ) else {
            continue;
        };
        let rotation = havok_quat_to_engine(body.rotation);
        if !rotation.is_finite() {
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
        });
    }
    if bodies.len() < 2 {
        return None;
    }

    let mut constraints = Vec::new();
    for block in scene.blocks.iter() {
        let Some(c) = block.as_any().downcast_ref::<BhkConstraint>() else {
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
            continue;
        };
        if body_a == body_b {
            continue; // a joint must link two distinct bodies
        }
        // A non-finite limit angle / pivot / axis (#1534) drops the joint —
        // `[NaN, NaN]` limits handed to the Rapier solver destabilize it.
        let kind = match &c.data {
            BhkConstraintData::Ragdoll(r) => ragdoll_joint(r, scale),
            BhkConstraintData::LimitedHinge(h) => limited_hinge_joint(h, scale),
            // #1539 — `bhkHingeConstraint` / `bhkBallAndSocketConstraint` /
            // `bhkPrismaticConstraint` / `bhkStiffSpringConstraint` all decode
            // to `Other`. Dropping one that links two ragdoll bones silently
            // disconnects the articulation: `orient_tree`
            // (`crates/physics/src/ragdoll.rs`) then yields a forest and
            // `build_ragdoll` builds the detached limb as an independent
            // free-floating multibody that free-falls. Every other block-drop
            // in this file logs (the FO4-NP / phantom arms `log::debug!`);
            // this one warns — louder, because unlike those benign
            // out-of-scope drops it can visibly break the ragdoll. (Long-term:
            // map a limitless hinge to `LimitedHinge { min: -PI, max: PI }`.)
            BhkConstraintData::Other => {
                log::warn!(
                    "extract_ragdoll: dropping unsupported constraint linking bones \
                     '{a}' <-> '{b}' — decoded as Other (bhkHinge / bhkBallAndSocket / \
                     bhkPrismatic / bhkStiffSpring not yet mapped to a canonical joint). \
                     The ragdoll edge is lost; if it was the sole link to a limb, that \
                     limb will detach and free-fall (#1539).",
                    a = bodies[body_a].bone_name,
                    b = bodies[body_b].bone_name,
                );
                continue;
            }
        };
        let Some(kind) = kind else { continue };
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
fn limited_hinge_joint(h: &LimitedHingeCInfo, scale: f32) -> Option<ImportedJointKind> {
    Some(ImportedJointKind::LimitedHinge {
        axis_a: finite_vec(havok_dir_to_engine(h.axis_a))?,
        pivot_a: finite_vec(havok_to_engine(h.pivot_a[0], h.pivot_a[1], h.pivot_a[2]) * scale)?,
        axis_b: finite_vec(havok_dir_to_engine(h.axis_b))?,
        pivot_b: finite_vec(havok_to_engine(h.pivot_b[0], h.pivot_b[1], h.pivot_b[2]) * scale)?,
        min_angle: finite(h.min_angle)?,
        max_angle: finite(h.max_angle)?,
    })
}

#[cfg(test)]
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
        BhkCollisionObject, BhkConstraint, BhkConstraintData, BhkRigidBody, BhkSphereShape,
        RagdollCInfo,
    };
    use crate::blocks::node::NiNode;
    use crate::blocks::NiObject;
    use crate::types::{BlockRef, NiTransform};
    use byroredux_core::ecs::components::collision::MotionType;

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
    fn rigid_body_at(shape_ref: usize, translation: [f32; 4]) -> Box<dyn NiObject> {
        Box::new(BhkRigidBody {
            shape_ref: BlockRef(shape_ref as u32),
            havok_filter: 0,
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
            .push(rigid_body_at(3, [f32::NAN, 0.0, 0.0, 0.0])); // [2]
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
}
