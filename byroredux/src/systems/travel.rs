//! Travel procedure (M42.4) — the second AI-package runtime to drive NPC
//! locomotion, reusing `wander_system`'s straight-line walk primitive via
//! `super::locomotion::step_toward`. **Live by default since M42.10**
//! (kill-switch: `BYRO_NO_AI_LOCOMOTION=1`; see
//! `boot/schedule/post_update.rs`).
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
//! - With no target FormID, or after the retry window for an unresolved
//!   target: fall back to `super::wander::pick_wander_target` (reused directly, not
//!   duplicated) — a hash-picked point within `radius` of the actor's own
//!   spawn position, the same v0 approximation Wander uses.
//!
//! A missing authored target is retried for [`ANCHOR_RETRY_SECS`] before
//! Travel falls back to its hash-picked point. The scheduler can run between
//! budgeted cell-load slices, so a target whose REFR sorts after the actor
//! may become live during that window. Targets that remain unavailable
//! (including off-cell and unspawned XMarker references) still use the
//! documented fallback.
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
//! - **Package re-evaluation is per in-game minute, not per frame.**
//!   `ambient_ai_package_system` (M42.9 / #2652) re-runs selection once per
//!   in-game minute per actor, and on an explicit `EvaluatePackageRequest`,
//!   swapping the behavior component when the winning PACK FormID changes —
//!   same cadence for every procedure in this directory.
//! - **Destination is frozen once resolved/picked.** If the resolved
//!   target entity moves after that first tick, Travel does not follow —
//!   that would be Follow's job (a different, unimplemented procedure).
//! - **Ground-snapped, not physically simulated**, same as Wander.

