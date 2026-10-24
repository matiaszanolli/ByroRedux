//! FleeNotCombat procedure (M42, FO3/FNV procedure 10) — keep away from a
//! target, or hold at a refuge, without entering combat.
//!
//! GECK *Flee not Combat Package*: "fleeing from something, or fleeing to
//! somewhere". The vanilla masters author both through sub-records the
//! parser already decodes (15 FNV / 8 FO3 packages): `PTDT` names what to
//! flee from and its distance is the minimum to keep (256–2500), `PLDT` the
//! place to flee to.
//!
//! - **Flee from** (a resolvable `PTDT` target): while the target is closer
//!   than the minimum distance the actor runs straight away from it, with
//!   the same flight line `ForceFlee` uses ([`super::flee::flight_target`]);
//!   once it is far enough the actor holds, and runs again if the target
//!   closes. A target that is dead or unresolved is no threat.
//! - **Flee to** (no threat, a `NearReference` `PLDT`): walk to the refuge
//!   and hold there — the GECK's "flee to the location and cower".
//!
//! ## v0 scope
//!
//! - **Start location ignored.** The GECK has a `Flee from` package go to
//!   its `PLDT` first, then keep its distance; two vanilla packages carry
//!   both. A package with a threat flees from it and nothing else.
//! - **Refuges that are not a `NearReference`** (object id, object type,
//!   editor location) have no entity to walk to: the actor stays put.
//! - **No navmesh routing, no run speed, no cowering animation** — the same
//!   straight collide-and-slide step `ForceFlee` takes, at the actor's walk
//!   stride.
//! - **Not while fighting.** An actor in combat, suspended by a script, or
//!   already on a `ForceFlee` flight is left to those systems.

use super::flee::{flight_target, range};
use super::locomotion::step_toward_detailed;
use byroredux_core::ecs::components::{Dead, FleePackageBehavior, GlobalTransform, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use byroredux_scripting::{AiCombatState, AiSuspended};

/// How close (world units, ground plane) to its refuge a fleeing actor
/// counts as arrived. An engine choice: the GECK authors no arrival radius
/// for Flee, and the `PLDT` radius there is the area it may flee *within*.
const REFUGE_ARRIVAL: f32 = 64.0;

/// What a fleeing actor should walk toward this tick, `None` to hold.
///
/// `threat` is the threat's position when there is a live one. The
/// distances are on the ground plane, as the flight line is.
fn flight_goal(at: Vec3, threat: Option<Vec3>, keep_away: f32, refuge: Option<Vec3>) -> Option<Vec3> {
    let ground = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, 0.0, a.z - b.z).length();
    match (threat, refuge) {
        (Some(threat), _) => (ground(at, threat) < keep_away).then(|| flight_target(at, threat)),
        (None, Some(refuge)) => (ground(at, refuge) > REFUGE_ARRIVAL).then_some(refuge),
        (None, None) => None,
    }
}

struct FleeStep {
    entity: EntityId,
    speed: f32,
    at: Vec3,
    rotation: Quat,
    goal: Vec3,
}

#[derive(Default)]
struct FleePackageScratch {
    eligible: Vec<(EntityId, FleePackageBehavior)>,
    steps: Vec<FleeStep>,
    moved: Vec<(EntityId, Vec3, Option<Quat>)>,
}

