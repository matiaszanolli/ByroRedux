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
//! - **Single-tile pathing only** (EX-16 item 3 Phase 4,
//!   `docs/engine/navmesh-pathfinding.md`), with a repath threshold
//!   instead of `travel_system`'s exact-match reuse: the target's live
//!   position changes every tick, so requiring an *exact* match would
//!   never reuse a cached path at all, and recomputing full single-tile
//!   A\* every tick against a moving goal would be needlessly expensive
//!   for a target that's barely moved (design doc §6). The cached path
//!   is kept as long as the live target hasn't moved more than
//!   [`FOLLOW_REPATH_THRESHOLD`] from the goal it was last computed
//!   against; a value flagged in the design doc's own §10 as a tunable
//!   engine default to land empirically, not derived from content — same
//!   posture as `LOCOMOTION_WALK_SPEED`.
//! - **No animation.** `AnimationPlayer` is untouched.
//! - **No per-frame package re-evaluation.** Same limitation as
//!   Sandbox/Wander/Travel.
//! - **Target-entity resolution happens once.** If the target isn't
//!   spawned yet on this actor's first tick, or later despawns, Follow
//!   does not re-resolve — the actor stands still from that point on.
//! - **No distance/relationship reasoning beyond stand-off.** Real Follow
//!   packages also gate on combat state, dialogue, etc. — none of that
//!   exists here; this is straight-line stand-off tracking only.

