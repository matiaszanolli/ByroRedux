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
//! The walk is the same straight-line `step_toward` locomotion the
//! force-greet bridge uses (KCC-backed, single resident NAVM tile) —
//! the v0 pathing caveat the other M42 procedures document applies here
//! too. Seating is one-shot: `Seated` skips the actor on later ticks,
//! and the package handover's `clear_ambient_behavior` un-seats
//! (restoring the pre-park animation, #3333) exactly as Sandbox does.

use byroredux_core::ecs::components::{
    EatBehavior, EatSleepLocation, EatSleepState, EditorPlacement, FurnitureMarker,
    FurnitureMarkerKind, GlobalTransform, Seated, SleepBehavior, Transform,
};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;

use byroredux_scripting::PackageTargetRegistry;

use crate::components::{SandboxSitClip, SeatReservations};

/// The one-shot walk destination's arrival threshold (world units) —
/// inside this, the actor stops walking and seats. Small relative to
/// the seat-search radius so the seat pick, not the walk, decides the
/// final resting furniture.
const ARRIVE_RADIUS: f32 = 64.0;

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
}

pub(crate) fn eat_sleep_system(world: &World, dt: f32) {
    let mut actors: Vec<EatSleepActor> = Vec::new();
    if let Some(query) = world.query::<EatBehavior>() {
        for (npc, behavior) in query.iter() {
            actors.push(EatSleepActor {
                npc,
                kind: EatOrSleep::Eat,
                radius: behavior.radius,
                location: behavior.location,
            });
        }
    }
    if let Some(query) = world.query::<SleepBehavior>() {
        for (npc, behavior) in query.iter() {
            actors.push(EatSleepActor {
                npc,
                kind: EatOrSleep::Sleep,
                radius: behavior.radius,
                location: behavior.location,
            });
        }
    }
    if actors.is_empty() {
        return;
    }
    // One-shot guard: a seated actor is done (the package handover
    // un-seats). Scoped so the destination/seat work below never holds
    // two same-type guards.
    {
        let seated_q = world.query::<Seated>();
        actors.retain(|actor| {
            !seated_q
                .as_ref()
                .is_some_and(|seated| seated.contains(actor.npc))
        });
    }
    if actors.is_empty() {
        return;
    }

    for EatSleepActor {
        npc,
        kind,
        radius,
        location,
    } in actors
    {
        // One-shot walk destination: resolve on first sight, reuse
        // after. Inserted through a scoped write so no read guard is
        // held across it.
        let destination = match world.get::<EatSleepState>(npc).map(|state| state.destination) {
            Some(destination) => destination,
            None => {
                // #5391 — an `InCell` package whose cell is not resident
                // idles (resolved again next tick, no walk, no seat).
                let Some(destination) = resolve_anchor(world, npc, location) else {
                    continue;
                };
                if let Some(mut states) = world.query_mut::<EatSleepState>() {
                    states.insert(npc, EatSleepState { destination });
                }
                destination
            }
        };
        let current = world
            .get::<GlobalTransform>(npc)
            .map(|transform| transform.translation)
            .unwrap_or_default();
        let flat = Vec3::new(destination.x - current.x, 0.0, destination.z - current.z);
        if flat.length() > ARRIVE_RADIUS {
            // Still walking — one KCC-backed step toward the destination.
            // #5373 — the step writes `Transform` (the authoritative
            // world pose on a propagation root), never
            // `GlobalTransform`: the derived global is rebuilt from the
            // local on the next propagation, so writing it erased the
            // step every frame and the diner never reached its marker.
            // Same write shape as travel/wander's pass 2.
            let speed = world
                .get::<crate::components::WalkSpeed>(npc)
                .map(|speed| speed.0)
                .unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
            let rotation = world
                .get::<Transform>(npc)
                .map(|transform| transform.rotation)
                .unwrap_or_default();
            // #5371 — `PhysicsWorld` is a lock-order sink (nothing
            // acquired under it, docs/engine/ecs.md): take it in its
            // own scope for the step computation, then apply the write
            // after it drops. The single-block shape held the guard
            // across the `Transform` write, inverting the order
            // production records.
            let (new_pos, new_rotation) = {
                let physics_guard =
                    world.try_resource::<byroredux_physics::PhysicsWorld>();
                crate::systems::locomotion::step_toward(
                    current,
                    rotation,
                    destination,
                    dt,
                    speed,
                    physics_guard.as_deref(),
                )
            };
            if let Some(mut transforms) = world.query_mut::<Transform>() {
                if let Some(transform) = transforms.get_mut(npc) {
                    transform.translation = new_pos;
                    if let Some(rotation) = new_rotation {
                        transform.rotation = rotation;
                    }
                }
            }
            continue;
        }
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
        seat_at_marker(world, npc, kind, Some(search_radius));
    }
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
fn resolve_anchor(world: &World, npc: EntityId, location: EatSleepLocation) -> Option<Vec3> {
    let current = world.get::<GlobalTransform>(npc)?.translation;
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

/// An entity's global (load-order) form id, or `None` when it carries no
/// `FormIdComponent` or the pool cannot resolve it — the same keying
/// `resolve_entity_by_global_form_id` searches (#3278's helper shape).
fn entity_global_form_id(world: &World, entity: EntityId) -> Option<u32> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let component = world.get::<FormIdComponent>(entity)?;
    let pool = world.try_resource::<FormIdPool>()?;
    pool.resolve(component.0).map(|pair| pair.local.0)
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
/// sandbox seating path (reservation, root snap, sit-enter park).
fn seat_at_marker(world: &World, npc: EntityId, kind: EatOrSleep, radius: Option<f32>) {
    // #5495 — creatures never take furniture: the per-cell sit clip's
    // channels are human-rig bones, so seating a creature freezes it in
    // a human pose at the marker for the life of the cell. The actor has
    // already walked to the authored location; it just stands there.
    if world
        .get::<byroredux_core::ecs::components::CreatureActor>(npc)
        .is_some()
    {
        return;
    }
    // No sit-enter clip → no seating path (Skyrim+/Havok games, or the
    // clip wasn't archived) — the actor has already walked to the
    // authored location and simply stands there, the same posture as a
    // sandbox actor in a clip-less cell.
    let Some((sit_handle, hold_time)) = world.try_resource::<SandboxSitClip>().and_then(|r| r.0)
    else {
        return;
    };
    let is_sleep = matches!(kind, EatOrSleep::Sleep);
    let mut collected: Vec<((EntityId, u32), GlobalTransform)> = Vec::new();
    if is_sleep {
        super::sandbox::collect_marker_seats(world, &mut collected, is_sleep_marker);
    }
    if collected.is_empty() {
        // #5390 — on FO3/FNV this is not "no bed found" but "no bed can
        // EVER resolve": legacy markers author animation_type 0 and the
        // translate boundary reads them all as Sit (see the module doc).
        // The fallback is the legacy sleep path by design until Phase C
        // decodes furnituremarkerNN.
        if is_sleep {
            log::debug!(
                "[m42] sleep npc={npc}: no decodable Sleep marker in range — \
                 legacy cells author none (animation_type 0 over-match, #5390); \
                 seating at the nearest sit marker"
            );
        }
        super::sandbox::collect_marker_seats(world, &mut collected, super::sandbox::is_sit_marker);
    }
    if collected.is_empty() {
        return;
    }
    let Some(current) = world.get::<GlobalTransform>(npc).map(|t| t.translation) else {
        return;
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
            &collected,
            reservations,
            radius.unwrap_or(super::sandbox::SEAT_SEARCH_RADIUS),
        )
    };
    let Some(((furn_e, marker_idx), seat)) = seat else {
        return;
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
}

fn is_sleep_marker(marker: &FurnitureMarker) -> bool {
    marker.kind == FurnitureMarkerKind::Sleep
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Furniture;
    use byroredux_core::ecs::components::GlobalTransform;

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
            editor_id: String::new(),
            display_name: None,
            references: Vec::new(),
            is_interior: true,
            show_sky: None,
            grid: None,
            lighting: None,
            landscape: None,
            water_height: None,
            water_height_is_explicit: false,
            image_space_form: None,
            water_type_form: None,
            acoustic_space_form: None,
            music_type_form: None,
            music_type_enum: None,
            climate_override: None,
            location_form: None,
            encounter_zone_form: None,
            regions: Vec::new(),
            lighting_template_form: None,
            ownership: None,
            regional_color_override: None,
            precombined_mesh_hashes: Vec::new(),
            absorbed_ref_bakes: Vec::new(),
            navmeshes: Vec::new(),
            pathgrids: Vec::new(),
            deleted_refs: Vec::new(),
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
}
