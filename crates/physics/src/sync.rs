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

use crate::components::{PlayerBody, RapierHandles};
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
    let mut pw = world.resource_mut::<PhysicsWorld>();
    if let Some(body) = pw.bodies.get_mut(handles.body) {
        body.set_linvel(vec3_to_na(velocity), true);
        true
    } else {
        false
    }
}

/// Scheduler-compatible physics tick. Place after transform propagation.
pub fn physics_sync_system(world: &World, dt: f32) {
    if world.try_resource::<PhysicsWorld>().is_none() {
        return;
    }

    // Build list of newcomers while holding only read locks on the
    // relevant storages. We collect to a Vec so Phase 1 can release the
    // read locks before acquiring write locks on PhysicsWorld + RapierHandles.
    let newcomers = collect_newcomers(world);
    if !newcomers.is_empty() {
        register_newcomers(world, newcomers);
    }

    // Phase 2: push kinematic poses.
    push_kinematic(world);

    // Phase 3: step.
    let steps = {
        let mut pw = world.resource_mut::<PhysicsWorld>();
        pw.step(dt)
    };
    if steps > 1 {
        log::trace!("physics: {steps} substeps consumed");
    }

    // Phase 4: pull dynamic transforms back into ECS.
    pull_dynamic(world);
}

// ── Phase 1 ─────────────────────────────────────────────────────────────

/// Snapshot of one entity to register in Rapier.
struct Newcomer {
    entity: EntityId,
    body_type: RigidBodyType,
    body_data: RigidBodyData,
    global: GlobalTransform,
    // Mutually exclusive: either a parsed collision shape (from NIF) or
    // a PlayerBody marker that implies a capsule built from its fields.
    source: NewcomerSource,
    lock_rotations: bool,
}

enum NewcomerSource {
    Shape(CollisionShape),
    Player(PlayerBody),
}

fn collect_newcomers(world: &World) -> Vec<Newcomer> {
    let mut out = Vec::new();

    let Some(gq) = world.query::<GlobalTransform>() else {
        return out;
    };

    // Path A: entities with CollisionShape + RigidBodyData from NIF import.
    if let (Some(shape_q), Some(body_q)) = (
        world.query::<CollisionShape>(),
        world.query::<RigidBodyData>(),
    ) {
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
                body_type: motion_type_to_rapier(body_data.motion_type),
                body_data: body_data.clone(),
                global: *global,
                source: NewcomerSource::Shape(shape.clone()),
                lock_rotations: false,
            });
        }
    }

    // Path B: PlayerBody marker entities without a CollisionShape.
    if let Some(player_q) = world.query::<PlayerBody>() {
        let handles_q = world.query::<RapierHandles>();
        for (entity, player) in player_q.iter() {
            if let Some(ref hq) = handles_q {
                if hq.contains(entity) {
                    continue;
                }
            }
            let Some(global) = gq.get(entity) else {
                continue;
            };
            let body_data = RigidBodyData {
                motion_type: MotionType::Dynamic,
                mass: 80.0,
                friction: 0.5,
                restitution: 0.0,
                linear_damping: 2.0,
                angular_damping: 5.0,
            };
            out.push(Newcomer {
                entity,
                body_type: RigidBodyType::Dynamic,
                body_data,
                global: *global,
                source: NewcomerSource::Player(*player),
                lock_rotations: true,
            });
        }
    }

    out
}

fn motion_type_to_rapier(m: MotionType) -> RigidBodyType {
    match m {
        MotionType::Static => RigidBodyType::Fixed,
        MotionType::Keyframed => RigidBodyType::KinematicPositionBased,
        MotionType::Dynamic => RigidBodyType::Dynamic,
    }
}

fn register_newcomers(world: &World, newcomers: Vec<Newcomer>) {
    let mut pw = world.resource_mut::<PhysicsWorld>();

    // Build Rapier objects outside the per-entity insert loop so we can
    // keep the code shape simple.
    let mut registered: Vec<(EntityId, RapierHandles)> = Vec::with_capacity(newcomers.len());

    for n in newcomers {
        // Convert the engine shape into a flat list of Rapier parts.
        // Compounds are fully flattened and TriMeshes surface as their
        // own parts — attaching one collider per part is Rapier's
        // idiomatic path for mixed-composite compositions (#373, which
        // eliminated the 9,555/30s parry3d panic storm the previous
        // single-compound path produced on exterior cells).
        let parts: Vec<(
            rapier3d::prelude::Isometry<f32>,
            rapier3d::prelude::SharedShape,
        )> = match &n.source {
            NewcomerSource::Shape(s) => collision_shape_to_parts(s),
            NewcomerSource::Player(p) => {
                use rapier3d::prelude::SharedShape;
                vec![(
                    rapier3d::prelude::Isometry::identity(),
                    SharedShape::capsule_y(p.half_height.max(1e-3), p.radius.max(1e-3)),
                )]
            }
        };
        if parts.is_empty() {
            continue;
        }

        let mut body_builder = RigidBodyBuilder::new(n.body_type)
            .position(iso_from_trs(n.global.translation, n.global.rotation))
            .linear_damping(n.body_data.linear_damping)
            .angular_damping(n.body_data.angular_damping);
        if n.lock_rotations {
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
        let mut first_collider_handle: Option<rapier3d::prelude::ColliderHandle> = None;
        for (iso, shape) in parts {
            let collider = ColliderBuilder::new(shape)
                .position(iso)
                .friction(n.body_data.friction)
                .restitution(n.body_data.restitution)
                .mass(part_mass)
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
    for (entity, handles) in handles_q.iter() {
        let Some(body_data) = body_q.get(entity) else {
            continue;
        };
        if body_data.motion_type != MotionType::Keyframed {
            continue;
        }
        let Some(g) = global_q.get(entity) else {
            continue;
        };
        if let Some(body) = pw.bodies.get_mut(handles.body) {
            body.set_next_kinematic_position(iso_from_trs(g.translation, g.rotation));
        }
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
