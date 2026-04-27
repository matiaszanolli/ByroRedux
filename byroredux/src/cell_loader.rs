//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    CellRoot, GlobalTransform, LightSource, Material, MeshHandle, ParticleEmitter, TextureHandle,
    Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::asset_provider::{
    merge_bgsm_into_mesh, resolve_texture, MaterialProvider, TextureProvider,
};
use crate::components::{
    AlphaBlend, CellLightingRes, DarkMapHandle, Decal, ExtraTextureMaps, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided, WeatherDataRes, WeatherTransitionRes,
};

#[path = "cell_loader_refr.rs"]
mod refr;
#[path = "cell_loader_load_order.rs"]
mod load_order;
#[path = "cell_loader_nif_import_registry.rs"]
mod nif_import_registry;
// Re-exports keep the existing `super::*` test imports working and let
// the rest of `cell_loader` reach these items unqualified.
pub(crate) use nif_import_registry::{CachedNifImport, NifImportRegistry};
pub(crate) use refr::{
    build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements, RefrTextureOverlay,
};
use load_order::{parse_record_indexes_in_load_order, plugin_for_form_id};

/// Result of loading a cell.
#[allow(dead_code)]
pub struct CellLoadResult {
    pub cell_name: String,
    pub entity_count: usize,
    /// Number of **mesh-bearing entities** spawned by this cell load —
    /// i.e. the count of `world.insert(entity, MeshHandle(...))` calls
    /// in `spawn_placed_instances` for this cell's references. Stable
    /// across repeat loads of the same cell (unlike the NIF-parse
    /// cache, which reports 0 on a second load even though the cell
    /// still spawns all its entities). Useful as a telemetry baseline:
    /// FNV Prospector Saloon should produce 784 here on every load,
    /// matching the draw-count. See #477 (FNV-3-L2).
    pub mesh_count: usize,
    /// Bounding box center of all placed objects (Y-up, for camera positioning).
    pub center: Vec3,
    /// Interior cell lighting (ambient + directional).
    pub lighting: Option<byroredux_plugin::esm::cell::CellLighting>,
    /// Weather data for exterior cells (from WRLD→CLMT→WTHR chain).
    pub weather: Option<byroredux_plugin::esm::records::WeatherRecord>,
    /// Climate record for exterior cells (sunrise/sunset timing bytes +
    /// weather probability table). Drives the time-of-day interpolator
    /// in `weather_system` so Capital Wasteland and Mojave run on their
    /// own schedules. See #463.
    pub climate: Option<byroredux_plugin::esm::records::ClimateRecord>,
    /// Owner token for every entity this load produced. Pass to
    /// [`unload_cell`] to tear the cell down (despawn entities + free
    /// mesh/BLAS/texture resources). See #372.
    pub cell_root: EntityId,
}

/// Stamp every entity spawned in `first..last` (exclusive) with
/// `CellRoot(cell_root)`. The range is obtained from
/// [`World::next_entity_id`] before/after the load. `cell_root` itself
/// also gets the component so it's picked up by the same query in
/// [`unload_cell`].
fn stamp_cell_root(world: &mut World, cell_root: EntityId, first: EntityId, last: EntityId) {
    world.insert(cell_root, CellRoot(cell_root));
    for eid in first..last {
        // `insert` is overwrite-safe, and entities that were never
        // given any component never created a CellRoot storage entry
        // — the row just stays in the sparse set for lookup.
        world.insert(eid, CellRoot(cell_root));
    }
}

/// Tear down a cell: despawn every entity owned by `cell_root` and
/// release the mesh/BLAS/texture GPU resources they referenced.
///
/// Handles are not reused — dropped mesh/texture slots remain as
/// placeholders in the registries to guarantee that any dangling
/// `GpuInstance.mesh_id` / `texture_index` can't reappear pointing at
/// a new mesh or texture. Entity IDs likewise grow monotonically (see
/// `World::despawn` docs). See #372.
///
/// Texture handles are refcounted (#524): each `resolve_texture` acquisition
/// bumps the `TextureEntry.ref_count` inside the registry, and this
/// function calls `drop_texture` once per entity-held handle. Shared
/// textures across still-resident cells survive an unload because the
/// remaining holders keep the refcount positive. M40 doorwalking needs
/// this — without it, cell A's unload would flip cell B's shared
/// clutter textures to the checkerboard.
#[allow(dead_code)] // exposed for scripting / doorwalking wiring (M40)
pub fn unload_cell(world: &mut World, ctx: &mut VulkanContext, cell_root: EntityId) {
    // Collect victims that the query iterator can see. Hold the lock
    // only for the iteration, then release before calling despawn
    // (which takes `&mut World`).
    let victims: Vec<EntityId> = {
        let Some(q) = world.query::<CellRoot>() else {
            return;
        };
        q.iter()
            .filter(|(_, root)| root.0 == cell_root)
            .map(|(eid, _)| eid)
            .collect()
    };

    // Gather mesh handles (HashSet — per-cell mesh buffers are unique,
    // a HashSet is only guarding against double-drops within one cell).
    //
    // Texture handles use a Vec instead: each entity's `resolve_texture`
    // bumped the registry's refcount by one; symmetric release means
    // one `drop_texture` call per component holder, no dedup. See
    // #524.
    let mut mesh_handles: HashSet<u32> = HashSet::new();
    let mut texture_drops: Vec<u32> = Vec::new();
    let mut terrain_tile_slots: HashSet<u32> = HashSet::new();
    let fallback_tex = ctx.texture_registry.fallback();
    let push_tex_drop = |handle: u32, sink: &mut Vec<u32>| {
        if handle != 0 && handle != fallback_tex {
            sink.push(handle);
        }
    };
    if let Some(mq) = world.query::<MeshHandle>() {
        for &eid in &victims {
            if let Some(mh) = mq.get(eid) {
                mesh_handles.insert(mh.0);
            }
        }
    }
    if let Some(tq) = world.query::<TextureHandle>() {
        for &eid in &victims {
            if let Some(th) = tq.get(eid) {
                push_tex_drop(th.0, &mut texture_drops);
            }
        }
    }
    if let Some(nq) = world.query::<NormalMapHandle>() {
        for &eid in &victims {
            if let Some(nh) = nq.get(eid) {
                push_tex_drop(nh.0, &mut texture_drops);
            }
        }
    }
    if let Some(dq) = world.query::<DarkMapHandle>() {
        for &eid in &victims {
            if let Some(dh) = dq.get(eid) {
                push_tex_drop(dh.0, &mut texture_drops);
            }
        }
    }
    if let Some(eq) = world.query::<ExtraTextureMaps>() {
        for &eid in &victims {
            if let Some(extra) = eq.get(eid) {
                push_tex_drop(extra.glow, &mut texture_drops);
                push_tex_drop(extra.detail, &mut texture_drops);
                push_tex_drop(extra.gloss, &mut texture_drops);
                push_tex_drop(extra.parallax, &mut texture_drops);
                push_tex_drop(extra.env, &mut texture_drops);
                push_tex_drop(extra.env_mask, &mut texture_drops);
            }
        }
    }
    if let Some(ttq) = world.query::<TerrainTileSlot>() {
        for &eid in &victims {
            if let Some(slot) = ttq.get(eid) {
                terrain_tile_slots.insert(slot.0);
            }
        }
    }

    // Sky textures live on `SkyParamsRes` (a Resource), not an ECS
    // component, so the per-victim sweep above can't reach them. The
    // bindless indices were acquired via `texture_registry.load_dds`
    // (sun) and `acquire_by_path` (cloud layers) at scene load time —
    // each bumped the registry refcount once. Without symmetric drops
    // every cell-cell transition leaks 4 cloud + 1 sun texture (#626).
    // The slot list is owned by `SkyParamsRes::texture_indices` so adding
    // a new slot updates both sites in lockstep.
    if let Some(sky) = world.try_resource::<SkyParamsRes>() {
        for idx in sky.texture_indices() {
            push_tex_drop(idx, &mut texture_drops);
        }
    }
    // Cell-scoped state resources hold no texture refs but get replaced
    // on the next `world.insert_resource` at cell load — clear them on
    // unload so a between-load query doesn't see stale state.
    world.remove_resource::<SkyParamsRes>();
    world.remove_resource::<CellLightingRes>();
    world.remove_resource::<WeatherDataRes>();
    world.remove_resource::<WeatherTransitionRes>();

    // Free terrain tile slots FIRST — late frames-in-flight reading the
    // SSBO then see either stale-but-valid data (if the slot was
    // reallocated) or the same data (no reuse this frame), rather than
    // undefined. See #470.
    //
    // Each slot owns 8 layer texture refcounts that `resolve_texture`
    // bumped via `acquire_by_path` at allocation time. The slot itself
    // isn't an ECS component, so the per-victim `TextureHandle` sweep
    // above can't reach those refs; capture them from the freed slot
    // and add them to `texture_drops` so the GPU release loop below
    // hands them off to `texture_registry.drop_texture`. Without this,
    // a 7×7 WastelandNV reload leaks ~150 texture refcounts (#627).
    for &slot in &terrain_tile_slots {
        if let Some(layer_indices) = ctx.free_terrain_tile(slot) {
            for idx in layer_indices {
                push_tex_drop(idx, &mut texture_drops);
            }
        }
    }

    // Free GPU resources. BLAS entries are keyed by mesh handle, so
    // `drop_blas` runs first over the same set. Order matters: BLAS
    // must be detached from any TLAS before its mesh's VkBuffer is
    // queued for destruction — both use the same MAX_FRAMES_IN_FLIGHT
    // countdown, which covers the overlap.
    if let Some(ref mut accel) = ctx.accel_manager {
        for &mh in &mesh_handles {
            accel.drop_blas(mh);
        }
        // #495 — the shared BLAS build scratch buffer is grow-only
        // across the process lifetime; a single peek at an 80–200 MB
        // scratch mesh (FO4 LOD terrain, Skyrim draugr skeletons,
        // Starfield `Saturn.nif`) permanently pins that much
        // DEVICE_LOCAL VRAM. Cell unload is a safe boundary — no BLAS
        // builds are in flight here — so shrink the scratch to the
        // new post-drop peak. SAFETY: we're on the main thread and no
        // BLAS build command buffer is currently referencing the
        // shared scratch (builds run synchronously through fenced
        // one-time command buffers). Skip when the allocator hasn't
        // been initialised yet (headless / pre-init test paths).
        if let Some(allocator) = ctx.allocator.as_ref() {
            unsafe {
                accel.shrink_blas_scratch_to_fit(&ctx.device, allocator);
            }
        }
    }
    for &mh in &mesh_handles {
        ctx.mesh_registry.drop_mesh(mh);
    }
    for &th in &texture_drops {
        ctx.texture_registry.drop_texture(&ctx.device, th);
    }

    // Remove every surviving component row for the victim entities.
    let victim_count = victims.len();
    for eid in victims {
        world.despawn(eid);
    }

    log::info!(
        "Cell unload: {} entities, {} meshes, {} texture refs released (cell_root {})",
        victim_count,
        mesh_handles.len(),
        texture_drops.len(),
        cell_root,
    );
}

