//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    CellRoot, GlobalTransform, LightSource, Material, MeshHandle, TextureHandle, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::{AlphaBlend, DarkMapHandle, Decal, NormalMapHandle, TwoSided};

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
}

/// Result of loading a cell.
#[allow(dead_code)]
pub struct CellLoadResult {
    pub cell_name: String,
    pub entity_count: usize,
    pub mesh_count: usize,
    /// Bounding box center of all placed objects (Y-up, for camera positioning).
    pub center: Vec3,
    /// Interior cell lighting (ambient + directional).
    pub lighting: Option<byroredux_plugin::esm::cell::CellLighting>,
    /// Weather data for exterior cells (from WRLD→CLMT→WTHR chain).
    pub weather: Option<byroredux_plugin::esm::records::WeatherRecord>,
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
/// Textures are freed best-effort: if another still-live cell shares a
/// texture through the path cache, its bindless slot falls back to the
/// checkerboard until the next load re-creates a fresh entry. Callers
/// that need concurrent multi-cell residency should ref-count texture
/// usage themselves (out of scope for #372).
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

    // Gather every unique mesh and texture handle referenced by the
    // victim set. Using `HashSet` deduplicates shared NIF clutter and
    // path-cached textures so we don't call `drop_*` twice on the same
    // handle.
    let mut mesh_handles: HashSet<u32> = HashSet::new();
    let mut texture_handles: HashSet<u32> = HashSet::new();
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
                // Fallback texture (handle 0) is process-wide; never drop it.
                let fallback = ctx.texture_registry.fallback();
                if th.0 != fallback {
                    texture_handles.insert(th.0);
                }
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
    }
    for &mh in &mesh_handles {
        ctx.mesh_registry.drop_mesh(mh);
    }
    for &th in &texture_handles {
        ctx.texture_registry.drop_texture(&ctx.device, th);
    }

    // Remove every surviving component row for the victim entities.
    let victim_count = victims.len();
    for eid in victims {
        world.despawn(eid);
    }

    log::info!(
        "Cell unload: {} entities, {} meshes, {} textures freed (cell_root {})",
        victim_count,
        mesh_handles.len(),
        texture_handles.len(),
        cell_root,
    );
}

/// Load an interior cell by editor ID.
///
/// Parses the ESM, finds the cell, loads all placed static objects from the BSA.
pub fn load_cell(
    esm_path: &str,
    cell_editor_id: &str,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
) -> anyhow::Result<CellLoadResult> {
    // Mark the high-water entity id before loading. Everything spawned
    // by this load (including the designated cell_root at the end) gets
    // CellRoot stamped on it for later unload. See #372.
    let first_entity = world.next_entity_id();

    // 1. Parse the ESM.
    let esm_data = std::fs::read(esm_path)
        .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", esm_path, e))?;

    log::info!(
        "Parsing ESM '{}' ({:.1} MB)...",
        esm_path,
        esm_data.len() as f64 / 1_048_576.0
    );
    let index = esm::cell::parse_esm_cells(&esm_data)?;

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
        &cell.editor_id,
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
        cell_root,
    })
}

