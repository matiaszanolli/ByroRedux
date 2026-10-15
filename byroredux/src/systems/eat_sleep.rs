//! M42 — Eat (FO3/FNV `PKDT` procedure 3) and Sleep (procedure 4)
//! procedures, v0.
//!
//! Both share one runtime: walk once to the package's `PLDT` anchor
//! (#5391 — [`EatSleepLocation`]: a `NearReference` lands the actor at
//! the authored furniture, "near editor location" at its
//! [`EditorPlacement`], "in cell" stays put while that cell is resident
//! and idles otherwise, and every other type stays where the actor
//! stands — no random walk), then occupy the nearest furniture marker
//! within the location radius through the
//! sandbox seating path (`collect_marker_seats` + `pick_nearest_seat` +
//! `apply_seat_assignments` — the same reservations, root snap, and
//! sit-enter final-frame park). Eat seats at **sit** markers; Sleep at
//! **sleep** markers, falling back to sit markers when a cell's beds
//! author none — parking the seated pose at the sleep marker's authored
//! entry position is the documented v0 approximation for the lie-down
//! clip no archive this engine reads carries.
//!
//! #5390 — the Sleep-marker preference is only reachable on Skyrim+:
//! `FurnitureMarkerKind::Sleep` resolves from `animation_type == 2`,
//! a Skyrim+ `BSFurnitureMarker` field, and legacy Oblivion/FO3/FNV
//! markers author `animation_type = 0` (the documented v0 `Sit`
//! over-match, which includes beds and lean markers — Phase C's
//! `furnituremarkerNN.nif` decode is the real fix). On FO3/FNV the sit
//! fallback IS the sleep path: every sleeper takes the nearest sit
//! marker, chairs included, and an Eat actor can symmetrically take a
//! bed's marker. The fallback logs at debug so a live session can see
//! which arm fired.
//!
//! The walk is the shared KCC-backed locomotion step the other frozen-goal
//! movers use (#5452): `navmesh_path::cached_walk_step` routes it along the
//! actor's cached single-resident-tile `NavPath` (straight at the goal when
//! no tile covers it — the v0 pathing caveat the other M42 procedures
//! document), and the KCC's `blocked` flag is consumed: an actor held
//! against an obstacle for [`LOCOMOTION_STUCK_REPICK_SECS`](crate::systems::locomotion::LOCOMOTION_STUCK_REPICK_SECS) while inside
//! its seat-search radius stops walking and attempts the seat pick from
//! where it stands, instead of grinding in place forever. Seating is one-shot: `Seated` skips the actor on later ticks,
//! and the package handover's `clear_ambient_behavior` un-seats
//! (restoring the pre-park animation, #3333) exactly as Sandbox does.
//!
//! #5449 — an arrived actor that cannot be seated (no marker in the cell,
//! every marker reserved, none within the radius) is not one-shot: it
//! retries, but on a coarse [`SEAT_RETRY_SECS`] cadence rather than every
//! frame, and every attempt in a frame shares one marker gather
//! ([`FrameSeats`]) held in the system's persistent [`EatSleepScratch`] —
//! the same factory-captured scratch `sandbox_seat_system` uses (#2033).
//!
//! #5527 — the same cadence covers an actor whose `PLDT` anchor is not
//! resolvable yet (an `InCell` package outside its resident interior, a
//! reference not loaded yet): it idles on [`ANCHOR_RETRY_SECS`] instead of
//! re-resolving every frame for the whole package window, and the pose /
//! speed / destination reads the walk leg needs are gathered once per frame
//! in one query pass instead of four `world.get` lock round-trips per actor.

use byroredux_core::ecs::components::{
    EatBehavior, EatSleepLocation, EatSleepState, EditorPlacement, FurnitureMarker,
    FurnitureMarkerKind, GlobalTransform, Seated, SleepBehavior, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};

use byroredux_scripting::condition::entity_global_form_id;
use byroredux_scripting::PackageTargetRegistry;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::components::{SandboxSitClip, SeatReservations};

/// The one-shot walk destination's arrival threshold (world units) —
/// inside this, the actor stops walking and seats. Small relative to
/// the seat-search radius so the seat pick, not the walk, decides the
/// final resting furniture.
const ARRIVE_RADIUS: f32 = 64.0;

/// Real seconds between seat attempts for an arrived actor that could not
/// be seated (#5449). One in-game minute at the default `time_scale` — the
/// cadence `ambient_ai_package_system` re-evaluates at — so a freed marker
/// is found promptly without a whole-world furniture gather per frame.
const SEAT_RETRY_SECS: f32 = 2.0;

/// Real seconds between anchor resolution attempts for an actor whose `PLDT`
/// anchor is not resolvable yet (#5527) — the seat-retry cadence, for the
/// same reason: a cell that becomes resident or a reference that finishes
/// loading is picked up within one in-game minute, without a map probe (and
/// a lowercase cell-key allocation) per idle actor per frame.
const ANCHOR_RETRY_SECS: f32 = SEAT_RETRY_SECS;

/// One world-space seat: `((furniture entity, marker index), transform)`.
type Seat = ((EntityId, u32), GlobalTransform);

/// The cell's seat markers, gathered at most once per frame per kind and
/// shared by every Eat/Sleep actor that attempts a seat that frame
/// (#5449). Backing allocations persist across frames.
#[derive(Default)]
struct FrameSeats {
    sit: Vec<Seat>,
    sit_ready: bool,
    sleep: Vec<Seat>,
    sleep_ready: bool,
}

impl FrameSeats {
    fn begin_frame(&mut self) {
        self.sit_ready = false;
        self.sleep_ready = false;
    }

    fn sit(&mut self, world: &World) -> &[Seat] {
        if !self.sit_ready {
            self.sit.clear();
            super::sandbox::collect_marker_seats(world, &mut self.sit, super::sandbox::is_sit_marker);
            self.sit_ready = true;
        }
        &self.sit
    }

