//! Force-greet — the Dialogue AI package procedure (FO3/FNV `PKDT`
//! procedure 15), #5367 Phase F.
//!
//! A procedure-15 package makes the NPC approach the player and open a
//! conversation *without activation*, speaking the package's authored
//! topic (`PKDD`'s FormID, decoded on [`PackRecord::dialogue_topic`]) or
//! the master's generic greeting when none is authored. Vanilla FO3/FNV
//! never lists these packages on NPC_ defaults (0 references across both
//! masters) — they are installed by quest scripts — so the runtime keys
//! on a [`ForceGreetDirective`] bridge component that any installer can
//! stamp: the `dialogue.forcegreet` console door today, quest/alias
//! package installers when those paths carry one.

use crate::systems::npc_dialogue::forcegreet_open;
use byroredux_core::ecs::components::{GlobalTransform, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::{Quat, Vec3};

/// Open distance in Bethesda units — vanilla's force-greet radius family
/// is 100-200; this is the shared default both the console door and the
/// ambient Dialogue-procedure wiring install with.
pub(crate) const FORCE_GREET_RADIUS: f32 = 128.0;

/// The force-greet bridge state. One-shot: consumed the moment the
/// conversation opens (removed with the directive). `NOT_SAVED_BY_DESIGN`
/// — a save taken mid-approach re-greets after load, which is the
/// behavior the procedure describes anyway.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ForceGreetDirective {
    /// The topic the package authors (`PKDD`), or `None` for the
    /// master's generic greeting.
    pub topic: Option<u32>,
    /// Open distance in Bethesda units (vanilla's force-greet radius
    /// family is 100-200; [`FORCE_GREET_RADIUS`] is the default).
    pub radius: f32,
}

impl Component for ForceGreetDirective {
    type Storage = SparseSetStorage<Self>;
}

