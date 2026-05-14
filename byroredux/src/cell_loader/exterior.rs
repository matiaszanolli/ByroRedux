//! Exterior cell loading — per-worldspace context + single-cell loader
//! used by both the bulk `--grid` path and the streaming system.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;
use byroredux_renderer::VulkanContext;
use std::sync::Arc;

use crate::asset_provider::{MaterialProvider, TextureProvider};

use super::load::stamp_cell_root;
use super::load_order::parse_record_indexes_in_load_order;
use super::references::load_references;
use super::{terrain, water};

pub struct ExteriorWorldContext {
    pub record_index: Arc<byroredux_plugin::esm::records::EsmIndex>,
    pub load_order: Arc<Vec<String>>,
    /// Lowercase EDID key into `record_index.cells.exterior_cells`.
    pub worldspace_key: String,
    /// Pre-resolved climate (one per worldspace).
    pub climate: Option<byroredux_plugin::esm::records::ClimateRecord>,
    /// Default weather — highest-chance entry from the climate's
    /// weather table. Used to seed `SkyParamsRes` / `WeatherDataRes` /
    /// `CellLightingRes` on the first cell load; subsequent loads
    /// reuse the same resources via the streaming control loop.
    pub default_weather: Option<byroredux_plugin::esm::records::WeatherRecord>,
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

/// Build the once-per-session context for exterior streaming.
///
/// Parses every plugin in load order, picks the worldspace using the
/// same priority chain the bulk loader has used since #444 (override →
/// preferred game-default → grid-coord match → max cells), and
/// resolves the worldspace's climate + default weather.
///
/// `center_x` / `center_y` / `radius` are used only by the
/// grid-coord-match step in the worldspace selector — pass the
/// initial player grid coords plus an estimate of the eventual stream
/// radius (e.g. 3) so the worldspace lookup picks the worldspace that
/// actually contains the player.
pub fn build_exterior_world_context(
    masters: &[String],
    esm_path: &str,
    center_x: i32,
    center_y: i32,
    radius: i32,
    wrld_override: Option<&str>,
) -> anyhow::Result<ExteriorWorldContext> {
    // Parse all plugins in load order with FormID remap. Empty
    // `masters` reduces to single-plugin behaviour (the remap's
    // self-reference path is a no-op).
    let plugin_paths: Vec<&str> = masters
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(esm_path))
        .collect();
    let (record_index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
    let index = &record_index.cells;

    // Find the best worldspace. Priority:
    //   1. Caller-supplied `--wrld <name>` (case-insensitive EDID match).
    //   2. Preferred game-default list: WastelandNV (FNV), Wasteland
    //      (FO3 Capital Wasteland), Tamriel (Oblivion), Skyrim (Skyrim).
    //   3. Worldspace that actually contains the requested grid coord.
    //   4. Worldspace with the most cells (ultimate fallback).
    // Pre-fix the Wasteland EDID was missing, so `--esm Fallout3.esm
    // --grid 0,0` landed on the max-cells fallback and silently picked
    // the wrong worldspace when any DLC master added its own. See #444.
    let worldspace_key = {
        let override_match = wrld_override.and_then(|name| {
            let lower = name.to_ascii_lowercase();
            index
                .exterior_cells
                .keys()
                .find(|k| k.eq_ignore_ascii_case(&lower))
                .cloned()
        });
        if let Some(ref name) = override_match {
            log::info!("Using worldspace '{name}' (from --wrld override)");
        }
        override_match
            .or_else(|| {
                let preferred = ["wastelandnv", "wasteland", "tamriel", "skyrim"];
                preferred
                    .iter()
                    .find(|&&name| index.exterior_cells.contains_key(name))
                    .map(|s| s.to_string())
            })
            .or_else(|| {
                // Prefer a worldspace that actually contains the
                // requested grid coord over raw cell count. Protects
                // multi-plugin loads where a DLC worldspace with many
                // cells but no grid 0,0 would otherwise outvote the
                // base game's Wasteland. See #444.
                let min_x = center_x.saturating_sub(radius);
                let max_x = center_x.saturating_add(radius);
                let min_y = center_y.saturating_sub(radius);
                let max_y = center_y.saturating_add(radius);
                index
                    .exterior_cells
                    .iter()
                    .find(|(_, cells)| {
                        cells.keys().any(|(gx, gy)| {
                            *gx >= min_x && *gx <= max_x && *gy >= min_y && *gy <= max_y
                        })
                    })
                    .map(|(name, _)| name.clone())
            })
            .or_else(|| {
                index
                    .exterior_cells
                    .iter()
                    .max_by_key(|(_, cells)| cells.len())
                    .map(|(name, _)| name.clone())
            })
            .ok_or_else(|| anyhow::anyhow!("No worldspace found in plugin set"))?
    };

    log::info!(
        "Exterior world context built: worldspace '{}' (target ({},{}) ±{})",
        worldspace_key,
        center_x,
        center_y,
        radius,
    );

    // Resolve weather + climate: WRLD → CLMT → WTHR. Climate carries
    // per-worldspace TNAM sunrise/sunset hours so the TOD interpolator
    // runs on the right clock (#463). Default weather is the
    // highest-chance entry; mods use -1 as a sentinel / subtractive
    // weight (#476).
    let climate = record_index
        .cells
        .worldspace_climates
        .get(&worldspace_key)
        .and_then(|fid| record_index.climates.get(fid).cloned())
        .inspect(|climate| {
            log::info!(
                "Worldspace '{}' climate '{}' ({:08X}): {} weathers, \
                 sunrise {:.2}–{:.2}h, sunset {:.2}–{:.2}h",
                worldspace_key,
                climate.editor_id,
                climate.form_id,
                climate.weathers.len(),
                climate.sunrise_begin as f32 / 6.0,
                climate.sunrise_end as f32 / 6.0,
                climate.sunset_begin as f32 / 6.0,
                climate.sunset_end as f32 / 6.0,
            );
        });
    let default_weather = climate.as_ref().and_then(|climate| {
        let best = climate
            .weathers
            .iter()
            .filter(|w| w.chance >= 0)
            .max_by_key(|w| w.chance)?;
        let wthr = record_index.weathers.get(&best.weather_form_id)?.clone();
        log::info!(
            "Default weather: '{}' ({:08X}, chance {})",
            wthr.editor_id,
            wthr.form_id,
            best.chance,
        );
        Some(wthr)
    });

    Ok(ExteriorWorldContext {
        record_index: Arc::new(record_index),
        load_order: Arc::new(load_order),
        worldspace_key,
        climate,
        default_weather,
    })
}

/// Load a single exterior cell at `(gx, gy)`.
///
/// Stamps its own `cell_root` so the streaming system can unload it
/// independently when the player moves out of range. Returns `None`
/// when the cell doesn't exist at that coord in the worldspace
/// (off-map / hole in the grid — common at exterior edges).
///
/// `terrain_blas_accumulator`:
///   * `Some(&mut acc)` — caller (bulk loader) collects BLAS specs
///     across all cells and submits one batched build at the end (the
///     #382 optimization for the 49-cell `--grid` initial load).
///   * `None` — submit BLAS immediately (one cell, one submit). The
///     streaming system passes `None` because cells stream in one at
///     a time; the per-cell submit overhead is negligible compared to
///     the parse cost the worker just paid.
#[tracing::instrument(
    name = "load_one_exterior_cell",
    skip_all,
    fields(gx = gx, gy = gy),
)]
#[allow(clippy::too_many_arguments)]
pub fn load_one_exterior_cell(
    wctx: &ExteriorWorldContext,
    gx: i32,
    gy: i32,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    terrain_blas_accumulator: Option<&mut Vec<(u32, u32, u32)>>,
) -> anyhow::Result<Option<OneCellLoadInfo>> {
    let index = &wctx.record_index.cells;
    let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
        return Ok(None);
    };
    let Some(cell) = cells_map.get(&(gx, gy)) else {
        return Ok(None);
    };

    let first_entity = world.next_entity_id();
    let has_land = cell.landscape.is_some();
    log::info!(
        "  Cell ({},{}) '{}': {} references{}",
        gx,
        gy,
        cell.editor_id,
        cell.references.len(),
        if has_land { " + LAND" } else { "" },
    );

    // Terrain mesh from LAND heightmap. Either accumulate the BLAS
    // spec for caller's batched build (bulk loader) or build it
    // ourselves (streaming).
    let mut local_blas: Vec<(u32, u32, u32)> = Vec::new();
    let blas_sink: &mut Vec<(u32, u32, u32)> = match terrain_blas_accumulator {
        Some(acc) => acc,
        None => &mut local_blas,
    };
    if let Some(ref land) = cell.landscape {
        let _ = terrain::spawn_terrain_mesh(
            world,
            ctx,
            tex_provider,
            &index.landscape_textures,
            gx,
            gy,
            land,
            blas_sink,
        );
    }
    // Water plane from XCLW / XCWT. Exterior cells without explicit
    // XCLW inherit the worldspace default, which the cell parser has
    // already collapsed into `cell.water_height` upstream.
    if let Some(water_height) = cell.water_height {
        // Exterior cell origin in Y-up world coords (matches the
        // terrain spawn convention): X = grid_x * 4096, Z = −grid_y * 4096.
        let origin_x = gx as f32 * 4096.0;
        let origin_z = -(gy as f32) * 4096.0;
        let _ = water::spawn_water_plane(
            world,
            ctx,
            tex_provider,
            &wctx.record_index.waters,
            water_height,
            cell.water_type_form,
            (origin_x + 2048.0, origin_z - 2048.0),
            water::exterior_half_extent(),
            blas_sink,
        );
    }
    // Streaming path: submit our own BLAS build now (one mesh, one submit).
    if !local_blas.is_empty() {
        let built = ctx.build_blas_batched(&local_blas);
        log::info!(
            "  Cell ({},{}) terrain BLAS: {built}/{} tiles",
            gx,
            gy,
            local_blas.len(),
        );
    }

    // Spawn placed references. Pre-#M40 every grid load went through a
    // single `load_references` over all 49 cells' refs; per-cell calls
    // share the process-lifetime `NifImportRegistry` so cross-cell mesh
    // re-use still hits the cache (#381).
    let label = format!("exterior({},{})", gx, gy);
    let result = load_references(
        &cell.references,
        index,
        &wctx.record_index,
        &wctx.record_index.npcs,
        &wctx.record_index.races,
        wctx.record_index.game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &label,
        wctx.load_order.as_ref(),
    );

    // Mid-cell terrain ground point — only meaningful for the
    // initial-load camera-positioning path used by the bulk loader.
    let center = if let Some(ref land) = cell.landscape {
        let mid_height = land.heights[16 * 33 + 16];
        let world_x = gx as f32 * 4096.0 + 16.0 * 128.0;
        let world_y = gy as f32 * 4096.0 + 16.0 * 128.0;
        // Z-up → Y-up: (x, height, -y), plus 200 units above ground.
        Vec3::new(world_x, mid_height + 200.0, -world_y)
    } else {
        result.center
    };

    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);

    Ok(Some(OneCellLoadInfo { cell_root, center }))
}

