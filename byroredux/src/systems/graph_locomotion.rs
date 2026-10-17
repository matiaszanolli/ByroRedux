//! Locomotion inputs for actors that run a behavior graph (M47.2).
//!
//! A Fallout 4 actor's `MTBehavior` walks, jogs and runs from the *variables* the engine
//! feeds it: `Speed` (world units a second), `Direction` (the heading of the motion
//! relative to the body, a fraction of a turn clockwise from straight ahead) and the
//! `moveStart` / `moveStop` events (CK: a graph "state machine whose transitions fire on
//! named events"). This system derives them from the actor's own displacement, the way
//! `npc_walk_animation_system` classifies the same motion for the Skyrim walk clip: the
//! locomotion procedures and combat chase move the root, and whatever moved it, the graph
//! sees the speed and heading that resulted. The graph's blenders (`Speed` between its walk
//! and run clips, `Direction` round its eight-way cycle) and its transition fades then do
//! the rest.
//!
//! The hysteresis windows are the walk-clip system's (a collision slide must not cut the
//! gait), and an actor that system already drives is left to it.

use super::walk_anim::{WALK_ANIM_MIN_SPEED, WALK_ANIM_RELEASE_DWELL, WALK_ANIM_TAKE_DWELL};
use crate::components::WalkAnimation;
use byroredux_core::ecs::components::{Dead, Seated, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::animation_graph::{play_animation, set_variable_value};
use byroredux_scripting::ObjectAnimationGraph;

/// A displacement faster than this in one tick is a teleport (a cell change, a script's
/// `MoveTo`), not walking: the baseline moves and no speed is read from it. A sprinting
/// actor is well under it.
const TELEPORT_SPEED: f32 = 1500.0;

/// Time constant of the speed smoothing, seconds: the per-frame displacement is noisy
/// (a fixed-step mover, collision slides) and the graph blends on it.
const SPEED_SMOOTHING_SECS: f32 = 0.1;

/// One actor's locomotion state. `NOT_SAVED_BY_DESIGN`: re-derived from the next frames'
/// displacement.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GraphLocomotion {
    last_pos: Vec3,
    /// Smoothed speed, world units a second.
    speed: f32,
    /// Whether `moveStart` has been sent (and `moveStop` has not).
    moving: bool,
    /// How long the motion has disagreed with `moving`.
    dwell: f32,
}

impl Component for GraphLocomotion {
    type Storage = SparseSetStorage<Self>;
}

/// Which way a behavior-graph actor faces in its own space: the Havok clips' forward is +Y
/// in Gamebryo's Z-up space, which is -Z once converted (`zup_to_yup_pos`). Measured on a
/// live Fallout 4 actor: with the root unrotated, the left foot's toe sits 8.4 units further
/// along -Z than the foot (the skeleton's reference pose agrees, toe at +8.5 Y Z-up). The
/// body's right-hand side is +X (its right foot is at +X in the same pose).
pub(crate) const GRAPH_MODEL_FORWARD: Vec3 = Vec3::NEG_Z;
pub(crate) const GRAPH_MODEL_RIGHT: Vec3 = Vec3::X;

/// The heading of `moved` relative to a body with `rotation`, as a fraction of a turn
/// clockwise (seen from above) from straight ahead: 0 forward, 0.25 right, 0.5 back, 0.75
/// left - the positions of the eight-way blenders' children.
pub(crate) fn heading_fraction(moved: Vec3, rotation: Quat) -> f32 {
    let forward = rotation * GRAPH_MODEL_FORWARD;
    let right = rotation * GRAPH_MODEL_RIGHT;
    let angle = moved.dot(right).atan2(moved.dot(forward));
    (angle / std::f32::consts::TAU).rem_euclid(1.0)
}

/// An actor the console is walking in a straight line (`actor.walk`): a stand-in mover
/// for exercising what the locomotion systems feed a graph on actors no package moves
/// yet. `NOT_SAVED_BY_DESIGN`: a debugging aid with a lifetime of seconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ConsoleWalk {
    /// World-space velocity, units a second.
    pub(crate) velocity: Vec3,
    pub(crate) remaining: f32,
}

impl Component for ConsoleWalk {
    type Storage = SparseSetStorage<Self>;
}

