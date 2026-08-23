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
//! - **Single-tile pathing only** (EX-16 item 3 Phase 3,
//!   `docs/engine/navmesh-pathfinding.md`), not full obstacle avoidance.
//!   When the actor's current position localizes onto a resident
//!   `NavmeshTile`, `travel_system` routes through that tile's triangle
//!   corridor instead of walking straight through it; a destination on a
//!   *different* tile than the one the actor started on still falls back
//!   to a straight line for the cross-tile remainder (Phase 2 is
//!   genuinely blocked — see the design doc), and a cell with no
//!   navmesh at all behaves exactly as before. The computed path is
//!   cached in [`crate::components::NavPath`] and only recomputed if the
//!   resolved destination changes, not every tick.
//! - **No animation.** `AnimationPlayer` is untouched — Transform moves,
//!   pose doesn't (no verified walk `.kf` path exists in this codebase).
//! - **No per-frame package re-evaluation.** `TravelBehavior` is attached
//!   once at spawn; the same limitation `SandboxBehavior`/`WanderBehavior`
//!   have today.
//! - **Destination is frozen once resolved/picked.** If the resolved
//!   target entity moves after that first tick, Travel does not follow —
//!   that would be Follow's job (a different, unimplemented procedure).
//! - **Ground-snapped, not physically simulated**, same as Wander.

use super::locomotion::{step_along_waypoints, LOCOMOTION_ARRIVAL_EPSILON};
use super::navmesh_path::resolve_cached_waypoints;
use super::wander::pick_wander_target;
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{
    GlobalTransform, Transform, TravelBehavior, TravelState, Traveled,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use std::collections::VecDeque;

/// Fallback pick radius (world units) around an actor's spawn position,
/// used when `TravelBehavior.radius` is `None` (no PLDT / radius 0) *and*
/// the target didn't resolve. Same scale as `wander.rs::WANDER_DEFAULT_RADIUS`
/// — both are "how far does this ambient behavior reach without an
/// authored radius" defaults for the same class of content.
const TRAVEL_DEFAULT_RADIUS: f32 = 512.0;

/// Resolve a `NearReference`-type PLDT FormID to the live target entity's
/// current world position — `None` if there's no FormID, it doesn't
/// resolve to a spawned entity (the common case; only ~12% of
/// `NearReference` targets do, per the Sandbox 2026-07-14 investigation
/// this module's doc cites), or the entity has no `GlobalTransform`.
///
/// The shared first half of [`resolve_destination`] below (Travel/Escort's
/// random-pick fallback) and `guard_system::resolve_anchor` (Guard's
/// fall-back-to-`home` — see that module's doc for why the two fallbacks
/// differ on purpose). Pulled out so the two procedures can't drift on the
/// actual resolve step while still applying their own fallback. `pub(crate)`
/// for that second consumer. See #2561.
pub(crate) fn resolve_near_reference_target(
    world: &World,
    target_form_id: Option<u32>,
) -> Option<Vec3> {
    let fid = target_form_id?;
    let target_entity = resolve_entity_by_global_form_id(world, fid)?;
    world
        .get::<GlobalTransform>(target_entity)
        .map(|gt| gt.translation)
}

/// Resolve (or pick) a `NearReference`-anchored destination once, on first
/// sight: [`resolve_near_reference_target`] first, falling back to a
/// hash-picked point within `radius` of `home` (via `pick_wander_target`)
/// on any miss. Pulled out of `travel_system`'s Pass 1 loop body for
/// readability, and generic over primitive fields (rather than
/// `&TravelBehavior` directly) so `escort_system` can reuse it verbatim for
/// its lead-phase destination. `pub(crate)` for that consumer.
pub(crate) fn resolve_destination(
    world: &World,
    target_form_id: Option<u32>,
    radius: f32,
    form_id: u32,
    home: Vec3,
) -> Vec3 {
    resolve_near_reference_target(world, target_form_id)
        .unwrap_or_else(|| pick_wander_target(home, radius, form_id, 0))
}

/// One actor's staged move input, collected in Pass 1a — `current`
/// position, `rotation`, and the resolved `destination` (which may itself
/// have required a `GlobalTransform` read via `resolve_destination`) — all
/// gathered before `PhysicsWorld` is acquired in Pass 1b (#2134:
/// `PhysicsWorld` must never be live at the same time as a
/// `GlobalTransform` read).
struct TravelPending {
    entity: EntityId,
    current: Vec3,
    rotation: Quat,
    destination: Vec3,
    /// Remaining single-tile NAVM waypoints toward `destination` (EX-16
    /// item 3 Phase 3) — reused from a cached [`NavPath`] whose `goal`
    /// still matches `destination`, or freshly computed via
    /// [`path_from_resident_tiles`] otherwise. Empty means "no resident-
    /// tile path found," which falls back to walking straight at
    /// `destination`, unchanged from pre-pathing behavior.
    waypoints: VecDeque<Vec3>,
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
    /// The updated `NavPath` cache to write in Pass 2 — `Some` while
    /// still traveling with a non-empty resident-tile path remaining,
    /// `None` when there's nothing worth caching (arrived, or no path
    /// was ever found for this destination) and any stale entry for this
    /// entity should be removed instead.
    nav_path: Option<NavPath>,
}

/// Reusable per-frame scratch for [`travel_system_inner`] — captured by
/// [`make_travel_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct TravelScratch {
    pending: Vec<TravelPending>,
    decisions: Vec<TravelDecision>,
}

