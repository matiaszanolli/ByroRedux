//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    CellRoot, GlobalTransform, LightSource, Material, MeshHandle, ParticleEmitter, Resource,
    TextureHandle, Transform, World,
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

/// Parsed + imported NIF scene data cached per unique model path.
///
/// The cell loader parses each distinct mesh exactly once and stores
/// the imported scene in this struct — subsequent placements of the
/// same model (very common in Bethesda cells, e.g. 40 chairs sharing
/// one chair.nif) reuse the `Arc` instead of re-parsing. This cuts
/// the parser warning spam down from O(N placements) to O(M unique
/// meshes) and halves the parse CPU cost at load time.
struct CachedNifImport {
    meshes: Vec<byroredux_nif::import::ImportedMesh>,
    collisions: Vec<byroredux_nif::import::ImportedCollision>,
    lights: Vec<byroredux_nif::import::ImportedLight>,
    /// Particle emitters detected in the NIF scene graph (NiParticleSystem
    /// and friends). Carries NIF-local position + nearest named ancestor's
    /// name, which the spawn step composes with the REFR placement and
    /// translates to a heuristic [`ParticleEmitter`] preset. See #401.
    particle_emitters: Vec<byroredux_nif::import::ImportedParticleEmitterFlat>,
    /// Ambient animation clip collecting every mesh-embedded controller
    /// (alpha fade, UV scroll, visibility flicker, material colour pulse,
    /// shader float/colour). Shared across REFR placements: the clip
    /// handle is registered once per cache load; each placement spawns
    /// its own `AnimationPlayer` scoped to the spawned root entity so
    /// the subtree-local name lookup matches the authored node names.
    /// `None` when the NIF authored no supported controllers. See #261.
    ///
    /// Currently write-only on this path — the cell-loader spawn
    /// doesn't attach `Name` components or parent meshes under a
    /// placement root, so there's no subtree the `AnimationStack`
    /// can anchor the clip against yet. Field is retained so the
    /// follow-up wiring pass doesn't have to re-thread the parser.
    #[allow(dead_code)]
    embedded_clip: Option<byroredux_nif::anim::AnimationClip>,
}

/// Process-lifetime cache of parsed-and-imported NIF scenes keyed by
/// lowercased model path. Promotes the per-`load_references`
/// `import_cache` (#383) to a world-resource so cell-to-cell traversal
/// re-uses every previously-parsed mesh.
///
/// Without this, a second visit to a cell (or an adjacent cell sharing
/// 90% of its clutter meshes) re-parses every NIF — the Session 6
/// optimization's win is real *within* one cell but doesn't persist.
/// That cost is invisible today because cells load exactly once per
/// process lifetime, but turns into a HIGH-severity per-doorwalk stall
/// once #372 (despawn/unload) lands. See audit F3-11 / #381.
///
/// `None` entries record a model that failed to parse (or had zero
/// useful geometry) so we don't re-try the parse on every placement.
///
/// **Memory bound:** unbounded for now. A full FNV mesh-archive sweep
/// would resolve into ~14k entries totaling a few hundred MB at most;
/// the engine's other registries (texture, mesh) are similarly
/// unbounded. An LRU cap is a follow-up if memory pressure justifies
/// it, exposed via the `mesh.cache` debug command.
#[derive(Default)]
pub(crate) struct NifImportRegistry {
    cache: HashMap<String, Option<Arc<CachedNifImport>>>,
    /// Cumulative cache hits across the process lifetime.
    pub(crate) hits: u64,
    /// Cumulative cache misses (a parse was performed) across the
    /// process lifetime. `hits + misses == total NIF lookups`.
    pub(crate) misses: u64,
    /// Successfully-parsed entries currently in the cache. Mirrors
    /// `cache.values().filter(|v| v.is_some()).count()` for O(1) reads
    /// from the `mesh.cache` debug command.
    pub(crate) parsed_count: u64,
    /// Failed-parse entries currently in the cache (`None` entries).
    pub(crate) failed_count: u64,
}

impl NifImportRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Clear all entries (e.g. before a hard world reset). Counters are
    /// preserved so the debug command can still display lifetime stats.
    #[allow(dead_code)]
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.parsed_count = 0;
        self.failed_count = 0;
    }

    /// Total number of cached entries (parsed + failed).
    pub(crate) fn len(&self) -> usize {
        self.cache.len()
    }

    /// Hit rate as a percentage `[0, 100]`. `0.0` when no lookups have
    /// happened yet (avoid NaN).
    pub(crate) fn hit_rate_pct(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            100.0 * self.hits as f64 / total as f64
        }
    }
}

impl Resource for NifImportRegistry {}

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

/// Lowercase basename of a plugin path. Used as the global load-order
/// key (case-insensitive on Bethesda content).
fn plugin_basename_lc(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase()
}

/// Build the [`FormIdRemap`] that turns this plugin's local FormIDs
/// (top byte = mod-index in its own MASTERS list) into globally
/// load-order-indexed FormIDs (top byte = position in the load order
/// passed to the cell loader).
///
/// Returns `Err` when the plugin declares a master that isn't in the
/// global load order — that's a load-order misconfiguration the
/// caller must fix (ESMs must be loaded in order: every declared
/// master must be present and earlier).
///
/// See M46.0 / #561 / #445.
fn build_remap_for_plugin(
    plugin_path: &str,
    plugin_data: &[u8],
    plugin_index: usize,
    load_order: &[String],
) -> anyhow::Result<esm::reader::FormIdRemap> {
    let mut reader = esm::reader::EsmReader::new(plugin_data);
    let header = reader
        .read_file_header()
        .map_err(|e| anyhow::anyhow!("Failed to read TES4 header for '{}': {}", plugin_path, e))?;

    let master_indices: Vec<u8> = header
        .master_files
        .iter()
        .map(|m| {
            let m_lc = m.to_ascii_lowercase();
            load_order
                .iter()
                .position(|name| name == &m_lc)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Plugin '{}' declares master '{}' which is not in the load order — \
                         pass `--master {}` before `--esm`",
                        plugin_path,
                        m,
                        m,
                    )
                })
                .map(|i| i as u8)
        })
        .collect::<anyhow::Result<Vec<u8>>>()?;

    Ok(esm::reader::FormIdRemap {
        plugin_index: plugin_index as u8,
        master_indices,
    })
}