/// Move every [`ConsoleWalk`] actor along its velocity (its facing is left alone, so a
/// strafe or a backward walk reads as one). Runs before the graph's inputs are read.
pub(crate) fn console_walk_system(world: &World, dt: f32) {
    let Some(mut walks) = world.query_mut::<ConsoleWalk>() else { return };
    if walks.iter().next().is_none() {
        return;
    }
    let Some(mut transforms) = world.query_mut::<Transform>() else { return };
    let mut done = Vec::new();
    for (entity, walk) in walks.iter_mut() {
        if !dt.is_finite() || !walk.remaining.is_finite() || !walk.velocity.is_finite() {
            done.push(entity);
            continue;
        }
        let step = dt.min(walk.remaining).max(0.0);
        if let Some(transform) = transforms.get_mut(entity) {
            let destination = transform.translation + walk.velocity * step;
            if byroredux_physics::PhysicsWorld::translation_is_sane(destination) {
                transform.translation = destination;
            } else {
                done.push(entity);
                continue;
            }
        }
        walk.remaining -= step;
        if walk.remaining <= 0.0 {
            done.push(entity);
        }
    }
    for entity in done {
        walks.remove(entity);
    }
}

struct Observed {
    entity: EntityId,
    position: Vec3,
    rotation: Quat,
    parked: bool,
}

/// Feed every graph-driven actor's `Speed` / `Direction` variables and `moveStart` /
/// `moveStop` events from its motion; see the module docs. Exclusive, after the movers.
pub(crate) fn graph_locomotion_system(world: &World, dt: f32) {
    let observed: Vec<Observed> = {
        let Some(graphs) = world.query::<ObjectAnimationGraph>() else { return };
        let Some(transforms) = world.query::<Transform>() else { return };
        let walkers = world.query::<WalkAnimation>();
        let seated = world.query::<Seated>();
        let dead = world.query::<Dead>();
        let cinematic = world.query::<byroredux_scripting::ActorCinematicState>();
        graphs
            .iter()
            .filter(|(entity, _)| !walkers.as_ref().is_some_and(|walkers| walkers.contains(*entity)))
            .filter_map(|(entity, _)| {
                let transform = transforms.get(entity)?;
                Some(Observed {
                    entity,
                    position: transform.translation,
                    rotation: transform.rotation,
                    parked: seated.as_ref().is_some_and(|q| q.contains(entity))
                        || dead.as_ref().is_some_and(|q| q.contains(entity))
                        || cinematic.as_ref().is_some_and(|q| q.contains(entity)),
                })
            })
            .collect()
    };
    if observed.is_empty() {
        return;
    }
    let known = |entity| world.query::<GraphLocomotion>().and_then(|q| q.get(entity).copied());
    let mut next: Vec<(EntityId, GraphLocomotion)> = Vec::with_capacity(observed.len());
    for actor in observed {
        let Some(mut state) = known(actor.entity) else {
            next.push((actor.entity, GraphLocomotion { last_pos: actor.position, speed: 0.0, moving: false, dwell: 0.0 }));
            continue;
        };
        let moved = Vec3::new(actor.position.x - state.last_pos.x, 0.0, actor.position.z - state.last_pos.z);
        state.last_pos = actor.position;
        let raw = if dt > f32::MIN_POSITIVE && dt < 0.25 { moved.length() / dt } else { 0.0 };
        let raw = if raw > TELEPORT_SPEED || actor.parked { 0.0 } else { raw };
        let blend = 1.0 - (-dt.max(0.0) / SPEED_SMOOTHING_SECS).exp();
        state.speed += (raw - state.speed) * blend;
        let in_motion = state.speed > WALK_ANIM_MIN_SPEED;
        if in_motion != state.moving {
            state.dwell += dt;
            let window = if in_motion { WALK_ANIM_TAKE_DWELL } else { WALK_ANIM_RELEASE_DWELL };
            if state.dwell >= window {
                state.moving = in_motion;
                state.dwell = 0.0;
                let event = if in_motion { "moveStart" } else { "moveStop" };
                let accepted = play_animation(world, actor.entity, event);
                log::debug!("graph locomotion: {event} on entity {} - {}", actor.entity, if accepted { "taken" } else { "no transition" });
            }
        } else {
            state.dwell = 0.0;
        }
        let speed = if state.moving { state.speed } else { 0.0 };
        set_variable_value(world, actor.entity, "Speed", speed);
        set_variable_value(world, actor.entity, "SpeedSmoothed", speed);
        if raw > WALK_ANIM_MIN_SPEED {
            set_variable_value(world, actor.entity, "Direction", heading_fraction(moved, actor.rotation));
        }
        next.push((actor.entity, state));
    }
    if let Some(mut states) = world.query_mut::<GraphLocomotion>() {
        for (entity, state) in next {
            states.insert(entity, state);
        }
    }
}

