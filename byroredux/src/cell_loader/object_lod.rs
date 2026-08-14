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
//! - **Oblivion**: a different scheme entirely — per-cell `DistantLOD\*.lod`
//!   placement lists instancing `_far.nif` meshes, handled by the sibling
//!   [`super::placement_lod`] module (#1726). **FO3/FNV do not share this**:
//!   `placement_lod_supported` gates on `GameKind::Oblivion` only (#2086) —
//!   FO3/FNV ship zero vanilla `distantlod\*.lod` files, so for those two
//!   titles this is a documented no-op, not "handled". FO3/FNV fold landmark
//!   LOD into the terrain-LOD block tree instead; there is currently no
//!   distant-object scheme wired for them.
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
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::IsLodTerrain;

use super::exterior::ExteriorWorldContext;
use super::lod_bands::{self, quad_min_chebyshev, LodBandLadder, LodBandSelection};
use super::lod_support::{worldspace_cell_bounds, LodReconcileInput, LodWorkBudget};

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

/// Stream the distant **object** LOD bands around the player (Skyrim+/FO4).
/// Mirrors [`super::terrain_lod::stream_lod_blocks`]: quads entering the ring
/// load their `.bto`, quads leaving unload. A quad loads only when it is
/// **entirely outside** `max_full_cell_radius`, so the baked LOD never
/// overlaps a resident full model (the runtime half of the VWD rule; proper
/// per-record full-model culling at the boundary is a further follow-up,
/// #1866).
///
/// Which **level** each quad draws at comes from [`super::lod_bands`]: a
/// quadtree descent over the game's own `[TerrainManager]` band distances,
/// keyed `(level, qx, qy)`. Pre-#2371 this streamed a single hardcoded
/// level-4 ring 16 cells deep; it now spans 4/8/16 (Skyrim) or 4/8/16/32
/// (FO4) out to the vanilla `fBlockMaximumDistance`, with band-switch
/// hysteresis. Because the descent partitions the ring, two levels can never
/// both claim the same ground.
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
/// No-op outside Skyrim / FO4. Oblivion uses the `DistantLOD\*.lod` +
/// `_far.nif` placement scheme; FO3/FNV ship neither standalone object-LOD
/// scheme (EXAL §5).
/// Reclaims are immediate; entering quads consume [`LodWorkBudget`] units.
/// Returns `true` when every desired quad is resident or represented by its
/// known-missing sentinel.
pub(crate) fn stream_object_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    blocks: &mut HashMap<(i32, i32, i32), ObjectLodBlock>,
    budget: &mut LodWorkBudget,
) -> bool {
    let tex_provider = input.tex_provider;
    let wctx = input.wctx;
    let player_grid = input.player_grid;
    let Some(ladder) = LodBandLadder::for_game(wctx.record_index.game) else {
        return true;
    };

    let selection = LodBandSelection {
        ladder: &ladder,
        player: player_grid,
        grid_origin: input.lod_grid_origin,
        exclude_within: input.max_full_cell_radius,
        world_bounds: worldspace_cell_bounds(wctx),
    };
    let mut desired = lod_bands::select_lod_quads(
        &selection,
        |level, qx, qy| blocks.contains_key(&(level, qx, qy)),
        |level, qx, qy| {
            tex_provider.has_mesh(&bto_archive_path(&wctx.worldspace_key, level, qx, qy))
        },
    );
    // Closest-first, so a budgeted reconcile fills the near bands before the
    // far ones. Ties break on level so a quad's own band stays grouped.
    desired.sort_unstable_by_key(|&(level, qx, qy)| {
        (
            quad_min_chebyshev(qx, qy, level, player_grid),
            level,
            qy,
            qx,
        )
    });
    let desired_set: std::collections::HashSet<_> = desired.iter().copied().collect();

    let mut spawned = 0usize;
    let mut unloaded = 0usize;

    // Unload quads that left the ring or changed band (skip empty sentinels —
    // nothing to free). A band switch shows up here as the old
    // `(level, qx, qy)` key vanishing from `desired_set`, so the coarser or
    // finer replacement can never double-draw the ground it covers.
    blocks.retain(|coord, blk| {
        if desired_set.contains(coord) {
            true
        } else {
            if !blk.entities.is_empty() {
                unload_object_lod_block(world, ctx, blk);
                unloaded += 1;
            }
            false
        }
    });

    let candidates: Vec<_> = desired
        .into_iter()
        .filter(|coord| !blocks.contains_key(coord))
        .collect();
    let candidate_count = candidates.len();
    let mut attempted = 0usize;

    // Load entering quads.
    for (level, qx, qy) in candidates {
        if !budget.try_take() {
            break;
        }
        attempted += 1;
        match spawn_object_lod_quad(world, ctx, tex_provider, wctx, level, qx, qy) {
            Some(blk) => {
                if !blk.entities.is_empty() {
                    spawned += 1;
                }
                blocks.insert((level, qx, qy), blk);
            }
            None => {
                // No `.bto` for this quad — remember so we don't re-extract.
                blocks.insert((level, qx, qy), ObjectLodBlock::empty());
            }
        }
    }

    let complete = attempted == candidate_count;
    if complete && spawned + unloaded > 0 {
        log::info!(
            "Object-LOD bands @cell ({},{}): +{} quads loaded, -{} unloaded \
             ({} tracked, levels 4..={}, {} cells deep)",
            player_grid.0,
            player_grid.1,
            spawned,
            unloaded,
            blocks.len(),
            ladder.coarsest_level(),
            ladder.max_cells(),
        );
    }

    complete
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
        let verts = super::lod_support::imported_mesh_to_vertices(mesh);

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
        let (lc, lr) = super::lod_support::local_aabb_center_radius(&mesh.positions);
        let bound = WorldBound::new(pos + rot * (lc * scale), lr * scale);

        let entity = world.spawn();
        world.insert(entity, Transform::new(pos, rot, scale));
        world.insert(entity, GlobalTransform::new(pos, rot, scale));
        world.insert(entity, MeshHandle(handle));
        if atlas != 0 {
            world.insert(entity, TextureHandle(atlas));
        }
        world.insert(entity, bound);
        // #2444 (MAT-D3-02) — imposters are drawn surfaces and need a
        // canonical `Material` like everything else. There is no per-object
        // source record to carry through (a quad bakes many statics into one
        // atlas-sampling mesh), so the honest classifier input is the atlas
        // this draw actually samples. That lands on the same matte default
        // the full architecture models resolve to, which is what closes the
        // shading pop the pre-fix hardcoded 0.5 stacked on top of the
        // geometric LOD pop.
        world.insert(
            entity,
            crate::material_translate::translate_texture_only_material(if atlas != 0 {
                Some(atlas_path.clone())
            } else {
                None
            }),
        );
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
        if let Some(accel) = ctx.accel_manager.as_mut() {
            accel.drop_blas(h);
        }
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
// Q2). All of them are streamed since #2371 — see [`super::lod_bands`] for
// the per-game ladder that picks between them. `lod_support::quad_origin`
// works for any level and keeps the worldspace-relative grid shared with
// terrain LOD (#2586).

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
    use crate::cell_loader::lod_support::quad_origin;
    use byroredux_plugin::esm::reader::GameKind;

    fn quads_for(player: (i32, i32), exclude_within: i32) -> Vec<(i32, i32, i32)> {
        let ladder = LodBandLadder::for_game(GameKind::Fallout4).unwrap();
        let selection = LodBandSelection {
            ladder: &ladder,
            player,
            grid_origin: (0, 0),
            exclude_within,
            world_bounds: None,
        };
        lod_bands::select_lod_quads(&selection, |_, _, _| false, |_, _, _| true)
    }

    /// #1866 / LC0703-01 — no quad may reach inside `radius_unload`, the
    /// streaming hysteresis boundary. Full cells load at `radius_load` but
    /// only unload past `radius_load + 1`, so that one-cell band can still
    /// hold a resident full REFR; baked LOD drawn there would z-fight it.
    /// Now enforced for every band, not just the old level-4 ring.
    #[test]
    fn no_band_reaches_inside_the_streaming_hysteresis_boundary() {
        let radius_load = 5;
        let radius_unload = radius_load + 1; // streaming.rs's hysteresis rule
                                             // Offset from a quad corner so a level-4 quad lands at exactly
                                             // `radius_unload`: from (2,0), quad (8, 0) is 6 cells out. At an
                                             // exact corner the two gatings are indistinguishable (4-aligned
                                             // quads step 0, 4, 8… so nothing sits at 6) and the regression
                                             // would not be observable at all.
        let player = (2, 0);

        for (level, qx, qy) in quads_for(player, radius_unload) {
            let d = quad_min_chebyshev(qx, qy, level, player);
            assert!(
                d > radius_unload,
                "level-{level} quad ({qx}, {qy}) sits {d} cells out — inside the \
                 load/unload hysteresis band, where a full REFR can still be resident"
            );
        }

        // Sanity: the guard actually binds. Gating on `radius_load` instead
        // admits a quad sitting exactly in the hysteresis band — the shape
        // of the pre-fix bug.
        assert!(
            quads_for(player, radius_load)
                .iter()
                .any(|&(level, qx, qy)| quad_min_chebyshev(qx, qy, level, player) == radius_unload),
            "radius_load gating must reproduce the pre-fix bug"
        );
    }

    /// #2371 — object LOD spans the full band ladder now, not one hardcoded
    /// level-4 ring 16 cells deep. FO4 authors `fBlockLevel2Distance`, so
    /// all four bands are live (and it ships level-32 `.bto` to match).
    ///
    /// Unioned over player positions within a coarse quad: a strict
    /// quadtree quantises each band's candidates to its own quad size, so
    /// any single position may skip one (see
    /// `lod_bands::every_declared_band_is_reachable_from_some_position`).
    #[test]
    fn object_lod_streams_every_band_out_to_the_vanilla_max_distance() {
        let mut levels: Vec<i32> = Vec::new();
        let mut furthest = 0;
        for py in 0..32 {
            for px in 0..32 {
                for (level, qx, qy) in quads_for((px, py), 6) {
                    levels.push(level);
                    furthest = furthest.max(quad_min_chebyshev(qx, qy, level, (px, py)));
                }
            }
        }
        levels.sort_unstable();
        levels.dedup();
        assert_eq!(levels, vec![4, 8, 16, 32]);
        assert!(
            furthest > 16,
            "bands must reach past the old 16-cell ring, got {furthest}"
        );
    }

    #[test]
    fn tamriel_quad_origin_snaps_to_level_multiples() {
        // Positive: cell (89, 9) at level 4 → SW corner (88, 8) — the quad
        // `tamriel.4.88.8.bto` covers cells [88,92)×[8,12).
        assert_eq!(quad_origin(89, 9, 4, (0, 0)), (88, 8));
        assert_eq!(quad_origin(88, 8, 4, (0, 0)), (88, 8)); // corner maps to itself
        assert_eq!(quad_origin(91, 11, 4, (0, 0)), (88, 8)); // last cell in the quad
                                                             // Negative: Euclidean floor — cell (-5, -13) at level 4 → (-8, -16),
                                                             // the quad `tamriel.4.-8.-16.bto` covers [-8,-4)×[-16,-12).
        assert_eq!(quad_origin(-5, -13, 4, (0, 0)), (-8, -16));
        assert_eq!(quad_origin(-8, -16, 4, (0, 0)), (-8, -16));
        // Coarser levels snap to their own multiples.
        assert_eq!(quad_origin(-70, -3, 8, (0, 0)), (-72, -8)); // → tamriel.8.-72.-8
        assert_eq!(quad_origin(5, 5, 16, (0, 0)), (0, 0)); // → tamriel.16.0.0
        assert_eq!(quad_origin(33, -1, 32, (0, 0)), (32, -32));
    }

    #[test]
    fn object_lod_bands_keep_nonzero_worldspace_phase() {
        let origin = (-50, -50);
        let ladder = LodBandLadder::for_game(GameKind::Skyrim).unwrap();
        let selection = LodBandSelection {
            ladder: &ladder,
            player: (-49, -49),
            grid_origin: origin,
            exclude_within: 0,
            world_bounds: None,
        };
        let quads = lod_bands::select_lod_quads(&selection, |_, _, _| false, |_, _, _| true);
        assert!(!quads.is_empty());
        assert!(quads.iter().all(|&(level, qx, qy)| {
            (qx - origin.0).rem_euclid(level) == 0 && (qy - origin.1).rem_euclid(level) == 0
        }));
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

    /// Skyrim ships no level-32 `.bto` (measured on
    /// `Skyrim - Meshes1.bsa`: 517 / 152 / 48 at levels 4 / 8 / 16, none at
    /// 32) and authors no `fBlockLevel2Distance`. The ladder must therefore
    /// never ask for a level-32 Skyrim object quad in the first place.
    #[test]
    fn skyrim_object_bands_stop_at_level_16() {
        let ladder = LodBandLadder::for_game(GameKind::Skyrim).unwrap();
        assert_eq!(ladder.coarsest_level(), 16);

        let selection = LodBandSelection {
            ladder: &ladder,
            player: (0, 0),
            grid_origin: (0, 0),
            exclude_within: 6,
            world_bounds: None,
        };
        let quads = lod_bands::select_lod_quads(&selection, |_, _, _| false, |_, _, _| true);
        assert!(quads.iter().all(|&(level, _, _)| level <= 16));
        assert!(quads.iter().any(|&(level, _, _)| level == 16));
    }
}