/// Load an interior cell with explicit master plugins.
///
/// `masters` is an ordered list of master ESM paths (base game first,
/// then any required DLC masters); `esm_path` is the main plugin
/// being loaded (DLC or mod). Each plugin's FormIDs are remapped to
/// global load-order indices before being merged into a single cell
/// index, so cross-plugin REFRs (e.g. a Dawnguard interior placing a
/// Skyrim.esm STAT) resolve correctly.
///
/// Pre-#561 the cell loader only accepted a single ESM and silently
/// rendered empty interiors when REFRs pointed into a missing master.
/// This entry point closes the audit's SK-D6-01 gap by threading
/// `parse_esm_with_load_order` through the cell-loader pipeline.
///
/// On unresolved REFR `base_form_id` lookups, the warning summary now
/// names the missing plugin so the failure mode is diagnosable
/// instead of silent. See M46.0 / #561.
pub fn load_cell_with_masters(
    masters: &[String],
    esm_path: &str,
    cell_editor_id: &str,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> anyhow::Result<CellLoadResult> {
    // Mark the high-water entity id before loading. Everything spawned
    // by this load (including the designated cell_root at the end) gets
    // CellRoot stamped on it for later unload. See #372.
    let first_entity = world.next_entity_id();

    // 1. Parse the ESM(s) into a single merged cell index. Empty
    //    `masters` list reduces to single-plugin behaviour (FormIDs
    //    pass through unchanged via the remap's self-reference path).
    let plugin_paths: Vec<&str> = masters
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(esm_path))
        .collect();
    // SK-D6-02 / #566 — use the full-record parser so the LGTM
    // lighting-template fallback can resolve through
    // `EsmIndex.lighting_templates`. Pre-#566 this path only loaded the
    // cell index, which couldn't see LGTM records and silently dropped
    // the XCLL-absent fallback. The cost is bounded: ~1 s extra to
    // parse the surrounding categories on FNV / Skyrim, paid once per
    // cell load.
    let (index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;

    // 2. Find the cell.
    let cell_key = cell_editor_id.to_ascii_lowercase();
    let cell = index.cells.cells.get(&cell_key).ok_or_else(|| {
        // List available cells for debugging.
        let available: Vec<&str> = index
            .cells
            .cells
            .values()
            .take(20)
            .map(|c| c.editor_id.as_str())
            .collect();
        anyhow::anyhow!(
            "Cell '{}' not found. {} interior cells available. Examples: {:?}",
            cell_editor_id,
            index.cells.cells.len(),
            available,
        )
    })?;

    log::info!(
        "Loading cell '{}' (form {:08X}): {} placed references",
        cell.editor_id,
        cell.form_id,
        cell.references.len(),
    );

    // 3. Load placed references.
    let result = load_references(
        &cell.references,
        &index.cells,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &cell.editor_id,
        &load_order,
    );

    // SK-D6-02 / #566 — LGTM lighting-template fallback. Vanilla
    // Skyrim ships interior cells (Solitude inn cluster, Dragonsreach
    // throne room, Markarth cells) that omit XCLL and rely on this
    // template chain. Pre-#566 the LTMP FormID was unparsed, so the
    // fallback never fired and these cells rendered with the engine
    // default ambient.
    let resolved_lighting = resolve_cell_lighting(cell, &index);
    log::info!("Cell lighting: {:?}", resolved_lighting);

    // Reserve a dedicated root entity and stamp CellRoot on every
    // entity in [first_entity, last_entity). The stamp is sparse-set
    // backed, so entities that never received any component simply
    // don't show up in the CellRoot storage — fine.
    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);

    Ok(CellLoadResult {
        cell_name: cell.editor_id.clone(),
        entity_count: result.entity_count,
        mesh_count: result.mesh_count,
        center: result.center,
        lighting: resolved_lighting,
        weather: None,
        climate: None,
        cell_root,
    })
}

/// Resolve a cell's lighting against the ESM index, applying the
/// XCLL → LTMP → engine-default fallback chain (SK-D6-02 / #566).
///
/// 1. **Explicit XCLL wins.** Every Skyrim+/FNV/FO3/Oblivion CELL that
///    authors `XCLL` returns its parsed `CellLighting` verbatim — the
///    template path is never consulted.
/// 2. **LGTM template synthesises a CellLighting.** When the cell has
///    no XCLL but its `LTMP` resolves through `index.lighting_templates`,
///    the LgtmRecord's ambient / directional / fog scalars project into
///    a fresh `CellLighting`. Fields the LGTM stub doesn't carry
///    (directional_rotation, ambient cube, specular) stay at their
///    pre-XCLL defaults — directional_rotation `[0, 0]` matches a
///    sun-from-+X cell origin and the Skyrim-extended optionals stay
///    `None` (the renderer falls back to legacy single-color ambient
///    when they're absent, the same path FO3/FNV cells take).
/// 3. **No XCLL and no resolvable LGTM** → `None` (engine default).
pub(crate) fn resolve_cell_lighting(
    cell: &esm::cell::CellData,
    index: &esm::records::EsmIndex,
) -> Option<esm::cell::CellLighting> {
    if let Some(lit) = cell.lighting.clone() {
        return Some(lit);
    }
    let template_form = cell.lighting_template_form?;
    let template = index.lighting_templates.get(&template_form)?;
    Some(esm::cell::CellLighting {
        ambient: template.ambient,
        directional_color: template.directional,
        // LGTM doesn't carry directional rotation. Sun-from-+X origin
        // is what FO3/FNV cells defaulted to before #379 added explicit
        // rotation parsing — same fallback shape here.
        directional_rotation: [0.0, 0.0],
        fog_color: template.fog_color,
        fog_near: template.fog_near,
        fog_far: template.fog_far,
        directional_fade: template.directional_fade,
        fog_clip: template.fog_clip,
        fog_power: template.fog_power,
        // Skyrim extended fields (ambient cube, specular, light fade,
        // fog far color) ride on the 92-byte XCLL only. The current
        // LgtmRecord stub doesn't extract them; future LGTM expansion
        // can fill these in without touching the fallback's call shape.
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    })
}

