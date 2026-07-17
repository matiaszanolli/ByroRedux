//! Follow procedure (M42.5) — the third AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`/`travel_system`'s straight-line
//! walk primitive via `super::locomotion::step_toward`. **Registered only
//! when `BYRO_FOLLOW` is set** (see `boot.rs`), mirroring
//! `BYRO_TRAVEL`/`BYRO_WANDER`/`BYRO_SANDBOX_SIT`.
//!
//! Unlike Travel (resolve/pick a destination once, walk there, stop for
//! good), Follow tracks a **live** target every tick: once
//! `FollowState::target_entity` is resolved (lazily, on first sight,
//! exactly like Travel resolves its destination), `follow_system` re-reads
//! that entity's `GlobalTransform` fresh each tick — a moving target is
//! actually followed, not just walked toward where it once was.
//!
//! ## Destination resolution
//!
//! - If [`FollowBehavior::target_form_id`] is `Some` (only ever populated
//!   for `SpecificReference`/`ObjectId`-type PTDT targets — see
//!   `crates/core`'s `follow.rs` module docs for why `Other` isn't
//!   attempted), `byroredux_scripting::condition::resolve_entity_by_global_form_id`
//!   resolves it to a live [`EntityId`] once, on this system's first tick
//!   for that actor.
//! - On any miss (unresolved, or no target FormID at all): the actor
//!   simply never moves. Unlike Travel/Wander there's no
//!   hash-picked-point fallback — a Follow package with no resolvable
//!   target has nothing meaningful to do, and silently substituting some
//!   other movement would be an undocumented behavior swap.
//! - Resolution is **not retried** on later ticks if it fails once (v0 —
//!   same "resolve once" discipline `TravelState` uses), and once
//!   resolved, `target_entity` itself never changes — only its *position*
//!   is re-read every tick, which is the whole point of this procedure.
//!
//! ## v0 scope (mirrors `wander.rs`/`travel.rs`'s documented approximations)
//!
//! - **No pathing.** Straight-line walk; no obstacle avoidance.
//! - **No animation.** `AnimationPlayer` is untouched.
//! - **No per-frame package re-evaluation.** Same limitation as
//!   Sandbox/Wander/Travel.
//! - **Target-entity resolution happens once.** If the target isn't
//!   spawned yet on this actor's first tick, or later despawns, Follow
//!   does not re-resolve — the actor stands still from that point on.
//! - **No distance/relationship reasoning beyond stand-off.** Real Follow
//!   packages also gate on combat state, dialogue, etc. — none of that
//!   exists here; this is straight-line stand-off tracking only.

use super::locomotion::{step_toward, LOCOMOTION_ARRIVAL_EPSILON};
use byroredux_core::ecs::components::{FollowBehavior, FollowState, GlobalTransform, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;

/// Fallback stand-off distance (world units), used when
/// `FollowBehavior.follow_distance` is `None` (no PTDT / distance 0).
/// Engine default — no authored equivalent enforced beyond PTDT's own
/// `count_or_distance`, same convention as `WANDER_DEFAULT_RADIUS`.
const FOLLOW_DEFAULT_DISTANCE: f32 = 128.0;

/// Resolve this actor's follow target once, on first sight. `None` when
/// there's no `target_form_id` or resolution fails — both are terminal
/// for this actor (no retry), per this module's docs.
fn resolve_follow_target(world: &World, behavior: &FollowBehavior) -> Option<EntityId> {
    resolve_entity_by_global_form_id(world, behavior.target_form_id?)
}

/// One actor's computed movement update for this tick, applied in Pass 2
/// after all Pass-1 reads have dropped (mirrors `wander_system`'s/
/// `travel_system`'s two-pass read-then-write structure).
struct FollowDecision {
    entity: EntityId,
    /// `None` when the actor didn't move this tick (already within
    /// stand-off distance, or has no resolvable target).
    movement: Option<(Vec3, Option<Quat>)>,
    /// Written on every tick this actor lacks `FollowState` yet (first
    /// sight) — resolved once and then never changed again.
    state_to_insert: Option<FollowState>,
}

/// Reusable per-frame scratch for [`follow_system_inner`] — captured by
/// [`make_follow_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct FollowScratch {
    decisions: Vec<FollowDecision>,
}

