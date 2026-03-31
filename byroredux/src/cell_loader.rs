//! Cell scene loader — loads an FNV interior cell from ESM + BSA into ECS entities.
//!
//! Given a cell editor ID, this module:
//! 1. Parses the ESM file to find the cell and its placed references
//! 2. Resolves each reference's base form ID to a STAT record (NIF path)
//! 3. Loads each NIF from the BSA, uploads meshes + textures
//! 4. Spawns ECS entities with correct world-space transforms

use byroredux_core::ecs::{MeshHandle, TextureHandle, Transform, World};
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

    // 3. Load each placed reference.
    let mut entity_count = 0;
    let mut mesh_cache: HashMap<String, Vec<u8>> = HashMap::new(); // path → raw NIF bytes
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    let mut stat_miss = 0u32;
    let mut stat_hit = 0u32;
    for placed_ref in &cell.references {
        // Resolve base form ID → STAT → model path.
        let stat = match index.statics.get(&placed_ref.base_form_id) {
            Some(s) => {
                stat_hit += 1;
                s
            }
            None => {
                stat_miss += 1;
                log::debug!("REFR base {:08X} not in STAT table", placed_ref.base_form_id);
                continue;
            }
        };

        // STAT model paths omit the "meshes\" prefix that BSA paths include.
        let model_path = if stat.model_path.to_ascii_lowercase().starts_with("meshes\\")
            || stat.model_path.to_ascii_lowercase().starts_with("meshes/") {
            stat.model_path.clone()
        } else {
            format!("meshes\\{}", stat.model_path)
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
        if placed_ref.rotation[0].abs() > 0.001 || placed_ref.rotation[1].abs() > 0.001 {
            log::debug!(
                "REFR base {:08X} '{}' rot=({:.4}, {:.4}, {:.4}) pos=({:.0}, {:.0}, {:.0})",
                placed_ref.base_form_id,
                stat.model_path,
                placed_ref.rotation[0], placed_ref.rotation[1], placed_ref.rotation[2],
                placed_ref.position[0], placed_ref.position[1], placed_ref.position[2],
            );
        }
        let ref_scale = placed_ref.scale;

        // Update bounds.
        bounds_min = bounds_min.min(ref_pos);
        bounds_max = bounds_max.max(ref_pos);

        // For cached NIFs, we need to re-upload the meshes for each placement
        // because each REFR has its own transform. But we can avoid re-parsing
        // the NIF by caching the parsed import data.
        //
        // For now, load the NIF each time (parsing is fast, GPU upload dominates).
        // The mesh cache prevents re-loading the same NIF from BSA.
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

        // Parse NIF, upload meshes, spawn entities with REFR transform applied.
        let count = load_nif_placed(world, ctx, &nif_data, &model_path, tex_provider,
                                     ref_pos, ref_rot, ref_scale);
        entity_count += count;
    }

    let center = (bounds_min + bounds_max) * 0.5;

    log::info!(
        "Cell '{}' loaded: {} entities, {} unique meshes, {} STAT hits, {} STAT misses, center=[{:.0},{:.0},{:.0}]",
        cell.editor_id, entity_count, mesh_cache.len(), stat_hit, stat_miss,
        center.x, center.y, center.z,
    );

    Ok(CellLoadResult {
        cell_name: cell.editor_id.clone(),
        entity_count,
        mesh_count: mesh_cache.len(),
        center,
    })
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
) -> usize {
    use byroredux_core::math::Mat3;
    use byroredux_renderer::Vertex;

    let scene = match byroredux_nif::parse_nif(nif_data) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Failed to parse NIF '{}': {}", label, e);
            return 0;
        }
    };

    let imported = byroredux_nif::import::import_nif(&scene);
    let alloc = ctx.allocator.as_ref().unwrap();
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

        let mesh_handle = match ctx.mesh_registry.upload(&ctx.device, alloc, &vertices, &mesh.indices) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Failed to upload mesh: {}", e);
                continue;
            }
        };

        // Load texture.
        let tex_handle = match &mesh.texture_path {
            Some(tex_path) => {
                if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
                    cached
                } else if let Some(dds_bytes) = tex_provider.extract(tex_path) {
                    ctx.texture_registry.load_dds(
                        &ctx.device, alloc, ctx.graphics_queue, ctx.command_pool,
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

        let entity = world.spawn();
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));
        if mesh.has_alpha {
            world.insert(entity, crate::AlphaBlend);
        }
        if mesh.two_sided {
            world.insert(entity, crate::TwoSided);
        }
        count += 1;
    }

    count
}

/// Convert Euler angles (radians, Z-up Bethesda convention) to a Y-up quaternion.
///
/// Bethesda rotation in Z-up: R = Rz(rz) · Ry(ry) · Rx(rx)
///
/// Coordinate change C: (x,y,z)_zup → (x,z,-y)_yup conjugates each:
///   C · Rx(a) · C^T = Rx(a)    (x → x)
///   C · Ry(a) · C^T = Rz(-a)   (y → -z)
///   C · Rz(a) · C^T = Ry(a)    (z → y)
///
/// Result: R_yup = Ry(rz) · Rz(-ry) · Rx(rx)
fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    Quat::from_rotation_y(rz) * Quat::from_rotation_z(-ry) * Quat::from_rotation_x(rx)
}

