//! `physics_sync_system` — per-tick bridge between ECS and Rapier.
//!
//! Runs after `transform_propagation_system` so that `GlobalTransform`
//! is fresh for Phase 1 spawning and Phase 2 kinematic pushes. Walks
//! four phases:
//!
//! 1. **Register** new entities: `(CollisionShape, RigidBodyData,
//!    GlobalTransform)` without `RapierHandles` → build & insert
//!    Rapier body + collider, attach `RapierHandles`.
//! 2. **Push kinematic** transforms: keyframed bodies track the ECS
//!    `GlobalTransform` via `set_next_kinematic_position`.
//! 3. **Step**: drain the fixed-timestep accumulator.
//! 4. **Pull dynamic** transforms back into ECS `Transform` (dynamic
//!    bodies only — static/keyframed are driven the other way).

use byroredux_core::ecs::components::collision::{CollisionShape, MotionType, RigidBodyData};
use byroredux_core::ecs::components::{GlobalTransform, Transform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use rapier3d::prelude::{ColliderBuilder, RigidBodyBuilder, RigidBodyType};

use crate::components::RapierHandles;
use crate::config::ContactConfig;
use crate::convert::{
    collision_shape_to_parts, iso_from_trs, quat_from_na, vec3_from_translation, vec3_to_na,
};
use crate::world::PhysicsWorld;

/// Helper for systems that don't depend on rapier3d: set the linear
/// velocity of an ECS-tracked body by `EntityId`. Returns `false` if the
/// entity has no physics handle yet.
pub fn set_linear_velocity(world: &World, entity: EntityId, velocity: glam::Vec3) -> bool {
    let handles = match world
        .query::<RapierHandles>()
        .and_then(|q| q.get(entity).copied())
    {
        Some(h) => h,
        None => return false,
    };
    let nonzero = velocity.length_squared() > 1e-6;
    let mut pw = world.resource_mut::<PhysicsWorld>();
    let Some(body) = pw.bodies.get_mut(handles.body) else {
        return false;
    };
    body.set_linvel(vec3_to_na(velocity), nonzero);
    if nonzero {
        // Re-engage the pipeline step this frame so the velocity integrates
        // even if the scene was otherwise asleep (the static-scene fast path).
        // A zero velocity (a stop) must NOT wake — otherwise a stationary
        // velocity-driven body pins the simulation awake every frame.
        pw.wake();
    }
    true
}

/// M28.5 — queue the next kinematic translation on a body by ECS
/// `EntityId`. Used by `byroredux::systems::character_controller_system`
/// to push the post-KCC-corrected position into Rapier so other
/// bodies' queries see the player at the right spot.
///
/// Mirrors [`set_linear_velocity`]'s shape — opaque to callers that
/// don't depend on rapier3d directly. Returns `false` if the entity
/// has no physics handle yet.
pub fn set_kinematic_translation(world: &World, entity: EntityId, translation: glam::Vec3) -> bool {
    let handles = match world
        .query::<RapierHandles>()
        .and_then(|q| q.get(entity).copied())
    {
        Some(h) => h,
        None => return false,
    };
    let mut pw = world.resource_mut::<PhysicsWorld>();
    let Some(body) = pw.bodies.get_mut(handles.body) else {
        return false;
    };
    let target = vec3_to_na(translation);
    // Only wake on actual movement. The character controller pushes the
    // player capsule every frame to track the camera — even in fly mode and
    // even when standing still — so an unconditional wake here pins the
    // simulation awake forever (the static-scene fast path never engages).
    // A no-op re-target leaves the body's kinematic velocity at zero anyway.
    let moved = (target - *body.translation()).norm_squared() > 1e-4;
    body.set_next_kinematic_translation(target);
    if moved {
        pw.wake();
    }
    true
}

/// Scheduler-compatible physics tick. Place after transform propagation.
pub fn physics_sync_system(world: &World, dt: f32) {
    if world.try_resource::<PhysicsWorld>().is_none() {
        return;
    }

    // `BYRO_PROFILE=1` logs a per-phase breakdown so the dominant phase
    // can be localized without guessing (Phase 1 collect/register, 2
    // push-kinematic, 3 Rapier step, 4 pull-dynamic). Silent otherwise.
    let profile = std::env::var_os("BYRO_PROFILE").is_some();
    let t = |on: bool| on.then(std::time::Instant::now);
    let ms = |s: Option<std::time::Instant>| s.map(|i| i.elapsed().as_secs_f32() * 1000.0);

    // Build list of newcomers while holding only read locks on the
    // relevant storages. We collect to a Vec so Phase 1 can release the
    // read locks before acquiring write locks on PhysicsWorld + RapierHandles.
    let s1 = t(profile);
    let newcomers = collect_newcomers(world);
    let n_new = newcomers.len();
    if !newcomers.is_empty() {
        register_newcomers(world, newcomers);
    }
    let ms1 = ms(s1);

    // Phase 2: push kinematic poses.
    let s2 = t(profile);
    push_kinematic(world);
    let ms2 = ms(s2);

    // Phase 3: step.
    let s3 = t(profile);
    let steps = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        pw.step(dt)
    };
    if steps > 1 {
        log::trace!("physics: {steps} substeps consumed");
    }
    let ms3 = ms(s3);

    // Phase 4: pull dynamic transforms back into ECS.
    let s4 = t(profile);
    pull_dynamic(world);
    let ms4 = ms(s4);

    if profile {
        let (awake_dyn, awake_kin) = world.resource::<PhysicsWorld>().awake_counts();
        log::info!(
            "physics_sync phases: collect/register={:.2}ms (new={}) push_kin={:.2}ms step={:.2}ms({} substeps) pull_dyn={:.2}ms | awake dyn={} kin={}",
            ms1.unwrap_or(0.0),
            n_new,
            ms2.unwrap_or(0.0),
            ms3.unwrap_or(0.0),
            steps,
            ms4.unwrap_or(0.0),
            awake_dyn,
            awake_kin,
        );
    }
}