/// Drive straight-line stand-off tracking for every [`FollowBehavior`]
/// actor. Registered `add_exclusive(Stage::PostUpdate, …)`, same
/// stage/slot family as `wander_system`/`travel_system` — NPC placement
/// roots are propagation roots (no `Parent`), so `Transform` == world
/// position for them; the *target*'s live position is read via
/// `GlobalTransform` since it may be any REFR, not necessarily a
/// propagation root.
fn follow_system_inner(world: &World, dt: f32, scratch: &mut FollowScratch) {
    let Some(behavior_q) = world.query::<FollowBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    scratch.decisions.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<FollowState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let existing_state = state_q.as_ref().and_then(|q| q.get(entity)).copied();
            let (target_entity, state_to_insert) = match existing_state {
                Some(s) => (s.target_entity, None),
                None => {
                    let resolved = resolve_follow_target(world, behavior);
                    (resolved, Some(FollowState { target_entity: resolved }))
                }
            };

            let Some(target_entity) = target_entity else {
                scratch.decisions.push(FollowDecision {
                    entity,
                    movement: None,
                    state_to_insert,
                });
                continue;
            };
            let Some(target_gt) = world.get::<GlobalTransform>(target_entity) else {
                // Target despawned since resolution — stand still this
                // tick rather than re-resolving (v0 discipline).
                scratch.decisions.push(FollowDecision {
                    entity,
                    movement: None,
                    state_to_insert,
                });
                continue;
            };

            let current = transform.translation;
            let distance = behavior.follow_distance.unwrap_or(FOLLOW_DEFAULT_DISTANCE);
            let target_xz = Vec3::new(target_gt.translation.x, current.y, target_gt.translation.z);
            let horiz_delta = Vec3::new(target_xz.x - current.x, 0.0, target_xz.z - current.z);

            let movement = if horiz_delta.length() > distance + LOCOMOTION_ARRIVAL_EPSILON {
                Some(step_toward(current, transform.rotation, target_xz, dt, physics.as_deref()))
            } else {
                None
            };

            scratch.decisions.push(FollowDecision {
                entity,
                movement,
                state_to_insert,
            });
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 2: apply writes (each a scoped single-type lock). ──
    if let Some(mut tq) = world.query_mut::<Transform>() {
        for d in &scratch.decisions {
            if let (Some((new_pos, rotation)), Some(t)) = (d.movement, tq.get_mut(d.entity)) {
                t.translation = new_pos;
                if let Some(r) = rotation {
                    t.rotation = r;
                }
            }
        }
    }
    if let Some(mut sq) = world.query_mut::<FollowState>() {
        for d in &scratch.decisions {
            if let Some(state) = d.state_to_insert {
                sq.insert(d.entity, state);
            }
        }
    }
}

/// Follow system factory — returns a closure with a persistent
/// [`FollowScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`follow_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_follow_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = FollowScratch::default();
    move |world: &World, dt: f32| {
        follow_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_follow_system`].
#[cfg(test)]
pub(crate) fn follow_system(world: &World, dt: f32) {
    follow_system_inner(world, dt, &mut FollowScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn follow_system_stands_still_with_no_target() {
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            FollowBehavior {
                target_form_id: None,
                follow_distance: None,
            },
        );

        follow_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert_eq!(tq.get(entity).unwrap().translation, Vec3::ZERO, "no target — actor must not move");

        let sq = world.query::<FollowState>().expect("FollowState registered");
        assert_eq!(
            sq.get(entity).copied(),
            Some(FollowState { target_entity: None }),
            "must persist a resolved-to-None state on first tick, not retry every frame"
        );
    }

    /// Spawns a target entity with a resolvable global FormID (inserting
    /// a fresh `FormIdPool` — call at most once per test `World`).
    fn spawn_target(world: &mut World, form_id_raw: u32, pos: Vec3) -> EntityId {
        let mut pool = FormIdPool::new();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(form_id_raw),
        });
        world.insert_resource(pool);

        let target = world.spawn();
        world.insert(target, FormIdComponent(fid));
        world.insert(target, GlobalTransform::new(pos, Quat::IDENTITY, 1.0));
        target
    }

    #[test]
    fn follow_system_resolves_target_and_closes_distance() {
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();

        spawn_target(&mut world, 0x0001_59E2, Vec3::new(1000.0, 10.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x0001_59E2),
                follow_distance: Some(50.0),
            },
        );

        follow_system(&world, 0.1);

        let sq = world.query::<FollowState>().expect("FollowState registered");
        assert!(
            sq.get(actor).unwrap().target_entity.is_some(),
            "target must resolve on first tick"
        );

        let tq = world.query::<Transform>().expect("Transform registered");
        let pos = tq.get(actor).unwrap().translation;
        assert!(pos.x > 0.0, "actor should have started closing toward the target");
    }

    #[test]
    fn follow_system_tracks_a_moving_target_live() {
        // The behavior that actually distinguishes Follow from Travel:
        // move the target between ticks and confirm the actor's direction
        // changes to chase the new position, rather than a frozen one.
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();

        let target = spawn_target(&mut world, 0x0002_AAAA, Vec3::new(1000.0, 0.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x0002_AAAA),
                follow_distance: Some(10.0),
            },
        );

        follow_system(&world, 0.1);
        let pos_after_first = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(actor).unwrap().translation
        };
        assert!(pos_after_first.x > 0.0 && pos_after_first.z.abs() < 1e-3);

        // Move the target far in Z instead — a frozen destination
        // (Travel-style) would keep chasing +X; Follow must re-read the
        // live position and turn toward +Z.
        if let Some(mut gq) = world.query_mut::<GlobalTransform>() {
            if let Some(gt) = gq.get_mut(target) {
                gt.translation = Vec3::new(pos_after_first.x, 0.0, 1000.0);
            }
        }

        follow_system(&world, 0.1);
        let pos_after_second = {
            let tq = world.query::<Transform>().expect("Transform registered");
            tq.get(actor).unwrap().translation
        };
        assert!(
            pos_after_second.z > pos_after_first.z + 1e-3,
            "actor must chase the target's NEW live position, not a frozen one"
        );
    }
}
