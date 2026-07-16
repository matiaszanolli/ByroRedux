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
//! - **No pathing.** Straight-line walk to point; a wall or obstacle
//!   between the actor and its target isn't routed around. Fine for open
//!   ground (the primary FNV/FO3 Wander use case — yard/plaza sandboxing);
//!   a walled room would need real navigation, out of scope here.
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

use super::locomotion::step_toward;
use byroredux_core::ecs::components::{Transform, WanderBehavior, WanderPhase, WanderState};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};

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
fn pick_pause_duration(form_id: u32, pick_count: u32) -> f32 {
    let frac = unit_frac(wander_seed(form_id, pick_count, 2));
    WANDER_PAUSE_MIN + frac * (WANDER_PAUSE_MAX - WANDER_PAUSE_MIN)
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors `sandbox_seat_system`'s
/// two-pass read-then-write structure).
struct WanderDecision {
    entity: EntityId,
    translation: Vec3,
    /// `None` when the actor didn't move enough this tick to have a
    /// meaningful facing direction (e.g. paused, or already at target).
    rotation: Option<Quat>,
    state: WanderState,
}

/// Drive straight-line walk-to-point locomotion for every [`WanderBehavior`]
/// actor. Registered `add_exclusive(Stage::PostUpdate, …)` so it reads this
/// frame's propagated `GlobalTransform`-equivalent position via `Transform`
/// directly — NPC placement roots are propagation roots (no `Parent`), so
/// `Transform` == world position for them, same as `sandbox_seat_system`.
pub fn wander_system(world: &World, dt: f32) {
    let Some(behavior_q) = world.query::<WanderBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    // Held guards are distinct component/resource types, so the
    // lock-tracker sees no conflict; writes happen in Pass 2 after these
    // read guards drop.
    let mut decisions: Vec<WanderDecision> = Vec::new();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<WanderState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let radius = behavior.wander_radius.unwrap_or(WANDER_DEFAULT_RADIUS);
            let mut state = state_q
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

            match state.phase {
                WanderPhase::Paused { remaining } => {
                    let remaining = remaining - dt;
                    if remaining <= 0.0 {
                        state.pick_count = state.pick_count.wrapping_add(1);
                        state.target =
                            pick_wander_target(state.home, radius, behavior.form_id, state.pick_count);
                        state.phase = WanderPhase::Walking;
                    } else {
                        state.phase = WanderPhase::Paused { remaining };
                    }
                    decisions.push(WanderDecision {
                        entity,
                        translation: transform.translation,
                        rotation: None,
                        state,
                    });
                }
                WanderPhase::Walking => {
                    let current = transform.translation;
                    // Move on the XZ plane only — Y is re-derived from the
                    // ground below, not interpolated toward the target's Y
                    // (which was only ever a copy of `home.y` at pick time
                    // and drifts from real terrain on sloped ground).
                    let target_xz = Vec3::new(state.target.x, current.y, state.target.z);
                    let (new_pos, rotation) =
                        step_toward(current, transform.rotation, target_xz, dt, physics.as_deref());

                    let horiz_delta =
                        Vec3::new(new_pos.x - state.target.x, 0.0, new_pos.z - state.target.z);
                    if horiz_delta.length_squared()
                        <= super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                            * super::locomotion::LOCOMOTION_ARRIVAL_EPSILON
                    {
                        state.phase = WanderPhase::Paused {
                            remaining: pick_pause_duration(behavior.form_id, state.pick_count),
                        };
                    }

                    decisions.push(WanderDecision {
                        entity,
                        translation: new_pos,
                        rotation,
                        state,
                    });
                }
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
    if let Some(mut sq) = world.query_mut::<WanderState>() {
        for d in &decisions {
            sq.insert(d.entity, d.state);
        }
    }
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
            assert_eq!(t.y, home.y, "target Y must stay at home.y (re-derived from terrain by the caller)");
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
                d >= WANDER_PAUSE_MIN && d < WANDER_PAUSE_MAX,
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
        world.insert(
            entity,
            Transform::from_translation(Vec3::ZERO),
        );
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

        let sq = world.query::<WanderState>().expect("WanderState registered");
        assert!(
            sq.get(entity).is_some(),
            "wander_system must lazily insert WanderState on first tick"
        );
    }
}
