//! Worldspace selection and the per-worldspace context / single-cell loader
//! (split out of `exterior.rs`, #6271).

use super::*;

/// Preferred base-game worldspaces, ordered from newest supported Fallout
/// default through the Gamebryo/Creation-era defaults.
pub(super) const PREFERRED_WORLDSPACES: [&str; 4] = ["wastelandnv", "wasteland", "tamriel", "skyrim"];

pub(super) fn summarize_foreground_cells<T>(
    cells: Option<&HashMap<(i32, i32), T>>,
    requested_coord: (i32, i32),
    suggestion_limit: usize,
    classify: impl Fn(&T) -> (bool, usize, usize),
) -> ExteriorForegroundReadiness {
    let content = |coord, cell: &T| {
        let (has_landscape, authored_reference_count, precombined_mesh_count) = classify(cell);
        ExteriorCellContent {
            coord,
            has_landscape,
            authored_reference_count,
            precombined_mesh_count,
        }
    };
    let requested = cells
        .and_then(|cells| cells.get(&requested_coord))
        .map(|cell| content(requested_coord, cell));

    let mut nearest_viable = Vec::new();
    if requested
        .as_ref()
        .is_none_or(|requested| !requested.is_content_backed())
    {
        if let Some(cells) = cells {
            nearest_viable.extend(cells.iter().filter_map(|(&coord, cell)| {
                let content = content(coord, cell);
                content.is_content_backed().then_some(content)
            }));
            nearest_viable.sort_unstable_by_key(|cell| {
                let dx = cell.coord.0.abs_diff(requested_coord.0);
                let dy = cell.coord.1.abs_diff(requested_coord.1);
                (dx.max(dy), u64::from(dx) + u64::from(dy), cell.coord)
            });
            nearest_viable.truncate(suggestion_limit);
        }
    }

    ExteriorForegroundReadiness {
        requested_coord,
        requested,
        nearest_viable,
    }
}

