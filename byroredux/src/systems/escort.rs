//! Escort procedure (M42.6) — the fourth AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`/`travel_system`/`follow_system`'s
//! straight-line walk primitive via `super::locomotion::step_toward`.
//! **Registered only when `BYRO_ESCORT` is set** (see `boot.rs`), mirroring
//! `BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`/`BYRO_SANDBOX_SIT`.
//!
//! Escort is the first M42 procedure that needs no new sub-record decode
//! work — it combines two pieces already parsed for prior procedures:
//! `PTDT` (who to collect, from Follow, M42.5) and `PLDT` (where to lead
//! them, from Travel, M42.4). The runtime is two phases sharing the same
//! locomotion primitive:
//!
//! 1. **Collect** — close to the PTDT target's *live* position (re-read
//!    every tick, exactly like `follow_system`) until within
//!    [`ESCORT_COLLECT_DISTANCE`].
//! 2. **Lead** — resolve the PLDT destination once (exactly like
//!    `travel_system`'s `resolve_destination`, same `NearReference`-or-
//!    hash-pick fallback) and walk straight there, tagging [`Escorted`]
//!    on arrival — a terminal marker mirroring [`super::travel::Traveled`].
//!
//! ## No target to collect
//!
//! Unlike Follow (which simply never moves without a resolvable target —
//! see `follow.rs`'s module docs for why), Escort skips straight to the
//! lead phase when there's nothing to collect: no `target_form_id`, or
//! resolution fails, or the resolved entity has since despawned. This is a
//! deliberate difference: Escort's whole point is reaching a destination,
//! and (per `ai.rs`'s own PLDT doc) most FO3/FNV packages carry a PLDT
//! regardless of PTDT, so falling back to "just go to the destination" is
//! more useful than standing still forever.
//!
//! ## v0 scope (mirrors `wander.rs`/`travel.rs`/`follow.rs`'s documented
//! approximations)
//!
//! - **No pathing.** Straight-line walk in both phases; no obstacle
//!   avoidance.
//! - **No animation.** `AnimationPlayer` is untouched.
//! - **No per-frame package re-evaluation.** Same limitation as
//!   Sandbox/Wander/Travel/Follow.
//! - **Target-entity resolution happens once**, on this system's first
//!   tick for the actor — mirrors `FollowState`'s discipline. If
//!   resolution fails, Escort does not retry; it just treats this tick
//!   as "nothing to collect" (see above).
//! - **Destination is frozen once resolved**, exactly like
//!   `TravelState::destination` — if the resolved/picked destination
//!   later becomes stale (e.g. a `NearReference` target moves), Escort
//!   does not re-track it, unlike the collect phase's live tracking.
//! - **No `PTD2`.** Two-target Escort variants aren't decoded (see
//!   `ai.rs`'s `PackRecord::target` doc) — v0 only ever escorts one actor.
//! - **One settle tick on the collect→lead transition.** The tick an
//!   actor is deemed "collected" is the same tick the destination is
//!   resolved and the first lead-phase step is taken (mirrors
//!   `travel_system` moving on the very tick it resolves) — there's no
//!   extra idle tick in between.

