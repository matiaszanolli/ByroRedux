//! Guard procedure (M42.7) — the fifth AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`/`travel_system`/`follow_system`/
//! `escort_system`'s straight-line walk primitive via
//! `super::locomotion::step_toward`. **Registered only when `BYRO_GUARD`
//! is set** (see `boot.rs`), mirroring
//! `BYRO_ESCORT`/`BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`.
//!
//! Guard needs only `PLDT` — no new sub-record decode work. Anchor
//! resolution tries a `NearReference`-type FormID first (via
//! `resolve_entity_by_global_form_id`, same as Travel/Escort), but its
//! **fallback differs from Travel's on purpose**: Travel picks a random
//! point within radius because its whole job is "go explore somewhere";
//! Guard's job is "hold a post", so a package with no resolvable anchor
//! falls back to the actor's own spawn position — the same "center is the
//! actor's own position" convention `SandboxBehavior`/`WanderBehavior` use
//! for their search/wander center, not Travel's "wander to a random point"
//! one. (Reusing Travel's random-pick fallback here was tried first and
//! reverted — it hands the actor a fresh point exactly `radius` away from
//! its own spawn, which trivially satisfies the leash check below and
//! results in an actor that never walks anywhere, having "arrived" by
//! construction.)
//!
//! What makes Guard different from Travel isn't just the fallback,
//! though — it's what happens after arrival: Travel reaches a terminal
//! state and stops forever ([`super::travel::Traveled`]); Guard never
//! does. Once the anchor is resolved, `guard_system` checks every tick
//! whether the actor has drifted more than [`GuardBehavior::radius`]
//! away, and walks back if so — an indefinite, non-terminal shape closer
//! to `wander_system`'s (just triggered by displacement instead of a
//! pause timer).
//!
//! ## No known displacement trigger yet
//!
//! Nothing in the engine currently pushes an NPC actor away from its
//! Transform once placed — there's no combat, no player shove, no
//! ragdoll-to-Transform writeback for guarding actors. So in practice, v0
//! Guard behaves observably like a non-terminal Travel: walk to the
//! anchor once, then hold. The per-tick leash check is real and tested,
//! not a stub — it's just dormant until a future system can actually
//! displace a guarding actor.
//!
//! ## v0 scope (mirrors `wander.rs`/`travel.rs`'s documented approximations)
//!
//! - **Single-tile pathing only** (EX-16 item 3 Phase 4,
//!   `docs/engine/navmesh-pathfinding.md`), reusing `travel_system`'s
//!   `crate::components::NavPath` caching shape exactly (frozen goal =
//!   `GuardState::anchor`, `0.0` repath threshold — recompute only when
//!   the anchor itself changes, which per this module's v0 scope is
//!   never, after the first resolve). **One caveat this cache doesn't
//!   yet handle**: it trusts that `current` only ever changes smoothly,
//!   via this system's own `step_toward` calls — true for every reachable
//!   path today (see "No known displacement trigger yet" above), but a
//!   *teleporting* future shove/knockback system would leave the cached
//!   path computed from the actor's pre-displacement position, not the
//!   post-displacement one the leash check reacts to. Not a guessed fix:
//!   flagged for whoever builds that system to revisit, same "dormant
//!   until reachable" posture the leash check itself already has.
//! - **No animation.** `AnimationPlayer` is untouched.
//! - **No per-frame package re-evaluation.** Same limitation as
//!   Sandbox/Wander/Travel/Follow/Escort.
//! - **Anchor is frozen once resolved/picked**, exactly like
//!   `TravelState::destination` — a `NearReference` anchor that moves
//!   after resolution isn't re-tracked (unlike Follow's live target).
//! - **Ground-snapped, not physically simulated**, same as Wander/Travel.

