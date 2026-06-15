//! Ragdoll activation + writeback (M41.x Phase 4).
//!
//! The NIF importer hands us an [`ImportedRagdoll`] (bone *names* +
//! joint geometry). At spawn we resolve those names against the freshly
//! loaded skeleton into a [`RagdollTemplate`] ECS component on the actor.
//! The `ragdoll <id>` console command then [`activate_ragdoll`]s it:
//! seed a [`byroredux_physics::RagdollSpec`] from each bone's *current*
//! world pose, build the Rapier multibody, and tag the actor
//! [`RagdollActive`]. Each frame [`ragdoll_writeback_system`] copies the
//! simulated body poses back onto the bone entities' `GlobalTransform`,
//! which the skinned mesh already reads — so the mesh crumples.
//!
//! Writeback runs in `Stage::Late`, after `physics_sync_system` (Physics)
//! has stepped the bodies *and* after transform propagation (PostUpdate).
//! Because it overwrites `GlobalTransform` last, no propagation/animation
//! skip is needed for slice 1: propagation's bind-pose recompute is
//! simply overwritten by the simulated pose every frame.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::components::CollisionShape;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, GlobalTransform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_nif::import::{ImportedJointKind, ImportedRagdoll};
use byroredux_physics::ragdoll::body_pose;
use byroredux_physics::{
    build_ragdoll, ContactConfig, PhysicsWorld, Ragdoll, RagdollBodySpec, RagdollConstraintSpec,
    RagdollJointSpec, RagdollSpec,
};

/// Per-actor ragdoll blueprint, resolved at spawn against the loaded
/// skeleton. Bone-local offsets + shapes + the joint graph; the world
/// seed is computed at activation from the bones' live poses.
#[derive(Debug, Clone)]
pub struct RagdollTemplate {
    pub bodies: Vec<RagdollTemplateBody>,
    pub constraints: Vec<RagdollTemplateConstraint>,
}

impl Component for RagdollTemplate {
    type Storage = SparseSetStorage<Self>;
}

#[derive(Debug, Clone)]
pub struct RagdollTemplateBody {
    /// Skeleton bone entity this body drives.
    pub bone: EntityId,
    /// Body origin offset relative to the bone (Y-up, scaled).
    pub local_translation: Vec3,
    pub local_rotation: Quat,
    pub shape: CollisionShape,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
}

#[derive(Debug, Clone)]
pub struct RagdollTemplateConstraint {
    pub body_a: usize,
    pub body_b: usize,
    pub joint: RagdollJointSpec,
}

/// Marker: this actor is currently simulating as a ragdoll.
#[derive(Debug, Clone, Copy)]
pub struct RagdollActive;

impl Component for RagdollActive {
    type Storage = SparseSetStorage<Self>;
}

/// Resolve an [`ImportedRagdoll`] (bone names) against a skeleton's
/// name→entity map into a [`RagdollTemplate`]. Bodies whose bone name
/// doesn't resolve are dropped and the constraint indices remapped;
/// returns `None` if fewer than 2 bodies or no joints survive.
pub fn template_from_imported(
    imported: &ImportedRagdoll,
    skel_map: &HashMap<Arc<str>, EntityId>,
) -> Option<RagdollTemplate> {
    let mut bodies = Vec::with_capacity(imported.bodies.len());
    let mut old_to_new: Vec<Option<usize>> = vec![None; imported.bodies.len()];
    for (i, b) in imported.bodies.iter().enumerate() {
        let Some(&bone) = skel_map.get(&b.bone_name) else {
            continue;
        };
        old_to_new[i] = Some(bodies.len());
        bodies.push(RagdollTemplateBody {
            bone,
            local_translation: b.translation,
            local_rotation: b.rotation,
            shape: b.shape.clone(),
            mass: b.mass,
            linear_damping: b.linear_damping,
            angular_damping: b.angular_damping,
            friction: b.friction,
            restitution: b.restitution,
        });
    }
    if bodies.len() < 2 {
        return None;
    }
    let mut constraints = Vec::new();
    for c in &imported.constraints {
        let (Some(a), Some(b)) = (old_to_new[c.body_a], old_to_new[c.body_b]) else {
            continue;
        };
        constraints.push(RagdollTemplateConstraint {
            body_a: a,
            body_b: b,
            joint: joint_from_imported(&c.kind),
        });
    }
    if constraints.is_empty() {
        return None;
    }
    Some(RagdollTemplate { bodies, constraints })
}