use super::locomotion::{step_along_waypoints_detailed, LOCOMOTION_ARRIVAL_EPSILON};
use super::navmesh_path::resolve_cached_waypoints;
use super::wander::pick_wander_target;
use crate::components::NavPath;
use byroredux_core::ecs::components::{
    GlobalTransform, Transform, TravelBehavior, TravelState, Traveled,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use std::collections::{HashMap, VecDeque};

/// Seconds to keep retrying a missing PLDT reference before using the
/// procedure's documented fallback. Shared with Eat/Sleep anchor retries.
pub(crate) const ANCHOR_RETRY_SECS: f32 = 2.0;

/// Fallback pick radius (world units) around an actor's spawn position,
/// used when `TravelBehavior.radius` is `None` (no PLDT / radius 0) *and*
/// the target didn't resolve. Same scale as `wander.rs::WANDER_DEFAULT_RADIUS`
/// — both are "how far does this ambient behavior reach without an
/// authored radius" defaults for the same class of content.
/// The value itself is the one shared fact
/// `locomotion::AI_PACKAGE_DEFAULT_RADIUS` (#5587).
const TRAVEL_DEFAULT_RADIUS: f32 = super::locomotion::AI_PACKAGE_DEFAULT_RADIUS;

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

/// Whether to defer a fallback while a `NearReference` target may still be
/// arriving in the budgeted cell load. A missing reference gets one full
/// retry window before the caller may use its procedure-specific fallback.
pub(crate) fn defer_unresolved_reference(
    world: &World,
    target_form_id: Option<u32>,
    entity: EntityId,
    dt: f32,
    retries: &mut HashMap<EntityId, f32>,
) -> bool {
    if target_form_id.is_none() || resolve_near_reference_target(world, target_form_id).is_some() {
        retries.remove(&entity);
        return false;
    }

    match retries.get_mut(&entity) {
        Some(remaining) => {
            *remaining -= dt.max(0.0);
            if *remaining > 0.0 {
                true
            } else {
                retries.remove(&entity);
                false
            }
        }
        None => {
            retries.insert(entity, ANCHOR_RETRY_SECS);
            true
        }
    }
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

/// The home a package's fallback centres on: the actor's `EditorPlacement` for a Near Editor Location
/// package (Travel and Wander #7225, Guard and Ambush #7254, Sandbox #7262, as Eat/Sleep's anchor since #5391), otherwise
/// where the actor stands. An actor with no placement (a script-created one) keeps its current position.
pub(super) fn package_home(world: &World, actor: EntityId, near_editor_location: bool, current: Vec3) -> Vec3 {
    if !near_editor_location {
        return current;
    }
    world
        .get::<byroredux_core::ecs::components::EditorPlacement>(actor)
        .map_or(current, |placement| placement.translation)
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
    /// The walk's pathgrid switch, resolved with `waypoints` and advanced by this tick's step.
    grid: crate::components::GridWalk,
    /// Authored stride speed (M42.11 `WalkSpeed`), engine default when
    /// absent.
    speed: f32,
    /// Walking to a load door to pass it (XCT-ROUTE-001): arrival is
    /// [`DOOR_HOP_REACH`] from the floor in front of it.
    door_hop: bool,
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

impl super::navmesh_path::WalkDecision for TravelDecision {
    fn entity(&self) -> EntityId {
        self.entity
    }
    fn movement(&self) -> Option<(Vec3, Option<Quat>)> {
        Some((self.translation, self.rotation))
    }
    fn take_nav_path(&mut self) -> Option<NavPath> {
        self.nav_path.take()
    }
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
    anchor_retry: HashMap<EntityId, f32>,
    /// XCT-ROUTE-001 — routes chosen this frame, stamped in Pass 2.
    hops: Vec<(EntityId, crate::cell_loader::relocated::CrossSpaceHop)>,
    /// **Unroutable** actors (XCT-DEST-003/004), asked again after
    /// `ANCHOR_RETRY_SECS` rather than every frame: the route is an `O(n)`
    /// FormID resolve plus a door scan.
    unroutable: HashMap<EntityId, f32>,
}

/// How close to the floor in front of a load door a traveler passes it: the
/// pose is where the engine places someone coming the other way, so it is
/// reachable, but the walk stops on a collider short of it as often as not.
const DOOR_HOP_REACH: f32 = 64.0;

/// Drive straight-line walk-to-destination-once locomotion for every
/// [`TravelBehavior`] actor not yet [`Traveled`]. Registered
/// `add_exclusive(Stage::PostUpdate, …)`, same stage/slot family as
/// `wander_system`/`sandbox_seat_system` — NPC placement roots are
/// propagation roots (no `Parent`), so `Transform` == world position for
/// them.
fn travel_system_inner(world: &World, dt: f32, scratch: &mut TravelScratch) {
    let Some(behavior_q) = world.query::<TravelBehavior>() else {
        scratch.anchor_retry.clear();
        scratch.unroutable.clear();
        return;
    };
    scratch.anchor_retry.retain(|entity, _| behavior_q.contains(*entity));
    scratch.unroutable.retain(|entity, _| behavior_q.contains(*entity));
    scratch.hops.clear();

    // ── Pass 1a: resolve destinations (reads only; `resolve_destination`
    // may itself read `GlobalTransform`, so `PhysicsWorld` is NOT acquired
    // in this scope — #2134). ──
    scratch.pending.clear();
    // #3256 — stamped onto every NavPath this system writes and compared on
    // every cache hit, so a NAVM tile streaming in (or out) invalidates cached
    // paths that goal distance alone would keep forever. Read once at function
    // scope: the resolve pass and the apply pass below are separate blocks and
    // must agree, and nothing can bump the counter between them (cell load /
    // unload runs in a different scheduler stage).
    let residency_generation = crate::components::navmesh_residency_generation(world);
    {
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let traveled_q = world.query::<Traveled>();
        let state_q = world.query::<TravelState>();
        let tile_q = super::navmesh_path::NavGraphs::of(world);
        let nav_path_q = world.query::<NavPath>();
        let walk_speed_q = world.query::<crate::components::WalkSpeed>();
        let hop_q = world.query::<crate::cell_loader::relocated::CrossSpaceHop>();

        for (entity, behavior) in behavior_q.iter() {
            if traveled_q.as_ref().is_some_and(|q| q.contains(entity)) {
                continue; // already arrived (one-shot guard)
            }
            // XCT-ROUTE-008 — fading out at its load door before it passes.
            if crate::cell_loader::relocated::is_fading_at_door(world, entity) {
                continue;
            }
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let current = transform.translation;
            let state = state_q.as_ref().and_then(|q| q.get(entity)).copied();
            let destination = match state {
                Some(s) => {
                    scratch.anchor_retry.remove(&entity);
                    s.destination
                }
                None => {
                    if let Some(wait) = scratch.unroutable.get_mut(&entity) {
                        *wait -= dt.max(0.0);
                        if *wait > 0.0 {
                            continue;
                        }
                        scratch.unroutable.remove(&entity);
                    }
                    // XCT-ROUTE-001 / -003 — a destination in another space is
                    // reached through the load doors leading there, the first
                    // in the actor's own cell.
                    use crate::cell_loader::relocated::{local_form, report, route_toward, CrossSpaceHop, Route};
                    let route = route_toward(world, entity, behavior.target_form_id);
                    if let Some(form) = local_form(world, entity) {
                        report(world, form, match &route {
                            Route::Unroutable(reason) => Some(*reason),
                            _ => None,
                        });
                    }
                    match route {
                        Route::Door { door, position, placement } => {
                            scratch.anchor_retry.remove(&entity);
                            let target = behavior.target_form_id.unwrap_or(0);
                            scratch.hops.push((entity, CrossSpaceHop { door, placement, target }));
                            position
                        }
                        // XCT-DEST-001 — in its own space but not resident: its
                        // load-order position, not a wander target.
                        Route::Within(position) => {
                            scratch.anchor_retry.remove(&entity);
                            position
                        }
                        Route::Unroutable(reason) => {
                            log::info!(
                                "Travel: actor {entity} cannot reach {:08X}: {reason}",
                                behavior.target_form_id.unwrap_or(0)
                            );
                            scratch.unroutable.insert(entity, ANCHOR_RETRY_SECS);
                            continue;
                        }
                        Route::SameSpace => {
                            if defer_unresolved_reference(
                                world,
                                behavior.target_form_id,
                                entity,
                                dt,
                                &mut scratch.anchor_retry,
                            ) {
                                continue;
                            }
                            resolve_destination(
                                world,
                                behavior.target_form_id,
                                behavior.radius.unwrap_or(TRAVEL_DEFAULT_RADIUS),
                                behavior.form_id,
                                package_home(world, entity, behavior.near_editor_location, current),
                            )
                        }
                    }
                }
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
            // A new destination (no `TravelState` yet) starts straight again, whatever a
            // previous walk cached: it must not inherit that walk's pathgrid switch.
            let cached = if state.is_some() { nav_path_q.as_ref().and_then(|q| q.get(entity)) } else { None };
            let (_, waypoints, grid) = resolve_cached_waypoints(
                cached,
                tile_q.as_ref(),
                current,
                destination,
                0.0,
                residency_generation,
            );

            let speed = crate::components::WalkSpeed::or_default(
                walk_speed_q.as_ref().and_then(|q| q.get(entity)),
            );
            // VATS-LIMB-006 - crippled legs shorten the stride.
            let speed = speed * crate::limbs::locomotion_mult(world, entity);
            let door_hop = hop_q.as_ref().is_some_and(|q| q.contains(entity))
                || scratch.hops.iter().any(|(routed, _)| *routed == entity);
            scratch.pending.push(TravelPending {
                entity,
                current,
                rotation: transform.rotation,
                destination,
                waypoints,
                grid,
                speed,
                door_hop,
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
        // #3269 — consume `pending` by value: `step_along_waypoints_detailed` takes
        // ownership of the `VecDeque`, so iterating by reference forced a
        // clone of the whole waypoint list per entity per tick purely to
        // satisfy the borrow checker. Nothing reads `pending` after this
        // pass, and `drain` keeps the allocation for next frame, so the
        // scratch buffer's whole purpose is preserved.
        for p in scratch.pending.drain(..) {
            // Step toward the next unconsumed waypoint (EX-16 item 3
            // Phase 3), falling back to the final destination directly
            // when no resident-tile path was found — unchanged straight-
            // line behavior in that case.
            let (new_pos, rotation, waypoints, blocked) = step_along_waypoints_detailed(
                p.current,
                p.rotation,
                p.waypoints,
                p.destination,
                dt,
                p.speed,
                physics.as_deref(),
            );

            let horiz_delta = Vec3::new(
                new_pos.x - p.destination.x,
                0.0,
                new_pos.z - p.destination.z,
            );
            let reach = if p.door_hop { DOOR_HOP_REACH } else { LOCOMOTION_ARRIVAL_EPSILON };
            let arrived = horiz_delta.length_squared() <= reach * reach;

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
                        residency_generation,
                        waypoints,
                        grid: p.grid.stepped(blocked && !arrived, dt),
                    })
                },
            });
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 2: apply writes (each a scoped single-type lock). The shared
    // Transform + NavPath tail (#5580), then this system's own state. ──
    super::navmesh_path::write_back_walk(world, &mut scratch.decisions);
    if !scratch.hops.is_empty()
        && let Some(mut hq) = world.query_mut::<crate::cell_loader::relocated::CrossSpaceHop>() {
            for (entity, hop) in scratch.hops.drain(..) {
                hq.insert(entity, hop);
            }
        }
    // XCT-ROUTE-002 / -008 — a traveler that reached its load door fades out
    // and passes it: the move is requested (after the fade) and carried out by
    // the app's relocation step, which needs the renderer to take the actor
    // out of its cell. It is tagged `Traveled` so it stands still until then;
    // the package itself has not ended (its destination is beyond the door).
    let passed: Vec<EntityId> = scratch
        .decisions
        .iter()
        .filter(|d| d.state.is_none())
        .filter(|d| world.has::<crate::cell_loader::relocated::CrossSpaceHop>(d.entity))
        .map(|d| d.entity)
        .collect();
    for &entity in &passed {
        crate::cell_loader::relocated::reach_door(world, entity);
    }
    if let Some(mut sq) = world.query_mut::<TravelState>() {
        for d in &scratch.decisions {
            if let Some(state) = d.state {
                sq.insert(d.entity, state);
            }
        }
    }
    // An actor at its load door is not arrived: its fade holds it, and
    // `pass_door` tags it `Traveled` as it passes (#6925).
    if let Some(mut tq) = world.query_mut::<Traveled>() {
        for d in &scratch.decisions {
            if d.state.is_none() && !passed.contains(&d.entity) {
                tq.insert(d.entity, Traveled);
            }
        }
    }
    // M47.2 — arriving is the Travel procedure completing: the package's End
    // fragment runs (the one completion this engine models). #5869 — only a
    // flat `procedure_type == 6` package installs `TravelBehavior` (FO3 / FNV /
    // Oblivion), and those games' packages carry no PF_ fragments; Skyrim+
    // packages resolve through the procedure tree, which has no Travel leaf
    // yet (`docs/engine/packal.md`), so this is driven only by the synthetic VM
    // test until that lands.
    for d in scratch
        .decisions
        .iter()
        .filter(|d| d.state.is_none() && !passed.contains(&d.entity))
    {
        let package = world
            .get::<crate::components::AmbientPackageRuntime>(d.entity)
            .and_then(|runtime| runtime.active_package_form_id);
        if let Some(package) = package {
            byroredux_scripting::report_package_end(world, d.entity, package);
        }
    }
}

/// Travel system factory — returns a closure with a persistent
/// [`TravelScratch`] (#2033 / PERF-D1-2026-07-16-01). Behavior is
/// identical to calling [`travel_system_inner`] fresh every frame; use
/// this when wiring the system into the scheduler.
pub(crate) fn make_travel_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = TravelScratch::default();
    let mut seats = super::eat_sleep::FrameSeats::default();
    move |world: &World, dt: f32| {
        travel_system_inner(world, dt, &mut scratch);
        seat_arrived_travelers(world, dt, &mut seats);
    }
}

/// How far from where a Travel package left an actor the destination furniture's sit marker may
/// be. Generous: only that piece's markers are candidates, and an actor can arrive standing on
/// the furniture (Doc Mitchell ends his walk ~220 units above his chair's marker).
const TRAVEL_SEAT_RADIUS: f32 = super::locomotion::AI_PACKAGE_DEFAULT_RADIUS;

/// A Travel package whose destination is a piece of furniture ends with the actor using it: CS
/// *Travel Package*, "If a furniture object such as a bed or chair is selected then the actor will
/// lay in the bed or sit in the chair". Each arrived, unseated traveler whose destination is a seat
/// takes it through the Eat / Sleep seating path (reservation, root snap, sit-enter park), walking
/// the last steps to the marker first. New Vegas's opening needs it: the psych test starts once
/// `DocMitchellREF.IsCurrentFurnitureRef DocMitchellChairREF`, and the package that takes Doc there
/// is a Travel to `DocMitchellChairREF`.
fn seat_arrived_travelers(world: &World, dt: f32, seats: &mut super::eat_sleep::FrameSeats) {
    let arrived: Vec<(EntityId, u32)> = {
        let (Some(behaviors), Some(traveled)) = (world.query::<TravelBehavior>(), world.query::<Traveled>()) else {
            return;
        };
        let seated = world.query::<byroredux_core::ecs::components::Seated>();
        behaviors
            .iter()
            .filter(|(entity, _)| traveled.contains(*entity) && !seated.as_ref().is_some_and(|q| q.contains(*entity)))
            .filter_map(|(entity, behavior)| Some((entity, behavior.target_form_id?)))
            .collect()
    };
    if arrived.is_empty() {
        return;
    }
    seats.begin_frame();
    for (actor, target) in arrived {
        let Some(furniture) = resolve_entity_by_global_form_id(world, target) else {
            continue;
        };
        if !crate::player_seat::is_seat(world, furniture) {
            continue;
        }
        match super::eat_sleep::seat_at_marker(world, actor, super::eat_sleep::EatOrSleep::Eat, Some(TRAVEL_SEAT_RADIUS), Some(furniture), seats) {
            super::eat_sleep::SeatAttempt::Seated => log::info!("Travel package: actor {actor} sat down at its destination {target:08X}"),
            super::eat_sleep::SeatAttempt::Approach(marker) => {
                let Some((current, rotation)) = world.get::<Transform>(actor).map(|t| (t.translation, t.rotation)) else {
                    continue;
                };
                let speed = world.get::<crate::components::WalkSpeed>(actor).map_or(super::locomotion::LOCOMOTION_WALK_SPEED, |s| s.0);
                let step = super::navmesh_path::cached_walk_step(world, actor, current, rotation, marker, 0.0, dt, speed);
                super::navmesh_path::apply_walk_step(world, [(actor, step.position, step.rotation)]);
            }
            super::eat_sleep::SeatAttempt::Retry => {}
        }
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
    use crate::components::{GridWalk, NavmeshTile};
    use crate::systems::navmesh_path::test_tiles::{grid_round_the_wall, two_triangle_quad_scaled, walled_physics};
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn unresolved_reference_fallback_waits_for_the_shared_retry_window() {
        let mut world = World::new();
        let actor = world.spawn();
        let mut retries = HashMap::new();
        assert!(defer_unresolved_reference(
            &world,
            Some(0x1234),
            actor,
            0.0,
            &mut retries
        ));
        assert!(defer_unresolved_reference(
            &world,
            Some(0x1234),
            actor,
            ANCHOR_RETRY_SECS * 0.5,
            &mut retries
        ));
        assert!(!defer_unresolved_reference(
            &world,
            Some(0x1234),
            actor,
            ANCHOR_RETRY_SECS * 0.5,
            &mut retries
        ));
        assert!(retries.is_empty(), "expired timers are removed before fallback");
    }

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
                near_editor_location: false,
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

    /// #7225 — a Near Editor Location Travel (a `HorseGoHome` / `SE45zReturnHome`) whose actor an earlier
    /// package walked away goes back to its editor placement, not to a point around where it stands. With no
    /// radius the destination is the placement exactly; a plain Travel still picks around the actor.
    #[test]
    fn a_near_editor_location_travel_returns_to_the_editor_placement() {
        let placement = Vec3::new(-300.0, 0.0, 40.0);
        let away = Vec3::new(500.0, 0.0, 500.0);
        for near_editor_location in [true, false] {
            let mut world = World::new();
            world.register::<TravelBehavior>();
            world.register::<TravelState>();
            world.register::<Traveled>();
            world.register::<Transform>();
            world.register::<byroredux_core::ecs::components::EditorPlacement>();
            let actor = world.spawn();
            world.insert(actor, Transform::from_translation(away));
            world.insert(actor, byroredux_core::ecs::components::EditorPlacement { translation: placement });
            world.insert(
                actor,
                TravelBehavior { radius: Some(0.0), target_form_id: None, form_id: 0x0009_0009, near_editor_location },
            );
            travel_system(&world, 0.016);
            if near_editor_location {
                let destination = world.get::<TravelState>(actor).expect("walking home").destination;
                assert!((destination - placement).length() < 1e-3, "destination {destination:?}, expected the placement");
                assert!(!world.has::<Traveled>(actor), "800 units from home, not arrived");
            } else {
                // A zero radius around where it stands: it has arrived (the pre-#7225 outcome for both).
                assert!(world.has::<Traveled>(actor));
            }
        }
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
                near_editor_location: false,
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
    fn travel_retries_a_near_reference_that_spawns_after_the_actor() {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.insert_resource(FormIdPool::default());

        // Spawn the package actor first; its referenced REFR is still in a
        // later budgeted cell-load slice.
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(
            actor,
            TravelBehavior {
                radius: Some(200.0),
                target_form_id: Some(0x0001_59E2),
                form_id: 0x0004_0004,
                near_editor_location: false,
            },
        );
        let mut scratch = TravelScratch::default();
        travel_system_inner(&world, 0.0, &mut scratch);
        assert!(
            world.get::<TravelState>(actor).is_none(),
            "a missing target must not freeze the first fallback into TravelState"
        );
        assert_eq!(
            world.get::<Transform>(actor).expect("transform").translation,
            Vec3::ZERO,
            "the actor waits while its authored target may still spawn"
        );

        let mut pool = world.remove_resource::<FormIdPool>().expect("pool");
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0001_59E2),
        });
        world.insert_resource(pool);
        let target = world.spawn();
        let destination = Vec3::new(300.0, 10.0, 400.0);
        world.insert(target, FormIdComponent(fid));
        world.insert(target, GlobalTransform::new(destination, Quat::IDENTITY, 1.0));

        travel_system_inner(&world, 0.1, &mut scratch);
        assert_eq!(
            world.get::<TravelState>(actor).expect("state").destination,
            destination,
            "the retry picks up the later-spawned authored target"
        );
        assert!(scratch.anchor_retry.is_empty());
    }

    /// M47.2 — arriving reports the active package's completion (once), so
    /// its End fragment can run; an actor with no active package reports
    /// nothing.
    #[test]
    fn travel_arrival_reports_the_package_end() {
        use crate::components::AmbientPackageRuntime;
        use byroredux_scripting::PendingPackageEnds;
        const PACK: u32 = 0x0001_0F30;
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<AmbientPackageRuntime>();
        world.insert_resource(PendingPackageEnds::default());

        let spot = Vec3::new(50.0, 0.0, 50.0);
        let mut spawn = |package: Option<u32>| {
            let actor = world.spawn();
            world.insert(actor, Transform::from_translation(spot));
            world.insert(
                actor,
                TravelBehavior { radius: Some(100.0), target_form_id: None, form_id: 0x0005_0005, near_editor_location: false },
            );
            world.insert(actor, TravelState { destination: spot });
            world.insert(
                actor,
                AmbientPackageRuntime {
                    package_candidates: Vec::new(),
                    active_package_form_id: package,
                    actor_form_id: 0x0005_0005,
                    last_evaluated_game_minute: None,
                },
            );
            actor
        };
        let with_package = spawn(Some(PACK));
        let _without = spawn(None);

        travel_system(&world, 0.5);
        travel_system(&world, 0.5);

        let ends = world.resource::<PendingPackageEnds>();
        assert_eq!(ends.0.len(), 1, "one completion, reported on the arrival tick only");
        assert_eq!((ends.0[0].actor, ends.0[0].package), (with_package, PACK));
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
                near_editor_location: false,
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

    /// #6925 (INC2 missing test): an actor that arrives at its load door with a cross-space hop and
    /// an authored fade starts fading there and is not `Traveled` yet; `pass_door` tags it when the
    /// fade-out ends. Dropping travel_system's `!passed.contains(..)` guard tags it at the door,
    /// so a save taken mid-fade would restore an arrived actor that never passed.
    #[test]
    fn an_actor_at_its_load_door_is_not_traveled_while_it_fades() {
        use crate::cell_loader::relocated::{is_fading_at_door, CrossSpaceHop, DoorFade};
        use byroredux_plugin::esm::records::{GameSetting, SettingValue};
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<CrossSpaceHop>();
        world.register::<DoorFade>();
        let mut index = byroredux_plugin::esm::EsmIndex::default();
        index.game_settings.insert(
            0x10,
            GameSetting { form_id: 0x10, editor_id: "fActorTeleportFadeSeconds".into(), value: SettingValue::Float(1.0) },
        );
        world.insert_resource(byroredux_scripting::obscript_state::GameSettings::from_index(&index));

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::new(50.0, 0.0, 50.0)));
        world.insert(entity, TravelBehavior { radius: Some(100.0), target_form_id: None, form_id: 0x0005_0005, near_editor_location: false });
        world.insert(entity, TravelState { destination: Vec3::new(50.0, 0.0, 50.0) });
        let far_side = byroredux_scripting::CreatedPlacement {
            cell: byroredux_scripting::CreatedCell::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) },
            position_zup: [1000.0, 0.0, 0.0],
            rotation_zup: [0.0; 3],
        };
        world.insert(entity, CrossSpaceHop { door: 9, placement: far_side, target: 0x77 });

        travel_system(&world, 0.5);
        assert!(is_fading_at_door(&world, entity), "it reached its door and the fade started");
        assert!(!world.has::<Traveled>(entity), "not arrived while it fades (#6925)");
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
                near_editor_location: false,
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
        world.insert(tile, NavmeshTile(two_triangle_quad_scaled(1000.0)));

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
                near_editor_location: false,
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
                near_editor_location: false,
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
        world.insert(tile, NavmeshTile(two_triangle_quad_scaled(1000.0)));

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
                near_editor_location: false,
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

    // ── AIP-WALK-003: Oblivion's pathgrid, taken once the straight walk is blocked ──

    /// A traveler at (-400,0,0) bound for (400,0,0), the pathgrid resident, `physics` installed.
    fn grid_traveler(physics: byroredux_physics::PhysicsWorld) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<TravelBehavior>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<FormIdComponent>();
        world.register::<NavPath>();
        world.insert_resource(physics);
        let mut pool = FormIdPool::new();
        let target_fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Oblivion.esm"),
            local: LocalFormId(0x000E_0001),
        });
        world.insert_resource(pool);
        let target = world.spawn();
        world.insert(target, FormIdComponent(target_fid));
        world.insert(target, GlobalTransform::new(Vec3::new(400.0, 0.0, 0.0), Quat::IDENTITY, 1.0));
        crate::components::spawn_pathgrid_tiles(&mut world, [&grid_round_the_wall()]);
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::new(-400.0, 0.0, 0.0)));
        world.insert(
            actor,
            TravelBehavior { radius: None, target_form_id: Some(0x000E_0001), form_id: 0x000E_0002, near_editor_location: false },
        );
        (world, actor)
    }

    /// A wall across the straight line: Travel walks into it, takes the grid after the controller
    /// has held it for `GridWalk::AFTER_BLOCKED_SECS`, goes round and arrives.
    #[test]
    fn a_traveler_blocked_by_a_wall_takes_the_pathgrid_and_arrives() {
        let (world, actor) = grid_traveler(walled_physics());
        let mut tick = make_travel_system();
        let (mut switched_at, mut arrived_at) = (None, None);
        for i in 0..400 {
            tick(&world, 0.1);
            let t = (i + 1) as f32 * 0.1;
            let on_grid = world.get::<NavPath>(actor).is_some_and(|p| p.grid.on_grid);
            if on_grid && switched_at.is_none() {
                switched_at = Some(t);
            }
            if world.get::<Traveled>(actor).is_some() {
                arrived_at = Some(t);
                break;
            }
        }
        let switched_at = switched_at.expect("a walk held against a wall must take the pathgrid");
        assert!(
            switched_at >= GridWalk::AFTER_BLOCKED_SECS,
            "not before the controller has held it for the threshold: switched at {switched_at}s"
        );
        assert!(arrived_at.is_some(), "the grid route must carry the traveler round the wall");
    }

    /// The Imperial Prison regression: a straight walk that is clear never takes the grid, whatever
    /// is resident, so its route is not bent into the stairs it stands on.
    #[test]
    fn a_traveler_with_a_clear_straight_walk_never_takes_the_pathgrid() {
        let mut floor_only = byroredux_physics::PhysicsWorld::new();
        floor_only.spawn_fixed_cuboid(Vec3::new(2000.0, 20.0, 2000.0), Vec3::new(0.0, -20.0, 0.0));
        let (world, actor) = grid_traveler(floor_only);
        let mut tick = make_travel_system();
        for _ in 0..200 {
            tick(&world, 0.1);
            assert!(
                !world.get::<NavPath>(actor).is_some_and(|p| p.grid.on_grid),
                "a clear walk must stay straight"
            );
            if world.get::<Traveled>(actor).is_some() {
                return;
            }
        }
        panic!("a clear straight walk must arrive");
    }
}