/// Apply the root motion of a graph-driven actor's animation to the actor.
///
/// Fallout 4's locomotion clips are authored in place, so the locomotion systems above
/// move the actor and the graph merely reads that motion. When the graph itself carries
/// the actor - `bGraphDriven` / `bAnimationDriven` set, as in the transitions that walk it
/// into a seat or turn it on the spot - the displacement the playing clips extract (the
/// animation system leaves it in `RootMotionDelta`, blended across the stack's layers) is
/// the movement: this moves the actor by it, in the actor's own frame, and consumes it.
/// Yaw in the extracted motion is not applied.
pub(crate) fn graph_root_motion_system(world: &World, _dt: f32) {
    use byroredux_core::animation::RootMotionDelta;
    let actors: Vec<(EntityId, Quat)> = {
        let Some(graphs) = world.query::<ObjectAnimationGraph>() else { return };
        let Some(transforms) = world.query::<Transform>() else { return };
        graphs
            .iter()
            .filter_map(|(entity, graph)| {
                let driven = ["bGraphDriven", "bAnimationDriven"]
                    .iter()
                    .any(|name| graph.variable(name).is_some_and(|value| value != 0.0));
                driven.then(|| transforms.get(entity).map(|transform| (entity, transform.rotation))).flatten()
            })
            .collect()
    };
    if actors.is_empty() {
        return;
    }
    // The animation system writes only where the component exists.
    if let Some(mut deltas) = world.query_mut::<RootMotionDelta>() {
        for (entity, _) in &actors {
            if deltas.get(*entity).is_none() {
                deltas.insert(*entity, RootMotionDelta(Vec3::ZERO));
            }
        }
    }
    let motions: Vec<(EntityId, Vec3, Quat)> = match world.query::<RootMotionDelta>() {
        Some(deltas) => actors
            .iter()
            .filter_map(|(entity, rotation)| {
                let delta = deltas.get(*entity)?.0;
                (delta.is_finite() && delta != Vec3::ZERO).then_some((*entity, delta, *rotation))
            })
            .collect(),
        None => return,
    };
    if motions.is_empty() {
        return;
    }
    let mut moved = Vec::with_capacity(motions.len());
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (entity, delta, rotation) in &motions {
            if let Some(transform) = transforms.get_mut(*entity) {
                transform.translation += *rotation * (*delta * transform.scale);
                moved.push((*entity, transform.translation));
            }
        }
    }
    if let Some(mut deltas) = world.query_mut::<RootMotionDelta>() {
        for (entity, _, _) in &motions {
            if let Some(delta) = deltas.get_mut(*entity) {
                delta.0 = Vec3::ZERO;
            }
        }
    }
    for (entity, position) in moved {
        byroredux_physics::set_kinematic_translation(world, entity, position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    use byroredux_hkx::{BehaviorGraph, ClipMode, State, StateClip, StateMachine, Transition, Variable, VariableKind};
    use byroredux_scripting::{GraphClip, LoadedGraph};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn clip(name: &str) -> StateClip {
        StateClip {
            animation: name.into(),
            mode: ClipMode::Looping,
            playback_speed: 1.0,
            start_time: 0.0,
            user_fraction: 0.0,
            triggers: Vec::new(),
            playback_speed_variable: None,
        }
    }

    /// Idle (state 0) --moveStart--> Move (state 1) --moveStop--> Idle.
    fn graph() -> ObjectAnimationGraph {
        let go = |event, to_state| Transition { event, to_state, priority: 0, flags: 0, ..Default::default() };
        let state = |id, name: &str, transitions, clip_name| State {
            id,
            name: name.into(),
            transitions,
            clip: Some(clip(clip_name)),
            content: Default::default(),
        };
        let real = |name: &str| Variable { name: name.into(), kind: VariableKind::Real, initial: 0.0 };
        let loaded = BehaviorGraph {
            variables: vec![real("Speed"), real("SpeedSmoothed"), real("Direction")],
            events: vec!["moveStart".into(), "moveStop".into()],
            machine: Some(StateMachine {
                start_state: 0,
                start_state_variable: None,
                states: vec![state(0, "Idle", vec![go(0, 1)], "idle"), state(1, "Move", vec![go(1, 0)], "walk")],
                wildcard: Vec::new(),
            }),
        };
        let mut clips = HashMap::new();
        clips.insert(("idle".to_owned(), true), GraphClip { handle: 1, duration: 2.0 });
        clips.insert(("walk".to_owned(), true), GraphClip { handle: 2, duration: 1.0 });
        ObjectAnimationGraph::new(Arc::new(LoadedGraph { graph: loaded, clips, sequences: HashMap::new(), lazy: None }), 0).unwrap()
    }

    fn world_with_actor() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<ObjectAnimationGraph>();
        world.register::<GraphLocomotion>();
        world.register::<Transform>();
        world.register::<AnimationPlayer>();
        world.register::<AnimationStack>();
        let actor = world.spawn();
        let mut graph = graph();
        graph.root = actor;
        world.insert(actor, graph);
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        (world, actor)
    }

    fn walk(world: &World, actor: EntityId, ticks: usize, dt: f32, step: Vec3) {
        for _ in 0..ticks {
            let mut transforms = world.query_mut::<Transform>().unwrap();
            transforms.get_mut(actor).unwrap().translation += step;
            drop(transforms);
            graph_locomotion_system(world, dt);
        }
    }

    fn variable(world: &World, actor: EntityId, name: &str) -> f32 {
        byroredux_scripting::animation_graph::variable_value(world, actor, name).unwrap()
    }

    /// Straight ahead is 0, a quarter turn right 0.25, backwards 0.5, left 0.75 -
    /// wrapping, whatever way the body is turned.
    #[test]
    fn the_heading_is_a_fraction_of_a_turn_from_straight_ahead() {
        let facing = |yaw: f32| Quat::from_rotation_y(yaw);
        let at = |moved: Vec3, yaw: f32| heading_fraction(moved, facing(yaw));
        assert!(at(Vec3::NEG_Z, 0.0).abs() < 1e-5, "an unrotated body faces -Z");
        assert!((at(Vec3::X, 0.0) - 0.25).abs() < 1e-5, "its right is +X");
        assert!((at(Vec3::Z, 0.0) - 0.5).abs() < 1e-5);
        assert!((at(Vec3::NEG_X, 0.0) - 0.75).abs() < 1e-5);
        // Turned a quarter (yaw +90 degrees takes -Z to -X): walking along -X is ahead.
        let ahead = at(Vec3::NEG_X, std::f32::consts::FRAC_PI_2);
        assert!(!(1e-5..=1.0 - 1e-5).contains(&ahead), "{ahead}");
    }

    /// A moving actor feeds its graph the speed it walks and starts the move state; a
    /// stopped one zeroes the speed and takes `moveStop` after the release window.
    #[test]
    fn an_actor_that_walks_feeds_the_graph_its_speed_and_events() {
        let (world, actor) = world_with_actor();
        graph_locomotion_system(&world, 0.016); // baseline
        // 100 units a second, forward.
        walk(&world, actor, 40, 0.016, Vec3::new(0.0, 0.0, -1.6));
        let speed = variable(&world, actor, "Speed");
        assert!((speed - 100.0).abs() < 2.0, "smoothed to the walked speed, got {speed}");
        assert_eq!(variable(&world, actor, "SpeedSmoothed"), speed);
        let direction = variable(&world, actor, "Direction");
        assert!(!(0.01..=0.99).contains(&direction), "walking along its facing: {direction}");
        let graphs = world.query::<ObjectAnimationGraph>().unwrap();
        assert_eq!(graphs.get(actor).unwrap().state(), 1, "moveStart moved the graph to its move state");
        drop(graphs);

        walk(&world, actor, 80, 0.016, Vec3::ZERO);
        assert_eq!(variable(&world, actor, "Speed"), 0.0);
        let graphs = world.query::<ObjectAnimationGraph>().unwrap();
        assert_eq!(graphs.get(actor).unwrap().state(), 0, "moveStop returned it to the idle");
    }

    /// A graph-driven actor is carried by the root motion its clips extract, in its own
    /// frame; a locomotion-driven one is not (its movers move it).
    #[test]
    fn root_motion_moves_only_a_graph_driven_actor() {
        use byroredux_core::animation::RootMotionDelta;
        let (mut world, actor) = world_with_actor();
        world.register::<RootMotionDelta>();
        // Rebuild the graph with the driven flag among its variables.
        let mut shared = (*world.get::<ObjectAnimationGraph>(actor).unwrap().graph_arc()).clone();
        shared.graph.variables.push(Variable { name: "bGraphDriven".into(), kind: VariableKind::Bool, initial: 0.0 });
        let mut graph = ObjectAnimationGraph::new(Arc::new(shared), actor).unwrap();
        graph.root = actor;
        world.insert(actor, graph);
        world.insert(actor, RootMotionDelta(Vec3::new(0.0, 0.0, -10.0)));
        world.query_mut::<Transform>().unwrap().get_mut(actor).unwrap().rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);

        graph_root_motion_system(&world, 0.016);
        assert_eq!(world.get::<Transform>(actor).unwrap().translation, Vec3::ZERO, "not graph-driven: left to the movers");

        byroredux_scripting::animation_graph::set_variable_value(&world, actor, "bGraphDriven", 1.0);
        graph_root_motion_system(&world, 0.016);
        let moved = world.get::<Transform>(actor).unwrap().translation;
        // Forward (-Z) turned a quarter by +90 degrees of yaw is -X.
        assert!((moved - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-4, "{moved:?}");
        assert_eq!(world.get::<RootMotionDelta>(actor).unwrap().0, Vec3::ZERO, "consumed");
    }

    /// A one-tick jump (a teleport) and a seated actor are not walking.
    #[test]
    fn teleports_and_parked_actors_do_not_read_as_walking() {
        let (world, actor) = world_with_actor();
        graph_locomotion_system(&world, 0.016);
        walk(&world, actor, 1, 0.016, Vec3::new(0.0, 0.0, 5000.0));
        walk(&world, actor, 20, 0.016, Vec3::ZERO);
        assert_eq!(variable(&world, actor, "Speed"), 0.0);
        let graphs = world.query::<ObjectAnimationGraph>().unwrap();
        assert_eq!(graphs.get(actor).unwrap().state(), 0, "never started moving");
    }

    /// #5861 — death takes the behavior-graph runtime with the animation
    /// players: left behind, the graph step and `moveStop` put a clip back on
    /// the corpse (FO4 ragdolls do not activate yet to override the pose).
    #[test]
    fn a_dead_actor_loses_its_graph_and_locomotion_state() {
        let (mut world, actor) = world_with_actor();
        world.register::<ConsoleWalk>();
        world.register::<Dead>();
        walk(&world, actor, 3, 0.1, Vec3::ZERO);
        world.insert(actor, ConsoleWalk { velocity: Vec3::X, remaining: 5.0 });
        world.insert(actor, Dead);
        assert!(world.get::<GraphLocomotion>(actor).is_some(), "the fixture built locomotion state");
        crate::combat::reconcile_dead_actor(&world, actor);
        assert!(world.get::<ObjectAnimationGraph>(actor).is_none());
        assert!(world.get::<GraphLocomotion>(actor).is_none());
        assert!(world.get::<ConsoleWalk>(actor).is_none());
    }

    #[test]
    fn console_walk_discards_non_finite_motion_without_poisoning_transform() {
        let mut world = World::new();
        world.register::<ConsoleWalk>();
        world.register::<Transform>();
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(actor, ConsoleWalk { velocity: Vec3::splat(f32::INFINITY), remaining: f32::NAN });

        console_walk_system(&world, 1.0 / 60.0);

        assert_eq!(world.get::<Transform>(actor).unwrap().translation, Vec3::ZERO);
        assert!(world.get::<ConsoleWalk>(actor).is_none());
    }
}
