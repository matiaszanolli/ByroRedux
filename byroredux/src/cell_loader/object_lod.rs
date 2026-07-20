//! Distant **object** LOD (Skyrim+ / FO4) — the prebaked per-quad `.bto`
//! macro-meshes that combine many static objects into one mesh per quad.
//!
//! This is the object-LOD counterpart to [`super::terrain_lod`] (which
//! synthesizes distant *terrain* from heightmaps). The two are structurally
//! different LOD schemes (EXAL §5, docs/engine/exal.md):
//!
//! - **Skyrim LE/SE, FO4** (this module): runtime loads baked per-quad
//!   `.bto` files — renamed NIFs — selected purely **by filename**
//!   (`meshes\terrain\<world>\objects\<world>.<level>.<x>.<y>.bto`). STAT
//!   `MNAM` is generation-time only; the engine never reads it at runtime
//!   (EXAL Q3, verified). The base record's VWD / "Has Distant LOD" flag is
//!   the one runtime signal — it culls the full model so the LOD doesn't
//!   z-fight it (future slice; for now object LOD is loaded only for quads
//!   **outside** the full-detail ring, where no full model is resident).
//! - **Oblivion / FO3 / FNV**: a different scheme entirely — per-cell
//!   `DistantLOD\*.lod` placement lists instancing `_far.nif` meshes,
//!   handled by the sibling [`super::placement_lod`] module (#1726).
//!
//! Verified (2026-06-02): vanilla Skyrim `.bto` (e.g.
//! `meshes\terrain\tamriel\objects\tamriel.4.-8.-16.bto`) parse with the
//! existing NIF pipeline (BSVER 100 / v20.2.0.7) and yield geometry — so the
//! loader is "resolve the filename → extract from BSA → `import_nif_scene`
//! → spawn the meshes as LOD entities", reusing the proven paths.

use std::collections::HashMap;

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::IsLodTerrain;

use super::exterior::ExteriorWorldContext;

/// Object-LOD quad level streamed by the first cut — level 4 (4×4-cell
/// quads), the closest / highest-detail band. Coarser bands (8/16/32) for
/// farther distances are a follow-up (a multi-band selector like terrain
/// LOD's single ring → multiple rings).
pub(crate) const OBJECT_LOD_LEVEL: i32 = 4;

/// Object-LOD ring radius in **cells** (Chebyshev). Quads whose nearest cell
/// is within this distance of the player — and entirely outside the
/// full-detail ring — load their `.bto`. 16 cells ≈ 65 K BU of distant
/// objects; conservative first-cut value (object LOD is many meshes per quad,
/// heavier than terrain LOD). Tunable.
pub(crate) const OBJECT_LOD_RADIUS_CELLS: i32 = 16;

/// One streamed object-LOD quad: the `.bto` macro-mesh imports to several
/// sub-meshes, each spawned as its own [`IsLodTerrain`] entity. Tracked so a
/// quad leaving the ring frees every mesh + entity (mirrors
/// [`super::terrain_lod::LodBlock`], but a quad is 1→N meshes).
pub(crate) struct ObjectLodBlock {
    pub(crate) entities: Vec<EntityId>,
    pub(crate) mesh_handles: Vec<u32>,
    /// Shared worldspace object atlas `TextureHandle`, acquired once via
    /// `resolve_texture` (refcount bump) per quad and reused across the
    /// sub-meshes. Dropped once on unload — `World::despawn` has no GPU side
    /// effects, so without it the refcount never reaches 0 (#1537, sibling
    /// of the terrain-LOD leak). `0` = fallback/untextured, never refcounted.
    pub(crate) texture_handle: u32,
}

impl ObjectLodBlock {
    /// Sentinel for a quad that has no baked `.bto` (most do not). Inserted
    /// so the streaming reconcile does not re-extract a missing archive entry
    /// on every cell-boundary crossing.
    fn empty() -> Self {
        Self {
            entities: Vec::new(),
            mesh_handles: Vec::new(),
            texture_handle: 0,
        }
    }
}