/// Drive straight-line walk-to-destination-once locomotion for every
/// [`TravelBehavior`] actor not yet [`Traveled`]. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`/`sandbox_seat_system` — NPC placement roots are
/// propagation roots (no `Parent`), so `Transform` == world position for
/// them.
fn travel_system_inner(world: &World, dt: f32, scratch: &mut TravelScratch) {
    let Some(behavior_q) = world.query::<TravelBehavior>() else {
        return;
    };

    // ── Pass 1a: resolve destinations (reads only; `resolve_destination`
    // may itself read `GlobalTransform`, so `PhysicsWorld` is NOT acquired
    // in this scope — #2134). ──
    scratch.pending.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let traveled_q = world.query::<Traveled>();
        let state_q = world.query::<TravelState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();

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
                None => resolve_destination(
                    world,
                    behavior.target_form_id,
                    behavior.radius.unwrap_or(TRAVEL_DEFAULT_RADIUS),
                    behavior.form_id,
                    current,
                ),
            };

            // EX-16 item 3 Phase 3: reuse the cached path if it was
            // computed for this same destination (the common case —
            // Travel's destination is frozen after the first tick, so
            // this fires exactly once per actor in practice); recompute
            // only when the goal changed or nothing is cached yet.
            // `0.0` threshold: a frozen goal, so any change at all means
            // "genuinely new destination," not "drifted a little."
            // Effective goal discarded (`_`): with a `0.0` threshold it's
            // always bit-identical to `destination`, whether reused or
            // freshly computed — see `resolve_cached_waypoints`'s own doc.
            let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
            let (_, waypoints) =
                resolve_cached_waypoints(cached, tile_q.as_ref(), current, destination, 0.0);

            scratch.pending.push(TravelPending {
                entity,
                current,
                rotation: transform.rotation,
                destination,
                waypoints,
            });
        }
    }
    if scratch.pending.is_empty() {
        scratch.decisions.clear();
        return;
    }

    // ── Pass 1b: compute movement via `step_toward`. `Transform` and
    // `GlobalTransform` locks from Pass 1a have already dropped, so
    // `PhysicsWorld` is acquired here without ever overlapping either
    // (#2134). ──
    scratch.decisions.clear();
    {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for p in &scratch.pending {
            // Step toward the next unconsumed waypoint (EX-16 item 3
            // Phase 3), falling back to the final destination directly
            // when no resident-tile path was found — unchanged straight-
            // line behavior in that case.
            let (new_pos, rotation, waypoints) = step_along_waypoints(
                p.current,
                p.rotation,
                p.waypoints.clone(),
                p.destination,
                dt,
                physics.as_deref(),
            );

            let horiz_delta = Vec3::new(
                new_pos.x - p.destination.x,
                0.0,
                new_pos.z - p.destination.z,
            );
            let arrived = horiz_delta.length_squared()
                <= LOCOMOTION_ARRIVAL_EPSILON * LOCOMOTION_ARRIVAL_EPSILON;

            scratch.decisions.push(TravelDecision {
                entity: p.entity,
                translation: new_pos,
                rotation,
                state: if arrived {
                    None
                } else {
                    Some(TravelState {
                        destination: p.destination,
                    })
                },
                // Cached even when `waypoints` is empty (no resident-tile
                // path was found): an empty-but-goal-matching `NavPath`
                // is a meaningful "already tried, nothing to find" result
                // — without caching it, an off-navmesh actor would retry
                // `path_from_resident_tiles` every single tick instead of
                // once, defeating the whole point of caching.
                nav_path: if arrived {
                    None
                } else {
                    Some(NavPath {
                        goal: p.destination,
                        waypoints,
                    })
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
    if let Some(mut sq) = world.query_mut::<TravelState>() {
        for d in &scratch.decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
    if let Some(mut tq) = world.query_mut::<Traveled>() {
        for d in &scratch.decisions {
            if d.state.is_none() {
                tq.insert(d.entity, Traveled);
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

/// Travel system factory — returns a closure with a persistent
/// [`TravelScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`travel_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_travel_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = TravelScratch::default();
    move |world: &World, dt: f32| {
        travel_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_travel_system`].
#[cfg(test)]
pub(crate) fn travel_system(world: &World, dt: f32) {
    travel_system_inner(world, dt, &mut TravelScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

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
        assert!(
            t.translation.length() > 0.0,
            "actor should have moved from the origin"
        );

        // Traveled before TravelState — matches `travel_system_inner`'s
        // acquisition order for this pair (#313).
        let travq = world.query::<Traveled>().expect("Traveled registered");
        let sq = world
            .query::<TravelState>()
            .expect("TravelState registered");
        assert!(
            sq.get(entity).is_some(),
            "travel_system must lazily insert TravelState on first tick"
        );

        assert!(
            travq.get(entity).is_none(),
            "should not have arrived in one 0.5s tick from a 200-unit radius pick"
        );
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

        let sq = world
            .query::<TravelState>()
            .expect("TravelState registered");
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
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)),
        );
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(100.0),
                target_form_id: None,
                form_id: 0x0005_0005,
            },
        );
        world.insert(
            entity,
            TravelState {
                destination: Vec3::new(50.0, 0.0, 50.0),
            },
        );

        travel_system(&world, 0.5);

        {
            let travq = world.query::<Traveled>().expect("Traveled registered");
            assert!(
                travq.get(entity).is_some(),
                "actor at its destination must be tagged Traveled"
            );
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
        assert_eq!(
            pos_after_first, pos_after_second,
            "Traveled actors must not move on later ticks"
        );
    }

    /// #2134 regression guard: with a *real* `PhysicsWorld` resource
    /// installed, `resolve_destination`'s `GlobalTransform` read (inside
    /// Pass 1a) must still resolve correctly before `PhysicsWorld` is
    /// acquired in Pass 1b — proving the split didn't change behavior,
    /// only lock-acquisition order.
    #[test]
    fn travel_system_resolves_target_with_a_real_physics_world_installed() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let mut pool = FormIdPool::new();
        let target_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x000E_0001),
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
                target_form_id: Some(0x000E_0001),
                form_id: 0x000E_0002,
            },
        );

        travel_system(&world, 0.001);

        let sq = world
            .query::<TravelState>()
            .expect("TravelState registered");
        assert_eq!(
            sq.get(actor).unwrap().destination,
            Vec3::new(300.0, 10.0, 400.0),
            "destination must still resolve to the target's live position with a real PhysicsWorld present"
        );
    }

    // ── EX-16 item 3 Phase 3: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos` (`(x, y, z) -> (x, -z, y)`), so a
    /// fixture's vertices can be authored in the Y-up coordinates the
    /// test reasons about while still exercising `parse_navm`'s real
    /// Z-up storage convention through `NavmRecord::vertices`.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// A 2-triangle quad spanning `[0,1000] x [0,1000]` on the XZ plane
    /// (Y-up), split along the `(0,0)-(1000,1000)` diagonal — the same
    /// shape `navmesh_path::tests::two_triangle_quad` pins at 1/100th
    /// scale, sized up here to match `LOCOMOTION_WALK_SPEED`'s real
    /// magnitude so a single tick doesn't overshoot the whole tile.
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
    fn travel_system_routes_through_a_resident_navmesh_tile_instead_of_straight_line() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        // start = (900, 0, 100): z=100 <= x=900, triangle 0.
        // destination = (600, 0, 900): z=900 >= x=600, triangle 1.
        // NOT a mirror pair across the diagonal, so a straight line
        // between them crosses the diagonal at (681.8, 681.8) -- a
        // clearly different point than the shared-edge midpoint (500,
        // 500) the pathed route must route through instead.
        let start = Vec3::new(900.0, 0.0, 100.0);
        let destination = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(start));
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(1.0), // irrelevant: TravelState below short-circuits resolution
                target_form_id: None,
                form_id: 0x0006_0006,
            },
        );
        world.insert(entity, TravelState { destination });

        travel_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        let expected_toward_waypoint = start.move_towards(Vec3::new(500.0, 0.0, 500.0), 100.0);
        let naive_straight_line = start.move_towards(destination, 100.0);
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
        let path = nq.get(entity).expect("a path must be cached after routing");
        assert_eq!(path.goal, destination);
        assert_eq!(
            path.waypoints.len(),
            2,
            "one intermediate waypoint (the diagonal midpoint) plus the goal itself must remain, got {:?}",
            path.waypoints
        );
        assert_eq!(path.waypoints[0], Vec3::new(500.0, 0.0, 500.0));
        assert_eq!(path.waypoints[1], destination);
    }

    #[test]
    fn travel_system_falls_back_to_straight_line_with_an_empty_resident_tile_set() {
        // NavmeshTile/NavPath are *registered* (unlike every other test in
        // this module) but zero tiles are ever spawned -- a distinct code
        // path from "the types were never touched at all" (which the rest
        // of this module's tests already cover via `world.query` returning
        // `None`), pinning that an empty-but-`Some` tile query degrades
        // identically to straight-line behavior.
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(200.0),
                target_form_id: None,
                form_id: 0x0007_0007,
            },
        );

        travel_system(&world, 0.5);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity)
                .expect("actor transform")
                .translation
                .length()
                > 0.0,
            "actor should still move via the straight-line fallback with no resident tiles"
        );
    }

    #[test]
    fn travel_system_reuses_the_cached_navpath_on_the_next_tick_without_recomputing() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let start = Vec3::new(900.0, 0.0, 100.0);
        let destination = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(start));
        world.insert(
            entity,
            TravelBehavior {
                radius: Some(1.0),
                target_form_id: None,
                form_id: 0x0008_0008,
            },
        );
        world.insert(entity, TravelState { destination });

        travel_system(&world, 1.0);
        let path_after_first = {
            let nq = world.query::<NavPath>().expect("NavPath registered");
            nq.get(entity).expect("cached after first tick").clone()
        };
        assert_eq!(path_after_first.goal, destination);

        // Second tick: same destination (TravelState unchanged) -- the
        // cache must be reused (same `goal`), and the walk continues
        // consuming the *same* waypoint chain rather than jumping back to
        // a fresh A* result computed from the new (moved) position, which
        // would still geometrically start with the same waypoint here but
        // would defeat the "computed once" cost posture this is pinning.
        travel_system(&world, 1.0);
        let path_after_second = {
            let nq = world.query::<NavPath>().expect("NavPath registered");
            nq.get(entity).expect("still traveling").clone()
        };
        assert_eq!(
            path_after_second.goal, destination,
            "goal must stay pinned across ticks while TravelState.destination is unchanged"
        );
        assert!(
            path_after_second.waypoints.len() <= path_after_first.waypoints.len(),
            "waypoints must only be consumed (popped), never grow, across ticks"
        );
    }
}
