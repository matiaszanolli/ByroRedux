//! EX-16 acceptance criterion 6 (#3806) — boundary + soak harness for an
//! actor's NAVM path across a cell unload/reload cycle.
//!
//! # What this covers, and why only half the criterion
//!
//! The criterion reads "boundary and soak tests cover unload/reload while
//! an actor path crosses a cell edge." It was filed blocked on two things:
//!
//! - **#3802** — cross-tile NAVM path connectivity. *Landed* (`494f18ad`).
//!   An actor path that "crosses a cell edge" is now a real, computable
//!   object: [`path_from_resident_tiles`] joins two tiles through the
//!   geometric portal their shared border edge forms.
//! - **#3803** — actor/package suspend, migrate, resume across stream
//!   boundaries. *Not landed.* It was closed as a duplicate of **#3299**,
//!   which is still open. `unload_cell_inner` still despawns every
//!   cell-owned entity wholesale, so there is no migration to observe.
//!
//! So the half of the criterion with substrate underneath it — *the path
//! must not survive its tiles* — is tested here in full. The half that
//! needs #3299 — *the actor must not duplicate, and must not lose package
//! state, when its own cell unloads* — has nothing to assert against yet:
//! today the actor is simply despawned, and a test pinning that would pin
//! the very behaviour #3299 exists to replace. When #3299 lands, its
//! `resume`-side tests belong in this file, next to these.
//!
//! The actor entity below is therefore deliberately **not** stamped into
//! either cell's root range. That models an actor whose own residency
//! outlives the tiles it is pathing over, which is exactly the case this
//! criterion cares about, and it is the one shape that stays valid on both
//! sides of #3299.
//!
//! # Why the unload is re-driven rather than called
//!
//! [`super::unload_cell`] takes `&mut VulkanContext`; there is no device in
//! `cargo test`. But everything that NAVM residency actually depends on is
//! device-free, and [`crate::components::NavmeshTile`]'s own doc says why:
//! it "carries no GPU handle" and rides the generic
//! `stamp_cell_root_range` → `CellRootIndex` → `unload_cell` chain. So
//! [`unload_navm_cell`] below calls the *real* [`drain_cell_victims`],
//! the *real* `despawn_batch`, and the *real*
//! [`bump_navmesh_residency`](crate::components::bump_navmesh_residency) —
//! the only two steps of `unload_cell_inner` a `NavmeshTile` ever reaches.
//! The mesh/texture/item/Rapier phases it skips are no-ops for these rows.
//!
//! That mirror is a liability if production drifts, so
//! [`the_harness_mirrors_the_unload_steps_navm_residency_depends_on`] pins
//! both steps at the source.

use byroredux_core::ecs::components::CellRoot;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{OwnershipSnapshot, Transform, World};
use byroredux_core::math::Vec3;
use std::collections::VecDeque;

use super::load::stamp_cell_root;
use super::unload::drain_cell_victims;
use crate::components::{
    bump_navmesh_residency, navmesh_residency_generation, spawn_navmesh_tiles, CellRootIndex,
    NavPath, NavmeshTile,
};
use crate::systems::navmesh_path::test_tiles::{adjacent_quad, two_triangle_quad};
use crate::systems::navmesh_path::{path_from_resident_tiles, resolve_cached_waypoints};

/// Form ID for the far tile. `two_triangle_quad` leaves `form_id` at its
/// `NavmRecord::default()` zero, and `CrossTileGraph::build` buckets border
/// edges by `mesh_form`, so the two tiles must not collide on it.
const FAR_TILE_FORM: u32 = 0x0000_0B0B;

/// The near tile spans `[0,10] x [0,10]`; the far tile `[10,20] x [0,10]`.
/// `NEAR_POINT` sits on the near tile's triangle 0 (`z <= x`), `FAR_POINT`
/// on the far tile's triangle 0 (`z <= x - 10`). No single tile contains
/// both, which is what forces `path_from_resident_tiles` past its same-tile
/// fast path and onto the cross-tile graph.
const NEAR_POINT: Vec3 = Vec3::new(7.0, 0.0, 3.0);
const FAR_POINT: Vec3 = Vec3::new(17.0, 0.0, 3.0);

/// The X coordinate at which the near tile ends and the far tile begins.
const CELL_EDGE_X: f32 = 10.0;

/// Frozen-goal posture (`travel`/`guard`/`escort`'s lead phase). Chosen
/// deliberately: it is the posture under which goal distance alone can
/// *never* invalidate a cached path, so every invalidation these tests
/// observe is attributable to the residency generation and nothing else.
const FROZEN_GOAL_THRESHOLD: f32 = 0.0;

