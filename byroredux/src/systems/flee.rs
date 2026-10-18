//! Running from a threat (M47.3 ObScript `ForceFlee`).
//!
//! GECK *ForceFlee*: "the actor analyzes its surroundings for potential threats, and flees if one is
//! found". The command only raises a [`FleeRequest`]; [`flee_system`] looks for the threat — an actor
//! fighting this one, or one that would attack it by the ambient hostility rules — and, if there is one,
//! takes the actor out of the AI ([`AiSuspended`]) and walks it straight away from the threat at its own
//! stride until the threat is out of detection range, when the AI takes it back.
//!
//! The engine has no run speed (combat chases at the actor's walk stride too) and no path-aware retreat:
//! the flight is a straight line through the same collide-and-slide step every walker uses.

use byroredux_core::ecs::components::{Dead, GlobalTransform, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_scripting::obscript_more::FleeRequest;
use byroredux_scripting::{AiCombatState, AiSuspended};

use super::locomotion::step_toward_detailed;

/// How far ahead (world units) the point an actor runs toward is placed along the flight line.
const FLEE_LEG: f32 = 1024.0;
/// How long a flight may stay held up against a wall before the actor gives up: it is cornered, with
/// no path-aware retreat to find another way.
const CORNERED_SECS: f32 = 1.0;
/// The detection range where the game authors none (no `fSneakMaxDistance`): how far away a threat
/// stops being one. An engine choice, as the hostility system's range is absent without that setting.
const FALLBACK_RANGE: f32 = 2048.0;

/// An actor running from `threat`. `NOT_SAVED_BY_DESIGN`: a flight lasts seconds and a save keeps the
/// actor standing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FleeState {
    pub(crate) threat: EntityId,
    /// Seconds the flight has been held up against geometry.
    pub(crate) blocked: f32,
}

impl Component for FleeState {
    type Storage = SparseSetStorage<Self>;
}

fn range(world: &World) -> f32 {
    let exterior = world.try_resource::<crate::components::CellLightingRes>().is_some_and(|cell| !cell.is_interior);
    world.try_resource::<crate::systems::DetectionConfig>().map_or(FALLBACK_RANGE, |config| config.range(exterior))
}

