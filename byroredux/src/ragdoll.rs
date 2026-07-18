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

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use byroredux_core::ecs::components::{CollisionShape, RigidBodyData};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{Children, EntityId, GlobalTransform, Parent, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_nif::import::{ImportedJointKind, ImportedRagdoll};
use byroredux_physics::ragdoll::body_pose;
use byroredux_physics::{
    build_ragdoll, ContactConfig, PhysicsWorld, Ragdoll, RagdollBodySpec, RagdollConstraintSpec,
    RagdollJointSpec, RagdollSpec, RapierHandles,
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
    // #1718 / FNV-D7-01 — collect dropped-body bone names so a skeleton
    // whose bone naming diverges from the ragdoll's authored names (variant
    // skeleton, renamed bone, importer canonicalisation mismatch) leaves a
    // breadcrumb instead of silently degrading/vanishing.
    let mut dropped_bones: Vec<&Arc<str>> = Vec::new();
    for (i, b) in imported.bodies.iter().enumerate() {
        let Some(&bone) = skel_map.get(&b.bone_name) else {
            dropped_bones.push(&b.bone_name);
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
    if !dropped_bones.is_empty() {
        log::warn!(
            "template_from_imported: {} ragdoll body/bodies dropped — bone name(s) not found \
             in skeleton: {:?}",
            dropped_bones.len(),
            dropped_bones,
        );
    }
    if bodies.len() < 2 {
        return None;
    }
    let mut constraints = Vec::new();
    let mut dropped_constraint_bones: Vec<(&Arc<str>, &Arc<str>)> = Vec::new();
    for c in &imported.constraints {
        let (Some(a), Some(b)) = (old_to_new[c.body_a], old_to_new[c.body_b]) else {
            dropped_constraint_bones.push((
                &imported.bodies[c.body_a].bone_name,
                &imported.bodies[c.body_b].bone_name,
            ));
            continue;
        };
        constraints.push(RagdollTemplateConstraint {
            body_a: a,
            body_b: b,
            joint: joint_from_imported(&c.kind),
        });
    }
    if !dropped_constraint_bones.is_empty() {
        // Mirrors the sibling drop-site diagnostic in
        // `crates/nif/src/import/collision.rs::extract_ragdoll` (#1539) —
        // same "dropping ... linking bones 'a' <-> 'b'" phrasing so both
        // ragdoll-fragmentation drop sites read as one unified telemetry
        // stream.
        for (a, b) in &dropped_constraint_bones {
            log::warn!(
                "template_from_imported: dropping constraint linking bones '{a}' <-> '{b}' \
                 — endpoint body's bone name was not found in the skeleton. The ragdoll edge \
                 is lost; if it was the sole link to a limb, that limb will detach and \
                 free-fall (#1718).",
            );
        }
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
                // #1852 — snapshot the seed-time scale so the writeback
                // inverse decomposes with the same value this was composed
                // with, regardless of any later live GlobalTransform.scale
                // mutation.
                scale: gt.scale,
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

    // 1.5. #2083 — capture any ragdoll from a prior activation of this actor.
    //    Re-activating (e.g. a second `ragdoll <id>`) rebuilt a fresh Rapier
    //    body/joint set unconditionally and `insert`ed it, overwriting the
    //    `Ragdoll` component without freeing the old handles: the orphaned
    //    first set (~18 bodies + ~17 joints for a humanoid) stayed in the
    //    solver forever, still simulating at its last pose and fighting the
    //    new multibody. Read-then-drop, matching the two-phase discipline
    //    used everywhere else here: no component read guard held across the
    //    PhysicsWorld write lock below.
    let old_ragdoll = world.query::<Ragdoll>().and_then(|q| q.get(actor).cloned());

    // 2. Build the Rapier multibody (read the live tuning config; copy out
    //    so no guard is held across the PhysicsWorld write lock).
    let cfg = world
        .try_resource::<ContactConfig>()
        .map(|c| *c)
        .unwrap_or(ContactConfig::DEFAULT);
    let ragdoll = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        if let Some(old) = &old_ragdoll {
            pw.remove_ragdoll(old);
        }
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

    // 4. #1772 — tear down each ragdolled bone's pre-existing keyframed
    //    collision body. At NPC spawn every ragdoll bone got a Keyframed
    //    `RigidBodyData` → kinematic Rapier follower body (`RapierHandles`,
    //    `keyframe_live_ragdoll_bones` + `physics_sync_system`). Left in place
    //    after activation those bodies (a) collide with the dynamic ragdoll
    //    bodies now occupying the same bones — kinematic-vs-dynamic contacts
    //    that fight the multibody solver — and (b) get re-driven every frame by
    //    `push_kinematic` chasing the writeback-updated `GlobalTransform`. Free
    //    the Rapier body and drop BOTH `RigidBodyData` (else `collect_newcomers`
    //    re-registers the bone next frame) and `RapierHandles`. The dynamic
    //    ragdoll bodies are the bones' physics representation from here on.
    //    Two-phase: collect handles under the read guard, then free + remove
    //    after it drops (no read guard across the PhysicsWorld write lock).
    let bone_handles: Vec<(EntityId, RapierHandles)> = match world.query::<RapierHandles>() {
        Some(hq) => spec
            .bodies
            .iter()
            .filter_map(|b| hq.get(b.entity).map(|h| (b.entity, *h)))
            .collect(),
        None => Vec::new(),
    };
    if !bone_handles.is_empty() {
        {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            for (_bone, h) in &bone_handles {
                pw.remove_body(h.body);
            }
        }
        if let Some(mut rbq) = world.query_mut::<RigidBodyData>() {
            for (bone, _) in &bone_handles {
                rbq.remove(*bone);
            }
        }
        if let Some(mut hq) = world.query_mut::<RapierHandles>() {
            for (bone, _) in &bone_handles {
                hq.remove(*bone);
            }
        }
    }

    Ok(n)
}

/// Per-frame: copy each active ragdoll's simulated body poses onto the
/// bone entities' `GlobalTransform`. Register in `Stage::Late` (after
/// `physics_sync_system` steps the sim). Only the rotation + translation
/// are written; the bone's `GlobalTransform.scale` is preserved.
///
/// After the body poses land, a localized transform propagation re-derives
/// every **non-body descendant** bone's `GlobalTransform` from its now-
/// simulated parent (`parent_global ∘ local`). Without it, bones that hang
/// under a ragdoll body but are not themselves bodies — on the FNV skeleton
/// the finger bones (children of `Bip01 [LR] Hand`) and the toes — keep the
/// `animated_parent_global ∘ local` pose that PostUpdate propagation left on
/// them (the *animated* parent, computed before writeback overwrote it), so
/// they float detached at the pre-ragdoll pose while the body crumples
/// (FNV-D7-01 / #1979). This is the "option 1" fix from the issue: a subtree
/// re-derivation in the same `Stage::Late` write, self-contained and with no
/// dependency on gating the (still-running) animation system.
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
    // Hierarchy + local-pose reads for the descendant re-derivation pass.
    // Absent (a flat skeleton with neither Parent nor Children) the pass is a
    // no-op and only the body writeback runs. Scratch reused across actors.
    let parent_q = world.query::<Parent>();
    let children_q = world.query::<Children>();
    let transform_q = world.query::<Transform>();
    let mut body_bones: HashSet<EntityId> = HashSet::new();
    let mut queue: VecDeque<EntityId> = VecDeque::new();
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
        for ((bone, handle, seed_scale), tb) in ragdoll.bodies.iter().zip(template.bodies.iter()) {
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
                //
                // #1852 — decompose with `seed_scale` (the value the seed
                // in `activate_ragdoll` composed `translation` with), NOT a
                // fresh `gt.scale` read. If the bone's live scale changed
                // since activation, using it here would de-compose against
                // a different scale than the seed used, displacing the
                // bone by `local_translation * Δscale`.
                gt.rotation = bone_rotation;
                gt.translation = t - bone_rotation * (tb.local_translation * *seed_scale);
            }
        }

        // ── #1979 — re-derive non-body descendants from the simulated pose ──
        //
        // The body loop above wrote GlobalTransform on the ragdoll bones only.
        // Any bone hanging under a body but not itself a body (fingers, toes)
        // still holds the pose PostUpdate propagation computed from the
        // *animated* parent, so it's detached from the crumpling body. Walk the
        // body bones' descendants BFS and recompose each non-body bone from its
        // parent's (now-simulated, or already-re-derived) global. Requires both
        // Parent (to find each node's parent global) and Children (to enqueue
        // grandchildren) — a flat skeleton with neither is already final.
        let (Some(pq), Some(cq)) = (parent_q.as_ref(), children_q.as_ref()) else {
            continue;
        };
        body_bones.clear();
        body_bones.extend(template.bodies.iter().map(|b| b.bone));
        // Seed with the children of every body bone; body bones themselves keep
        // their simulated global (authoritative) and are only recursed through.
        queue.clear();
        for tb in &template.bodies {
            if let Some(children) = cq.get(tb.bone) {
                queue.extend(children.0.iter().copied());
            }
        }
        while let Some(entity) = queue.pop_front() {
            // A descendant that is itself a body keeps its simulated pose; do
            // not overwrite it, but still walk through to its own children.
            if !body_bones.contains(&entity) {
                let Some(parent) = pq.get(entity) else {
                    continue;
                };
                // Copy the parent global out first (BFS guarantees the parent
                // is already final: a body from the writeback loop, or a
                // non-body re-derived earlier in this walk).
                let Some(parent_global) = gtq.get_mut(parent.0).map(|g| *g) else {
                    continue;
                };
                let local = transform_q
                    .as_ref()
                    .and_then(|tq| tq.get(entity).copied())
                    .unwrap_or(Transform::IDENTITY);
                let composed = GlobalTransform::compose(
                    &parent_global,
                    local.translation,
                    local.rotation,
                    local.scale,
                );
                if let Some(g) = gtq.get_mut(entity) {
                    *g = composed;
                }
            }
            if let Some(children) = cq.get(entity) {
                queue.extend(children.0.iter().copied());
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

    /// #2083 — activating an already-active ragdoll must free the previous
    /// body/joint set, not leak it. Pre-fix, a second `activate_ragdoll` on
    /// the same actor built a fresh Rapier multibody and overwrote the
    /// `Ragdoll` component without calling `remove_ragdoll` on the old one,
    /// so `PhysicsWorld::body_count` grew by a full ragdoll's worth on every
    /// re-activation. Same 3-bone template as `activate_then_writeback_moves_bones`.
    #[test]
    fn reactivating_ragdoll_does_not_leak_previous_bodies() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        let actor = world.spawn();
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

        let n = activate_ragdoll(&world, actor).expect("first activation should succeed");
        assert_eq!(n, 3);
        let count_after_first = world.resource::<PhysicsWorld>().body_count();

        // Re-activate the same actor (e.g. a second `ragdoll <id>` hit) —
        // the body count must NOT grow: the old set is freed before the new
        // one is built.
        let n2 = activate_ragdoll(&world, actor).expect("re-activation should succeed");
        assert_eq!(n2, 3);
        let count_after_second = world.resource::<PhysicsWorld>().body_count();
        assert_eq!(
            count_after_first, count_after_second,
            "re-activating a ragdoll must not leak the previous body set \
             (first={count_after_first}, second={count_after_second})"
        );

        // Exactly one `Ragdoll` component remains attached, and it references
        // the newly-built bodies (not the freed ones).
        let ragdoll = world.query::<Ragdoll>().unwrap().get(actor).unwrap().clone();
        assert_eq!(ragdoll.bodies.len(), 3);
        assert!(
            world.query::<RagdollActive>().unwrap().get(actor).is_some(),
            "actor must still be tagged RagdollActive after re-activation"
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

    /// #1852 — seed a body with a non-uniform-vs-later `GlobalTransform.scale`
    /// (2.0 at activation) and a non-zero body-local offset, then MUTATE the
    /// bone's live scale to a different value (1.0) before running writeback
    /// with no physics step. Pre-fix the writeback inverse re-read the live
    /// (now-mutated) `gt.scale`, decomposing against the wrong value and
    /// displacing the bone by `local_translation * Δscale`. Post-fix the
    /// snapshotted `RagdollBodySpec::scale` (2.0, taken at activation) is used
    /// instead, so the bone still round-trips back to its original pose
    /// despite the live scale having changed underneath it.
    #[test]
    fn writeback_uses_seed_time_scale_not_live_scale_after_mutation() {
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
            scale: 2.0,
        };
        world.insert(bone, orig);

        let template = RagdollTemplate {
            bodies: vec![RagdollTemplateBody {
                bone,
                // Non-zero offset — the term the wrong scale would corrupt.
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

        // Mutate the bone's live GlobalTransform.scale AFTER activation —
        // simulates a gameplay system (shrink/enlarge FX) rescaling an
        // active ragdoll bone mid-sim. The seed already composed the body
        // pose using scale=2.0; only the snapshot should be used on the way
        // back, not this new live value.
        {
            let mut gtq = world.query_mut::<GlobalTransform>().unwrap();
            gtq.get_mut(bone).unwrap().scale = 1.0;
        }

        // No physics step — the body sits at its seeded pose, so the
        // inverse must recover the ORIGINAL bone pose exactly (modulo float
        // epsilon), despite the live scale mutation above.
        ragdoll_writeback_system(&world, 0.0);

        let gt = *world
            .query::<GlobalTransform>()
            .unwrap()
            .get(bone)
            .unwrap();
        assert!(
            (gt.translation - orig.translation).length() < 1e-2,
            "bone translation must round-trip using the seed-time scale, not \
             the mutated live scale: {:?} vs {:?} (#1852)",
            gt.translation,
            orig.translation
        );
        assert!(
            gt.rotation.dot(orig.rotation).abs() > 1.0 - 1e-3,
            "bone rotation must round-trip: {:?} vs {:?}",
            gt.rotation,
            orig.rotation
        );
    }

    /// #1979 — a bone that hangs under a ragdoll body but is NOT itself a body
    /// (fingers, toes) must follow the simulated parent after writeback, not
    /// float at the pre-ragdoll animated pose. Build `hand` (a body) with a
    /// non-body child `finger`; ragdoll + fall the hand under gravity; assert
    /// the finger's `GlobalTransform` is exactly `hand_global ∘ finger_local`
    /// (so it tracks the crumpling hand) and that it actually moved with it.
    /// Pre-fix the writeback touched only body bones, so `finger` kept its
    /// standing global and detached from the fallen hand.
    #[test]
    fn writeback_rederives_non_body_descendant_from_simulated_parent() {
        use byroredux_core::ecs::{Children, Parent};
        use byroredux_physics::world::PHYSICS_DT;

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Parent>();
        world.register::<Children>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        let actor = world.spawn();

        // `hand` is a ragdoll body at (0, 1000, 0).
        let hand = world.spawn();
        world.insert(hand, Transform::IDENTITY);
        world.insert(
            hand,
            GlobalTransform {
                translation: Vec3::new(0.0, 1000.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
        );

        // `finger` hangs off `hand` with a +X local offset. Its initial global
        // is the correct standing placement (hand ∘ local); the bug is that it
        // stays there while the hand falls.
        let finger = world.spawn();
        let finger_local = Transform::new(Vec3::new(3.0, 0.0, 0.0), Quat::IDENTITY, 1.0);
        world.insert(finger, finger_local);
        world.insert(
            finger,
            GlobalTransform {
                translation: Vec3::new(3.0, 1000.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
        );
        world.insert(finger, Parent(hand));
        world.insert(hand, Children(vec![finger]));

        // A second body so the physics multibody is well-formed; jointless is
        // fine for a free fall (matches `writeback_inverts_body_local_offset`).
        let elbow = world.spawn();
        world.insert(
            elbow,
            GlobalTransform {
                translation: Vec3::new(-50.0, 1000.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
        );

        let body = |bone| RagdollTemplateBody {
            bone,
            local_translation: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            shape: CollisionShape::Ball { radius: 5.0 },
            mass: 4.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            friction: 0.5,
            restitution: 0.0,
        };
        world.insert(
            actor,
            RagdollTemplate {
                bodies: vec![body(hand), body(elbow)],
                constraints: Vec::new(),
            },
        );

        activate_ragdoll(&world, actor).expect("activation should succeed");

        let finger_init_y = 1000.0_f32;
        for _ in 0..120 {
            {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                pw.step(PHYSICS_DT);
            }
            ragdoll_writeback_system(&world, PHYSICS_DT);
        }

        let gq = world.query::<GlobalTransform>().unwrap();
        let gh = *gq.get(hand).unwrap();
        let gf = *gq.get(finger).unwrap();

        // The hand fell (parent is simulated, not static — test is meaningful).
        assert!(
            gh.translation.y < 1000.0 - 1.0,
            "hand body should have fallen under gravity: {}",
            gh.translation.y
        );
        // The finger tracks the simulated hand: global == hand ∘ finger_local.
        let expected = GlobalTransform::compose(
            &gh,
            finger_local.translation,
            finger_local.rotation,
            finger_local.scale,
        );
        assert!(
            (gf.translation - expected.translation).length() < 1e-3,
            "finger global must be re-derived from the simulated hand: {:?} vs {:?}",
            gf.translation,
            expected.translation
        );
        assert!(
            gf.rotation.dot(expected.rotation).abs() > 1.0 - 1e-4,
            "finger rotation must follow the simulated hand",
        );
        // And it actually moved down with the hand (pre-fix it stayed at 1000).
        assert!(
            gf.translation.y < finger_init_y - 1.0,
            "finger must move down with the hand, not float at the standing pose: {}",
            gf.translation.y
        );
    }

    /// #1772 — at NPC spawn each ragdoll bone carries a Keyframed
    /// `RigidBodyData` that `physics_sync_system` registers as a kinematic
    /// Rapier follower body. `activate_ragdoll` must tear those down: left in
    /// place they collide with the dynamic ragdoll bodies now on the same
    /// bones (kinematic-vs-dynamic contacts fight the solver) and keep being
    /// driven by `push_kinematic`. Assert each bone's `RigidBodyData` +
    /// `RapierHandles` are gone post-activation AND a re-run of
    /// `physics_sync_system` does NOT re-register them (dropping `RigidBodyData`
    /// is what stops `collect_newcomers` recreating the follower).
    #[test]
    fn activation_tears_down_keyframed_bone_bodies() {
        use byroredux_core::ecs::components::MotionType;
        use byroredux_physics::physics_sync_system;

        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<CollisionShape>();
        world.register::<RigidBodyData>();
        world.register::<RapierHandles>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        let actor = world.spawn();
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
            world.insert(e, CollisionShape::Ball { radius: 5.0 });
            // Exactly what keyframe_live_ragdoll_bones leaves on each bone.
            world.insert(
                e,
                RigidBodyData {
                    motion_type: MotionType::Keyframed,
                    ..Default::default()
                },
            );
            bones.push(e);
        }
        // Phase 1 of the sim registers the keyframed follower bodies.
        physics_sync_system(&world, PHYSICS_DT);
        for &b in &bones {
            assert!(
                world.query::<RapierHandles>().unwrap().get(b).is_some(),
                "each bone must register a kinematic follower body before activation",
            );
        }
        assert_eq!(
            world.resource::<PhysicsWorld>().body_count(),
            3,
            "3 keyframed follower bodies registered before activation",
        );

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
            constraints: Vec::new(),
        };
        world.insert(actor, template);

        let n = activate_ragdoll(&world, actor).expect("activation should succeed");
        assert_eq!(n, 3);

        for &b in &bones {
            assert!(
                world.query::<RigidBodyData>().unwrap().get(b).is_none(),
                "keyframed RigidBodyData must be removed on activation",
            );
            assert!(
                world.query::<RapierHandles>().unwrap().get(b).is_none(),
                "keyframed RapierHandles must be removed on activation",
            );
        }
        // 3 keyframed followers freed; the 3 dynamic ragdoll bodies remain.
        assert_eq!(
            world.resource::<PhysicsWorld>().body_count(),
            3,
            "keyframed followers freed, only the 3 dynamic ragdoll bodies remain",
        );

        // Re-run Phase 1: a ragdolled bone must NOT re-register (RigidBodyData
        // is gone, so it's no longer a collect_newcomers candidate).
        physics_sync_system(&world, PHYSICS_DT);
        for &b in &bones {
            assert!(
                world.query::<RapierHandles>().unwrap().get(b).is_none(),
                "a ragdolled bone must not re-register a kinematic follower",
            );
        }
        assert_eq!(
            world.resource::<PhysicsWorld>().body_count(),
            3,
            "no keyframed follower re-registered after activation",
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

    // ── #1718 / FNV-D7-01 — dropped-bone ragdoll telemetry ──────────────
    //
    // `template_from_imported` now warns when a body's bone name doesn't
    // resolve against the skeleton, and when a constraint's endpoint
    // references such a dropped body. These tests pin the *functional*
    // drop/remap behaviour the warns are attached to (no log-capture
    // harness exists in this codebase, matching the untested sibling
    // warn at `crates/nif/src/import/collision.rs::extract_ragdoll` /
    // #1539) — a regression in the drop logic itself would surface here.

    use byroredux_nif::import::{ImportedRagdollBody, ImportedRagdollConstraint};

    fn body(bone_name: &str) -> ImportedRagdollBody {
        ImportedRagdollBody {
            bone_name: Arc::from(bone_name),
            mass: 1.0,
            linear_damping: 0.05,
            angular_damping: 0.05,
            friction: 0.5,
            restitution: 0.0,
            shape: CollisionShape::Ball { radius: 5.0 },
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        }
    }

    fn hinge_constraint(body_a: usize, body_b: usize) -> ImportedRagdollConstraint {
        ImportedRagdollConstraint {
            body_a,
            body_b,
            kind: ImportedJointKind::LimitedHinge {
                axis_a: Vec3::X,
                pivot_a: Vec3::ZERO,
                axis_b: Vec3::X,
                pivot_b: Vec3::ZERO,
                min_angle: -1.0,
                max_angle: 1.0,
            },
        }
    }

    /// Baseline: every bone resolves — all bodies and constraints survive.
    #[test]
    fn all_bones_resolve_yields_full_template() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        let mut skel_map = HashMap::new();
        skel_map.insert(Arc::<str>::from("Spine"), spine);
        skel_map.insert(Arc::<str>::from("Head"), head);

        let imported = ImportedRagdoll {
            bodies: vec![body("Spine"), body("Head")],
            constraints: vec![hinge_constraint(0, 1)],
        };
        let template =
            template_from_imported(&imported, &skel_map).expect("both bones resolve");
        assert_eq!(template.bodies.len(), 2);
        assert_eq!(template.constraints.len(), 1);
        assert_eq!(template.bodies[0].bone, spine);
        assert_eq!(template.bodies[1].bone, head);
    }

    /// One body's bone name is absent from the skeleton map (renamed bone /
    /// variant skeleton / importer canonicalisation mismatch). It must be
    /// dropped, remaining bodies remap correctly, and any constraint that
    /// referenced the dropped body is also dropped — without panicking.
    #[test]
    fn dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        // No entry for "LFoot" — simulates a bone-name mismatch.
        let mut skel_map = HashMap::new();
        skel_map.insert(Arc::<str>::from("Spine"), spine);
        skel_map.insert(Arc::<str>::from("Head"), head);

        let imported = ImportedRagdoll {
            bodies: vec![body("Spine"), body("LFoot"), body("Head")],
            constraints: vec![
                // Spine <-> LFoot: LFoot is dropped, so this must vanish too.
                hinge_constraint(0, 1),
                // Spine <-> Head: both resolve, must survive.
                hinge_constraint(0, 2),
            ],
        };
        let template = template_from_imported(&imported, &skel_map)
            .expect("2 of 3 bones resolve, 1 of 2 constraints survives");
        assert_eq!(template.bodies.len(), 2, "LFoot body must be dropped");
        assert_eq!(
            template.constraints.len(),
            1,
            "the constraint referencing the dropped LFoot body must be dropped"
        );
        // Remaining indices must remap to the surviving Spine/Head bodies,
        // not the original (now-invalid) 0/2 indices into `imported.bodies`.
        assert_eq!(template.bodies[template.constraints[0].body_a].bone, spine);
        assert_eq!(template.bodies[template.constraints[0].body_b].bone, head);
    }

    /// Fewer than 2 surviving bodies returns `None` (matches the documented
    /// contract) rather than a degenerate single-body template.
    #[test]
    fn single_surviving_body_returns_none() {
        let mut world = World::new();
        let spine = world.spawn();
        let mut skel_map = HashMap::new();
        skel_map.insert(Arc::<str>::from("Spine"), spine);

        let imported = ImportedRagdoll {
            bodies: vec![body("Spine"), body("Unknown1"), body("Unknown2")],
            constraints: vec![hinge_constraint(0, 1)],
        };
        assert!(template_from_imported(&imported, &skel_map).is_none());
    }

    /// 2+ bodies survive but every constraint referenced a dropped body —
    /// no articulation survives, so this must return `None` too.
    #[test]
    fn surviving_bodies_with_no_surviving_constraints_returns_none() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        let mut skel_map = HashMap::new();
        skel_map.insert(Arc::<str>::from("Spine"), spine);
        skel_map.insert(Arc::<str>::from("Head"), head);

        let imported = ImportedRagdoll {
            bodies: vec![body("Spine"), body("Head"), body("LFoot")],
            // The only constraint links Spine (0) to the dropped LFoot (2).
            constraints: vec![hinge_constraint(0, 2)],
        };
        assert!(template_from_imported(&imported, &skel_map).is_none());
    }
}