fn flee_package_system_inner(world: &World, dt: f32, scratch: &mut FleePackageScratch) {
    scratch.eligible.clear();
    scratch.steps.clear();
    scratch.moved.clear();
    let Some(behavior_q) = world.query::<FleePackageBehavior>() else {
        return;
    };
    if behavior_q.is_empty() {
        return;
    }
    let detection = range(world);

    // ── Pass 0: who may flee. The state markers are read before any
    // `Transform` guard exists: `AiCombatState` is taken ahead of `Transform`
    // elsewhere, so reading it under one closes an ABBA cycle. ──
    for (entity, behavior) in behavior_q.iter() {
        if world.has::<Dead>(entity)
            || world.has::<AiCombatState>(entity)
            || world.has::<AiSuspended>(entity)
            || world.has::<super::flee::FleeState>(entity)
        {
            continue;
        }
        scratch.eligible.push((entity, *behavior));
    }

    // ── Pass 1: decide each actor's goal. `PhysicsWorld` is NOT held here
    // (#2134: it must never be live beside a `GlobalTransform` read). ──
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let walk_speed_q = world.query::<crate::components::WalkSpeed>();
        for &(entity, ref behavior) in &scratch.eligible {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let position_of = |form_id: Option<u32>| {
                let target = resolve_entity_by_global_form_id(world, form_id?)?;
                world
                    .get::<GlobalTransform>(target)
                    .map(|global| global.translation)
            };
            let threat = behavior
                .threat_form_id
                .and_then(|form_id| resolve_entity_by_global_form_id(world, form_id))
                .filter(|&target| !world.has::<Dead>(target))
                .and_then(|target| world.get::<GlobalTransform>(target).map(|g| g.translation));
            let Some(goal) = flight_goal(
                transform.translation,
                threat,
                behavior.min_distance.unwrap_or(detection),
                position_of(behavior.refuge_form_id),
            ) else {
                continue;
            };
            scratch.steps.push(FleeStep {
                entity,
                speed: crate::components::WalkSpeed::or_default(
                    walk_speed_q.as_ref().and_then(|q| q.get(entity)),
                ),
                at: transform.translation,
                rotation: transform.rotation,
                goal,
            });
        }
    }
    drop(behavior_q);
    if scratch.steps.is_empty() {
        return;
    }

    // ── Pass 2: step through collision, then write the transforms. ──
    {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for step in &scratch.steps {
            let (position, turned, _) = step_toward_detailed(
                step.at,
                step.rotation,
                step.goal,
                dt,
                step.speed,
                physics.as_deref(),
            );
            scratch.moved.push((step.entity, position, turned));
        }
    }
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for &(entity, position, turned) in &scratch.moved {
            if let Some(transform) = transforms.get_mut(entity) {
                transform.translation = position;
                if let Some(rotation) = turned {
                    transform.rotation = rotation;
                }
            }
        }
    }
}

/// The scheduler entry: [`flee_package_system_inner`] with scratch that
/// survives across frames (#2033).
pub(crate) fn make_flee_package_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = FleePackageScratch::default();
    move |world: &World, dt: f32| flee_package_system_inner(world, dt, &mut scratch)
}