    fn sleep(&mut self, world: &World) -> &[Seat] {
        if !self.sleep_ready {
            self.sleep.clear();
            super::sandbox::collect_marker_seats(world, &mut self.sleep, is_sleep_marker);
            self.sleep_ready = true;
        }
        &self.sleep
    }
}

/// Persistent per-system state captured by [`make_eat_sleep_system`].
/// `NOT_SAVED_BY_DESIGN`: pure scheduling state — a load starts every
/// actor's retry clock at zero, so the first post-load attempt is
/// immediate.
#[derive(Default)]
struct EatSleepScratch {
    seats: FrameSeats,
    /// Seconds until an arrived-but-unseated actor may attempt again.
    seat_retry: FxHashMap<EntityId, f32>,
    /// Seconds until an actor whose anchor did not resolve may resolve it
    /// again (#5527).
    anchor_retry: FxHashMap<EntityId, f32>,
    /// Continuous KCC-blocked walk seconds per walking actor (#5452) —
    /// runtime-only, the `WalkStuckTimer` role for this mover.
    stuck_secs: FxHashMap<EntityId, f32>,
    /// The frame's gathered actor list, kept for its capacity (#5707).
    actors: Vec<EatSleepActor>,
    /// Membership set over `actors` for the retry-map pruning (#5707).
    live: FxHashSet<EntityId>,
}

/// Which procedure an actor carries this tick — collected first so the
/// per-actor loop below never holds two behavior storages at once.
#[derive(Clone, Copy)]
enum EatOrSleep {
    Eat,
    Sleep,
}

/// System: drive Eat/Sleep procedure actors — walk to the `PLDT`
/// destination, then seat at the nearest matching furniture marker.
/// Registered `add_exclusive(Stage::PostUpdate, …)` beside the other
/// M42 procedure systems; reads this frame's propagated
/// `GlobalTransform`s.
/// One collected actor per tick — the behavior pair is flattened into
/// a row so the per-actor loop below never holds two behavior storages
/// at once.
struct EatSleepActor {
    npc: EntityId,
    kind: EatOrSleep,
    radius: Option<f32>,
    location: EatSleepLocation,
    /// The recorded walk destination, once the anchor has resolved.
    destination: Option<Vec3>,
    /// World position from this frame's propagated `GlobalTransform`.
    global: Option<Vec3>,
    /// Local rotation (the authoritative pose on a propagation root).
    rotation: Quat,
    /// Authored walk speed, if the actor carries one.
    walk_speed: Option<f32>,
}

/// Eat/Sleep system factory: captures the persistent [`EatSleepScratch`]
/// (shared marker gather + per-actor seat-retry clocks, #5449).
pub(crate) fn make_eat_sleep_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = EatSleepScratch::default();
    move |world: &World, dt: f32| eat_sleep_system_inner(world, dt, &mut scratch)
}

