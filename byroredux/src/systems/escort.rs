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
//! - **Single-tile pathing in both phases** (EX-16 item 3 Phase 4,
//!   `docs/engine/navmesh-pathfinding.md`) — the lead phase reuses
//!   `travel_system`'s frozen-goal `crate::components::NavPath` caching
//!   exactly (`0.0` repath threshold); the collect phase reuses
//!   `follow_system`'s live-goal caching exactly
//!   ([`ESCORT_COLLECT_REPATH_THRESHOLD`], a separate constant at the
//!   same scale as `follow.rs::FOLLOW_REPATH_THRESHOLD` rather than
//!   importing it — same "own tuning constant per procedure,
//!   cross-referenced not shared" convention `ESCORT_COLLECT_DISTANCE`
//!   already follows for `FOLLOW_DEFAULT_DISTANCE`). One `NavPath` per
//!   actor serves both phases in sequence — the collect→lead transition
//!   naturally invalidates it (the destination is a different goal than
//!   the collect target's last position), no special-casing needed.
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

use super::locomotion::{step_along_waypoints, LOCOMOTION_ARRIVAL_EPSILON};
use super::navmesh_path::resolve_cached_waypoints;
use super::travel::resolve_destination as resolve_travel_destination;
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{
    EscortBehavior, EscortState, Escorted, GlobalTransform, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use std::collections::VecDeque;

/// Distance (world units) within which the target is considered
/// "collected" — the collect phase ends and the lead phase begins. Engine
/// default, same scale as `follow.rs::FOLLOW_DEFAULT_DISTANCE`.
const ESCORT_COLLECT_DISTANCE: f32 = 128.0;

/// Fallback pick radius (world units) around an actor's current position,
/// used when `EscortBehavior.destination_radius` is `None` (no PLDT /
/// radius 0) *and* `destination_form_id` doesn't resolve. Same scale as
/// `travel.rs::TRAVEL_DEFAULT_RADIUS`.
const ESCORT_DEFAULT_RADIUS: f32 = 512.0;

/// Collect-phase repath threshold (EX-16 item 3 Phase 4) — same value
/// and reasoning as `follow.rs::FOLLOW_REPATH_THRESHOLD`, kept as its
/// own constant rather than imported (see this module's doc).
const ESCORT_COLLECT_REPATH_THRESHOLD: f32 = 64.0;

/// Resolve this actor's Escort target once, on first sight. Mirrors
/// `follow.rs::resolve_follow_target`.
fn resolve_escort_target(world: &World, behavior: &EscortBehavior) -> Option<EntityId> {
    resolve_entity_by_global_form_id(world, behavior.target_form_id?)
}

/// Resolve (or pick) the lead-phase destination — delegates to
/// `travel_system::resolve_destination` (M42.7 extracted it to primitive
/// fields for exactly this kind of second consumer): `NearReference`-type
/// PLDT FormID first, hash-picked point within radius as the fallback.
fn resolve_destination(world: &World, behavior: &EscortBehavior, home: Vec3) -> Vec3 {
    resolve_travel_destination(
        world,
        behavior.destination_form_id,
        behavior.destination_radius.unwrap_or(ESCORT_DEFAULT_RADIUS),
        behavior.form_id,
        home,
    )
}

/// Which phase a staged [`EscortPending`] move belongs to — determines how
/// Pass 1b (which has `PhysicsWorld` available) finishes the decision.
enum EscortPendingKind {
    /// Walking toward a frozen destination — either already leading, or
    /// transitioning into the lead phase this very tick. `new_state` is
    /// `Some` only on the transition tick (Pass 2 must persist the newly
    /// resolved `EscortState`); `None` while already leading with
    /// unchanged state. Pass 1b checks arrival against `destination`.
    Lead {
        destination: Vec3,
        new_state: Option<EscortState>,
    },
    /// Still collecting: walking toward the target's live position.
    /// `new_state` is `Some` only on this actor's first tick with
    /// `EscortState` at all. Never "arrived" (only the lead phase tags
    /// [`Escorted`]).
    Collect { new_state: Option<EscortState> },
}

/// One actor's staged move input, collected in Pass 1a — before
/// `PhysicsWorld` is acquired in Pass 1b (#2134: `PhysicsWorld` must never
/// be live at the same time as a `GlobalTransform` read; both the collect
/// target's live position and `resolve_destination`'s `NearReference`
/// lookup read `GlobalTransform`).
struct EscortPending {
    entity: EntityId,
    current: Vec3,
    rotation: Quat,
    /// The raw (un-Y-adjusted) point this tick is walking toward —
    /// `step_along_waypoints` projects it onto the XZ plane itself, same
    /// as `travel_system`/`guard_system`'s usage.
    goal: Vec3,
    /// Resident-tile waypoints toward `goal` (EX-16 item 3 Phase 4).
    waypoints: VecDeque<Vec3>,
    /// The goal to persist into `NavPath` alongside `waypoints` —
    /// `resolve_cached_waypoints`'s *effective* goal, not necessarily
    /// `goal` itself. Identical to `goal` for the Lead phase (frozen,
    /// `0.0` threshold); may differ for the Collect phase (live target,
    /// real threshold) — see `follow_system`'s doc for why conflating
    /// the two there would silently defeat the threshold.
    effective_goal: Vec3,
    kind: EscortPendingKind,
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
    /// The updated `NavPath` cache to write in Pass 2 — `None` once
    /// `arrived` (nothing left to cache), `Some` otherwise (mirrors
    /// `TravelDecision`/`FollowDecision`'s identical contract).
    nav_path: Option<NavPath>,
}

/// Reusable per-frame scratch for [`escort_system_inner`] — captured by
/// [`make_escort_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct EscortScratch {
    pending: Vec<EscortPending>,
    decisions: Vec<EscortDecision>,
}

