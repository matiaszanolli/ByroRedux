//! Wander procedure (M42.3) — the first non-Sandbox AI-package runtime and
//! the first NPC locomotion primitive in the engine. **Registered only
//! when `BYRO_WANDER` is set** (see `boot.rs`), mirroring the
//! `BYRO_SANDBOX_SIT` opt-in gate for `sandbox_seat_system`.
//!
//! For each [`WanderBehavior`] actor, walk in a straight line (no
//! pathing/NAVM) toward a randomly picked point within `wander_radius` of
//! the actor's own spawn position, pause for a few seconds on arrival,
//! then pick a new point and repeat indefinitely. [`WanderState`] is the
//! per-actor runtime state — lazily inserted the first tick an actor is
//! seen, since (unlike Sandbox's seat assignment) Wander never reaches a
//! terminal state.
//!
//! Randomness is **deterministic, not a `rand`-crate RNG** — target points
//! and pause durations are derived from a SplitMix64-style avalanche hash
//! seeded on `(WanderBehavior::form_id, WanderState::pick_count)`, the
//! same no-RNG-dependency convention `npc_spawn.rs::idle_desync` uses for
//! per-actor idle phase/speed desync. This keeps wandering save/reload-
//! stable without adding a dependency.
//!
//! ## v0 scope (documented approximations, mirroring `sandbox.rs`'s style)
//!
//! - **Single-tile pathing** (EX-16 item 3 Phase 4,
//!   `docs/engine/navmesh-pathfinding.md`) — landed alongside
//!   `patrol_system` (M42.8), which shares [`step_oscillating_wander`]
//!   verbatim and needed the exact same signature extension; deferred out
//!   of Phase 3 specifically to avoid rushing that shared-primitive
//!   change in alongside `travel_system`'s simpler (non-shared, one-shot)
//!   integration. [`step_oscillating_wander`] itself still only decides
//!   *when* to pause/re-pick — the resident-tile waypoint queue is
//!   resolved and consumed by the caller (`wander_system_inner`/
//!   `patrol_system_inner`) and passed in as an override, since
//!   `WanderState`/`PatrolState` are save-registered `MUTABLE_DELTA_COLUMNS`
//!   and must not gain a path field (see `crate::components::NavPath`'s
//!   own doc for why the cache lives outside them entirely, same as
//!   every other locomotion system's).
//! - **No target-reference resolution.** The wander *center* is always the
//!   actor's own position on first tick, never a resolved PLDT FormID
//!   target — the same v0 simplification `SandboxBehavior` uses (Sandbox's
//!   2026-07-14 investigation into `NearReference` resolution found only
//!   ~12% of vanilla packages resolve to anything spawnable; the same
//!   reasoning applies here without needing a second investigation).
//! - **No animation.** `AnimationPlayer` is untouched while an actor
//!   walks — Transform moves, pose doesn't. A real `walkforward.kf`-class
//!   path has never been verified against a game archive in this
//!   codebase; verifying one is deferred to a later on-device polish pass
//!   (the same class of visual debt Sandbox v0 accepted for legacy marker
//!   over-match).
//! - **No per-frame package re-evaluation.** `WanderBehavior` is attached
//!   once at spawn (`npc_spawn.rs`); an actor picked for Wander at spawn
//!   keeps wandering even if its package's schedule would no longer be
//!   active at the current game hour — the same limitation
//!   `SandboxBehavior` has today.
//! - **Ground-snapped, not physically simulated.** Y is corrected each
//!   tick via a downward raycast against static colliders
//!   (`PhysicsWorld::cast_ray_down`, the same mechanism `scene.rs` uses for
//!   camera placement), not through the physics simulation itself — an
//!   actor can't be pushed, blocked, or fall.

use super::locomotion::{pop_reached_waypoint, step_toward};
use super::navmesh_path::resolve_cached_waypoints;
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{Transform, WanderBehavior, WanderPhase, WanderState};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use std::collections::VecDeque;

