//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.
//!
//! The pipeline is split across submodules:
//!
//! - [`load`] — interior entry point (`load_cell_with_masters`) +
//!   `CellLoadResult` + `resolve_cell_lighting` + the shared
//!   `stamp_cell_root` lifecycle helper.
//! - [`unload`] — `unload_cell` teardown path + per-victim inventory
//!   instance release.
//! - [`exterior`] — `ExteriorWorldContext`, `build_exterior_world_context`,
//!   and the single-cell streaming entry point `load_one_exterior_cell`.
//! - [`references`] — the inner REFR walk, BSA NIF/SPT parse + cache
//!   path, and dispatch into `spawn`.
//! - [`spawn`] — per-placement ECS entity creation (meshes, lights,
//!   particles, collision shapes).
//! - [`partial`] — `finish_partial_import`, the main-thread drain for
//!   the M40 streaming worker.
//! - [`euler`] — Z-up → Y-up Euler-angle → quaternion conversion +
//!   `--rotation-mode` diagnostic switch.

// Items in scope here are visible to child test modules via
// `use super::*;` — kept lean but covering everything the test files
// historically relied on through the (pre-split) flat `cell_loader.rs`.
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::components::{Inventory, ItemInstanceId};
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::resources::ItemInstancePool;
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::storage::EntityId;
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::{
    CellRoot, GlobalTransform, LightSource, Material, MeshHandle, ParticleEmitter, TextureHandle,
    Transform, World,
};
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::math::{Quat, Vec3};
#[cfg(test)]
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[cfg(test)]
#[allow(unused_imports)]
use std::sync::Arc;

#[cfg(test)]
#[allow(unused_imports)]
use crate::components::{
    AlphaBlend, CellLightingRes, CellRootIndex, MaterialTextureHandles, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided, WeatherDataRes, WeatherTransitionRes,
};

pub(crate) mod created;
mod euler;
mod exterior;
mod graph_nodes;
mod index;
mod interior_spawn;
mod load;
pub(crate) mod load_order;
mod legacy_lod_index;
mod lod_bands;
mod lod_coverage;
mod lod_support;
mod material_flags;
mod nif_import_registry;
mod object_lod;
mod object_sequences;
mod partial;
mod placement_lod;
pub(crate) mod precombined;
pub(crate) mod references;
mod refr;
pub(crate) mod spawn;
pub(crate) mod reference_state;
pub(crate) mod stream_snapshot;
mod terrain;
mod terrain_lod;
mod terrain_lod_btr;
mod tree_lod;
mod terrain_seam;
mod transition;
pub(crate) mod unload;
mod water;
mod work_budget;

pub use index::LoadedCellIndex;
pub use interior_spawn::SpawnPose;
pub use transition::{
    log_transition_header, position_zup_to_yup, queue_door_transition,
    queue_transition_to_reference, reposition_camera,
    rotation_zup_to_yup_quat, take_pending_transition, unload_current_interior, CellTransitionInFlight, CurrentCellContext,
    CurrentCellRoot, CurrentExteriorContext, InteriorCellRequest, LoadedPluginSet,
    PendingCellTransitionSlot, TransitionDestination,
};
// #3891 — these three have no call site by name, but each is load-bearing
// and the compiler cannot see why, so the allow is scoped to exactly them
// with the reason attached:
//   * `QueuedDoorTransition` / `QueueDoorTransitionError` are the Ok and Err
//     halves of the re-exported `queue_door_transition`'s return type —
//     dropping them would make a live function's signature unnameable
//     through `cell_loader::`.
//   * `PendingCellTransition` is the target of intra-doc links in
//     `app_step.rs` and `commands/scene.rs`; `transition` is a private
//     module, so this re-export is the only path those links can resolve.
pub(crate) use transition::{InteriorCellApply, InteriorCellApplyProgress};
// #5222 — the Fallout-legacy authored-LOD-quad index; re-exported for
// `streaming::WorldStreamingState`'s cached field.
pub(crate) use legacy_lod_index::LegacyLodQuadIndex;
#[allow(unused_imports)]
pub use transition::{PendingCellTransition, QueueDoorTransitionError, QueuedDoorTransition};
// `clear_current_exterior_identity` is an internal teardown helper (only
// `streaming_helpers::drain_streaming_state` calls it) — `pub(crate)`, not
// part of the crate's external `pub` surface the block above re-exports.
pub(crate) use transition::clear_current_exterior_identity;

// Re-exports — keep the existing `crate::cell_loader::FOO` call sites in
// main.rs / streaming/ / commands.rs working without further changes.
//
// #3891 — these used to carry a blanket `#[allow(unused_imports)]`
// justified by "several only show up in external crates (tests, other
// workspace members) or as the public API surface". `byroredux` is a
// BINARY crate: it has no external consumers and no public API surface, so
// that justification could not hold, and the blanket switched off the one
// lint that detects a dead re-export. It had been hiding three
// (`load_interior_cell`, `CellLoadPhaseTimings`, `OneCellLoadInfo`) — the
// first for over three months. Any allow below is now per-block with the
// specific reason it is needed.
// #3891 — `cfg_attr(not(test), ...)`, not a blanket allow: these names are
// consumed only by child `#[cfg(test)]` modules through `use super::*`, so
// the lint stays ACTIVE for the test build (where a genuinely dead
// re-export would still be caught) and is silenced only in the non-test
// build where the consumer is compiled out.
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use nif_import_registry::{
    canonical_model_path_key, CachedNifImport, NifImportRegistry,
};
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use refr::{
    build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements_if,
    mswp_source_matches, same_material_path, scol_cm_path,
};
// The no-predicate wrapper is only what the expansion tests call (#5523).
#[cfg(test)]
pub(crate) use refr::expand_scol_placements;

