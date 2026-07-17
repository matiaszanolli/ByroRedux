//! Guard procedure (M42.7) — the fifth AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`/`travel_system`/`follow_system`/
//! `escort_system`'s straight-line walk primitive via
//! `super::locomotion::step_toward`. **Registered only when `BYRO_GUARD`
//! is set** (see `boot.rs`), mirroring
//! `BYRO_ESCORT`/`BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`.
//!
//! Guard needs only `PLDT` — no new sub-record decode work. Anchor
//! resolution tries a `NearReference`-type FormID first (via
//! `resolve_entity_by_global_form_id`, same as Travel/Escort), but its
//! **fallback differs from Travel's on purpose**: Travel picks a random
//! point within radius because its whole job is "go explore somewhere";
//! Guard's job is "hold a post", so a package with no resolvable anchor
//! falls back to the actor's own spawn position — the same "center is the
//! actor's own position" convention `SandboxBehavior`/`WanderBehavior` use
//! for their search/wander center, not Travel's "wander to a random point"
//! one. (Reusing Travel's random-pick fallback here was tried first and
//! reverted — it hands the actor a fresh point exactly `radius` away from
//! its own spawn, which trivially satisfies the leash check below and
//! results in an actor that never walks anywhere, having "arrived" by
//! construction.)
//!
//! What makes Guard different from Travel isn't just the fallback,
//! though — it's what happens after arrival: Travel reaches a terminal
//! state and stops forever ([`super::travel::Traveled`]); Guard never
//! does. Once the anchor is resolved, `guard_system` checks every tick
//! whether the actor has drifted more than [`GuardBehavior::radius`]
//! away, and walks back if so — an indefinite, non-terminal shape closer
//! to `wander_system`'s (just triggered by displacement instead of a
//! pause timer).
//!
//! ## No known displacement trigger yet
//!
//! Nothing in the engine currently pushes an NPC actor away from its
//! Transform once placed — there's no combat, no player shove, no
//! ragdoll-to-Transform writeback for guarding actors. So in practice, v0
//! Guard behaves observably like a non-terminal Travel: walk to the
//! anchor once, then hold. The per-tick leash check is real and tested,
//! not a stub — it's just dormant until a future system can actually
//! displace a guarding actor.
//!
//! ## v0 scope (mirrors `wander.rs`/`travel.rs`'s documented approximations)
//!
//! - **No pathing.** Straight-line walk; no obstacle avoidance.
//! - **No animation.** `AnimationPlayer` is untouched.
//! - **No per-frame package re-evaluation.** Same limitation as
//!   Sandbox/Wander/Travel/Follow/Escort.
//! - **Anchor is frozen once resolved/picked**, exactly like
//!   `TravelState::destination` — a `NearReference` anchor that moves
//!   after resolution isn't re-tracked (unlike Follow's live target).
//! - **Ground-snapped, not physically simulated**, same as Wander/Travel.

