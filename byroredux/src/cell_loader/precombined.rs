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
//! **Current state** (M49 — complete): this loader reads each `_oc.nif`
//! from the asset BSA chain (typically `Fallout4 - MeshesExtra.ba2`), then
//! resolves the vertex / triangle data from the companion `Fallout4 - Geometry.csg`
//! blob via `CsgArchive` (one big zlib-compressed PSG keyed by filename
//! hash + offset per `BSPackedGeomObject`). Meshes are decoded to Y-up, spawned
//! at cell-local identity, and tagged as `RenderLayer::Architecture`. LOD is
//! selected by triangle count (finest LOD only, per `fo4-csg-format.md:138-142`).
//! Absorption gate in [`super::load::load_cell_with_masters`] (conditional on
//! spawn count) honors the cell's `absorbed_refs` list, suppressing per-REFR
//! rendering of baked REFRs.
//!
//! Deferred sub-items (M49 Stage B):
//! - Collision (`_precomb.nif` siblings) — authored convex hulls for baked
//!   surfaces. FO4 architecture today gets synthesized trimesh colliders via
//!   fallback in `spawn.rs`, spawned as separate MeshHandle-free ghost
//!   entities so they stay out of BLAS/TLAS.
//! - Visibility / `.uvd` occlusion data — previs PVS keyed to visibility groups.
//!   Currently no occlusion-volume or CPU coarse-cull system exists.