pub(super) fn log_foreground_readiness(worldspace: &str, readiness: &ExteriorForegroundReadiness) {
    if let Some(content) = readiness
        .requested
        .as_ref()
        .filter(|content| content.is_content_backed())
    {
        log::info!(
            "Exterior foreground '{}' ({},{}): LAND={}, {} references, {} precombined meshes",
            worldspace,
            content.coord.0,
            content.coord.1,
            content.has_landscape,
            content.authored_reference_count,
            content.precombined_mesh_count,
        );
        return;
    }

    let reason = match readiness.requested.as_ref() {
        Some(_) => "CELL exists but has no LAND, references, or precombined meshes",
        None => "CELL is missing",
    };
    let suggestions = readiness
        .nearest_viable
        .iter()
        .map(|cell| {
            format!(
                "({},{})[LAND={}, refs={}, precombines={}]",
                cell.coord.0,
                cell.coord.1,
                cell.has_landscape,
                cell.authored_reference_count,
                cell.precombined_mesh_count,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    log::warn!(
        "Exterior foreground '{}' ({}, {}) is not content-backed: {}; nearest viable cells: {}. \
         Character mode will fall back to FlyCam unless --player explicitly overrides it",
        worldspace,
        readiness.requested_coord.0,
        readiness.requested_coord.1,
        reason,
        if suggestions.is_empty() {
            "none".to_string()
        } else {
            suggestions
        },
    );
}

/// Pick the exterior worldspace key. Priority (D4-1 / #1655, #2340):
///   1. Caller-supplied `--wrld <name>` (case-insensitive EDID match).
///   2. Worldspace(s) that contain the requested grid coord. A unique
///      match wins; ambiguous matches use the preferred game-default list,
///      then stable most-cells/name ordering.
///   3. When no worldspace contains the grid, the preferred game-default
///      list: WastelandNV (FNV), Wasteland (FO3 Capital Wasteland), Tamriel
///      (Oblivion), Skyrim (Skyrim).
///   4. Worldspace with the most cells (ultimate fallback).
///
/// Pre-fix the Wasteland EDID was missing, so `--esm Fallout3.esm
/// --grid 0,0` landed on the max-cells fallback and silently picked
/// the wrong worldspace when any DLC master added its own. See #444.
/// #5226 — an override that matches nothing is a CLI typo, not a hint to
/// run the heuristic: it errors naming the requested EDID and the loaded
/// set, instead of silently loading a different worldspace's cell (a
/// `MegatonWrld` typo on FO3 used to land on Wasteland's copy of the same
/// coordinate).
pub(super) fn select_worldspace_key<T>(
    exterior_cells: &std::collections::HashMap<String, std::collections::HashMap<(i32, i32), T>>,
    center_x: i32,
    center_y: i32,
    radius: i32,
    wrld_override: Option<&str>,
) -> anyhow::Result<String> {
    if let Some(requested) = wrld_override {
        if let Some(name) = exterior_cells
            .keys()
            .find(|candidate| candidate.eq_ignore_ascii_case(requested))
            .cloned()
        {
            log::info!("Using worldspace '{name}' (from --wrld override)");
            return Ok(name);
        }
        let mut available: Vec<&str> = exterior_cells.keys().map(String::as_str).collect();
        available.sort_unstable();
        anyhow::bail!(
            "--wrld '{}' matches no loaded worldspace (available: {})",
            requested,
            if available.is_empty() {
                "none".to_string()
            } else {
                available.join(", ")
            },
        );
    }

    let min_x = center_x.saturating_sub(radius);
    let max_x = center_x.saturating_add(radius);
    let min_y = center_y.saturating_sub(radius);
    let max_y = center_y.saturating_add(radius);
    let mut containing: Vec<_> = exterior_cells
        .iter()
        .filter(|(_, cells)| {
            cells
                .keys()
                .any(|(gx, gy)| *gx >= min_x && *gx <= max_x && *gy >= min_y && *gy <= max_y)
        })
        .collect();

    // Stable fallback: prefer the worldspace with more cells, then the
    // lexicographically first EDID. HashMap iteration order must never decide
    // which game world the same command loads (#2340).
    containing.sort_unstable_by(|(name_a, cells_a), (name_b, cells_b)| {
        cells_b
            .len()
            .cmp(&cells_a.len())
            .then_with(|| name_a.cmp(name_b))
    });

    if containing.len() > 1 {
        let candidates = containing
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        log::warn!(
            "Grid ({center_x},{center_y}) ±{radius} is present in multiple worldspaces: \
             {candidates}; applying the deterministic preferred/size tie-break"
        );
    }

    if let Some(name) = PREFERRED_WORLDSPACES.iter().find_map(|preferred| {
        containing
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(preferred))
            .map(|(candidate, _)| (*candidate).clone())
    }) {
        return Ok(name);
    }
    if let Some((name, _)) = containing.first() {
        return Ok((*name).clone());
    }

    if let Some(name) = PREFERRED_WORLDSPACES.iter().find_map(|preferred| {
        exterior_cells
            .keys()
            .find(|candidate| candidate.eq_ignore_ascii_case(preferred))
            .cloned()
    }) {
        return Ok(name);
    }

    let mut by_size: Vec<_> = exterior_cells.iter().collect();
    by_size.sort_unstable_by(|(name_a, cells_a), (name_b, cells_b)| {
        cells_b
            .len()
            .cmp(&cells_a.len())
            .then_with(|| name_a.cmp(name_b))
    });
    by_size
        .first()
        .map(|(name, _)| (*name).clone())
        .ok_or_else(|| anyhow::anyhow!("No worldspace found in plugin set"))
}


/// Build the once-per-session context for exterior streaming.
///
/// Parses every plugin in load order, picks the worldspace using the
/// same priority chain the bulk loader has used since #444 (override →
/// grid-coord match, with preferred-game tie-breaking → max cells), and
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
    // #6123 — the plugin set the session already parsed is reused as is; the
    // exterior-only landscape inheritance (#5505) is part of the shared parse.
    let (record_index, load_order) = parse_record_indexes_shared(&plugin_paths, true)?;
    let index = &record_index.cells;

    // Best worldspace per `select_worldspace_key`'s documented priority:
    // `--wrld` override (an unmatched one is an error, #5226), then
    // grid-containment → game defaults → most cells (#444).
    let worldspace_key = select_worldspace_key(
        &index.exterior_cells,
        center_x,
        center_y,
        radius,
        wrld_override,
    )?;

    log::info!(
        "Exterior world context built: worldspace '{}' (target ({},{}) ±{})",
        worldspace_key,
        center_x,
        center_y,
        radius,
    );
    let foreground_readiness = summarize_foreground_cells(
        index.exterior_cells.get(&worldspace_key),
        (center_x, center_y),
        5,
        |cell| {
            (
                cell.landscape.is_some(),
                cell.references.len(),
                cell.precombined_mesh_hashes.len(),
            )
        },
    );
    log_foreground_readiness(&worldspace_key, &foreground_readiness);

    // Resolve weather + climate: WRLD → CLMT → WTHR. Climate carries
    // per-worldspace TNAM sunrise/sunset hours so the TOD interpolator
    // runs on the right clock (#463). Default weather is the
    // highest-chance entry; mods use -1 as a sentinel / subtractive
    // weight (#476).
    //
    // #2450 / EXAL-02 — chases the WNAM parent-worldspace chain (gated on
    // the PNAM climate-inherit bit) when this worldspace authors no own
    // CNAM, instead of a flat lookup that always misses on inheriting
    // child worldspaces (Skyrim DLC/holdout worlds, FO4 sub-worlds,
    // Oblivion-plane worlds) and silently falls back to the procedural
    // default sky.
    // #5423 — every climate rung (WRLD link → region chain → the
    // Oblivion naming/richest fallbacks) settles in one table-shaped
    // boundary helper in env_translate, beside `resolve_worldspace_climate`
    // and `resolve_default_weather`; no `game ==` branch here.
    let climate_form = crate::env_translate::resolve_exterior_climate(
        index,
        &record_index.climates,
        &record_index.regions,
        &worldspace_key,
        center_x,
        center_y,
        record_index.game,
    );
    let climate = climate_form
        .and_then(|fid| record_index.climates.get(&fid).cloned())
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
    // #5424 — the whole default-weather rule, WTHS stand-in included,
    // lives in `resolve_default_weather` so this resolve and the per-cell
    // XCCM re-resolve pick by the same rule.
    let default_weather = climate.as_ref().and_then(|climate| {
        crate::env_translate::resolve_default_weather(climate, &record_index.weathers).map(
            |(wthr, chance)| {
                log::info!(
                    "Default weather: '{}' ({:08X}, chance {})",
                    wthr.editor_id,
                    wthr.form_id,
                    chance,
                );
                wthr.clone()
            },
        )
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
            &record_index.cells.worldspaces,
            &worldspace_key,
            record_index.game,
        );

    Ok(ExteriorWorldContext {
        record_index,
        load_order,
        plugin_path: esm_path.to_string(),
        plugin_paths: plugin_paths.iter().map(|s| s.to_string()).collect(),
        worldspace_key,
        climate,
        default_weather,
        default_water_height,
        default_water_type_form,
    })
}

/// The climate the center cell's regions carry (Oblivion's region→climate
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
#[allow(clippy::too_many_arguments)] // Streaming entry point: the world, GPU context, both asset providers and the accumulator are separate caller-owned borrows.
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
