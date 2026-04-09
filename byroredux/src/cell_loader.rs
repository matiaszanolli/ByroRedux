//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.

use byroredux_core::ecs::{
    GlobalTransform, LightSource, MeshHandle, TextureHandle, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;
use std::sync::Arc;

use crate::asset_provider::TextureProvider;
use crate::components::{AlphaBlend, Decal, TwoSided};

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

    Ok(CellLoadResult {
        cell_name: cell.editor_id.clone(),
        entity_count: result.entity_count,
        mesh_count: result.mesh_count,
        center: result.center,
        lighting: cell.lighting.clone(),
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
    let esm_data = std::fs::read(esm_path)
        .map_err(|e| anyhow::anyhow!("Failed to read ESM '{}': {}", esm_path, e))?;

    log::info!(
        "Parsing ESM '{}' ({:.1} MB)...",
        esm_path,
        esm_data.len() as f64 / 1_048_576.0
    );
    let index = esm::cell::parse_esm_cells(&esm_data)?;

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

    // Collect all references from cells in the grid.
    let mut all_refs = Vec::new();
    let mut cells_loaded = 0u32;
    if let Some(cells_map) = wrld_cells {
        for gx in (center_x - radius)..=(center_x + radius) {
            for gy in (center_y - radius)..=(center_y + radius) {
                if let Some(cell) = cells_map.get(&(gx, gy)) {
                    log::info!(
                        "  Cell ({},{}) '{}': {} references",
                        gx,
                        gy,
                        cell.editor_id,
                        cell.references.len(),
                    );
                    all_refs.extend_from_slice(&cell.references);
                    cells_loaded += 1;
                }
            }
        }
    }

    let grid_size = (radius * 2 + 1) as u32;
    log::info!(
        "Found {}/{} cells in {}x{} grid",
        cells_loaded,
        grid_size * grid_size,
        grid_size,
        grid_size,
    );

    let label = format!("exterior({},{})", center_x, center_y);
    let result = load_references(&all_refs, &index, world, ctx, tex_provider, &label);

    Ok(CellLoadResult {
        cell_name: format!("{} ({},{})", wrld_name, center_x, center_y),
        entity_count: result.entity_count,
        mesh_count: result.mesh_count,
        center: result.center,
        lighting: None,
    })
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

        // Skip non-renderable effect meshes.
        let model_lower = stat.model_path.to_ascii_lowercase();
        if model_lower.contains("fxlightrays")
            || model_lower.contains("fxlight")
            || model_lower.contains("fxfog")
        {
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
    for light in nif_lights {
        // Skip lights whose diffuse contribution is effectively zero —
        // these are usually authored-off placeholders.
        if light.color[0] + light.color[1] + light.color[2] < 1e-4 {
            continue;
        }
        let nif_pos = Vec3::new(light.translation[0], light.translation[1], light.translation[2]);
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        // Ambient / directional lights have no meaningful placement radius;
        // fall back to a large cell-scale default so the renderer still
        // picks them up instead of culling by radius == 0.
        let radius = if light.radius > 0.0 {
            light.radius * ref_scale
        } else {
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

        let entity = world.spawn();
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(
            entity,
            GlobalTransform::new(final_pos, final_rot, final_scale),
        );
        world.insert(entity, coll.shape.clone());
        world.insert(entity, coll.body.clone());
    }

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
            match ctx.mesh_registry.upload(
                &ctx.device,
                alloc,
                &ctx.graphics_queue,
                ctx.command_pool,
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

        // Build BLAS for RT shadow rays.
        ctx.build_blas_for_mesh(mesh_handle, num_verts as u32, mesh.indices.len() as u32);

        // Load texture.
        let tex_handle = match &mesh.texture_path {
            Some(tex_path) => {
                if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
                    cached
                } else if let Some(dds_bytes) = tex_provider.extract(tex_path) {
                    let alloc = ctx.allocator.as_ref().unwrap();
                    ctx.texture_registry
                        .load_dds(
                            &ctx.device,
                            alloc,
                            &ctx.graphics_queue,
                            ctx.command_pool,
                            tex_path,
                            &dds_bytes,
                        )
                        .unwrap_or_else(|e| {
                            log::warn!("Failed to load DDS '{}': {}", tex_path, e);
                            ctx.texture_registry.fallback()
                        })
                } else {
                    log::debug!("Texture not found in BSA: '{}'", tex_path);
                    ctx.texture_registry.fallback()
                }
            }
            None => ctx.texture_registry.fallback(),
        };

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
        if mesh.has_alpha {
            world.insert(entity, AlphaBlend);
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        if mesh.is_decal {
            world.insert(entity, Decal);
        }
        if let Some(ld) = light_data {
            world.insert(
                entity,
                LightSource {
                    radius: ld.radius,
                    color: ld.color,
                    flags: ld.flags,
                },
            );
        }
        count += 1;
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
