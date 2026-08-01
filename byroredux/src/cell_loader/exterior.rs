//! Exterior cell loading — per-worldspace context + single-cell loader
//! used by both the bulk `--grid` path and the streaming system.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::coord::{cell_grid_to_world_yup, EXTERIOR_CELL_UNITS};
use byroredux_core::math::Vec3;
use byroredux_renderer::VulkanContext;
use std::sync::Arc;

use crate::asset_provider::{MaterialProvider, TextureProvider};

use super::load::{register_cell_root, stamp_cell_root_range};
use super::load_order::parse_record_indexes_in_load_order;
use super::references::{
    flush_pending_cell_textures, load_references, load_references_budgeted, ReferenceLoadJob,
    ReferenceLoadProgress,
};
use super::{terrain, water, FrameTimeBudget};

pub struct ExteriorWorldContext {
    pub record_index: Arc<byroredux_plugin::esm::records::EsmIndex>,
    pub load_order: Arc<Vec<String>>,
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

/// Per-cell load result emitted by [`load_one_exterior_cell`]. Each
/// cell gets its own `cell_root` so streaming can unload cells
/// independently as the player moves.
pub struct OneCellLoadInfo {
    pub cell_root: EntityId,
    /// Mid-cell terrain ground point in Y-up world space (only used by
    /// the bulk loader to seat the initial camera).
    pub center: Vec3,
}

/// Spawn the active worldspace's persistent CELL once, outside the
/// coordinate-streamed tile set.
///
/// Bethesda stores globally persistent exterior actors here (Skyrim's
/// Hadvar/Ralof ACHRs are the MQ101-critical examples). Giving the CELL its
/// own root keeps those entities alive across ordinary tile unloads while
/// still making the whole set reclaimable on a worldspace transition.
pub fn load_worldspace_persistent_cell(
    wctx: &ExteriorWorldContext,
    center_grid: (i32, i32),
    full_3d_radius: i32,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> Option<EntityId> {
    super::load::ensure_globals_resource(world, &wctx.record_index.globals);
    let index = &wctx.record_index.cells;
    let cell = index
        .worldspace_persistent_cells
        .get(&wctx.worldspace_key)?;

    let mut local_refs = Vec::new();
    let mut remote_actor_refs = Vec::new();
    for placed in &cell.references {
        if persistent_position_is_local(placed.position, center_grid, full_3d_radius) {
            local_refs.push(placed.clone());
        } else if wctx.record_index.npcs.contains_key(&placed.base_form_id) {
            remote_actor_refs.push(placed);
        }
    }

    let cell_root = world.spawn();
    register_cell_root(world, cell_root);
    let first_entity = world.next_entity_id();
    let result = load_references(
        &local_refs,
        index,
        &wctx.record_index,
        &wctx.record_index.npcs,
        &wctx.record_index.races,
        wctx.record_index.game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &format!("{} persistent", wctx.worldspace_key),
        wctx.load_order.as_ref(),
        &std::collections::HashSet::new(),
    );
    let live_actor_candidates: std::collections::HashMap<u32, (EntityId, bool)> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, candidate)| {
                    (
                        candidate.reference_form_id,
                        (
                            entity,
                            world.has::<byroredux_core::ecs::GlobalTransform>(entity),
                        ),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let local_actor_refs: Vec<_> = local_refs
        .iter()
        .filter(|placed| wctx.record_index.npcs.contains_key(&placed.base_form_id))
        .collect();
    let local_actor_3d = local_actor_refs
        .iter()
        .filter(|placed| {
            live_actor_candidates
                .get(&placed.form_id)
                .is_some_and(|(_, loaded_3d)| *loaded_3d)
        })
        .count();
    let mut logical_stub_refs = remote_actor_refs;
    logical_stub_refs.extend(
        local_actor_refs
            .iter()
            .copied()
            .filter(|placed| !live_actor_candidates.contains_key(&placed.form_id)),
    );
    // Persistent actors outside the active 3D radius retain their logical
    // reference/base identity for quest-alias resolution, but deliberately
    // carry no Transform/GlobalTransform or render hierarchy. `HasLoaded3D`
    // therefore remains false until a future persistent-3D streamer promotes
    // them. Local actors were fully spawned above and must not get duplicate
    // logical stubs.
    for placed in &logical_stub_refs {
        let entity = world.spawn();
        let plugin_name = super::load_order::plugin_for_form_id(placed.form_id, &wctx.load_order)
            .unwrap_or("Engine.esm");
        let form_id = world
            .resource_mut::<byroredux_core::form_id::FormIdPool>()
            .intern(byroredux_core::form_id::FormIdPair {
                plugin: byroredux_core::form_id::PluginId::from_filename(plugin_name),
                local: byroredux_core::form_id::LocalFormId(placed.form_id),
            });
        world.insert(
            entity,
            byroredux_core::ecs::components::FormIdComponent(form_id),
        );
        world.insert(
            entity,
            byroredux_scripting::SceneAliasCandidate {
                reference_form_id: placed.form_id,
                base_form_id: placed.base_form_id,
                location_ref_types: placed.location_ref_types.clone(),
            },
        );
    }
    if !logical_stub_refs.is_empty() {
        byroredux_scripting::mark_scene_actor_bindings_dirty(world);
    }
    stamp_cell_root_range(world, cell_root, first_entity, world.next_entity_id());
    flush_pending_cell_textures(ctx);
    log::info!(target: "engine::scene_persistent",
        "Worldspace '{}' persistent CELL {:08X}: {} local refs -> {} entities; {}/{} local actors have 3D, {} logical-only actor identities ({} authored refs total)",
        wctx.worldspace_key,
        cell.form_id,
        local_refs.len(),
        result.entity_count,
        local_actor_3d,
        local_actor_refs.len(),
        logical_stub_refs.len(),
        cell.references.len(),
    );
    Some(cell_root)
}

fn persistent_position_is_local(
    position_zup: [f32; 3],
    center_grid: (i32, i32),
    radius: i32,
) -> bool {
    let gx = (position_zup[0] / EXTERIOR_CELL_UNITS).floor() as i32;
    let gy = (position_zup[1] / EXTERIOR_CELL_UNITS).floor() as i32;
    (gx - center_grid.0).abs().max((gy - center_grid.1).abs()) <= radius.max(0)
}

#[cfg(test)]
mod persistent_grid_tests {
    use super::persistent_position_is_local;

    #[test]
    fn persistent_3d_filter_uses_floor_for_negative_cells_and_chebyshev_radius() {
        assert!(persistent_position_is_local(
            [4.25 * 4096.0, -19.75 * 4096.0, 0.0],
            (4, -20),
            0,
        ));
        assert!(persistent_position_is_local(
            [5.0 * 4096.0, -21.0 * 4096.0, 0.0],
            (4, -20),
            1,
        ));
        assert!(!persistent_position_is_local(
            [6.0 * 4096.0, -20.0 * 4096.0, 0.0],
            (4, -20),
            1,
        ));
        assert!(!persistent_position_is_local([-0.01, 0.0, 0.0], (0, 0), 0,));
    }
}

/// Resumable main-thread application of one exterior cell.
///
/// Terrain, water, and FO4 precombined geometry are installed during
/// construction as one guaranteed-progress unit. The remaining placed REFRs
/// advance cooperatively through [`ReferenceLoadJob`].
pub(crate) struct ExteriorCellApplyJob {
    gx: i32,
    gy: i32,
    cell_root: EntityId,
    terrain_center: Option<Vec3>,
    precombined_spawned: usize,
    references: Option<Box<ReferenceLoadJob>>,
}

pub(crate) enum ExteriorCellApplyProgress {
    Pending(ExteriorCellApplyJob),
    Complete(OneCellLoadInfo),
}

impl ExteriorCellApplyJob {
    /// Reclaim every ECS/GPU object produced by an unfinished apply.
    pub(crate) fn cancel(mut self, world: &mut World, ctx: &mut VulkanContext) {
        if let Some(references) = self.references.take() {
            references.cancel(world);
        }
        super::unload_cell(world, ctx, self.cell_root);
    }
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

    // Find the best worldspace. Priority (D4-1 / #1655 reordered 2↔3):
    //   1. Caller-supplied `--wrld <name>` (case-insensitive EDID match).
    //   2. Worldspace that actually contains the requested grid coord — the
    //      user asked for a specific `--grid`, so a worldspace holding it must
    //      win over a hardcoded default.
    //   3. Preferred game-default list: WastelandNV (FNV), Wasteland
    //      (FO3 Capital Wasteland), Tamriel (Oblivion), Skyrim (Skyrim) —
    //      fallback only when no worldspace contains the grid.
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
                // D4-1 / #1655 — grid-containment runs BEFORE the preferred-
                // default list: the user asked for a specific `--grid`, so the
                // worldspace that actually contains that grid must win over the
                // hardcoded base-game default. Pre-fix, a `--grid` into an FO3
                // DLC worldspace (Anchorage / Point Lookout / Zeta) without
                // `--wrld` silently landed on Capital Wasteland because the
                // preferred list matched first. Also protects multi-plugin
                // loads where a DLC worldspace with many cells but no grid 0,0
                // would otherwise outvote the base game's Wasteland (#444).
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
                // Preferred base-game default — only when no worldspace
                // contains the requested grid (e.g. a stray grid coord).
                let preferred = ["wastelandnv", "wasteland", "tamriel", "skyrim"];
                preferred
                    .iter()
                    .find(|&&name| index.exterior_cells.contains_key(name))
                    .map(|s| s.to_string())
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

    // Resolve the worldspace-default water once (#1305 / OBL-D6-NEW-02).
    // Oblivion WRLD has no DNAM/height field, so the default height is the
    // global Tamriel sea level (Z=0); a worldspace advertises that it has
    // default water via its NAM2 `water_form`. FO3/FNV/Skyrim+/FO4 resolve
    // the height from the worldspace's own parsed DNAM second f32 instead
    // (see `default_water_for_worldspace`). Cells without XCLW inherit
    // this in `load_one_exterior_cell`.
    let (default_water_height, default_water_type_form) =
        crate::env_translate::default_water_for_worldspace(
            record_index.cells.worldspaces.get(&worldspace_key),
            record_index.game,
        );

    Ok(ExteriorWorldContext {
        record_index: Arc::new(record_index),
        load_order: Arc::new(load_order),
        plugin_path: esm_path.to_string(),
        plugin_paths: plugin_paths.iter().map(|s| s.to_string()).collect(),
        worldspace_key,
        climate,
        default_weather,
        default_water_height,
        default_water_type_form,
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
    let mut mat_provider = mat_provider;
    let mut budget = FrameTimeBudget::unlimited();
    let Some(mut job) = ExteriorCellApplyJob::begin(
        wctx,
        gx,
        gy,
        world,
        ctx,
        tex_provider,
        mat_provider.as_deref_mut(),
        terrain_blas_accumulator,
        &mut budget,
    )?
    else {
        return Ok(None);
    };
    loop {
        match job.advance(
            wctx,
            world,
            ctx,
            tex_provider,
            mat_provider.as_deref_mut(),
            &mut budget,
        ) {
            ExteriorCellApplyProgress::Pending(next) => job = next,
            ExteriorCellApplyProgress::Complete(info) => return Ok(Some(info)),
        }
    }
}

impl ExteriorCellApplyJob {
    /// Install the non-reference phases and create the cell's reclaim root.
    ///
    /// This is deliberately one cooperative unit: the expensive high-cardinality
    /// phase is the REFR walk, while terrain/water/precombine preserve their
    /// existing atomic GPU submission boundaries.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin(
        wctx: &ExteriorWorldContext,
        gx: i32,
        gy: i32,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
        terrain_blas_accumulator: Option<&mut Vec<(u32, u32, u32)>>,
        budget: &mut FrameTimeBudget,
    ) -> anyhow::Result<Option<Self>> {
        // #1668 — surface GLOB runtime values once per streaming session so
        // CTDA "Use Global" comparands resolve. The exterior path streams many
        // cells from one `record_index`; build the lean `Globals` map only when
        // it isn't already present rather than rebuilding it each cell.
        super::load::ensure_globals_resource(world, &wctx.record_index.globals);

        let index = &wctx.record_index.cells;
        let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
            return Ok(None);
        };
        let Some(cell) = cells_map.get(&(gx, gy)) else {
            return Ok(None);
        };

        let cell_root = world.spawn();
        register_cell_root(world, cell_root);
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
            // #1855 — `spawn_terrain_mesh` already `log::warn!`s a mesh-upload
            // failure with its own (gx,gy) (the only realistic None cause in
            // production; the other — no GPU allocator — can't happen once
            // VulkanContext is constructed). This call-site line adds the
            // "cell had no terrain" correlation the per-cell log block above
            // otherwise lacks, so a partial-cell-render is diagnosable from
            // this cell's own log lines without cross-referencing the callee.
            if terrain::spawn_terrain_mesh(
                world,
                terrain::TerrainSpawnCtx {
                    ctx: &mut *ctx,
                    tex_provider,
                    landscape_textures: &index.landscape_textures,
                    landscape_texture_sets: &index.landscape_texture_sets,
                    blas_specs: &mut *blas_sink,
                },
                gx,
                gy,
                land,
            )
            .is_none()
            {
                log::warn!(
                    "  Cell ({gx},{gy}): terrain mesh spawn failed — no terrain will render"
                );
            }
        }
        // Water plane. A cell's explicit XCLW height wins; cells without one
        // inherit the worldspace default (Tamriel sea level Z=0, resolved into
        // `wctx.default_water_height` when the worldspace has a NAM2 water
        // form). 98% of Oblivion exterior cells have no XCLW and relied on
        // this fallback — without it every coastal/sea cell rendered dry
        // seabed (#1305 / OBL-D6-NEW-02). The XCLW "no water" sentinel is
        // already filtered to None at parse time (#1305 sentinel fix), so it
        // does not reach here as a spurious height.
        if let Some(water_height) = cell.water_height.or(wctx.default_water_height) {
            // Exterior cell origin in Y-up world coords. The helper composes
            // grid-scale and the Z-up→Y-up flip; see TD3-202 / #1112.
            let origin = cell_grid_to_world_yup(gx, gy);
            let half = water::exterior_half_extent();
            // #1855 — `spawn_water_plane` already `log::warn!`s a mesh-upload
            // failure, but without cell coords (it doesn't take gx/gy). This
            // call-site line adds the cell correlation so a dry cell that
            // should have had water is diagnosable from this cell's own log
            // lines.
            if water::spawn_water_plane(
                world,
                ctx,
                tex_provider,
                &wctx.record_index.waters,
                water_height,
                cell.water_type_form.or(wctx.default_water_type_form),
                (origin.x + half, origin.z - half),
                half,
                blas_sink,
            )
            .is_none()
            {
                log::warn!("  Cell ({gx},{gy}): water plane spawn failed — no water will render");
            }
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

        // FO4+ PreCombined Mesh spawn (#1221 / D3-NEW-02). Mirrors the
        // interior loader's Phase-3a. `cell.precombined_mesh_hashes` and
        // `cell.absorbed_refs` are populated post-#1220 by the exterior
        // walker in `wrld.rs`. M49 complete: CSG geometry is resolved and
        // spawned; the absorption gate honors the XPRI list, suppressing
        // per-REFR rendering of baked architecture on Commonwealth tiles.
        //
        // `cell_grid_to_world_yup(gx, gy)` is the per-tile world-space
        // origin (Y-up); the bake's cell-local coords are translated by
        // this offset. Without it every tile's bake would stack at the
        // world origin (#1222).
        let cell_origin = cell_grid_to_world_yup(gx, gy);
        let (pc_spawned, _pc_misses) = super::precombined::spawn_precombined_meshes(
            cell,
            cell_origin,
            world,
            ctx,
            tex_provider,
            mat_provider,
            // M49 — active plugin (Data dir + CSG fallback); the owning plugin
            // per cell form-id selects the actual `<Plugin> - Geometry.csg` and
            // the `_oc.nif` path. #1590.
            &wctx.plugin_path,
            &wctx
                .plugin_paths
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );

        // Mid-cell terrain ground point — only meaningful for the
        // initial-load camera-positioning path used by the bulk loader.
        let terrain_center = cell.landscape.as_ref().map(|land| {
            let mid_height = land.heights[16 * 33 + 16];
            let world_x = gx as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
            let world_y = gy as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
            // Z-up → Y-up: (x, height, -y), plus 200 units above ground.
            Vec3::new(world_x, mid_height + 200.0, -world_y)
        });
        let last_entity = world.next_entity_id();
        stamp_cell_root_range(world, cell_root, first_entity, last_entity);
        budget.complete_unit();

        Ok(Some(Self {
            gx,
            gy,
            cell_root,
            terrain_center,
            precombined_spawned: pc_spawned,
            references: None,
        }))
    }

    /// Advance placed references until the frame budget expires.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance(
        mut self,
        wctx: &ExteriorWorldContext,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> ExteriorCellApplyProgress {
        let index = &wctx.record_index.cells;
        let cell = index
            .exterior_cells
            .get(&wctx.worldspace_key)
            .and_then(|cells| cells.get(&(self.gx, self.gy)))
            .expect("exterior cell existed when its apply job began");
        let absorbed = super::precombined::absorbed_refs_or_empty(
            &cell.absorbed_refs,
            self.precombined_spawned,
        );
        let label = format!("exterior({},{})", self.gx, self.gy);
        let first_entity = world.next_entity_id();
        let progress = load_references_budgeted(
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
            absorbed,
            self.references.take(),
            budget,
        );
        stamp_cell_root_range(world, self.cell_root, first_entity, world.next_entity_id());

        match progress {
            ReferenceLoadProgress::Pending(references) => {
                self.references = Some(references);
                flush_pending_cell_textures(ctx);
                ExteriorCellApplyProgress::Pending(self)
            }
            ReferenceLoadProgress::Complete(result) => {
                let center = self.terrain_center.unwrap_or(result.center);
                ExteriorCellApplyProgress::Complete(OneCellLoadInfo {
                    cell_root: self.cell_root,
                    center,
                })
            }
        }
    }
}

// `default_water_for_worldspace` (+ its #1305 / OBL-D6-NEW-02 per-game
// default-water regressions) moved to the EXAL boundary in
// `crate::env_translate`; the tests moved with it.