use byroredux_bsa::CsgArchive;
use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::import::precombine::{decode_shared_geom_object, psg_vertex_stride};
use byroredux_nif::import::{ImportedMesh, MeshResolver};
use byroredux_nif::scene::NifScene;
use byroredux_plugin::esm::cell::CellData;
use byroredux_renderer::vulkan::context::VulkanContext;
use std::path::Path;
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
    // Path to the cell's master plugin (e.g. `…/Data/Fallout4.esm`). Used
    // to locate the companion `<Plugin> - Geometry.csg` blob that holds
    // the shared-variant vertex/triangle data (M49). When the CSG is
    // absent (non-FO4, or no shared precombines) the loader falls back to
    // per-REFR rendering as before.
    plugin_path: &str,
) -> (usize, usize) {
    if cell.precombined_mesh_hashes.is_empty() {
        return (0, 0);
    }

    // Resolve the shared-geometry CSG. #1585 / F6 — route through the
    // `MaterialProvider` cache so the ~240 MB blob is opened (and its chunk
    // table parsed) once per session instead of once per cell, preserving the
    // warm zlib `ChunkCache` across adjacent tiles. When no provider is
    // present (paths that pass `None`) fall back to an uncached open so CSG
    // resolution still works. `None` keeps the pre-M49 REFR fallback.
    let csg: Option<Arc<CsgArchive>> = match mat_provider.as_deref_mut() {
        Some(mp) => mp.geometry_csg(plugin_path),
        None => open_geometry_csg(plugin_path).map(Arc::new),
    };

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
            // #1217 / D2 FIND-3 — cache-hit on a zero-mesh entry surfaces
            // post-mortem visibility for the CSG-deferred fallback. The
            // first cache MISS fires the zero-contribution warn in
            // `parse_and_import_nif` (#1215); subsequent cells re-using
            // the same `_oc.nif` path hit this branch and skip the
            // warn site. Without this debug line an operator only
            // sees the first occurrence per process.
            if c.meshes.is_empty() && c.collisions.is_empty() && c.lights.is_empty() {
                log::debug!(
                    "PreCombined cache hit on zero-mesh entry: '{}' \
                     (cell {:08X}) — CSG-deferred fallback",
                    path,
                    cell.form_id,
                );
            }
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
            // M49 — shared-variant precombines store their geometry in the
            // companion `.csg`, which the standard walk-based import skips
            // (it produces zero meshes). When the CSG resolved, decode the
            // packed-combined objects directly into spawnable meshes. Falls
            // through to the standard import path when the CSG is absent or
            // the `_oc.nif` carries no shared geometry (baked variant /
            // non-precombine content).
            let csg_parsed: Option<Arc<CachedNifImport>> =
                csg.as_ref()
                    .and_then(|csg| match byroredux_nif::parse_nif(&bytes) {
                        Ok(scene) => {
                            let meshes = {
                                let mut pool = world.resource_mut::<StringPool>();
                                build_precombine_meshes(&scene, csg, &mut pool)
                            };
                            (!meshes.is_empty()).then(|| Arc::new(geometry_only_cached(meshes)))
                        }
                        Err(e) => {
                            log::warn!(
                                "PreCombined CSG parse failed: '{path}' (cell {:08X}): {e}",
                                cell.form_id
                            );
                            None
                        }
                    });
            let parsed = match csg_parsed {
                Some(c) => Some(c),
                None => {
                    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                    super::references::parse_and_import_nif_pub(
                        &bytes,
                        &path,
                        mat_provider.as_deref_mut(),
                        &mut pool,
                        Some(tex_provider as &dyn MeshResolver),
                    )
                }
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
        // M47.0 Phase 3b — discard the placement_root return.
        // Precombined bake artifacts have no per-REFR script binding
        // (they're geometry merges, not source REFRs); script attach
        // runs only on the references.rs call site.
        let (_placement_root, count) = spawn_placed_instances(
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
            /* placement_form_id_pair = */
            None,
            // Precombines absorb static architecture / clutter; doors
            // are excluded from the absorption set by Bethesda's bake
            // pipeline, so this path never carries XTEL.
            /* teleport = */
            None,
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

/// Open the `<Plugin> - Geometry.csg` blob that sits next to `plugin_path`
/// in the Data directory (M49). Vanilla FO4 precombines reference a single
/// CSG named for the cell's master plugin (`Fallout4 - Geometry.csg`); the
/// `BSPackedGeomObject.filename_hash` cross-check (BSCRC32) is a follow-up,
/// so v1 keys purely off the plugin stem. Returns `None` when the plugin
/// has no companion CSG (non-FO4 content, or a plugin that authored no
/// shared precombines) — the caller then falls back to per-REFR rendering.
pub(crate) fn open_geometry_csg(plugin_path: &str) -> Option<CsgArchive> {
    let p = Path::new(plugin_path);
    let dir = p.parent()?;
    let stem = p.file_stem()?.to_str()?;
    let csg_path = dir.join(format!("{stem} - Geometry.csg"));
    if !csg_path.is_file() {
        return None;
    }
    match CsgArchive::open(&csg_path) {
        Ok(a) => {
            log::info!(
                "PreCombined: opened CSG '{}' ({} objects, {} chunks)",
                csg_path.display(),
                a.num_objects(),
                a.num_chunks(),
            );
            Some(a)
        }
        Err(e) => {
            log::warn!(
                "PreCombined: failed to open CSG '{}': {e}",
                csg_path.display()
            );
            None
        }
    }
}

/// Resolve every `BSPackedCombinedSharedGeomDataExtra` object in a
/// precombined `_oc.nif` scene against `csg`, producing one spawnable
/// [`ImportedMesh`] per placed instance (M49). Pure (no GPU / ECS) so it
/// is unit-testable against real data without a Vulkan device.
///
/// Each object's geometry is decoded once and cloned per
/// `BSPackedGeomDataCombined` instance transform. Objects whose CSG slice
/// is missing or fails to decode are skipped with a debug log rather than
/// aborting the whole bake. The Baked variant
/// (`BSPackedCombinedGeomDataExtra`, geometry inline) is not vanilla and
/// is left for a follow-up.
pub(super) fn build_precombine_meshes(
    scene: &NifScene,
    csg: &CsgArchive,
    pool: &mut StringPool,
) -> Vec<ImportedMesh> {
    let mut meshes = Vec::new();
    // `collect_precombine_geom_refs` pairs each shared-geometry object with
    // the material the owning shape's shader/alpha properties resolve to
    // (M49 texturing) — so precombines render with their real diffuse /
    // normal / alpha-test instead of the untextured placeholder.
    for geom in byroredux_nif::import::precombine::collect_precombine_geom_refs(scene, pool) {
        if geom.num_verts == 0 {
            continue;
        }
        let stride = psg_vertex_stride(geom.vertex_desc);
        // The 3 LODs are alternative triangulations of the SAME surface
        // (nif.xml: "switch a geometry at a specified distance"), stored
        // back-to-back as `[LOD0][LOD1][LOD2]` in one index buffer.
        // Rendering more than one z-fights — pick the finest (highest
        // triangle count); LOD index is NOT a reliable detail order (some
        // objects ship lod0 ≫ lod2, others lod0 ≪ lod2). The chosen LOD's
        // triangles start at its index-unit offset / 3.
        let (lod_count, lod_off_idx) = (0..3)
            .map(|i| (geom.lod_counts[i], geom.lod_offsets[i]))
            .max_by_key(|&(c, _)| c)
            .unwrap();
        let lod_count = lod_count as usize;
        if lod_count == 0 {
            continue;
        }
        let tri_start = (lod_off_idx / 3) as usize;
        let need = geom.num_verts * stride + (tri_start + lod_count) * 6;
        let psg = match csg.read_psg(geom.data_offset as u64, need) {
            Ok(b) => b,
            Err(e) => {
                log::debug!(
                    "PreCombined: CSG read at offset {} failed: {e}",
                    geom.data_offset
                );
                continue;
            }
        };
        let decoded = match decode_shared_geom_object(
            &psg,
            geom.vertex_desc,
            geom.num_verts,
            tri_start,
            lod_count,
        ) {
            Ok(g) => g,
            Err(e) => {
                log::debug!(
                    "PreCombined: decode at offset {} failed: {e}",
                    geom.data_offset
                );
                continue;
            }
        };
        // One placed instance per combined transform, each carrying the
        // resolved material. Objects with no combined entries carry no
        // placement (an unplaced merge) and contribute nothing.
        for inst in &geom.instances {
            let mut mesh = decoded.clone().into_imported_mesh(inst);
            geom.material.apply(&mut mesh);
            meshes.push(mesh);
        }
    }
    meshes
}

/// Wrap precombine-decoded meshes in a geometry-only [`CachedNifImport`]
/// (no collisions / lights / clips / particles) so the existing
/// [`spawn_placed_instances`] path uploads + spawns them.
fn geometry_only_cached(meshes: Vec<ImportedMesh>) -> CachedNifImport {
    CachedNifImport {
        meshes,
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard: None,
        bsx_flags: 0,
        root_flags: 0,
        flame_attach_offset: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_bsa::Ba2Archive;
    use std::path::PathBuf;

    fn fo4_data_dir() -> Option<PathBuf> {
        if let Ok(v) = std::env::var("BYROREDUX_FO4_DATA") {
            let p = PathBuf::from(&v);
            if p.is_dir() {
                return Some(p);
            }
        }
        let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data");
        p.is_dir().then_some(p)
    }

    /// Real-data, Vulkan-free regression for the M49 spawn path's decode
    /// half: a vanilla FO4 `_oc.nif` + `Fallout4 - Geometry.csg` must
    /// yield non-empty, index-valid meshes. Gated on `BYROREDUX_FO4_DATA`:
    /// `cargo test -p byroredux -- --ignored build_precombine_meshes`.
    #[test]
    #[ignore]
    fn build_precombine_meshes_decodes_real_oc_nif() {
        let Some(data) = fo4_data_dir() else {
            eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
            return;
        };
        let ba2 = match Ba2Archive::open(data.join("Fallout4 - MeshesExtra.ba2")) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Skipping: open MeshesExtra.ba2: {e}");
                return;
            }
        };
        let csg = match CsgArchive::open(data.join("Fallout4 - Geometry.csg")) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping: open Geometry.csg: {e}");
                return;
            }
        };

        let bytes = ba2
            .extract("meshes\\precombined\\0000e2db_02be5e11_oc.nif")
            .expect("extract _oc.nif");
        let scene = byroredux_nif::parse_nif(&bytes).expect("parse _oc.nif");
        let mut pool = StringPool::new();
        let meshes = build_precombine_meshes(&scene, &csg, &mut pool);

        assert!(
            !meshes.is_empty(),
            "shared precombine must decode at least one mesh from the CSG"
        );
        let mut textured = 0usize;
        for m in &meshes {
            assert!(!m.positions.is_empty(), "mesh has vertices");
            assert!(!m.indices.is_empty(), "mesh has indices");
            assert_eq!(m.normals.len(), m.positions.len(), "normal per vertex");
            let max_idx = m.indices.iter().copied().max().unwrap();
            assert!(
                (max_idx as usize) < m.positions.len(),
                "index {max_idx} in range for {} verts",
                m.positions.len()
            );
            if m.texture_path.is_some() {
                textured += 1;
            }
        }
        // M49 texturing: this object's shape resolves a real diffuse path
        // (Landscape/Rocks/CoastCliff01Wet_d.dds), so the mesh must carry it.
        assert!(
            textured > 0,
            "precombine meshes must resolve a diffuse texture from the owning shape"
        );
        eprintln!(
            "build_precombine_meshes: decoded {} mesh(es), {textured} textured",
            meshes.len()
        );
    }
}