/// A world with the resources and storages a cell load/unload round trip
/// touches, and nothing else.
fn streaming_world() -> World {
    let mut world = World::new();
    world.insert_resource(CellRootIndex::new());
    world.register::<NavmeshTile>();
    world.register::<NavPath>();
    world.register::<Transform>();
    world.register::<CellRoot>();
    world
}

/// Make one cell's NAVM tiles resident, the way both production loaders do:
/// spawn inside a captured entity window, then stamp that window onto a
/// fresh cell root so the generic reclaim chain owns the teardown.
///
/// Mirrors `cell_loader::load`'s ordering — `spawn_navmesh_tiles` runs
/// *before* `last_entity` is captured, which is the comment at its call
/// site and the reason NAVM tiles need no bespoke reclaim path.
fn load_navm_cell(
    world: &mut World,
    navmeshes: &[byroredux_plugin::esm::records::NavmRecord],
) -> EntityId {
    let first_entity = world.next_entity_id();
    spawn_navmesh_tiles(world, navmeshes);
    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);
    cell_root
}

/// Drive the two steps of `unload_cell_inner` that a `NavmeshTile` reaches.
/// Returns the victim count so callers can assert the cell was actually
/// tracked rather than silently draining empty.
fn unload_navm_cell(world: &mut World, cell_root: EntityId) -> usize {
    let victims = drain_cell_victims(world, cell_root);
    let victim_count = victims.len();
    world.despawn_batch(victims);
    if victim_count > 0 {
        bump_navmesh_residency(world);
    }
    victim_count
}

/// Resolve the waypoint queue an actor would walk this tick, reading its
/// cached [`NavPath`] and the live tile set out of the world exactly as a
/// locomotion system does.
fn resolve_for(world: &World, actor: EntityId, goal: Vec3) -> (Vec3, VecDeque<Vec3>) {
    let cached = world.query::<NavPath>().and_then(|q| q.get(actor).cloned());
    let current = world
        .query::<Transform>()
        .and_then(|q| q.get(actor).map(|t| t.translation))
        .expect("the actor must have a Transform");
    let tiles = world.query::<NavmeshTile>();
    resolve_cached_waypoints(
        cached.as_ref(),
        tiles.as_ref(),
        current,
        goal,
        FROZEN_GOAL_THRESHOLD,
        navmesh_residency_generation(world),
    )
}

/// Write a resolved result back onto the actor, the way every NAVM-pathed
/// procedure does — stamping the *live* generation, and `effective_goal`
/// rather than the raw goal argument.
fn store_path(world: &mut World, actor: EntityId, effective_goal: Vec3, waypoints: VecDeque<Vec3>) {
    let residency_generation = navmesh_residency_generation(world);
    world.insert(
        actor,
        NavPath {
            goal: effective_goal,
            residency_generation,
            waypoints,
        },
    );
}

/// An actor standing on the near tile with a live cached path to the far
/// tile, plus both cell roots. The returned path is non-empty and crosses
/// the edge — asserted here so every test below inherits a non-vacuous
/// starting state.
fn actor_mid_path_across_the_edge(world: &mut World) -> (EntityId, EntityId, EntityId) {
    let near_root = load_navm_cell(world, &[two_triangle_quad()]);
    let far_root = load_navm_cell(world, &[adjacent_quad(FAR_TILE_FORM, CELL_EDGE_X)]);

    let actor = world.spawn();
    world.insert(actor, Transform::from_translation(NEAR_POINT));

    let (effective_goal, waypoints) = resolve_for(world, actor, FAR_POINT);
    assert!(
        !waypoints.is_empty(),
        "fixture precondition: both tiles resident must yield a cross-tile path"
    );
    assert!(
        waypoints.iter().any(|w| w.x >= CELL_EDGE_X),
        "fixture precondition: the path must actually cross the cell edge"
    );
    store_path(world, actor, effective_goal, waypoints);

    (actor, near_root, far_root)
}

/// Waypoints that lie in the far tile's footprint — i.e. the part of a
/// path that becomes a dangling reference the moment that cell unloads.
fn waypoints_beyond_the_edge(waypoints: &VecDeque<Vec3>) -> usize {
    waypoints.iter().filter(|w| w.x >= CELL_EDGE_X).count()
}

#[test]
fn both_cells_resident_yields_a_path_that_crosses_the_cell_edge() {
    let mut world = streaming_world();
    let (actor, _near, _far) = actor_mid_path_across_the_edge(&mut world);

    let cached = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("the actor holds a cached path");

    assert_eq!(cached.goal, FAR_POINT);
    assert_eq!(
        cached.waypoints.back().copied(),
        Some(FAR_POINT),
        "a resolved path always ends at its goal"
    );
    assert!(
        waypoints_beyond_the_edge(&cached.waypoints) > 0,
        "the cached path must own at least one waypoint inside the far cell"
    );
}

