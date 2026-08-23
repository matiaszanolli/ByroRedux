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
//! v0 scope: identical to `wander.rs`'s documented approximations,
//! including single-tile pathing (EX-16 item 3 Phase 4,
//! `docs/engine/navmesh-pathfinding.md`) — `patrol_system` resolves and
//! consumes its own `crate::components::NavPath` cache exactly the way
//! `wander_system` does, passing the result into the same
//! `waypoint_override` parameter of the shared `step_oscillating_wander`
//! core; no target-reference resolution, no animation-clip swap, no
//! per-frame package re-evaluation.

use super::locomotion::pop_reached_waypoint;
use super::navmesh_path::resolve_cached_waypoints;
use super::wander::{step_oscillating_wander, OscillateWalk};
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{PatrolBehavior, PatrolState, Transform, WanderPhase};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use std::collections::VecDeque;

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
    /// The updated `NavPath` cache to write in Pass 2 — mirrors
    /// `WanderDecision::nav_path`'s exact contract.
    nav_path: Option<NavPath>,
}

/// Reusable per-frame scratch for [`patrol_system_inner`] — captured by
/// [`make_patrol_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct PatrolScratch {
    decisions: Vec<PatrolDecision>,
}

/// Drive `wander_system`'s shared oscillating-walk core for every
/// [`PatrolBehavior`] actor. Registered `add_exclusive(Stage::PostUpdate,
/// …)`, same stage/slot family as `wander_system` — NPC placement roots
/// are propagation roots (no `Parent`), so `Transform` == world position
/// for them.
fn patrol_system_inner(world: &World, dt: f32, scratch: &mut PatrolScratch) {
    let Some(behavior_q) = world.query::<PatrolBehavior>() else {
        return;
    };

    // ── Pass 1: gather decisions (reads only). ──
    scratch.decisions.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<PatrolState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();
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
                        target: super::wander::pick_wander_target(
                            home,
                            radius,
                            behavior.form_id,
                            0,
                        ),
                        phase: WanderPhase::Walking,
                        pick_count: 0,
                    }
                });

            // EX-16 item 3 Phase 4: mirrors `wander_system_inner` exactly
            // (same shared primitive, same caching shape) — only resolve
            // a path while actually walking this leg.
            let (waypoint_override, effective_goal, mut waypoints) =
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
                waypoint_override,
            );
            if !waypoints.is_empty() {
                pop_reached_waypoint(new_pos, &mut waypoints);
            }
            let nav_path = matches!(new_state.phase, WanderPhase::Walking).then_some(NavPath {
                goal: effective_goal,
                waypoints,
            });

            scratch.decisions.push(PatrolDecision {
                entity,
                translation: new_pos,
                rotation,
                nav_path,
                state: PatrolState {
                    home: new_state.home,
                    target: new_state.target,
                    phase: new_state.phase,
                    pick_count: new_state.pick_count,
                },
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
    if let Some(mut sq) = world.query_mut::<PatrolState>() {
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

/// Patrol system factory — returns a closure with a persistent
/// [`PatrolScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`patrol_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_patrol_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = PatrolScratch::default();
    move |world: &World, dt: f32| {
        patrol_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_patrol_system`].
#[cfg(test)]
pub(crate) fn patrol_system(world: &World, dt: f32) {
    patrol_system_inner(world, dt, &mut PatrolScratch::default());
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
            PatrolBehavior {
                patrol_radius: Some(200.0),
                form_id: 0x000D_0001,
            },
        );

        patrol_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.length() > 0.0,
            "actor should have moved from the origin on the first tick"
        );

        let sq = world
            .query::<PatrolState>()
            .expect("PatrolState registered");
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
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(10.0, 0.0, 10.0)),
        );
        world.insert(
            entity,
            PatrolBehavior {
                patrol_radius: Some(200.0),
                form_id: 0x000D_0002,
            },
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

        let sq = world
            .query::<PatrolState>()
            .expect("PatrolState registered");
        assert!(
            matches!(sq.get(entity).unwrap().phase, WanderPhase::Paused { .. }),
            "actor already at its target must transition to Paused, not keep walking in place"
        );
    }

    // ── EX-16 item 3 Phase 4: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos`, mirroring `wander::tests::
    /// zup_from_yup` (Patrol's own copy, per this codebase's per-module
    /// test-fixture convention).
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// The same 1000-unit two-triangle quad every other locomotion
    /// module's test suite pins.
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
    fn patrol_system_routes_through_a_resident_navmesh_tile_instead_of_straight_line() {
        // Pins that PatrolBehavior/PatrolState get the exact same
        // pathing treatment WanderBehavior/WanderState do via the shared
        // step_oscillating_wander core -- not a re-test of the algorithm
        // itself (navmesh_path's own suite already covers that), just
        // that patrol_system_inner wires the override/cache through
        // correctly for its own component types.
        let mut world = World::new();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let current = Vec3::new(900.0, 0.0, 100.0);
        let target = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(current));
        world.insert(
            entity,
            PatrolBehavior {
                patrol_radius: Some(1.0),
                form_id: 0x000E_0001,
            },
        );
        world.insert(
            entity,
            PatrolState {
                home: current,
                target,
                phase: WanderPhase::Walking,
                pick_count: 0,
            },
        );

        patrol_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

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
            "actor should step toward the shared-edge waypoint (500,0,500), got {moved_to:?}"
        );

        let nq = world.query::<NavPath>().expect("NavPath registered");
        assert_eq!(nq.get(entity).expect("cached this leg").goal, target);
    }
}