fn eat_sleep_system_inner(world: &World, dt: f32, scratch: &mut EatSleepScratch) {
    scratch.seats.begin_frame();
    // #5707 — the gather reuses the scratch's buffers. Taken out of the
    // scratch for the frame (the per-actor loop borrows the retry maps
    // beside it) and handed back before every exit.
    let mut actors = std::mem::take(&mut scratch.actors);
    actors.clear();
    scratch.live.clear();
    // One-shot guard: a seated actor is done (the package handover
    // un-seats), so it never enters the list — a sleeper stays seated for
    // hours of game time and would otherwise be re-gathered every frame.
    // Probed while filling; the `Seated` guard drops with this block,
    // before the destination/seat work below takes any same-type guard.
    {
        let seated_q = world.query::<Seated>();
        let is_seated = |npc: EntityId| seated_q.as_ref().is_some_and(|seated| seated.contains(npc));
        if let Some(query) = world.query::<EatBehavior>() {
            for (npc, behavior) in query.iter() {
                if is_seated(npc) {
                    continue;
                }
                actors.push(EatSleepActor {
                    npc,
                    kind: EatOrSleep::Eat,
                    radius: behavior.radius,
                    location: behavior.location,
                    destination: None,
                    global: None,
                    rotation: Quat::IDENTITY,
                    walk_speed: None,
                });
            }
        }
        if let Some(query) = world.query::<SleepBehavior>() {
            for (npc, behavior) in query.iter() {
                if is_seated(npc) {
                    continue;
                }
                actors.push(EatSleepActor {
                    npc,
                    kind: EatOrSleep::Sleep,
                    radius: behavior.radius,
                    location: behavior.location,
                    destination: None,
                    global: None,
                    rotation: Quat::IDENTITY,
                    walk_speed: None,
                });
            }
        }
    }
    if actors.is_empty() {
        scratch.seat_retry.clear();
        scratch.anchor_retry.clear();
        scratch.stuck_secs.clear();
        scratch.actors = actors;
        return;
    }
    // Drop retry / stuck clocks of actors that seated, changed procedure
    // or left: one membership set built from the gather, so the sweeps are
    // O(retry entries) instead of O(entries × actors).
    if !(scratch.seat_retry.is_empty()
        && scratch.anchor_retry.is_empty()
        && scratch.stuck_secs.is_empty())
    {
        scratch.live.extend(actors.iter().map(|actor| actor.npc));
        let live = &scratch.live;
        scratch.seat_retry.retain(|npc, _| live.contains(npc));
        scratch.anchor_retry.retain(|npc, _| live.contains(npc));
        scratch.stuck_secs.retain(|npc, _| live.contains(npc));
    }
    // #5527 — everything the loop below reads per actor, gathered in one
    // pass: a single read-lock acquire per storage instead of one
    // `world.get` (acquire + release + lock-tracker entry) per component per
    // actor per frame. Nothing in the loop changes another actor's
    // `GlobalTransform`, `WalkSpeed` or recorded destination, and its own
    // `Transform` write happens after its own read, so the snapshot is exact.
    // Every guard drops with this block, before the writes below.
    //
    // #5535 — `Transform` is taken before `GlobalTransform` and `WalkSpeed`,
    // the process-wide order every other mover (ragdoll, follow, escort,
    // forcegreet, the wander/patrol gather) acquires them in. #5527 first
    // gathered `GlobalTransform` → `WalkSpeed` → `Transform`, closing both a
    // `Transform`/`GlobalTransform` and a `Transform`/`WalkSpeed` lock-order
    // cycle (`BYRO_LOCK_ORDER_CHECK=1`).
    {
        let state_q = world.query::<EatSleepState>();
        let transform_q = world.query::<Transform>();
        let global_q = world.query::<GlobalTransform>();
        let speed_q = world.query::<crate::components::WalkSpeed>();
        for actor in &mut actors {
            actor.destination = state_q
                .as_ref()
                .and_then(|q| q.get(actor.npc))
                .map(|state| state.destination);
            actor.global = global_q
                .as_ref()
                .and_then(|q| q.get(actor.npc))
                .map(|transform| transform.translation);
            actor.walk_speed = speed_q
                .as_ref()
                .and_then(|q| q.get(actor.npc))
                .map(|speed| speed.0);
            actor.rotation = transform_q
                .as_ref()
                .and_then(|q| q.get(actor.npc))
                .map(|transform| transform.rotation)
                .unwrap_or_default();
        }
    }

    for EatSleepActor {
        npc,
        kind,
        radius,
        location,
        destination,
        global,
        rotation,
        walk_speed,
    } in actors.drain(..)
    {
        // One-shot walk destination: resolve on first sight, reuse
        // after. Inserted through a scoped write so no read guard is
        // held across it.
        let destination = match destination {
            Some(destination) => destination,
            None => {
                // #5527 — an anchor that did not resolve waits out its
                // clock instead of re-resolving every frame.
                if let Some(wait) = scratch.anchor_retry.get_mut(&npc) {
                    *wait -= dt;
                    if *wait > 0.0 {
                        continue;
                    }
                }
                // #5391 — an `InCell` package whose cell is not resident
                // idles (resolved again after `ANCHOR_RETRY_SECS`, no
                // walk, no seat).
                let Some(destination) = resolve_anchor(world, npc, global, location) else {
                    scratch.anchor_retry.insert(npc, ANCHOR_RETRY_SECS);
                    continue;
                };
                scratch.anchor_retry.remove(&npc);
                if let Some(mut states) = world.query_mut::<EatSleepState>() {
                    states.insert(npc, EatSleepState { destination });
                }
                destination
            }
        };
        let current = global.unwrap_or_default();
        let flat = Vec3::new(destination.x - current.x, 0.0, destination.z - current.z);
        // Arrived: seat at the nearest matching marker. Sleep prefers
        // sleep markers, falling back to sit markers when the cell's
        // beds author none; Eat sits. #5500 — an In-Cell package's
        // location IS the resident cell (the GECK greys out its radius),
        // so its search spans the whole cell rather than 512 BU of
        // wherever the actor happens to stand: every one of the 70 FNV
        // In-Cell packages authors radius 0, and a sleeper whose bed sat
        // further than the default radius took the nearest chair or
        // nothing.
        let search_radius = match location {
            EatSleepLocation::InCell(_) => f32::INFINITY,
            _ => radius.unwrap_or(super::sandbox::SEAT_SEARCH_RADIUS),
        };
        if flat.length() > ARRIVE_RADIUS {
            // Still walking — one KCC-backed step toward the destination.
            // #5373 — the step writes `Transform` (the authoritative
            // world pose on a propagation root), never
            // `GlobalTransform`: the derived global is rebuilt from the
            // local on the next propagation, so writing it erased the
            // step every frame and the diner never reached its marker.
            // Same write shape as travel/wander's pass 2.
            let speed = walk_speed.unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
            // #5452 — routed along the cached NAVM path like Travel (which
            // also lines `destination.y` up with the ground contract), and
            // the KCC's `blocked` flag feeds the stuck clock. #5371 —
            // `cached_walk_step` keeps `PhysicsWorld` a lock-order sink: the
            // guard is its own scope, the `Transform` write comes after.
            let step = super::navmesh_path::cached_walk_step(
                world,
                npc,
                current,
                rotation,
                destination,
                0.0,
                dt,
                speed,
            );
            if let Some(mut transforms) = world.query_mut::<Transform>() {
                if let Some(transform) = transforms.get_mut(npc) {
                    transform.translation = step.position;
                    if let Some(rotation) = step.rotation {
                        transform.rotation = rotation;
                    }
                }
            }
            let stuck = scratch.stuck_secs.entry(npc).or_insert(0.0);
            let gave_up =
                crate::systems::locomotion::advance_stuck_repick(stuck, step.blocked, dt);
            // Held against an obstacle inside the seat-search radius: the
            // destination is unreachable but a marker may well be in
            // range, so try the seat from here. Anywhere else keep
            // walking (or grinding — a further give-up has no better
            // option than the next stuck window).
            if !(gave_up && flat.length() <= search_radius) {
                continue;
            }
        }
        scratch.stuck_secs.remove(&npc);
        // #5449 — an arrived actor that failed to seat waits out its
        // retry clock instead of re-gathering the world's furniture
        // every frame.
        if let Some(wait) = scratch.seat_retry.get_mut(&npc) {
            *wait -= dt;
            if *wait > 0.0 {
                continue;
            }
        }
        if seat_at_marker(world, npc, kind, Some(search_radius), &mut scratch.seats) {
            scratch.seat_retry.remove(&npc);
            super::navmesh_path::clear_cached_path(world, npc);
        } else {
            scratch.seat_retry.insert(npc, SEAT_RETRY_SECS);
        }
    }
    // The drain emptied the list; hand the buffer back for next frame.
    scratch.actors = actors;
}