/// Fallback wander radius (world units) around an actor's spawn position,
/// used when `WanderBehavior.wander_radius` is `None` (no PLDT / radius 0).
/// Reuses `sandbox.rs::SEAT_SEARCH_RADIUS`'s scale for consistency — both
/// are "how far does this ambient behavior reach without an authored
/// radius" defaults for the same class of FNV-interior/yard-scale content.
const WANDER_DEFAULT_RADIUS: f32 = 512.0;

/// Pause duration range (seconds) between walks. Engine default.
const WANDER_PAUSE_MIN: f32 = 3.0;
const WANDER_PAUSE_MAX: f32 = 8.0;

/// SplitMix64-style avalanche — same core as `npc_spawn.rs::idle_desync`,
/// extracted as a standalone step since Wander needs more than the two
/// fractions `idle_desync` packs from one seed (target angle, target
/// distance, and pause duration each need their own draw).
fn avalanche(seed: u64) -> u64 {
    let mut z = seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic per-(actor, pick, draw) seed. `salt` distinguishes
/// independent draws from the same `(form_id, pick_count)` pair (angle vs.
/// distance vs. pause duration) without them collapsing to the same value.
fn wander_seed(form_id: u32, pick_count: u32, salt: u64) -> u64 {
    let base = (form_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((pick_count as u64) << 32)
        .wrapping_add(salt)
        .wrapping_add(0x1234_5678);
    avalanche(base)
}

/// Map an avalanche hash to `[0, 1)`.
fn unit_frac(h: u64) -> f32 {
    ((h & 0xFFFF_FFFF) as f32) / (u32::MAX as f32 + 1.0)
}

/// Pick a new wander target: a random point within `radius` of `home` on
/// the XZ plane (Y is ground-snapped independently by the caller). Pure —
/// unit-tested directly, mirroring `idle_desync`'s test style.
///
/// `pub(crate)` — `travel_system` (M42.4) reuses this directly (always
/// with `pick_count = 0`, since Travel only picks once) as its
/// no-target-resolved fallback, rather than duplicating the hash logic.
pub(crate) fn pick_wander_target(home: Vec3, radius: f32, form_id: u32, pick_count: u32) -> Vec3 {
    let angle = unit_frac(wander_seed(form_id, pick_count, 0)) * std::f32::consts::TAU;
    let dist = unit_frac(wander_seed(form_id, pick_count, 1)) * radius;
    home + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist)
}

/// Pick a new pause duration in `[WANDER_PAUSE_MIN, WANDER_PAUSE_MAX)`.
/// Pure — unit-tested directly.
///
/// `pub(crate)` — `step_oscillating_wander` below needs it, and
/// (transitively) so does `patrol_system` (M42.8).
pub(crate) fn pick_pause_duration(form_id: u32, pick_count: u32) -> f32 {
    let frac = unit_frac(wander_seed(form_id, pick_count, 2));
    WANDER_PAUSE_MIN + frac * (WANDER_PAUSE_MAX - WANDER_PAUSE_MIN)
}

/// Plain-value shape of one oscillating-walk actor's state, decoupled from
/// any specific ECS component type — both `WanderState` and `PatrolState`
/// (M42.8) convert to/from this rather than either depending on the
/// other's component type.
pub(crate) struct OscillateWalk {
    pub home: Vec3,
    pub target: Vec3,
    pub phase: WanderPhase,
    pub pick_count: u32,
}

/// One tick of the shared "walk to a random point within `radius`, pause,
/// repeat" state machine — the core algorithm `wander_system` runs,
/// extracted so `patrol_system` (M42.8) can reuse it verbatim rather than
/// duplicating the phase-transition logic. v0 Patrol has no route data to
/// differ on (see `crates/plugin`'s `PROCEDURE_PATROL` doc for why), so
/// the two procedures share this exactly; only the ECS component types
/// (and their opt-in env gates) differ. Returns the new translation, the
/// new rotation (`None` when paused or already at target), and the
/// updated state.
///
/// `waypoint_override` (EX-16 item 3 Phase 4) — when `Some`, this is what
/// `step_toward` actually walks toward this tick instead of `state.target`
/// directly; the arrival/Walking→Paused-transition check still compares
/// against `state.target` unconditionally, so pausing only triggers once
/// the actor has truly walked the whole path to the final wander target,
/// not just reached an intermediate waypoint. `None` reproduces the exact
/// pre-Phase-4 straight-line behavior — the caller's own resident-tile
/// waypoint queue is what decides which one this call gets; this function
/// itself has no NAVM awareness at all, matching the "landed ahead of a
/// consumer that owns the actual pathing decision" shape every other
/// locomotion system in this codebase already uses (`step_toward` itself
/// knows nothing about NAVM either).
#[allow(clippy::too_many_arguments)] // Pure locomotion step with explicit authored inputs.
pub(crate) fn step_oscillating_wander(
    current: Vec3,
    current_rotation: Quat,
    dt: f32,
    physics: Option<&byroredux_physics::PhysicsWorld>,
    radius: f32,
    form_id: u32,
    mut state: OscillateWalk,
    waypoint_override: Option<Vec3>,
) -> (Vec3, Option<Quat>, OscillateWalk) {
    match state.phase {
        WanderPhase::Paused { remaining } => {
            let remaining = remaining - dt;
            if remaining <= 0.0 {
                state.pick_count = state.pick_count.wrapping_add(1);
                state.target = pick_wander_target(state.home, radius, form_id, state.pick_count);
                state.phase = WanderPhase::Walking;
            } else {
                state.phase = WanderPhase::Paused { remaining };
            }
            (current, None, state)
        }
        WanderPhase::Walking => {
            // Move on the XZ plane only — Y is re-derived from the ground
            // below, not interpolated toward the target's Y (which was
            // only ever a copy of `home.y` at pick time and drifts from
            // real terrain on sloped ground).
            let step_point = waypoint_override.unwrap_or(state.target);
            let step_xz = Vec3::new(step_point.x, current.y, step_point.z);
            let (new_pos, rotation) = step_toward(current, current_rotation, step_xz, dt, physics);

            let horiz_delta =
                Vec3::new(new_pos.x - state.target.x, 0.0, new_pos.z - state.target.z);
            if horiz_delta.length_squared()
                <= super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                    * super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
            {
                state.phase = WanderPhase::Paused {
                    remaining: pick_pause_duration(form_id, state.pick_count),
                };
            }
            (new_pos, rotation, state)
        }
    }
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors `sandbox_seat_system`'s
/// two-pass read-then-write structure).
struct WanderDecision {
    entity: EntityId,
    source_rotation: Quat,
    radius: f32,
    form_id: u32,
    waypoint_override: Option<Vec3>,
    effective_goal: Vec3,
    waypoints: VecDeque<Vec3>,
    translation: Vec3,
    /// `None` when the actor didn't move enough this tick to have a
    /// meaningful facing direction (e.g. paused, or already at target).
    rotation: Option<Quat>,
    state: WanderState,
    /// The updated `NavPath` cache to write in Pass 2 (EX-16 item 3
    /// Phase 4) — `None` while paused (nothing to walk toward) or once
    /// the final wander target is reached (mirrors every other
    /// locomotion system's "drop the stale cache" posture), `Some`
    /// while actively walking a resident-tile path.
    nav_path: Option<NavPath>,
}

/// Reusable per-frame scratch for [`wander_system_inner`] — captured by
/// [`make_wander_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct WanderScratch {
    decisions: Vec<WanderDecision>,
}

/// Drive straight-line walk-to-point locomotion for every [`WanderBehavior`]
/// actor. Registered `add_exclusive(Stage::PostUpdate, …)` so it reads this
/// frame's propagated `GlobalTransform`-equivalent position via `Transform`
/// directly — NPC placement roots are propagation roots (no `Parent`), so
/// `Transform` == world position for them, same as `sandbox_seat_system`.
fn wander_system_inner(world: &World, dt: f32, scratch: &mut WanderScratch) {
    // ── Pass 1a: gather component snapshots. `PhysicsWorld` is NOT
    // acquired in this scope, so it never overlaps the five storage read
    // guards (#3262, mirroring the #2134 sibling shape). ──
    scratch.decisions.clear();
    {
        let Some(behavior_q) = world.query::<WanderBehavior>() else {
            return;
        };
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<WanderState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();
        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let radius = behavior.wander_radius.unwrap_or(WANDER_DEFAULT_RADIUS);
            let state = state_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .copied()
                .unwrap_or_else(|| {
                    let home = transform.translation;
                    WanderState {
                        home,
                        target: pick_wander_target(home, radius, behavior.form_id, 0),
                        phase: WanderPhase::Walking,
                        pick_count: 0,
                    }
                });

            // EX-16 item 3 Phase 4: only bother resolving a path while
            // actually walking this leg — mirrors `guard_system`'s same
            // "skip the resident-tile scan while nothing to walk toward"
            // shape (here: while `Paused`).
            let (waypoint_override, effective_goal, waypoints) =
                if matches!(state.phase, WanderPhase::Walking) {
                    let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                    let (effective_goal, waypoints) = resolve_cached_waypoints(
                        cached,
                        tile_q.as_ref(),
                        transform.translation,
                        state.target,
                        0.0,
                    );
                    (waypoints.front().copied(), effective_goal, waypoints)
                } else {
                    (None, state.target, VecDeque::new())
                };

            scratch.decisions.push(WanderDecision {
                entity,
                source_rotation: transform.rotation,
                radius,
                form_id: behavior.form_id,
                waypoint_override,
                effective_goal,
                waypoints,
                translation: transform.translation,
                rotation: None,
                state,
                nav_path: None,
            });
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 1b: finish movement with only `PhysicsWorld` live. All
    // component guards from Pass 1a have dropped before this acquisition
    // (#3262). ──
    {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for d in &mut scratch.decisions {
            let (new_pos, rotation, new_state) = step_oscillating_wander(
                d.translation,
                d.source_rotation,
                dt,
                physics.as_deref(),
                d.radius,
                d.form_id,
                OscillateWalk {
                    home: d.state.home,
                    target: d.state.target,
                    phase: d.state.phase,
                    pick_count: d.state.pick_count,
                },
                d.waypoint_override,
            );
            if !d.waypoints.is_empty() {
                pop_reached_waypoint(new_pos, &mut d.waypoints);
            }
            d.translation = new_pos;
            d.rotation = rotation;
            d.state = WanderState {
                home: new_state.home,
                target: new_state.target,
                phase: new_state.phase,
                pick_count: new_state.pick_count,
            };
            // `Some` only while still walking — arriving (transition to
            // Paused) drops the cache, same "arrived, remove" posture
            // every other locomotion system in this codebase uses.
            d.nav_path = matches!(new_state.phase, WanderPhase::Walking).then(|| NavPath {
                goal: d.effective_goal,
                waypoints: std::mem::take(&mut d.waypoints),
            });
        }
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
    if let Some(mut sq) = world.query_mut::<WanderState>() {
        for d in &scratch.decisions {
            sq.insert(d.entity, d.state);
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

/// Wander system factory — returns a closure with a persistent
/// [`WanderScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`wander_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_wander_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = WanderScratch::default();
    move |world: &World, dt: f32| {
        wander_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_wander_system`].
#[cfg(test)]
pub(crate) fn wander_system(world: &World, dt: f32) {
    wander_system_inner(world, dt, &mut WanderScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_wander_target_is_deterministic_per_form_id_and_pick() {
        // Same (form_id, pick_count) → identical target every call (save/
        // reload + cell re-stream must re-derive the same point).
        let a = pick_wander_target(Vec3::ZERO, 512.0, 0x0010_A2F3, 3);
        let b = pick_wander_target(Vec3::ZERO, 512.0, 0x0010_A2F3, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn pick_wander_target_stays_within_radius_and_varies_by_pick() {
        let home = Vec3::new(100.0, 20.0, -50.0);
        let radius = 300.0;
        let mut targets = Vec::new();
        for pick in 0..5 {
            let t = pick_wander_target(home, radius, 0x0100_0001, pick);
            let horiz = Vec3::new(t.x - home.x, 0.0, t.z - home.z);
            assert!(
                horiz.length() <= radius + 1e-3,
                "target {t:?} escaped radius {radius} around {home:?}"
            );
            assert_eq!(
                t.y, home.y,
                "target Y must stay at home.y (re-derived from terrain by the caller)"
            );
            targets.push(t);
        }
        // Sequential picks diverge (avalanche hash, not a fixed offset).
        assert!((targets[0] - targets[1]).length() > 1e-3);
        assert!((targets[1] - targets[2]).length() > 1e-3);
    }

    #[test]
    fn pick_wander_target_diverges_across_adjacent_form_ids() {
        // Bethesda hands FormIds out in sequential runs — adjacent ids must
        // not collapse to near-identical targets (avalanche, not linear).
        let home = Vec3::ZERO;
        let t0 = pick_wander_target(home, 500.0, 0x0100_0001, 0);
        let t1 = pick_wander_target(home, 500.0, 0x0100_0002, 0);
        assert!((t0 - t1).length() > 1.0);
    }

    #[test]
    fn pick_pause_duration_is_deterministic_and_in_range() {
        let a = pick_pause_duration(0xDEAD_BEEF, 1);
        let b = pick_pause_duration(0xDEAD_BEEF, 1);
        assert_eq!(a, b);
        for pick in 0..10 {
            let d = pick_pause_duration(0xABCD_EF01, pick);
            assert!(
                (WANDER_PAUSE_MIN..WANDER_PAUSE_MAX).contains(&d),
                "pause {d} outside [{WANDER_PAUSE_MIN},{WANDER_PAUSE_MAX})"
            );
        }
    }

    #[test]
    fn wander_system_moves_actor_toward_target_with_no_physics_world() {
        // Synthetic World with no PhysicsWorld resource — the ground-snap
        // raycast must be skipped (not panic), falling back to the
        // XZ-moved Y.
        let mut world = World::new();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            WanderBehavior {
                wander_radius: Some(200.0),
                form_id: 0x0002_0002,
            },
        );

        wander_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        let t = tq.get(entity).expect("actor transform");
        assert!(
            t.translation.length() > 0.0,
            "actor should have moved from the origin on the first tick"
        );

        let sq = world
            .query::<WanderState>()
            .expect("WanderState registered");
        assert!(
            sq.get(entity).is_some(),
            "wander_system must lazily insert WanderState on first tick"
        );
    }

    /// #3262 regression guard: the component snapshot phase must finish
    /// before a real `PhysicsWorld` guard is acquired. Behavior remains
    /// unchanged when the optional resource is present.
    #[test]
    fn wander_system_moves_actor_with_a_real_physics_world_installed() {
        let mut world = World::new();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<Transform>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            WanderBehavior {
                wander_radius: Some(200.0),
                form_id: 0x0002_3262,
            },
        );

        wander_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.length() > 0.0,
            "actor should still move with a real PhysicsWorld present"
        );
    }

    /// Regression for #2033 / PERF-D1-2026-07-16-01. Pre-fix,
    /// `wander_system` declared `let mut decisions: Vec<WanderDecision> =
    /// Vec::new()` fresh every call — a per-frame allocation regardless of
    /// how it was driven. Post-fix, [`WanderScratch::decisions`] is
    /// captured once by [`make_wander_system`] and reused via `clear()`
    /// each tick. Drives [`wander_system_inner`] directly against the same
    /// persistent scratch across two ticks and asserts (a) the backing
    /// allocation's capacity doesn't grow on the second tick (the actual
    /// perf property this fix delivers) and (b) `len()` resets to 1 each
    /// tick rather than accumulating — the second assertion is what would
    /// actually catch a missing `clear()` call, since a growing `Vec`
    /// alone wouldn't be visible from outside without it.
    #[test]
    fn wander_scratch_reuses_allocation_across_frames() {
        let mut world = World::new();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            WanderBehavior {
                wander_radius: Some(200.0),
                form_id: 0x0002_0002,
            },
        );

        let mut scratch = WanderScratch::default();
        wander_system_inner(&world, 0.5, &mut scratch);
        assert_eq!(
            scratch.decisions.len(),
            1,
            "first tick must populate exactly one decision"
        );
        let cap_after_first = scratch.decisions.capacity();
        assert!(cap_after_first > 0);

        wander_system_inner(&world, 0.5, &mut scratch);
        assert_eq!(
            scratch.decisions.len(),
            1,
            "clear() must reset len each frame, not accumulate stale decisions"
        );
        assert_eq!(
            scratch.decisions.capacity(),
            cap_after_first,
            "capacity must not grow on a second frame with the same entity count — \
             the backing allocation must be reused via clear(), not reallocated"
        );
    }

    // ── EX-16 item 3 Phase 4: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos`, mirroring the other locomotion
    /// modules' identically-named test helper.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// The same 1000-unit two-triangle quad `travel`/`guard`/`follow`/
    /// `escort`'s test modules each pin their own copy of.
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

    #[test]
    fn wander_system_routes_through_a_resident_navmesh_tile_instead_of_straight_line() {
        let mut world = World::new();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        // Same non-mirror-symmetric pair every other locomotion module's
        // equivalent test uses.
        let current = Vec3::new(900.0, 0.0, 100.0);
        let target = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(current));
        world.insert(
            entity,
            WanderBehavior {
                wander_radius: Some(1.0), // irrelevant: WanderState below fixes the target
                form_id: 0x0003_0001,
            },
        );
        // Seed directly into a Walking leg toward a known target, instead
        // of the (deterministic but arithmetic-derived) hash-picked one.
        world.insert(
            entity,
            WanderState {
                home: current,
                target,
                phase: WanderPhase::Walking,
                pick_count: 0,
            },
        );

        wander_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        let expected_toward_waypoint = current.move_towards(Vec3::new(500.0, 0.0, 500.0), 100.0);
        let naive_straight_line = current.move_towards(target, 100.0);
        assert!(
            (expected_toward_waypoint - naive_straight_line).length() > 10.0,
            "test fixture must be non-degenerate: pathed and naive directions should differ"
        );

        let tq = world.query::<Transform>().expect("Transform registered");
        let moved_to = tq.get(entity).expect("actor transform").translation;
        assert!(
            (moved_to - expected_toward_waypoint).length() < 1e-3,
            "actor should step toward the shared-edge waypoint (500,0,500), got {moved_to:?}, \
             expected ~{expected_toward_waypoint:?} (naive straight-line would have been {naive_straight_line:?})"
        );

        let nq = world.query::<NavPath>().expect("NavPath registered");
        let path = nq.get(entity).expect("cached while walking this leg");
        assert_eq!(path.goal, target);
    }

    #[test]
    fn wander_system_skips_the_resident_tile_scan_and_drops_the_cache_while_paused() {
        let mut world = World::new();
        world.register::<WanderBehavior>();
        world.register::<WanderState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(10.0, 0.0, 10.0)),
        );
        world.insert(
            entity,
            WanderBehavior {
                wander_radius: Some(200.0),
                form_id: 0x0003_0002,
            },
        );
        // Already paused, with a stale cached path left over from a
        // previous leg — must be dropped, not reused or extended.
        world.insert(
            entity,
            WanderState {
                home: Vec3::new(10.0, 0.0, 10.0),
                target: Vec3::new(10.0, 0.0, 10.0),
                phase: WanderPhase::Paused { remaining: 5.0 },
                pick_count: 0,
            },
        );
        world.insert(
            entity,
            NavPath {
                goal: Vec3::new(999.0, 0.0, 999.0),
                waypoints: std::collections::VecDeque::from(vec![Vec3::new(999.0, 0.0, 999.0)]),
            },
        );

        wander_system(&world, 0.5);

        let nq = world.query::<NavPath>().expect("NavPath registered");
        assert!(
            nq.get(entity).is_none(),
            "paused — nothing to walk toward this tick, stale cache must be cleared"
        );
    }
}