/// Parse a sequence of plugins in load order (masters first, main
/// plugin last) and return a single merged `EsmCellIndex` plus the
/// load-order plugin basenames.
///
/// Each plugin gets a [`FormIdRemap`] built against the global load
/// order so cross-plugin REFRs land in the merged maps under their
/// global FormIDs. Last-write-wins on key collisions per
/// [`EsmCellIndex::merge_from`] — a DLC overriding a master cell or
/// static record wins. See M46.0 / #561.
fn parse_cell_indexes_in_load_order(
    plugin_paths: &[&str],
) -> anyhow::Result<(esm::cell::EsmCellIndex, Vec<String>)> {
    let load_order: Vec<String> = plugin_paths.iter().map(|p| plugin_basename_lc(p)).collect();

    // Detect duplicates in the supplied load order — an easy CLI
    // misconfig that would silently make the second copy override
    // the first with itself.
    {
        let mut seen = std::collections::HashSet::with_capacity(load_order.len());
        for name in &load_order {
            if !seen.insert(name) {
                return Err(anyhow::anyhow!(
                    "Plugin '{}' appears twice in the load order — \
                     a plugin can only be passed once",
                    name
                ));
            }
        }
    }

    let mut merged = esm::cell::EsmCellIndex::default();
    for (idx, path) in plugin_paths.iter().enumerate() {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", path, e))?;
        log::info!(
            "Parsing plugin {}/{} '{}' ({:.1} MB) at load-order index {}…",
            idx + 1,
            plugin_paths.len(),
            path,
            bytes.len() as f64 / 1_048_576.0,
            idx,
        );
        let remap = build_remap_for_plugin(path, &bytes, idx, &load_order)?;
        let plugin_cells = esm::cell::parse_esm_cells_with_load_order(&bytes, Some(remap))?;
        merged.merge_from(plugin_cells);
    }
    Ok((merged, load_order))
}

/// Resolve a FormID's mod-index byte to the owning plugin's basename.
/// Used by the loud-fail diagnostic when a REFR's `base_form_id` is
/// unresolved — the audit's #561 completeness item: "name the missing
/// master" instead of silently rendering empty.
fn plugin_for_form_id(form_id: u32, load_order: &[String]) -> Option<&str> {
    let mod_index = (form_id >> 24) as usize;
    load_order.get(mod_index).map(|s| s.as_str())
}

/// Same shape as [`parse_cell_indexes_in_load_order`] but uses the
/// full `parse_esm_with_load_order` walker so the broader
/// [`esm::records::EsmIndex`] (climates, weathers, items, NPCs, …)
/// is available alongside the cell tables. Exterior loads need this
/// for the `wrld → CLMT` and `CELL → WTHR` resolution paths the
/// renderer's day-night arc consumes.
fn parse_record_indexes_in_load_order(
    plugin_paths: &[&str],
) -> anyhow::Result<(esm::records::EsmIndex, Vec<String>)> {
    let load_order: Vec<String> = plugin_paths.iter().map(|p| plugin_basename_lc(p)).collect();
    {
        let mut seen = std::collections::HashSet::with_capacity(load_order.len());
        for name in &load_order {
            if !seen.insert(name) {
                return Err(anyhow::anyhow!(
                    "Plugin '{}' appears twice in the load order — \
                     a plugin can only be passed once",
                    name
                ));
            }
        }
    }
    let mut merged = esm::records::EsmIndex::default();
    for (idx, path) in plugin_paths.iter().enumerate() {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", path, e))?;
        log::info!(
            "Parsing plugin {}/{} '{}' ({:.1} MB) at load-order index {}…",
            idx + 1,
            plugin_paths.len(),
            path,
            bytes.len() as f64 / 1_048_576.0,
            idx,
        );
        let remap = build_remap_for_plugin(path, &bytes, idx, &load_order)?;
        let plugin_records = esm::records::parse_esm_with_load_order(&bytes, Some(remap))
            .unwrap_or_else(|e| {
                log::warn!("Record parse failed for '{}': {}", path, e);
                esm::records::EsmIndex::default()
            });
        merged.merge_from(plugin_records);
    }
    Ok((merged, load_order))
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
    let (index, load_order) = parse_cell_indexes_in_load_order(&plugin_paths)?;

    // 2. Find the cell.
    let cell_key = cell_editor_id.to_ascii_lowercase();
    let cell = index.cells.get(&cell_key).ok_or_else(|| {
        // List available cells for debugging.
        let available: Vec<&str> = index
            .cells
            .values()
            .take(20)
            .map(|c| c.editor_id.as_str())
            .collect();
        anyhow::anyhow!(
            "Cell '{}' not found. {} interior cells available. Examples: {:?}",
            cell_editor_id,
            index.cells.len(),
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
        &index,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &cell.editor_id,
        &load_order,
    );

    log::info!("Cell lighting: {:?}", cell.lighting);

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
        lighting: cell.lighting.clone(),
        weather: None,
        climate: None,
        cell_root,
    })
}