/// #5391 — the walk destination for one actor's `PLDT` anchor, resolved
/// once. `None` means "not resolvable yet": the actor idles and the next
/// tick retries — an `InCell` package whose cell is not the resident
/// interior, or (#5500) a reference anchor whose target is not loaded
/// yet. A budgeted cell load spawns references in authored order, so a
/// spawn-time Eat/Sleep winner can be asked before its marker exists
/// (163 FNV actor/target pairs place the target after the actor); the
/// old code cached the actor's then-current position for the life of
/// the package and froze the diner at its spawn point.
///
/// `global` is the actor's world position from the frame's gather pass; an
/// actor without a `GlobalTransform` idles, as before.
fn resolve_anchor(
    world: &World,
    npc: EntityId,
    global: Option<Vec3>,
    location: EatSleepLocation,
) -> Option<Vec3> {
    let current = global?;
    match location {
        // #5500 — an unresolvable reference (not loaded yet) resolves
        // again next tick rather than freezing the actor's current
        // position as the destination.
        EatSleepLocation::NearReference(form_id) => {
            super::travel::resolve_near_reference_target(world, Some(form_id))
        }
        // #5500 — the target of the actor's own XLKR edge, through the
        // same registry the scene runtime reads (keyword 0 = the default
        // link, preferred over named edges).
        EatSleepLocation::NearLinkedReference => {
            let actor_form_id = entity_global_form_id(world, npc)?;
            let registry = world.try_resource::<PackageTargetRegistry>()?;
            let linked = registry.linked_reference(actor_form_id)?;
            registry.position(linked)
        }
        EatSleepLocation::InCell(cell_form_id) => {
            cell_is_resident(world, cell_form_id).then_some(current)
        }
        EatSleepLocation::NearEditorLocation => Some(
            world
                .get::<EditorPlacement>(npc)
                .map(|placement| placement.translation)
                .unwrap_or(current),
        ),
        EatSleepLocation::NearCurrentLocation => Some(current),
    }
}

/// Whether `cell_form_id` is the interior currently loaded. Exterior
/// `InCell` targets are not resolved (v0) and read as not resident.
fn cell_is_resident(world: &World, cell_form_id: u32) -> bool {
    // Each guard drops before the next is taken.
    let Some(key) = world
        .try_resource::<crate::cell_loader::CurrentCellContext>()
        .map(|context| context.cell_editor_id.to_ascii_lowercase())
    else {
        return false;
    };
    world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()
        .is_some_and(|index| {
            index
                .0
                .cells
                .cells
                .get(&key)
                .is_some_and(|cell| cell.form_id == cell_form_id)
        })
}

/// Seat one arrived actor at its procedure's marker kind, reusing the
/// sandbox seating path (reservation, root snap, sit-enter park). Returns
/// whether the actor was seated; `false` means "try again later".
fn seat_at_marker(
    world: &World,
    npc: EntityId,
    kind: EatOrSleep,
    radius: Option<f32>,
    frame_seats: &mut FrameSeats,
) -> bool {
    // #5495 — creatures never take furniture: the per-cell sit clip's
    // channels are human-rig bones, so seating a creature freezes it in
    // a human pose at the marker for the life of the cell. The actor has
    // already walked to the authored location; it just stands there.
    if world
        .get::<byroredux_core::ecs::components::CreatureActor>(npc)
        .is_some()
    {
        return false;
    }
    // No sit-enter clip → no seating path (Skyrim+/Havok games, or the
    // clip wasn't archived) — the actor has already walked to the
    // authored location and simply stands there, the same posture as a
    // sandbox actor in a clip-less cell.
    let Some((sit_handle, hold_time)) = world.try_resource::<SandboxSitClip>().and_then(|r| r.0)
    else {
        return false;
    };
    let is_sleep = matches!(kind, EatOrSleep::Sleep);
    let use_sleep_markers = is_sleep && !frame_seats.sleep(world).is_empty();
    if is_sleep && !use_sleep_markers {
        // #5390 — on FO3/FNV this is not "no bed found" but "no bed can
        // EVER resolve": legacy markers author animation_type 0 and the
        // translate boundary reads them all as Sit (see the module doc).
        // The fallback is the legacy sleep path by design until Phase C
        // decodes furnituremarkerNN.
        log::debug!(
            "[m42] sleep npc={npc}: no decodable Sleep marker in range — \
             legacy cells author none (animation_type 0 over-match, #5390); \
             seating at the nearest sit marker"
        );
    }
    let collected: &[Seat] = if use_sleep_markers {
        &frame_seats.sleep
    } else {
        frame_seats.sit(world)
    };
    if collected.is_empty() {
        return false;
    }
    let Some(current) = world.get::<GlobalTransform>(npc).map(|t| t.translation) else {
        return false;
    };
    let seat = {
        let empty = std::collections::HashMap::new();
        // Bind the resource guard: `pick_nearest_seat` borrows the map,
        // and a `.map(|r| &r.0)` on the guard's temporary would not
        // outlive the expression.
        let reservations_guard = world.try_resource::<SeatReservations>();
        let reservations = reservations_guard
            .as_deref()
            .map(|reservations| &reservations.0)
            .unwrap_or(&empty);
        super::sandbox::pick_nearest_seat(
            current,
            collected,
            reservations,
            radius.unwrap_or(super::sandbox::SEAT_SEARCH_RADIUS),
        )
    };
    let Some(((furn_e, marker_idx), seat)) = seat else {
        return false;
    };
    // Reserve through the shared resource so a sandboxing actor (or a
    // second diner) cannot take the same marker.
    if let Some(mut reservations) = world.try_resource_mut::<SeatReservations>() {
        reservations.0.insert((furn_e, marker_idx), npc);
    }
    let assignments = vec![(npc, furn_e, seat)];
    super::sandbox::apply_seat_assignments(world, sit_handle, hold_time, &assignments);
    log::info!(
        "[m42] {} npc={} seated at furn={} marker world=({:.1},{:.1},{:.1})",
        if is_sleep { "sleep" } else { "eat" },
        npc,
        furn_e,
        seat.translation.x,
        seat.translation.y,
        seat.translation.z,
    );
    true
}