fn joint_from_imported(k: &ImportedJointKind) -> RagdollJointSpec {
    match k {
        ImportedJointKind::Ragdoll {
            twist_a,
            plane_a,
            pivot_a,
            twist_b,
            plane_b,
            pivot_b,
            cone_max,
            twist_min,
            twist_max,
            ..
        } => RagdollJointSpec::Ragdoll {
            twist_a: *twist_a,
            plane_a: *plane_a,
            pivot_a: *pivot_a,
            twist_b: *twist_b,
            plane_b: *plane_b,
            pivot_b: *pivot_b,
            cone_max: *cone_max,
            twist_min: *twist_min,
            twist_max: *twist_max,
        },
        ImportedJointKind::LimitedHinge {
            axis_a,
            pivot_a,
            axis_b,
            pivot_b,
            min_angle,
            max_angle,
        } => RagdollJointSpec::LimitedHinge {
            axis_a: *axis_a,
            pivot_a: *pivot_a,
            axis_b: *axis_b,
            pivot_b: *pivot_b,
            min_angle: *min_angle,
            max_angle: *max_angle,
        },
    }
}

/// Flip `actor` from animated/bind-pose to a live Rapier ragdoll. Reads
/// the actor's [`RagdollTemplate`], seeds each body from its bone's
/// current `GlobalTransform`, builds the multibody, and attaches
/// [`Ragdoll`] + [`RagdollActive`]. Returns the body count on success.
pub fn activate_ragdoll(world: &World, actor: EntityId) -> Result<usize, String> {
    // 1. Build the world-seeded spec while holding the read guards, then
    //    drop them before taking the PhysicsWorld write lock.
    let spec = {
        let tq = world
            .query::<RagdollTemplate>()
            .ok_or("RagdollTemplate storage not registered")?;
        let template = tq
            .get(actor)
            .ok_or_else(|| format!("entity {actor} has no RagdollTemplate"))?;
        let gtq = world
            .query::<GlobalTransform>()
            .ok_or("GlobalTransform storage not registered")?;

        let mut bodies = Vec::with_capacity(template.bodies.len());
        for b in &template.bodies {
            let gt = gtq
                .get(b.bone)
                .ok_or_else(|| format!("ragdoll bone {} has no GlobalTransform", b.bone))?;
            // World seed = bone global ∘ body-local offset.
            let translation = gt.translation + gt.rotation * (b.local_translation * gt.scale);
            let rotation = gt.rotation * b.local_rotation;
            bodies.push(RagdollBodySpec {
                entity: b.bone,
                translation,
                rotation,
                shape: b.shape.clone(),
                mass: b.mass,
                linear_damping: b.linear_damping,
                angular_damping: b.angular_damping,
                friction: b.friction,
                restitution: b.restitution,
            });
        }
        let constraints = template
            .constraints
            .iter()
            .map(|c| RagdollConstraintSpec {
                body_a: c.body_a,
                body_b: c.body_b,
                joint: c.joint.clone(),
            })
            .collect();
        RagdollSpec { bodies, constraints }
    };

    // 2. Build the Rapier multibody (read the live tuning config; copy out
    //    so no guard is held across the PhysicsWorld write lock).
    let cfg = world
        .try_resource::<ContactConfig>()
        .map(|c| *c)
        .unwrap_or(ContactConfig::DEFAULT);
    let ragdoll = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        build_ragdoll(&mut pw, &spec, &cfg)
    };
    let n = ragdoll.bodies.len();

    // 3. Tag the actor.
    world
        .query_mut::<Ragdoll>()
        .ok_or("Ragdoll storage not registered")?
        .insert(actor, ragdoll);
    world
        .query_mut::<RagdollActive>()
        .ok_or("RagdollActive storage not registered")?
        .insert(actor, RagdollActive);

    Ok(n)
}