/// The nearest actor within `range` that fights `actor` or would attack it.
fn nearest_threat(world: &World, actor: EntityId, at: Vec3, range: f32) -> Option<EntityId> {
    let candidates: Vec<EntityId> = world.query::<byroredux_core::ecs::components::ActorVitals>()?.iter().map(|(entity, _)| entity).collect();
    let player = world.try_resource::<crate::systems::PlayerEntity>().and_then(|player| player.0);
    candidates
        .into_iter()
        .chain(player)
        .filter(|&other| other != actor && !world.has::<Dead>(other))
        .filter_map(|other| Some((other, world.get::<GlobalTransform>(other)?.translation.distance(at))))
        .filter(|&(other, distance)| {
            distance <= range
                && (world.get::<AiCombatState>(other).is_some_and(|state| state.target == actor)
                    || crate::systems::actor_would_attack(world, other, actor))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(other, _)| other)
}

pub(crate) fn flee_system(world: &World, dt: f32) {
    start_flights(world);
    run_flights(world, dt);
}

fn start_flights(world: &World) {
    let asked: Vec<EntityId> = match world.query::<FleeRequest>() {
        Some(requests) if !requests.is_empty() => requests.iter().map(|(actor, _)| actor).collect(),
        _ => return,
    };
    if let Some(mut requests) = world.query_mut::<FleeRequest>() {
        for actor in &asked {
            requests.remove(*actor);
        }
    }
    let reach = range(world);
    for actor in asked {
        let Some(at) = world.get::<GlobalTransform>(actor).map(|t| t.translation) else { continue };
        if world.has::<Dead>(actor) {
            continue;
        }
        let Some(threat) = nearest_threat(world, actor, at, reach) else {
            log::info!("ForceFlee: actor {actor} found no threat and stays");
            continue;
        };
        if let Some(mut combat) = world.query_mut::<AiCombatState>() {
            combat.remove(actor);
        }
        crate::npc_spawn::ai_package::clear_ambient_behavior(world, actor);
        if let Some(mut flights) = world.query_mut::<FleeState>() {
            flights.insert(actor, FleeState { threat, blocked: 0.0 });
        }
        if let Some(mut held) = world.query_mut::<AiSuspended>() {
            held.insert(actor, AiSuspended);
        }
        log::info!("ForceFlee: actor {actor} runs from {threat}");
    }
}

/// The horizontal point `FLEE_LEG` away from `threat`, level with `at`.
pub(crate) fn flight_target(at: Vec3, threat: Vec3) -> Vec3 {
    let away = Vec3::new(at.x - threat.x, 0.0, at.z - threat.z).normalize_or_zero();
    let away = if away == Vec3::ZERO { Vec3::X } else { away };
    Vec3::new(at.x + away.x * FLEE_LEG, at.y, at.z + away.z * FLEE_LEG)
}

fn run_flights(world: &World, dt: f32) {
    let flights: Vec<(EntityId, FleeState)> = match world.query::<FleeState>() {
        Some(flights) if !flights.is_empty() => flights.iter().map(|(actor, state)| (actor, *state)).collect(),
        _ => return,
    };
    let reach = range(world);
    let mut moves = Vec::new();
    let mut done = Vec::new();
    for (actor, state) in flights {
        let threat_at = world.get::<GlobalTransform>(state.threat).map(|t| t.translation);
        let me = world.get::<Transform>(actor).map(|t| (t.translation, t.rotation));
        match (me, threat_at) {
            (Some((at, rotation)), Some(threat_at)) if !world.has::<Dead>(actor) && !world.has::<Dead>(state.threat) && at.distance(threat_at) <= reach => {
                let speed = crate::components::WalkSpeed::or_default(world.query::<crate::components::WalkSpeed>().as_ref().and_then(|q| q.get(actor)));
                moves.push((actor, state, at, rotation, flight_target(at, threat_at), speed));
            }
            // Out of range, or either of them gone: the flight is over.
            _ => done.push(actor),
        }
    }
    if !moves.is_empty() {
        let stepped: Vec<(EntityId, FleeState, Vec3, Option<byroredux_core::math::Quat>, bool)> = {
            let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
            moves
                .iter()
                .map(|&(actor, state, at, rotation, target, speed)| {
                    let (position, turned, blocked) = step_toward_detailed(at, rotation, target, dt, speed, physics.as_deref());
                    (actor, state, position, turned, blocked)
                })
                .collect()
        };
        if let Some(mut transforms) = world.query_mut::<Transform>() {
            for &(actor, _, position, turned, _) in &stepped {
                if let Some(transform) = transforms.get_mut(actor) {
                    transform.translation = position;
                    if let Some(rotation) = turned {
                        transform.rotation = rotation;
                    }
                }
            }
        }
        if let Some(mut flights) = world.query_mut::<FleeState>() {
            for (actor, state, _, _, blocked) in stepped {
                let held = if blocked { state.blocked + dt } else { 0.0 };
                if held >= CORNERED_SECS {
                    done.push(actor);
                } else {
                    flights.insert(actor, FleeState { blocked: held, ..state });
                }
            }
        }
    }
    for actor in done {
        if let Some(mut flights) = world.query_mut::<FleeState>() {
            flights.remove(actor);
        }
        if let Some(mut held) = world.query_mut::<AiSuspended>() {
            held.remove(actor);
        }
        log::info!("ForceFlee: actor {actor} is clear and stops");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flight_line_runs_directly_away_from_the_threat_on_the_ground_plane() {
        let target = flight_target(Vec3::new(10.0, 5.0, 0.0), Vec3::new(0.0, 0.0, 0.0));
        assert!((target.x - (10.0 + FLEE_LEG)).abs() < 1e-3 && target.z.abs() < 1e-3);
        assert_eq!(target.y, 5.0, "level with the actor");
        // Standing on the threat still picks a direction.
        assert!(flight_target(Vec3::ZERO, Vec3::ZERO).length() > 0.0);
    }
}
