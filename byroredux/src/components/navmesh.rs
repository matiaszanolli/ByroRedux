//! Navmesh tile components, residency generation and nav paths. (#5467 split from `components.rs`)

use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::math::Vec3;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use std::collections::VecDeque;

/// A resident navmesh tile — CPU-only pathing data, no GPU footprint.
///
/// EX-16 item 2 (#2372): makes `NAVM` records "resident" alongside their
/// owning interior cell / exterior grid tile, so a future pathfinder (item
/// 3, deliberately scoped as its own follow-up issue — "pathfinding needs
/// resident tiles first") has something to query. Today this is pure
/// residency plumbing with zero consumers — but unlike the
/// `PersistentRefIndex` family (deleted in #3884 once EX-16 closed
/// without wiring it), this one's consumer is a named, still-open
/// follow-up rather than a gate that has already shut.
///
/// Spawned as a plain entity within the same `first_entity..last_entity`
/// range every other cell-owned entity is spawned in
/// (`cell_loader::load::load_cell_with_masters` for interiors,
/// `ExteriorCellApplyJob::begin` for exterior tiles), so it rides the
/// existing `stamp_cell_root_range` → `CellRootIndex` → `unload_cell`
/// teardown chain automatically — no bespoke reclaim path needed the way
/// `LodBlock`/`ObjectLodBlock` need one, because there's no GPU handle
/// here to leak.
pub(crate) struct NavmeshTile(pub(crate) byroredux_plugin::esm::records::NavmRecord);

impl Component for NavmeshTile {
    type Storage = SparseSetStorage<Self>;
}

/// Spawn one [`NavmeshTile`] entity per `navmeshes` entry. Shared by the
/// interior and exterior cell loaders so the spawn shape can't drift
/// between them. Caller is responsible for calling this within the same
/// `first_entity..last_entity` window its `stamp_cell_root_range` call
/// covers — this function only spawns entities, it does not stamp them.
pub(crate) fn spawn_navmesh_tiles(
    world: &mut byroredux_core::ecs::World,
    navmeshes: &[byroredux_plugin::esm::records::NavmRecord],
) {
    for navm in navmeshes {
        let entity = world.spawn();
        world.insert(entity, NavmeshTile(navm.clone()));
    }
    if !navmeshes.is_empty() {
        bump_navmesh_residency(world);
    }
}

/// Monotonic counter of NAVM tile-residency changes (#3256).
///
/// [`NavPath`] caches a path — **including a deliberately-cached empty
/// "no resident-tile path found" result** — keyed only on goal distance. With
/// a frozen goal and a `0.0` repath threshold, that comparison matches
/// bit-identically forever, so the first tick an actor resolves a frozen
/// destination is the only tick that ever consults tile residency for it. If
/// no tile localized the actor at that instant, `path_from_resident_tiles` is
/// never retried for the rest of that travel/guard leash — even after the
/// relevant tile streams in.
///
/// Stamping the generation onto each cached path and requiring it to match
/// makes residency changes invalidate the cache, which goal distance alone
/// cannot express.
///
/// Deliberately **not** save-registered, matching [`NavPath`] and
/// [`NavmeshTile`]: after a load the counter restarts at 0 and every cached
/// path is stale-by-default, which is the safe direction (one repath).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NavmeshResidency {
    pub(crate) generation: u64,
}

impl Resource for NavmeshResidency {}

/// Bump the residency generation, invalidating every cached [`NavPath`].
///
/// Called on NAVM tile spawn ([`spawn_navmesh_tiles`]) and on cell unload.
/// Over-bumping is cheap and safe — it costs at most one recompute per
/// pathing actor — whereas under-bumping is the bug this exists to prevent,
/// so the unload side fires for any cell teardown rather than trying to prove
/// a `NavmeshTile` was among the victims.
pub(crate) fn bump_navmesh_residency(world: &mut byroredux_core::ecs::World) {
    // Scoped so the resource borrow ends before the `insert_resource`
    // fallback below can take `&mut World`.
    let bumped = {
        if let Some(mut residency) = world.try_resource_mut::<NavmeshResidency>() {
            residency.generation = residency.generation.wrapping_add(1);
            true
        } else {
            false
        }
    };
    if !bumped {
        world.insert_resource(NavmeshResidency { generation: 1 });
    }
}

