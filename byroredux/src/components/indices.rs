//! Entity indices and caches (`NameIndex`, `SubtreeCache`, `CellRootIndex`, `CinematicReAdoption`). (#5467 split from `components.rs`)

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::Resource;
use byroredux_core::string::FixedString;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use rustc_hash::FxHashMap;
use std::collections::HashMap;

/// Cached name→entity mapping for the animation system.
///
/// Rebuilt only when the count of `Name` components changes. Previously
/// the generation tracked `world.next_entity_id()`, which forced a full
/// rebuild on every entity spawn regardless of whether the spawn
/// involved a `Name` — a 3000-entity cell load with only 500 named
/// entities still triggered one rebuild on the next frame. Using the
/// `Name` storage size as the generation means only spawns/despawns
/// that actually touch `Name` invalidate the cache. See #249.
///
/// Edge case: in-place `Name` replacement (re-inserting `Name` on an
/// existing entity without removing it first) does not change the
/// count and therefore does not invalidate the index. No code in the
/// engine currently renames entities after spawn, so this is not a
/// concern today — add an explicit `invalidate()` call if that
/// changes.
/// `FxHashMap` (#3677 / PERF-D1-2026-08-30-01) — `FixedString`-keyed,
/// probed once per animated channel per animated entity per frame; see
/// `_audit-common.md`'s "Hot-path hashing" doctrine.
pub(crate) struct NameIndex {
    pub(crate) map: FxHashMap<FixedString, EntityId>,
    /// Count of `Name` components seen at the last rebuild. `usize::MAX`
    /// on a fresh index so the first comparison always rebuilds.
    pub(crate) generation: usize,
}
impl Resource for NameIndex {}

/// Persisted subtree name maps for animation — maps root entity →
/// (bone name → entity) so the BFS walk isn't repeated every frame.
/// Invalidated alongside `NameIndex` when the Name component count changes. #278.
/// `FxHashMap` (#3677) — same doctrine as `NameIndex.map`; the inner map is
/// the one actually probed per channel (`build_subtree_name_map`'s
/// return type in `anim_convert.rs`).
pub(crate) struct SubtreeCache {
    pub(crate) map: FxHashMap<EntityId, FxHashMap<FixedString, EntityId>>,
    /// Name component count at last rebuild — same invalidation signal as NameIndex.
    pub(crate) generation: usize,
}
impl Resource for SubtreeCache {}
impl SubtreeCache {
    pub(crate) fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            generation: usize::MAX,
        }
    }
}

/// Inverted index of `CellRoot → owned entities`, populated by
/// `cell_loader::stamp_cell_root` and drained by `unload_cell`. Pre-#791
/// the unload path iterated the entire `CellRoot` SparseSet to filter
/// down to victims of a single cell, scanning ~13.5k rows on a
/// radius-3 streaming grid (49 cells × ~1.5k entities) to find the
/// ~1.5k that belong to the unloading cell. With this index the lookup
/// is `HashMap::remove`, independent of the number of resident cells.
///
/// Memory cost is ~8 B per cell-owned entity (one `EntityId` per slot
/// in the inner `Vec`), dwarfed by the entity's component data.
pub(crate) struct CellRootIndex {
    pub(crate) map: HashMap<EntityId, Vec<EntityId>>,
}
impl Resource for CellRootIndex {}
impl CellRootIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

/// Entities whose cinematic tether/vehicle chain ended (#3817) while no
/// loaded cell covered their position, so the re-adoption pass could not
/// stamp a `CellRoot` on them yet. The cinematic route system retries this
/// list every tick: once an entity sits inside a loaded exterior cell it is
/// stamped onto that cell's root (and removed here); an entity that has
/// been despawned some other way is dropped. Un-rooted entities are
/// despawn-immune to cell unload (nothing enumerates them), so a pending
/// entry that never resolves means a permanently resident entity — the
/// list bounds and surfaces that population instead of hiding it.
#[derive(Default)]
pub(crate) struct CinematicReAdoption {
    pub(crate) pending: Vec<EntityId>,
}
impl Resource for CinematicReAdoption {}

impl NameIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            generation: usize::MAX, // Force rebuild on first use.
        }
    }
}
