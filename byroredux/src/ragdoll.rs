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
use byroredux_core::ecs::{
    Children, EntityId, GlobalTransform, LocalBound, Parent, Transform, World, WorldBound,
};
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
    rest_pose_by_name: &HashMap<Arc<str>, GlobalTransform>,
) -> Option<RagdollTemplate> {
    let mut bodies = Vec::with_capacity(imported.bodies.len());
    let mut old_to_new: Vec<Option<usize>> = vec![None; imported.bodies.len()];
    // #1718 / FNV-D7-01 — collect dropped-body bone names so a skeleton
    // whose bone naming diverges from the ragdoll's authored names (variant
    // skeleton, renamed bone, importer canonicalisation mismatch) leaves a
    // breadcrumb instead of silently degrading/vanishing.
    let mut dropped_bones: Vec<&Arc<str>> = Vec::new();
    let mut dropped_rest_poses: Vec<&Arc<str>> = Vec::new();
    for (i, b) in imported.bodies.iter().enumerate() {
        // #2458 — exact match first, falling back to a case-insensitive
        // scan so a case-only divergence between the ragdoll's authored
        // bone names and the skeleton's node names doesn't silently drop
        // the body. See `crate::name_lookup`'s module doc.
        let Some(&bone) = crate::name_lookup::get_case_insensitive(skel_map, &b.bone_name) else {
            dropped_bones.push(&b.bone_name);
            continue;
        };
        let Some(rest) = crate::name_lookup::get_case_insensitive(rest_pose_by_name, &b.bone_name)
        else {
            dropped_rest_poses.push(&b.bone_name);
            continue;
        };
        if !rest.translation.is_finite()
            || !rest.rotation.is_finite()
            || !rest.scale.is_finite()
            || rest.scale.abs() <= f32::EPSILON
        {
            dropped_rest_poses.push(&b.bone_name);
            continue;
        }

        // A `bhkRigidBodyT`'s CInfo transform is the collision object's pose
        // **relative to the NiNode that owns it** — the same reading
        // `extract_from_classic` (the architecture-collider path, #2316) has
        // always applied to the identical `BhkRigidBody::{is_t, translation,
        // rotation}` fields. It is therefore used verbatim as the bone-local
        // offset.
        //
        // #3318 — this used to subtract the bone's rest pose out of it, on the
        // premise that the value was authored in skeleton-root space. That
        // premise is falsified by the authored data: across FNV's ragdoll
        // corpus the median |CInfo translation| is 8.6 game units and only 9
        // of 268 exceed 40 (a chandelier, a queen-ant clavicle, a swinging
        // I-beam — single-body props, not limbs). Root-space poses would put
        // nearly every limb within ~8.6 units of the skeleton root, which is
        // impossible; and the subtraction produced "bone-local" offsets whose
        // magnitude tracked the bone's own distance from the root instead
        // (robobrain `Bip01 Head`: authored 18.1, computed local 378.3, bone
        // 379.3 from root). 351 of 351 T bodies came out > 1 unit off, 297 of
        // them > 10.
        //
        // The subtraction arrived with #2336, which was diagnosed on *non-T*
        // bodies — and #2447 has since gated those out of this branch
        // entirely, so it was left applying only to the case it was never
        // validated against. Its guard for that case is kept below.
        //
        // #2447 / PHYS-01 — `b.translation`/`b.rotation` are only meaningful
        // when `b.is_t` (the source block was `bhkRigidBodyT`). Plain
        // `bhkRigidBody` carries the same wire-format CInfo bytes, but
        // Gamebryo treats them as identity (#2316) — using stale/garbage
        // bytes here would displace the body from its bone by whatever
        // leftover offset survived the authoring tool's export. Fall back to
        // zero local offset (body coincident with the bone's rest transform).
        let (local_translation, local_rotation) = if b.is_t {
            (b.translation, b.rotation)
        } else {
            (Vec3::ZERO, Quat::IDENTITY)
        };
        old_to_new[i] = Some(bodies.len());
        bodies.push(RagdollTemplateBody {
            bone,
            local_translation,
            local_rotation,
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
    if !dropped_rest_poses.is_empty() {
        log::warn!(
            "template_from_imported: {} ragdoll body/bodies dropped — missing or invalid \
             skeleton rest pose for bone name(s): {:?}",
            dropped_rest_poses.len(),
            dropped_rest_poses,
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
    Some(RagdollTemplate {
        bodies,
        constraints,
    })
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
            // `plane_min` / `plane_max` (the asymmetric swing limits decoded
            // in `crates/nif/src/import/collision/ragdoll.rs::ragdoll_joint`)
            // are intentionally dropped into `..` here: `build_joint`
            // (`crates/physics/src/ragdoll.rs`) applies a *symmetric*
            // `[-cone, cone]` on both swing axes (`JointAxis::AngY` / `AngZ`).
            // This is the documented "cone → both swing axes" simplification —
            // see `docs/engine/physal.md` § Known approximation. Mapping the
            // plane range onto Rapier's per-axis AngY/AngZ limits is PHYSAL
            // rollout step 3. Unlike the sibling edge-drop sites this is a
            // per-field fidelity loss on every ragdoll joint, so it stays a
            // comment rather than a per-joint `log::warn!` (which would flood
            // the log at ragdoll-activation time). FNV-D7-03 / #1982.
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
            perp_a,
            pivot_a,
            axis_b,
            perp_b,
            pivot_b,
            min_angle,
            max_angle,
        } => RagdollJointSpec::LimitedHinge {
            axis_a: *axis_a,
            perp_a: *perp_a,
            pivot_a: *pivot_a,
            axis_b: *axis_b,
            perp_b: *perp_b,
            pivot_b: *pivot_b,
            min_angle: *min_angle,
            max_angle: *max_angle,
        },
        ImportedJointKind::Prismatic {
            axis_a,
            perp_a,
            pivot_a,
            axis_b,
            perp_b,
            pivot_b,
            min_distance,
            max_distance,
        } => RagdollJointSpec::Prismatic {
            axis_a: *axis_a,
            perp_a: *perp_a,
            pivot_a: *pivot_a,
            axis_b: *axis_b,
            perp_b: *perp_b,
            pivot_b: *pivot_b,
            min_distance: *min_distance,
            max_distance: *max_distance,
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
                // #2868 / #3065 — retain canonical authored geometry here.
                // `build_ragdoll` passes this snapshotted scale to the shared
                // PHYSAL converter, which resizes every shape variant exactly
                // once. Pre-scaling here produced scale² limb geometry while
                // the joint pivots below remained scale¹.
                shape: b.shape.clone(),
                mass: b.mass,
                linear_damping: b.linear_damping,
                angular_damping: b.angular_damping,
                friction: b.friction,
                restitution: b.restitution,
            });
        }
        // #2868 — the joint pivots are authored in the same bind space as the
        // shapes and must be seeded against the same scale as the body poses
        // above. `activate_ragdoll` is the one boundary where the live actor
        // scale and the authored spec meet, so the multiplication belongs
        // here; `build_joint` stays unit-agnostic. Each side takes its own
        // endpoint body's scale, since each pivot is in that body's frame.
        // A constraint naming an out-of-range body index is left unscaled —
        // `orient_tree` drops it, and fabricating a scale for a body that
        // doesn't exist would hide the upstream defect.
        let constraints = template
            .constraints
            .iter()
            .map(|c| {
                let scale_of = |index: usize| bodies.get(index).map(|b: &RagdollBodySpec| b.scale);
                let joint = match (scale_of(c.body_a), scale_of(c.body_b)) {
                    (Some(scale_a), Some(scale_b)) => c.joint.scaled_pivots(scale_a, scale_b),
                    _ => c.joint.clone(),
                };
                RagdollConstraintSpec {
                    body_a: c.body_a,
                    body_b: c.body_b,
                    joint,
                }
            })
            .collect();
        RagdollSpec {
            bodies,
            constraints,
        }
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
///
/// #1981 (FNV-D7-02) — a final pass expands the actor's skinned-mesh
/// `WorldBound`(s) to enclose the live simulated body positions. The mesh
/// entity's own `GlobalTransform` never moves during ragdoll (only its
/// *bones* do — the mesh is deformed by the GPU bone palette, not by
/// re-placing the mesh entity), so `make_world_bound_propagation_system`'s
/// `LocalBound × GlobalTransform` leaf bound stays anchored at the
/// bind-pose extent every frame, regardless of how far the simulated
/// bodies travel. A ragdoll that crumples in place stays within that
/// radius (benign); one that slides/falls away from spawn can be
/// frustum-culled or carry a stale TLAS-instance bound while still
/// on-screen. See `WorldBound::merge`'s "smallest enclosing sphere"
/// construction — merging with an already-enclosed point is a no-op, so
/// this needs no separate "did it leave the bind-pose radius" branch.
pub fn ragdoll_writeback_system(world: &World, _dt: f32) {
    let Some(rq) = world.query::<Ragdoll>() else {
        return;
    };
    let Some(tq) = world.query::<RagdollTemplate>() else {
        return;
    };
    // Hierarchy + local-pose reads for the descendant re-derivation pass.
    // Absent (a flat skeleton with neither Parent nor Children) the pass is a
    // no-op and only the body writeback runs. Scratch reused across actors.
    // Transform, then Parent, then Children — matches
    // `transform_propagation_system`'s acquisition order for these pairs;
    // GlobalTransform (write) is taken last of the four, then PhysicsWorld
    // last of all — matching `push_kinematic`'s order for that pair (#313).
    let transform_q = world.query::<Transform>();
    let parent_q = world.query::<Parent>();
    let children_q = world.query::<Children>();
    let Some(mut gtq) = world.query_mut::<GlobalTransform>() else {
        return;
    };
    let Some(pw) = world.try_resource::<PhysicsWorld>() else {
        return;
    };
    // #1981 — LocalBound (read) grouped with the other read-only hierarchy
    // queries above; WorldBound (write) grouped with GlobalTransform (write)
    // below, matching this function's existing "reads first, writes last"
    // discipline. Both are `Option` (not `let Some(..) else return`): a
    // world that hasn't registered bounds at all (most of this file's own
    // unit tests) must still get the bone writeback + #1979 descendant
    // re-derivation; the mesh-bound expansion pass below is skipped instead.
    let local_bound_q = world.query::<LocalBound>();
    let mut world_bound_q = world.query_mut::<WorldBound>();
    let mut body_bones: HashSet<EntityId> = HashSet::new();
    let mut queue: VecDeque<EntityId> = VecDeque::new();
    // #1981 scratch, reused across actors: live simulated body positions
    // this frame, and the BFS queue for finding LocalBound-bearing mesh
    // entities under the actor's subtree (independent of `queue` above,
    // which the #1979 pass is still using when this runs).
    let mut live_body_positions: Vec<Vec3> = Vec::new();
    let mut mesh_walk_queue: VecDeque<EntityId> = VecDeque::new();
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
        live_body_positions.clear();
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
            // #1981 — collected regardless of whether the bone resolves
            // below (a stale/removed bone entity shouldn't hide a live
            // simulated body from the mesh-bound expansion pass).
            live_body_positions.push(t);
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

        // ── #1981 — expand skinned-mesh WorldBound(s) to enclose the ragdoll ──
        //
        // Walk `actor`'s subtree (BFS via Children only — independent of the
        // #1979 pass below, so it still runs even on a world with no
        // `Parent` storage) for entities carrying a `LocalBound`: the leaf
        // mesh entities `make_world_bound_propagation_system` derives a
        // `WorldBound` for (#1213), and the ones left stale because the
        // mesh entity's own `GlobalTransform` never moves during ragdoll.
        // For each, merge in a sphere per live body position using the
        // mesh's own bind-pose `LocalBound.radius` as a conservative
        // per-body margin — generous rather than tight, since the mesh has
        // no per-bone extent data to draw a tighter bound from and the
        // failure mode this fixes is under-coverage (culling / TLAS pop),
        // not over-coverage. A body still within the existing bound is a
        // no-op: `WorldBound::merge` returns the unchanged bound when one
        // sphere already contains the other.
        if !live_body_positions.is_empty() {
            if let (Some(lb_q), Some(wb_q), Some(cq)) = (
                local_bound_q.as_ref(),
                world_bound_q.as_mut(),
                children_q.as_ref(),
            ) {
                mesh_walk_queue.clear();
                if let Some(children) = cq.get(actor) {
                    mesh_walk_queue.extend(children.0.iter().copied());
                }
                while let Some(entity) = mesh_walk_queue.pop_front() {
                    if let Some(children) = cq.get(entity) {
                        mesh_walk_queue.extend(children.0.iter().copied());
                    }
                    let Some(local) = lb_q.get(entity) else {
                        continue;
                    };
                    let Some(current) = wb_q.get(entity).copied() else {
                        continue;
                    };
                    let mut merged = current;
                    for &pos in &live_body_positions {
                        merged = merged.merge(&WorldBound::new(pos, local.radius));
                    }
                    if merged.center != current.center || merged.radius != current.radius {
                        if let Some(wb) = wb_q.get_mut(entity) {
                            *wb = merged;
                        }
                    }
                }
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

    /// Regression for #2868 and #3065. `activate_ragdoll` is the boundary
    /// where the live actor scale meets the authored template: it snapshots
    /// that scale for the shared collider converter and applies it to the
    /// joint pivots. Geometry and articulation must both end at scale¹.
    ///
    /// Two bones of a 2x actor sit 100 apart with authored ±25 pivots (bind
    /// separation 50). Pre-fix the pivots went through verbatim and multibody
    /// forward kinematics pulled the child back to 50 on the first step — a
    /// scaled NPC's corpse crushed to bind proportions while the writeback
    /// kept stretching its bones to 2x, the "visibly crushed, interpenetrating
    /// corpse" symptom. `scale = 1.0` is the majority case every other test
    /// covers, which is exactly why this one was invisible.
    #[test]
    fn scaled_actor_ragdoll_keeps_its_seeded_bone_separation() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.insert_resource(PhysicsWorld::new());

        const ACTOR_SCALE: f32 = 2.0;
        const SEEDED_SEPARATION: f32 = 100.0;

        let actor = world.spawn();
        let bones: Vec<_> = (0..2)
            .map(|index| {
                let bone = world.spawn();
                world.insert(
                    bone,
                    GlobalTransform {
                        translation: Vec3::new(index as f32 * SEEDED_SEPARATION, 1000.0, 0.0),
                        rotation: Quat::IDENTITY,
                        scale: ACTOR_SCALE,
                    },
                );
                bone
            })
            .collect();

        world.insert(
            actor,
            RagdollTemplate {
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
                // Authored in BIND units: ±25 → a bind separation of 50, half
                // the distance the 2x bones are actually seeded apart.
                constraints: vec![RagdollTemplateConstraint {
                    body_a: 0,
                    body_b: 1,
                    joint: RagdollJointSpec::Ragdoll {
                        twist_a: Vec3::X,
                        plane_a: Vec3::Y,
                        pivot_a: Vec3::new(25.0, 0.0, 0.0),
                        twist_b: Vec3::X,
                        plane_b: Vec3::Y,
                        pivot_b: Vec3::new(-25.0, 0.0, 0.0),
                        cone_max: std::f32::consts::PI,
                        twist_min: -std::f32::consts::PI,
                        twist_max: std::f32::consts::PI,
                    },
                }],
            },
        );

        activate_ragdoll(&world, actor).expect("activation should succeed");

        let handles: Vec<_> = {
            let ragdolls = world.query::<Ragdoll>().unwrap();
            let ragdoll = ragdolls.get(actor).unwrap();
            ragdoll
                .bodies
                .iter()
                .map(|(_, handle, _)| *handle)
                .collect()
        };
        {
            let pw = world.resource::<PhysicsWorld>();
            let root = pw.colliders_near_xz(0.0, 1000.0, 0.0, 25.0);
            assert_eq!(root.len(), 1, "only the root limb overlaps this probe");
            let diameter = root[0].aabb_max[0] - root[0].aabb_min[0];
            assert!(
                (diameter - 20.0).abs() < 1e-3,
                "a 2× actor with an authored radius-5 limb needs diameter 20, not scale²: \
                 {diameter}"
            );
        }
        {
            let mut pw = world.resource_mut::<PhysicsWorld>();
            pw.step(PHYSICS_DT);
        }

        let pw = world.resource::<PhysicsWorld>();
        let root = body_pose(&pw, handles[0]).unwrap().0;
        let child = body_pose(&pw, handles[1]).unwrap().0;
        let separation = (child - root).length();
        assert!(
            (separation - SEEDED_SEPARATION).abs() < 1.0,
            "the 2x actor's bones collapsed toward bind separation: \
             {SEEDED_SEPARATION} → {separation}"
        );
    }

    /// Regression for #1981 (FNV-D7-02) — a ragdoll body that falls far
    /// from its bind-pose position must expand the skinned-mesh
    /// `WorldBound` (a sibling `Children` of the actor, carrying
    /// `LocalBound`) to keep enclosing it, instead of leaving that bound
    /// anchored at the stale bind-pose sphere the mesh entity's own
    /// (unmoving) `GlobalTransform` would otherwise imply forever.
    #[test]
    fn falling_ragdoll_expands_skinned_mesh_world_bound() {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<RagdollTemplate>();
        world.register::<RagdollActive>();
        world.register::<Ragdoll>();
        world.register::<Children>();
        world.register::<LocalBound>();
        world.register::<WorldBound>();
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

        // The skinned mesh: a child of `actor`, anchored at the bind-pose
        // origin with a small LocalBound — never moved by writeback
        // (mirrors real content: the mesh entity's own GlobalTransform
        // stays put; only its bones move).
        let mesh = world.spawn();
        world.insert(
            mesh,
            GlobalTransform {
                translation: Vec3::new(25.0, 1000.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
        );
        world.insert(mesh, LocalBound::new(Vec3::ZERO, 10.0));
        let bind_pose_bound = WorldBound::new(Vec3::new(25.0, 1000.0, 0.0), 10.0);
        world.insert(mesh, bind_pose_bound);
        world.insert(actor, Children(vec![mesh]));

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

        activate_ragdoll(&world, actor).expect("activation should succeed");

        // Fall under gravity, same as `activate_then_writeback_moves_bones`,
        // long enough to travel well past the 10-unit bind-pose radius.
        for _ in 0..120 {
            {
                let mut pw = world.resource_mut::<PhysicsWorld>();
                pw.step(PHYSICS_DT);
            }
            ragdoll_writeback_system(&world, PHYSICS_DT);
        }

        let far_bone = bones[2];
        let final_pos = world
            .query::<GlobalTransform>()
            .unwrap()
            .get(far_bone)
            .unwrap()
            .translation;
        assert!(
            (final_pos - bind_pose_bound.center).length() > bind_pose_bound.radius,
            "test setup must actually exercise the fix: the bone should have \
             fallen outside the bind-pose bound, got {final_pos:?}"
        );

        let final_bound = *world.query::<WorldBound>().unwrap().get(mesh).unwrap();
        assert!(
            final_bound.radius > bind_pose_bound.radius,
            "mesh WorldBound must have grown past the stale bind-pose radius \
             ({}), got {}",
            bind_pose_bound.radius,
            final_bound.radius
        );
        assert!(
            final_bound.contains_point(final_pos),
            "mesh WorldBound must enclose the fallen bone's live position \
             {final_pos:?}, got center={:?} radius={}",
            final_bound.center,
            final_bound.radius
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
        let ragdoll = world
            .query::<Ragdoll>()
            .unwrap()
            .get(actor)
            .unwrap()
            .clone();
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

        let gt = *world.query::<GlobalTransform>().unwrap().get(bone).unwrap();
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

        let gt = *world.query::<GlobalTransform>().unwrap().get(bone).unwrap();
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
            is_t: true,
        }
    }

    fn hinge_constraint(body_a: usize, body_b: usize) -> ImportedRagdollConstraint {
        ImportedRagdollConstraint {
            body_a,
            body_b,
            kind: ImportedJointKind::LimitedHinge {
                axis_a: Vec3::X,
                perp_a: Vec3::Y,
                pivot_a: Vec3::ZERO,
                axis_b: Vec3::X,
                perp_b: Vec3::Y,
                pivot_b: Vec3::ZERO,
                min_angle: -1.0,
                max_angle: 1.0,
            },
        }
    }

    fn identity_rest_poses(
        skel_map: &HashMap<Arc<str>, EntityId>,
    ) -> HashMap<Arc<str>, GlobalTransform> {
        skel_map
            .keys()
            .map(|name| (name.clone(), GlobalTransform::IDENTITY))
            .collect()
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
        let rest_poses = identity_rest_poses(&skel_map);
        let template =
            template_from_imported(&imported, &skel_map, &rest_poses).expect("both bones resolve");
        assert_eq!(template.bodies.len(), 2);
        assert_eq!(template.constraints.len(), 1);
        assert_eq!(template.bodies[0].bone, spine);
        assert_eq!(template.bodies[1].bone, head);
    }

    /// Regression: #2458 — a ragdoll authored with different letter-casing
    /// than the bound skeleton's node names (e.g. an outfit's ragdoll data
    /// says "Bip01 Spine" while the skeleton's node is "bip01 spine", or
    /// vice versa — Bethesda's own tooling is case-insensitive, so modded
    /// content has no incentive to be byte-exact) must still resolve via
    /// the case-insensitive fallback in `crate::name_lookup`, instead of
    /// being silently dropped like a genuinely-missing bone.
    #[test]
    fn case_mismatched_bone_name_still_resolves() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        let mut skel_map = HashMap::new();
        // Skeleton's own node names, lowercase (as StringPool would key them).
        skel_map.insert(Arc::<str>::from("bip01 spine"), spine);
        skel_map.insert(Arc::<str>::from("bip01 head"), head);

        let imported = ImportedRagdoll {
            // Ragdoll data authored with the mixed-case convention.
            bodies: vec![body("Bip01 Spine"), body("Bip01 Head")],
            constraints: vec![hinge_constraint(0, 1)],
        };
        let rest_poses = identity_rest_poses(&skel_map);
        let template = template_from_imported(&imported, &skel_map, &rest_poses)
            .expect("case-mismatched bone names must resolve via the case-insensitive fallback");
        assert_eq!(template.bodies.len(), 2, "both bones must resolve");
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
        let rest_poses = identity_rest_poses(&skel_map);
        let template = template_from_imported(&imported, &skel_map, &rest_poses)
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
        let rest_poses = identity_rest_poses(&skel_map);
        assert!(template_from_imported(&imported, &skel_map, &rest_poses).is_none());
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
        let rest_poses = identity_rest_poses(&skel_map);
        assert!(template_from_imported(&imported, &skel_map, &rest_poses).is_none());
    }

    /// #3318 — a `bhkRigidBodyT`'s CInfo transform is the collision object's
    /// pose relative to its owning NiNode, so it is the bone-local offset
    /// verbatim. This test replaces
    /// `imported_root_space_body_pose_converts_to_bone_local_once`, which
    /// asserted the opposite (root-space, rest pose subtracted out) on a
    /// synthetic fixture built to match that assumption. The authored data
    /// falsifies it: FNV's median authored |translation| is 8.6 units and
    /// only 9 of 268 exceed 40, which is limb-scale, not skeleton-scale.
    ///
    /// The bone rest poses here are deliberately far from the origin and
    /// non-identity in rotation and scale — under the old reading every
    /// assertion below would be off by exactly that rest pose.
    #[test]
    fn imported_t_body_pose_is_taken_as_bone_local_verbatim() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        let spine_name = Arc::<str>::from("Spine");
        let head_name = Arc::<str>::from("Head");
        let skel_map = HashMap::from([(spine_name.clone(), spine), (head_name.clone(), head)]);

        let spine_rest = GlobalTransform {
            translation: Vec3::new(100.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: 2.0,
        };
        let head_rest = GlobalTransform {
            translation: Vec3::new(100.0, 30.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: 2.0,
        };
        let rest_poses = HashMap::from([(spine_name, spine_rest), (head_name, head_rest)]);

        let spine_local_t = Vec3::new(5.0, 0.0, 0.0);
        let spine_local_r = Quat::from_rotation_z(std::f32::consts::FRAC_PI_6);
        let mut spine_body = body("Spine");
        spine_body.translation = spine_local_t;
        spine_body.rotation = spine_local_r;

        let head_local_t = Vec3::new(0.0, 4.0, 0.0);
        let mut head_body = body("Head");
        head_body.translation = head_local_t;
        head_body.rotation = Quat::IDENTITY;

        let imported = ImportedRagdoll {
            bodies: vec![spine_body, head_body],
            constraints: vec![hinge_constraint(0, 1)],
        };

        let template =
            template_from_imported(&imported, &skel_map, &rest_poses).expect("both bodies resolve");

        assert!(
            (template.bodies[0].local_translation - spine_local_t).length() < 1e-4,
            "authored translation must pass through untouched, got {:?}",
            template.bodies[0].local_translation
        );
        assert!(
            template.bodies[0].local_rotation.dot(spine_local_r).abs() > 1.0 - 1e-4,
            "authored rotation must pass through untouched"
        );
        assert!(
            (template.bodies[1].local_translation - head_local_t).length() < 1e-4,
            "authored translation must pass through untouched, got {:?}",
            template.bodies[1].local_translation
        );

        // The regression this guards: a limb-scale authored offset must stay
        // limb-scale. The old rest-pose subtraction turned these into ~100-unit
        // offsets tracking each bone's distance from the skeleton root.
        for b in &template.bodies {
            assert!(
                b.local_translation.length() < 40.0,
                "bone-local offset must stay limb-scale, got {}",
                b.local_translation.length()
            );
        }
    }

    /// Regression for #2447 / PHYS-01: a plain (non-T) `bhkRigidBody`
    /// ragdoll bone carrying stale, non-identity CInfo translation/rotation
    /// bytes — the exact pattern #2316 fixed for architecture colliders —
    /// must resolve to zero local offset (coincident with the bone's own
    /// rest transform), not the garbage bytes.
    #[test]
    fn non_t_ragdoll_body_ignores_stale_cinfo_offset() {
        let mut world = World::new();
        let spine = world.spawn();
        let head = world.spawn();
        let spine_name = Arc::<str>::from("Spine");
        let head_name = Arc::<str>::from("Head");
        let skel_map = HashMap::from([(spine_name.clone(), spine), (head_name.clone(), head)]);

        let spine_rest = GlobalTransform {
            translation: Vec3::new(100.0, 0.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: 2.0,
        };
        let head_rest = GlobalTransform {
            translation: Vec3::new(100.0, 30.0, 0.0),
            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: 2.0,
        };
        let rest_poses = HashMap::from([(spine_name, spine_rest), (head_name, head_rest)]);

        let mut spine_body = body("Spine");
        // Stale non-identity bytes surviving in a plain bhkRigidBody's CInfo
        // — exactly the #2316 pattern, just on a ragdoll bone instead of
        // architecture. `is_t: false` (the default from `body()` is
        // overridden below) means these must be ignored entirely.
        spine_body.translation = Vec3::new(9999.0, -500.0, 42.0);
        spine_body.rotation = Quat::from_rotation_z(1.23);
        spine_body.is_t = false;
        let mut head_body = body("Head");
        head_body.translation = Vec3::new(-1234.0, 77.0, 3.0);
        head_body.rotation = Quat::from_rotation_x(0.7);
        head_body.is_t = false;
        let imported = ImportedRagdoll {
            bodies: vec![spine_body, head_body],
            constraints: vec![hinge_constraint(0, 1)],
        };

        let template = template_from_imported(&imported, &skel_map, &rest_poses)
            .expect("both non-T bodies still resolve");
        assert_eq!(
            template.bodies[0].local_translation,
            Vec3::ZERO,
            "non-T body must ignore its stale CInfo translation"
        );
        assert_eq!(
            template.bodies[0].local_rotation,
            Quat::IDENTITY,
            "non-T body must ignore its stale CInfo rotation"
        );
        assert_eq!(template.bodies[1].local_translation, Vec3::ZERO);
        assert_eq!(template.bodies[1].local_rotation, Quat::IDENTITY);
    }
}