/// Minimum Chebyshev distance (in cells) from `player` to any cell of the
/// level-`level` quad with SW corner `(qx, qy)` (cells `[qx, qx+level) ×
/// [qy, qy+level)`). `0` when the player stands inside the quad.
fn quad_min_chebyshev(qx: i32, qy: i32, level: i32, player: (i32, i32)) -> i32 {
    let nx = player.0.clamp(qx, qx + level - 1);
    let ny = player.1.clamp(qy, qy + level - 1);
    (player.0 - nx).abs().max((player.1 - ny).abs())
}

/// Quads whose `.bto` should be resident this frame: level-aligned, within
/// [`OBJECT_LOD_RADIUS_CELLS`] of the player, and **entirely** beyond
/// `max_full_cell_radius` (every cell the quad covers has
/// `quad_min_chebyshev > max_full_cell_radius`). Mirrors
/// [`super::placement_lod::placement_lod_cells_in_radius`], but per-quad
/// rather than per-cell.
///
/// `max_full_cell_radius` **must** be the caller's `radius_unload` — see
/// [`stream_object_lod_blocks`] (#1866 / LC0703-01).
fn object_lod_quads_in_radius(
    player: (i32, i32),
    max_full_cell_radius: i32,
    level: i32,
) -> Vec<(i32, i32)> {
    let mut quads = Vec::new();
    let rq = OBJECT_LOD_RADIUS_CELLS / level + 1;
    let (pqx, pqy) = quad_origin(player.0, player.1, level);
    for dj in -rq..=rq {
        for di in -rq..=rq {
            let qx = pqx + di * level;
            let qy = pqy + dj * level;
            let d = quad_min_chebyshev(qx, qy, level, player);
            if d > max_full_cell_radius && d <= OBJECT_LOD_RADIUS_CELLS {
                quads.push((qx, qy));
            }
        }
    }
    quads
}

/// Stream the distant **object** LOD ring around the player (Skyrim+/FO4).
/// Mirrors [`super::terrain_lod::stream_lod_blocks`]: quads entering the ring
/// load their `.bto`, quads leaving unload. A quad loads only when it is
/// **entirely outside** `max_full_cell_radius`, so the baked LOD never
/// overlaps a resident full model (the runtime half of the VWD rule; proper
/// per-record full-model culling at the boundary is a further follow-up,
/// #1866).
///
/// `max_full_cell_radius` **must** be the caller's cell-streaming
/// `radius_unload`, not `radius_load` — #1866 / LC0703-01. Full cells load at
/// `radius_load` but only unload past `radius_unload` (`radius_load + 1`,
/// the streaming hysteresis band that prevents load/unload thrash at the
/// boundary — see `streaming.rs`), so a cell at exactly `radius_load + 1`
/// can still hold a resident full REFR. Gating this ring on `radius_load`
/// let a quad covering that cell become LOD-eligible while the full model
/// was still there, producing full-model/LOD z-fighting in that one-cell
/// band. Gating on `radius_unload` instead means a quad only loads once
/// every cell it covers is provably beyond any possible full-cell residency.
///
/// No-op for Oblivion / FO3 / FNV — those ship the `DistantLOD\*.lod` +
/// `_far.nif` placement scheme, not `.bto` (EXAL §5; a separate module).
pub(crate) fn stream_object_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &ExteriorWorldContext,
    player_grid: (i32, i32),
    max_full_cell_radius: i32,
    blocks: &mut HashMap<(i32, i32), ObjectLodBlock>,
) {
    if !matches!(
        wctx.record_index.game,
        GameKind::Skyrim | GameKind::Fallout4
    ) {
        return;
    }
    let level = OBJECT_LOD_LEVEL;
    let desired: std::collections::HashSet<(i32, i32)> =
        object_lod_quads_in_radius(player_grid, max_full_cell_radius, level)
            .into_iter()
            .collect();

    let mut spawned = 0usize;
    let mut unloaded = 0usize;

    // Unload quads that left the ring (skip empty sentinels — nothing to free).
    blocks.retain(|coord, blk| {
        if desired.contains(coord) {
            true
        } else {
            if !blk.entities.is_empty() {
                unload_object_lod_block(world, ctx, blk);
                unloaded += 1;
            }
            false
        }
    });

    // Load entering quads.
    for &(qx, qy) in &desired {
        if blocks.contains_key(&(qx, qy)) {
            continue; // already loaded (or a known-missing sentinel)
        }
        match spawn_object_lod_quad(world, ctx, tex_provider, wctx, level, qx, qy) {
            Some(blk) => {
                if !blk.entities.is_empty() {
                    spawned += 1;
                }
                blocks.insert((qx, qy), blk);
            }
            None => {
                // No `.bto` for this quad — remember so we don't re-extract.
                blocks.insert((qx, qy), ObjectLodBlock::empty());
            }
        }
    }

    if spawned + unloaded > 0 {
        log::info!(
            "Object-LOD ring @cell ({},{}): +{} quads loaded, -{} unloaded ({} tracked)",
            player_grid.0,
            player_grid.1,
            spawned,
            unloaded,
            blocks.len(),
        );
    }
}

