//! FO4+ PreCombined Mesh loader (#1188).
//!
//! Bethesda's CK / GECK bakes individual architecture STAT placements
//! (walls, floors, ceilings, ductwork, etc.) into a single
//! `meshes\precombined\<cell_formid:08x>_<hash:08x>_oc.nif` file per
//! cell-tile. Those individual REFRs are then **absorbed** — the cell
//! record's REFR list still carries them, but with the XPRI sub-record
//! flagging them as precombined. The runtime spawns the combined NIF
//! instead.
//!
//! **Current state**: this loader walks the `_oc.nif` files but each
//! BSTriShape ships with `num_vertices = 0` and an empty inline buffer
//! — vanilla FO4 stores the actual vertex / triangle bytes in a
//! companion `Fallout4 - Geometry.csg` blob (one big binary keyed by
//! filename hash + offset, per `BSPackedGeomObject`). Until a CSG
//! reader lands, **this pass spawns zero entities and the conditional
//! gate in [`super::load::load_cell_with_masters`] falls back to
//! per-REFR rendering** (matches Bethesda's `bUseCombinedObjects=0`
//! behaviour). Without that fallback Dugout Inn rendered as "props
//! floating in a void" (2026-05-19).
//!
//! Stage A scope (extraction works, no geometry):
//! - Read each `_oc.nif` from the asset BSA chain (typically
//!   `Fallout4 - MeshesExtra.ba2`).
//! - Parse it through the standard NIF pipeline.
//! - Spawn entities at cell-local identity if any mesh survives import.
//! - Tag as `RenderLayer::Architecture`.
//!
//! Deferred (future PreCombined-Geometry milestone):
//! - CSG / PSG companion reader (the actual data we're missing today).
//! - LOD-mip selection (load level-0 only).
//! - Visibility / `.uvd` occlusion data.
//! - Collision (`_precomb.nif` siblings).

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_nif::import::MeshResolver;
use byroredux_plugin::esm::cell::CellData;
use byroredux_renderer::vulkan::context::VulkanContext;
use std::sync::Arc;

use super::nif_import_registry::{CachedNifImport, NifImportRegistry};
use super::spawn::spawn_placed_instances;
use crate::asset_provider::{MaterialProvider, TextureProvider};

/// Spawn the precombined `_oc.nif` files referenced by `cell.precombined_mesh_hashes`.
///
/// `cell_origin` is the world-space position the bake should land at:
/// - **Interior cells** (called from `load.rs`): pass `Vec3::ZERO` — the
///   interior cell IS the world origin, so the bake's cell-local coords
///   already are world coords (#1222 / D3-NEW-03).
/// - **Exterior cells** (called from `exterior.rs`, #1221 / D3-NEW-02):
///   pass `cell_grid_to_world_yup(gx, gy)` so the bake lands at the
///   correct Commonwealth tile position. Without this offset every
///   exterior precombine would stack at the world origin.
///
/// Returns `(spawned_entities, skipped_misses)` — the second number
/// counts hashes whose `_oc.nif` file failed to extract from the
/// asset chain (missing texture archive, mod-content cell that
/// references stripped precombines, etc.).
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_precombined_meshes(
    cell: &CellData,
    cell_origin: Vec3,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
) -> (usize, usize) {
    if cell.precombined_mesh_hashes.is_empty() {
        return (0, 0);
    }

    // Precombined NIFs are baked in cell-local coords; `cell_origin`
    // shifts them into world space (zero for interior, cell-grid-
    // derived for exterior). No rotation / scale — the bake is
    // axis-aligned and at unit scale by construction.
    let pos = cell_origin;
    let rot = Quat::IDENTITY;
    let scale = 1.0;

    let mut spawned = 0usize;
    let mut misses = 0usize;

    for hash in &cell.precombined_mesh_hashes {
        let path = format!(
            "meshes\\precombined\\{:08x}_{:08x}_oc.nif",
            cell.form_id, hash
        );

        // Check the process-lifetime cache first; precombined NIFs are
        // typically unique-per-cell so the hit-rate is near zero on
        // cold loads but we still want the path through the LRU so
        // the `_oc.nif` survives a brief un/reload (e.g. interior →
        // re-enter same cell).
        let cached: Option<Arc<CachedNifImport>> = {
            let reg = world.resource::<NifImportRegistry>();
            reg.get(&path).and_then(|opt| opt.clone())
        };

        let cached = if let Some(c) = cached {
            c
        } else {
            // Cache miss — extract + parse + import + commit. Use the
            // same `parse_and_import_nif` path that loose REFR meshes
            // use (BGSM merge + collision extraction + animation
            // capture) so precombines benefit from the same texture /
            // material plumbing.
            let bytes = match tex_provider.extract_mesh(&path) {
                Some(b) => b,
                None => {
                    if misses < 3 {
                        // Surface the first 3 misses at WARN so an
                        // operator can verify the path shape. Default
                        // log level may suppress debug!. The bulk
                        // miss count is logged at the end of this fn.
                        log::warn!(
                            "PreCombined miss: '{}' not found in mesh archives \
                             (cell {:08X}, hash {:08x})",
                            path,
                            cell.form_id,
                            hash,
                        );
                    }
                    misses += 1;
                    continue;
                }
            };
            let parsed = {
                let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                super::references::parse_and_import_nif_pub(
                    &bytes,
                    &path,
                    mat_provider.as_deref_mut(),
                    &mut pool,
                    Some(tex_provider as &dyn MeshResolver),
                )
            };
            // Commit to registry so a re-load of this cell hits the cache.
            {
                let mut reg = world.resource_mut::<NifImportRegistry>();
                let _freed = reg.insert(path.clone(), parsed.clone());
            }
            match parsed {
                Some(c) => c,
                None => {
                    log::warn!(
                        "PreCombined parse failed: '{}' (cell {:08X})",
                        path,
                        cell.form_id,
                    );
                    misses += 1;
                    continue;
                }
            }
        };

        // Spawn one entity per precombined NIF at the cell origin.
        // No REFR overlay (precombines bake textures into the geometry
        // already), no embedded clip handle (precombines are static),
        // no light data (precombines exclude lights — those stay as
        // individual REFRs outside the absorption set).
        let count = spawn_placed_instances(
            world,
            ctx,
            &cached,
            tex_provider,
            pos,
            rot,
            scale,
            /* light_data = */ None,
            /* refr_overlay = */ None,
            /* clip_handle = */ None,
            RenderLayer::Architecture,
            /* mesh_cache_key = */ Some(&path),
            // Precombined entities are bake artifacts, not placed REFRs
            // — no placement form-id. #1212.
            /* placement_form_id_pair = */ None,
        );
        spawned += count;
    }

    if misses > 0 {
        log::info!(
            "  PreCombined: {} hashes — {} entities spawned, {} misses (#1188)",
            cell.precombined_mesh_hashes.len(),
            spawned,
            misses,
        );
    } else {
        log::info!(
            "  PreCombined: {} hashes — {} entities spawned (#1188)",
            cell.precombined_mesh_hashes.len(),
            spawned,
        );
    }

    (spawned, misses)
}
