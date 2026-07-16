//! Travel procedure (M42.4) — the second AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`'s straight-line walk primitive via
//! `super::locomotion::step_toward`. **Registered only when `BYRO_TRAVEL`
//! is set** (see `boot.rs`), mirroring `BYRO_WANDER`/`BYRO_SANDBOX_SIT`.
//!
//! Unlike Wander (walk to a random point, pause, repeat forever), Travel
//! walks **once** to a destination and stops — [`Traveled`] is a terminal
//! marker (mirrors `Seated`'s one-shot role in `sandbox_seat_system`), not
//! an oscillating state.
//!
//! ## Destination resolution
//!
//! Travel's whole point is arriving somewhere specific, so — unlike
//! Wander/Sandbox, where "actor's own position" is a legitimate
//! approximation for a *search center* — this system first tries to
//! resolve the package's authored PLDT target to a real live entity's
//! position:
//!
//! - If [`TravelBehavior::target_form_id`] is `Some` (only ever populated
//!   for a `NearReference`-type PLDT — see `crates/core`'s `travel.rs`
//!   module docs for why other location types aren't attempted),
//!   `byroredux_scripting::condition::resolve_entity_by_global_form_id`
//!   resolves it to a live [`EntityId`], and that entity's
//!   [`GlobalTransform`] gives the destination.
//! - On any miss (unresolved target, or no target FormID at all): fall
//!   back to `super::wander::pick_wander_target` (reused directly, not
//!   duplicated) — a hash-picked point within `radius` of the actor's own
//!   spawn position, the same v0 approximation Wander uses.
//!
//! This resolution happens once, lazily, on `travel_system`'s first tick
//! per actor — i.e. *after* the whole cell has finished loading, which
//! sidesteps the same-pass spawn-ordering concern Sandbox's 2026-07-14
//! `NearReference` investigation raised (a target later in the same
//! REFR list not being live yet doesn't apply here — by the time any
//! system's first tick runs, the whole cell is loaded). It still won't
//! resolve most targets: that investigation found only ~12% of
//! `NearReference` targets resolve to anything spawnable at all (most
//! are off-cell, or the hardcoded XMarker family `cell_loader` never
//! spawns) — this system inherits that same ceiling, just from a
//! strictly better vantage point than a spawn-time attempt would have had.
//!
//! ## v0 scope (mirrors `wander.rs`'s documented approximations)
//!
//! - **No pathing.** Straight-line walk; no obstacle avoidance.
//! - **No animation.** `AnimationPlayer` is untouched — Transform moves,
//!   pose doesn't (no verified walk `.kf` path exists in this codebase).
//! - **No per-frame package re-evaluation.** `TravelBehavior` is attached
//!   once at spawn; the same limitation `SandboxBehavior`/`WanderBehavior`
//!   have today.
//! - **Destination is frozen once resolved/picked.** If the resolved
//!   target entity moves after that first tick, Travel does not follow —
//!   that would be Follow's job (a different, unimplemented procedure).
//! - **Ground-snapped, not physically simulated**, same as Wander.

use super::locomotion::{step_toward, LOCOMOTION_ARRIVAL_EPSILON};
use super::wander::pick_wander_target;
use byroredux_core::ecs::components::{GlobalTransform, Transform, TravelBehavior, Traveled, TravelState};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;

/// Fallback pick radius (world units) around an actor's spawn position,
/// used when `TravelBehavior.radius` is `None` (no PLDT / radius 0) *and*
/// the target didn't resolve. Same scale as `wander.rs::WANDER_DEFAULT_RADIUS`
/// — both are "how far does this ambient behavior reach without an
/// authored radius" defaults for the same class of content.
const TRAVEL_DEFAULT_RADIUS: f32 = 512.0;