use super::locomotion::step_toward;
use byroredux_core::ecs::components::{GlobalTransform, GuardBehavior, GuardState, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;

/// Fallback leash tolerance (world units) around a guard's anchor, used
/// when `GuardBehavior.radius` is `None` (no PLDT / radius 0). Same scale
/// as `travel.rs::TRAVEL_DEFAULT_RADIUS`/`wander.rs::WANDER_DEFAULT_RADIUS`.
const GUARD_DEFAULT_RADIUS: f32 = 512.0;

/// Resolve this actor's guard anchor once, on first sight: a
/// `NearReference`-type PLDT FormID first (mirrors
/// `travel_system`/`escort_system`'s `NearReference` resolution exactly),
/// falling back to `home` — the actor's own spawn position — on any miss.
/// See this module's doc for why the fallback is `home` rather than
/// Travel's random-pick-within-radius.
fn resolve_anchor(world: &World, behavior: &GuardBehavior, home: Vec3) -> Vec3 {
    if let Some(fid) = behavior.anchor_form_id {
        if let Some(target_entity) = resolve_entity_by_global_form_id(world, fid) {
            if let Some(gt) = world.get::<GlobalTransform>(target_entity) {
                return gt.translation;
            }
        }
    }
    home
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors
/// `travel_system`/`follow_system`'s two-pass structure).
struct GuardDecision {
    entity: EntityId,
    translation: Vec3,
    rotation: Option<Quat>,
    /// `Some` only when `GuardState` needs to change this tick (first
    /// sight — the anchor doesn't change afterward, so later ticks never
    /// rewrite it).
    state: Option<GuardState>,
}

/// Drive anchor-hold-and-return locomotion for every [`GuardBehavior`]
/// actor. Registered `add_exclusive(Stage::PostUpdate, …)`, same
/// stage/slot family as `wander_system`/`travel_system` — NPC placement
/// roots are propagation roots (no `Parent`), so `Transform` == world
/// position for them.
pub fn guard_system(world: &World, dt: f32) {
    let Some(behavior_q) = world.query::<GuardBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    let mut decisions: Vec<GuardDecision> = Vec::new();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<GuardState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let current = transform.translation;
            let existing_state = state_q.as_ref().and_then(|q| q.get(entity)).copied();

            let (anchor, state) = match existing_state {
                Some(s) => (s.anchor, None),
                None => {
                    let anchor = resolve_anchor(world, behavior, current);
                    (anchor, Some(GuardState { anchor }))
                }
            };

            let leash = behavior.radius.unwrap_or(GUARD_DEFAULT_RADIUS);
            let horiz_delta = Vec3::new(anchor.x - current.x, 0.0, anchor.z - current.z);
            let movement = if horiz_delta.length() > leash {
                let target_xz = Vec3::new(anchor.x, current.y, anchor.z);
                Some(step_toward(current, transform.rotation, target_xz, dt, physics.as_deref()))
            } else {
                None
            };

            decisions.push(GuardDecision {
                entity,
                translation: movement.map_or(current, |(pos, _)| pos),
                rotation: movement.and_then(|(_, rot)| rot),
                state,
            });
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
    if let Some(mut sq) = world.query_mut::<GuardState>() {
        for d in &decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn guard_system_holds_home_position_with_no_target_and_no_physics() {
        // No `anchor_form_id` to resolve — v0 falls back to the actor's
        // own spawn position (not a random pick, see this module's doc),
        // so there's nowhere to walk and the actor must not move.
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            GuardBehavior { anchor_form_id: None, radius: Some(200.0), form_id: 0x000A_0001 },
        );

        guard_system(&world, 0.5);

        let sq = world.query::<GuardState>().expect("GuardState registered");
        let state = sq.get(entity).expect("guard_system must lazily insert GuardState on first tick");
        assert_eq!(state.anchor, Vec3::ZERO, "fallback anchor must be the actor's own spawn position");

        let tq = world.query::<Transform>().expect("Transform registered");
        assert_eq!(
            tq.get(entity).unwrap().translation,
            Vec3::ZERO,
            "already at the fallback anchor — must not move"
        );
    }

    #[test]
    fn guard_system_resolves_anchor_form_id_to_live_entity_position() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();

        let mut pool = FormIdPool::new();
        let anchor_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x000B_0001),
        });
        world.insert_resource(pool);

        let anchor = world.spawn();
        world.insert(anchor, FormIdComponent(anchor_fid));
        world.insert(anchor, GlobalTransform::new(Vec3::new(300.0, 10.0, 400.0), Quat::IDENTITY, 1.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            GuardBehavior { anchor_form_id: Some(0x000B_0001), radius: None, form_id: 0x000B_0002 },
        );

        guard_system(&world, 0.001);

        let sq = world.query::<GuardState>().expect("GuardState registered");
        assert_eq!(
            sq.get(actor).unwrap().anchor,
            Vec3::new(300.0, 10.0, 400.0),
            "anchor must be the resolved target's live position, not a hash-picked fallback"
        );
    }

    #[test]
    fn guard_system_holds_position_once_within_leash_and_returns_when_displaced() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();

        let entity = world.spawn();
        // Seed the actor already at its anchor, well inside the leash.
        world.insert(entity, Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)));
        world.insert(
            entity,
            GuardBehavior { anchor_form_id: None, radius: Some(20.0), form_id: 0x000C_0001 },
        );
        world.insert(entity, GuardState { anchor: Vec3::new(50.0, 0.0, 50.0) });

        guard_system(&world, 0.5);
        {
            let tq = world.query::<Transform>().expect("Transform registered");
            assert_eq!(
                tq.get(entity).unwrap().translation,
                Vec3::new(50.0, 0.0, 50.0),
                "within the leash — must hold position, not wander off"
            );
        }

        // Displace the actor beyond the leash (simulating a future
        // shove/knockback system) and confirm the next tick walks back.
        if let Some(mut tq) = world.query_mut::<Transform>() {
            tq.get_mut(entity).unwrap().translation = Vec3::new(500.0, 0.0, 50.0);
        }
        guard_system(&world, 0.5);
        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.x < 500.0,
            "beyond the leash — must step back toward the anchor"
        );
    }
}