/// Reusable per-worldspace context for streaming cell loads.
///
/// Built once per session (`build_exterior_world_context`) — holds the
/// parsed `EsmIndex` snapshot, the global load-order list, the chosen
/// worldspace's lowercase EDID key, and the resolved
/// climate + default-weather records for that worldspace. Cheap to
/// clone the `Arc`s into a streaming worker thread so the worker can
/// look up cell records, base forms, and landscape textures without
/// re-parsing.
///
/// Pre-#M40 the bulk loader rebuilt all of this every call. The
/// streaming system needs a stable handle so the per-cell loader
/// (`load_one_exterior_cell`) is cheap enough to call once per cell
/// boundary crossed.
#[allow(dead_code)] // `record_index` / `load_order` consumed by streaming worker (M40 Phase 1a)
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
#[allow(dead_code)] // `cell_root` consumed by streaming `WorldStreamingState` (M40 Phase 1a)
pub struct OneCellLoadInfo {
    pub cell_root: EntityId,
    pub entity_count: usize,
    pub mesh_count: usize,
    /// Mid-cell terrain ground point in Y-up world space (only used by
    /// the bulk loader to seat the initial camera).
    pub center: Vec3,
    pub gx: i32,
    pub gy: i32,
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
        .map(|climate| {
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
            climate
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
    let mut terrain_entities = 0usize;
    let mut local_blas: Vec<(u32, u32, u32)> = Vec::new();
    let blas_sink: &mut Vec<(u32, u32, u32)> = match terrain_blas_accumulator {
        Some(acc) => acc,
        None => &mut local_blas,
    };
    if let Some(ref land) = cell.landscape {
        if let Some(count) = terrain::spawn_terrain_mesh(
            world,
            ctx,
            tex_provider,
            &index.landscape_textures,
            gx,
            gy,
            land,
            blas_sink,
        ) {
            terrain_entities += count;
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

    // Spawn placed references. Pre-#M40 every grid load went through a
    // single `load_references` over all 49 cells' refs; per-cell calls
    // share the process-lifetime `NifImportRegistry` so cross-cell mesh
    // re-use still hits the cache (#381).
    let label = format!("exterior({},{})", gx, gy);
    let result = load_references(
        &cell.references,
        index,
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

    Ok(Some(OneCellLoadInfo {
        cell_root,
        entity_count: result.entity_count + terrain_entities,
        mesh_count: result.mesh_count + terrain_entities,
        center,
        gx,
        gy,
    }))
}

/// Result of loading references from one or more cells.
struct RefLoadResult {
    entity_count: usize,
    mesh_count: usize,
    center: Vec3,
}

/// Shared reference-loading pipeline: resolve base forms, load NIFs, spawn entities.
///
/// `load_order` holds the global plugin basenames (lowercase) — used
/// only to enrich the loud-fail diagnostic when a REFR's
/// `base_form_id` doesn't resolve. Pass `&[]` for legacy single-plugin
/// callers; the cell loader entry points (`load_cell_with_masters`,
/// `load_exterior_cells_with_masters`) thread the real load order.
/// See M46.0 / #561.
fn load_references(
    refs: &[esm::cell::PlacedRef],
    index: &esm::cell::EsmCellIndex,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    label: &str,
    load_order: &[String],
) -> RefLoadResult {
    let mut entity_count = 0;
    // Number of mesh-bearing entities (those that receive a
    // `MeshHandle` insert in `spawn_placed_instances`). Distinct from
    // `entity_count` which also sums LIGH-only / effect-sprite-light
    // entities that carry no renderable mesh. See #477.
    let mut mesh_entity_count = 0usize;
    // Process-lifetime cache of parsed-and-imported NIF scene data
    // (`NifImportRegistry`, #381). Each unique mesh is parsed exactly
    // once across the entire process — subsequent placements of the
    // same model in this cell *and* later cells reuse the shared
    // `Arc` and only pay the per-reference spawn cost (vertex upload,
    // texture resolve, entity insertion). A `None` entry records a
    // mesh that failed to parse — we skip subsequent placements of
    // the same model silently. Per-cell hit/miss accounting (the
    // numbers logged at end-of-cell) is computed against the lifetime
    // counters by snapshotting them at entry.
    let (cache_hits_at_entry, cache_misses_at_entry, cache_size_at_entry) = {
        let reg = world.resource::<NifImportRegistry>();
        (reg.hits, reg.misses, reg.len())
    };
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    let mut stat_miss = 0u32;
    let mut stat_hit = 0u32;
    let mut enable_skipped = 0u32;
    // Bounded sample of distinct miss FormIDs so an operator can
    // cross-reference in xEdit without flipping the whole log to
    // debug. Cap at 20 unique IDs; duplicates (same FormID placed
    // repeatedly across a worldspace) get deduped. See #386.
    let mut stat_miss_sample: Vec<u32> = Vec::with_capacity(20);

    // Per-call accumulators — committed to `NifImportRegistry` in a
    // single `resource_mut` borrow after the loop instead of acquiring
    // the write lock on every REFR. Previously every iteration took
    // `world.resource_mut::<NifImportRegistry>()` (write lock + atomic
    // CAS) even on the hot cache-hit path; for Prospector Saloon's 809
    // REFRs that was 809 write-lock cycles serialising nothing. See
    // #523.
    let mut this_call_hits: u64 = 0;
    let mut this_call_misses: u64 = 0;
    // Parses performed during this call. Merged into the registry at
    // end-of-function. `pending_new.get` shadows the registry read so
    // subsequent iterations of the loop see this call's own parses
    // without re-entering the registry.
    let mut pending_new: HashMap<String, Option<Arc<CachedNifImport>>> = HashMap::new();
    // Cache keys that resolved through a registry hit this call. Bulk-
    // bumped through `NifImportRegistry::touch_keys` at end-of-load so
    // recently-used entries float above the LRU eviction watermark.
    // #635 / FNV-D3-05.
    let mut pending_hits: Vec<String> = Vec::new();
    // Embedded-clip handles registered during this call. Mirrors
    // `pending_new` so the spawn loop can reach a freshly-registered
    // handle through the per-call shadow before the end-of-load
    // batched commit pushes it into `NifImportRegistry.clip_handles`.
    // Each parsed NIF whose `embedded_clip` is `Some` produces one
    // entry (after the conversion + `AnimationClipRegistry::add`
    // round-trip). Subsequent REFRs of the same model — within this
    // load or across cells — reach the same `u32` handle without
    // re-running `convert_nif_clip`. See #544 / #261.
    let mut pending_clip_handles: HashMap<String, u32> = HashMap::new();

    for placed_ref in refs {
        // Skip REFRs whose XESP gating would keep them hidden under
        // the parents-assumed-enabled heuristic: inverted XESP children
        // are visible only when the parent is *disabled*, so under the
        // default they stay off. Non-inverted XESP children fall through
        // and render. See #471 (flipped #349's over-hiding predicate)
        // — long-term fix is a two-pass loader that reads the parent
        // REFR's own 0x0800 "initial disabled" flag.
        if let Some(ep) = placed_ref.enable_parent {
            if ep.default_disabled() {
                enable_skipped += 1;
                continue;
            }
        }

        // Convert the outer REFR's placement (Z-up Bethesda → Y-up
        // renderer). For normal REFRs this is the spawn transform; for
        // SCOL REFRs it's the parent transform the child placements
        // compose against.
        let outer_pos = Vec3::new(
            placed_ref.position[0],
            placed_ref.position[2],
            -placed_ref.position[1],
        );
        let outer_rot = euler_zup_to_quat_yup(
            placed_ref.rotation[0],
            placed_ref.rotation[1],
            placed_ref.rotation[2],
        );
        let outer_scale = placed_ref.scale;

        // Build per-REFR texture overlay once. Shared across every
        // synthetic SCOL child — FO4 REFRs that overlay textures at the
        // SCOL level apply the same swap to every child placement.
        // #584.
        let refr_overlay = {
            let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
            build_refr_texture_overlay(placed_ref, index, mat_provider.as_deref_mut(), &mut pool)
        };

        // Compose REFR expansion from composite-record helpers:
        //   1. PKIN (#589) — Pack-In bundle fans out to one synth per
        //      `CNAM` content at the outer transform.
        //   2. SCOL (#585) — Static Collection fans out to one synth
        //      per `ONAM/DATA` placement when no cached `CM*.NIF`.
        //   3. Default — single synth at the outer transform.
        //
        // First expander that fires wins; `expand_scol_placements`
        // already returns the single-entry default when the base form
        // isn't a SCOL, so the chain closes cleanly.
        let synth_refs = expand_pkin_placements(
            placed_ref.base_form_id,
            outer_pos,
            outer_rot,
            outer_scale,
            index,
        )
        .unwrap_or_else(|| {
            expand_scol_placements(
                placed_ref.base_form_id,
                outer_pos,
                outer_rot,
                outer_scale,
                index,
            )
        });

        for (child_form_id, ref_pos, ref_rot, ref_scale) in synth_refs {
            let stat = match index.statics.get(&child_form_id) {
                Some(s) => {
                    stat_hit += 1;
                    s
                }
                None => {
                    stat_miss += 1;
                    // Collect a bounded sample so the summary line can
                    // surface actual FormIDs without pulling down a
                    // full RUST_LOG=debug run. Linear dedup is fine
                    // for 20 entries. See #386.
                    if stat_miss_sample.len() < 20 && !stat_miss_sample.contains(&child_form_id) {
                        stat_miss_sample.push(child_form_id);
                    }
                    log::debug!("REFR base {:08X} not in statics table", child_form_id);
                    continue;
                }
            };

            // Update bounds from the (possibly SCOL-composed) placement.
            bounds_min = bounds_min.min(ref_pos);
            bounds_max = bounds_max.max(ref_pos);

            // Spawn light-only entities (LIGH with no mesh).
            if stat.model_path.is_empty() {
                if let Some(ref ld) = stat.light_data {
                    let entity = world.spawn();
                    world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
                    world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
                    world.insert(
                        entity,
                        LightSource {
                            radius: light_radius_or_default(ld.radius),
                            color: ld.color,
                            flags: ld.flags,
                        },
                    );
                    entity_count += 1;
                }
                continue;
            }

            // Skip non-renderable meshes: editor markers, effect
            // sprites, fog. Still spawn the ESM light entity if this
            // LIGH record carries one — the effect mesh is visual-only
            // but the point light is real.
            let model_lower = stat.model_path.to_ascii_lowercase();

            // Extract the filename (after the last \ or /) for prefix matching.
            let filename = model_lower
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(&model_lower);

            if filename.starts_with("marker")
                || filename.starts_with("xmarker")
                || filename.starts_with("defaultsetmarker")
                || filename.starts_with("doormarker")
                || filename.starts_with("northmarker")
                || filename.starts_with("prisonmarker")
                || filename.starts_with("travelmarker")
                || filename.starts_with("roommarker")
                || filename.starts_with("vatsmarker")
            {
                continue;
            }

            if model_lower.contains("fxlightrays")
                || model_lower.contains("fxlight")
                || model_lower.contains("fxfog")
            {
                if let Some(ref ld) = stat.light_data {
                    let entity = world.spawn();
                    world.insert(entity, Transform::from_translation(ref_pos));
                    world.insert(entity, GlobalTransform::new(ref_pos, Quat::IDENTITY, 1.0));
                    world.insert(
                        entity,
                        LightSource {
                            radius: light_radius_or_default(ld.radius),
                            color: ld.color,
                            flags: ld.flags,
                        },
                    );
                    entity_count += 1;
                }
                continue;
            }

            let model_path =
                if model_lower.starts_with("meshes\\") || model_lower.starts_with("meshes/") {
                    stat.model_path.clone()
                } else {
                    format!("meshes\\{}", stat.model_path)
                };

            // Fetch parsed+imported NIF from the process-lifetime
            // registry, or load+parse once. Three-tier lookup (#523):
            //   1. `pending_new` — this call's own parses, zero lock
            //      cost.
            //   2. Registry read-lock — a shared borrow that doesn't
            //      serialise against concurrent readers.
            //   3. Parse outside any lock, insert into `pending_new`;
            //      the merge into the registry happens in a single
            //      write lock after the loop.
            //
            // Previously this block took `resource_mut` (write lock)
            // on every iteration even on the hit path; see #523 / #381
            // for the wider cache history.
            let cache_key = model_path.to_ascii_lowercase();
            let cached = if let Some(entry) = pending_new.get(&cache_key).cloned() {
                this_call_hits += 1;
                entry
            } else {
                let reg_entry = {
                    let reg = world.resource::<NifImportRegistry>();
                    reg.get(&cache_key).cloned()
                };
                match reg_entry {
                    Some(entry) => {
                        this_call_hits += 1;
                        // Mark for LRU touch at the end-of-load batched
                        // commit so frequently-revisited meshes don't
                        // get evicted under `BYRO_NIF_CACHE_MAX`. The
                        // batched flush keeps the read path on a shared
                        // lock — preserves the #523 invariant.
                        pending_hits.push(cache_key.clone());
                        entry
                    }
                    None => {
                        // Slow-path: parse outside any registry borrow.
                        // Take the StringPool write lock only for the
                        // parse + intern + BGSM merge — the read lock
                        // on `NifImportRegistry` was released at the
                        // close of the `reg_entry` scope above, so the
                        // two locks never overlap. See #609.
                        let parsed = match tex_provider.extract_mesh(&model_path) {
                            Some(d) => {
                                let mut pool =
                                    world.resource_mut::<byroredux_core::string::StringPool>();
                                parse_and_import_nif(
                                    &d,
                                    &model_path,
                                    mat_provider.as_deref_mut(),
                                    &mut pool,
                                )
                            }
                            None => {
                                log::debug!("NIF not found in BSA: '{}'", model_path);
                                None
                            }
                        };
                        this_call_misses += 1;
                        // #544 — register the embedded animation clip
                        // exactly once per parsed NIF, before stashing
                        // into `pending_new`. Subsequent REFRs of this
                        // model reach the handle through the per-call
                        // shadow (`pending_clip_handles`) or, on later
                        // cell loads, through `NifImportRegistry::
                        // clip_handle_for` after the end-of-load
                        // commit. The conversion runs at most once per
                        // unique model across the process — matches
                        // the loose-NIF path's one-clip-per-NIF
                        // invariant from #261.
                        if let Some(ref cached) = parsed {
                            if let Some(nif_clip) = cached.embedded_clip.as_ref() {
                                let handle = {
                                    let mut pool = world.resource_mut::<
                                        byroredux_core::string::StringPool,
                                    >();
                                    let clip = crate::anim_convert::convert_nif_clip(
                                        nif_clip, &mut pool,
                                    );
                                    drop(pool);
                                    let mut clip_reg = world.resource_mut::<
                                        byroredux_core::animation::AnimationClipRegistry,
                                    >();
                                    clip_reg.add(clip)
                                };
                                pending_clip_handles.insert(cache_key.clone(), handle);
                            }
                        }
                        pending_new.insert(cache_key.clone(), parsed.clone());
                        parsed
                    }
                }
            };
            let Some(cached) = cached else { continue };

            // #544 — embedded animation-clip handle for this REFR's
            // model. Three-tier lookup mirrors the cache:
            //   1. `pending_clip_handles` — registered earlier in this
            //      call's slow path.
            //   2. `NifImportRegistry::clip_handle_for` — registered
            //      by an earlier cell load. Read-only / shared lock.
            //   3. `None` — the cached NIF authored no controllers.
            // Subsequent REFRs of the same model in this same load
            // hit case (1) and never touch the registry write path.
            let clip_handle = pending_clip_handles
                .get(&cache_key)
                .copied()
                .or_else(|| {
                    world
                        .resource::<NifImportRegistry>()
                        .clip_handle_for(&cache_key)
                });

            let count = spawn_placed_instances(
                world,
                ctx,
                &cached,
                tex_provider,
                ref_pos,
                ref_rot,
                ref_scale,
                stat.light_data.as_ref(),
                refr_overlay.as_ref(),
                clip_handle,
            );
            entity_count += count;
            mesh_entity_count += count;
        }
    }

    let center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
    // Commit the accumulated counters + pending entries in a single
    // write lock. Stats snapshot happens in the same scope so the log
    // line below reflects post-commit numbers. See #523. `insert`
    // drives `parsed_count` / `failed_count` and runs LRU eviction; we
    // touch hit keys first so they bump above the LRU watermark before
    // any new inserts fight them for cache space (#635 / FNV-D3-05).
    let (this_cell_hits, this_cell_misses, this_cell_unique, lifetime_hit_rate) = {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        reg.hits += this_call_hits;
        reg.misses += this_call_misses;
        reg.touch_keys(pending_hits.iter().map(String::as_str));
        for (key, entry) in pending_new {
            reg.insert(key, entry);
        }
        // #544 — commit per-call clip handles into the process-lifetime
        // registry. Future cell loads of the same NIF reach the
        // memoised handle through `clip_handle_for` without
        // re-converting the channel arrays.
        for (key, handle) in pending_clip_handles {
            reg.set_clip_handle(key, handle);
        }
        let new_entries = reg.len().saturating_sub(cache_size_at_entry);
        (
            reg.hits.saturating_sub(cache_hits_at_entry),
            reg.misses.saturating_sub(cache_misses_at_entry),
            new_entries,
            reg.hit_rate_pct(),
        )
    };
    log::info!(
        "'{}' loaded: {} entities, {} new unique meshes parsed, NIF cache hits/misses {}/{} this cell ({:.1}% lifetime hit rate), {} statics hits, {} statics misses",
        label,
        entity_count,
        this_cell_unique,
        this_cell_hits,
        this_cell_misses,
        lifetime_hit_rate,
        stat_hit,
        stat_miss,
    );
    log::info!(
        "  Bounds: min=[{:.0},{:.0},{:.0}] max=[{:.0},{:.0},{:.0}] size=[{:.0},{:.0},{:.0}] center=[{:.0},{:.0},{:.0}]",
        bounds_min.x, bounds_min.y, bounds_min.z,
        bounds_max.x, bounds_max.y, bounds_max.z,
        dims.x, dims.y, dims.z,
        center.x, center.y, center.z,
    );
    if stat_miss > 0 {
        // Log the bounded sample at info level so the miss types are
        // diagnosable without flipping to debug. Common causes:
        // leveled-list targets (LVLI/LVLN/LVLC — parsed elsewhere, not
        // in `index.statics`), master-ESM-only forms, and mod-added
        // records without a loaded master. See #386 for the roadmap
        // toward leveled-list resolution.
        let sample_str = stat_miss_sample
            .iter()
            .map(|id| {
                let plugin = plugin_for_form_id(*id, load_order).unwrap_or("???");
                format!("{:08X} (from '{}')", id, plugin)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let truncation_marker = if (stat_miss_sample.len() as u32) < stat_miss {
            format!(", … +{} more", stat_miss - stat_miss_sample.len() as u32)
        } else {
            String::new()
        };
        // #561 — when load_order has more than one plugin, also break
        // down misses by plugin so the user can tell whether a missing
        // master is the cause (top byte points at a plugin in the
        // load order whose statics table is missing the FormID =
        // unresolved cross-plugin override) vs. a leveled-list /
        // dynamic-form target (top byte points at a loaded plugin
        // whose statics table genuinely doesn't carry the form).
        let plugin_breakdown = if load_order.len() > 1 {
            let mut by_plugin: std::collections::HashMap<&str, u32> =
                std::collections::HashMap::new();
            for id in &stat_miss_sample {
                let plugin = plugin_for_form_id(*id, load_order).unwrap_or("???");
                *by_plugin.entry(plugin).or_insert(0) += 1;
            }
            let mut rows: Vec<_> = by_plugin.into_iter().collect();
            rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
            let s = rows
                .iter()
                .map(|(p, n)| format!("{}={}", p, n))
                .collect::<Vec<_>>()
                .join(", ");
            format!(" — by plugin (in sample): {}", s)
        } else {
            String::new()
        };
        log::warn!(
            "  {} base forms not found in statics table (sample: {}{}){}",
            stat_miss,
            sample_str,
            truncation_marker,
            plugin_breakdown,
        );
    }
    if enable_skipped > 0 {
        log::info!(
            "  {} REFRs skipped via XESP enable-parent gating (#349)",
            enable_skipped,
        );
    }

    RefLoadResult {
        entity_count,
        // Mesh-bearing entities spawned this load. Pre-#477 this was
        // `reg.len() - cache_size_at_entry` — "newly parsed NIFs" —
        // which reported 0 on a repeat load of the same cell despite
        // spawning hundreds of entities. The new count is stable
        // across repeat loads and matches the rasterizer draw budget
        // (modulo instancing). The parse-work telemetry moved to the
        // `this_cell_unique` log line above; `NifImportRegistry.hits`
        // / `.misses` remain the source of truth for cache analysis.
        mesh_count: mesh_entity_count,
        center,
    }
}

/// Parse + import a NIF scene once. Returns `None` on parse failure
/// or when the scene has zero useful geometry. All per-block parse
/// warnings and the truncation message (if any) are emitted exactly
/// once per unique NIF at this step — subsequent placements read
/// from the cache without re-parsing. See runtime-spam incident from
/// the `AnvilHeinrichOakenHallsHouse` trace.
fn parse_and_import_nif(
    nif_data: &[u8],
    label: &str,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut byroredux_core::string::StringPool,
) -> Option<Arc<CachedNifImport>> {
    let scene = match byroredux_nif::parse_nif(nif_data) {
        Ok(s) => {
            log::debug!("Parsed NIF '{}': {} blocks", label, s.len());
            if s.truncated {
                log::warn!(
                    "  NIF '{}' parsed with truncation — downstream import will \
                     work from the partial block list",
                    label
                );
            }
            s
        }
        Err(e) => {
            log::warn!("Failed to parse NIF '{}': {}", label, e);
            return None;
        }
    };

    // BSXFlags bit 5 (0x20) marks the entire NIF as an editor marker —
    // invisible in-game objects like XMarker, PrisonMarker, etc.
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    if bsx & 0x20 != 0 {
        log::debug!("Skipping editor marker NIF '{}'", label);
        return None;
    }

    let (mut meshes, collisions) = byroredux_nif::import::import_nif_with_collision(&scene, pool);
    // FO4+ external material resolution (#493). Walk once at cache-fill
    // time so every REFR sharing this NIF sees the merged texture paths.
    // NIF fields take precedence; only empty slots are filled from the
    // resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut meshes {
            merge_bgsm_into_mesh(mesh, provider, pool);
        }
    }
    let lights = byroredux_nif::import::import_nif_lights(&scene);
    let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
    let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
    // Cell-load path doesn't yet attach `Name` components or a
    // per-placement subtree root to spawned mesh entities, so the
    // AnimationStack's name-keyed subtree lookup can't anchor onto the
    // flat-spawn hierarchy. Clips extracted here are captured on the
    // cache entry for a follow-up wiring pass (add placement-root
    // entities + parent meshes under them, then attach a scoped
    // AnimationPlayer per placement). See #261. The loose-NIF
    // `load_nif_bytes` path already consumes embedded clips end-to-end.
    if let Some(ref clip) = embedded_clip {
        log::debug!(
            "NIF '{}' has {} embedded controllers ({} float + {} color + {} bool) \
             — captured on cache; cell-loader spawn wiring is a follow-up",
            label,
            clip.float_channels.len() + clip.color_channels.len() + clip.bool_channels.len(),
            clip.float_channels.len(),
            clip.color_channels.len(),
            clip.bool_channels.len(),
        );
    }
    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
        particle_emitters,
        embedded_clip,
    }))
}

/// `true` when an `ImportedLight` has a non-trivial diffuse colour
/// contribution and therefore would actually spawn a `LightSource`
/// entity. Authored-off placeholder lights (FNV light-bulb meshes
/// park a zero-colour `NiPointLight` to mark intent without baking
/// the colour; the ESM LIGH base record carries the real value)
/// fail this predicate so the ESM-fallback gate in
/// `spawn_placed_instances` can attach the authoritative LightSource
/// instead.
///
/// Threshold of `1e-4` matches the in-loop check exactly — kept as
/// a free function so #632's regression tests can pin the predicate
/// without standing up a full Vulkan context.
fn is_spawnable_nif_light(light: &byroredux_nif::import::ImportedLight) -> bool {
    light.color[0] + light.color[1] + light.color[2] >= 1e-4
}

/// Count NIF lights that would survive `is_spawnable_nif_light`. The
/// ESM-fallback gate uses this instead of `nif_lights.is_empty()` so
/// a NIF carrying only zero-colour placeholders still receives the
/// ESM LIGH-authored `LightSource` (#632).
fn count_spawnable_nif_lights(nif_lights: &[byroredux_nif::import::ImportedLight]) -> usize {
    nif_lights
        .iter()
        .filter(|l| is_spawnable_nif_light(l))
        .count()
}

/// Sanitise a placement-time light radius before it reaches the GPU
/// `position_radius.w` slot. A non-positive value would zero the
/// shader's `effectiveRange = radius * 4.0` attenuation window
/// (light contributes nothing) AND collapse the shadow-ray jitter
/// disk to the dead 1.5u floor (RT-9 / #672 — penumbra degenerates
/// to a hard point shadow if the light ever crosses the
/// `contribution >= 0.001` gate).
///
/// `4096.0` matches the cell-scale fallback already used at the
/// NIF-direct spawn site for ambient / directional placeholders
/// without an authored radius. Authored Bethesda XCLL radii are
/// 256–4096 units, so this default is a "covers the cell" net,
/// not a typical value — a malformed LIGH record that ships
/// `radius=0` becomes visible rather than silently invisible.
#[inline]
fn light_radius_or_default(radius: f32) -> f32 {
    if radius > 0.0 { radius } else { 4096.0 }
}

/// Spawn entities for every mesh / light / collision in a pre-parsed NIF
/// with a parent REFR transform applied. Each NIF sub-mesh has its own
/// local transform from the scene graph which composes on top of the
/// REFR placement transform. `cached` is produced by
/// `parse_and_import_nif` and shared across all placements of the same
/// model via `Arc`.
fn spawn_placed_instances(
    world: &mut World,
    ctx: &mut VulkanContext,
    cached: &CachedNifImport,
    tex_provider: &TextureProvider,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    light_data: Option<&esm::cell::LightData>,
    refr_overlay: Option<&RefrTextureOverlay>,
    clip_handle: Option<u32>,
) -> usize {
    use byroredux_core::ecs::{Name, Parent};
    use byroredux_renderer::Vertex;

    let imported = &cached.meshes;
    let collisions = &cached.collisions;
    let nif_lights = &cached.lights;
    let mut count = 0;

    // #544 — per-REFR placement root entity. Mesh entities spawned
    // below become its children with NIF-local transforms; the
    // transform-propagation system composes the REFR transform onto
    // them each frame. Pre-#544 every mesh was anchored independently
    // at the world-space-composed transform, which prevented the
    // embedded animation clip's subtree walk from finding the spawned
    // entities (no `Parent` / `Children` edges, no `Name` to bind
    // node-keyed channels against). The placement root carries the
    // composed REFR transform AND the world-space `GlobalTransform`
    // up front so any read that hits the entity before the next
    // propagation tick still sees the right placement (e.g. BLAS
    // build during `build_blas_batched` later in the function).
    let placement_root = world.spawn();
    world.insert(
        placement_root,
        Transform::new(ref_pos, ref_rot, ref_scale),
    );
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
    );
    // Pre-compute how many NIF lights will actually spawn. The
    // ESM-fallback gate at the bottom of this function uses this
    // count instead of `nif_lights.is_empty()` so a NIF that
    // authored only zero-colour placeholders (FNV light-bulb
    // meshes are the audit's example) still receives the ESM
    // LIGH-authored LightSource. Pre-#632 the gate checked the
    // raw array length, so placeholders prevented the fallback
    // and the cell rendered dark even when both NIF intent and
    // ESM authority agreed it should be lit.
    let spawned_nif_lights = count_spawnable_nif_lights(nif_lights);

    // Spawn per-mesh NiLight blocks as LightSource entities. Parented
    // through the reference transform so torches/candles inside cell
    // refs contribute to the live GpuLight buffer. See issue #156.
    // When the ESM LIGH record provides an authored radius, prefer it
    // over the NIF-computed attenuation_radius (which often returns 2048
    // for NiPointLights with constant-only attenuation coefficients).
    let esm_radius = light_data.as_ref().map(|ld| ld.radius);

    for light in nif_lights {
        // Skip lights whose diffuse contribution is effectively zero —
        // these are usually authored-off placeholders. The audit's
        // FNV Prospector Saloon evidence: light-bulb meshes ship a
        // disabled NiPointLight to mark intent without baking colour;
        // the ESM LIGH base record carries the real authored colour.
        // Predicate kept in lockstep with `is_spawnable_nif_light`.
        if !is_spawnable_nif_light(light) {
            continue;
        }
        let nif_pos = Vec3::new(
            light.translation[0],
            light.translation[1],
            light.translation[2],
        );
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        // Pick the authored radius source, then sanitise. Pre-#672
        // an `esm_radius == Some(0.0)` slipped through as a real
        // `0 * ref_scale = 0` and the light became invisible at
        // the shader (zero attenuation, dead-floor jitter disk).
        // Falling through to `light_radius_or_default` keeps the
        // 4096u cell-scale fallback that previously only fired on
        // the NIF-side `else` branch.
        let raw_radius = match esm_radius {
            Some(r) if r > 0.0 => r * ref_scale,
            _ if light.radius > 0.0 => light.radius * ref_scale,
            _ => 0.0,
        };
        let radius = light_radius_or_default(raw_radius);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(final_pos));
        world.insert(entity, GlobalTransform::new(final_pos, Quat::IDENTITY, 1.0));
        world.insert(
            entity,
            LightSource {
                radius,
                color: light.color,
                flags: 0,
            },
        );
    }