fn is_sleep_marker(marker: &FurnitureMarker) -> bool {
    marker.kind == FurnitureMarkerKind::Sleep
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Furniture;
    use byroredux_core::ecs::components::GlobalTransform;

    /// #5588 — the entity → global-form-id accessor is the scripting crate's
    /// `condition::entity_global_form_id` (the inverse of
    /// `resolve_entity_by_global_form_id`, keyed the same way); no crate keeps
    /// a private copy.
    #[test]
    fn entity_global_form_id_has_one_definition() {
        for (name, src) in [
            ("systems/eat_sleep.rs", include_str!("eat_sleep.rs")),
            ("interaction.rs", include_str!("../interaction.rs")),
            ("systems/trap_hits.rs", include_str!("trap_hits.rs")),
        ] {
            let src = byroredux_core::source_scan::production_text_or_whole(src);
            assert!(
                !src.contains("fn entity_global_form_id("),
                "{name} redefines entity_global_form_id — use \
                 byroredux_scripting::condition::entity_global_form_id (#5588)"
            );
        }
    }

    /// One tick with a fresh scratch — no retry clock carries over, so a
    /// call behaves like the system's first tick. The retry cadence is
    /// exercised through [`make_eat_sleep_system`] directly.
    fn eat_sleep_system(world: &World, dt: f32) {
        eat_sleep_system_inner(world, dt, &mut EatSleepScratch::default());
    }

    fn setup() -> (World, EntityId) {
        let mut world = World::new();
        world.register::<EatBehavior>();
        world.register::<SleepBehavior>();
        world.register::<EatSleepState>();
        world.register::<EditorPlacement>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.register::<Seated>();
        world.register::<Furniture>();
        world.register::<byroredux_core::animation::AnimationPlayer>();
        world.insert_resource(SeatReservations::default());
        let actor = world.spawn();
        world.insert(
            actor,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        world.insert(actor, Transform::default());
        (world, actor)
    }

    /// #5500 — type 6 (Near Linked Reference) anchors on the actor's own
    /// XLKR edge: the registry's linked target position is the walk
    /// destination. Pre-fix the classification dumped type 6 into
    /// NearCurrentLocation and the actor dined wherever it stood.
    #[test]
    fn linked_reference_package_walks_to_the_actor_own_xlkr_target() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

        let (mut world, actor) = setup();
        world.register::<FormIdComponent>();
        let mut pool = byroredux_core::form_id::FormIdPool::default();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0100_0042),
        });
        world.insert_resource(pool);
        world.insert(actor, FormIdComponent(fid));
        byroredux_scripting::install_package_linked_references(
            &mut world,
            vec![(0x0100_0042, vec![(0, 0x0100_0099)])],
        );
        byroredux_scripting::install_package_target_positions(
            &mut world,
            vec![(0x0100_0099, Vec3::new(400.0, 0.0, 0.0))],
        );
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearLinkedReference,
                form_id: 0xAB,
            },
        );

        eat_sleep_system(&world, 1.0);
        let state = world.get::<EatSleepState>(actor).expect("state inserted");
        assert_eq!(
            state.destination,
            Vec3::new(400.0, 0.0, 0.0),
            "the anchor is the XLKR target's position, not the actor's own"
        );
    }

    /// #5500 — an unresolved NearReference anchor must NOT cache the
    /// actor's current position: the target may spawn later in the same
    /// budgeted load (references walk in authored order), so the retry
    /// next tick is the correct behaviour. Pre-fix the diner froze at its
    /// spawn point for the life of the package.
    #[test]
    fn unresolved_near_reference_waits_instead_of_caching_the_spawn_point() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                // No entity carries this form id yet.
                location: EatSleepLocation::NearReference(0x0200_7777),
                form_id: 0xAC,
            },
        );
        eat_sleep_system(&world, 1.0);
        assert!(
            world.get::<EatSleepState>(actor).is_none(),
            "an unresolvable anchor caches nothing — it retries next tick"
        );
    }

    /// #5500 — an In-Cell package searches the whole resident cell, not
    /// 512 BU of the actor: the GECK greys out the radius for In Cell and
    /// every FNV In-Cell package authors radius 0.
    #[test]
    fn in_cell_seat_search_spans_the_cell_not_the_default_radius() {
        use crate::cell_loader::{CurrentCellContext, LoadedCellIndex};
        use byroredux_core::ecs::components::Seated;
        use byroredux_plugin::esm::cell::CellData;

        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((3, 1.0))));
        // A resident interior whose form id matches the package.
        let mut cell = CellData {
            form_id: 0x0001_2345,
            is_interior: true,
            ..Default::default()
        };
        cell.editor_id = "GSProspectorSaloonInterior".to_string();
        let mut loaded = byroredux_plugin::esm::records::EsmIndex::default();
        loaded.cells.cells.insert(
            "gsprospectorsalooninterior".to_string(),
            cell,
        );
        world.insert_resource(LoadedCellIndex(std::sync::Arc::new(loaded)));
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "GSProspectorSaloonInterior".to_string(),
            esm_path: String::new(),
            masters: Vec::new(),
        });
        // A bed 2,000 BU away — far outside the 512 BU default radius.
        let bed = world.spawn();
        world.insert(bed, GlobalTransform::default());
        world.insert(
            bed,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [2000.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 2,
                    kind: FurnitureMarkerKind::Sleep,
                }],
            },
        );
        world.insert(
            actor,
            SleepBehavior {
                radius: Some(0.0),
                location: EatSleepLocation::InCell(0x0001_2345),
                form_id: 0xAD,
            },
        );

        eat_sleep_system(&world, 1.0);
        assert!(
            world.get::<Seated>(actor).is_some(),
            "an In-Cell package reaches a bed beyond the default radius (#5500)"
        );
    }

    /// Far from the destination (a "near editor location" package whose
    /// placement is 300 units away), the actor walks: an `EatSleepState`
    /// lands on first sight and the step advances `Transform` — the
    /// authoritative pose on a propagation root (#5373: the step used
    /// to write `GlobalTransform`, which the next propagation rebuilt
    /// from the unmoved local, erasing the walk every frame).
    #[test]
    fn eat_actor_far_from_destination_walks() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EditorPlacement {
                translation: Vec3::new(300.0, 0.0, 0.0),
            },
        );
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearEditorLocation,
                form_id: 0xAA,
            },
        );
        eat_sleep_system(&world, 1.0);
        let state = world.get::<EatSleepState>(actor).expect("state inserted");
        let destination = state.destination;
        assert!(
            destination.x != 0.0 || destination.z != 0.0,
            "the editor placement resolves a non-origin destination"
        );
        let moved = world.get::<Transform>(actor).expect("transform").translation;
        let before = Vec3::ZERO;
        assert_ne!(moved, before, "the actor takes a step toward the destination");
        assert!(
            (moved - destination).length() < before.distance(destination),
            "the step is toward the resolved destination"
        );
        assert!(
            world.get::<GlobalTransform>(actor).expect("global").translation == before,
            "the derived global is NOT written directly — propagation owns it"
        );
    }

    /// #5373's real-world shape: the system and transform propagation
    /// alternate every frame. The walk must survive propagation — the
    /// actor converges on its destination instead of snapping back to
    /// its spawn point every frame.
    #[test]
    fn eat_walk_survives_transform_propagation() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EditorPlacement {
                translation: Vec3::new(300.0, 0.0, 0.0),
            },
        );
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearEditorLocation,
                form_id: 0xAA,
            },
        );
        let mut propagate = byroredux_core::ecs::systems::make_transform_propagation_system();
        let mut last_distance = f32::MAX;
        for _ in 0..64 {
            eat_sleep_system(&world, 0.5);
            propagate(&world, 0.5);
            let current = world
                .get::<GlobalTransform>(actor)
                .expect("global")
                .translation;
            let destination = world
                .get::<EatSleepState>(actor)
                .expect("state")
                .destination;
            let distance = (current - destination).length();
            assert!(
                distance <= last_distance,
                "propagation must not erase the walk step (distance went {last_distance} -> {distance})"
            );
            last_distance = distance;
            if distance <= ARRIVE_RADIUS {
                break;
            }
        }
        assert!(
            last_distance <= ARRIVE_RADIUS,
            "the diner reaches its destination through alternating system+propagation frames (still {last_distance} away)"
        );
    }

    /// Arrived + furniture with a sit marker + sit clip: the actor
    /// seats through the sandbox path — `Seated` tagged, the seat
    /// reserved, the root snapped to the marker's world transform.
    /// The destination is pre-seeded so the walk phase is done.
    #[test]
    fn arrived_eat_actor_seats_at_sit_marker() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            EatBehavior {
                radius: None,
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAA,
            },
        );
        world.insert(
            actor,
            EatSleepState {
                destination: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        let furniture = world.spawn();
        world.insert(
            furniture,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [0.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 1,
                    kind: FurnitureMarkerKind::Sit,
                }],
            },
        );
        world.insert(
            furniture,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        eat_sleep_system(&world, 1.0);
        assert!(
            world.get::<Seated>(actor).is_some(),
            "the arrived diner seats through the sandbox path"
        );
        assert!(
            world
                .try_resource::<SeatReservations>()
                .is_some_and(|reservations| {
                    reservations.0.contains_key(&(furniture, 0))
                }),
            "the seat is reserved against double-claim"
        );
    }

    /// #5371 — with a real `PhysicsWorld` installed, the walk branch
    /// must take the physics resource in a scope of its own (a
    /// lock-order sink: nothing acquired under it). The detector aborts
    /// on the inverted order under `BYRO_LOCK_ORDER_CHECK=1`; same
    /// shape as travel.rs's physics regression.
    #[test]
    fn eat_walk_with_real_physics_world() {
        let (mut world, actor) = setup();
        world.insert_resource(byroredux_physics::PhysicsWorld::new());
        world.insert(
            actor,
            EditorPlacement {
                translation: Vec3::new(300.0, 0.0, 0.0),
            },
        );
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearEditorLocation,
                form_id: 0xAB,
            },
        );
        eat_sleep_system(&world, 1.0);
        let moved = world.get::<Transform>(actor).expect("transform").translation;
        assert_ne!(
            moved, Vec3::ZERO,
            "the step computed through the physics world"
        );
    }

    /// Sleep prefers sleep markers: with a sleep marker adjacent, the
    /// sleeper seats at it (the sit-pose v0 approximation documented in
    /// the module docs), even when a sit marker exists farther away.
    #[test]
    fn sleep_actor_prefers_the_sleep_marker() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            SleepBehavior {
                radius: None,
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAB,
            },
        );
        world.insert(
            actor,
            EatSleepState {
                destination: Vec3::ZERO,
            },
        );
        let bed = world.spawn();
        world.insert(
            bed,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [10.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 2,
                    kind: FurnitureMarkerKind::Sleep,
                }],
            },
        );
        world.insert(
            bed,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        eat_sleep_system(&world, 1.0);
        let seated_at = world
            .get::<Seated>(actor)
            .expect("the sleeper occupies the bed marker");
        assert_eq!(seated_at.furniture, bed);
    }

    /// #5390 — the legacy FO3/FNV shape: no marker in the cell can
    /// resolve `FurnitureMarkerKind::Sleep` (the field keys on
    /// `animation_type == 2`, a Skyrim+ `BSFurnitureMarker` field;
    /// legacy markers author 0 and all read as Sit). The sit fallback
    /// IS the legacy sleep path — the documented v0 contract until
    /// Phase C decodes `furnituremarkerNN.nif` — so the sleeper seats
    /// at the nearest sit marker, chairs included.
    #[test]
    fn legacy_sleep_actor_seats_at_the_sit_fallback() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            SleepBehavior {
                radius: None,
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAB,
            },
        );
        world.insert(
            actor,
            EatSleepState {
                destination: Vec3::ZERO,
            },
        );
        // The legacy shape: animation_type 0, translated to Sit.
        let chair = world.spawn();
        world.insert(
            chair,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [10.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 0,
                    kind: FurnitureMarkerKind::Sit,
                }],
            },
        );
        world.insert(
            chair,
            GlobalTransform {
                translation: Vec3::ZERO,
                ..Default::default()
            },
        );
        eat_sleep_system(&world, 1.0);
        let seated_at = world
            .get::<Seated>(actor)
            .expect("the legacy sleeper seats through the fallback");
        assert_eq!(
            seated_at.furniture, chair,
            "with no decodable Sleep marker the sit fallback is the sleep path"
        );
    }

    /// #5391 — "near current location" (and every type without an
    /// anchor) stays put: the destination is where the actor stands, so
    /// no random walk precedes the seat search.
    #[test]
    fn near_current_location_does_not_walk() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAA,
            },
        );
        eat_sleep_system(&world, 1.0);
        assert_eq!(
            world.get::<EatSleepState>(actor).expect("state").destination,
            Vec3::ZERO
        );
        assert_eq!(world.get::<Transform>(actor).expect("transform").translation, Vec3::ZERO);
    }

    /// #5391 — "near editor location" anchors on the authored placement,
    /// not on wherever an earlier package left the actor, and resolves
    /// to the same point again after the unsaved state is dropped (a
    /// load) — the allowlist's idempotence claim.
    #[test]
    fn near_editor_location_anchors_on_the_placement_across_reloads() {
        let (mut world, actor) = setup();
        let home = Vec3::new(-200.0, 0.0, 50.0);
        world.insert(actor, EditorPlacement { translation: home });
        // A Travel package walked the actor to a bar 900 units away.
        world.insert(
            actor,
            GlobalTransform {
                translation: Vec3::new(900.0, 0.0, 0.0),
                ..Default::default()
            },
        );
        world.insert(
            actor,
            SleepBehavior {
                radius: None,
                location: EatSleepLocation::NearEditorLocation,
                form_id: 0xAA,
            },
        );
        eat_sleep_system(&world, 0.1);
        assert_eq!(world.get::<EatSleepState>(actor).expect("state").destination, home);
        if let Some(mut states) = world.query_mut::<EatSleepState>() {
            states.remove(actor);
        }
        eat_sleep_system(&world, 0.1);
        assert_eq!(
            world.get::<EatSleepState>(actor).expect("state").destination,
            home,
            "the re-resolved destination after a load is the same point"
        );
    }

    /// #5391 — an `InCell` package whose cell is not the resident
    /// interior idles: no destination, no walk.
    #[test]
    fn in_cell_package_idles_when_its_cell_is_not_resident() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: None,
                location: EatSleepLocation::InCell(0x0001_2345),
                form_id: 0xAA,
            },
        );
        eat_sleep_system(&world, 1.0);
        assert!(world.get::<EatSleepState>(actor).is_none());
        assert_eq!(world.get::<Transform>(actor).expect("transform").translation, Vec3::ZERO);
    }

    /// #5527 — an actor whose anchor does not resolve used to re-resolve on
    /// every frame for the whole package window (a map probe plus, for
    /// `InCell`, a lowercase cell-key allocation). It now waits out
    /// [`ANCHOR_RETRY_SECS`]: an anchor that becomes resolvable mid-wait is
    /// not found until the clock expires, then is, and the clock is dropped.
    #[test]
    fn unresolved_anchor_retries_on_a_coarse_cadence() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

        let (mut world, actor) = setup();
        world.register::<FormIdComponent>();
        let mut pool = byroredux_core::form_id::FormIdPool::default();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(0x0100_0042),
        });
        world.insert_resource(pool);
        world.insert(actor, FormIdComponent(fid));
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearLinkedReference,
                form_id: 0xAB,
            },
        );
        let mut scratch = EatSleepScratch::default();

        // No XLKR edge registered: the anchor is unresolvable and arms the clock.
        eat_sleep_system_inner(&world, 0.1, &mut scratch);
        assert!(world.get::<EatSleepState>(actor).is_none());
        assert_eq!(scratch.anchor_retry.get(&actor), Some(&ANCHOR_RETRY_SECS));

        // The edge appears, but the clock has not expired: not looked at.
        byroredux_scripting::install_package_linked_references(
            &mut world,
            vec![(0x0100_0042, vec![(0, 0x0100_0099)])],
        );
        byroredux_scripting::install_package_target_positions(
            &mut world,
            vec![(0x0100_0099, Vec3::new(400.0, 0.0, 0.0))],
        );
        eat_sleep_system_inner(&world, 0.5, &mut scratch);
        assert!(
            world.get::<EatSleepState>(actor).is_none(),
            "still inside the anchor retry wait"
        );

        // The clock expires: the anchor resolves and the clock is dropped.
        eat_sleep_system_inner(&world, ANCHOR_RETRY_SECS, &mut scratch);
        assert_eq!(
            world.get::<EatSleepState>(actor).expect("anchor resolved").destination,
            Vec3::new(400.0, 0.0, 0.0)
        );
        assert!(scratch.anchor_retry.is_empty(), "a resolved actor keeps no anchor clock");
    }

    /// #5449 — an arrived actor with nowhere to sit used to re-gather every
    /// `Furniture` entity in the world each frame, forever. It now waits
    /// out [`SEAT_RETRY_SECS`] between attempts: a marker that appears
    /// mid-wait is not found until the clock expires, then is found, and
    /// the retry clock is dropped once the actor is seated.
    #[test]
    fn unseatable_arrived_actor_retries_on_a_coarse_cadence() {
        let (mut world, actor) = setup();
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAA,
            },
        );
        let mut scratch = EatSleepScratch::default();

        // No furniture in the cell: the first attempt fails and arms the clock.
        eat_sleep_system_inner(&world, 0.1, &mut scratch);
        assert!(world.get::<Seated>(actor).is_none());
        assert_eq!(scratch.seat_retry.get(&actor), Some(&SEAT_RETRY_SECS));

        // A chair appears, but the clock has not expired: no new gather.
        let chair = world.spawn();
        world.insert(
            chair,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [0.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 1,
                    kind: FurnitureMarkerKind::Sit,
                }],
            },
        );
        world.insert(chair, GlobalTransform::default());
        eat_sleep_system_inner(&world, 0.5, &mut scratch);
        assert!(world.get::<Seated>(actor).is_none(), "still inside the retry wait");
        assert!(!scratch.seats.sit_ready, "the wait must skip the furniture gather");

        // The clock expires: the retry finds the chair and clears the clock.
        eat_sleep_system_inner(&world, SEAT_RETRY_SECS, &mut scratch);
        assert!(world.get::<Seated>(actor).is_some(), "the retry seats the actor");
        assert!(scratch.seat_retry.is_empty(), "a seated actor keeps no retry clock");
    }

    /// #5707 — a seated sleeper stays under its behavior component for the
    /// whole package window; it must not be re-gathered each frame, its
    /// stale retry clocks are pruned, and the gather buffers are the
    /// scratch's own (kept, empty, between frames).
    #[test]
    fn seated_actor_is_not_gathered_and_its_clocks_are_pruned() {
        let (mut world, actor) = setup();
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearCurrentLocation,
                form_id: 0xAA,
            },
        );
        let mut scratch = EatSleepScratch::default();
        eat_sleep_system_inner(&world, 0.1, &mut scratch);
        assert_eq!(scratch.seat_retry.get(&actor), Some(&SEAT_RETRY_SECS));
        assert!(scratch.actors.is_empty(), "the list is handed back drained");
        // A second frame has a clock to prune, so the membership set fills.
        eat_sleep_system_inner(&world, 0.1, &mut scratch);
        assert!(scratch.live.contains(&actor), "an unseated actor is live");

        let chair = world.spawn();
        let seated = Seated {
            furniture: chair,
            animation_restore: Default::default(),
        };
        world.insert(actor, seated);
        eat_sleep_system_inner(&world, 0.1, &mut scratch);
        assert!(scratch.live.is_empty(), "a seated actor never enters the gather");
        assert!(scratch.seat_retry.is_empty(), "its retry clock is pruned");
    }

    /// A floor topped at y=0 plus a tall wall across x=0 — a straight line
    /// from x<0 to x>0 crosses solid architecture the capsule cannot step.
    fn walled_physics() -> byroredux_physics::PhysicsWorld {
        let mut pw = byroredux_physics::PhysicsWorld::new();
        pw.spawn_fixed_cuboid(Vec3::new(2000.0, 20.0, 2000.0), Vec3::new(0.0, -20.0, 0.0));
        pw.spawn_fixed_cuboid(Vec3::new(10.0, 300.0, 2000.0), Vec3::new(0.0, 300.0, 0.0));
        pw
    }

    /// #5452 — an Eat actor whose straight line to the authored furniture
    /// anchor crosses a wall used to press into it forever (`blocked`
    /// discarded, completion a pure distance test), never reaching
    /// `seat_at_marker`. Held against the wall for the stuck window while
    /// inside its seat-search radius it now seats at the in-range chair
    /// from where it stands.
    #[test]
    fn walk_blocked_by_a_wall_gives_up_and_seats_within_the_search_radius() {
        let (mut world, actor) = setup();
        world.insert_resource(walled_physics());
        world.insert_resource(SandboxSitClip(Some((7, 0.5))));
        let start = Vec3::new(-400.0, 0.0, 0.0);
        world.insert(actor, GlobalTransform { translation: start, ..Default::default() });
        world.insert(actor, Transform { translation: start, ..Default::default() });
        // The authored anchor sits behind the wall: 800 BU away at the
        // start, 434 BU from where the capsule stops (inside the 512 radius).
        world.insert(actor, EditorPlacement { translation: Vec3::new(400.0, 0.0, 0.0) });
        world.insert(
            actor,
            EatBehavior {
                radius: Some(512.0),
                location: EatSleepLocation::NearEditorLocation,
                form_id: 0xAB,
            },
        );
        let chair = world.spawn();
        world.insert(
            chair,
            Furniture {
                markers: vec![FurnitureMarker {
                    local_offset: [0.0, 0.0, 0.0],
                    heading_z_radians: None,
                    animation_type: 1,
                    kind: FurnitureMarkerKind::Sit,
                }],
            },
        );
        world.insert(
            chair,
            GlobalTransform { translation: Vec3::new(-200.0, 0.0, 0.0), ..Default::default() },
        );

        let mut tick = make_eat_sleep_system();
        for _ in 0..200 {
            // Stand-in for transform propagation: the derived global
            // follows the authoritative local.
            let local = world.get::<Transform>(actor).expect("transform").translation;
            world.insert(actor, GlobalTransform { translation: local, ..Default::default() });
            tick(&world, 0.1);
            if world.get::<Seated>(actor).is_some() {
                break;
            }
        }
        let seated = world
            .get::<Seated>(actor)
            .expect("a wall-blocked diner must give up the walk and seat in range");
        assert_eq!(seated.furniture, chair);
    }
}