/// Drive the two-phase collect-then-lead locomotion for every
/// [`EscortBehavior`] actor not yet [`Escorted`]. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`/`travel_system`/`follow_system` — NPC placement roots
/// are propagation roots (no `Parent`), so `Transform` == world position
/// for them; both the collect target and the resolved destination
/// reference may be any REFR, so their positions are read via
/// `GlobalTransform`.
fn escort_system_inner(world: &World, dt: f32, scratch: &mut EscortScratch) {
    let Some(behavior_q) = world.query::<EscortBehavior>() else {
        return;
    };

    // ── Pass 1a: resolve targets/destinations (reads only). Both the
    // collect target's live position and `resolve_destination`'s
    // `NearReference` lookup read `GlobalTransform`, so `PhysicsWorld` is
    // NOT acquired in this scope (#2134). ──
    scratch.pending.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let escorted_q = world.query::<Escorted>();
        let state_q = world.query::<EscortState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();

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
                // Effective goal discarded (`_`): with a `0.0` threshold
                // it's always bit-identical to `destination`, whether
                // reused or freshly computed.
                let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                let (_, waypoints) =
                    resolve_cached_waypoints(cached, tile_q.as_ref(), current, destination, 0.0);
                scratch.pending.push(EscortPending {
                    entity,
                    current,
                    rotation: transform.rotation,
                    goal: destination,
                    waypoints,
                    effective_goal: destination,
                    kind: EscortPendingKind::Lead {
                        destination,
                        new_state: None,
                    },
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
                // travel_system moving on the very tick it resolves. A
                // brand-new destination, so this always misses any cache
                // (nothing was ever computed for it before) and falls
                // through to a fresh resolve — same shape as Travel's
                // first tick.
                let destination = resolve_destination(world, behavior, current);
                let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                let (_, waypoints) =
                    resolve_cached_waypoints(cached, tile_q.as_ref(), current, destination, 0.0);
                scratch.pending.push(EscortPending {
                    entity,
                    current,
                    rotation: transform.rotation,
                    goal: destination,
                    waypoints,
                    effective_goal: destination,
                    kind: EscortPendingKind::Lead {
                        destination,
                        new_state: Some(EscortState {
                            target_entity,
                            destination: Some(destination),
                        }),
                    },
                });
            } else {
                // Still collecting: walk toward the target's live
                // position, with the same repath-threshold reuse
                // `follow_system` uses for its own live target.
                let pos = live_target_pos.expect("collected == false implies Some");
                let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                let (effective_goal, waypoints) = resolve_cached_waypoints(
                    cached,
                    tile_q.as_ref(),
                    current,
                    pos,
                    ESCORT_COLLECT_REPATH_THRESHOLD,
                );
                scratch.pending.push(EscortPending {
                    entity,
                    current,
                    rotation: transform.rotation,
                    goal: pos,
                    waypoints,
                    effective_goal,
                    kind: EscortPendingKind::Collect {
                        new_state: existing_state.is_none().then_some(EscortState {
                            target_entity,
                            destination: None,
                        }),
                    },
                });
            }
        }
    }
    if scratch.pending.is_empty() {
        scratch.decisions.clear();
        return;
    }

    // ── Pass 1b: compute movement via `step_along_waypoints`. `Transform`
    // and `GlobalTransform` locks from Pass 1a have already dropped, so
    // `PhysicsWorld` is acquired here without ever overlapping either
    // (#2134). ──
    scratch.decisions.clear();
    {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for p in &scratch.pending {
            let (new_pos, rotation, waypoints) = step_along_waypoints(
                p.current,
                p.rotation,
                p.waypoints.clone(),
                p.goal,
                dt,
                physics.as_deref(),
            );
            let (state, arrived) = match &p.kind {
                EscortPendingKind::Lead {
                    destination,
                    new_state,
                } => {
                    let horiz_delta =
                        Vec3::new(new_pos.x - destination.x, 0.0, new_pos.z - destination.z);
                    let arrived = horiz_delta.length_squared()
                        <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON;
                    (*new_state, arrived)
                }
                EscortPendingKind::Collect { new_state } => (*new_state, false),
            };
            scratch.decisions.push(EscortDecision {
                entity: p.entity,
                translation: new_pos,
                rotation,
                state,
                arrived,
                // `effective_goal`, not `p.goal` — see `EscortPending`'s
                // own doc for why those two may differ in the Collect
                // phase (mirrors `follow_system`'s identical concern).
                nav_path: (!arrived).then_some(NavPath {
                    goal: p.effective_goal,
                    waypoints,
                }),
            });
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 2: apply writes (each a scoped single-type lock). ──
    if let Some(mut tq) = world.query_mut::<Transform>() {
        for d in &scratch.decisions {
            if let Some(t) = tq.get_mut(d.entity) {
                t.translation = d.translation;
                if let Some(r) = d.rotation {
                    t.rotation = r;
                }
            }
        }
    }
    if let Some(mut sq) = world.query_mut::<EscortState>() {
        for d in &scratch.decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
    if let Some(mut eq) = world.query_mut::<Escorted>() {
        for d in &scratch.decisions {
            if d.arrived {
                eq.insert(d.entity, Escorted);
            }
        }
    }
    if let Some(mut nq) = world.query_mut::<NavPath>() {
        for d in &scratch.decisions {
            match &d.nav_path {
                Some(path) => {
                    nq.insert(d.entity, path.clone());
                }
                None => {
                    nq.remove(d.entity);
                }
            }
        }
    }
}

/// Escort system factory — returns a closure with a persistent
/// [`EscortScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`escort_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_escort_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = EscortScratch::default();
    move |world: &World, dt: f32| {
        escort_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_escort_system`].
#[cfg(test)]
pub(crate) fn escort_system(world: &World, dt: f32) {
    escort_system_inner(world, dt, &mut EscortScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    fn spawn_entity_at(world: &mut World, form_id_raw: u32, pos: Vec3) -> EntityId {
        let mut pool = world.remove_resource::<FormIdPool>().unwrap_or_default();
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

        let sq = world
            .query::<EscortState>()
            .expect("EscortState registered");
        let state = sq.get(entity).expect("state written on first tick");
        assert_eq!(state.target_entity, None);
        assert!(
            state.destination.is_some(),
            "must resolve a lead-phase destination immediately"
        );
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

        // Transform before EscortState — matches `escort_system_inner`'s
        // acquisition order for this pair (#313).
        let tq = world.query::<Transform>().expect("Transform registered");
        let sq = world
            .query::<EscortState>()
            .expect("EscortState registered");
        let state = sq.get(actor).expect("state written on first tick");
        assert_eq!(state.target_entity, Some(target));
        assert!(
            state.destination.is_none(),
            "still collecting — must not have a destination yet"
        );

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

        let sq = world
            .query::<EscortState>()
            .expect("EscortState registered");
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
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)),
        );
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
            EscortState {
                target_entity: None,
                destination: Some(Vec3::new(50.0, 0.0, 50.0)),
            },
        );

        escort_system(&world, 0.5);

        {
            let eq = world.query::<Escorted>().expect("Escorted registered");
            assert!(
                eq.get(entity).is_some(),
                "actor at its destination must be tagged Escorted"
            );
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
        assert_eq!(
            pos_after_first, pos_after_second,
            "Escorted actors must not move on later ticks"
        );
    }

    /// #2134 regression guard: with a *real* `PhysicsWorld` resource
    /// installed, the collect-phase target's `GlobalTransform` read
    /// (Pass 1a) must still resolve correctly before `PhysicsWorld` is
    /// acquired in Pass 1b — proving the split didn't change behavior,
    /// only lock-acquisition order.
    #[test]
    fn escort_system_collects_target_with_a_real_physics_world_installed() {
        let mut world = World::new();
        register_all(&mut world);
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let target = spawn_entity_at(&mut world, 0x000F_0001, Vec3::new(1000.0, 0.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            EscortBehavior {
                target_form_id: Some(0x000F_0001),
                destination_form_id: None,
                destination_radius: Some(200.0),
                form_id: 0x000F_0002,
            },
        );

        escort_system(&world, 0.1);

        {
            let sq = world
                .query::<EscortState>()
                .expect("EscortState registered");
            let state = sq.get(actor).expect("state written on first tick");
            assert_eq!(state.target_entity, Some(target));
        }

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(actor).unwrap().translation.x > 0.0,
            "actor should still close toward the target with a real PhysicsWorld present"
        );
    }

    // ── EX-16 item 3 Phase 4: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos`, mirroring the other locomotion
    /// modules' identically-named test helper.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// The same 1000-unit two-triangle quad `travel`/`guard`/`follow`'s
    /// test modules each pin their own copy of.
    fn two_triangle_quad_navm() -> byroredux_plugin::esm::records::NavmRecord {
        use byroredux_plugin::esm::records::{NavmRecord, NavmTriangle};
        let yup_verts = [
            [0.0, 0.0, 0.0],
            [1000.0, 0.0, 0.0],
            [1000.0, 0.0, 1000.0],
            [0.0, 0.0, 1000.0],
        ];
        NavmRecord {
            vertices: yup_verts.iter().map(|v| zup_from_yup(*v)).collect(),
            triangles: vec![
                NavmTriangle {
                    vertices: [0, 1, 2],
                    edge_neighbours: [None, Some(1), None],
                    flags: 0,
                },
                NavmTriangle {
                    vertices: [0, 2, 3],
                    edge_neighbours: [Some(0), None, None],
                    flags: 0,
                },
            ],
            ..NavmRecord::default()
        }
    }

    /// Shared assertion both phase-specific tests below need: the actor
    /// stepped toward the shared-edge waypoint (500,0,500), not a
    /// straight line toward `raw_goal` that happens to look similar.
    fn assert_routed_through_waypoint(current: Vec3, raw_goal: Vec3, moved_to: Vec3) {
        let expected_toward_waypoint = current.move_towards(Vec3::new(500.0, 0.0, 500.0), 100.0);
        let naive_straight_line = current.move_towards(raw_goal, 100.0);
        assert!(
            (expected_toward_waypoint - naive_straight_line).length() > 10.0,
            "test fixture must be non-degenerate: pathed and naive directions should differ"
        );
        assert!(
            (moved_to - expected_toward_waypoint).length() < 1e-3,
            "actor should step toward the shared-edge waypoint (500,0,500), got {moved_to:?}, \
             expected ~{expected_toward_waypoint:?} (naive straight-line would have been {naive_straight_line:?})"
        );
    }

    #[test]
    fn escort_system_routes_through_a_resident_navmesh_tile_during_the_lead_phase() {
        let mut world = World::new();
        register_all(&mut world);
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let current = Vec3::new(900.0, 0.0, 100.0);
        let destination = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(current));
        world.insert(
            entity,
            EscortBehavior {
                target_form_id: None,
                destination_form_id: None,
                destination_radius: Some(1.0),
                form_id: 0x0010_0001,
            },
        );
        // Seed directly into the lead phase, mirroring
        // escort_system_tags_escorted_on_arrival_and_then_stops.
        world.insert(
            entity,
            EscortState {
                target_entity: None,
                destination: Some(destination),
            },
        );

        escort_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        let tq = world.query::<Transform>().expect("Transform registered");
        let moved_to = tq.get(entity).expect("actor transform").translation;
        assert_routed_through_waypoint(current, destination, moved_to);

        let nq = world.query::<NavPath>().expect("NavPath registered");
        let path = nq.get(entity).expect("cached during the lead phase");
        assert_eq!(path.goal, destination);
    }

    #[test]
    fn escort_system_routes_through_a_resident_navmesh_tile_during_the_collect_phase() {
        let mut world = World::new();
        register_all(&mut world);
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let current = Vec3::new(900.0, 0.0, 100.0);
        let target_pos = Vec3::new(600.0, 0.0, 900.0);
        let target = spawn_entity_at(&mut world, 0x0011_0001, target_pos);

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(current));
        world.insert(
            actor,
            EscortBehavior {
                target_form_id: Some(0x0011_0001),
                destination_form_id: None,
                destination_radius: Some(200.0),
                form_id: 0x0011_0002,
            },
        );

        escort_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        {
            let sq = world
                .query::<EscortState>()
                .expect("EscortState registered");
            assert_eq!(
                sq.get(actor).unwrap().target_entity,
                Some(target),
                "still collecting -- far outside ESCORT_COLLECT_DISTANCE"
            );
        }

        let tq = world.query::<Transform>().expect("Transform registered");
        let moved_to = tq.get(actor).expect("actor transform").translation;
        assert_routed_through_waypoint(current, target_pos, moved_to);
    }
}