/// Resolve (or pick) this actor's destination once, on first sight.
/// Pulled out of `travel_system`'s Pass 1 loop body for readability —
/// still a plain read against `world`, called only when no `TravelState`
/// exists yet for this entity.
fn resolve_destination(world: &World, behavior: &TravelBehavior, home: Vec3) -> Vec3 {
    if let Some(fid) = behavior.target_form_id {
        if let Some(target_entity) = resolve_entity_by_global_form_id(world, fid) {
            if let Some(gt) = world.get::<GlobalTransform>(target_entity) {
                return gt.translation;
            }
        }
    }
    let radius = behavior.radius.unwrap_or(TRAVEL_DEFAULT_RADIUS);
    pick_wander_target(home, radius, behavior.form_id, 0)
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors `wander_system`'s
/// two-pass read-then-write structure).
struct TravelDecision {
    entity: EntityId,
    translation: Vec3,
    rotation: Option<Quat>,
    /// `Some` while still traveling (state to persist); `None` once this
    /// tick's move reaches the destination — Pass 2 tags `Traveled`
    /// instead of writing `TravelState` for those entities.
    state: Option<TravelState>,
}

/// Drive straight-line walk-to-destination-once locomotion for every
/// [`TravelBehavior`] actor not yet [`Traveled`]. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`/`sandbox_seat_system` — NPC placement roots are
/// propagation roots (no `Parent`), so `Transform` == world position for
/// them.
pub fn travel_system(world: &World, dt: f32) {
    let Some(behavior_q) = world.query::<TravelBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    let mut decisions: Vec<TravelDecision> = Vec::new();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let traveled_q = world.query::<Traveled>();
        let state_q = world.query::<TravelState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            if traveled_q.as_ref().is_some_and(|q| q.contains(entity)) {
                continue; // already arrived (one-shot guard)
            }
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let current = transform.translation;
            let state = state_q.as_ref().and_then(|q| q.get(entity)).copied();
            let destination = match state {
                Some(s) => s.destination,
                None => resolve_destination(world, behavior, current),
            };

            let target_xz = Vec3::new(destination.x, current.y, destination.z);
            let (new_pos, rotation) =
                step_toward(current, transform.rotation, target_xz, dt, physics.as_deref());

            let horiz_delta = Vec3::new(new_pos.x - destination.x, 0.0, new_pos.z - destination.z);
            let arrived = horiz_delta.length_squared() <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON;

            decisions.push(TravelDecision {
                entity,
                translation: new_pos,
                rotation,
                state: if arrived {
                    None
                } else {
                    Some(TravelState { destination })
                },
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
    if let Some(mut sq) = world.query_mut::<TravelState>() {
        for d in &decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
    if let Some(mut tq) = world.query_mut::<Traveled>() {
        for d in &decisions {
            if d.state.is_none() {
                tq.insert(d.entity, Traveled);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::ecs::components::FormIdComponent;

    #[test]
    fn travel_system_falls_back_to_hash_pick_with_no_target_and_no_physics() {
        // No target_form_id, no PhysicsWorld — same shape as
        // wander_system's no-physics test. Must move and lazily insert
        // TravelState (not panic).
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(200.0),
                target_form_id: None,
                form_id: 0x0003_0003,
            },
        );

        travel_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        let t = tq.get(entity).expect("actor transform");
        assert!(t.translation.length() > 0.0, "actor should have moved from the origin");

        let sq = world.query::<TravelState>().expect("TravelState registered");
        assert!(sq.get(entity).is_some(), "travel_system must lazily insert TravelState on first tick");

        let travq = world.query::<Traveled>().expect("Traveled registered");
        assert!(travq.get(entity).is_none(), "should not have arrived in one 0.5s tick from a 200-unit radius pick");
    }

    #[test]
    fn travel_system_resolves_target_form_id_to_live_entity_position() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();

        let mut pool = FormIdPool::new();
        let target_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_59E2),
        });
        world.insert_resource(pool);

        let target = world.spawn();
        world.insert(target, FormIdComponent(target_fid));
        world.insert(
            target,
            GlobalTransform::new(Vec3::new(300.0, 10.0, 400.0), Quat::IDENTITY, 1.0),
        );

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            TravelBehavior {
                radius: None,
                target_form_id: Some(0x0001_59E2),
                form_id: 0x0004_0004,
            },
        );

        travel_system(&world, 0.001); // tiny dt: just enough to resolve, not arrive

        let sq = world.query::<TravelState>().expect("TravelState registered");
        let state = sq.get(actor).expect("destination resolved on first tick");
        assert_eq!(
            state.destination,
            Vec3::new(300.0, 10.0, 400.0),
            "destination must be the resolved target's live position, not a hash-picked fallback"
        );
    }

    #[test]
    fn travel_system_tags_traveled_on_arrival_and_then_stops_moving() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();

        let entity = world.spawn();
        // Start within LOCOMOTION_ARRIVAL_EPSILON of the fallback-pick
        // destination isn't guaranteed by construction, so instead seed
        // TravelState directly at a destination equal to the actor's
        // current position — arrival is immediate on tick 1.
        world.insert(entity, Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)));
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(100.0),
                target_form_id: None,
                form_id: 0x0005_0005,
            },
        );
        world.insert(entity, TravelState { destination: Vec3::new(50.0, 0.0, 50.0) });

        travel_system(&world, 0.5);

        {
            let travq = world.query::<Traveled>().expect("Traveled registered");
            assert!(travq.get(entity).is_some(), "actor at its destination must be tagged Traveled");
        }

        let pos_after_first = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(entity).expect("actor transform").translation
        };

        travel_system(&world, 0.5); // second tick: one-shot guard must skip it

        let pos_after_second = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(entity).expect("actor transform").translation
        };
        assert_eq!(pos_after_first, pos_after_second, "Traveled actors must not move on later ticks");
    }
}