    // Spawn particle emitter entities (#401). One ECS entity per
    // detected NiParticleSystem, positioned at the composed REFR + NIF-
    // local transform. The heuristic preset is picked from the nearest
    // named ancestor in the NIF (host_name):
    //   spark/ember/cinder → embers (small, bright, additive — checked
    //                                FIRST so "FireSparks" doesn't fall
    //                                into the larger flame body)
    //   torch/fire/flame/brazier/candle → torch_flame
    //   smoke/steam/ash      → smoke
    //   magic/enchant/sparkle/glow → magic_sparkles
    //   fallback             → torch_flame so the audit's "every torch
    //                          invisible" failure is resolved end-to-
    //                          end even when the host node carries no
    //                          descriptive name.
    // Mirrored in `byroredux/src/scene.rs` — keep both lists in lockstep.
    // The proper data-driven fix (NIF-authored colour curves via
    // `NiPSysColorModifier` → `NiColorData`) stays open at #707; this
    // is the heuristic band-aid that landed first.
    for em in &cached.particle_emitters {
        let nif_pos = Vec3::new(
            em.local_position[0],
            em.local_position[1],
            em.local_position[2],
        );
        let world_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let host = em.host_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let preset = if host.contains("spark") || host.contains("ember") || host.contains("cinder")
        {
            ParticleEmitter::embers()
        } else if host.contains("torch")
            || host.contains("fire")
            || host.contains("flame")
            || host.contains("brazier")
            || host.contains("candle")
        {
            ParticleEmitter::torch_flame()
        } else if host.contains("smoke") || host.contains("steam") || host.contains("ash") {
            ParticleEmitter::smoke()
        } else if host.contains("magic")
            || host.contains("enchant")
            || host.contains("sparkle")
            || host.contains("glow")
        {
            ParticleEmitter::magic_sparkles()
        } else {
            ParticleEmitter::torch_flame()
        };
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(world_pos));
        world.insert(entity, GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0));
        world.insert(entity, preset);
    }

    // Spawn collision entities from NiNode collision data.
    // Guard against parry3d panics from nested composite shapes — some
    // Bethesda NIFs have deeply nested bhkCompressedMeshShape hierarchies
    // that parry3d's Compound shape rejects. Skip those shapes gracefully.
    for coll in collisions {
        let nif_pos = Vec3::new(
            coll.translation[0],
            coll.translation[1],
            coll.translation[2],
        );
        let nif_quat = Quat::from_xyzw(
            coll.rotation[0],
            coll.rotation[1],
            coll.rotation[2],
            coll.rotation[3],
        );
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let final_rot = ref_rot * nif_quat;
        let final_scale = ref_scale * coll.scale;

        // parry3d panics on nested Compound shapes. Clone inside
        // catch_unwind so a bad shape doesn't kill the entire load.
        let shape_result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coll.shape.clone()));
        let shape = match shape_result {
            Ok(s) => s,
            Err(_) => {
                log::warn!(
                    "Skipping collision shape (nested composite) at ({:.0},{:.0},{:.0})",
                    final_pos.x,
                    final_pos.y,
                    final_pos.z
                );
                continue;
            }
        };

        let entity = world.spawn();
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(
            entity,
            GlobalTransform::new(final_pos, final_rot, final_scale),
        );
        world.insert(entity, shape);
        world.insert(entity, coll.body.clone());
    }

    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    for mesh in imported {
        let num_verts = mesh.positions.len();
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                // Drop alpha — current `Vertex` color is 3-channel; the
                // alpha lane lives on `ImportedMesh::colors[i][3]` for
                // when the renderer extends to a 4-channel vertex (#618).
                let color3 = if i < mesh.colors.len() {
                    let c = mesh.colors[i];
                    [c[0], c[1], c[2]]
                } else {
                    [1.0, 1.0, 1.0]
                };
                Vertex::new(
                    mesh.positions[i],
                    color3,
                    if i < mesh.normals.len() {
                        mesh.normals[i]
                    } else {
                        [0.0, 1.0, 0.0]
                    },
                    if i < mesh.uvs.len() {
                        mesh.uvs[i]
                    } else {
                        [0.0, 0.0]
                    },
                )
            })
            .collect();

        let mesh_handle = {
            let alloc = ctx.allocator.as_ref().unwrap();
            match ctx.mesh_registry.upload_scene_mesh(
                &ctx.device,
                alloc,
                &ctx.graphics_queue,
                ctx.transfer_pool,
                &vertices,
                &mesh.indices,
                ctx.device_caps.ray_query_supported,
                None,
            ) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("Failed to upload mesh: {}", e);
                    continue;
                }
            }
        };

        // Collect BLAS specs for batched build after the loop.
        blas_specs.push((mesh_handle, num_verts as u32, mesh.indices.len() as u32));

        // Effective texture slot paths. REFR overlay (XATO/XTNM/XTXR)
        // wins over the NIF-authored paths when present; for slots the
        // overlay left empty the cached NIF's texture rides through.
        // `None` on both sides means this slot has no texture. See #584.
        //
        // Both inputs hold `FixedString` handles (#609). Resolve through
        // the engine `StringPool` once here so the per-slot Material
        // construction below can stay on `Option<String>` paths.
        // Resolved strings are owned `String`s — one allocation per
        // populated slot per entity, much better than the pre-#609
        // ~3-allocations-per-slot per entity from the redundant
        // ImportedMesh.clone path.
        let ov = refr_overlay;
        let pool_read = world.resource::<byroredux_core::string::StringPool>();
        let resolve_owned = |sym: Option<byroredux_core::string::FixedString>| -> Option<String> {
            sym.and_then(|s| pool_read.resolve(s))
                .map(|s| s.to_string())
        };
        let eff_texture_path =
            resolve_owned(ov.and_then(|o| o.diffuse).or(mesh.texture_path));
        let eff_normal_map = resolve_owned(ov.and_then(|o| o.normal).or(mesh.normal_map));
        let eff_glow_map = resolve_owned(ov.and_then(|o| o.glow).or(mesh.glow_map));
        let eff_gloss_map = resolve_owned(ov.and_then(|o| o.specular).or(mesh.gloss_map));
        let eff_parallax_map = resolve_owned(ov.and_then(|o| o.height).or(mesh.parallax_map));
        let eff_env_map = resolve_owned(ov.and_then(|o| o.env).or(mesh.env_map));
        let eff_env_mask = resolve_owned(ov.and_then(|o| o.env_mask).or(mesh.env_mask));
        let eff_material_path =
            resolve_owned(ov.and_then(|o| o.material_path).or(mesh.material_path));
        // The detail/dark slots come straight from `mesh`; resolve the
        // same way so the downstream `resolve` closure that walks them
        // stays uniform.
        let eff_detail_map = resolve_owned(mesh.detail_map);
        let eff_dark_map = resolve_owned(mesh.dark_map);
        drop(pool_read);

        // Load texture (shared resolve: cache → BSA → fallback).
        let tex_handle = resolve_texture(ctx, tex_provider, eff_texture_path.as_deref());

        // #544 — mesh entities now sit in the NIF-local frame and
        // descend from the placement root. The transform-propagation
        // system composes `placement_root` (the REFR transform) onto
        // them each frame to produce the world-space `GlobalTransform`
        // the renderer / BLAS / lighting consume. Pre-#544 every mesh
        // pre-baked the REFR composition into its own `Transform`,
        // which left it anchored to nothing the embedded animation
        // clip could walk to.
        //
        // The composed `final_*` values are still computed up front
        // because the `GlobalTransform` we seed on the mesh has to
        // match what the propagation pass will compute on the first
        // tick — anything that reads `GlobalTransform` before then
        // (renderer's per-frame data collection, BLAS build below)
        // gets a correctly-placed value in the meantime.
        let nif_quat = Quat::from_xyzw(
            mesh.rotation[0],
            mesh.rotation[1],
            mesh.rotation[2],
            mesh.rotation[3],
        );
        let nif_pos = Vec3::new(
            mesh.translation[0],
            mesh.translation[1],
            mesh.translation[2],
        );

        // World-space placement (parent_rot * (parent_scale *
        // child_pos) + parent_pos) — used only to seed the initial
        // `GlobalTransform`. `Transform` itself stays NIF-local so
        // the propagation pass produces the same value next tick.
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let final_rot = ref_rot * nif_quat;
        let final_scale = ref_scale * mesh.scale;

        // Diagnostic: log meshes with significant NIF-internal offsets
        // (these are wall/structural pieces most likely to show positioning issues)
        let nif_offset_len = nif_pos.length();
        if nif_offset_len > 50.0 {
            log::debug!(
                "  NIF offset {:.0} for mesh {:?}: nif_pos=({:.0},{:.0},{:.0}) \
                 final=({:.0},{:.0},{:.0})",
                nif_offset_len,
                mesh.name,
                nif_pos.x,
                nif_pos.y,
                nif_pos.z,
                final_pos.x,
                final_pos.y,
                final_pos.z,
            );
        }

        let entity = world.spawn();
        // NIF-local Transform for hierarchy propagation; world-space
        // GlobalTransform for first-tick consumers. See #544.
        world.insert(entity, Transform::new(nif_pos, nif_quat, mesh.scale));
        world.insert(
            entity,
            GlobalTransform::new(final_pos, final_rot, final_scale),
        );
        // Parent/Children edge → embedded animation clip's subtree
        // walk discovers this mesh through `placement_root`.
        world.insert(entity, Parent(placement_root));
        crate::helpers::add_child(world, placement_root, entity);
        // Name from `ImportedMesh.name` so the clip's node-keyed
        // channels (`FixedString` interned at parse time, #340)
        // resolve through `build_subtree_name_map` to this entity.
        // Pre-#544 the cell-loader path skipped this insert, so even
        // if `Parent` had been wired the channels would have failed
        // their name lookup and silently no-op'd.
        if let Some(ref name) = mesh.name {
            let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
            let sym = pool.intern(name);
            drop(pool);
            world.insert(entity, Name(sym));
        }
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                diffuse_color: mesh.diffuse_color,
                ambient_color: mesh.ambient_color,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: eff_normal_map.clone(),
                texture_path: eff_texture_path.clone(),
                material_path: eff_material_path.clone(),
                glow_map: eff_glow_map.clone(),
                detail_map: eff_detail_map.clone(),
                gloss_map: eff_gloss_map.clone(),
                dark_map: eff_dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
                z_test: mesh.z_test,
                z_write: mesh.z_write,
                z_function: mesh.z_function,
                shader_type_fields: if mesh.shader_type_fields.is_empty() {
                    None
                } else {
                    Some(Box::new(mesh.shader_type_fields.to_core()))
                },
            },
        );
        // Load and attach normal map if the material specifies one.
        if let Some(ref nmap_path) = eff_normal_map {
            let h = resolve_texture(ctx, tex_provider, Some(nmap_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, NormalMapHandle(h));
            }
        }
        // Load and attach dark/lightmap if the material specifies one (#264).
        if let Some(ref dark_path) = eff_dark_map {
            let h = resolve_texture(ctx, tex_provider, Some(dark_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, DarkMapHandle(h));
            }
        }
        // #399 — Resolve glow / detail / gloss texture handles. All three
        // default to 0 (no map; shader falls through to inline material
        // constants). The component is only attached when at least one
        // path resolved to a real handle, keeping the SparseSet small
        // for the bulk of meshes that have no extra maps.
        let mut resolve = |path: &Option<String>| -> u32 {
            path.as_deref()
                .map(|p| resolve_texture(ctx, tex_provider, Some(p)))
                .filter(|&h| h != ctx.texture_registry.fallback())
                .unwrap_or(0)
        };
        let glow_h = resolve(&eff_glow_map);
        let detail_h = resolve(&eff_detail_map);
        let gloss_h = resolve(&eff_gloss_map);
        let parallax_h = resolve(&eff_parallax_map);
        let env_h = resolve(&eff_env_map);
        let env_mask_h = resolve(&eff_env_mask);
        if glow_h != 0
            || detail_h != 0
            || gloss_h != 0
            || parallax_h != 0
            || env_h != 0
            || env_mask_h != 0
        {
            world.insert(
                entity,
                ExtraTextureMaps {
                    glow: glow_h,
                    detail: detail_h,
                    gloss: gloss_h,
                    parallax: parallax_h,
                    env: env_h,
                    env_mask: env_mask_h,
                    parallax_height_scale: mesh.parallax_height_scale.unwrap_or(0.04),
                    parallax_max_passes: mesh.parallax_max_passes.unwrap_or(4.0),
                },
            );
        }
        if mesh.has_alpha {
            world.insert(
                entity,
                AlphaBlend {
                    src_blend: mesh.src_blend_mode,
                    dst_blend: mesh.dst_blend_mode,
                },
            );
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        if mesh.is_decal {
            world.insert(entity, Decal);
        }
        // Attach ESM light_data ONLY if the NIF didn't actually spawn
        // any lights (avoids duplicates) and only on the first mesh
        // (avoids N copies when a lamp NIF has multiple sub-meshes).
        //
        // Pre-#632 this gated on `nif_lights.is_empty()` — wrong
        // because zero-colour placeholders take a slot in the array
        // but get filtered out at the spawn loop above. Cells with
        // light-bulb meshes (Prospector Saloon) rendered dark even
        // though both the NIF placeholder and the ESM LIGH record
        // agreed there should be a light. Track real spawns instead.
        if let Some(ld) = light_data {
            if spawned_nif_lights == 0 && count == 0 {
                world.insert(
                    entity,
                    LightSource {
                        radius: light_radius_or_default(ld.radius),
                        color: ld.color,
                        flags: ld.flags,
                    },
                );
            }
        }
        count += 1;
    }

    // Batched BLAS build: single GPU submission for all meshes in this cell.
    if !blas_specs.is_empty() {
        let built = ctx.build_blas_batched(&blas_specs);
        log::info!("Cell BLAS batch: {built}/{} meshes", blas_specs.len());
    }

    // #544 — bind the embedded animation clip to this REFR. Mirrors
    // the loose-NIF path in `scene.rs::load_nif_bytes`. The clip
    // registration itself happens once per unique parsed NIF in
    // `load_references` (cached on `NifImportRegistry`); here we
    // just spawn one `AnimationPlayer` per placement so the
    // animation system's subtree walk finds this REFR's mesh
    // children. Without this insert, water UV scrolls / lava
    // emissive pulses / torch visibility flickers / fade-in alphas
    // all stay frozen on cell-rendered REFRs, while loose-NIF
    // imports of the same models animate correctly.
    if let Some(handle) = clip_handle {
        let player_entity = world.spawn();
        let mut player =
            byroredux_core::animation::AnimationPlayer::new(handle);
        player.root_entity = Some(placement_root);
        world.insert(player_entity, player);
    }

    count
}