/// Resolve + import + spawn one quad's `.bto`. Returns `None` when the quad
/// has no baked `.bto` (the common case), `Some(empty)`-equivalent is handled
/// by the caller. Each imported sub-mesh becomes an [`IsLodTerrain`] entity
/// (no BLAS, lean static draw) positioned by its world-absolute import
/// transform (verified: `.bto` geometry is authored in engine-aligned world
/// coords — EXAL step 6). All sub-meshes share the worldspace object atlas.
fn spawn_object_lod_quad(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &ExteriorWorldContext,
    level: i32,
    qx: i32,
    qy: i32,
) -> Option<ObjectLodBlock> {
    let path = bto_archive_path(&wctx.worldspace_key, level, qx, qy);
    let bytes = tex_provider.extract_mesh(&path)?;
    let scene = match byroredux_nif::parse_nif(&bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Object-LOD '{}' parse failed: {}", path, e);
            return None;
        }
    };
    // Local pool — we consume only geometry + transforms, not the interned
    // texture handles (the atlas path is deterministic).
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
    if imported.meshes.is_empty() {
        return None;
    }

    ctx.allocator.as_ref()?;

    // Shared object atlas for the worldspace (`<world>.objects.dds`). `0` /
    // fallback → the LOD draws untextured-grey, still better than no distant
    // objects. Resolved once and reused across the quad's sub-meshes.
    let w = wctx.worldspace_key.to_ascii_lowercase();
    let atlas_path = format!("textures\\terrain\\{w}\\objects\\{w}.objects.dds");
    let atlas = resolve_texture(ctx, tex_provider, Some(atlas_path.as_str()));
    let atlas = if atlas == ctx.texture_registry.fallback() {
        0
    } else {
        atlas
    };

    let mut entities = Vec::new();
    let mut mesh_handles = Vec::new();

    for mesh in &imported.meshes {
        if mesh.positions.is_empty() || mesh.indices.is_empty() {
            continue;
        }
        let verts: Vec<Vertex> = (0..mesh.positions.len())
            .map(|i| {
                let color = mesh.colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]);
                let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
                let mut v = Vertex::new_rgba(mesh.positions[i], color, normal, uv);
                if let Some(t) = mesh.tangents.get(i) {
                    v.tangent = *t;
                }
                v
            })
            .collect();

        // Lean global-only upload (no per-mesh buffers / no BLAS) — same path
        // terrain LOD uses: LOD geometry rasterizes from the global SSBO and
        // never enters the TLAS.
        let handle = match ctx
            .mesh_registry
            .upload_scene_mesh_global_only(&verts, &mesh.indices)
        {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Object-LOD '{}' mesh upload failed: {}", path, e);
                continue;
            }
        };

        let pos = Vec3::from_array(mesh.translation);
        let rot = Quat::from_xyzw(
            mesh.rotation[0],
            mesh.rotation[1],
            mesh.rotation[2],
            mesh.rotation[3],
        );
        let scale = mesh.scale;

        // World-space bound from the local AABB through the transform (rotation
        // preserves length, so the radius just scales).
        let mut lmin = Vec3::splat(f32::INFINITY);
        let mut lmax = Vec3::splat(f32::NEG_INFINITY);
        for p in &mesh.positions {
            let v = Vec3::from_array(*p);
            lmin = lmin.min(v);
            lmax = lmax.max(v);
        }
        let lc = (lmin + lmax) * 0.5;
        let lr = (lmax - lc).length();
        let bound = WorldBound::new(pos + rot * (lc * scale), lr * scale);

        let entity = world.spawn();
        world.insert(entity, Transform::new(pos, rot, scale));
        world.insert(entity, GlobalTransform::new(pos, rot, scale));
        world.insert(entity, MeshHandle(handle));
        if atlas != 0 {
            world.insert(entity, TextureHandle(atlas));
        }
        world.insert(entity, bound);
        world.insert(entity, RenderLayer::Architecture);
        // No BLAS, lean static draw, kept out of the TLAS (shared with terrain
        // LOD). The active full-model VWD cull is deferred; quads load only
        // outside the full-detail ring, so no resident full model conflicts
        // here. The per-record VWD signal is now materialised as the
        // `VisibleWhenDistant` marker at spawn (#1889) — the hook that cull
        // would read once the full-detail radius is decoupled from the ring.
        world.insert(entity, IsLodTerrain);

        entities.push(entity);
        mesh_handles.push(handle);
    }

    if entities.is_empty() {
        return None;
    }
    Some(ObjectLodBlock {
        entities,
        mesh_handles,
        texture_handle: atlas,
    })
}