use super::locomotion::{step_toward, LOCOMOTION_ARRIVAL_EPSILON};
use super::wander::pick_wander_target;
use byroredux_core::ecs::components::{
    EscortBehavior, EscortState, Escorted, GlobalTransform, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;

/// Distance (world units) within which the target is considered
/// "collected" — the collect phase ends and the lead phase begins. Engine
/// default, same scale as `follow.rs::FOLLOW_DEFAULT_DISTANCE`.
const ESCORT_COLLECT_DISTANCE: f32 = 128.0;

/// Fallback pick radius (world units) around an actor's current position,
/// used when `EscortBehavior.destination_radius` is `None` (no PLDT /
/// radius 0) *and* `destination_form_id` doesn't resolve. Same scale as
/// `travel.rs::TRAVEL_DEFAULT_RADIUS`.
const ESCORT_DEFAULT_RADIUS: f32 = 512.0;

/// Resolve this actor's Escort target once, on first sight. Mirrors
/// `follow.rs::resolve_follow_target`.
fn resolve_escort_target(world: &World, behavior: &EscortBehavior) -> Option<EntityId> {
    resolve_entity_by_global_form_id(world, behavior.target_form_id?)
}

/// Resolve (or pick) the lead-phase destination. Mirrors
/// `travel.rs::resolve_destination` exactly — `NearReference`-type PLDT
/// FormID first, hash-picked point within radius as the fallback.
fn resolve_destination(world: &World, behavior: &EscortBehavior, home: Vec3) -> Vec3 {
    if let Some(fid) = behavior.destination_form_id {
        if let Some(target_entity) = resolve_entity_by_global_form_id(world, fid) {
            if let Some(gt) = world.get::<GlobalTransform>(target_entity) {
                return gt.translation;
            }
        }
    }
    let radius = behavior.destination_radius.unwrap_or(ESCORT_DEFAULT_RADIUS);
    pick_wander_target(home, radius, behavior.form_id, 0)
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors
/// `wander_system`/`travel_system`/`follow_system`'s two-pass structure).
struct EscortDecision {
    entity: EntityId,
    translation: Vec3,
    rotation: Option<Quat>,
    /// `Some` when `EscortState` needs to change this tick (first sight,
    /// or the collect→lead transition); `None` when the existing state
    /// already matches (e.g. mid-collect with a previously-resolved
    /// target, or mid-lead with a frozen destination).
    state: Option<EscortState>,
    /// True when this tick's lead-phase move reached the destination.
    arrived: bool,
}

/// Drive the two-phase collect-then-lead locomotion for every
/// [`EscortBehavior`] actor not yet [`Escorted`]. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`/`travel_system`/`follow_system` — NPC placement roots
/// are propagation roots (no `Parent`), so `Transform` == world position
/// for them; both the collect target and the resolved destination
/// reference may be any REFR, so their positions are read via
/// `GlobalTransform`.
pub fn escort_system(world: &World, dt: f32) {
    let Some(behavior_q) = world.query::<EscortBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    let mut decisions: Vec<EscortDecision> = Vec::new();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let escorted_q = world.query::<Escorted>();
        let state_q = world.query::<EscortState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            if escorted_q.as_ref().is_some_and(|q| q.contains(entity)) {
                continue; // already arrived (one-shot guard)
            }
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let current = transform.translation;
            let existing_state = state_q.as_ref().and_then(|q| q.get(entity)).copied();

            let target_entity = match existing_state {
                Some(s) => s.target_entity,
                None => resolve_escort_target(world, behavior),
            };

            // ── Already leading: walk toward the frozen destination. ──
            if let Some(destination) = existing_state.and_then(|s| s.destination) {
                let target_xz = Vec3::new(destination.x, current.y, destination.z);
                let (new_pos, rotation) =
                    step_toward(current, transform.rotation, target_xz, dt, physics.as_deref());
                let horiz_delta = Vec3::new(new_pos.x - destination.x, 0.0, new_pos.z - destination.z);
                let arrived =
                    horiz_delta.length_squared() <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON;
                decisions.push(EscortDecision {
                    entity,
                    translation: new_pos,
                    rotation,
                    state: None,
                    arrived,
                });
                continue;
            }

            // ── Not leading yet: check whether the target is collected. ──
            let live_target_pos = target_entity
                .and_then(|te| world.get::<GlobalTransform>(te))
                .map(|gt| gt.translation);
            let collected = match live_target_pos {
                None => true, // nothing to collect — skip straight to leading
                Some(pos) => {
                    let d = Vec3::new(pos.x - current.x, 0.0, pos.z - current.z);
                    d.length() <= ESCORT_COLLECT_DISTANCE + LOCOMOTION_ARRIVAL_EPSILON
                }
            };

            if collected {
                // Transition into the lead phase this tick — resolve the
                // destination and start walking immediately, mirroring
                // travel_system moving on the very tick it resolves.
                let destination = resolve_destination(world, behavior, current);
                let target_xz = Vec3::new(destination.x, current.y, destination.z);
                let (new_pos, rotation) =
                    step_toward(current, transform.rotation, target_xz, dt, physics.as_deref());
                let horiz_delta = Vec3::new(new_pos.x - destination.x, 0.0, new_pos.z - destination.z);
                let arrived =
                    horiz_delta.length_squared() <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON;
                decisions.push(EscortDecision {
                    entity,
                    translation: new_pos,
                    rotation,
                    state: Some(EscortState { target_entity, destination: Some(destination) }),
                    arrived,
                });
            } else {
                // Still collecting: walk toward the target's live position.
                let pos = live_target_pos.expect("collected == false implies Some");
                let target_xz = Vec3::new(pos.x, current.y, pos.z);
                let (new_pos, rotation) =
                    step_toward(current, transform.rotation, target_xz, dt, physics.as_deref());
                decisions.push(EscortDecision {
                    entity,
                    translation: new_pos,
                    rotation,
                    state: existing_state
                        .is_none()
                        .then_some(EscortState { target_entity, destination: None }),
                    arrived: false,
                });
            }
        }
    }
    if decisions.is_empty() {
        return;
    }

    // ── Pass 2: apply writes (each a scoped single-type lock). ──
    if let Some(mut tq) = world.query_mut::<Transform>() {
        for d in &decisions {
            if let Some(t) = tq.get_mut(d.entity) {
                t.translation = d.translation;
                if let Some(r) = d.rotation {
                    t.rotation = r;
                }
            }
        }
    }
    if let Some(mut sq) = world.query_mut::<EscortState>() {
        for d in &decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
    if let Some(mut eq) = world.query_mut::<Escorted>() {
        for d in &decisions {
            if d.arrived {
                eq.insert(d.entity, Escorted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn spawn_entity_at(world: &mut World, form_id_raw: u32, pos: Vec3) -> EntityId {
        let mut pool = world
            .remove_resource::<FormIdPool>()
            .unwrap_or_else(FormIdPool::new);
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(form_id_raw),
        });
        world.insert_resource(pool);

        let entity = world.spawn();
        world.insert(entity, FormIdComponent(fid));
        world.insert(entity, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
        entity
    }

    fn register_all(world: &mut World) {
        world.register::<EscortBehavior>();
        world.register::<EscortState>();
        world.register::<Escorted>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
    }

    #[test]
    fn escort_system_skips_to_lead_when_no_target() {
        let mut world = World::new();
        register_all(&mut world);

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            EscortBehavior {
                target_form_id: None,
                destination_form_id: None,
                destination_radius: Some(200.0),
                form_id: 0x0006_0006,
            },
        );

        escort_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.length() > 0.0,
            "no target to collect — actor should skip straight to the lead phase and move"
        );

        let sq = world.query::<EscortState>().expect("EscortState registered");
        let state = sq.get(entity).expect("state written on first tick");
        assert_eq!(state.target_entity, None);
        assert!(state.destination.is_some(), "must resolve a lead-phase destination immediately");
    }

    #[test]
    fn escort_system_walks_toward_far_target_before_leading() {
        let mut world = World::new();
        register_all(&mut world);

        let target = spawn_entity_at(&mut world, 0x0007_0001, Vec3::new(1000.0, 0.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            EscortBehavior {
                target_form_id: Some(0x0007_0001),
                destination_form_id: None,
                destination_radius: Some(200.0),
                form_id: 0x0007_0002,
            },
        );

        escort_system(&world, 0.1);

        let sq = world.query::<EscortState>().expect("EscortState registered");
        let state = sq.get(actor).expect("state written on first tick");
        assert_eq!(state.target_entity, Some(target));
        assert!(state.destination.is_none(), "still collecting — must not have a destination yet");

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(actor).unwrap().translation.x > 0.0,
            "actor should be closing toward the far-away target"
        );
    }

    #[test]
    fn escort_system_transitions_to_lead_once_target_is_within_collect_range() {
        let mut world = World::new();
        register_all(&mut world);

        spawn_entity_at(&mut world, 0x0008_0001, Vec3::new(5.0, 0.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            EscortBehavior {
                target_form_id: Some(0x0008_0001),
                destination_form_id: None,
                destination_radius: Some(200.0),
                form_id: 0x0008_0002,
            },
        );

        escort_system(&world, 0.1);

        let sq = world.query::<EscortState>().expect("EscortState registered");
        let state = sq.get(actor).expect("state written on first tick");
        assert!(
            state.destination.is_some(),
            "target already within collect range — must begin leading on the very first tick"
        );
    }

    #[test]
    fn escort_system_tags_escorted_on_arrival_and_then_stops() {
        let mut world = World::new();
        register_all(&mut world);

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)));
        world.insert(
            entity,
            EscortBehavior {
                target_form_id: None,
                destination_form_id: None,
                destination_radius: Some(100.0),
                form_id: 0x0009_0009,
            },
        );
        // Seed the lead phase directly at the actor's current position so
        // arrival is immediate — mirrors travel_system's arrival test.
        world.insert(
            entity,
            EscortState { target_entity: None, destination: Some(Vec3::new(50.0, 0.0, 50.0)) },
        );

        escort_system(&world, 0.5);

        {
            let eq = world.query::<Escorted>().expect("Escorted registered");
            assert!(eq.get(entity).is_some(), "actor at its destination must be tagged Escorted");
        }

        let pos_after_first = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(entity).expect("actor transform").translation
        };

        escort_system(&world, 0.5); // second tick: one-shot guard must skip it

        let pos_after_second = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(entity).expect("actor transform").translation
        };
        assert_eq!(pos_after_first, pos_after_second, "Escorted actors must not move on later ticks");
    }
}