/// Convert Euler angles (radians, Z-up Bethesda convention) to a Y-up quaternion.
///
/// Bethesda uses Gamebryo's **clockwise-positive** rotation convention:
///   R = Rz_cw(rz) · Ry_cw(ry) · Rx_cw(rx)
///
/// Since glam uses the standard counter-clockwise convention, each
/// CW rotation by angle t equals a CCW rotation by -t:
///   R = Rz_ccw(-rz) · Ry_ccw(-ry) · Rx_ccw(-rx)
///
/// Coordinate change C: (x,y,z)_zup → (x,z,-y)_yup conjugates each:
///   C · Rx(-rx) · C^T = Rx(-rx)     (x → x)
///   C · Ry(-ry) · C^T = Rz(ry)      (y → -z, double negate)
///   C · Rz(-rz) · C^T = Ry(-rz)     (z → y)
///
/// Result: R_yup = Ry(-rz) · Rz(ry) · Rx(-rx)
///
/// `pub(crate)` so non-REFR callers (XCLL directional lighting in
/// `scene.rs`, #380) can route authored Z-up Euler angles through the
/// same CW-convention helper instead of reinventing the spherical
/// math inline and drifting from the authored intent.
pub(crate) fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)
}

#[path = "cell_loader_terrain.rs"]
mod terrain;