use super::locomotion::step_along_waypoints;
use super::navmesh_path::resolve_cached_waypoints;
use crate::components::{NavPath, NavmeshTile};
use byroredux_core::ecs::components::{GuardBehavior, GuardState, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use std::collections::VecDeque;

/// Fallback leash tolerance (world units) around a guard's anchor, used
/// when `GuardBehavior.radius` is `None` (no PLDT / radius 0). Same scale
/// as `travel.rs::TRAVEL_DEFAULT_RADIUS`/`wander.rs::WANDER_DEFAULT_RADIUS`.
const GUARD_DEFAULT_RADIUS: f32 = 512.0;

/// Resolve this actor's guard anchor once, on first sight: delegates the
/// `NearReference`-type PLDT FormID resolve to
/// `travel_system::resolve_near_reference_target` (shared with
/// Travel/Escort — #2561, this module previously duplicated that logic
/// inline), falling back to `home` — the actor's own spawn position — on
/// any miss. See this module's doc for why the fallback is `home` rather
/// than Travel's random-pick-within-radius.
fn resolve_anchor(world: &World, behavior: &GuardBehavior, home: Vec3) -> Vec3 {
    super::travel::resolve_near_reference_target(world, behavior.anchor_form_id).unwrap_or(home)
}

/// One actor's staged move input, collected in Pass 1a — `resolve_anchor`
/// reads `GlobalTransform`, so this is gathered before `PhysicsWorld` is
/// acquired in Pass 1b (#2134: `PhysicsWorld` must never be live at the
/// same time as a `GlobalTransform` read).
struct GuardPending {
    entity: EntityId,
    current: Vec3,
    rotation: Quat,
    /// `Some(anchor)` when beyond the leash and a walk-back is needed
    /// this tick; carries the anchor itself (not yet an XZ target) since
    /// [`resolve_cached_waypoints`] needs it as the goal to resolve
    /// against.
    walk_back_to: Option<Vec3>,
    /// Resident-tile waypoints toward `walk_back_to`'s anchor (EX-16 item
    /// 3 Phase 4) — empty when `walk_back_to` is `None` (nothing to walk
    /// toward this tick) or when no resident-tile path was found.
    waypoints: VecDeque<Vec3>,
    state: Option<GuardState>,
}

/// One actor's computed movement/state update for this tick, applied in
/// Pass 2 after all Pass-1 reads have dropped (mirrors
/// `travel_system`/`follow_system`'s two-pass structure).
struct GuardDecision {
    entity: EntityId,
    translation: Vec3,
    rotation: Option<Quat>,
    /// `Some` only when `GuardState` needs to change this tick (first
    /// sight — the anchor doesn't change afterward, so later ticks never
    /// rewrite it).
    state: Option<GuardState>,
    /// The updated `NavPath` cache to write in Pass 2 — mirrors
    /// `TravelDecision::nav_path`'s exact contract (`None` removes any
    /// stale entry, `Some` caches even an empty "no path found" result).
    nav_path: Option<NavPath>,
}

/// Reusable per-frame scratch for [`guard_system_inner`] — captured by
/// [`make_guard_system`] so the `decisions` backing allocation survives
/// across frames instead of being re-declared `Vec::new()` every tick
/// (#2033 / PERF-D1-2026-07-16-01), mirroring `animation_system`'s
/// `AnimScratch` (#1372).
#[derive(Default)]
struct GuardScratch {
    pending: Vec<GuardPending>,
    decisions: Vec<GuardDecision>,
}

/// Drive anchor-hold-and-return locomotion for every [`GuardBehavior`]
/// actor. Registered `add_exclusive(Stage::PostUpdate, …)`, same
/// stage/slot family as `wander_system`/`travel_system` — NPC placement
/// roots are propagation roots (no `Parent`), so `Transform` == world
/// position for them.
fn guard_system_inner(world: &World, dt: f32, scratch: &mut GuardScratch) {
    let Some(behavior_q) = world.query::<GuardBehavior>() else {
        return;
    };

    // ── Pass 1a: resolve anchors (reads only; `resolve_anchor` may itself
    // read `GlobalTransform`, so `PhysicsWorld` is NOT acquired in this
    // scope — #2134). ──
    scratch.pending.clear();
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let state_q = world.query::<GuardState>();
        let tile_q = world.query::<NavmeshTile>();
        let nav_path_q = world.query::<NavPath>();

        for (entity, behavior) in behavior_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let current = transform.translation;
            let existing_state = state_q.as_ref().and_then(|q| q.get(entity)).copied();

            let (anchor, state) = match existing_state {
                Some(s) => (s.anchor, None),
                None => {
                    let anchor = resolve_anchor(world, behavior, current);
                    (anchor, Some(GuardState { anchor }))
                }
            };

            let leash = behavior.radius.unwrap_or(GUARD_DEFAULT_RADIUS);
            let horiz_delta = Vec3::new(anchor.x - current.x, 0.0, anchor.z - current.z);
            let walk_back_to = (horiz_delta.length() > leash).then_some(anchor);

            // EX-16 item 3 Phase 4: only bother resolving a path when
            // actually walking back this tick — mirrors `wander_system`'s
            // "skip the resident-tile scan while Paused" optimization.
            // Effective goal discarded (`_`): with a `0.0` threshold it's
            // always bit-identical to `goal` (the anchor), whether reused
            // or freshly computed — see `resolve_cached_waypoints`'s doc.
            let waypoints = match walk_back_to {
                Some(goal) => {
                    let cached = nav_path_q.as_ref().and_then(|q| q.get(entity));
                    let (_, waypoints) =
                        resolve_cached_waypoints(cached, tile_q.as_ref(), current, goal, 0.0);
                    waypoints
                }
                None => VecDeque::new(),
            };

            scratch.pending.push(GuardPending {
                entity,
                current,
                rotation: transform.rotation,
                walk_back_to,
                waypoints,
                state,
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
        // #3269 — consume `pending` by value: `step_along_waypoints` takes
        // ownership of the `VecDeque`, so iterating by reference forced a
        // clone of the whole waypoint list per entity per tick purely to
        // satisfy the borrow checker. Nothing reads `pending` after this
        // pass, and `drain` keeps the allocation for next frame, so the
        // scratch buffer's whole purpose is preserved.
        for p in scratch.pending.drain(..) {
            let (translation, rotation, nav_path) = match p.walk_back_to {
                Some(anchor) => {
                    let (new_pos, rotation, waypoints) = step_along_waypoints(
                        p.current,
                        p.rotation,
                        p.waypoints,
                        anchor,
                        dt,
                        physics.as_deref(),
                    );
                    (
                        new_pos,
                        rotation,
                        Some(NavPath {
                            goal: anchor,
                            waypoints,
                        }),
                    )
                }
                // Within the leash — hold position exactly like today,
                // and drop any stale cached path (nothing to walk toward
                // this tick; a fresh one is resolved the next time the
                // actor drifts beyond the leash).
                None => (p.current, None, None),
            };

            scratch.decisions.push(GuardDecision {
                entity: p.entity,
                translation,
                rotation,
                state: p.state,
                nav_path,
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
    if let Some(mut sq) = world.query_mut::<GuardState>() {
        for d in &scratch.decisions {
            if let Some(state) = d.state {
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

/// Guard system factory — returns a closure with a persistent
/// [`GuardScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`guard_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_guard_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = GuardScratch::default();
    move |world: &World, dt: f32| {
        guard_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics — tests call this directly and don't need
/// persistent scratch. Production code uses [`make_guard_system`].
#[cfg(test)]
pub(crate) fn guard_system(world: &World, dt: f32) {
    guard_system_inner(world, dt, &mut GuardScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn guard_system_holds_home_position_with_no_target_and_no_physics() {
        // No `anchor_form_id` to resolve — v0 falls back to the actor's
        // own spawn position (not a random pick, see this module's doc),
        // so there's nowhere to walk and the actor must not move.
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            GuardBehavior {
                anchor_form_id: None,
                radius: Some(200.0),
                form_id: 0x000A_0001,
            },
        );

        guard_system(&world, 0.5);

        // Transform before GuardState — matches `guard_system_inner`'s
        // acquisition order for this pair (#313).
        let tq = world.query::<Transform>().expect("Transform registered");
        let sq = world.query::<GuardState>().expect("GuardState registered");
        let state = sq
            .get(entity)
            .expect("guard_system must lazily insert GuardState on first tick");
        assert_eq!(
            state.anchor,
            Vec3::ZERO,
            "fallback anchor must be the actor's own spawn position"
        );

        assert_eq!(
            tq.get(entity).unwrap().translation,
            Vec3::ZERO,
            "already at the fallback anchor — must not move"
        );
    }

    #[test]
    fn guard_system_resolves_anchor_form_id_to_live_entity_position() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();

        let mut pool = FormIdPool::new();
        let anchor_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x000B_0001),
        });
        world.insert_resource(pool);

        let anchor = world.spawn();
        world.insert(anchor, FormIdComponent(anchor_fid));
        world.insert(
            anchor,
            GlobalTransform::new(Vec3::new(300.0, 10.0, 400.0), Quat::IDENTITY, 1.0),
        );

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            GuardBehavior {
                anchor_form_id: Some(0x000B_0001),
                radius: None,
                form_id: 0x000B_0002,
            },
        );

        guard_system(&world, 0.001);

        let sq = world.query::<GuardState>().expect("GuardState registered");
        assert_eq!(
            sq.get(actor).unwrap().anchor,
            Vec3::new(300.0, 10.0, 400.0),
            "anchor must be the resolved target's live position, not a hash-picked fallback"
        );
    }

    #[test]
    fn guard_system_holds_position_once_within_leash_and_returns_when_displaced() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();

        let entity = world.spawn();
        // Seed the actor already at its anchor, well inside the leash.
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)),
        );
        world.insert(
            entity,
            GuardBehavior {
                anchor_form_id: None,
                radius: Some(20.0),
                form_id: 0x000C_0001,
            },
        );
        world.insert(
            entity,
            GuardState {
                anchor: Vec3::new(50.0, 0.0, 50.0),
            },
        );

        guard_system(&world, 0.5);
        {
            let tq = world.query::<Transform>().expect("Transform registered");
            assert_eq!(
                tq.get(entity).unwrap().translation,
                Vec3::new(50.0, 0.0, 50.0),
                "within the leash — must hold position, not wander off"
            );
        }

        // Displace the actor beyond the leash (simulating a future
        // shove/knockback system) and confirm the next tick walks back.
        if let Some(mut tq) = world.query_mut::<Transform>() {
            tq.get_mut(entity).unwrap().translation = Vec3::new(500.0, 0.0, 50.0);
        }
        guard_system(&world, 0.5);
        let tq = world.query::<Transform>().expect("Transform registered");
        assert!(
            tq.get(entity).unwrap().translation.x < 500.0,
            "beyond the leash — must step back toward the anchor"
        );
    }

    /// #2134 regression guard: with a *real* `PhysicsWorld` resource
    /// installed, `resolve_anchor`'s `GlobalTransform` read (Pass 1a)
    /// must still resolve correctly before `PhysicsWorld` is acquired in
    /// Pass 1b — proving the split didn't change behavior, only
    /// lock-acquisition order.
    #[test]
    fn guard_system_resolves_anchor_with_a_real_physics_world_installed() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());

        let mut pool = FormIdPool::new();
        let anchor_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0010_0001),
        });
        world.insert_resource(pool);

        let anchor = world.spawn();
        world.insert(anchor, FormIdComponent(anchor_fid));
        world.insert(
            anchor,
            GlobalTransform::new(Vec3::new(300.0, 10.0, 400.0), Quat::IDENTITY, 1.0),
        );

        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            GuardBehavior {
                anchor_form_id: Some(0x0010_0001),
                radius: None,
                form_id: 0x0010_0002,
            },
        );

        guard_system(&world, 0.001);

        let sq = world.query::<GuardState>().expect("GuardState registered");
        assert_eq!(
            sq.get(actor).unwrap().anchor,
            Vec3::new(300.0, 10.0, 400.0),
            "anchor must still resolve to the target's live position with a real PhysicsWorld present"
        );
    }

    // ── EX-16 item 3 Phase 4: single-tile NAVM pathing integration ──

    /// The inverse of `zup_to_yup_pos`, mirroring
    /// `travel::tests::zup_from_yup` — each module owns its own test
    /// fixtures per this codebase's established convention.
    fn zup_from_yup(p: [f32; 3]) -> [f32; 3] {
        [p[0], -p[2], p[1]]
    }

    /// The same 1000-unit two-triangle quad `travel::tests::
    /// two_triangle_quad_navm` pins, duplicated here rather than shared
    /// across modules (each locomotion system owns its own test
    /// fixtures, matching the rest of this file's conventions).
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
    fn guard_system_routes_through_a_resident_navmesh_tile_when_walking_back() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let tile = world.spawn();
        world.insert(tile, NavmeshTile(two_triangle_quad_navm()));

        // Same non-mirror-symmetric pair travel_system's equivalent test
        // uses: a straight line between them crosses the diagonal at
        // (681.8, 681.8), a clearly different point than the shared-edge
        // midpoint (500, 500) the pathed route must go through instead.
        let current = Vec3::new(900.0, 0.0, 100.0);
        let anchor = Vec3::new(600.0, 0.0, 900.0);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(current));
        world.insert(
            entity,
            GuardBehavior {
                anchor_form_id: None,
                radius: Some(1.0), // small leash: the anchor is far beyond it
                form_id: 0x0011_0001,
            },
        );
        world.insert(entity, GuardState { anchor });

        guard_system(&world, 1.0); // 1s @ 100 u/s = 100 units of travel

        let expected_toward_waypoint = current.move_towards(Vec3::new(500.0, 0.0, 500.0), 100.0);
        let naive_straight_line = current.move_towards(anchor, 100.0);
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
        let path = nq
            .get(entity)
            .expect("a path must be cached while walking back");
        assert_eq!(path.goal, anchor);
        assert_eq!(
            path.waypoints.len(),
            2,
            "midpoint waypoint plus the anchor itself, got {:?}",
            path.waypoints
        );
    }

    #[test]
    fn guard_system_drops_any_cached_path_once_back_within_the_leash() {
        let mut world = World::new();
        world.register::<GuardBehavior>();
        world.register::<GuardState>();
        world.register::<Transform>();
        world.register::<NavmeshTile>();
        world.register::<NavPath>();

        let entity = world.spawn();
        world.insert(
            entity,
            Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)),
        );
        world.insert(
            entity,
            GuardBehavior {
                anchor_form_id: None,
                radius: Some(20.0),
                form_id: 0x0012_0001,
            },
        );
        world.insert(
            entity,
            GuardState {
                anchor: Vec3::new(50.0, 0.0, 50.0),
            },
        );
        // Seed a stale cached path as if the actor had been walking back
        // moments ago — must be dropped once within the leash again, not
        // left to accumulate forever.
        world.insert(
            entity,
            NavPath {
                goal: Vec3::new(50.0, 0.0, 50.0),
                waypoints: std::collections::VecDeque::from(vec![Vec3::new(50.0, 0.0, 50.0)]),
            },
        );

        guard_system(&world, 0.5);

        let nq = world.query::<NavPath>().expect("NavPath registered");
        assert!(
            nq.get(entity).is_none(),
            "within the leash — any stale cached path must be cleared, nothing to walk toward"
        );
    }
}