#[cfg(test)]
pub(crate) fn flee_package_system(world: &World, dt: f32) {
    flee_package_system_inner(world, dt, &mut FleePackageScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn a_threat_inside_the_minimum_distance_sends_the_actor_directly_away() {
        let goal = flight_goal(Vec3::new(100.0, 5.0, 0.0), Some(Vec3::ZERO), 500.0, None)
            .expect("inside 500 units of the threat");
        assert!(goal.x > 100.0 && goal.z.abs() < 1e-3, "away along +x, got {goal:?}");
        assert_eq!(goal.y, 5.0, "level with the actor");
    }

    #[test]
    fn a_threat_beyond_the_minimum_distance_lets_the_actor_hold() {
        assert_eq!(flight_goal(Vec3::new(600.0, 0.0, 0.0), Some(Vec3::ZERO), 500.0, None), None);
        // Height does not count toward the distance kept.
        assert_eq!(flight_goal(Vec3::new(600.0, 9000.0, 0.0), Some(Vec3::ZERO), 500.0, None), None);
    }

    #[test]
    fn a_threat_outranks_the_refuge_and_no_threat_walks_to_it() {
        let refuge = Vec3::new(0.0, 0.0, 900.0);
        // Threat close: flee from it, not toward the refuge.
        let away = flight_goal(Vec3::new(100.0, 0.0, 0.0), Some(Vec3::ZERO), 500.0, Some(refuge));
        assert_ne!(away, Some(refuge));
        // Threat far: hold, the refuge is not visited.
        assert_eq!(flight_goal(Vec3::new(900.0, 0.0, 0.0), Some(Vec3::ZERO), 500.0, Some(refuge)), None);
        // No threat: go to the refuge, then stop once there.
        assert_eq!(flight_goal(Vec3::ZERO, None, 500.0, Some(refuge)), Some(refuge));
        assert_eq!(flight_goal(Vec3::new(0.0, 0.0, 880.0), None, 500.0, Some(refuge)), None);
        assert_eq!(flight_goal(Vec3::ZERO, None, 500.0, None), None);
    }

    fn world_with_target(form_id: u32, at: Vec3) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<FleePackageBehavior>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.register::<Dead>();
        world.register::<AiCombatState>();
        let mut pool = FormIdPool::new();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(form_id),
        });
        world.insert_resource(pool);
        let target = world.spawn();
        world.insert(target, FormIdComponent(fid));
        world.insert(target, GlobalTransform::new(at, Quat::IDENTITY, 1.0));
        (world, target)
    }

    fn actor_at(world: &mut World, at: Vec3, behavior: FleePackageBehavior) -> EntityId {
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(at));
        world.insert(actor, behavior);
        actor
    }

    fn position(world: &World, entity: EntityId) -> Vec3 {
        world.get::<Transform>(entity).expect("Transform").translation
    }

    const FLEE_FROM_TARGET: FleePackageBehavior = FleePackageBehavior {
        threat_form_id: Some(0x0001_59E2),
        min_distance: Some(500.0),
        refuge_form_id: None,
    };

    #[test]
    fn the_system_runs_a_close_actor_away_and_leaves_a_distant_one() {
        let (mut world, _) = world_with_target(0x0001_59E2, Vec3::ZERO);
        let near = actor_at(&mut world, Vec3::new(100.0, 0.0, 0.0), FLEE_FROM_TARGET);
        let far = actor_at(&mut world, Vec3::new(2000.0, 0.0, 0.0), FLEE_FROM_TARGET);
        flee_package_system(&world, 0.5);
        assert!(position(&world, near).x > 100.0, "the close actor runs away");
        assert_eq!(position(&world, far), Vec3::new(2000.0, 0.0, 0.0), "the distant one holds");
    }

    #[test]
    fn the_system_walks_to_a_refuge_when_there_is_no_threat() {
        let (mut world, _) = world_with_target(0x0001_59E2, Vec3::new(0.0, 0.0, 800.0));
        let actor = actor_at(
            &mut world,
            Vec3::ZERO,
            FleePackageBehavior {
                threat_form_id: None,
                min_distance: None,
                refuge_form_id: Some(0x0001_59E2),
            },
        );
        flee_package_system(&world, 0.5);
        assert!(position(&world, actor).z > 0.0, "the actor heads for the refuge");
    }

    #[test]
    fn a_dead_threat_is_no_threat_and_combat_or_death_stop_the_flight() {
        let (mut world, threat) = world_with_target(0x0001_59E2, Vec3::ZERO);
        let start = Vec3::new(100.0, 0.0, 0.0);
        let actor = actor_at(&mut world, start, FLEE_FROM_TARGET);
        world.insert(threat, Dead);
        flee_package_system(&world, 0.5);
        assert_eq!(position(&world, actor), start, "a dead threat is nothing to flee");

        world.remove::<Dead>(threat);
        world.insert(
            actor,
            AiCombatState {
                target: threat,
                attack_cooldown_remaining: 0.0,
            },
        );
        flee_package_system(&world, 0.5);
        assert_eq!(position(&world, actor), start, "a fighting actor is not this system's");

        world.remove::<AiCombatState>(actor);
        world.insert(actor, Dead);
        flee_package_system(&world, 0.5);
        assert_eq!(position(&world, actor), start, "a dead actor does not run");

        world.remove::<Dead>(actor);
        flee_package_system(&world, 0.5);
        assert!(position(&world, actor).x > start.x, "control: the same actor does run");
    }
}