/// Read the current NAVM residency generation, or `0` when the resource
/// isn't registered (test fixtures, pre-streaming boot). A missing resource
/// reads as a single stable generation, so behaviour matches the pre-#3256
/// goal-only cache rather than thrashing.
pub(crate) fn navmesh_residency_generation(world: &byroredux_core::ecs::World) -> u64 {
    world
        .try_resource::<NavmeshResidency>()
        .map(|residency| residency.generation)
        .unwrap_or(0)
}

/// Cached single-tile NAVM path toward `goal`, computed by
/// `navmesh_path::path_from_resident_tiles` and consumed incrementally by
/// a `step_toward`-driven locomotion system (EX-16 item 3 Phase 3,
/// `docs/engine/navmesh-pathfinding.md`) — currently `travel_system`.
///
/// Computed **once per (entity, goal) pair, not every tick** — the design
/// doc's §7 cost posture. A caller recomputes only when its current goal
/// no longer matches [`goal`](Self::goal); until then it consumes
/// [`waypoints`](Self::waypoints) by popping the front entry once the
/// actor arrives within `LOCOMOTION_ARRIVAL_EPSILON` of it.
///
/// Deliberately **not** save-registered: rederived on demand from
/// resident `NavmeshTile` data, same posture as `NavmeshTile` itself — an
/// empty/missing `NavPath` after a load just means "repath on the next
/// tick that needs one," harmless and cheap, never lossy gameplay state.
///
/// An empty `waypoints` with a matching `goal` is itself a meaningful,
/// intentionally-cached result ("already tried, no resident-tile path
/// found for this goal") — see `travel_system`'s Pass 1a for why that
/// negative result is cached too, not just successful paths.
#[derive(Clone)]
pub(crate) struct NavPath {
    /// The destination this path was computed for.
    pub(crate) goal: Vec3,
    /// The [`NavmeshResidency`] generation this path was computed under
    /// (#3256). A cache hit requires it to match the live generation: goal
    /// distance alone cannot express "the set of resident tiles changed",
    /// which is exactly what a cached *empty* path needs to notice.
    pub(crate) residency_generation: u64,
    /// Remaining stepping points, nearest first, always ending with
    /// [`goal`](Self::goal) itself while non-empty. Empty means "no
    /// resident-tile path was found for this goal" — callers fall back
    /// to walking straight at `goal`, today's pre-pathing behavior.
    pub(crate) waypoints: VecDeque<Vec3>,
}
impl Component for NavPath {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod navmesh_tile_tests {
    use super::*;
    use byroredux_plugin::esm::records::NavmRecord;

    fn navm(form_id: u32) -> NavmRecord {
        NavmRecord {
            form_id,
            ..NavmRecord::default()
        }
    }

    #[test]
    fn spawns_one_entity_per_navmesh() {
        let mut world = byroredux_core::ecs::World::new();
        spawn_navmesh_tiles(&mut world, &[navm(0x100), navm(0x101)]);

        let q = world.query::<NavmeshTile>().expect("NavmeshTile storage");
        let form_ids: std::collections::HashSet<u32> =
            q.iter().map(|(_, tile)| tile.0.form_id).collect();
        assert_eq!(
            form_ids,
            [0x100, 0x101].into_iter().collect(),
            "one resident tile per NavmRecord, identity preserved"
        );
    }

    #[test]
    fn empty_navmesh_list_spawns_nothing() {
        let mut world = byroredux_core::ecs::World::new();
        spawn_navmesh_tiles(&mut world, &[]);
        assert!(
            world
                .query::<NavmeshTile>()
                .is_none_or(|q| q.iter().count() == 0),
            "a cell with no NAVM children must spawn zero tile entities"
        );
    }
}
