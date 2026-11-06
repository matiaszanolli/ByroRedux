//! Entity indices and caches (`NameIndex`, `SubtreeCache`, `CellRootIndex`, `CinematicReAdoption`). (#5467 split from `components.rs`)

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::Resource;
use byroredux_core::string::FixedString;
use byroredux_plugin::esm::records::EsmIndex;
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::sync::Arc;

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

/// World-resource wrapper around the parsed [`EsmIndex`] for the
/// currently-loaded scene. Set by both cell-load entry points after the
/// ESM parse completes. The cell table itself is `.0.cells`.
///
/// **Why the whole index and not just `EsmCellIndex`** (#3415): the
/// exterior context already owns an `Arc<EsmIndex>`
/// (`ExteriorWorldContext::record_index`), and an `Arc` cannot be
/// projected onto one of its fields — so a resource holding
/// `Arc<EsmCellIndex>` could only be built there by deep-cloning every
/// cell / static / TXST map in the load order. Widening the payload lets
/// the exterior arm hand over a refcount bump, and costs the interior arm
/// nothing: it owns its `EsmIndex` outright and moves it into the `Arc`.
/// The exterior path has kept the full index resident for the whole
/// session since it was written, so this is the profile that was already
/// proven on the same games, applied to both arms.
///
/// Read-only after insertion — the parsed index is treated as
/// immutable scene metadata; subsequent cell loads (e.g. through an
/// XTEL portal) replace the resource wholesale rather than mutating in
/// place, so the borrow patterns stay simple.
pub struct LoadedCellIndex(pub Arc<EsmIndex>);

impl Resource for LoadedCellIndex {}

/// Plugin-load configuration captured at engine boot. The transition
/// orchestrator re-uses this to call `load_cell_with_masters` for the
/// destination cell without re-parsing CLI args. Set by the boot path
/// in `scene::setup_scene` whenever a `--esm`-driven cell load runs;
/// absent otherwise (loose-NIF / sweet-roll modes).
///
/// Fields mirror the CLI arg shape so the trigger site reading this
/// resource can hand it straight to `PendingCellTransition`.
#[derive(Clone, Debug)]
pub struct LoadedPluginSet {
    /// Repeatable `--master <path>` args in CLI order.
    pub masters: Vec<String>,
    /// The `--esm <path>` value.
    pub esm_path: String,
}

impl Resource for LoadedPluginSet {}

/// Identity of the interior cell currently loaded, plus the plugin set it
/// came from — everything a save needs to reload the same cell.
///
/// Set by `load_cell_with_masters` on every interior load;
/// the M45 save registry serialises it as a resource column so `load`
/// can re-issue the same `TransitionDestination::Interior` before applying
/// saved component deltas. Absent in loose-NIF / exterior-streaming modes
/// (no single interior cell identity).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CurrentCellContext {
    /// Cell editor-ID (the `--cell` value / transition destination key).
    pub cell_editor_id: String,
    /// Main ESM path (the `--esm` value).
    pub esm_path: String,
    /// Master plugin paths in load order (the repeated `--master` values).
    pub masters: Vec<String>,
}

impl Resource for CurrentCellContext {}

/// Identity of the exterior worldspace/grid currently streaming — the
/// `CurrentCellContext` equivalent for exterior sessions (EX-09/17 item 4).
///
/// `WorldStreamingState` (the thing that actually knows worldspace/grid/
/// radius) lives on `App`, not as a `World` resource — it can't, since the
/// streaming driver needs to borrow it alongside `VulkanContext` and the
/// asset providers every frame, and `SaveCommand::execute` only ever sees
/// `&World`. This resource is the deliberately-thin mirror: just the
/// identity fields a save/load round-trip needs, kept in sync at every
/// point that starts, moves, or tears down exterior streaming (see
/// `scene::begin_exterior_streaming`, which sets this after every
/// fresh exterior session, `App::step_streaming`'s grid-crossing update,
/// and `streaming_helpers::drain_streaming_state`, which clears
/// it — the exterior mirror of `clear_current_interior_identity` below).
///
/// Absent in loose-NIF / interior modes, same posture as `CurrentCellContext`
/// being absent in exterior mode — the two are mutually exclusive, matching
/// `WorldStreamingState`/`CurrentCellRoot`'s existing either-or contract.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CurrentExteriorContext {
    /// Lowercase worldspace EDID (`ExteriorWorldContext::worldspace_key`).
    pub worldspace_key: String,
    /// Main ESM path (the `--esm` value).
    pub esm_path: String,
    /// Master plugin paths in load order (the repeated `--master` values).
    pub masters: Vec<String>,
    /// Player's current grid cell — `WorldStreamingState::last_player_grid`.
    pub grid: (i32, i32),
    /// Load radius the session was streaming at.
    pub radius_load: i32,
    /// Unload radius the session was streaming at.
    pub radius_unload: i32,
}

impl Resource for CurrentExteriorContext {}