#[test]
fn unloading_the_far_cell_invalidates_the_actors_cached_cross_edge_path() {
    let mut world = streaming_world();
    let (actor, _near, far_root) = actor_mid_path_across_the_edge(&mut world);

    let before = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("cached path");
    let generation_before = navmesh_residency_generation(&world);

    assert!(
        unload_navm_cell(&mut world, far_root) > 0,
        "far cell tracked"
    );

    assert_ne!(
        navmesh_residency_generation(&world),
        generation_before,
        "a cell teardown that took NAVM rows with it must bump residency"
    );

    // Non-vacuity: under the frozen-goal posture the cached path's goal is
    // bit-identically the goal being requested, so the distance half of the
    // cache predicate is a *hit*. The generation stamp is the only thing
    // that can invalidate here — if this test ever passes for another
    // reason, this assertion is what will have stopped being true.
    assert!(
        before.goal.distance(FAR_POINT) <= FROZEN_GOAL_THRESHOLD,
        "goal distance alone would have been a cache hit"
    );

    let (_goal, waypoints) = resolve_for(&world, actor, FAR_POINT);
    assert_ne!(
        waypoints, before.waypoints,
        "the stale cross-edge path must not be replayed after its far cell unloaded"
    );
}

#[test]
fn the_stale_path_is_not_replayed_into_the_unloaded_cell() {
    let mut world = streaming_world();
    let (actor, _near, far_root) = actor_mid_path_across_the_edge(&mut world);

    let before = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("cached path");
    assert!(
        waypoints_beyond_the_edge(&before.waypoints) > 0,
        "precondition: the live path pointed into the cell about to unload"
    );

    unload_navm_cell(&mut world, far_root);
    let (_goal, waypoints) = resolve_for(&world, actor, FAR_POINT);

    assert_eq!(
        waypoints_beyond_the_edge(&waypoints),
        0,
        "no waypoint may survive inside a cell that is no longer resident"
    );
    assert!(
        waypoints.is_empty(),
        "with the goal's tile gone the goal no longer localizes, so the \
         honest answer is the cached-negative empty path, not a partial route"
    );
}

#[test]
fn unloading_the_actors_own_cell_invalidates_rather_than_stranding_the_path() {
    let mut world = streaming_world();
    let (actor, near_root, _far) = actor_mid_path_across_the_edge(&mut world);

    let before = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("cached path");

    // The mirror case: the actor stands still and the ground under it
    // streams out. `current` stops localizing, so the cross-tile search
    // has no start node.
    unload_navm_cell(&mut world, near_root);
    let (_goal, waypoints) = resolve_for(&world, actor, FAR_POINT);

    assert_ne!(
        waypoints, before.waypoints,
        "the cached path must not persist"
    );
    assert!(
        waypoints.is_empty(),
        "an actor whose own tile unloaded has no start node to path from"
    );
}

#[test]
fn reloading_the_far_cell_restores_the_boundary_crossing_path() {
    let mut world = streaming_world();
    let (actor, _near, far_root) = actor_mid_path_across_the_edge(&mut world);

    let original = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("cached path");

    unload_navm_cell(&mut world, far_root);
    let (goal_while_gone, gone) = resolve_for(&world, actor, FAR_POINT);
    store_path(&mut world, actor, goal_while_gone, gone);

    let _reloaded_root = load_navm_cell(&mut world, &[adjacent_quad(FAR_TILE_FORM, CELL_EDGE_X)]);
    let (_goal, restored) = resolve_for(&world, actor, FAR_POINT);

    assert_eq!(
        restored, original.waypoints,
        "the same two tiles must produce the same crossing after a reload"
    );
}

/// Enough cycles that a per-cycle leak of even one row is unmistakable,
/// while staying instant in `cargo test`.
const SOAK_CYCLES: usize = 64;