/// Per-frame: copy each active ragdoll's simulated body poses onto the
/// bone entities' `GlobalTransform`. Register in `Stage::Late` (after
/// `physics_sync_system` steps the sim). Only the rotation + translation
/// are written; the bone's `GlobalTransform.scale` is preserved.
pub fn ragdoll_writeback_system(world: &World, _dt: f32) {
    let Some(pw) = world.try_resource::<PhysicsWorld>() else {
        return;
    };
    let Some(rq) = world.query::<Ragdoll>() else {
        return;
    };
    let Some(tq) = world.query::<RagdollTemplate>() else {
        return;
    };
    let Some(mut gtq) = world.query_mut::<GlobalTransform>() else {
        return;
    };
    for (actor, ragdoll) in rq.iter() {
        // The seed (activate_ragdoll) composed the body world pose as
        // body = bone ∘ body-local: `body_t = bone_t + bone_r * (local_t *
        // scale)`, `body_r = bone_r * local_r`. Invert that here so the
        // *bone* pose lands on GlobalTransform, not the body origin — bodies
        // authored as bhkRigidBodyT carry a non-zero local offset, and
        // writing the raw body pose displaced the skinned mesh. #1616.
        let Some(template) = tq.get(actor) else {
            continue;
        };
        for ((bone, handle), tb) in ragdoll.bodies.iter().zip(template.bodies.iter()) {
            let Some((t, r)) = body_pose(&pw, *handle) else {
                continue;
            };
            // #1534 belt-and-suspenders: never let a non-finite simulated
            // pose (a solver that went unstable despite the import-side
            // finite guards) reach `GlobalTransform` → bone palette → GPU
            // skinning, where a NaN vertex is UB and NaN pixels stick through
            // SVGF/TAA history. Skip the bone this frame; it holds its last
            // good pose.
            if !t.is_finite() || !r.is_finite() {
                continue;
            }
            if let Some(gt) = gtq.get_mut(*bone) {
                // bone_rotation = body_rotation * local_rotation⁻¹
                let bone_rotation = r * tb.local_rotation.inverse();
                // bone_translation = body_translation
                //                  - bone_rotation * (local_translation * scale)
                gt.rotation = bone_rotation;
                gt.translation = t - bone_rotation * (tb.local_translation * gt.scale);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::Transform;
    use byroredux_physics::world::PHYSICS_DT;

    /// Full headless flow: a synthetic skeleton (root + 3 hanging bones) +
    /// a RagdollTemplate → `activate_ragdoll` → step → `ragdoll_writeback`
    /// moves the bone `GlobalTransform`s under gravity while keeping them
    /// jointed. Exercises every Phase-4 logic path without a GPU.
    #[test]
    fn activate_then_writeback_moves_bones() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        let actor = world.spawn();
        // Three bones in a horizontal row at y=1000, all upright.
        let mut bones = Vec::new();
        for i in 0..3 {
            let e = world.spawn();
            world.insert(
                e,
                GlobalTransform {
                    translation: Vec3::new(i as f32 * 50.0, 1000.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: 1.0,
                },
            );
            bones.push(e);
        }

        let joint = |_a: usize, _b: usize| RagdollJointSpec::Ragdoll {
            twist_a: Vec3::X,
            plane_a: Vec3::Y,
            pivot_a: Vec3::new(25.0, 0.0, 0.0),
            twist_b: Vec3::X,
            plane_b: Vec3::Y,
            pivot_b: Vec3::new(-25.0, 0.0, 0.0),
            cone_max: std::f32::consts::PI,
            twist_min: -std::f32::consts::PI,
            twist_max: std::f32::consts::PI,
        };
        let template = RagdollTemplate {
            bodies: bones
                .iter()
                .map(|&bone| RagdollTemplateBody {
                    bone,
                    local_translation: Vec3::ZERO,
                    local_rotation: Quat::IDENTITY,
                    shape: CollisionShape::Ball { radius: 5.0 },
                    mass: 4.0,
                    linear_damping: 0.05,
                    angular_damping: 0.05,
                    friction: 0.5,
                    restitution: 0.0,
                })
                .collect(),
            constraints: vec![
                RagdollTemplateConstraint {
                    body_a: 0,
                    body_b: 1,
                    joint: joint(0, 1),
                },
                RagdollTemplateConstraint {
                    body_a: 1,
                    body_b: 2,
                    joint: joint(1, 2),
                },
            ],
        };
        world.insert(actor, template);

        let n = activate_ragdoll(&world, actor).expect("activation should succeed");
        assert_eq!(n, 3);
        assert!(
            world.query::<RagdollActive>().unwrap().get(actor).is_some(),
            "actor must be tagged RagdollActive"
        );

        let far_bone = bones[2];
        let init_y = bone_y(&world, far_bone);

        // Step the sim + run writeback each frame. With no floor the chain
        // falls under gravity; the writeback must propagate that onto the
        // bone GlobalTransforms (joints-hold is covered by the physics-crate
        // chain test). 120 frames ≈ 2 s.
        for _ in 0..120 {
            {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                pw.step(PHYSICS_DT);
            }
            ragdoll_writeback_system(&world, PHYSICS_DT);
        }

        let end = world
            .query::<GlobalTransform>()
            .unwrap()
            .get(far_bone)
            .unwrap()
            .translation;
        assert!(end.is_finite(), "writeback produced non-finite pose");
        assert!(
            end.y < init_y - 1.0,
            "writeback should move the bone down under gravity: {init_y} → {}",
            end.y
        );
    }

    /// #1616 — seed a single body with a NON-zero body-local offset, then run
    /// writeback with no physics step. The seed composed body = bone ∘ local;
    /// the writeback must invert that, so the bone `GlobalTransform` round-trips
    /// back to its original pose. Pre-fix the writeback wrote the raw body
    /// pose (bone + offset), displacing the bone by the offset every frame.
    #[test]
    fn writeback_inverts_body_local_offset_round_trip() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        let actor = world.spawn();
        let bone = world.spawn();
        let orig = GlobalTransform {
            translation: Vec3::new(100.0, 200.0, 300.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            scale: 1.0,
        };
        world.insert(bone, orig);

        let template = RagdollTemplate {
            bodies: vec![RagdollTemplateBody {
                bone,
                // Non-zero offset on BOTH translation and rotation.
                local_translation: Vec3::new(5.0, -10.0, 2.0),
                local_rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_6),
                shape: CollisionShape::Ball { radius: 5.0 },
                mass: 4.0,
                linear_damping: 0.05,
                angular_damping: 0.05,
                friction: 0.5,
                restitution: 0.0,
            }],
            constraints: Vec::new(),
        };
        world.insert(actor, template);

        activate_ragdoll(&world, actor).expect("activation should succeed");
        // No physics step — the body sits at its seeded pose, so the inverse
        // must recover the original bone pose exactly (modulo float epsilon).
        ragdoll_writeback_system(&world, 0.0);

        let gt = *world
            .query::<GlobalTransform>()
            .unwrap()
            .get(bone)
            .unwrap();
        assert!(
            (gt.translation - orig.translation).length() < 1e-2,
            "bone translation must round-trip: {:?} vs {:?}",
            gt.translation,
            orig.translation
        );
        // Quaternion proximity via |dot| ≈ 1.
        assert!(
            gt.rotation.dot(orig.rotation).abs() > 1.0 - 1e-3,
            "bone rotation must round-trip: {:?} vs {:?}",
            gt.rotation,
            orig.rotation
        );
    }

    fn bone_y(world: &World, bone: EntityId) -> f32 {
        world
            .query::<GlobalTransform>()
            .unwrap()
            .get(bone)
            .unwrap()
            .translation
            .y
    }
}