/// Free one object-LOD quad: drop each sub-mesh's global-SSBO range and
/// despawn its entity (mirrors [`super::terrain_lod::unload_lod_block`]).
pub(crate) fn unload_object_lod_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    block: &ObjectLodBlock,
) {
    for &h in &block.mesh_handles {
        ctx.mesh_registry.drop_mesh(h);
    }
    // #1537 — release the shared atlas refcount once (acquired once per quad
    // at spawn). Skip `0`/fallback. Mirrors the terrain-LOD reclaim.
    if block.texture_handle != 0 {
        ctx.texture_registry
            .drop_texture(&ctx.device, block.texture_handle);
    }
    for &e in &block.entities {
        world.despawn(e);
    }
}

// Canonical quad levels (cells per quad edge), Skyrim+: 4 = closest/highest
// detail (4×4 cells), then 8, 16, 32 (lowest; level 32 also makes the world
// map). Matches `LODSettings\<World>.lod`'s level-min 4 / level-max 32 (EXAL
// Q2). The first cut loads only level 4 ([`OBJECT_LOD_LEVEL`]); coarser bands
// are a follow-up. `quad_origin` works for any of them.

/// SW-corner cell of the level-`level` quad containing cell `(gx, gy)`.
///
/// Quad SW coords are integer multiples of `level` (verified against real
/// filenames: `tamriel.4.88.8`, `tamriel.8.-72.-8`, `tamriel.16.0.0`). Uses
/// Euclidean floor so quads tile consistently across the worldspace origin
/// (the same `div_euclid` convention `terrain_lod` blocks use).
pub(crate) fn quad_origin(gx: i32, gy: i32, level: i32) -> (i32, i32) {
    (gx.div_euclid(level) * level, gy.div_euclid(level) * level)
}