/// Load a 3x3 grid of exterior cells from a worldspace.
pub fn load_exterior_cells(
    esm_path: &str,
    center_x: i32,
    center_y: i32,
    radius: i32,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
) -> anyhow::Result<CellLoadResult> {
    // See `load_cell` — same pattern for unload tracking (#372).
    let first_entity = world.next_entity_id();

    let esm_data = std::fs::read(esm_path)
        .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", esm_path, e))?;

    log::info!(
        "Parsing ESM '{}' ({:.1} MB)...",
        esm_path,
        esm_data.len() as f64 / 1_048_576.0
    );
    // Single combined parse: `parse_esm` already calls `parse_esm_cells`
    // internally for its `cells` field, so calling them separately ran a
    // second full walk over the (potentially 500 MB) ESM buffer for no
    // gain. Pre-#374 this triggered three "ESM parsed: ..." log lines
    // per exterior load (1 from the standalone cell parse + 2 from the
    // record parse's internal cell parse + record-walk pass) and added
    // ~1.2 s of avoidable load stall on FNV.
    let record_index = esm::records::parse_esm(&esm_data).unwrap_or_else(|e| {
        log::warn!("Record parse failed: {e}");
        esm::records::EsmIndex::default()
    });
    let index = &record_index.cells;

    // Find the best worldspace. Try common FNV names, then fall back to largest.
    let wrld_key = {
        let preferred = ["wastelandnv", "tamriel", "skyrim"];
        preferred
            .iter()
            .find(|&&name| index.exterior_cells.contains_key(name))
            .map(|s| s.to_string())
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
    let result = load_references(&all_refs, index, world, ctx, tex_provider, &label);

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

    // Resolve weather: WRLD → CLMT → WTHR (first pleasant or highest-chance weather).
    let weather = wrld_key.as_deref().and_then(|wrld_name_lc| {
        let climate_fid = index.worldspace_climates.get(wrld_name_lc)?;
        let climate = record_index.climates.get(climate_fid)?;
        log::info!(
            "Worldspace '{}' climate '{}' ({:08X}): {} weathers",
            wrld_name_lc,
            climate.editor_id,
            climate_fid,
            climate.weathers.len(),
        );
        // Pick the weather with the highest chance (most common / default).
        let best = climate.weathers.iter().max_by_key(|w| w.chance)?;
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

            vertices.push(Vertex::new(position, color, normal, uv));
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
    // Full per-quadrant multi-layer splatting is deferred — for now the
    // entire cell gets one texture (the base layer of the first quadrant).
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
fn load_references(
    refs: &[esm::cell::PlacedRef],
    index: &esm::cell::EsmCellIndex,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    label: &str,
) -> RefLoadResult {
    let mut entity_count = 0;
    // Cache parsed-and-imported NIF scene data keyed by model path.
    // Each unique mesh is parsed exactly once per cell load; subsequent
    // placements of the same model reuse the shared `Arc` and only pay
    // the per-reference spawn cost (vertex upload, texture resolve,
    // entity insertion). Previously we cached only the raw bytes and
    // re-parsed on every placement, producing N× the parser warning
    // spam and N× the parse CPU cost for a cell with N placements.
    // A `None` entry records a mesh that failed to parse — we skip
    // subsequent placements of the same model silently.
    let mut import_cache: HashMap<String, Option<Arc<CachedNifImport>>> = HashMap::new();
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    let mut stat_miss = 0u32;
    let mut stat_hit = 0u32;
    for placed_ref in refs {
        let stat = match index.statics.get(&placed_ref.base_form_id) {
            Some(s) => {
                stat_hit += 1;
                s
            }
            None => {
                stat_miss += 1;
                log::debug!(
                    "REFR base {:08X} not in statics table",
                    placed_ref.base_form_id
                );
                continue;
            }
        };

        // Convert REFR placement: Z-up → Y-up.
        let ref_pos = Vec3::new(
            placed_ref.position[0],
            placed_ref.position[2],
            -placed_ref.position[1],
        );
        let ref_rot = euler_zup_to_quat_yup(
            placed_ref.rotation[0],
            placed_ref.rotation[1],
            placed_ref.rotation[2],
        );
        let ref_scale = placed_ref.scale;

        // Update bounds.
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

        // Skip non-renderable meshes: editor markers, effect sprites, fog.
        // Still spawn the ESM light entity if this LIGH record carries one —
        // the effect mesh is visual-only but the point light is real.
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

        // Fetch parsed+imported NIF from cache, or load+parse once.
        let cached = match import_cache.get(&model_path) {
            Some(entry) => entry.clone(),
            None => {
                let nif_data = match tex_provider.extract_mesh(&model_path) {
                    Some(d) => d,
                    None => {
                        log::debug!("NIF not found in BSA: '{}'", model_path);
                        import_cache.insert(model_path.clone(), None);
                        continue;
                    }
                };
                let parsed = parse_and_import_nif(&nif_data, &model_path);
                import_cache.insert(model_path.clone(), parsed.clone());
                parsed
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
        );
        entity_count += count;
    }

    let center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
    log::info!(
        "'{}' loaded: {} entities, {} unique meshes, {} hits, {} misses",
        label,
        entity_count,
        import_cache.len(),
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
        log::warn!(
            "  {} base forms not found in statics table — run with RUST_LOG=debug for details",
            stat_miss,
        );
    }

    RefLoadResult {
        entity_count,
        // Count only successfully-parsed entries.
        mesh_count: import_cache.values().filter(|e| e.is_some()).count(),
        center,
    }
}

/// Parse + import a NIF scene once. Returns `None` on parse failure
/// or when the scene has zero useful geometry. All per-block parse
/// warnings and the truncation message (if any) are emitted exactly
/// once per unique NIF at this step — subsequent placements read
/// from the cache without re-parsing. See runtime-spam incident from
/// the `AnvilHeinrichOakenHallsHouse` trace.
fn parse_and_import_nif(nif_data: &[u8], label: &str) -> Option<Arc<CachedNifImport>> {
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

    let (meshes, collisions) = byroredux_nif::import::import_nif_with_collision(&scene);
    let lights = byroredux_nif::import::import_nif_lights(&scene);
    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
    }))
}

/// Spawn entities for every mesh/light/collision in a pre-parsed NIF
/// with a parent REFR transform applied. Each NIF sub-mesh has its
/// own local transform (from the scene graph) which is composed on
/// top of the REFR placement transform. The `cached` parameter is
/// produced by `parse_and_import_nif` and shared across all
/// placements of the same model via `Arc`.
fn spawn_placed_instances(
    world: &mut World,
    ctx: &mut VulkanContext,
    cached: &CachedNifImport,
    tex_provider: &TextureProvider,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    light_data: Option<&esm::cell::LightData>,
) -> usize {
    use byroredux_renderer::Vertex;

    let imported = &cached.meshes;
    let collisions = &cached.collisions;
    let nif_lights = &cached.lights;
    let mut count = 0;

    // Spawn per-mesh NiLight blocks as LightSource entities. Parented
    // through the reference transform so torches/candles inside cell
    // refs contribute to the live GpuLight buffer. See issue #156.
    // When the ESM LIGH record provides an authored radius, prefer it
    // over the NIF-computed attenuation_radius (which often returns 2048
    // for NiPointLights with constant-only attenuation coefficients).
    let esm_radius = light_data.as_ref().map(|ld| ld.radius);

    for light in nif_lights {
        // Skip lights whose diffuse contribution is effectively zero —
        // these are usually authored-off placeholders.
        if light.color[0] + light.color[1] + light.color[2] < 1e-4 {
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
                Vertex::new(
                    mesh.positions[i],
                    if i < mesh.colors.len() {
                        mesh.colors[i]
                    } else {
                        [1.0, 1.0, 1.0]
                    },
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

        // Load texture (shared resolve: cache → BSA → fallback).
        let tex_handle = resolve_texture(ctx, tex_provider, mesh.texture_path.as_deref());

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
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: mesh.normal_map.clone(),
                texture_path: mesh.texture_path.clone(),
                material_path: mesh.material_path.clone(),
                glow_map: mesh.glow_map.clone(),
                detail_map: mesh.detail_map.clone(),
                gloss_map: mesh.gloss_map.clone(),
                dark_map: mesh.dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
            },
        );
        // Load and attach normal map if the material specifies one.
        if let Some(ref nmap_path) = mesh.normal_map {
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
        // Attach ESM light_data ONLY if the NIF has no embedded lights
        // (avoids duplicates) and only on the first mesh (avoids N copies
        // when a lamp NIF has multiple sub-meshes).
        if let Some(ld) = light_data {
            if nif_lights.is_empty() && count == 0 {
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
fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)
}