/// Load a 3x3 grid of exterior cells from a worldspace, optionally
/// across a multi-plugin load order. Same semantics as
/// [`load_cell_with_masters`] — masters parsed first and merged into
/// a unified EsmIndex via FormID load-order remap. Pass `&[]` for a
/// single-plugin load. See M46.0 / #561.
pub fn load_exterior_cells_with_masters(
    masters: &[String],
    esm_path: &str,
    center_x: i32,
    center_y: i32,
    radius: i32,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    wrld_override: Option<&str>,
) -> anyhow::Result<CellLoadResult> {
    // See `load_cell` — same pattern for unload tracking (#372).
    let first_entity = world.next_entity_id();

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
    let wrld_key = {
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
    };

    let wrld_name = wrld_key.as_deref().unwrap_or("(none)");
    log::info!(
        "Loading exterior cells around ({},{}) radius {} from worldspace '{}'",
        center_x,
        center_y,
        radius,
        wrld_name,
    );

    let wrld_cells = match &wrld_key {
        Some(key) => index.exterior_cells.get(key),
        None => None,
    };

    // Collect all references from cells in the grid and spawn terrain meshes.
    let mut all_refs = Vec::new();
    let mut cells_loaded = 0u32;
    let mut terrain_entities = 0usize;
    // Accumulator for terrain BLAS specs across the whole grid — built in
    // one batched submission below instead of N one-shots (#382). On a
    // 7×7 load that's ~49 GPU submits collapsed into 1.
    let mut terrain_blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    if let Some(cells_map) = wrld_cells {
        for gx in (center_x - radius)..=(center_x + radius) {
            for gy in (center_y - radius)..=(center_y + radius) {
                if let Some(cell) = cells_map.get(&(gx, gy)) {
                    let has_land = cell.landscape.is_some();
                    log::info!(
                        "  Cell ({},{}) '{}': {} references{}",
                        gx,
                        gy,
                        cell.editor_id,
                        cell.references.len(),
                        if has_land { " + LAND" } else { "" },
                    );
                    all_refs.extend_from_slice(&cell.references);
                    // Spawn terrain mesh from LAND heightmap.
                    if let Some(ref land) = cell.landscape {
                        if let Some(count) = spawn_terrain_mesh(
                            world,
                            ctx,
                            tex_provider,
                            &index.landscape_textures,
                            gx,
                            gy,
                            land,
                            &mut terrain_blas_specs,
                        ) {
                            terrain_entities += count;
                        }
                    }
                    cells_loaded += 1;
                }
            }
        }
    }

    // Single batched terrain BLAS build for the entire grid (#382).
    // Done before `load_references` so terrain appears in the TLAS for
    // the first frame; clutter BLAS still builds per-spawn inside
    // `spawn_placed_instances` (consolidating that path is a separate
    // follow-up — see audit's "AND clutter" extension).
    if !terrain_blas_specs.is_empty() {
        let built = ctx.build_blas_batched(&terrain_blas_specs);
        log::info!(
            "Terrain BLAS batch: {built}/{} tiles (one submit, was {} submits pre-#382)",
            terrain_blas_specs.len(),
            terrain_blas_specs.len()
        );
    }

    let grid_size = (radius * 2 + 1) as u32;
    log::info!(
        "Found {}/{} cells in {}x{} grid ({} terrain meshes)",
        cells_loaded,
        grid_size * grid_size,
        grid_size,
        grid_size,
        terrain_entities,
    );

    let label = format!("exterior({},{})", center_x, center_y);
    let result = load_references(
        &all_refs,
        index,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &label,
        &load_order,
    );

    // Camera spawn: use terrain height at the center cell's midpoint
    // so the camera starts at ground level instead of inside the terrain.
    let spawn_center = if let Some(cells_map) = wrld_cells {
        if let Some(cell) = cells_map.get(&(center_x, center_y)) {
            if let Some(ref land) = cell.landscape {
                // Sample the center of the 33×33 grid (vertex 16,16).
                let mid_height = land.heights[16 * 33 + 16];
                let world_x = center_x as f32 * 4096.0 + 16.0 * 128.0;
                let world_y = center_y as f32 * 4096.0 + 16.0 * 128.0;
                // Z-up → Y-up: (x, height, -y), plus 200 units above ground.
                Vec3::new(world_x, mid_height + 200.0, -world_y)
            } else {
                result.center
            }
        } else {
            result.center
        }
    } else {
        result.center
    };

    // Resolve weather + climate: WRLD → CLMT → WTHR.
    // The climate record carries per-worldspace TNAM sunrise/sunset
    // hours so the time-of-day interpolator runs on the right clock.
    // See #463.
    let climate = wrld_key.as_deref().and_then(|wrld_name_lc| {
        let climate_fid = index.worldspace_climates.get(wrld_name_lc)?;
        let climate = record_index.climates.get(climate_fid)?.clone();
        log::info!(
            "Worldspace '{}' climate '{}' ({:08X}): {} weathers, \
             sunrise {:.2}–{:.2}h, sunset {:.2}–{:.2}h",
            wrld_name_lc,
            climate.editor_id,
            climate_fid,
            climate.weathers.len(),
            climate.sunrise_begin as f32 / 6.0,
            climate.sunrise_end as f32 / 6.0,
            climate.sunset_begin as f32 / 6.0,
            climate.sunset_end as f32 / 6.0,
        );
        Some(climate)
    });
    let weather = climate.as_ref().and_then(|climate| {
        // Pick the weather with the highest chance (most common / default).
        // Skip entries with negative chance — mods use -1 as a sentinel
        // or subtractive weight, not a valid selection score. See #476.
        let best = climate
            .weathers
            .iter()
            .filter(|w| w.chance >= 0)
            .max_by_key(|w| w.chance)?;
        let wthr = record_index.weathers.get(&best.weather_form_id)?;
        log::info!(
            "Default weather: '{}' ({:08X}, chance {})",
            wthr.editor_id,
            wthr.form_id,
            best.chance,
        );
        Some(wthr.clone())
    });

    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);

    Ok(CellLoadResult {
        cell_name: format!("{} ({},{})", wrld_name, center_x, center_y),
        entity_count: result.entity_count + terrain_entities,
        mesh_count: result.mesh_count + terrain_entities,
        center: spawn_center,
        lighting: None,
        weather,
        climate,
        cell_root,
    })
}

/// Generate a terrain mesh from LAND heightmap data and spawn it as an entity.
///
/// Each exterior cell's LAND record has a 33×33 vertex grid spanning
/// 4096×4096 Bethesda units (128-unit vertex spacing). We generate a
/// triangle mesh from this grid with proper normals and vertex colors,
/// upload it to the GPU, and attach it to a new ECS entity.
///
/// Coordinate conversion: Bethesda uses Z-up; we convert to Y-up:
///   world_x = grid_x * 4096 + col * 128  → X
///   world_z = heights[row][col]           → Y (up)
///   world_y = grid_y * 4096 + row * 128   → -Z (negate for Y-up)
/// Resolved terrain splat layers for one cell — up to 8 cell-global
/// layers, each with its resolved bindless texture handle and the
/// per-quadrant alpha grids contributed by every quadrant that painted
/// that LTEX. Produced by [`build_cell_splat_layers`] and consumed by
/// the vertex packer below. See #470.
#[derive(Default)]
struct CellSplatLayers {
    /// 0–8 entries sorted by ascending `layer_sort_key`, then by
    /// `ltex_form_id` for deterministic tiebreak.
    layers: Vec<CellSplatLayer>,
}

