//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.

use byroredux_core::ecs::{GlobalTransform, LightSource, MeshHandle, TextureHandle, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;

use crate::TextureProvider;

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

    log::info!("Parsing ESM '{}' ({:.1} MB)...", esm_path, esm_data.len() as f64 / 1_048_576.0);
    let index = esm::cell::parse_esm_cells(&esm_data)?;

    // 2. Find the cell.
    let cell_key = cell_editor_id.to_ascii_lowercase();
    let cell = index.cells.get(&cell_key)
        .ok_or_else(|| {
            // List available cells for debugging.
            let available: Vec<&str> = index.cells.values()
                .take(20)
                .map(|c| c.editor_id.as_str())
                .collect();
            anyhow::anyhow!(
                "Cell '{}' not found. {} interior cells available. Examples: {:?}",
                cell_editor_id, index.cells.len(), available,
            )
        })?;

    log::info!(
        "Loading cell '{}' (form {:08X}): {} placed references",
        cell.editor_id, cell.form_id, cell.references.len(),
    );

    // 3. Load placed references.
    let result = load_references(
        &cell.references, &index, world, ctx, tex_provider,
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

    log::info!("Parsing ESM '{}' ({:.1} MB)...", esm_path, esm_data.len() as f64 / 1_048_576.0);
    let index = esm::cell::parse_esm_cells(&esm_data)?;

    // Find the best worldspace. Try common FNV names, then fall back to largest.
    let wrld_key = {
        let preferred = ["wastelandnv", "tamriel", "skyrim"];
        preferred.iter()
            .find(|&&name| index.exterior_cells.contains_key(name))
            .map(|s| s.to_string())
            .or_else(|| {
                index.exterior_cells.iter()
                    .max_by_key(|(_, cells)| cells.len())
                    .map(|(name, _)| name.clone())
            })
    };

    let wrld_name = wrld_key.as_deref().unwrap_or("(none)");
    log::info!(
        "Loading exterior cells around ({},{}) radius {} from worldspace '{}'",
        center_x, center_y, radius, wrld_name,
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
                        gx, gy, cell.editor_id, cell.references.len(),
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
        cells_loaded, grid_size * grid_size, grid_size, grid_size,
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
    let mut mesh_cache: HashMap<String, Vec<u8>> = HashMap::new();
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
                log::debug!("REFR base {:08X} not in statics table", placed_ref.base_form_id);
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
                world.insert(entity, LightSource {
                    radius: ld.radius,
                    color: ld.color,
                    flags: ld.flags,
                });
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

        let model_path = if model_lower.starts_with("meshes\\")
            || model_lower.starts_with("meshes/") {
            stat.model_path.clone()
        } else {
            format!("meshes\\{}", stat.model_path)
        };

        let nif_data = match mesh_cache.get(&model_path) {
            Some(data) => data.clone(),
            None => {
                match tex_provider.extract_mesh(&model_path) {
                    Some(d) => {
                        mesh_cache.insert(model_path.clone(), d.clone());
                        d
                    }
                    None => {
                        log::debug!("NIF not found in BSA: '{}'", model_path);
                        continue;
                    }
                }
            }
        };

        let count = load_nif_placed(
            world, ctx, &nif_data, &model_path, tex_provider,
            ref_pos, ref_rot, ref_scale, stat.light_data.as_ref(),
        );
        entity_count += count;
    }

    let center = (bounds_min + bounds_max) * 0.5;
    let dims = bounds_max - bounds_min;
    log::info!(
        "'{}' loaded: {} entities, {} unique meshes, {} hits, {} misses",
        label, entity_count, mesh_cache.len(), stat_hit, stat_miss,
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
        mesh_count: mesh_cache.len(),
        center,
    }
}

/// Parse a NIF and spawn all its sub-meshes with a parent REFR transform applied.
///
/// Each NIF sub-mesh has its own local transform (from the NIF scene graph).
/// The REFR placement transform is composed on top as the parent.
fn load_nif_placed(
    world: &mut World,
    ctx: &mut VulkanContext,
    nif_data: &[u8],
    label: &str,
    tex_provider: &TextureProvider,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    light_data: Option<&esm::cell::LightData>,
) -> usize {
    use byroredux_renderer::Vertex;

    let scene = match byroredux_nif::parse_nif(nif_data) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to parse NIF '{}': {}", label, e);
            return 0;
        }
    };

    let imported = byroredux_nif::import::import_nif(&scene);
    let mut count = 0;

    for mesh in &imported {
        let num_verts = mesh.positions.len();
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                Vertex::new(
                    mesh.positions[i],
                    if i < mesh.colors.len() { mesh.colors[i] } else { [1.0, 1.0, 1.0] },
                    if i < mesh.normals.len() { mesh.normals[i] } else { [0.0, 1.0, 0.0] },
                    if i < mesh.uvs.len() { mesh.uvs[i] } else { [0.0, 0.0] },
                )
            })
            .collect();

        let mesh_handle = {
            let alloc = ctx.allocator.as_ref().unwrap();
            match ctx.mesh_registry.upload(&ctx.device, alloc, &ctx.graphics_queue, ctx.command_pool, &vertices, &mesh.indices, ctx.device_caps.ray_query_supported) {
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
                    ctx.texture_registry.load_dds(
                        &ctx.device, alloc, &ctx.graphics_queue, ctx.command_pool,
                        tex_path, &dds_bytes,
                    ).unwrap_or_else(|e| {
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
            mesh.rotation[0], mesh.rotation[1], mesh.rotation[2], mesh.rotation[3],
        );
        let nif_pos = Vec3::new(mesh.translation[0], mesh.translation[1], mesh.translation[2]);

        // Composed: parent_rot * (parent_scale * child_pos) + parent_pos
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let final_rot = ref_rot * nif_quat;
        let final_scale = ref_scale * mesh.scale;

        // Diagnostic: log meshes with significant NIF-internal offsets
        // (these are wall/structural pieces most likely to show positioning issues)
        let nif_offset_len = nif_pos.length();
        if nif_offset_len > 50.0 {
            log::debug!(
                "  NIF offset {:.0} for '{}' mesh {:?}: nif_pos=({:.0},{:.0},{:.0}) \
                 final=({:.0},{:.0},{:.0})",
                nif_offset_len, label, mesh.name,
                nif_pos.x, nif_pos.y, nif_pos.z,
                final_pos.x, final_pos.y, final_pos.z,
            );
        }

        let entity = world.spawn();
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(entity, GlobalTransform::new(final_pos, final_rot, final_scale));
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));
        if mesh.has_alpha {
            world.insert(entity, crate::AlphaBlend);
        }
        if mesh.two_sided {
            world.insert(entity, crate::TwoSided);
        }
        if mesh.is_decal {
            world.insert(entity, crate::Decal);
        }
        if let Some(ld) = light_data {
            world.insert(entity, LightSource {
                radius: ld.radius,
                color: ld.color,
                flags: ld.flags,
            });
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