#[test]
fn soak_repeated_boundary_crossings_leak_no_ecs_rows() {
    let mut world = streaming_world();
    let (actor, _near, mut far_root) = actor_mid_path_across_the_edge(&mut world);

    let original = world
        .query::<NavPath>()
        .and_then(|q| q.get(actor).cloned())
        .expect("cached path");

    let mut settled = OwnershipSnapshot::default();
    crate::ownership_sample::sample_ecs_owners(&world, &mut settled);

    for cycle in 0..SOAK_CYCLES {
        // ── stream out ────────────────────────────────────────────────
        unload_navm_cell(&mut world, far_root);
        let (goal_while_gone, gone) = resolve_for(&world, actor, FAR_POINT);
        assert!(
            gone.is_empty(),
            "cycle {cycle}: path must drop while the far cell is gone"
        );
        store_path(&mut world, actor, goal_while_gone, gone);

        let mut out = OwnershipSnapshot::default();
        crate::ownership_sample::sample_ecs_owners(&world, &mut out);
        assert_eq!(
            out.navm_tiles_resident, 1,
            "cycle {cycle}: exactly the near tile stays resident mid-cycle"
        );

        // ── stream back in ────────────────────────────────────────────
        far_root = load_navm_cell(&mut world, &[adjacent_quad(FAR_TILE_FORM, CELL_EDGE_X)]);
        let (goal, restored) = resolve_for(&world, actor, FAR_POINT);
        assert_eq!(
            restored, original.waypoints,
            "cycle {cycle}: the crossing must come back identical"
        );
        store_path(&mut world, actor, goal, restored);

        let mut back = OwnershipSnapshot::default();
        crate::ownership_sample::sample_ecs_owners(&world, &mut back);
        assert_eq!(
            (
                back.navm_tiles_resident,
                back.cell_root_rows,
                back.cell_root_index_entries,
                back.transform_rows
            ),
            (
                settled.navm_tiles_resident,
                settled.cell_root_rows,
                settled.cell_root_index_entries,
                settled.transform_rows
            ),
            "cycle {cycle}: a settled cycle must return every ECS owner class \
             to its pre-cycle count"
        );
        settled = back;
    }
}

#[test]
fn the_soak_leak_check_reads_row_counts_because_entity_ids_never_recycle() {
    let mut world = streaming_world();
    let (actor, _near, mut far_root) = actor_mid_path_across_the_edge(&mut world);

    let mut before = OwnershipSnapshot::default();
    crate::ownership_sample::sample_ecs_owners(&world, &mut before);

    for _ in 0..8 {
        unload_navm_cell(&mut world, far_root);
        far_root = load_navm_cell(&mut world, &[adjacent_quad(FAR_TILE_FORM, CELL_EDGE_X)]);
    }
    let _ = actor;

    let mut after = OwnershipSnapshot::default();
    crate::ownership_sample::sample_ecs_owners(&world, &mut after);

    // `World::despawn_batch` documents that it "does not reclaim entity
    // IDs", and `next_entity_id` is a high-water mark, not a census. A soak
    // assertion written against `entities_spawned` would therefore fail on
    // a perfectly clean run — pinned here so nobody tightens the check
    // above into that trap.
    assert!(
        after.entities_spawned > before.entities_spawned,
        "the high-water mark is expected to climb across cycles"
    );
    assert_eq!(
        after.navm_tiles_resident, before.navm_tiles_resident,
        "row counts are the leak signal, and they must be flat"
    );
}

#[test]
fn the_harness_mirrors_the_unload_steps_navm_residency_depends_on() {
    let src = include_str!("unload.rs");
    let fn_start = src
        .find("fn unload_cell_inner(")
        .expect("unload_cell_inner must still exist");
    let fn_end = src[fn_start..]
        .find("\nfn finish_unload_batch")
        .map(|rel| fn_start + rel)
        .expect("finish_unload_batch must still follow unload_cell_inner");
    let body = &src[fn_start..fn_end];

    // `unload_navm_cell` above stands in for these two lines. If production
    // stops doing either — or starts doing something else a `NavmeshTile`
    // row can reach — this harness is testing a shape the engine no longer
    // has, and that must break loudly rather than keep passing.
    assert!(
        body.contains("world.despawn_batch(victims)"),
        "unload_cell_inner must still despawn its victims in one batch"
    );
    assert!(
        body.contains("bump_navmesh_residency(world)"),
        "unload_cell_inner must still bump NAVM residency after a despawn"
    );
    assert!(
        body.contains("drain_cell_victims("),
        "unload_cell_inner must still source its victims from the shared drain"
    );
}

#[test]
fn a_cross_tile_path_needs_both_tiles_in_the_same_resident_set() {
    // Guards the fixture's own premise: the two quads are adjacent because
    // they share an exact border-edge vertex pair, and that is what
    // `CrossTileGraph` buckets on. A future edit that nudges either
    // footprint would leave every test above asserting over two
    // unconnected tiles — which would still "pass" the invalidation checks
    // for entirely the wrong reason.
    let mut world = streaming_world();
    load_navm_cell(&mut world, &[two_triangle_quad()]);

    let tiles = world.query::<NavmeshTile>().expect("NavmeshTile storage");
    assert!(
        path_from_resident_tiles(&tiles, NEAR_POINT, FAR_POINT).is_none(),
        "with only the near tile resident there is no route across the edge"
    );
    drop(tiles);

    load_navm_cell(&mut world, &[adjacent_quad(FAR_TILE_FORM, CELL_EDGE_X)]);
    let tiles = world.query::<NavmeshTile>().expect("NavmeshTile storage");
    assert!(
        path_from_resident_tiles(&tiles, NEAR_POINT, FAR_POINT).is_some(),
        "adding the far tile must open the portal the fixtures document"
    );
}