// ── Phase 1 ─────────────────────────────────────────────────────────────

/// Snapshot of one entity to register in Rapier.
///
/// One unified path: every newcomer carries the engine `CollisionShape`,
/// `RigidBodyData` (with motion type), and `GlobalTransform`. The
/// rotation-lock decision is derived from `motion_type` — character
/// kinematic bodies (and only those) lock rotations, everything else
/// follows the body data verbatim.
struct Newcomer {
    entity: EntityId,
    shape: CollisionShape,
    body_data: RigidBodyData,
    global: GlobalTransform,
}

impl Newcomer {
    fn body_type(&self) -> RigidBodyType {
        motion_type_to_rapier(self.body_data.motion_type)
    }

    fn lock_rotations(&self) -> bool {
        matches!(self.body_data.motion_type, MotionType::CharacterKinematic)
    }
}

fn collect_newcomers(world: &World) -> Vec<Newcomer> {
    let mut out = Vec::new();

    let Some(gq) = world.query::<GlobalTransform>() else {
        return out;
    };

    let (Some(shape_q), Some(body_q)) = (
        world.query::<CollisionShape>(),
        world.query::<RigidBodyData>(),
    ) else {
        return out;
    };

    let handles_q = world.query::<RapierHandles>();
    for (entity, shape) in shape_q.iter() {
        if let Some(ref hq) = handles_q {
            if hq.contains(entity) {
                continue;
            }
        }
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        let Some(global) = gq.get(entity) else {
            continue;
        };
        out.push(Newcomer {
            entity,
            shape: shape.clone(),
            body_data: body_data.clone(),
            global: *global,
        });
    }

    out
}

fn motion_type_to_rapier(m: MotionType) -> RigidBodyType {
    match m {
        MotionType::Static => RigidBodyType::Fixed,
        MotionType::Keyframed | MotionType::CharacterKinematic => {
            RigidBodyType::KinematicPositionBased
        }
        MotionType::Dynamic => RigidBodyType::Dynamic,
    }
}

fn register_newcomers(world: &World, newcomers: Vec<Newcomer>) {
    // Snapshot `ContactConfig` once per batch. Defaults if missing — the
    // resource is optional for backwards compatibility with embedders
    // that don't insert it explicitly. (See `byroredux::scene` for the
    // engine-side install.)
    let cfg = world
        .try_resource::<ContactConfig>()
        .map(|r| *r)
        .unwrap_or_default();

    let mut pw = world.resource_mut::<PhysicsWorld>();

    let mut registered: Vec<(EntityId, RapierHandles)> = Vec::with_capacity(newcomers.len());

    for n in newcomers {
        // Convert the engine shape into a flat list of Rapier parts.
        // Compounds are fully flattened and TriMeshes surface as their
        // own parts — attaching one collider per part is Rapier's
        // idiomatic path for mixed-composite compositions (#373, which
        // eliminated the 9,555/30s parry3d panic storm the previous
        // single-compound path produced on exterior cells).
        let parts = collision_shape_to_parts(&n.shape, &cfg);
        if parts.is_empty() {
            continue;
        }

        let body_type = n.body_type();
        let lock_rotations = n.lock_rotations();

        let mut body_builder = RigidBodyBuilder::new(body_type)
            .position(iso_from_trs(n.global.translation, n.global.rotation))
            .linear_damping(n.body_data.linear_damping)
            .angular_damping(n.body_data.angular_damping);
        if lock_rotations {
            body_builder = body_builder.lock_rotations();
        }
        let body = body_builder.build();
        let body_handle = pw.bodies.insert(body);

        // Split-borrow: destructure to avoid "&mut pw twice" through field access.
        let PhysicsWorld {
            ref mut bodies,
            ref mut colliders,
            ..
        } = *pw;

        // Distribute mass across parts so the total matches the body's
        // configured mass — Rapier sums collider masses on insertion.
        // Friction/restitution copy onto every collider identically.
        let part_mass = n.body_data.mass.max(0.0) / parts.len() as f32;
        let contact_skin = cfg.default_contact_skin_bu.max(0.0);
        let mut first_collider_handle: Option<rapier3d::prelude::ColliderHandle> = None;
        for (iso, shape) in parts {
            let collider = ColliderBuilder::new(shape)
                .position(iso)
                .friction(n.body_data.friction)
                .restitution(n.body_data.restitution)
                .mass(part_mass)
                .contact_skin(contact_skin)
                .build();
            let handle = colliders.insert_with_parent(collider, body_handle, bodies);
            if first_collider_handle.is_none() {
                first_collider_handle = Some(handle);
            }
        }

        registered.push((
            n.entity,
            RapierHandles {
                body: body_handle,
                // The first collider is the representative handle the
                // ECS keeps a reference to — Rapier owns the rest of
                // the parts through the parent body relationship.
                collider: first_collider_handle.expect("at least one part was appended above"),
            },
        ));
    }

    // New bodies (and the colliders that change the query pipeline) must be
    // stepped at least once so dynamic newcomers settle and the query
    // pipeline rebuilds — otherwise the static-scene fast path could sleep
    // through their first frame. See `PhysicsWorld::step`.
    if !registered.is_empty() {
        pw.wake();
    }

    drop(pw);

    // Attach RapierHandles components. The ECS Query API doesn't expose
    // per-entity insert through a QueryWrite on a previously-unknown
    // entity, so we need mutable access to the World. But systems only
    // get `&World`. We work around this by pre-registering storage and
    // going through the QueryWrite::insert path, which IS available.
    if !registered.is_empty() {
        // Ensure storage exists for QueryWrite to succeed.
        let mut handles_q = match world.query_mut::<RapierHandles>() {
            Some(q) => q,
            None => {
                log::error!(
                    "RapierHandles storage missing — call World::register::<RapierHandles>() \
                     during setup before running physics_sync_system"
                );
                return;
            }
        };
        for (entity, handles) in registered {
            handles_q.insert(entity, handles);
        }
    }
}

