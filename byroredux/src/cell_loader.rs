//! Cell scene loader — loads an FNV interior cell from ESM + BSA into ECS entities.
//!
//! Given a cell editor ID, this module:
//! 1. Parses the ESM file to find the cell and its placed references
//! 2. Resolves each reference's base form ID to a STAT record (NIF path)
//! 3. Loads each NIF from the BSA, uploads meshes + textures
//! 4. Spawns ECS entities with correct world-space transforms

use byroredux_core::ecs::{MeshHandle, TextureHandle, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::{self, CellData, EsmCellIndex};
use byroredux_renderer::VulkanContext;
use std::collections::HashMap;
use std::path::Path;

use crate::{load_nif_bytes, TextureProvider};

/// Result of loading a cell.
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
    let alloc = ctx.allocator.as_ref().unwrap();
    let mut entity_count = 0;
    let mut mesh_cache: HashMap<String, (u32, u32)> = HashMap::new(); // path → (mesh_handle, tex_handle)
    let mut bounds_min = Vec3::splat(f32::INFINITY);
    let mut bounds_max = Vec3::splat(f32::NEG_INFINITY);

    for placed_ref in &cell.references {
        // Resolve base form ID → STAT → model path.
        let stat = match index.statics.get(&placed_ref.base_form_id) {
            Some(s) => s,
            None => {
                // Not a STAT — could be FURN, DOOR, MSTT, ACTI, etc. Skip for now.
                continue;
            }
        };

        let model_path = &stat.model_path;

        // Check mesh cache — don't re-upload the same NIF.
        let (mesh_handle, tex_handle) = if let Some(&cached) = mesh_cache.get(model_path) {
            cached
        } else {
            // Load NIF from BSA.
            let nif_data = match tex_provider.extract_mesh(model_path) {
                Some(d) => d,
                None => {
                    log::debug!("NIF not found in BSA: '{}'", model_path);
                    continue;
                }
            };

            // Parse NIF and upload first mesh (most STATs have one shape).
            let mesh_count = load_nif_bytes(world, ctx, &nif_data, model_path, tex_provider);
            if mesh_count == 0 {
                continue;
            }

            // The NIF loader spawned entities — get the last one's handles.
            // This is a workaround until we have a proper "upload mesh only" API.
            // For now, we'll record the handles from the NIF loader's entities.
            let (mh, th) = get_last_mesh_handles(world);
            mesh_cache.insert(model_path.clone(), (mh, th));
            (mh, th)
        };

        // Convert Z-up → Y-up: position (x,y,z) → (x,z,-y), rotation similarly.
        let pos = Vec3::new(
            placed_ref.position[0],
            placed_ref.position[2],
            -placed_ref.position[1],
        );

        // Euler rotation: Bethesda stores (rx, ry, rz) in Z-up.
        // Convert to Y-up quaternion.
        let rot = euler_zup_to_quat_yup(
            placed_ref.rotation[0],
            placed_ref.rotation[1],
            placed_ref.rotation[2],
        );

        // Update bounds.
        bounds_min = bounds_min.min(pos);
        bounds_max = bounds_max.max(pos);

        // Spawn entity.
        let entity = world.spawn();
        world.insert(entity, Transform::new(pos, rot, placed_ref.scale));
        world.insert(entity, MeshHandle(mesh_handle));
        world.insert(entity, TextureHandle(tex_handle));
        entity_count += 1;
    }

    let center = (bounds_min + bounds_max) * 0.5;

    log::info!(
        "Cell '{}' loaded: {} entities, {} unique meshes, center=[{:.0},{:.0},{:.0}]",
        cell.editor_id, entity_count, mesh_cache.len(),
        center.x, center.y, center.z,
    );

    Ok(CellLoadResult {
        cell_name: cell.editor_id.clone(),
        entity_count,
        mesh_count: mesh_cache.len(),
        center,
    })
}

/// Convert Euler angles (radians, Z-up Bethesda convention) to a Y-up quaternion.
fn euler_zup_to_quat_yup(rx: f32, ry: f32, rz: f32) -> Quat {
    // Bethesda Euler order: Z * Y * X (extrinsic) in Z-up space.
    // Build quaternion in Z-up, then rotate the whole thing to Y-up.
    let qx = Quat::from_rotation_x(rx);
    let qy = Quat::from_rotation_y(ry);
    let qz = Quat::from_rotation_z(rz);
    let zup_quat = qz * qy * qx;

    // Z-up → Y-up rotation: 90° around X (rotates Z axis to Y axis).
    let to_yup = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    to_yup * zup_quat
}

/// Get the mesh and texture handles of the most recently spawned entity with MeshHandle.
fn get_last_mesh_handles(world: &World) -> (u32, u32) {
    let mut last_mesh = 0u32;
    let mut last_tex = 0u32;
    if let Some(mq) = world.query::<MeshHandle>() {
        for (entity, mh) in mq.iter() {
            last_mesh = mh.0;
            if let Some(tq) = world.query::<TextureHandle>() {
                if let Some(th) = tq.get(entity) {
                    last_tex = th.0;
                }
            }
        }
    }
    (last_mesh, last_tex)
}
