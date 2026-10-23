//! Exterior cell loading — per-worldspace context + single-cell loader
//! used by both the bulk `--grid` path and the streaming system.

use byroredux_core::ecs::components::CellFormId;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::coord::{cell_grid_to_world_yup, EXTERIOR_CELL_UNITS};
use byroredux_core::math::Vec3;
use byroredux_renderer::VulkanContext;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::asset_provider::{MaterialProvider, TextureProvider};

use super::load::{register_cell_root, stamp_cell_root_range};
use super::load_order::parse_record_indexes_shared;
use super::references::{
    flush_pending_cell_textures, flush_pending_cell_textures_on_yield, load_references_budgeted,
    ReferenceLoadJob, ReferenceLoadProgress,
};
use super::{terrain, water, FrameTimeBudget};

// #6271 — the three jobs this file used to interleave, one module each. The
// children glob-import this module (`use super::*`), and it glob-imports their
// `pub(super)` items back, so siblings and the test modules below see one flat
// namespace, as they did in the single file.
mod apply_job;
mod persistent;
mod worldspace;

pub(crate) use apply_job::{ExteriorCellApplyJob, ExteriorCellApplyProgress};
pub(crate) use persistent::{
    begin_worldspace_persistent_cell, ensure_persistent_actor_stubs,
    persistent_root_survives_crossing, PersistentCellApplyJob, PersistentCellApplyProgress,
    PersistentCellWindow,
};
pub use worldspace::{build_exterior_world_context, load_one_exterior_cell};

use persistent::*;
use worldspace::*;

/// Production text of every file the exterior loader spans, test modules
/// stripped — what the source-pin tests scan instead of one file's text.
#[cfg(test)]
pub(crate) fn production_source() -> String {
    [
        include_str!("exterior.rs"),
        include_str!("exterior/persistent.rs"),
        include_str!("exterior/apply_job.rs"),
        include_str!("exterior/worldspace.rs"),
    ]
    .into_iter()
    .map(byroredux_core::source_scan::strip_test_modules)
    .collect::<Vec<_>>()
    .join("\n")
}

#[cfg(test)]
mod cell_load_finished_tests;
#[cfg(test)]
mod exterior_water_height_tests;
#[cfg(test)]
mod logical_stub_source_pin_tests;
#[cfg(test)]
mod persistent_cell_identity_unchanged_tests;
#[cfg(test)]
mod persistent_cell_stamp_tests;
#[cfg(test)]
mod persistent_grid_tests;
#[cfg(test)]
mod persistent_root_survives_crossing_tests;
#[cfg(test)]
mod persistent_tile_tests;
#[cfg(test)]
mod resolve_persistent_cell_tests;
#[cfg(test)]
mod worldspace_selection_tests;

pub struct ExteriorWorldContext {
    pub record_index: Arc<byroredux_plugin::esm::records::EsmIndex>,
    /// #3366 — carries each plugin's global slot alongside its name, so the
    /// unresolved-REFR diagnostic can name ESL-owned forms and stays correct
    /// when an ESL precedes a regular plugin.
    pub load_order: Arc<crate::cell_loader::load_order::LoadOrder>,
    /// Path to the worldspace's plugin (the `esm_path` this context was
    /// built from), used to locate the companion `<Plugin> -
    /// Geometry.csg` for FO4 precombined geometry (M49). The cells'
    /// `_oc.nif` precombines reference the CSG named for this plugin.
    pub plugin_path: String,
    /// Correctly-cased plugin paths in load order (masters first, active
    /// plugin last), aligned with the global form-id mod-index byte. Lets the
    /// precombine loader open the *owning* plugin's `<Plugin> - Geometry.csg`
    /// (not the worldspace plugin's) for master-owned exterior tiles — e.g. a
    /// Commonwealth cell loaded with a DLC as the active plugin. #1590.
    pub plugin_paths: Vec<String>,
    /// Lowercase EDID key into `record_index.cells.exterior_cells`.
    pub worldspace_key: String,
    /// Pre-resolved climate (one per worldspace).
    pub climate: Option<byroredux_plugin::esm::records::ClimateRecord>,
    /// Default weather — highest-chance entry from the climate's
    /// weather table. Used to seed `SkyParamsRes` / `WeatherDataRes` /
    /// `CellLightingRes` on the first cell load; subsequent loads
    /// reuse the same resources via the streaming control loop.
    pub default_weather: Option<byroredux_plugin::esm::records::WeatherRecord>,
    /// Worldspace-default water height for exterior cells with no XCLW.
    /// `Some(0.0)` for an **Oblivion** worldspace carrying a NAM2 default
    /// water form (Tamriel sea level Z=0 — Oblivion WRLD has no DNAM
    /// height field); for FO3/FNV/Skyrim+/FO4, `Some(height)` from the
    /// worldspace's parsed DNAM second f32 (`WorldspaceRecord::
    /// default_water_height`, `crates/plugin/src/esm/cell/wrld.rs`) when
    /// present; `None` when the worldspace has no default water at all.
    /// Resolved once here so `load_one_exterior_cell` can fall back to it
    /// per-cell. The 98% of Oblivion cells without XCLW relied on this
    /// default; without it every coastal/sea cell rendered dry seabed
    /// (#1305 / OBL-D6-NEW-02). See `default_water_for_worldspace` for the
    /// per-game resolution, regression-tested by
    /// `wrld_dnam_captures_default_water_height` (parser) and
    /// `non_oblivion_uses_dnam_default_water_height` (consumer).
    pub default_water_height: Option<f32>,
    /// Worldspace-default water TYPE form (NAM2 → WATR), used for the
    /// water plane's appearance when a cell inherits the default height.
    pub default_water_type_form: Option<u32>,
}