/// System: walk directive carriers toward the player; on arrival open
/// the conversation through the same selection the activation path
/// uses. Registered in `Stage::Update` after the ambient package
/// system. Movement rides the shared `step_toward` locomotion helper
/// (KCC-backed, navmesh-routing when a resident tile covers the actor).
pub(crate) fn forcegreet_system(world: &World, dt: f32) {
    let directives: Vec<(EntityId, ForceGreetDirective)> = world
        .query::<ForceGreetDirective>()
        .map(|query| query.iter().map(|(npc, d)| (npc, *d)).collect())
        .unwrap_or_default();
    if directives.is_empty() {
        return;
    }
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let Some(player_pos) = world.get::<GlobalTransform>(player).map(|t| t.translation) else {
        return;
    };
    // #5371 — the three-pass shape every other locomotion system
    // shares (travel.rs / patrol.rs / wander.rs / …): `PhysicsWorld` is
    // a lock-order sink ("nothing taken under it", docs/engine/ecs.md),
    // so Pass 1 gathers under storage guards only, Pass 1b takes the
    // physics resource alone for the step computation, and Pass 2
    // applies writes / opens conversations after it has dropped. The
    // single-loop shape held the physics guard across the refusal
    // reads, the transform reads/writes AND the whole
    // `forcegreet_open` dialogue stack, inverting the order production
    // records (`Transform -> PhysicsWorld -> Transform`).
    //
    // Pass 1 — gather under storage guards only.
    enum Step {
        Refuses,
        Walk { new_pos: Vec3, rotation: Option<Quat> },
        Open { topic: Option<u32> },
    }
    let mut steps: Vec<(EntityId, Step)> = Vec::with_capacity(directives.len());
    for (npc, directive) in directives {
        // Refusing NPCs (dead/combat/unconscious) drop the directive —
        // the same refusal gate the activation path applies.
        if crate::systems::npc_dialogue::npc_refuses_dialogue(world, npc).is_some() {
            steps.push((npc, Step::Refuses));
            continue;
        }
        let Some(current) = world.get::<GlobalTransform>(npc).map(|t| t.translation) else {
            continue;
        };
        let flat = Vec3::new(player_pos.x - current.x, 0.0, player_pos.z - current.z);
        if flat.length() > directive.radius {
            // Still approaching: one walk step toward the player.
            // #5373 — the step writes `Transform` (the authoritative
            // world pose on a propagation root), never
            // `GlobalTransform`: the derived global is rebuilt from
            // the local on the next propagation, so writing it
            // erased the step every frame and the NPC only ever
            // turned in place.
            let speed = world
                .get::<crate::components::WalkSpeed>(npc)
                .map(|speed| speed.0)
                .unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
            let target = Vec3::new(player_pos.x, current.y, player_pos.z);
            let rotation = world
                .get::<Transform>(npc)
                .map(|t| t.rotation)
                .unwrap_or_default();
            // Pass 1b — physics alone: nothing else is acquired under
            // the resource guard.
            let physics_guard = world.try_resource::<byroredux_physics::PhysicsWorld>();
            let (new_pos, new_rotation) = crate::systems::locomotion::step_toward(
                current,
                rotation,
                target,
                dt,
                speed,
                physics_guard.as_deref(),
            );
            steps.push((npc, Step::Walk { new_pos, rotation: new_rotation }));
            continue;
        }
        steps.push((npc, Step::Open { topic: directive.topic }));
    }
    // Pass 2 — apply the writes, then open the conversations (the
    // dialogue stack acquires its own guards; the physics guard is
    // long dropped).
    let mut opened: Vec<EntityId> = Vec::new();
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (npc, step) in &steps {
            if let Step::Walk { new_pos, rotation } = step {
                if let Some(t) = transforms.get_mut(*npc) {
                    t.translation = *new_pos;
                    if let Some(rotation) = rotation {
                        t.rotation = *rotation;
                    }
                }
            }
        }
    }
    for (npc, step) in &steps {
        match step {
            Step::Refuses => opened.push(*npc), // consume below
            Step::Open { topic } => {
                if forcegreet_open(world, *npc, *topic) {
                    opened.push(*npc);
                }
            }
            Step::Walk { .. } => {}
        }
    }
    // Consume directives whose conversation opened (or whose carrier
    // refuses one) — one greet per install.
    if !opened.is_empty() {
        if let Some(mut directives) = world.query_mut::<ForceGreetDirective>() {
            for npc in opened {
                directives.remove(npc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{GlobalTransform, Transform};

    /// #5373 — the approach step must advance `Transform` (the
    /// authoritative pose on a propagation root), never
    /// `GlobalTransform`: with the old derived-global write, transform
    /// propagation rebuilt the global from the unmoved local every
    /// frame, so a force-greet that started outside its radius turned
    /// in place and never reached the player. Drives the system and
    /// propagation alternately, the live frame shape.
    #[test]
    fn forcegreet_walk_survives_transform_propagation() {
        use byroredux_core::ecs::systems::make_transform_propagation_system;
        let mut world = World::new();
        world.register::<ForceGreetDirective>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.insert_resource(crate::systems::character::PlayerEntity(None));

        let player = world.spawn();
        world.insert(
            player,
            GlobalTransform {
                translation: Vec3::new(0.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        world.insert(player, Transform::default());
        world.insert_resource(crate::systems::character::PlayerEntity(Some(player)));

        let npc = world.spawn();
        world.insert(
            npc,
            GlobalTransform {
                translation: Vec3::new(400.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        world.insert(npc, Transform::default());
        world.insert(
            npc,
            ForceGreetDirective {
                topic: None,
                radius: FORCE_GREET_RADIUS,
            },
        );

        let mut propagate = make_transform_propagation_system();
        let mut last_distance = 400.0f32;
        for _ in 0..64 {
            forcegreet_system(&world, 0.5);
            propagate(&world, 0.5);
            let current = world
                .get::<GlobalTransform>(npc)
                .expect("global")
                .translation;
            let distance = current.distance(Vec3::ZERO);
            assert!(
                distance <= last_distance,
                "propagation must not erase the approach step (distance went \
                 {last_distance} -> {distance})"
            );
            last_distance = distance;
            if distance <= FORCE_GREET_RADIUS {
                break;
            }
        }
        assert!(
            last_distance <= FORCE_GREET_RADIUS,
            "the greeter reaches the player through alternating \\

             system+propagation frames (still {last_distance} away)"
        );
    }

    /// #5371 — with a real `PhysicsWorld` installed (every loaded cell
    /// has one), the walk path must not acquire any storage under the
    /// physics guard. Under `BYRO_LOCK_ORDER_CHECK=1` the inverted
    /// order (`Transform -> PhysicsWorld -> Transform`) aborts the
    /// detector; the release build records the edges all the same.
    /// Same shape as travel.rs's / follow.rs's physics regressions.
    #[test]
    fn forcegreet_walk_with_real_physics_world() {
        let mut world = World::new();
        world.register::<ForceGreetDirective>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.insert_resource(crate::systems::character::PlayerEntity(None));
        let player = world.spawn();
        world.insert(
            player,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        world.insert(player, Transform::default());
        world.insert_resource(crate::systems::character::PlayerEntity(Some(player)));
        let npc = world.spawn();
        world.insert(
            npc,
            GlobalTransform {
                translation: Vec3::new(400.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        world.insert(npc, Transform::default());
        world.insert(
            npc,
            ForceGreetDirective {
                topic: None,
                radius: FORCE_GREET_RADIUS,
            },
        );
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        forcegreet_system(&world, 0.5);
        let moved = world.get::<Transform>(npc).expect("transform").translation;
        assert_ne!(moved.x, 400.0, "the step computed through the physics world");
    }
}