// ── Phase 2 ─────────────────────────────────────────────────────────────

fn push_kinematic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };
    let Some(global_q) = world.query::<GlobalTransform>() else {
        return;
    };

    let mut pw = world.resource_mut::<PhysicsWorld>();
    let mut pushed = false;
    for (entity, handles) in handles_q.iter() {
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        // Only `Keyframed` bodies (doors, platforms, scripted props)
        // track their ECS `GlobalTransform` automatically. Character
        // kinematic bodies are driven explicitly by the character
        // controller system via `set_kinematic_translation`; pushing
        // the ECS Transform here would race with the controller's
        // KCC-corrected pose write.
        if body_data.motion_type != MotionType::Keyframed {
            continue;
        }
        let Some(g) = global_q.get(entity) else {
            continue;
        };
        if let Some(body) = pw.bodies.get_mut(handles.body) {
            let target = iso_from_trs(g.translation, g.rotation);
            let cur = *body.position();
            // Only re-target a keyframed body whose pose actually changed.
            // Re-pushing an idle body (a closed door) every frame gives it a
            // (near-zero but nonzero) kinematic velocity, keeping the solver
            // busy and the `pending_wake` gate engaged — so it would pin the
            // whole simulation awake. Skipping the no-op push leaves its
            // velocity at exactly zero (the solver skips it) and lets the
            // static-scene fast path engage. See `PhysicsWorld::step`.
            let dt = (cur.translation.vector - target.translation.vector).norm();
            let dr = cur.rotation.angle_to(&target.rotation);
            if dt * dt > 1e-6 || dr > 1e-5 {
                body.set_next_kinematic_position(target);
                pushed = true;
            }
        }
    }
    // A keyframed body got a fresh target — re-engage the pipeline step so
    // it actually moves even if the rest of the scene is asleep.
    if pushed {
        pw.wake();
    }
}

// ── Phase 4 ─────────────────────────────────────────────────────────────

fn pull_dynamic(world: &World) {
    let Some(handles_q) = world.query::<RapierHandles>() else {
        return;
    };
    let Some(body_q) = world.query::<RigidBodyData>() else {
        return;
    };

    // Build a list of (entity, new local translation, new local rotation)
    // before taking the Transform write lock.
    let mut updates: Vec<(EntityId, glam::Vec3, glam::Quat)> = Vec::new();
    {
        let pw = world.resource::<PhysicsWorld>();
        for (entity, handles) in handles_q.iter() {
            let Some(body_data) = body_q.get(entity) else {
                continue;
            };
            if body_data.motion_type != MotionType::Dynamic {
                continue;
            }
            let Some(body) = pw.bodies.get(handles.body) else {
                continue;
            };
            let iso = *body.position();
            let translation = vec3_from_translation(iso.translation);
            let rotation = quat_from_na(iso.rotation);
            updates.push((entity, translation, rotation));
        }
    }

    if updates.is_empty() {
        return;
    }

    let Some(mut tq) = world.query_mut::<Transform>() else {
        return;
    };
    for (entity, pos, rot) in updates {
        if let Some(t) = tq.get_mut(entity) {
            t.translation = pos;
            t.rotation = rot;
        }
    }
}
