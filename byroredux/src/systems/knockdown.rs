//! Knocking an actor down and getting it up again (M47.3 ObScript `PushActorAway`).
//!
//! GECK *PushActorAway*: "knocks down (ragdoll-izes) one actor and pushes it away from another with a
//! given force ... Fred will slide past Buddy ... an actor that has been pushed can have its inventory
//! accessed by the player until the actor gets up again." A negative force pulls the actor toward the
//! pusher. The command only raises a [`PushActorRequest`]; [`push_actor_system`] turns the actor into a
//! simulated ragdoll (the skeleton's animation and the actor's AI stop, as they do for a death) and gives
//! it the push, and [`get_up_system`] stands it up where it came to rest.
//!
//! What no source gives, and is therefore an engine choice rather than a quotation: the unit of the
//! force (the GECK says only "a havok force"; it is applied here as a velocity change of that many world
//! units per second, capped at the `fKnockbackForceMax` setting, 512 in the Fallout masters), and the
//! moment the actor gets up (one second after the ragdoll has come to rest).

use byroredux_core::ecs::components::{Dead, GlobalTransform, Transform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_physics::{PhysicsWorld, Ragdoll};
use byroredux_scripting::obscript_more::PushActorRequest;

use crate::components::AnimationTarget;

/// `fKnockbackForceMax` where the load order authors none.
const DEFAULT_FORCE_MAX: f32 = 512.0;
/// The ragdoll's speed (world units per second) below which it counts as at rest.
const REST_SPEED: f32 = 8.0;
/// How long the ragdoll must stay at rest before the actor gets up.
const REST_SECS: f32 = 1.0;
/// How much of the push is aimed upward: the actor is thrown, not dragged along the floor.
const LIFT: f32 = 0.35;

/// The cap on a push's force.
#[derive(Debug, Clone, Copy)]
pub(crate) struct KnockbackForceMax(pub f32);

impl Resource for KnockbackForceMax {}

/// An actor that is down after a push. `NOT_SAVED_BY_DESIGN`: the ragdoll it follows is not saved either
/// (a save keeps the actor standing).
#[derive(Debug, Clone, Copy)]
pub(crate) struct KnockedDown {
    /// Seconds the ragdoll has been at rest.
    pub(crate) rested: f32,
    /// The target the actor was fighting, to take up again on getting up.
    pub(crate) fighting: Option<EntityId>,
}

impl Component for KnockedDown {
    type Storage = SparseSetStorage<Self>;
}

pub(crate) fn install_force_max(world: &mut World, index: &byroredux_plugin::esm::records::EsmIndex) {
    world.insert_resource(KnockbackForceMax(index.game_setting_float("fKnockbackForceMax").unwrap_or(DEFAULT_FORCE_MAX)));
}

fn skeleton_root(world: &World, actor: EntityId) -> EntityId {
    world.get::<AnimationTarget>(actor).map_or(actor, |target| target.skeleton_root)
}

/// Knock down the actors a script pushed this frame.
pub(crate) fn push_actor_system(world: &World, _dt: f32) {
    for (actor, request) in super::take_requests::<PushActorRequest>(world) {
        match knock_down(world, actor, request) {
            Ok(note) => log::info!("PushActorAway: actor {actor}: {note}"),
            Err(why) => log::warn!("PushActorAway: actor {actor} not pushed: {why}"),
        }
    }
}

/// The horizontal direction from `from` to `actor` (a negative force flips it), lifted by [`LIFT`].
fn push_direction(from: Vec3, actor: Vec3, force: f32) -> Vec3 {
    let away = super::level_away(from, actor);
    let sign = if force < 0.0 { -1.0 } else { 1.0 };
    (away * sign + Vec3::Y * LIFT).normalize()
}

fn knock_down(world: &World, actor: EntityId, request: PushActorRequest) -> Result<String, String> {
    if world.has::<Dead>(actor) {
        return Err("it is dead".into());
    }
    // #6230 — the player's visual body is built without a ragdoll, and its idle and controls have no
    // get-up path to come back through.
    if crate::systems::PlayerEntity::get(world) == Some(actor) {
        return Err("it is the player".into());
    }
    let root = skeleton_root(world, actor);
    let positions = (
        world.get::<GlobalTransform>(request.from).map(|t| t.translation),
        world.get::<GlobalTransform>(actor).map(|t| t.translation),
    );
    let (Some(from), Some(at)) = positions else { return Err("the pusher or the actor is not placed".into()) };
    // #6230 — the ragdoll first: an actor it cannot be built for (no template, a refused seed pose)
    // keeps its animation, package and fight, since only a get-up would give them back. Activation seeds
    // from the current pose, which the release below does not touch.
    if world.get::<crate::ragdoll::RagdollActive>(root).is_none() {
        crate::ragdoll::activate_ragdoll(world, root)?;
    }
    // The animation and the AI that would fight the simulation stop, as they do for a death.
    let fighting = world.get::<byroredux_scripting::AiCombatState>(actor).map(|state| state.target);
    release_actor_for_ragdoll(world, actor, root);
    let cap = world.try_resource::<KnockbackForceMax>().map_or(DEFAULT_FORCE_MAX, |max| max.0);
    let speed = request.force.abs().min(cap);
    let direction = push_direction(from, at, request.force);
    let bodies: Vec<_> = world.get::<Ragdoll>(root).map(|ragdoll| ragdoll.bodies.iter().map(|(_, handle, _)| *handle).collect()).unwrap_or_default();
    if let Some(mut physics) = world.try_resource_mut::<PhysicsWorld>() {
        // Every body gets the same velocity change: the impulse is its mass times it.
        for handle in &bodies {
            if let Some(mass) = physics.body_mass(*handle) {
                physics.apply_impulse(*handle, direction * speed * mass);
            }
        }
    }
    if let Some(mut down) = world.query_mut::<KnockedDown>() {
        down.insert(actor, KnockedDown { rested: 0.0, fighting });
    }
    // An actor that is down neither walks nor thinks: the ambient AI skips it until it gets up.
    if let Some(mut held) = world.query_mut::<byroredux_scripting::AiSuspended>() {
        held.insert(actor, byroredux_scripting::AiSuspended);
    }
    Ok(format!("knocked down ({} bodies) at {speed} u/s", bodies.len()))
}

/// Stop what would fight the simulation for the pose: the animation on the actor and on its skeleton,
/// the ambient package's mover, and a fight in progress (taken up again on getting up).
fn release_actor_for_ragdoll(world: &World, actor: EntityId, root: EntityId) {
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    fn remove<T: Component>(world: &World, entity: EntityId) {
        if let Some(mut query) = world.query_mut::<T>() {
            query.remove(entity);
        }
    }
    crate::npc_spawn::ai_package::clear_ambient_behavior(world, actor);
    remove::<byroredux_scripting::AiCombatState>(world, actor);
    for entity in [actor, root] {
        remove::<AnimationPlayer>(world, entity);
        remove::<AnimationStack>(world, entity);
    }
}

/// The mean position of the ragdoll's bodies.
fn ragdoll_centre(world: &World, root: EntityId) -> Option<(Vec3, f32)> {
    let ragdoll = world.get::<Ragdoll>(root)?;
    let physics = world.try_resource::<PhysicsWorld>()?;
    let (mut sum, mut speed, mut count) = (Vec3::ZERO, 0.0f32, 0.0f32);
    for (_, handle, _) in &ragdoll.bodies {
        let motion = physics.body_motion(*handle)?;
        sum += motion.position;
        speed = speed.max(motion.velocity.length());
        count += 1.0;
    }
    (count > 0.0).then(|| (sum / count, speed))
}

/// Stand the knocked-down actors up once their ragdoll has rested.
pub(crate) fn get_up_system(world: &World, dt: f32) {
    let down: Vec<(EntityId, KnockedDown)> = match world.query::<KnockedDown>() {
        Some(down) if !down.is_empty() => down.iter().map(|(actor, state)| (actor, *state)).collect(),
        _ => return,
    };
    for (actor, state) in down {
        if world.has::<Dead>(actor) {
            // It died while down: the corpse keeps its ragdoll.
            release(world, actor);
            continue;
        }
        let root = skeleton_root(world, actor);
        let Some((centre, speed)) = ragdoll_centre(world, root) else {
            release(world, actor);
            continue;
        };
        let rested = if speed < REST_SPEED { state.rested + dt } else { 0.0 };
        if rested < REST_SECS {
            if let Some(mut downs) = world.query_mut::<KnockedDown>() {
                downs.insert(actor, KnockedDown { rested, ..state });
            }
            continue;
        }
        // Where it came to rest: the placement root moves there, onto the ground under it.
        let ground = world
            .try_resource::<PhysicsWorld>()
            .and_then(|physics| physics.cast_ray_down(centre + Vec3::Y * 64.0, 512.0, None));
        let notes = crate::systems::revive::stand_up(world, actor);
        if let Some(mut transforms) = world.query_mut::<Transform>() {
            if let Some(transform) = transforms.get_mut(actor) {
                transform.translation = Vec3::new(centre.x, ground.unwrap_or(transform.translation.y), centre.z);
            }
        }
        if let Some(target) = state.fighting.filter(|&target| !world.has::<Dead>(target)) {
            if let Some(mut combat) = world.query_mut::<byroredux_scripting::AiCombatState>() {
                combat.insert(actor, byroredux_scripting::AiCombatState { target, attack_cooldown_remaining: 0.0 });
            }
        }
        release(world, actor);
        log::info!("actor {actor} got up: {}", notes.join("; "));
    }
}

fn release(world: &World, actor: EntityId) {
    if let Some(mut downs) = world.query_mut::<KnockedDown>() {
        downs.remove(actor);
    }
    if let Some(mut held) = world.query_mut::<byroredux_scripting::AiSuspended>() {
        held.remove(actor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_runs_away_from_the_pusher_and_a_pull_toward_it_both_lifted() {
        let pusher = Vec3::new(0.0, 0.0, 0.0);
        let actor = Vec3::new(100.0, 0.0, 0.0);
        let away = push_direction(pusher, actor, 10.0);
        assert!(away.x > 0.0 && away.y > 0.0 && away.z.abs() < 1e-6);
        let toward = push_direction(pusher, actor, -10.0);
        assert!(toward.x < 0.0 && toward.y > 0.0);
        // Standing on the pusher still goes somewhere.
        assert!(push_direction(pusher, pusher, 5.0).length() > 0.99);
    }

    /// An actor and a pusher a short way apart; the actor animates and is fighting `target`.
    fn placed_pair() -> (World, EntityId, EntityId, EntityId) {
        use byroredux_core::animation::AnimationPlayer;
        let mut world = World::new();
        let (pusher, actor, target) = (world.spawn(), world.spawn(), world.spawn());
        world.insert(pusher, GlobalTransform::new(Vec3::ZERO, Default::default(), 1.0));
        world.insert(actor, GlobalTransform::new(Vec3::new(100.0, 0.0, 0.0), Default::default(), 1.0));
        world.insert(actor, AnimationPlayer::new(0));
        world.insert(actor, byroredux_scripting::AiCombatState { target, attack_cooldown_remaining: 0.0 });
        (world, pusher, actor, target)
    }

    fn assert_untouched(world: &World, actor: EntityId, target: EntityId) {
        assert!(world.has::<byroredux_core::animation::AnimationPlayer>(actor), "the idle must survive");
        assert_eq!(world.get::<byroredux_scripting::AiCombatState>(actor).map(|state| state.target), Some(target));
        assert!(!world.has::<KnockedDown>(actor) && !world.has::<byroredux_scripting::AiSuspended>(actor));
    }

    /// #6230 — an actor with no ragdoll to fall into keeps its animation and its fight.
    #[test]
    fn a_push_on_an_actor_without_a_ragdoll_releases_nothing() {
        let (world, pusher, actor, target) = placed_pair();
        let error = knock_down(&world, actor, PushActorRequest { from: pusher, force: 100.0 }).unwrap_err();
        assert!(error.contains("RagdollTemplate"), "{error}");
        assert_untouched(&world, actor, target);
    }

    /// #6230 — the player is refused outright.
    #[test]
    fn a_push_on_the_player_is_refused() {
        let (mut world, pusher, actor, target) = placed_pair();
        world.insert_resource(crate::systems::PlayerEntity(Some(actor)));
        let error = knock_down(&world, actor, PushActorRequest { from: pusher, force: 100.0 }).unwrap_err();
        assert!(error.contains("player"), "{error}");
        assert_untouched(&world, actor, target);
    }
}