struct CellSplatLayer {
    /// Bindless texture handle (resolved via LTEX → TXST → diffuse path).
    /// 0 means the texture failed to load; fragment shader skips (index 0
    /// is the fallback checkerboard).
    texture_index: u32,
    /// Per-quadrant contribution. `[SW, SE, NW, NE]` — `None` means the
    /// quadrant didn't paint this LTEX. Each `Some` is a 17×17 alpha grid.
    per_quadrant_alpha: [Option<Vec<f32>>; 4],
}

/// Collect cell-global splat layers from the 4 quadrants. Dedup by
/// `ltex_form_id`; take the minimum `layer` field as the sort key so
/// seam vertices across quadrants resolve to the same cell-global
/// layer. Caps at 8 per UESP's LAND format spec; excess is dropped
/// with a warning.
fn build_cell_splat_layers(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    land: &esm::cell::LandscapeData,
) -> CellSplatLayers {
    use std::collections::hash_map::Entry;

    // Collect (ltex_form_id, min_layer_sort_key, per-quadrant alpha).
    // Use the indexmap-style approach with a HashMap for dedup + a
    // Vec for insertion order — small N, linear scan is fine.
    let mut by_ltex: HashMap<u32, (u16, [Option<Vec<f32>>; 4])> = HashMap::new();
    for (q_idx, q) in land.quadrants.iter().enumerate() {
        for l in &q.layers {
            let Some(ref alpha) = l.alpha else {
                // Malformed ATXT without VTXT — nothing to paint. #470.
                log::debug!(
                    "Terrain quadrant {}: ATXT LTEX {:08X} layer {} has no VTXT; skipped",
                    q_idx,
                    l.ltex_form_id,
                    l.layer
                );
                continue;
            };
            match by_ltex.entry(l.ltex_form_id) {
                Entry::Vacant(v) => {
                    let mut slots: [Option<Vec<f32>>; 4] = Default::default();
                    slots[q_idx] = Some(alpha.clone());
                    v.insert((l.layer, slots));
                }
                Entry::Occupied(mut o) => {
                    let (min_layer, slots) = o.get_mut();
                    if l.layer < *min_layer {
                        *min_layer = l.layer;
                    }
                    // Merge into the same quadrant slot (should be rare —
                    // one LTEX per quadrant is the vanilla pattern).
                    if let Some(existing) = slots[q_idx].as_mut() {
                        for (dst, src) in existing.iter_mut().zip(alpha.iter()) {
                            *dst = dst.max(*src);
                        }
                    } else {
                        slots[q_idx] = Some(alpha.clone());
                    }
                }
            }
        }
    }

    // Sort by (layer_sort_key, ltex_form_id) for deterministic order.
    let mut sorted: Vec<(u32, u16, [Option<Vec<f32>>; 4])> = by_ltex
        .into_iter()
        .map(|(ltex, (layer, slots))| (ltex, layer, slots))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // Cap at 8. Bethesda's own LAND authoring tool caps at 8 per
    // UESP, but modded content (TTW, Project Nevada, DLC merges) has
    // been observed going higher.
    if sorted.len() > 8 {
        log::warn!(
            "Terrain cell has {} splat layers, capping at 8 (dropping {} with highest `layer` field). #470",
            sorted.len(),
            sorted.len() - 8,
        );
        sorted.truncate(8);
    }

    // Resolve each LTEX → bindless texture handle.
    let mut layers = Vec::with_capacity(sorted.len());
    for (ltex, _layer_key, per_quadrant_alpha) in sorted {
        let texture_index = if let Some(tex_path) = landscape_textures.get(&ltex) {
            resolve_texture(ctx, tex_provider, Some(tex_path.as_str()))
        } else {
            log::debug!(
                "Terrain splat: LTEX {:08X} not in landscape_textures map; skipping layer",
                ltex
            );
            0
        };
        layers.push(CellSplatLayer {
            texture_index,
            per_quadrant_alpha,
        });
    }

    CellSplatLayers { layers }
}

/// Map a global 33×33 `(row, col)` to the list of contributing
/// `(quadrant_index, local_row_in_17, local_col_in_17)` tuples. Most
/// vertices belong to exactly one quadrant; edges belong to two,
/// corners to four.
fn quadrant_samples_for_vertex(row: usize, col: usize) -> [(u8, u8, u8); 4] {
    // Sentinel: 0xFF means "unused slot" — the caller checks `q < 4`
    // to decide whether to sample. Using u8 keeps the return POD.
    let mut out = [(0xFFu8, 0u8, 0u8); 4];
    let mut n = 0;
    // SW quadrant (0): rows [0..=16], cols [0..=16].
    if row <= 16 && col <= 16 {
        out[n] = (0, row as u8, col as u8);
        n += 1;
    }
    // SE quadrant (1): rows [0..=16], cols [16..=32]. Local col = col-16.
    if row <= 16 && col >= 16 {
        out[n] = (1, row as u8, (col - 16) as u8);
        n += 1;
    }
    // NW quadrant (2): rows [16..=32], cols [0..=16]. Local row = row-16.
    if row >= 16 && col <= 16 {
        out[n] = (2, (row - 16) as u8, col as u8);
        n += 1;
    }
    // NE quadrant (3): rows [16..=32], cols [16..=32].
    if row >= 16 && col >= 16 {
        out[n] = (3, (row - 16) as u8, (col - 16) as u8);
        n += 1;
    }
    let _ = n;
    out
}