fn resolved_exterior_water_height(
    water_height_is_explicit: bool,
    water_height: Option<f32>,
    default_water_height: Option<f32>,
) -> Option<f32> {
    if water_height_is_explicit {
        water_height
    } else {
        default_water_height
    }
}

/// Return the center and half extent used by an exterior water tile.
///
/// Keeping this conversion next to the exterior loader makes the tiling
/// contract explicit: neighboring cells must share the exact same world-XZ
/// edge after Bethesda's Z-up coordinates are converted to renderer Y-up.
#[inline]
fn exterior_water_tile_transform(gx: i32, gy: i32) -> ((f32, f32), f32) {
    let origin = cell_grid_to_world_yup(gx, gy);
    let half = water::exterior_half_extent();
    ((origin.x + half, origin.z - half), half)
}

/// Authored content signals for one exterior CELL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExteriorCellContent {
    pub coord: (i32, i32),
    pub has_landscape: bool,
    pub authored_reference_count: usize,
    pub precombined_mesh_count: usize,
}

impl ExteriorCellContent {
    /// Whether the CELL has enough authored content to be a meaningful
    /// foreground. This is a pre-load readiness gate, not a collision claim:
    /// EX-04 separately verifies that one of these sources produces walkable
    /// ground after import.
    pub fn is_content_backed(&self) -> bool {
        self.has_landscape || self.authored_reference_count > 0 || self.precombined_mesh_count > 0
    }
}

/// Deterministic pre-load diagnosis for an explicitly requested exterior
/// foreground CELL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExteriorForegroundReadiness {
    pub requested_coord: (i32, i32),
    pub requested: Option<ExteriorCellContent>,
    pub nearest_viable: Vec<ExteriorCellContent>,
}

impl ExteriorForegroundReadiness {
    pub fn is_content_backed(&self) -> bool {
        self.requested
            .as_ref()
            .is_some_and(ExteriorCellContent::is_content_backed)
    }
}

impl ExteriorWorldContext {
    /// #5496 — the worldspace the distant LOD rings are authored on
    /// (`lod_source_worldspace`): the child itself, or the nearest
    /// `WNAM` ancestor reached while the chain sets the PNAM "Use LOD
    /// Data" bit. Every distant-ring key — the legacy quad index, the
    /// `.btr`/`.bto` archive paths, the generated distant textures and
    /// their worldspace form id, the atlas paths, and the band bounds —
    /// resolves on THIS key; only the full-detail residency (the child's
    /// own cells and their exclusion masks) stays on
    /// [`Self::worldspace_key`].
    pub fn lod_worldspace_key(&self) -> &str {
        crate::env_translate::lod_source_worldspace(
            &self.record_index.cells.worldspaces,
            &self.worldspace_key,
        )
    }

    /// Inspect the requested tile without loading it. Suggestions are sorted
    /// by Chebyshev distance, then Manhattan distance and grid coordinate, so
    /// diagnostics never depend on `HashMap` iteration order.
    pub fn foreground_readiness(
        &self,
        coord: (i32, i32),
        suggestion_limit: usize,
    ) -> ExteriorForegroundReadiness {
        let cells = self
            .record_index
            .cells
            .exterior_cells
            .get(&self.worldspace_key);
        summarize_foreground_cells(cells, coord, suggestion_limit, |cell| {
            (
                cell.landscape.is_some(),
                cell.references.len(),
                cell.precombined_mesh_hashes.len(),
            )
        })
    }
}

/// Per-cell load result emitted by [`load_one_exterior_cell`]. Each
/// cell gets its own `cell_root` so streaming can unload cells
/// independently as the player moves.
pub struct OneCellLoadInfo {
    pub cell_root: EntityId,
    /// Mid-cell terrain ground point in Y-up world space (only used by
    /// the bulk loader to seat the initial camera).
    pub center: Vec3,
}

/// Mark `cell_root` as an exterior cell of the active worldspace (`IsInInterior` / `GetInWorldspace`).
fn stamp_exterior_cell(world: &mut World, cell_root: EntityId, wctx: &ExteriorWorldContext) {
    if let Some(record) = wctx.record_index.cells.worldspaces.get(&wctx.worldspace_key) {
        world.insert(cell_root, byroredux_scripting::cell_kind::ExteriorCell(record.form_id));
    }
}