pub(crate) use euler::euler_zup_to_quat_yup_refr;
pub use euler::set_refr_rotation_mode_diag;

pub(crate) use exterior::{
    begin_worldspace_persistent_cell, ensure_persistent_actor_stubs,
    persistent_root_survives_crossing, ExteriorCellApplyJob, ExteriorCellApplyProgress,
    PersistentCellApplyJob, PersistentCellApplyProgress, PersistentCellWindow,
};
pub use exterior::{build_exterior_world_context, load_one_exterior_cell, ExteriorWorldContext};
pub(crate) use load::apply_interior_cell_lighting;
// Load-bearing for `cell_loader/lgtm_fallback_tests.rs`, which reaches it
// through `use super::*`.
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use load::resolve_cell_lighting;
pub use load::{load_cell_with_masters, validate_cell_loadable};
// #3891 — `CellLoadResult` is the `Ok` type of the re-exported
// `load_cell_with_masters`; no caller names it today, but removing the
// re-export would make that function's return type unnameable through
// `cell_loader::`.
#[allow(unused_imports)]
pub use load::CellLoadResult;
// Re-exported for the headless `--list-cells` catalogue, which needs the
// same masters-first merged index the cell loader builds but stops after
// the parse phase.
pub(crate) use load_order::parse_record_indexes_in_load_order;
pub(crate) use lod_coverage::{
    find_full_detail_overlaps, find_overlaps, find_terrain_full_detail_overlaps, ChurnTracker,
};
pub(crate) use lod_support::{resolve_lod_grid_origin, LodReconcileInput, LodWorkBudget};
pub(crate) use object_lod::{
    object_lod_scheme, stream_object_lod_blocks, unload_object_lod_block, ObjectLodBlock,
    ObjectLodScheme,
};
pub(crate) use partial::finish_partial_import;
pub(crate) use tree_lod::{stream_tree_lod_blocks, TreeLodRing};
pub(crate) use placement_lod::{
    stream_placement_lod_blocks, unload_placement_lod_block, PlacementLodBlock,
};
pub(crate) use terrain_lod::{stream_lod_blocks, unload_lod_block};
pub(crate) use terrain_seam::{check_seam, SeamDirection};
pub use unload::{unload_cell, unload_cells, UnloadPhaseTimings};
pub(crate) use unload::purge_cinematic_retention_state;
pub(crate) use water::{
    rebuild_lod_water_mesh, spawn_lod_water_plane, unload_lod_water_plane, DistantWaterCell,
};
pub(crate) use work_budget::FrameTimeBudget;

// The Studio gallery owns imported assets and its room through cell roots
// so removal reuses `unload_cell`'s GPU reference accounting.
pub(crate) use load::{stamp_cell_root, stamp_cell_root_range};
// Test-only re-exports so the `use super::*;` patterns inside the
// child test modules see the helpers they exercise. Production code
// paths reach these via their owning submodule path.
#[cfg(test)]
pub(crate) use load::register_cell_root;
#[cfg(test)]
pub(crate) use spawn::{
    count_spawnable_nif_lights, is_known_exporter_artifact_light_name, is_spawnable_nif_light,
    light_radius_or_default, spawn_nif_lights,
};
#[cfg(test)]
pub(crate) use unload::release_victim_item_instances;
#[cfg(test)]
pub(crate) use unload::release_victim_rapier_bodies;
pub(crate) use load::note_player_entered_cell;
pub(crate) use material_flags::{pack_effect_shader_flags, pack_imported_material_flags};

#[cfg(test)]
mod attach_points_spawn_tests;
#[cfg(test)]
mod euler_zup_to_quat_yup_tests;
#[cfg(test)]
mod finish_partial_tests;
#[cfg(test)]
mod interior_spawn_tests;
#[cfg(test)]
mod inventory_release_tests;
#[cfg(test)]
mod legacy_lod_index_tests;
#[cfg(test)]
mod lgtm_fallback_tests;
#[cfg(test)]
mod navm_boundary_soak_tests;
#[cfg(test)]
mod nif_import_registry_tests;
#[cfg(test)]
mod nif_light_spawn_gate_tests;
#[cfg(test)]
mod pkin_expansion_tests;
#[cfg(test)]
mod placement_root_subtree_tests;
#[cfg(test)]
mod precombined_clip_handle_tests;
#[cfg(test)]
mod rapier_release_tests;
#[cfg(test)]
mod reference_enable_gate_tests;
#[cfg(test)]
mod scripted_lock_gate_tests;
#[cfg(test)]
pub(crate) mod refr_texture_overlay_tests;
#[cfg(test)]
mod root_index_tests;
#[cfg(test)]
mod scol_expansion_tests;
#[cfg(test)]
mod sky_params_cleanup_tests;
#[cfg(test)]
mod terrain_splat_tests;
#[cfg(test)]
mod unload_greyscale_lut_tests;
#[cfg(test)]
mod unload_skin_cleanup_tests;