/// Sample one splat weight for a global vertex by taking the max
/// across every contributing quadrant's alpha grid. Absent quadrants
/// contribute 0. Returns a u8 ready to pack into the vertex attribute.
fn splat_weight_for_vertex(layer: &CellSplatLayer, row: usize, col: usize) -> u8 {
    let samples = quadrant_samples_for_vertex(row, col);
    let mut best = 0.0_f32;
    for (q, lr, lc) in samples {
        if q >= 4 {
            continue;
        }
        let Some(ref alpha) = layer.per_quadrant_alpha[q as usize] else {
            continue;
        };
        let local_idx = (lr as usize) * 17 + (lc as usize);
        if local_idx < alpha.len() {
            best = best.max(alpha[local_idx]);
        }
    }
    (best.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn spawn_terrain_mesh(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    grid_x: i32,
    grid_y: i32,
    land: &esm::cell::LandscapeData,
    blas_specs: &mut Vec<(u32, u32, u32)>,
) -> Option<usize> {
    use byroredux_renderer::Vertex;

    const CELL_SIZE: f32 = 4096.0;
    const GRID: usize = 33;
    const SPACING: f32 = CELL_SIZE / 32.0; // 128.0

    let origin_x = grid_x as f32 * CELL_SIZE;
    let origin_y = grid_y as f32 * CELL_SIZE;

    // Collect cell-global splat layers before the vertex loop — we
    // need all 8 resolved before we can pack per-vertex weights. #470.
    let splat_layers = build_cell_splat_layers(ctx, tex_provider, landscape_textures, land);

    // Build vertices (33×33 = 1089).
    let mut vertices = Vec::with_capacity(GRID * GRID);
    for row in 0..GRID {
        for col in 0..GRID {
            let idx = row * GRID + col;

            // World-space position (Z-up → Y-up conversion).
            let bx = origin_x + col as f32 * SPACING;
            let by = origin_y + row as f32 * SPACING;
            let bz = land.heights[idx];
            let position = [bx, bz, -by]; // X stays, Z→Y(up), -Y→Z

            // Normal (from VNML bytes or default up).
            let normal = if let Some(ref nml) = land.normals {
                let ni = idx * 3;
                // VNML bytes are unsigned 0–255, center at 128 = zero.
                // Bethesda Z-up: X, Y, Z → convert to Y-up: X, Z, -Y.
                let nx = (nml[ni] as f32 - 128.0) / 127.0;
                let ny = (nml[ni + 1] as f32 - 128.0) / 127.0;
                let nz = (nml[ni + 2] as f32 - 128.0) / 127.0;
                // Z-up to Y-up: (nx, nz, -ny)
                let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
                [nx / len, nz / len, -ny / len]
            } else {
                [0.0, 1.0, 0.0] // default up
            };

            // Vertex color (from VCLR bytes or default white).
            let color = if let Some(ref vc) = land.vertex_colors {
                let ci = idx * 3;
                [
                    vc[ci] as f32 / 255.0,
                    vc[ci + 1] as f32 / 255.0,
                    vc[ci + 2] as f32 / 255.0,
                ]
            } else {
                [1.0, 1.0, 1.0]
            };

            // UV: tile across the cell (0–1 per cell, repeats for texture).
            let uv = [col as f32 / 32.0, 1.0 - row as f32 / 32.0];

            // Pack up to 8 splat weights into 2× RGBA8 unorm (#470).
            // Layers beyond what the cell actually contains get 0.
            let mut splat0 = [0u8; 4];
            let mut splat1 = [0u8; 4];
            for (i, layer) in splat_layers.layers.iter().enumerate() {
                let w = splat_weight_for_vertex(layer, row, col);
                if i < 4 {
                    splat0[i] = w;
                } else {
                    splat1[i - 4] = w;
                }
            }

            vertices.push(Vertex::new_terrain(
                position, color, normal, uv, splat0, splat1,
            ));
        }
    }

    // Build indices (32×32 quads × 2 triangles = 2048 triangles).
    let mut indices = Vec::with_capacity(32 * 32 * 6);
    for row in 0..32u32 {
        for col in 0..32u32 {
            let tl = row * GRID as u32 + col;
            let tr = tl + 1;
            let bl = (row + 1) * GRID as u32 + col;
            let br = bl + 1;
            // Two triangles per quad. The Z-up → Y-up transform negates
            // the Z axis, flipping winding. Use CW here so it becomes CCW
            // (Vulkan front face) after the coordinate conversion.
            indices.push(tl);
            indices.push(tr);
            indices.push(bl);
            indices.push(tr);
            indices.push(br);
            indices.push(bl);
        }
    }

    // Upload to GPU via upload_scene_mesh so the terrain participates in
    // the global geometry SSBO that RT reflection/GI rays sample from.
    // Plain upload() leaves global_vertex_offset/global_index_offset at 0,
    // which would make RT rays hitting terrain read whichever clutter mesh
    // landed at SSBO offset 0. See #371.
    let allocator = ctx.allocator.as_ref()?;
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &vertices,
        &indices,
        ctx.device_caps.ray_query_supported,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!(
                "Failed to upload terrain mesh ({},{}): {}",
                grid_x,
                grid_y,
                e
            );
            return None;
        }
    };

    // Resolve terrain base texture: pick the first available BTXT from
    // any quadrant, resolve via LTEX → texture path, load from BSA.
    // Per-quadrant BTXT disagreement is handled best-effort: we pick
    // the first non-zero base and rely on the ATXT splat layers above
    // to paint the other quadrants' bases as additional layers (the
    // quadrants that share the chosen BTXT get it as their floor; the
    // rest see ATXT layers on top). See #470 (D7 follow-up).
    let tex_handle = {
        let base_ltex = land.quadrants.iter().find_map(|q| q.base);
        if let Some(ltex_id) = base_ltex {
            if ltex_id == 0 {
                // BTXT with form ID 0 = "default dirt" per UESP.
                // Try the engine's built-in fallback dirt texture.
                resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
            } else if let Some(tex_path) = landscape_textures.get(&ltex_id) {
                resolve_texture(ctx, tex_provider, Some(tex_path.as_str()))
            } else {
                log::debug!(
                    "Terrain ({},{}): LTEX {:08X} not in landscape_textures map",
                    grid_x,
                    grid_y,
                    ltex_id,
                );
                0 // fallback
            }
        } else {
            // No BTXT at all — try default dirt.
            resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
        }
    };

    // Allocate a terrain tile slot and upload its 8-layer texture
    // indices. The slot is freed in `unload_cell` via
    // `VulkanContext::free_terrain_tile_slot`. Only enabled when the
    // cell actually has splat layers; BTXT-only cells skip this and
    // render with the pre-#470 single-texture path for free. See #470.
    let terrain_tile_index = if !splat_layers.layers.is_empty() {
        let mut indices_arr = [0u32; 8];
        for (i, layer) in splat_layers.layers.iter().enumerate() {
            indices_arr[i] = layer.texture_index;
        }
        ctx.allocate_terrain_tile(indices_arr)
    } else {
        None
    };

    // Queue BLAS build into the caller's batched-spec list rather than
    // submitting one-shot per tile (#382). Terrain must be in the TLAS
    // for RT shadows/GI; the actual `build_blas_batched` call happens
    // after every tile in the grid is uploaded, collapsing what used to
    // be N separate submits + barriers (one per cell) into one.
    if ctx.device_caps.ray_query_supported {
        blas_specs.push((mesh_handle, vertices.len() as u32, indices.len() as u32));
    }

    // Spawn ECS entity at origin (vertices are already in world-space).
    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    if let Some(slot) = terrain_tile_index {
        world.insert(entity, TerrainTileSlot(slot));
    }

    log::debug!(
        "Terrain mesh ({},{}): {} verts, {} tris, height range {:.0}–{:.0}",
        grid_x,
        grid_y,
        vertices.len(),
        indices.len() / 3,
        land.heights.iter().cloned().fold(f32::INFINITY, f32::min),
        land.heights
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max),
    );

    Some(1)
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
    let mut this_call_parsed: u64 = 0;
    let mut this_call_failed: u64 = 0;
    // Parses performed during this call. Merged into the registry at
    // end-of-function. `pending_new.get` shadows the registry read so
    // subsequent iterations of the loop see this call's own parses
    // without re-entering the registry.
    let mut pending_new: HashMap<String, Option<Arc<CachedNifImport>>> = HashMap::new();

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
        let refr_overlay =
            build_refr_texture_overlay(placed_ref, index, mat_provider.as_deref_mut());

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
                            radius: ld.radius,
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
                            radius: ld.radius,
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
                    reg.cache.get(&cache_key).cloned()
                };
                match reg_entry {
                    Some(entry) => {
                        this_call_hits += 1;
                        entry
                    }
                    None => {
                        // Slow-path: parse outside any registry borrow.
                        let parsed = match tex_provider.extract_mesh(&model_path) {
                            Some(d) => {
                                parse_and_import_nif(&d, &model_path, mat_provider.as_deref_mut())
                            }
                            None => {
                                log::debug!("NIF not found in BSA: '{}'", model_path);
                                None
                            }
                        };
                        this_call_misses += 1;
                        if parsed.is_some() {
                            this_call_parsed += 1;
                        } else {
                            this_call_failed += 1;
                        }
                        pending_new.insert(cache_key, parsed.clone());
                        parsed
                    }
                }
            };
            let Some(cached) = cached else { continue };

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
            );
            entity_count += count;
            mesh_entity_count += count;
        }
    }

    let center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
    // Commit the accumulated counters + pending entries in a single
    // write lock. Stats snapshot happens in the same scope so the log
    // line below reflects post-commit numbers. See #523.
    let (this_cell_hits, this_cell_misses, this_cell_unique, lifetime_hit_rate) = {
        let mut reg = world.resource_mut::<NifImportRegistry>();
        reg.hits += this_call_hits;
        reg.misses += this_call_misses;
        reg.parsed_count += this_call_parsed;
        reg.failed_count += this_call_failed;
        for (key, entry) in pending_new {
            reg.cache.insert(key, entry);
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

    let (mut meshes, collisions) = byroredux_nif::import::import_nif_with_collision(&scene);
    // FO4+ external material resolution (#493). Walk once at cache-fill
    // time so every REFR sharing this NIF sees the merged texture paths.
    // NIF fields take precedence; only empty slots are filled from the
    // resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut meshes {
            merge_bgsm_into_mesh(mesh, provider);
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

/// Spawn entities for every mesh/light/collision in a pre-parsed NIF
/// with a parent REFR transform applied. Each NIF sub-mesh has its
/// own local transform (from the scene graph) which is composed on
/// top of the REFR placement transform. The `cached` parameter is
/// produced by `parse_and_import_nif` and shared across all
/// placements of the same model via `Arc`.
/// Per-REFR texture overlay computed from the REFR's XATO / XTNM / XTXR
/// sub-records (#584 / FO4-DIM6-02).
///
/// A populated overlay means the REFR overrides one or more texture
/// slots of its base mesh. Each `Option<String>` is `Some` when the
/// REFR provides a replacement texture for that slot; `None` means the
/// base NIF's slot stands. The precedence is:
///   1. XATO full-TXST overlay (and XTNM for LAND-scoped refs) fills
///      whatever the referenced `TextureSet` carries.
///   2. XTXR per-slot swaps override specific slots afterwards (later
///      XTXR entries win for the same slot).
///   3. If the overlay picked up a `material_path` (MNAM-only TXSTs),
///      the BGSM chain fills any still-empty slot.
///
/// The overlay's diffuse/normal/glow/height/env/env_mask/specular/
/// material_path are applied at spawn time inside
/// `spawn_placed_instances`, shadowing the cached `ImportedMesh` reads.
/// The original `ImportedMesh` is never mutated — overlay is a per-REFR
/// shadow that respects the process-lifetime NIF import cache.
///
/// Pre-#584 37 % of vanilla Fallout4.esm TXSTs (140 / 382, MNAM-only)
/// parsed cleanly into `EsmCellIndex.texture_sets` with nowhere to go.
#[derive(Debug, Default, Clone)]
pub(crate) struct RefrTextureOverlay {
    pub(crate) diffuse: Option<String>,
    pub(crate) normal: Option<String>,
    pub(crate) glow: Option<String>,
    pub(crate) height: Option<String>,
    pub(crate) env: Option<String>,
    pub(crate) env_mask: Option<String>,
    /// BSShaderTextureSet slot 6 — MultiLayerParallax inner layer.
    /// Not yet consumed by the spawn path; preserved for parity with
    /// `TextureSet.inner` so the slot_index=6 XTXR swap round-trips.
    #[allow(dead_code)]
    pub(crate) inner: Option<String>,
    pub(crate) specular: Option<String>,
    pub(crate) material_path: Option<String>,
}

impl RefrTextureOverlay {
    /// First-non-empty-wins fill for an overlay slot.
    fn fill(slot: &mut Option<String>, value: Option<&str>) {
        if slot.is_none() {
            if let Some(v) = value {
                if !v.is_empty() {
                    *slot = Some(v.to_string());
                }
            }
        }
    }

    fn merge_from_texture_set(&mut self, ts: &esm::cell::TextureSet) {
        Self::fill(&mut self.diffuse, ts.diffuse.as_deref());
        Self::fill(&mut self.normal, ts.normal.as_deref());
        Self::fill(&mut self.glow, ts.glow.as_deref());
        Self::fill(&mut self.height, ts.height.as_deref());
        Self::fill(&mut self.env, ts.env.as_deref());
        Self::fill(&mut self.env_mask, ts.env_mask.as_deref());
        Self::fill(&mut self.inner, ts.inner.as_deref());
        Self::fill(&mut self.specular, ts.specular.as_deref());
        Self::fill(&mut self.material_path, ts.material_path.as_deref());
    }

    /// Apply a single XTXR slot swap. `slot_index` picks one of TX00..TX07
    /// on the host mesh; the source path comes from the swap TXST's
    /// same-index slot. Later XTXR for the same slot overwrites.
    fn apply_slot_swap(&mut self, ts: &esm::cell::TextureSet, slot_index: u32) {
        let src = match slot_index {
            0 => ts.diffuse.as_deref(),
            1 => ts.normal.as_deref(),
            2 => ts.glow.as_deref(),
            3 => ts.height.as_deref(),
            4 => ts.env.as_deref(),
            5 => ts.env_mask.as_deref(),
            6 => ts.inner.as_deref(),
            7 => ts.specular.as_deref(),
            _ => return, // Out-of-range — drop silently.
        };
        let Some(path) = src else { return };
        if path.is_empty() {
            return;
        }
        let dest = match slot_index {
            0 => &mut self.diffuse,
            1 => &mut self.normal,
            2 => &mut self.glow,
            3 => &mut self.height,
            4 => &mut self.env,
            5 => &mut self.env_mask,
            6 => &mut self.inner,
            7 => &mut self.specular,
            _ => return,
        };
        *dest = Some(path.to_string());
    }

    /// Walk the overlay's `material_path` BGSM/BGEM chain and fill any
    /// still-empty texture slot. Matches `merge_bgsm_into_mesh`'s
    /// first-wins policy so REFR overlays and per-mesh imports agree on
    /// precedence for MNAM-only TXSTs. No-op when the path isn't a
    /// `.bgsm` / `.bgem` or the provider can't resolve it.
    fn fill_from_bgsm(&mut self, provider: &mut MaterialProvider) {
        let Some(path) = self.material_path.clone() else {
            return;
        };
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".bgsm") {
            let Some(resolved) = provider.resolve_bgsm(&path) else {
                return;
            };
            for step in resolved.walk() {
                let f = &step.file;
                Self::fill(&mut self.diffuse, Some(f.diffuse_texture.as_str()));
                Self::fill(&mut self.normal, Some(f.normal_texture.as_str()));
                Self::fill(&mut self.glow, Some(f.glow_texture.as_str()));
                Self::fill(&mut self.specular, Some(f.smooth_spec_texture.as_str()));
                Self::fill(&mut self.env, Some(f.envmap_texture.as_str()));
                Self::fill(&mut self.height, Some(f.displacement_texture.as_str()));
            }
        } else if lower.ends_with(".bgem") {
            let Some(bgem) = provider.resolve_bgem(&path) else {
                return;
            };
            Self::fill(&mut self.normal, Some(bgem.normal_texture.as_str()));
            Self::fill(&mut self.glow, Some(bgem.glow_texture.as_str()));
            Self::fill(&mut self.env, Some(bgem.envmap_texture.as_str()));
        }
    }
}

/// Expand a PKIN (Pack-In) REFR into synthetic children.
///
/// PKIN records (FO4+) bundle LVLI / CONT / STAT / MSTT / FURN
/// references behind a single form ID so a level designer can drop a
/// reusable "generic workbench with loot" as one REFR. The parser
/// captures every `CNAM` sub-record into `PkinRecord::contents` at
/// ESM-load time; this helper fans the REFR out into one synthetic
/// placement per content entry — all at the SAME outer transform
/// (PKIN carries no per-child placement data, unlike SCOL).
///
/// Returns `None` when the outer REFR's base isn't a PKIN (so the
/// caller can fall through to the SCOL / default-single-entry paths),
/// or when the PKIN's `contents` list is empty (malformed or
/// author-trimmed record).
///
/// Pre-#589 all 872 vanilla Fallout4.esm PKIN records silently
/// produced no world content because the MODL-only parser discarded
/// the CNAM list. See audit FO4-DIM4-03.
pub(crate) fn expand_pkin_placements(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
) -> Option<Vec<(u32, Vec3, Quat, f32)>> {
    let pkin = index.packins.get(&base_form_id)?;
    if pkin.contents.is_empty() {
        return None;
    }
    // Every child spawns at the outer REFR's world transform — PKIN
    // has no per-child offset. The cell_loader's inner spawn body
    // then resolves each `child_form_id` through `statics` exactly
    // as for a normal REFR.
    let out = pkin
        .contents
        .iter()
        .map(|&child_form_id| (child_form_id, outer_pos, outer_rot, outer_scale))
        .collect();
    Some(out)
}

/// Produce the list of `(base_form_id, composed_pos, composed_rot,
/// composed_scale)` placements to spawn for one REFR.
///
/// Normal (non-SCOL) REFR: returns a single-entry vec carrying the
/// outer REFR's own base form ID and world-space transform. The hot
/// path for interior cells (~99 % of REFRs).
///
/// SCOL REFR with no cached `CM*.NIF`: flattens `ScolRecord.parts` into
/// synthetic children. Each `ScolPlacement` (Z-up Euler-radian local
/// transform, per `records/scol.rs`) composes with the outer REFR's
/// world-space transform:
///
/// ```text
/// final_pos    = outer_rot * (outer_scale * local_pos) + outer_pos
/// final_rot    = outer_rot * local_rot
/// final_scale  = outer_scale * local_scale
/// ```
///
/// Vanilla FO4 ships 2616 / 2617 SCOLs with a cached `CM*.NIF` in
/// `statics[base].model_path`, so the normal path runs for those.
/// Mod-added SCOLs (and vanilla SCOLs whose CM file is absent under a
/// previsibine-bypass loadout) hit the expansion branch. Single-level
/// expansion only: if a synthetic child is itself a SCOL we don't
/// recurse — vanilla FO4 has no SCOL-of-SCOL nesting. See #585.
pub(crate) fn expand_scol_placements(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
) -> Vec<(u32, Vec3, Quat, f32)> {
    // Expand only when the outer REFR's base is a SCOL with no valid
    // cached model. `statics.get(base).model_path` empty — or the base
    // isn't in `statics` at all (mod-added SCOL without EDID/MODL) —
    // plus the base form IS in `scols`.
    let must_expand = index.scols.contains_key(&base_form_id)
        && index
            .statics
            .get(&base_form_id)
            .map_or(true, |s| s.model_path.is_empty());
    if !must_expand {
        return vec![(base_form_id, outer_pos, outer_rot, outer_scale)];
    }

    let Some(scol) = index.scols.get(&base_form_id) else {
        // Defensive: if contains_key passed but get returned None
        // (shouldn't happen outside of concurrent mutation), fall back
        // to the non-expanded single-entry path so the REFR at least
        // gets logged as a stats miss rather than silently dropped.
        return vec![(base_form_id, outer_pos, outer_rot, outer_scale)];
    };

    let mut out = Vec::new();
    for part in &scol.parts {
        for p in &part.placements {
            // Z-up Bethesda → Y-up renderer, matching the outer REFR
            // conversion policy in `load_references`.
            let local_pos = Vec3::new(p.pos[0], p.pos[2], -p.pos[1]);
            let local_rot = euler_zup_to_quat_yup(p.rot[0], p.rot[1], p.rot[2]);
            let final_pos = outer_rot * (outer_scale * local_pos) + outer_pos;
            let final_rot = outer_rot * local_rot;
            let final_scale = outer_scale * p.scale;
            out.push((part.base_form_id, final_pos, final_rot, final_scale));
        }
    }
    out
}

/// Build a texture overlay for a REFR when its parser-side override
/// sub-records (XATO, XTNM, XTXR) carry actionable TXST FormIDs.
/// Returns `None` when the REFR has no overrides — the hot path for
/// interior cells where > 99 % of REFRs use their base mesh's textures
/// verbatim.
pub(crate) fn build_refr_texture_overlay(
    placed: &esm::cell::PlacedRef,
    index: &esm::cell::EsmCellIndex,
    mat_provider: Option<&mut MaterialProvider>,
) -> Option<RefrTextureOverlay> {
    if placed.alt_texture_ref.is_none()
        && placed.land_texture_ref.is_none()
        && placed.texture_slot_swaps.is_empty()
    {
        return None;
    }

    let mut ov = RefrTextureOverlay::default();

    // XATO — mesh-scoped TXST override. Resolves the whole TextureSet
    // onto the overlay's 8 slots + material_path.
    if let Some(txst_ref) = placed.alt_texture_ref {
        if let Some(ts) = index.texture_sets.get(&txst_ref) {
            ov.merge_from_texture_set(ts);
        }
    }
    // XTNM — LAND-scoped TXST override. Same wire layout as XATO; fills
    // whatever slots XATO didn't cover. Typical REFRs carry only one
    // of the two, so the "first non-empty wins" policy just picks up
    // whichever is present.
    if let Some(txst_ref) = placed.land_texture_ref {
        if let Some(ts) = index.texture_sets.get(&txst_ref) {
            ov.merge_from_texture_set(ts);
        }
    }

    // XTXR — per-slot swaps, applied after the full-TXST overlay so
    // individual slot swaps can override what XATO/XTNM set. Later
    // XTXR for the same slot wins (authoring-order semantics).
    for swap in &placed.texture_slot_swaps {
        if let Some(ts) = index.texture_sets.get(&swap.texture_set) {
            ov.apply_slot_swap(ts, swap.slot_index);
        }
    }

    // BGSM chain fill — MNAM-only TXSTs contribute nothing to the 8
    // direct slots, but their `material_path` resolves through the
    // BGSM template chain to real textures. Matches import-time
    // `merge_bgsm_into_mesh` semantics.
    if ov.material_path.is_some() {
        if let Some(mp) = mat_provider {
            ov.fill_from_bgsm(mp);
        }
    }

    Some(ov)
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
) -> usize {
    use byroredux_renderer::Vertex;

    let imported = &cached.meshes;
    let collisions = &cached.collisions;
    let nif_lights = &cached.lights;
    let mut count = 0;
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
        let radius = if let Some(r) = esm_radius {
            // ESM radius is authored by the level designer — ground truth.
            r * ref_scale
        } else if light.radius > 0.0 {
            light.radius * ref_scale
        } else {
            // Ambient / directional lights have no meaningful placement radius;
            // fall back to a large cell-scale default.
            4096.0
        };
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
        let ov = refr_overlay;
        let eff_texture_path = ov
            .and_then(|o| o.diffuse.clone())
            .or_else(|| mesh.texture_path.clone());
        let eff_normal_map = ov
            .and_then(|o| o.normal.clone())
            .or_else(|| mesh.normal_map.clone());
        let eff_glow_map = ov
            .and_then(|o| o.glow.clone())
            .or_else(|| mesh.glow_map.clone());
        let eff_gloss_map = ov
            .and_then(|o| o.specular.clone())
            .or_else(|| mesh.gloss_map.clone());
        let eff_parallax_map = ov
            .and_then(|o| o.height.clone())
            .or_else(|| mesh.parallax_map.clone());
        let eff_env_map = ov
            .and_then(|o| o.env.clone())
            .or_else(|| mesh.env_map.clone());
        let eff_env_mask = ov
            .and_then(|o| o.env_mask.clone())
            .or_else(|| mesh.env_mask.clone());
        let eff_material_path = ov
            .and_then(|o| o.material_path.clone())
            .or_else(|| mesh.material_path.clone());

        // Load texture (shared resolve: cache → BSA → fallback).
        let tex_handle = resolve_texture(ctx, tex_provider, eff_texture_path.as_deref());

        // Compose: REFR parent transform * NIF local transform.
        // mesh.rotation is already a Y-up quaternion [x,y,z,w] extracted via SVD.
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

        // Composed: parent_rot * (parent_scale * child_pos) + parent_pos
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
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(
            entity,
            GlobalTransform::new(final_pos, final_rot, final_scale),
        );
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
                detail_map: mesh.detail_map.clone(),
                gloss_map: eff_gloss_map.clone(),
                dark_map: mesh.dark_map.clone(),
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
        if let Some(ref dark_path) = mesh.dark_map {
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
        let detail_h = resolve(&mesh.detail_map);
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
                        radius: ld.radius,
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
