//! Bringing an actor back (M47.3 ObScript `Resurrect`).
//!
//! CS *Resurrect*: "resurrects the calling actor. If the AnimateFlag argument is 1, the actor will animate
//! getting up (as if from a knockdown)"; GECK *ResurrectActor*: "resurrects an actor from the dead or
//! unconscious state". The script command only raises a [`ReviveRequest`]; this system undoes what death
//! did, in place, so every reference to the actor — scripts, quests, the placed reference — still means
//! the same actor:
//!
//! * the unconscious flag is cleared, and for a dead actor the [`Dead`] marker and any bleedout;
//! * Health and the limb conditions get their damage back (a resurrected actor with no health would die
//!   again on its first frame);
//! * the simulated ragdoll is freed and the bones' keyframed collision bodies are put back, so the actor
//!   is hittable and the animation drives the skeleton again;
//! * the actor's generic idle plays again, and its AI package stack is re-armed (the ambient AI picks the
//!   winning package on its next pass, as it does for a fresh spawn).
//!
//! Not modelled: the get-up animation the animate flag asks for (the actor stands at once, whatever the
//! flag), and the items the CS says "may" be removed. The death *count* is not lowered (GECK *GetDeadCount*:
//! "Resurrecting a killed actor will not lower the death count").

use byroredux_core::ecs::components::{Dead, MotionType, RigidBodyData};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_physics::{PhysicsWorld, Ragdoll};
use byroredux_scripting::obscript_state::ReviveRequest;

use crate::components::{AmbientPackageRuntime, AnimationTarget};
use crate::ragdoll::{RagdollActive, RagdollTemplate};

/// Revive the actors `Resurrect` was called on this frame.
pub(crate) fn revive_system(world: &World, _dt: f32) {
    let requested: Vec<EntityId> = match world.query::<ReviveRequest>() {
        Some(requests) if !requests.is_empty() => requests.iter().map(|(actor, _)| actor).collect(),
        _ => return,
    };
    if let Some(mut requests) = world.query_mut::<ReviveRequest>() {
        for actor in &requested {
            requests.remove(*actor);
        }
    }
    for actor in requested {
        log::info!("actor {actor}: {}", revive_actor(world, actor));
    }
}

/// Undo `actor`'s death or unconsciousness. The returned note says what was done, for the log.
pub(crate) fn revive_actor(world: &World, actor: EntityId) -> String {
    byroredux_scripting::update_actor_control(world, actor, |state| state.set_unconscious(false));
    if !world.has::<Dead>(actor) {
        return "woken (it was not dead)".to_owned();
    }
    if let Some(mut dead) = world.query_mut::<Dead>() {
        dead.remove(actor);
    }
    if let Some(mut bleedout) = world.query_mut::<byroredux_scripting::Bleedout>() {
        bleedout.remove(actor);
    }
    byroredux_scripting::obscript_actor_values::reset_health(world, actor);

    let mut notes = vec!["revived".to_owned()];
    notes.extend(stand_up(world, actor));
    notes.join("; ")
}

/// Put an actor whose ragdoll is down back on its feet, in place: the simulated ragdoll is freed, the bones'
/// keyframed collision bodies are put back, its generic idle drives the skeleton again, and its AI package
/// stack is re-armed. What revive and the end of a knockdown share; the notes say what was done.
pub(crate) fn stand_up(world: &World, actor: EntityId) -> Vec<String> {
    // The ragdoll lives on the skeleton root, as does the template it was built from.
    let root = world.get::<AnimationTarget>(actor).map_or(actor, |target| target.skeleton_root);
    let mut notes = Vec::new();
    if let Some(ragdoll) = world.get::<Ragdoll>(root).map(|ragdoll| ragdoll.clone()) {
        world.resource_mut::<PhysicsWorld>().remove_ragdoll(&ragdoll);
        notes.push(format!("ragdoll freed ({} bodies)", ragdoll.bodies.len()));
    }
    if let Some(mut ragdolls) = world.query_mut::<Ragdoll>() {
        ragdolls.remove(root);
    }
    if let Some(mut active) = world.query_mut::<RagdollActive>() {
        active.remove(root);
    }
    // The keyframed collision body each ragdoll bone had while the actor lived (activation removed it).
    let bones: Vec<EntityId> = world.get::<RagdollTemplate>(root).map(|template| template.bodies.iter().map(|body| body.bone).collect()).unwrap_or_default();
    if let Some(mut bodies) = world.query_mut::<RigidBodyData>() {
        for bone in &bones {
            if bodies.get(*bone).is_none() {
                bodies.insert(*bone, RigidBodyData { motion_type: MotionType::Keyframed, ..Default::default() });
            }
        }
    }
    notes.push(format!("{} bone bodies restored", bones.len()));

    // The generic idle, on the actor, driving its skeleton.
    let idle = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|index| index.0.game).and_then(|game| {
        crate::npc_spawn::humanoid_default_idle_kf_path(game).and_then(|path| {
            world.try_resource::<byroredux_core::animation::AnimationClipRegistry>().and_then(|clips| clips.get_by_path(path))
        })
    });
    if let (Some(handle), Some(mut players)) = (idle, world.query_mut::<byroredux_core::animation::AnimationPlayer>()) {
        players.insert(actor, byroredux_core::animation::AnimationPlayer::new(handle).with_root(root));
        notes.push("idle restored".to_owned());
    }

    // The package stack, re-armed: the ambient AI installs the winner on its next pass.
    if rearm_ai(world, actor) {
        notes.push("AI re-armed".to_owned());
    }
    notes
}

/// Give `actor` back its [`AmbientPackageRuntime`] (death removes it): its resolved PKID stack, no winner
/// yet, a first-tick confirmation due.
fn rearm_ai(world: &World, actor: EntityId) -> bool {
    let Some(index) = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|index| index.0.clone()) else {
        return false;
    };
    let Some(base) = world.get::<byroredux_scripting::SceneAliasCandidate>(actor).map(|candidate| candidate.base_form_id) else {
        return false;
    };
    let Some(npc) = index.npcs.get(&base) else { return false };
    let resolved = byroredux_plugin::equip::ResolvedNpc::resolve(npc, &index);
    let Some(mut runtimes) = world.query_mut::<AmbientPackageRuntime>() else { return false };
    runtimes.insert(
        actor,
        AmbientPackageRuntime {
            package_candidates: resolved.ai_packages.ai_packages.clone(),
            active_package_form_id: None,
            actor_form_id: resolved.shell.form_id,
            last_evaluated_game_minute: None,
        },
    );
    true
}