#[cfg(test)]
#[path = "cell_loader_euler_zup_to_quat_yup_tests.rs"]
mod euler_zup_to_quat_yup_tests;

#[cfg(test)]
#[path = "cell_loader_nif_import_registry_tests.rs"]
mod nif_import_registry_tests;

#[cfg(test)]
#[path = "cell_loader_refr_texture_overlay_tests.rs"]
mod refr_texture_overlay_tests;

#[cfg(test)]
#[path = "cell_loader_pkin_expansion_tests.rs"]
mod pkin_expansion_tests;

#[cfg(test)]
#[path = "cell_loader_scol_expansion_tests.rs"]
mod scol_expansion_tests;

#[cfg(test)]
#[path = "cell_loader_terrain_splat_tests.rs"]
mod terrain_splat_tests;

#[cfg(test)]
#[path = "cell_loader_sky_params_cleanup_tests.rs"]
mod sky_params_cleanup_tests;

#[cfg(test)]
#[path = "cell_loader_nif_light_spawn_gate_tests.rs"]
mod nif_light_spawn_gate_tests;

#[cfg(test)]
#[path = "cell_loader_lgtm_fallback_tests.rs"]
mod lgtm_fallback_tests;

#[cfg(test)]
#[path = "cell_loader_placement_root_subtree_tests.rs"]
mod placement_root_subtree_tests;
