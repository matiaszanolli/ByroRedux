//! Patrol procedure (M42.8) — the sixth AI-package runtime, and the first
//! that runs no algorithm of its own: it calls
//! `wander_system`'s shared oscillating-walk core
//! (`super::wander::step_oscillating_wander`) directly. **Registered only
//! when `BYRO_PATROL` is set** (see `boot.rs`), mirroring
//! `BYRO_GUARD`/`BYRO_ESCORT`/`BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`.
//!
//! Real Bethesda Patrol packages walk a route defined by linked
//! patrol-idle markers — data this codebase decodes nowhere (it lives
//! outside `PACK`'s own sub-records; see `crates/plugin`'s
//! `PROCEDURE_PATROL` doc). Absent that, there is nothing to distinguish
//! v0 Patrol from Wander: both reduce to "walk to a random point within a
//! radius, pause, repeat" with no target-reference resolution. Rather than
//! duplicate `wander_system`'s ~40-line state machine under a new name (a
//! straight copy-paste that would drift the moment one side changes),
//! `patrol_system` reuses the exact same core function, differing only in
//! which component types it reads/writes ([`PatrolBehavior`]/
//! [`PatrolState`] instead of `WanderBehavior`/`WanderState`) — kept
//! separate so Patrol and Wander actors stay independently selectable and
//! inspectable, matching every other M42 procedure having its own
//! component pair.
//!
//! If real patrol-route data is ever decoded, this is the seam to swap:
//! `patrol_system` would gain its own state machine here without touching
//! `wander_system` at all.
//!
//! v0 scope: identical to `wander.rs`'s documented approximations — no
//! pathing, no target-reference resolution, no animation-clip swap, no
//! per-frame package re-evaluation.

use super::wander::{step_oscillating_wander, OscillateWalk};
use byroredux_core::ecs::components::{PatrolBehavior, PatrolState, Transform, WanderPhase};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};

/// Fallback patrol radius (world units) around an actor's spawn position,
/// used when `PatrolBehavior.patrol_radius` is `None` (no PLDT / radius
/// 0). Same value as `wander.rs::WANDER_DEFAULT_RADIUS` — the two
/// procedures share the same v0 algorithm, so they share the same default.
const PATROL_DEFAULT_RADIUS: f32 = 512.0;

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors `wander_system`'s
/// two-pass structure).
struct PatrolDecision {
    entity: EntityId,
    translation: Vec3,
    rotation: Option<Quat>,
    state: PatrolState,
}

/// Drive `wander_system`'s shared oscillating-walk core for every
/// [`PatrolBehavior`] actor. Registered `add_exclusive(Stage::PostUpdate,
/// …)`, same stage/slot family as `wander_system` — NPC placement roots
/// are propagation roots (no `Parent`), so `Transform` == world position
/// for them.
pub fn patrol_system(world: &World, dt: f32) {
    let Some(behavior_q) = world.query::<PatrolBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    let mut decisions: Vec<PatrolDecision> = Vec::new();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<PatrolState>();
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let radius = behavior.patrol_radius.unwrap_or(PATROL_DEFAULT_RADIUS);
            let state = state_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .copied()
                .unwrap_or_else(|| {
                    let home = transform.translation;
                    PatrolState {
                        home,
                        target: super::wander::pick_wander_target(home, radius, behavior.form_id, 0),
                        phase: WanderPhase::Walking,
                        pick_count: 0,
                    }
                });

            let (new_pos, rotation, new_state) = step_oscillating_wander(
                transform.translation,
                transform.rotation,
                dt,
                physics.as_deref(),
                radius,
                behavior.form_id,
                OscillateWalk {
                    home: state.home,
                    target: state.target,
                    phase: state.phase,
                    pick_count: state.pick_count,
                },
            );

            decisions.push(PatrolDecision {
                entity,
                translation: new_pos,
                rotation,
                state: PatrolState {
                    home: new_state.home,
                    target: new_state.target,
                    phase: new_state.phase,
                    pick_count: new_state.pick_count,
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
    if let Some(mut sq) = world.query_mut::<PatrolState>() {
        for d in &decisions {
            sq.insert(d.entity, d.state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patrol_system_moves_actor_toward_target_with_no_physics_world() {
        // Mirrors wander_system's equivalent test — same algorithm, so the
        // same synthetic-World shape must produce the same kind of result.
        let mut world = World::new();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            PatrolBehavior { patrol_radius: Some(200.0), form_id: 0x000D_0001 },
        );

        patrol_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.length() > 0.0,
            "actor should have moved from the origin on the first tick"
        );

        let sq = world.query::<PatrolState>().expect("PatrolState registered");
        assert!(
            sq.get(entity).is_some(),
            "patrol_system must lazily insert PatrolState on first tick"
        );
    }

    #[test]
    fn patrol_system_pauses_on_arrival_like_wander() {
        let mut world = World::new();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<Transform>();

        let entity = world.spawn();
        // Seed state already at its own target — arrival is immediate.
        world.insert(entity, Transform::from_translation(Vec3::new(10.0, 0.0, 10.0)));
        world.insert(
            entity,
            PatrolBehavior { patrol_radius: Some(200.0), form_id: 0x000D_0002 },
        );
        world.insert(
            entity,
            PatrolState {
                home: Vec3::new(10.0, 0.0, 10.0),
                target: Vec3::new(10.0, 0.0, 10.0),
                phase: WanderPhase::Walking,
                pick_count: 0,
            },
        );

        patrol_system(&world, 0.5);

        let sq = world.query::<PatrolState>().expect("PatrolState registered");
        assert!(
            matches!(sq.get(entity).unwrap().phase, WanderPhase::Paused { .. }),
            "actor already at its target must transition to Paused, not keep walking in place"
        );
    }
}