use super::locomotion::{step_along_waypoints, LOCOMOTION_ARRIVAL_EPSILON};
use super::navmesh_path::resolve_cached_waypoints;
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{FollowBehavior, FollowState, GlobalTransform, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use std::collections::VecDeque;

/// Fallback stand-off distance (world units), used when
/// `FollowBehavior.follow_distance` is `None` (no PTDT / distance 0).
/// Engine default — no authored equivalent enforced beyond PTDT's own
/// `count_or_distance`, same convention as `WANDER_DEFAULT_RADIUS`.
const FOLLOW_DEFAULT_DISTANCE: f32 = 128.0;

/// How far (world units) the live target may move from the goal a
/// cached `NavPath` was last resolved against before `follow_system`
/// recomputes it. See this module's doc for why this differs from
/// `travel_system`'s exact-match reuse. Engine default, not derived from
/// content — chosen as half of `FOLLOW_DEFAULT_DISTANCE`, the scale this
/// system already reasons in.
const FOLLOW_REPATH_THRESHOLD: f32 = 64.0;

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
    /// Staged move inputs (current position, rotation, target XZ)
    /// collected in Pass 1a while `Transform`/`GlobalTransform` are held
    /// but `PhysicsWorld` is NOT — filled into `movement` by Pass 1b,
    /// which acquires `PhysicsWorld` only after those locks have
    /// dropped (#2134: `PhysicsWorld` must never be live at the same
    /// time as a `GlobalTransform` read, matching `sync.rs::push_kinematic`'s
    /// GlobalTransform-before-PhysicsWorld order).
    pending_move: Option<(Vec3, Quat, Vec3)>,
    /// Resident-tile waypoints toward `pending_move`'s target XZ (EX-16
    /// item 3 Phase 4) — only meaningful when `pending_move` is `Some`;
    /// empty otherwise (nothing to walk toward this tick).
    waypoints: VecDeque<Vec3>,
    /// The goal to persist into `NavPath` alongside `waypoints` —
    /// `resolve_cached_waypoints`'s returned *effective* goal, not this
    /// tick's raw live target position; see that function's doc for why
    /// those two must not be conflated for a live-goal (repath-threshold)
    /// caller like this one.
    effective_goal: Vec3,
    /// `None` when the actor didn't move this tick (already within
    /// stand-off distance, or has no resolvable target).
    movement: Option<(Vec3, Option<Quat>)>,
    /// The updated `NavPath` cache to write in Pass 2 — mirrors
    /// `TravelDecision::nav_path`'s contract (`None` removes any stale
    /// entry, `Some` caches even an empty "no path found" result).
    nav_path: Option<NavPath>,
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

    // ── Pass 1a: resolve targets + stage move inputs. `Transform` and
    // `GlobalTransform` are read here; `PhysicsWorld` is NOT acquired in
    // this scope (#2134). ──
    scratch.decisions.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<FollowState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let existing_state = state_q.as_ref().and_then(|q| q.get(entity)).copied();
            let (target_entity, state_to_insert) = match existing_state {
                Some(s) => (s.target_entity, None),
                None => {
                    let resolved = resolve_follow_target(world, behavior);
                    (
                        resolved,
                        Some(FollowState {
                            target_entity: resolved,
                        }),
                    )
                }
            };

            let Some(target_entity) = target_entity else {
                scratch.decisions.push(FollowDecision {
                    entity,
                    pending_move: None,
                    waypoints: VecDeque::new(),
                    effective_goal: Vec3::ZERO,
                    movement: None,
                    nav_path: None,
                    state_to_insert,
                });
                continue;
            };
            let Some(target_gt) = world.get::<GlobalTransform>(target_entity) else {
                // Target despawned since resolution — stand still this
                // tick rather than re-resolving (v0 discipline).
                scratch.decisions.push(FollowDecision {
                    entity,
                    pending_move: None,
                    waypoints: VecDeque::new(),
                    effective_goal: Vec3::ZERO,
                    movement: None,
                    nav_path: None,
                    state_to_insert,
                });
                continue;
            };

            let current = transform.translation;
            let distance = behavior.follow_distance.unwrap_or(FOLLOW_DEFAULT_DISTANCE);
            let target_xz = Vec3::new(target_gt.translation.x, current.y, target_gt.translation.z);
            let horiz_delta = Vec3::new(target_xz.x - current.x, 0.0, target_xz.z - current.z);

            let pending_move = if horiz_delta.length() > distance + LOCOMOTION_ARRIVAL_EPSILON {
                Some((current, transform.rotation, target_xz))
            } else {
                None
            };

            // EX-16 item 3 Phase 4: only bother resolving a path when
            // actually moving this tick — mirrors `guard_system`'s same
            // "skip the resident-tile scan when not walking" shape.
            let (effective_goal, waypoints) = if pending_move.is_some() {
                let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                resolve_cached_waypoints(
                    cached,
                    tile_q.as_ref(),
                    current,
                    target_xz,
                    FOLLOW_REPATH_THRESHOLD,
                )
            } else {
                (Vec3::ZERO, VecDeque::new())
            };

            scratch.decisions.push(FollowDecision {
                entity,
                pending_move,
                waypoints,
                effective_goal,
                movement: None,
                nav_path: None,
                state_to_insert,
            });
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 1b: compute movement via `step_along_waypoints`. `Transform`
    // and `GlobalTransform` locks from Pass 1a have already dropped, so
    // `PhysicsWorld` is acquired here without ever overlapping either
    // (#2134). ──
    {
        let physics = world.try_resource::<byroredux_physics::PhysicsWorld>();
        for d in &mut scratch.decisions {
            if let Some((current, rotation, target_xz)) = d.pending_move {
                let (new_pos, new_rotation, waypoints) = step_along_waypoints(
                    current,
                    rotation,
                    d.waypoints.clone(),
                    target_xz,
                    dt,
                    physics.as_deref(),
                );
                d.movement = Some((new_pos, new_rotation));
                // `effective_goal`, NOT `target_xz` — see FollowDecision's
                // own doc and `resolve_cached_waypoints`'s for why using
                // the raw per-tick live position here would silently
                // defeat the repath threshold.
                d.nav_path = Some(NavPath {
                    goal: d.effective_goal,
                    waypoints,
                });
            }
            // `None` when not moving this tick — drops any stale cached
            // path (nothing to walk toward, mirrors `guard_system`'s
            // within-leash clearing) rather than leaving it to linger.
        }
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
        assert_eq!(
            tq.get(entity).unwrap().translation,
            Vec3::ZERO,
            "no target — actor must not move"
        );

        let sq = world
            .query::<FollowState>()
            .expect("FollowState registered");
        assert_eq!(
            sq.get(entity).copied(),
            Some(FollowState {
                target_entity: None
            }),
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

        // Transform before FollowState — matches `follow_system_inner`'s
        // acquisition order for this pair (#313).
        let tq = world.query::<Transform>().expect("Transform registered");
        let sq = world
            .query::<FollowState>()
            .expect("FollowState registered");
        assert!(
            sq.get(actor).unwrap().target_entity.is_some(),
            "target must resolve on first tick"
        );

        let pos = tq.get(actor).unwrap().translation;
        assert!(
            pos.x > 0.0,
            "actor should have started closing toward the target"
        );
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

    /// #2134 regression guard: with a *real* `PhysicsWorld` resource
    /// installed (not `try_resource` returning `None`), the resolved
    /// target's `GlobalTransform` must still be readable and movement
    /// still computed correctly — proving the Pass 1a/1b split (target
    /// resolution before `PhysicsWorld` is acquired) didn't change
    /// behavior, only lock-acquisition order.
    #[test]
    fn follow_system_resolves_target_with_a_real_physics_world_installed() {
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        spawn_target(&mut world, 0x000D_0001, Vec3::new(1000.0, 10.0, 0.0));

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x000D_0001),
                follow_distance: Some(50.0),
            },
        );

        follow_system(&world, 0.1);

        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(actor).unwrap().translation.x > 0.0,
            "actor should still close toward the target with a real PhysicsWorld present"
        );
    }

    // ── EX-16 item 3 Phase 4: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos`, mirroring `travel::tests::
    /// zup_from_yup`/`guard::tests::zup_from_yup` — each module owns its
    /// own test fixtures per this codebase's established convention.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// The same 1000-unit two-triangle quad `travel::tests::
    /// two_triangle_quad_navm`/`guard::tests::two_triangle_quad_navm` pin.
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
    fn follow_system_routes_through_a_resident_navmesh_tile_instead_of_straight_line() {
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        // Same non-mirror-symmetric pair travel_system's/guard_system's
        // equivalent tests use.
        let current = Vec3::new(900.0, 0.0, 100.0);
        let target_pos = Vec3::new(600.0, 0.0, 900.0);
        spawn_target(&mut world, 0x0009_0001, target_pos);

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(current));
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x0009_0001),
                follow_distance: Some(1.0), // small stand-off: well within range to keep moving
            },
        );

        follow_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        let expected_toward_waypoint = current.move_towards(Vec3::new(500.0, 0.0, 500.0), 100.0);
        let naive_straight_line = current.move_towards(target_pos, 100.0);
        assert!(
            (expected_toward_waypoint - naive_straight_line).length() > 10.0,
            "test fixture must be non-degenerate: pathed and naive directions should differ"
        );

        let tq = world.query::<Transform>().expect("Transform registered");
        let moved_to = tq.get(actor).expect("actor transform").translation;
        assert!(
            (moved_to - expected_toward_waypoint).length() < 1e-3,
            "actor should step toward the shared-edge waypoint (500,0,500), got {moved_to:?}, \
             expected ~{expected_toward_waypoint:?} (naive straight-line would have been {naive_straight_line:?})"
        );
    }

    #[test]
    fn follow_system_reuses_the_cached_path_when_the_target_barely_moves() {
        let mut world = World::new();
        world.register::<FollowBehavior>();
        world.register::<FollowState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        let target = spawn_target(&mut world, 0x000A_0001, Vec3::new(600.0, 0.0, 900.0));

        let actor = world.spawn();
        world.insert(
            actor,
            Transform::from_translation(Vec3::new(900.0, 0.0, 100.0)),
        );
        world.insert(
            actor,
            FollowBehavior {
                target_form_id: Some(0x000A_0001),
                follow_distance: Some(1.0),
            },
        );

        follow_system(&world, 0.1);
        let goal_after_first = {
            let nq = world.query::<NavPath>().expect("NavPath registered");
            nq.get(actor).expect("cached after first tick").goal
        };

        // Move the target by less than FOLLOW_REPATH_THRESHOLD -- the
        // cached path must be reused (same `goal`), not recomputed
        // against the barely-moved position.
        if let Some(mut gq) = world.query_mut::<GlobalTransform>() {
            gq.get_mut(target).unwrap().translation += Vec3::new(5.0, 0.0, 0.0);
        }
        follow_system(&world, 0.1);
        let goal_after_second = {
            let nq = world.query::<NavPath>().expect("NavPath registered");
            nq.get(actor).expect("still tracking").goal
        };
        assert_eq!(
            goal_after_first, goal_after_second,
            "a sub-threshold target move must reuse the cached path's goal, not recompute"
        );
    }
}