/// Archive-relative path of the object-LOD `.bto` for a worldspace quad:
/// `meshes\terrain\<world>\objects\<world>.<level>.<x>.<y>.bto`.
///
/// **Level-first** naming with the quad's SW-corner cell `(qx, qy)` — the
/// ordering EXAL Q2 corrected (it is NOT `<x>.<y>.<level>`). The worldspace
/// folder + filename stem are the EDID lowercased (the cell loader's
/// `worldspace_key` is already lowercase). Backslash separators match the
/// BSA's internal path convention (see `terrain_lod`'s texture lookups).
pub(crate) fn bto_archive_path(worldspace_key: &str, level: i32, qx: i32, qy: i32) -> String {
    let w = worldspace_key.to_ascii_lowercase();
    format!("meshes\\terrain\\{w}\\objects\\{w}.{level}.{qx}.{qy}.bto")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1866 / LC0703-01 — a quad whose nearest cell sits exactly at the
    /// streaming hysteresis boundary (`radius_load + 1 == radius_unload`)
    /// must NOT be desired when gated on `radius_unload`, even though the
    /// pre-fix code (gating on `radius_load`) would have included it —
    /// that one-cell band is exactly where a full REFR can still be
    /// resident (unload only fires past `radius_unload`), so loading LOD
    /// there would z-fight it.
    #[test]
    fn ring_excludes_hysteresis_band_when_gated_on_radius_unload() {
        let radius_load = 5;
        let radius_unload = radius_load + 1; // streaming.rs's hysteresis rule
        let level = 1; // 1×1 quads so "nearest cell distance" is exact
        let player = (0, 0);

        // Buggy pre-fix behaviour: gating on radius_load includes the
        // hysteresis-band cell (distance == radius_unload == 6).
        let buggy = object_lod_quads_in_radius(player, radius_load, level);
        assert!(
            buggy.contains(&(6, 0)),
            "sanity: radius_load gating must reproduce the pre-fix bug"
        );

        // Fixed behaviour: gating on radius_unload excludes it.
        let fixed = object_lod_quads_in_radius(player, radius_unload, level);
        assert!(
            !fixed.contains(&(6, 0)),
            "a cell at exactly radius_load+1 can still hold a resident full \
             REFR under the load/unload hysteresis band — LOD must not load there"
        );
        // A cell safely beyond the hysteresis band still loads.
        assert!(fixed.contains(&(7, 0)));
    }

    #[test]
    fn quad_origin_snaps_to_level_multiples() {
        // Positive: cell (89, 9) at level 4 → SW corner (88, 8) — the quad
        // `tamriel.4.88.8.bto` covers cells [88,92)×[8,12).
        assert_eq!(quad_origin(89, 9, 4), (88, 8));
        assert_eq!(quad_origin(88, 8, 4), (88, 8)); // corner maps to itself
        assert_eq!(quad_origin(91, 11, 4), (88, 8)); // last cell in the quad
                                                     // Negative: Euclidean floor — cell (-5, -13) at level 4 → (-8, -16),
                                                     // the quad `tamriel.4.-8.-16.bto` covers [-8,-4)×[-16,-12).
        assert_eq!(quad_origin(-5, -13, 4), (-8, -16));
        assert_eq!(quad_origin(-8, -16, 4), (-8, -16));
        // Coarser levels snap to their own multiples.
        assert_eq!(quad_origin(-70, -3, 8), (-72, -8)); // → tamriel.8.-72.-8
        assert_eq!(quad_origin(5, 5, 16), (0, 0)); // → tamriel.16.0.0
        assert_eq!(quad_origin(33, -1, 32), (32, -32));
    }

    #[test]
    fn bto_path_matches_vanilla_skyrim_filenames() {
        // These four paths were extracted verbatim from vanilla
        // Skyrim - Meshes1.bsa (2026-06-02) and parsed OK by the NIF pipeline.
        assert_eq!(
            bto_archive_path("Tamriel", 4, 88, 8),
            "meshes\\terrain\\tamriel\\objects\\tamriel.4.88.8.bto"
        );
        assert_eq!(
            bto_archive_path("tamriel", 4, -8, -16),
            "meshes\\terrain\\tamriel\\objects\\tamriel.4.-8.-16.bto"
        );
        assert_eq!(
            bto_archive_path("Tamriel", 16, 0, 0),
            "meshes\\terrain\\tamriel\\objects\\tamriel.16.0.0.bto"
        );
        assert_eq!(
            bto_archive_path("DLC2SolstheimWorld", 8, 0, 8),
            "meshes\\terrain\\dlc2solstheimworld\\objects\\dlc2solstheimworld.8.0.8.bto"
        );
    }

    #[test]
    fn object_lod_level_is_closest_band() {
        assert_eq!(OBJECT_LOD_LEVEL, 4);
    }
}
