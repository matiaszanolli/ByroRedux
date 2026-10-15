//! Patrol procedure (M42.8) — the sixth AI-package runtime, and the first
//! that runs no algorithm of its own: it calls
//! `wander_system`'s shared oscillating-walk core
//! (`super::wander::step_oscillating_wander`) directly. **Live by default
//! since M42.10** (kill-switch: `BYRO_NO_AI_LOCOMOTION=1`), alongside the
//! other five locomotion procedures.
//!
//! Real Bethesda Patrol packages walk a route defined by linked
//! patrol-idle markers — data this codebase decodes nowhere (it lives
//! outside `PACK`'s own sub-records; see `crates/plugin`'s
//! `PROCEDURE_PATROL` doc). Absent that, there is nothing to distinguish
//! v0 Patrol from Wander: both reduce to "walk to a random point within a
//! radius, pause, repeat" with no target-reference resolution. Rather than
//! duplicate `wander_system` under a new name (a straight copy-paste that
//! would drift the moment one side changes), Patrol runs the very same
//! generic system (`super::wander::oscillating_walk_system_inner`), differing
//! only in which component types it reads/writes ([`PatrolBehavior`]/
//! [`PatrolState`] instead of `WanderBehavior`/`WanderState`), supplied
//! through the `OscillatingWalkProcedure` impl below — kept separate so
//! Patrol and Wander actors stay independently selectable and inspectable,
//! matching every other M42 procedure having its own component pair.
//! (#5580 — until then only the inner step was shared and this file carried
//! a ~130-line copy of the gather / cache / write-back body.)
//!
//! If real patrol-route data is ever decoded, this is the seam to swap:
//! Patrol would drop the `OscillatingWalkProcedure` impl and gain its own
//! system here without touching `wander_system` at all.
//!
//! v0 scope: identical to `wander.rs`'s documented approximations,
//! including single-tile pathing (EX-16 item 3 Phase 4,
//! `docs/engine/navmesh-pathfinding.md`) — `patrol_system` resolves and
//! consumes its own `crate::components::NavPath` cache through the same
//! generic system `wander_system` runs, passing the result into the
//! `waypoint_override` parameter of the shared `step_oscillating_wander`
//! core; no target-reference resolution and no animation-clip swap. Package
//! selection is re-run once per in-game minute by
//! `ambient_ai_package_system`, not per frame — see `systems::travel`.

use super::wander::{
    oscillating_walk_system_inner, OscillateScratch, OscillateWalk, OscillatingWalkProcedure,
};
use byroredux_core::ecs::components::{PatrolBehavior, PatrolState};
use byroredux_core::ecs::World;

/// Fallback patrol radius (world units) around an actor's spawn position,
/// used when `PatrolBehavior.patrol_radius` is `None` (no PLDT / radius
/// 0). Same value as `wander.rs::WANDER_DEFAULT_RADIUS` — the two
/// procedures share the same v0 algorithm, so they share the same default.
/// The value itself is the one shared fact
/// `locomotion::AI_PACKAGE_DEFAULT_RADIUS` (#5587).
const PATROL_DEFAULT_RADIUS: f32 = super::locomotion::AI_PACKAGE_DEFAULT_RADIUS;

/// Patrol's component pair on the shared oscillating-walk system (#5580):
/// the gather, the step, the `NavPath` cache and the write-back are
/// `wander.rs`'s generic [`oscillating_walk_system_inner`]; only the
/// component types and the radius field are Patrol's own.
impl OscillatingWalkProcedure for PatrolBehavior {
    type State = PatrolState;
    const DEFAULT_RADIUS: f32 = PATROL_DEFAULT_RADIUS;
    fn authored_radius(&self) -> Option<f32> {
        self.patrol_radius
    }
    fn form_id(&self) -> u32 {
        self.form_id
    }
    fn walk_of(state: &PatrolState) -> OscillateWalk {
        OscillateWalk {
            home: state.home,
            target: state.target,
            phase: state.phase,
            pick_count: state.pick_count,
        }
    }
    fn state_of(walk: OscillateWalk) -> PatrolState {
        PatrolState {
            home: walk.home,
            target: walk.target,
            phase: walk.phase,
            pick_count: walk.pick_count,
        }
    }
}

/// Patrol system factory — returns a closure with a persistent
/// [`OscillateScratch`] (#2033 / PERF-D1-2026-07-16-01) driving the shared
/// oscillating-walk system for every [`PatrolBehavior`] actor. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`.
pub(crate) fn make_patrol_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = OscillateScratch::default();
    move |world: &World, dt: f32| {
        oscillating_walk_system_inner::<PatrolBehavior>(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_patrol_system`].
#[cfg(test)]
pub(crate) fn patrol_system(world: &World, dt: f32) {
    oscillating_walk_system_inner::<PatrolBehavior>(world, dt, &mut OscillateScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{NavPath, NavmeshTile};
    use crate::systems::navmesh_path::test_tiles::two_triangle_quad_scaled;
    use byroredux_core::ecs::components::{Transform, WanderPhase};
    use byroredux_core::math::Vec3;

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

    /// #3262 regression guard: all five component snapshots must be
    /// released before the real `PhysicsWorld` guard is acquired.
    #[test]
    fn patrol_system_moves_actor_with_a_real_physics_world_installed() {
        let mut world = World::new();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<Transform>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            PatrolBehavior {
                patrol_radius: Some(200.0),
                form_id: 0x000D_3262,
            },
        );

        patrol_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.length() > 0.0,
            "actor should still move with a real PhysicsWorld present"
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

    #[test]
    fn patrol_system_routes_through_a_resident_navmesh_tile_instead_of_straight_line() {
        // Pins that PatrolBehavior/PatrolState get the exact same
        // pathing treatment WanderBehavior/WanderState do via the shared
        // step_oscillating_wander core -- not a re-test of the algorithm
        // itself (navmesh_path's own suite already covers that), just
        // that the shared oscillating-walk system wires the override/cache
        // through correctly for Patrol's own component types.
        let mut world = World::new();
        world.register::<PatrolBehavior>();
        world.register::<PatrolState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_scaled(1000.0)));

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
