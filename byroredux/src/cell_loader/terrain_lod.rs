//! Distant-terrain LOD blocks — engine-generated coarse meshes that extend
//! exterior view distance far beyond the full-detail streamed ring.
//!
//! Each LOD block merges a `LOD_BLOCK_CELLS × LOD_BLOCK_CELLS` square of
//! exterior cells into ONE decimated mesh sampled from the worldspace
//! heightmaps already parsed at load time (`ExteriorWorldContext`). A block:
//!   * samples each cell's 33×33 grid at `STRIDE` (`SAMPLES_PER_CELL` quads
//!     per cell edge) — ~1/64th the triangles of full-detail terrain,
//!   * uses an authored legacy LOD diffuse/normal quad where available, then a
//!     single base ground texture fallback (no per-vertex splat blend),
//!   * builds **no BLAS** and spawns with [`IsLodTerrain`] so the renderer
//!     keeps it out of the TLAS — distant terrain needs no RT shadows/GI,
//!     costing zero ray-tracing budget,
//!   * holes out any cell inside the full-detail load radius so the LOD
//!     never overlaps / z-fights the streamed near terrain.
//!
//! [`stream_lod_blocks`] streams the ring as the player walks: blocks
//! entering the LOD radius spawn, blocks leaving unload, and boundary
//! blocks whose hole mask changed (the full-detail region moved with the
//! player) regenerate. Blocks are tracked by block-coord on
//! `WorldStreamingState.lod_blocks`, so a worldspace re-entry / teleport
//! reclaims the prior ring instead of leaking it (#1373).

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::{
    GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::coord::{zup_to_yup_pos, EXTERIOR_CELL_UNITS};
use byroredux_core::math::Vec3;
use byroredux_nif::import::MaterialTextureSet;
use byroredux_plugin::esm::cell::CellData;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::{IsLodTerrain, MaterialTextureHandles};
use crate::env_translate::translate_terrain_lod_textures;
use crate::streaming::LodBlock;

use super::lod_bands::{self, quad_min_chebyshev, LodBandLadder, LodBandSelection};
use super::lod_support::{
    combined_lod_supported, legacy_landscape_lod_supported, worldspace_cell_bounds,
    LodReconcileInput, LodWorkBudget,
};

/// Cells per LOD-block edge. A block is one merged mesh covering
/// `LOD_BLOCK_CELLS²` cells (4×4 = 16 cells = 16384×16384 BU). Bigger
/// blocks → fewer draw entities; smaller → finer hole-out granularity at
/// the full-detail boundary. 4 keeps the block count bounded while still
/// aligning to the 4-quadrant cell layout.
pub(crate) const LOD_BLOCK_CELLS: i32 = 4;

/// LOD ring radius in blocks (Chebyshev). 12 blocks × 4 cells × 4096 BU =
/// ~196 K BU of distant terrain in every direction — ~9.6× the default
/// full-detail radius-5 view (20 480 BU). Tunable; the camera far plane
/// (`Camera::default`) is sized to cover the resulting far-corner diagonal.
pub(crate) const LOD_RADIUS_BLOCKS: i32 = 12;

/// Heightmap sample stride within a cell's 33-vertex grid. 8 → 4 quads per
/// cell edge (5 samples incl. the shared seam), 1/64th the triangles.
const STRIDE: usize = 8;
/// Quad samples contributed per cell edge (= 32 / STRIDE).
const SAMPLES_PER_CELL: usize = 32 / STRIDE;
/// World-space spacing between adjacent block-mesh vertices (BU).
const VERT_SPACING: f32 = EXTERIOR_CELL_UNITS / 32.0 * STRIDE as f32; // 1024.0

/// Map a block-vertex index `v ∈ [0, k·SAMPLES_PER_CELL]` to the
/// `(cell_offset ∈ [0, k), local ∈ [0, 32])` it samples. The block's far
/// edge index belongs to the previous cell's local-32 seam, not a phantom
/// cell `k`.
fn sample_to_cell(v: usize, k: usize) -> (usize, usize) {
    let max = k * SAMPLES_PER_CELL;
    if v == max {
        (k - 1, 32)
    } else {
        (v / SAMPLES_PER_CELL, (v % SAMPLES_PER_CELL) * STRIDE)
    }
}

/// Chebyshev (chessboard) distance between two grid coords.
fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// Relative block index containing `cell` on a worldspace-aligned LOD grid.
fn block_index(cell: (i32, i32), grid_origin: (i32, i32)) -> (i32, i32) {
    (
        (cell.0 - grid_origin.0).div_euclid(LOD_BLOCK_CELLS),
        (cell.1 - grid_origin.1).div_euclid(LOD_BLOCK_CELLS),
    )
}

/// Authored SW cell coordinate for relative block `(bi, bj)`.
fn block_cell_origin(bi: i32, bj: i32, grid_origin: (i32, i32)) -> (i32, i32) {
    (
        grid_origin.0 + bi * LOD_BLOCK_CELLS,
        grid_origin.1 + bj * LOD_BLOCK_CELLS,
    )
}

/// Whether cell `(gx, gy)` sits inside the full-detail region and should be
/// holed out of the LOD mesh (left for the streamed near terrain to cover).
///
/// `max_full_cell_radius` **must** be the caller's cell-streaming
/// `radius_unload`, not `radius_load` — #1871 / LC0703-02 (the terrain
/// sibling of #1866 / LC0703-01). Full cells load at `radius_load` but only
/// unload past `radius_unload` (`radius_load + 1`, the streaming hysteresis
/// band that prevents load/unload thrash at the boundary — see
/// `streaming.rs`), so a cell at exactly `radius_load + 1` can still hold a
/// resident full landscape. Gating this predicate on `radius_load` let the
/// LOD mesh render un-holed at that same cell, z-fighting the still-resident
/// full-detail terrain. Gating on `radius_unload` means a cell is only left
/// un-holed once it's provably beyond any possible full-cell residency.
/// Single source of truth for both [`block_hole_mask`] and
/// [`spawn_lod_block`]'s per-vertex hole test, so the two can't drift apart.
fn cell_is_full_detail(
    gx: i32,
    gy: i32,
    player_grid: (i32, i32),
    max_full_cell_radius: i32,
) -> bool {
    chebyshev((gx, gy), player_grid) <= max_full_cell_radius
}

// Compile-time footprint assertions (#1378): pin the worst-case LOD ring
// vertex + index count against the renderer's pool soft caps so that a
// parameter retune (larger RADIUS, smaller STRIDE) fails CI rather than
// surfacing as a one-shot runtime warn deep in the boot log.
//
// Worst-case ring: full Chebyshev square of (2·LOD_RADIUS_BLOCKS+1)²
// blocks, each with (LOD_BLOCK_CELLS·SAMPLES_PER_CELL+1)² vertices and
// (LOD_BLOCK_CELLS·SAMPLES_PER_CELL)² × 2 triangles = ×6 indices.
//
// Current values: 625 blocks × 289 verts = 180 625 verts (~4.5% soft cap)
//                 625 blocks × 1536 idx  = 960 000 idx  (~6.0% soft cap)
const LOD_RING_VERTEX_FOOTPRINT: usize = {
    let blocks_per_side = 2 * LOD_RADIUS_BLOCKS as usize + 1;
    let verts_per_edge = LOD_BLOCK_CELLS as usize * SAMPLES_PER_CELL + 1;
    blocks_per_side * blocks_per_side * verts_per_edge * verts_per_edge
};
const LOD_RING_INDEX_FOOTPRINT: usize = {
    let blocks_per_side = 2 * LOD_RADIUS_BLOCKS as usize + 1;
    let quads_per_edge = LOD_BLOCK_CELLS as usize * SAMPLES_PER_CELL;
    blocks_per_side * blocks_per_side * quads_per_edge * quads_per_edge * 6
};
const _: () = assert!(
    LOD_RING_VERTEX_FOOTPRINT < byroredux_renderer::mesh::VERTEX_POOL_SOFT_CAP,
    "LOD ring vertex footprint >= VERTEX_POOL_SOFT_CAP — shrink LOD_RADIUS_BLOCKS or increase STRIDE"
);
const _: () = assert!(
    LOD_RING_INDEX_FOOTPRINT < byroredux_renderer::mesh::INDEX_POOL_SOFT_CAP,
    "LOD ring index footprint >= INDEX_POOL_SOFT_CAP — shrink LOD_RADIUS_BLOCKS or increase STRIDE"
);

// The hole mask is a `u16`, one bit per cell, so a block holds at most 16
// cells. LOD_BLOCK_CELLS = 4 → 4×4 = 16. A bigger block would silently
// truncate the mask (and `all_holed_mask`'s shift).
const _: () = assert!(
    (LOD_BLOCK_CELLS * LOD_BLOCK_CELLS) <= 16,
    "LOD hole mask is u16 — LOD_BLOCK_CELLS squared must be <= 16",
);

/// OR a hole bit for every cell `(bx0+dx, by0+dy)` in the finest-level block
/// whose SW corner is `(bx0, by0)`, for which `holed(gx, gy)` is true. Pure
/// (no `CellData` map) so the player-tracking regen logic is unit-testable in
/// isolation.
fn assemble_hole_mask(bx0: i32, by0: i32, holed: impl Fn(i32, i32) -> bool) -> u16 {
    let k = LOD_BLOCK_CELLS;
    let mut mask: u16 = 0;
    for dy in 0..k {
        for dx in 0..k {
            if holed(bx0 + dx, by0 + dy) {
                mask |= 1u16 << (dy * k + dx);
            }
        }
    }
    mask
}

/// Mask with every cell holed — an empty block (no geometry to draw).
fn all_holed_mask() -> u16 {
    ((1u32 << (LOD_BLOCK_CELLS * LOD_BLOCK_CELLS)) - 1) as u16
}

/// The 16-bit per-cell hole pattern for the finest-level block at SW cell
/// `(bx0, by0)`, given the player's grid cell and full-detail radius. A cell
/// is holed when it's inside the full-detail radius (streamed near terrain
/// renders it) or has no landscape. Stored on [`LodBlock`] so a boundary
/// block is regenerated only when its pattern actually changes.
///
/// Only the finest band needs this. Coarser bands are selected only far
/// beyond `max_full_cell_radius` (their refine thresholds start at 15 cells
/// against a ~6-cell streaming radius) and are drawn from prebaked `.btr`
/// meshes that already bake in the game's own coverage, so their mask is
/// always 0 — see [`stream_lod_blocks`].
///
/// `max_full_cell_radius` **must** be the caller's `radius_unload` — see
/// [`cell_is_full_detail`] (#1871 / LC0703-02).
fn block_hole_mask(
    cells_map: &HashMap<(i32, i32), CellData>,
    bx0: i32,
    by0: i32,
    player_grid: (i32, i32),
    max_full_cell_radius: i32,
    resident_full_cells: &[(i32, i32)],
) -> u16 {
    assemble_hole_mask(bx0, by0, |gx, gy| {
        cell_is_full_detail(gx, gy, player_grid, max_full_cell_radius)
            || resident_full_cells.contains(&(gx, gy))
            || cells_map
                .get(&(gx, gy))
                .and_then(|cell| cell.landscape.as_ref())
                .is_none()
    })
}

/// Worst case of [`lod_ring_reach_cells`] over every supported game — the
/// widest distant-LOD ring the engine can ever stream.
///
/// Pinned as a plain constant so the camera far plane can be checked against
/// it at compile time ([`cells_from_bu`] is float math and cannot run in a
/// `const`). `max_ring_reach_covers_every_game` proves it stays both correct
/// and tight.
pub(crate) const MAX_LOD_RING_REACH_CELLS: i32 = 61;

// #2371 / EX-11 — the camera far plane must clear the ring's far-corner
// diagonal (`reach · √2`). The frustum's far plane is extracted from the
// view-projection matrix, so geometry past it is *culled*, not merely
// clipped: an under-sized far plane makes whole LOD blocks wink out along
// the view diagonals. Squared to keep the check in exact integer math.
//
// This fires if either side is retuned — a deeper ring or a shorter far
// plane — instead of silently truncating the horizon, which is exactly how
// the 61-cell baked ladder came to overshoot the previous 300 000 far plane.
const _: () = {
    let reach_bu = MAX_LOD_RING_REACH_CELLS as i64 * EXTERIOR_CELL_UNITS as i64;
    let far = byroredux_core::ecs::components::DEFAULT_RENDER_DISTANCE as i64;
    assert!(
        far * far >= 2 * reach_bu * reach_bu,
        "camera far plane no longer covers the LOD ring's far-corner diagonal — \
         raise DEFAULT_RENDER_DISTANCE or shrink MAX_LOD_RING_REACH_CELLS"
    );
};

/// Outer reach of the distant-terrain LOD ring for `game`, in cells.
///
/// Single source of truth for "how far does distant terrain go", so anything
/// sized to match it cannot drift. Games with a baked quadtree reach their
/// own `fBlockMaximumDistance` (61 cells at the vanilla Ultra tier); the rest
/// keep the synthesized ring's `LOD_RADIUS_BLOCKS × LOD_BLOCK_CELLS`.
pub(crate) fn lod_ring_reach_cells(game: GameKind) -> i32 {
    LodBandLadder::for_game(game)
        .map(|ladder| ladder.max_cells())
        .unwrap_or(LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS)
}

/// The `(level, qx, qy)` quads the terrain ring wants resident this
/// reconcile, closest-first so a budgeted initialization grows outward
/// deterministically instead of following HashMap iteration order.
///
/// Two regimes, by whether the game uses the combined `.btr` quadtree (#2371):
///
/// * **Skyrim / FO4** descend [`super::lod_bands`]' 4/8/16/32 ladder. A
///   coarse band is only offered where the game actually baked a `.btr`;
///   where it did not, the descent subdivides instead of leaving a hole,
///   and the finest band always reports available because heightmap synth
///   can cover any footprint.
/// * **Oblivion / FO3 / FNV** ship older NIF/DDS terrain quadtrees, not `.btr`.
///   Their current geometry provider remains the single synthesized
///   [`LOD_RADIUS_BLOCKS`]-deep ring at the finest level, with EXAL supplying
///   its authored level-4 texture quads (#3100). `has_btr` is never consulted.
#[allow(clippy::too_many_arguments)]
fn desired_lod_quads(
    ladder: Option<&LodBandLadder>,
    player_grid: (i32, i32),
    grid_origin: (i32, i32),
    max_full_cell_radius: i32,
    world_bounds: Option<((i32, i32), (i32, i32))>,
    resident: impl Fn(i32, i32, i32) -> bool,
    has_btr: impl Fn(i32, i32, i32) -> bool,
) -> Vec<(i32, i32, i32)> {
    let k = LOD_BLOCK_CELLS;
    let mut desired = match ladder {
        Some(ladder) => {
            let selection = LodBandSelection {
                ladder,
                player: player_grid,
                grid_origin,
                exclude_within: max_full_cell_radius,
                world_bounds,
            };
            lod_bands::select_lod_quads(&selection, resident, |level, qx, qy| {
                level == k || has_btr(level, qx, qy)
            })
        }
        None => {
            let (pbi, pbj) = block_index(player_grid, grid_origin);
            let mut coords = Vec::new();
            for bj in (pbj - LOD_RADIUS_BLOCKS)..=(pbj + LOD_RADIUS_BLOCKS) {
                for bi in (pbi - LOD_RADIUS_BLOCKS)..=(pbi + LOD_RADIUS_BLOCKS) {
                    let (bx0, by0) = block_cell_origin(bi, bj, grid_origin);
                    coords.push((k, bx0, by0));
                }
            }
            coords
        }
    };
    desired.sort_unstable_by_key(|&(level, qx, qy)| {
        (
            quad_min_chebyshev(qx, qy, level, player_grid),
            level,
            qy,
            qx,
        )
    });
    desired
}

/// Stream the distant-terrain LOD bands around the player's grid cell
/// `player_grid` (#1373). Reconciles the resident `lod_blocks` against
/// [`desired_lod_quads`]:
///   * quads entering the ring are spawned,
///   * quads leaving — or switching band — are unloaded (`drop_mesh` +
///     despawn); a band switch is exactly an old `(level, …)` key dropping
///     out of the desired set, which is what stops two levels double-drawing
///     the same ground,
///   * finest-band boundary blocks whose hole mask changed (the full-detail
///     region moved with the player) are regenerated so the LOD never
///     overlaps or gaps against the streamed near terrain.
///
/// Cells inside `max_full_cell_radius` are holed out. Reclaims and stale-mask
/// invalidations happen immediately; entering/regenerated blocks consume
/// [`LodWorkBudget`] units and may be deferred. Returns `true` once every
/// desired coordinate is either resident or recorded in `missing_blocks`.
///
/// `max_full_cell_radius` **must** be the caller's cell-streaming
/// `radius_unload`, not `radius_load` — see [`cell_is_full_detail`]
/// (#1871 / LC0703-02, the terrain sibling of #1866 / LC0703-01).
pub(crate) fn stream_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    lod_blocks: &mut HashMap<(i32, i32, i32), LodBlock>,
    missing_blocks: &mut HashMap<(i32, i32, i32), u16>,
    budget: &mut LodWorkBudget,
) -> bool {
    let tex_provider = input.tex_provider;
    let wctx = input.wctx;
    let player_grid = input.player_grid;
    let max_full_cell_radius = input.max_full_cell_radius;
    let index = &wctx.record_index.cells;
    let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
        return true;
    };
    // Combined `.btr` distant terrain is a Skyrim/FO4 scheme. Legacy games
    // keep the heightmap-synth geometry, but EXAL can now supply their
    // authored NIF-era diffuse quads to that common path (#3100).
    let game = wctx.record_index.game;
    let worldspace_key = wctx.worldspace_key.as_str();
    // Worldspace form id — keys the baked `landscapelod\generated` distant-LOD
    // textures (Tamriel = 60). Small worlds (AnvilWorld) ship none, so their
    // synth blocks resolve no texture and are suppressed (#1745).
    let world_form_id = index
        .worldspaces
        .get(worldspace_key)
        .map(|w| w.form_id)
        .unwrap_or(0);
    let k = LOD_BLOCK_CELLS;
    let grid_origin = input.lod_grid_origin;
    let ladder = LodBandLadder::for_game(game);

    let desired = desired_lod_quads(
        ladder.as_ref(),
        player_grid,
        grid_origin,
        max_full_cell_radius,
        worldspace_cell_bounds(wctx),
        |level, qx, qy| lod_blocks.contains_key(&(level, qx, qy)),
        |level, qx, qy| {
            tex_provider.has_mesh(&super::terrain_lod_btr::btr_archive_path(
                worldspace_key,
                level,
                qx,
                qy,
            ))
        },
    );
    let desired_set: HashSet<_> = desired.iter().copied().collect();

    let mut spawned = 0usize;
    let mut btr_spawned = 0usize;
    let mut invalidated = 0usize;
    let mut unloaded = 0usize;

    // Unload blocks that left the ring or changed band.
    lod_blocks.retain(|coord, block| {
        if desired_set.contains(coord) {
            true
        } else {
            unload_lod_block(world, ctx, block);
            unloaded += 1;
            false
        }
    });
    missing_blocks.retain(|coord, _| desired_set.contains(coord));

    let all_holed = all_holed_mask();
    let mut candidates = Vec::new();

    // First invalidate every stale block, regardless of the spawn budget.
    // This prevents old hole masks from overlapping full-detail terrain while
    // replacement geometry waits for an idle frame.
    for &(level, qx, qy) in &desired {
        // Only the finest band can touch the full-detail region or need
        // per-cell holes; coarse bands are baked meshes far outside it.
        let mask = if level == k {
            block_hole_mask(
                cells_map,
                qx,
                qy,
                player_grid,
                max_full_cell_radius,
                input.resident_full_cells,
            )
        } else {
            0
        };
        // Copy the prior mask out before mutating the map (no borrow held).
        let prev_mask = lod_blocks.get(&(level, qx, qy)).map(|b| b.hole_mask);

        // Empty block (every cell holed) — ensure no stale entry remains.
        if mask == all_holed {
            if let Some(block) = lod_blocks.remove(&(level, qx, qy)) {
                unload_lod_block(world, ctx, &block);
                unloaded += 1;
            }
            missing_blocks.remove(&(level, qx, qy));
            continue;
        }

        match prev_mask {
            Some(m) if m == mask => {
                missing_blocks.remove(&(level, qx, qy));
                continue;
            }
            Some(_) => {
                // Hole pattern moved — drop the stale block now; its
                // replacement may be deferred below.
                if let Some(block) = lod_blocks.remove(&(level, qx, qy)) {
                    unload_lod_block(world, ctx, &block);
                }
                invalidated += 1;
            }
            None => {} // new block
        }

        if missing_blocks.get(&(level, qx, qy)) == Some(&mask) {
            continue;
        }
        missing_blocks.remove(&(level, qx, qy));
        candidates.push((level, qx, qy, mask));
    }

    let candidate_count = candidates.len();
    let mut attempted = 0usize;
    for (level, qx, qy, mask) in candidates {
        if !budget.try_take() {
            break;
        }
        attempted += 1;

        // Prefer the game's prebaked `.btr` distant-terrain mesh for a
        // fully-distant block (`mask == 0` → every cell has landscape and
        // none lie inside the full-detail ring): it carries per-quad
        // texturing instead of the synth path's single flat base texture.
        // Boundary blocks (`mask != 0`) need per-cell hole-masking a baked
        // mesh can't do, and missing `.btr` / older games fall through — the
        // synth path handles all of those. Exactly one block per coordinate,
        // so the textured LOD never double-draws the synth LOD (EXAL §5).
        //
        // Coarse bands (`level > k`) exist *only* as `.btr`: the descent
        // above only proposes them where one is baked, and the synth builder
        // is finest-band-only (its hole mask is a `u16`, one bit per cell).
        let btr_block = if mask == 0 && combined_lod_supported(game) {
            super::terrain_lod_btr::spawn_btr_block(
                world,
                ctx,
                tex_provider,
                worldspace_key,
                level,
                qx,
                qy,
                mask,
            )
        } else {
            None
        };
        let from_btr = btr_block.is_some();
        let block = btr_block.or_else(|| {
            if level != k {
                // A coarse band whose `.btr` is present in the archive but
                // unusable (parse or upload failure — `spawn_btr_block`
                // warns). The synth builder cannot cover this footprint: its
                // hole mask is a `u16`, one bit per cell, so it only handles
                // the finest band. The quad is recorded in `missing_blocks`
                // below and stays absent for as long as the player keeps it
                // in range — a hole in the horizon rather than a retry loop.
                // Availability is judged on the file existing, so re-
                // descending into finer children would need failure memory
                // that outlives the desired set; not worth the thrash risk
                // for a hard asset failure this rare.
                return None;
            }
            spawn_lod_block(
                world,
                ctx,
                tex_provider,
                &index.landscape_textures,
                cells_map,
                qx,
                qy,
                player_grid,
                max_full_cell_radius,
                input.resident_full_cells,
                game,
                worldspace_key,
                world_form_id,
            )
        });
        if let Some(block) = block {
            lod_blocks.insert((level, qx, qy), block);
            missing_blocks.remove(&(level, qx, qy));
            spawned += 1;
            if from_btr {
                btr_spawned += 1;
            }
        } else {
            // Remember a real miss at this exact hole mask so a budgeted
            // reconcile can continue outward instead of retrying forever.
            missing_blocks.insert((level, qx, qy), mask);
        }
    }

    let complete = attempted == candidate_count;
    if complete && spawned + invalidated + unloaded > 0 {
        let reach_cells = lod_ring_reach_cells(game);
        log::info!(
            "LOD ring @cell ({},{}): +{} spawned ({} prebaked .btr / {} synth), \
             ~{} stale invalidated, -{} unloaded ({} resident, ~{:.0}K BU)",
            player_grid.0,
            player_grid.1,
            spawned,
            btr_spawned,
            spawned - btr_spawned,
            invalidated,
            unloaded,
            lod_blocks.len(),
            reach_cells as f32 * EXTERIOR_CELL_UNITS / 1000.0,
        );
    }

    complete
}

/// Tear down one streamed LOD block (#1373): retire its optional nearby-shadow
/// BLAS, free its global-SSBO geometry, and despawn its entity. `drop_mesh`
/// marks the SSBO dirty so the next `rebuild_geometry_ssbo` compacts the dead
/// range out. LOD blocks have no `CellRoot`, so this is their only reclaim path
/// (mirrors the scene-mesh half of `cell_loader::unload_cell`).
pub(crate) fn unload_lod_block(world: &mut World, ctx: &mut VulkanContext, block: &LodBlock) {
    if let Some(accel) = ctx.accel_manager.as_mut() {
        accel.drop_blas(block.mesh_handle);
    }
    ctx.mesh_registry.drop_mesh(block.mesh_handle);
    // #1537 — release the base ground texture refcount. `World::despawn`
    // has no GPU side effects, so without this the `resolve_texture` bump
    // from spawn never reaches 0 and the VkImage + bindless slot pin for the
    // session (one leak per boundary-block regen during normal play). Skip
    // the fallback/placeholder slot (0), as `collect_victim_gpu_handles` does.
    if block.texture_handle != 0 {
        ctx.texture_registry
            .drop_texture(&ctx.device, block.texture_handle);
    }
    // #2371 — same reclaim for the `.btr` per-quad normal map.
    if block.normal_texture_handle != 0 {
        ctx.texture_registry
            .drop_texture(&ctx.device, block.normal_texture_handle);
    }
    world.despawn(block.entity);
}

/// Build + spawn one finest-band LOD block whose SW-corner cell is
/// `(bx0, by0)`. Returns `None`
/// (nothing spawned) when the block is entirely holes — every cell either
/// missing, landscape-less, or inside the full-detail radius — or the
/// upload fails. On success returns the [`LodBlock`] for streaming tracking.
///
/// `max_full_cell_radius` **must** be the caller's `radius_unload` — see
/// [`cell_is_full_detail`] (#1871 / LC0703-02).
#[allow(clippy::too_many_arguments)]
fn spawn_lod_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    cells_map: &HashMap<(i32, i32), CellData>,
    bx0: i32,
    by0: i32,
    player_grid: (i32, i32),
    max_full_cell_radius: i32,
    resident_full_cells: &[(i32, i32)],
    game: GameKind,
    worldspace_key: &str,
    world_form_id: u32,
) -> Option<LodBlock> {
    let k = LOD_BLOCK_CELLS;
    let origin_x = bx0 as f32 * EXTERIOR_CELL_UNITS;
    let origin_y = by0 as f32 * EXTERIOR_CELL_UNITS;

    let n = (k as usize) * SAMPLES_PER_CELL + 1; // vertices per block edge

    let mut vertices: Vec<Vertex> = Vec::with_capacity(n * n);
    // Parallel hole mask — a vertex is a hole when its source cell is
    // missing, landscape-less, or inside the full-detail radius. Quads
    // touching a hole vertex are not emitted.
    let mut holes: Vec<bool> = Vec::with_capacity(n * n);

    for r in 0..n {
        let (cdy, lrow) = sample_to_cell(r, k as usize);
        for c in 0..n {
            let (cdx, lcol) = sample_to_cell(c, k as usize);
            let gx = bx0 + cdx as i32;
            let gy = by0 + cdy as i32;

            let world_x = origin_x + c as f32 * VERT_SPACING;
            let world_y_zup = origin_y + r as f32 * VERT_SPACING;

            // Cell inside the full-detail ring → leave a hole for the
            // streamed near terrain (no overlap / z-fight).
            let full_detail = cell_is_full_detail(gx, gy, player_grid, max_full_cell_radius);
            let land = if full_detail {
                None
            } else {
                cells_map
                    .get(&(gx, gy))
                    .and_then(|cell| cell.landscape.as_ref())
            };

            let Some(land) = land else {
                // Placeholder vertex at y=0 (finite — never referenced by
                // an emitted index, but keeps the buffer free of NaN).
                vertices.push(Vertex::new(
                    zup_to_yup_pos([world_x, world_y_zup, 0.0]),
                    [1.0, 1.0, 1.0],
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0],
                ));
                holes.push(true);
                continue;
            };

            let li = lrow * 33 + lcol;
            let height = land.heights[li];

            // Normal: same Z-up→Y-up decode as the full-detail terrain
            // path (center at 128, then the canonical helper).
            let normal = if let Some(ref nml) = land.normals {
                let ni = li * 3;
                let nx = (nml[ni] as f32 - 128.0) / 127.0;
                let ny = (nml[ni + 1] as f32 - 128.0) / 127.0;
                let nz = (nml[ni + 2] as f32 - 128.0) / 127.0;
                let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
                zup_to_yup_pos([nx / len, ny / len, nz / len])
            } else {
                [0.0, 1.0, 0.0]
            };

            // Tile the diffuse `LAND_TEXTURE_TILES_PER_CELL` times per cell,
            // matching the full-detail terrain (`terrain.rs`) so the LOD seam
            // at the full-detail boundary tiles identically. `c /
            // SAMPLES_PER_CELL` is the cell-fraction along the block edge.
            let uv = [
                c as f32 / SAMPLES_PER_CELL as f32 * super::terrain::LAND_TEXTURE_TILES_PER_CELL,
                (1.0 - r as f32 / SAMPLES_PER_CELL as f32)
                    * super::terrain::LAND_TEXTURE_TILES_PER_CELL,
            ];

            vertices.push(Vertex::new(
                zup_to_yup_pos([world_x, world_y_zup, height]),
                [1.0, 1.0, 1.0],
                normal,
                uv,
            ));
            holes.push(false);
        }
    }

    // Indices: 2 triangles per non-hole quad. Same CW winding as the
    // full-detail terrain (the Z negate flips it to Vulkan-front CCW).
    let mut indices: Vec<u32> = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for r in 0..(n - 1) {
        for c in 0..(n - 1) {
            let tl = r * n + c;
            let tr = tl + 1;
            let bl = (r + 1) * n + c;
            let br = bl + 1;
            if holes[tl] || holes[tr] || holes[bl] || holes[br] {
                continue;
            }
            indices.push(tl as u32);
            indices.push(tr as u32);
            indices.push(bl as u32);
            indices.push(tr as u32);
            indices.push(br as u32);
            indices.push(bl as u32);
        }
    }

    if indices.is_empty() {
        return None; // entirely holes — nothing to draw
    }

    ctx.allocator.as_ref()?;

    // World-space bound from the emitted (non-hole) vertices for frustum
    // culling. Computed over all pushed vertices; hole placeholders sit at
    // y=0 inside the block footprint, so they never enlarge the sphere
    // beyond the real terrain extent.
    let bound = block_bound(&vertices, &holes);

    // Distant-terrain texture. Priority:
    //   1. EXAL's authored legacy diffuse quad: Oblivion's FormID-keyed
    //      32-cell DDS or FO3/FNV's EditorID-keyed level-4 DDS (#3100).
    //      Both are UV-mapped by their translated world-cell footprint.
    //   2. The block's first BTXT base LTEX, tiled like full-detail terrain
    //      (Skyrim/FO4 boundary blocks and legacy worlds whose quad is absent).
    //   3. Neither resolves → return `None`. Pre-#1745 this fell back to a
    //      hardcoded `landscape\dirt02.dds` that doesn't exist in Oblivion, so
    //      the block got the translucent magenta-checker placeholder and ghosted
    //      over full-detail content (Small World city worldspaces like AnvilWorld
    //      ship no LOD textures at all). Suppressing the block is correct: those
    //      worlds have no distant terrain.
    let translated_lod = if legacy_landscape_lod_supported(game) {
        translate_terrain_lod_textures(game, worldspace_key, world_form_id, k, bx0, by0)
    } else {
        None
    };
    let lod_quad_tex = translated_lod
        .as_ref()
        .map(|lod| resolve_texture(ctx, tex_provider, Some(lod.diffuse_path.as_str())))
        .unwrap_or(0);
    let translated_lod = translated_lod
        .filter(|_| lod_quad_tex != 0 && lod_quad_tex != ctx.texture_registry.fallback());
    let resolved_normal = translated_lod
        .as_ref()
        .map(|lod| resolve_texture(ctx, tex_provider, Some(lod.normal_path.as_str())))
        .unwrap_or(0);
    let normal_texture_handle =
        if resolved_normal != 0 && resolved_normal != ctx.texture_registry.fallback() {
            resolved_normal
        } else {
            0
        };
    // #2444 (MAT-D3-02) — the path travels with the handle: it is the
    // classifier input for this block's canonical `Material` at spawn.
    let (tex_handle, base_texture_path) = if let Some(lod) = translated_lod.as_ref() {
        (lod_quad_tex, Some(lod.diffuse_path.clone()))
    } else {
        let base_ltex = (0..k)
            .flat_map(|dy| (0..k).map(move |dx| (dx, dy)))
            .find_map(|(dx, dy)| {
                cells_map
                    .get(&(bx0 + dx, by0 + dy))
                    .and_then(|cell| cell.landscape.as_ref())
                    .and_then(|land| land.quadrants.iter().find_map(|q| q.base))
            });
        match base_ltex {
            Some(ltex_id) if ltex_id != 0 => match landscape_textures.get(&ltex_id) {
                Some(path) => (
                    resolve_texture(ctx, tex_provider, Some(path.as_str())),
                    Some(path.clone()),
                ),
                None => (0, None),
            },
            _ => (0, None),
        }
    };
    if tex_handle == 0 {
        // No real distant-terrain texture (Small World / ocean-only quad).
        return None;
    }

    // Re-map UVs for the baked quad: each vertex samples it by world position
    // over the footprint EXAL translated rather than by the per-cell tiling
    // the LTEX path uses. `world_y_zup = -position.z` is the
    // algebraic inverse of `zup_to_yup_pos`'s Z component (`z_yup = -y_zup`),
    // not a fresh derivation — no forward swizzle to replace here.
    if let Some(lod) = translated_lod.as_ref() {
        let quad_origin_x = lod.quad_origin.0 as f32 * EXTERIOR_CELL_UNITS;
        let quad_origin_y = lod.quad_origin.1 as f32 * EXTERIOR_CELL_UNITS;
        let quad_bu = lod.quad_cells as f32 * EXTERIOR_CELL_UNITS;
        for v in vertices.iter_mut() {
            let wx = v.position[0];
            let wy_zup = -v.position[2];
            v.uv = [
                (wx - quad_origin_x) / quad_bu,
                1.0 - (wy_zup - quad_origin_y) / quad_bu,
            ];
        }
    }

    // Upload into the global geometry pool only (#1370). LOD blocks
    // rasterize from the global vertex/index buffer and never enter the
    // TLAS, so the per-mesh buffers `upload_scene_mesh` would create are
    // pure boot-time waste — ~2 synchronous fence-waits + 2 tiny
    // device-local sub-allocations per block, ×hundreds of blocks. The
    // geometry rides the single `rebuild_geometry_ssbo` the frame loop
    // already runs for the resident scene.
    let mesh_handle = match ctx
        .mesh_registry
        .upload_scene_mesh_global_only(&vertices, &indices)
    {
        Ok(h) => h,
        Err(e) => {
            log::warn!(
                "Failed to upload LOD terrain block at cell ({},{}): {}",
                bx0,
                by0,
                e
            );
            // Release the texture acquired above so a failed upload doesn't
            // pin the VkImage + bindless slot (the resolve bumped its refcount).
            if tex_handle != 0 {
                ctx.texture_registry.drop_texture(&ctx.device, tex_handle);
            }
            if normal_texture_handle != 0 {
                ctx.texture_registry
                    .drop_texture(&ctx.device, normal_texture_handle);
            }
            return None;
        }
    };

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    if normal_texture_handle != 0 {
        world.insert(
            entity,
            MaterialTextureHandles {
                textures: MaterialTextureSet {
                    normal: normal_texture_handle,
                    ..Default::default()
                },
                normal_has_alpha: false,
                parallax_height_scale: 0.04,
                parallax_max_passes: 4.0,
            },
        );
    }
    world.insert(entity, bound);
    // #2444 (MAT-D3-02) — canonical `Material` from the same boundary helper
    // the full-detail terrain uses, classified off whichever distant texture
    // this block actually resolved (baked LOD quad or tiled base LTEX). Keeps
    // the near/far terrain shading consistent across the full-detail ring
    // boundary instead of 0.85 inside it and the render path's hardcoded 0.5
    // outside.
    world.insert(
        entity,
        crate::material_translate::translate_texture_only_material(base_texture_path),
    );
    // Architecture layer (zero depth bias) — same canonical baseline the
    // full-detail terrain uses.
    world.insert(entity, RenderLayer::Architecture);
    // Marker: routes through the static draw loop with `in_tlas = false`
    // and is skipped by any TLAS-membership logic.
    world.insert(entity, IsLodTerrain);

    let hole_mask = block_hole_mask(
        cells_map,
        bx0,
        by0,
        player_grid,
        max_full_cell_radius,
        resident_full_cells,
    );
    Some(LodBlock {
        entity,
        mesh_handle,
        texture_handle: tex_handle,
        normal_texture_handle,
        hole_mask,
    })
}

/// World-space bounding sphere over the block's non-hole vertices. Falls
/// back to the block footprint centre with a half-diagonal radius when the
/// block is degenerate (shouldn't happen — caller already rejected
/// all-hole blocks).
fn block_bound(vertices: &[Vertex], holes: &[bool]) -> WorldBound {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (v, &hole) in vertices.iter().zip(holes.iter()) {
        if hole {
            continue;
        }
        let p = Vec3::new(v.position[0], v.position[1], v.position[2]);
        min = min.min(p);
        max = max.max(p);
    }
    if !min.is_finite() {
        return WorldBound::ZERO;
    }
    let center = (min + max) * 0.5;
    let radius = (max - center).length();
    WorldBound::new(center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sample_to_cell` walks cleanly across cell boundaries and maps the
    /// block's far edge to the previous cell's shared seam (local 32),
    /// not a phantom cell `k`.
    #[test]
    fn sample_to_cell_walks_boundaries() {
        let k = LOD_BLOCK_CELLS as usize; // 4
                                          // First cell: samples at local 0, 8, 16, 24.
        assert_eq!(sample_to_cell(0, k), (0, 0));
        assert_eq!(sample_to_cell(1, k), (0, 8));
        assert_eq!(sample_to_cell(3, k), (0, 24));
        // Cell boundary: index 4 is the start of cell 1 (its local 0).
        assert_eq!(sample_to_cell(4, k), (1, 0));
        assert_eq!(sample_to_cell(8, k), (2, 0));
        // Far edge: maps to the last cell's seam (local 32), not cell k.
        let max = k * SAMPLES_PER_CELL;
        assert_eq!(sample_to_cell(max, k), (k - 1, 32));
    }

    /// Adjacent block vertices are exactly `VERT_SPACING` apart and a full
    /// block spans exactly `LOD_BLOCK_CELLS` cells of world units, so
    /// blocks tile seamlessly (the world_x = origin + c·spacing identity
    /// the builder relies on).
    #[test]
    fn block_spans_whole_cells() {
        let k = LOD_BLOCK_CELLS as usize;
        let n = k * SAMPLES_PER_CELL + 1;
        let span = (n - 1) as f32 * VERT_SPACING;
        assert_eq!(span, LOD_BLOCK_CELLS as f32 * EXTERIOR_CELL_UNITS);
        // Vertex spacing equals stride × per-vertex spacing (128 BU).
        assert_eq!(VERT_SPACING, STRIDE as f32 * 128.0);
    }

    /// #2061 — the LOD terrain builder now calls the canonical
    /// `zup_to_yup_pos` helper instead of hand-deriving the same
    /// `(x, y, z) → (x, z, -y)` swizzle inline. Pin that both position
    /// forms (main vertex + placeholder + normal) remain bit-identical to
    /// the pre-fix manual literals for representative inputs.
    #[test]
    fn zup_to_yup_pos_matches_old_inline_swizzle() {
        let world_x = 123.5_f32;
        let world_y_zup = -456.25_f32;
        let height = 78.0_f32;

        // Main vertex position: old inline literal was `[world_x, height, -world_y_zup]`.
        assert_eq!(
            zup_to_yup_pos([world_x, world_y_zup, height]),
            [world_x, height, -world_y_zup]
        );

        // Placeholder vertex: old inline literal was `[world_x, 0.0, -world_y_zup]`.
        assert_eq!(
            zup_to_yup_pos([world_x, world_y_zup, 0.0]),
            [world_x, 0.0, -world_y_zup]
        );

        // Normal: old inline literal was `[nx, nz, -ny]`.
        let (nx, ny, nz) = (0.25_f32, 0.5_f32, -0.75_f32);
        assert_eq!(zup_to_yup_pos([nx, ny, nz]), [nx, nz, -ny]);
    }

    #[test]
    fn chebyshev_distance() {
        assert_eq!(chebyshev((0, 0), (0, 0)), 0);
        assert_eq!(chebyshev((3, 1), (0, 0)), 3);
        assert_eq!(chebyshev((-2, 5), (0, 0)), 5);
    }

    /// #1871 / LC0703-02 — the terrain sibling of #1866 / LC0703-01. A cell
    /// whose nearest full-detail distance sits exactly at the streaming
    /// hysteresis boundary (`radius_load + 1 == radius_unload`) must NOT be
    /// left un-holed (i.e. must stay full-detail-excluded from the LOD mesh)
    /// when gated on `radius_unload`, even though the pre-fix code (gating
    /// on `radius_load`) would have left it un-holed — that one-cell band is
    /// exactly where a full cell can still be resident (unload only fires
    /// past `radius_unload`), so rendering LOD terrain there would z-fight
    /// the still-resident full-detail terrain.
    #[test]
    fn cell_is_full_detail_covers_hysteresis_band_when_gated_on_radius_unload() {
        let radius_load = 5;
        let radius_unload = radius_load + 1; // streaming.rs's hysteresis rule
        let player = (0, 0);
        let cell = (radius_unload, 0); // nearest cell at exactly radius_load+1

        // Buggy pre-fix behaviour: gating on radius_load does NOT cover this
        // cell (it's one past radius_load), so it would be left un-holed.
        assert!(
            !cell_is_full_detail(cell.0, cell.1, player, radius_load),
            "sanity: radius_load gating must reproduce the pre-fix bug"
        );

        // Fixed behaviour: gating on radius_unload covers it (holed out —
        // left for the streamed near terrain, not rendered as LOD).
        assert!(
            cell_is_full_detail(cell.0, cell.1, player, radius_unload),
            "a cell at exactly radius_load+1 can still hold a resident full \
             REFR under the load/unload hysteresis band — LOD terrain must \
             not render there"
        );

        // A cell safely beyond the hysteresis band is still LOD-eligible.
        assert!(!cell_is_full_detail(
            radius_unload + 1,
            0,
            player,
            radius_unload
        ));
    }

    /// #2371 / EX-11 — [`MAX_LOD_RING_REACH_CELLS`] backs a compile-time
    /// check on the camera far plane, so it has to be both an upper bound on
    /// every game's ring (or the horizon gets culled) and tight (or the far
    /// plane is padded for a ring nothing streams, wasting depth precision
    /// that is already the limiting factor — see `DEFAULT_RENDER_DISTANCE`).
    #[test]
    fn max_ring_reach_covers_every_game() {
        let games = [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
            GameKind::Fallout76,
            GameKind::Starfield,
        ];
        let mut widest = 0;
        for game in games {
            let reach = lod_ring_reach_cells(game);
            assert!(
                reach <= MAX_LOD_RING_REACH_CELLS,
                "{game:?} streams {reach} cells, past the pinned maximum of {}",
                MAX_LOD_RING_REACH_CELLS
            );
            widest = widest.max(reach);
        }
        assert_eq!(
            widest, MAX_LOD_RING_REACH_CELLS,
            "the pinned maximum is looser than any real ring — tighten it"
        );

        // Games with no combined `.btr` ladder keep the synthesized ring; the
        // combined-ladder games reach further, which is what moved the far plane.
        assert_eq!(
            lod_ring_reach_cells(GameKind::Oblivion),
            LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS
        );
        assert!(lod_ring_reach_cells(GameKind::Skyrim) > LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS);
    }

    /// #2371 / #3100 — Oblivion / FO3 / FNV have no combined `.btr` ladder, so
    /// their geometry ring remains what it was before band selection existed:
    /// one finest-level ring, `LOD_RADIUS_BLOCKS` blocks deep, centred on the
    /// player's block. A regression here is a silently shrunken horizon on
    /// the three games that have no coarser source to fall back to.
    #[test]
    fn games_without_a_combined_btr_ladder_keep_the_single_synth_ring() {
        let grid_origin = (0, 0);
        let player = (5, -3);
        let quads = desired_lod_quads(
            None,
            player,
            grid_origin,
            6,
            None,
            |_, _, _| false,
            |_, _, _| panic!(".btr availability must never be probed without a ladder"),
        );

        let side = (2 * LOD_RADIUS_BLOCKS + 1) as usize;
        assert_eq!(quads.len(), side * side);
        assert!(quads.iter().all(|&(level, _, _)| level == LOD_BLOCK_CELLS));

        // Exactly the old block square, expressed in SW-corner cells.
        let (pbi, pbj) = block_index(player, grid_origin);
        let mut expected: Vec<(i32, i32, i32)> = Vec::new();
        for bj in (pbj - LOD_RADIUS_BLOCKS)..=(pbj + LOD_RADIUS_BLOCKS) {
            for bi in (pbi - LOD_RADIUS_BLOCKS)..=(pbi + LOD_RADIUS_BLOCKS) {
                let (bx0, by0) = block_cell_origin(bi, bj, grid_origin);
                expected.push((LOD_BLOCK_CELLS, bx0, by0));
            }
        }
        let mut got = quads.clone();
        got.sort_unstable();
        expected.sort_unstable();
        assert_eq!(got, expected);
    }

    /// The band ladder only ever offers a coarse terrain quad where the game
    /// baked a `.btr` for it. Where it did not, the descent must subdivide
    /// down to the finest band — which heightmap synth can always cover —
    /// rather than leave a hole in the horizon.
    #[test]
    fn coarse_terrain_bands_require_a_baked_btr() {
        let ladder = LodBandLadder::for_game(GameKind::Skyrim).unwrap();
        let quads = desired_lod_quads(
            Some(&ladder),
            (0, 0),
            (0, 0),
            6,
            None,
            |_, _, _| false,
            |_, _, _| false, // no `.btr` baked anywhere
        );
        assert!(!quads.is_empty());
        assert!(
            quads.iter().all(|&(level, _, _)| level == LOD_BLOCK_CELLS),
            "with no baked `.btr`, every terrain quad must fall back to the synth band"
        );

        // With `.btr` present, coarse bands are used and the ring reaches
        // further than the synth-only ring ever did.
        let banded = desired_lod_quads(
            Some(&ladder),
            (0, 0),
            (0, 0),
            6,
            None,
            |_, _, _| false,
            |_, _, _| true,
        );
        assert!(banded.iter().any(|&(level, _, _)| level > LOD_BLOCK_CELLS));
        let reach = banded
            .iter()
            .map(|&(l, qx, qy)| quad_min_chebyshev(qx, qy, l, (0, 0)))
            .max()
            .unwrap();
        assert!(
            reach > LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS,
            "banded terrain must reach past the old {}-cell synth ring, got {reach}",
            LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS
        );
    }

    /// Only the finest band can ever need a hole mask. Coarse bands are
    /// selected far outside the streaming radius, so `stream_lod_blocks` is
    /// entitled to hardcode their mask to 0 — this pins the premise.
    #[test]
    fn coarse_terrain_bands_never_reach_the_full_detail_region() {
        let radius_unload = 6;
        for game in [GameKind::Skyrim, GameKind::Fallout4] {
            let ladder = LodBandLadder::for_game(game).unwrap();
            for py in 0..8 {
                for px in 0..8 {
                    let quads = desired_lod_quads(
                        Some(&ladder),
                        (px, py),
                        (0, 0),
                        radius_unload,
                        None,
                        |_, _, _| false,
                        |_, _, _| true,
                    );
                    for (level, qx, qy) in quads {
                        assert!(
                            quad_min_chebyshev(qx, qy, level, (px, py)) > radius_unload,
                            "level-{level} quad ({qx}, {qy}) reaches inside the streaming radius"
                        );
                    }
                }
            }
        }
    }

    /// #1373 — a boundary block's hole mask shifts as the player crosses
    /// cells, which is exactly what triggers regeneration. Pure radius
    /// closure, no `CellData` map needed.
    #[test]
    fn hole_mask_shifts_with_player_full_detail_region() {
        let full = 1;
        let holed = |p: (i32, i32)| move |gx: i32, gy: i32| chebyshev((gx, gy), p) <= full;

        // The block at SW cell (0,0) covers cells (0,0)..=(3,3). Player at
        // (0,0) holes its SW corner; moving the player east to (4,0) (into
        // the next block) clears those holes — the mask must change
        // (→ regenerate).
        let blk0_at_origin = assemble_hole_mask(0, 0, holed((0, 0)));
        let blk0_at_east = assemble_hole_mask(0, 0, holed((4, 0)));
        assert_ne!(
            blk0_at_origin, blk0_at_east,
            "boundary block mask must change as the player crosses cells"
        );

        // The block the player moved INTO gains the full-detail holes.
        let blk1_at_origin = assemble_hole_mask(4, 0, holed((0, 0)));
        let blk1_at_east = assemble_hole_mask(4, 0, holed((4, 0)));
        assert_ne!(blk1_at_origin, blk1_at_east);

        // A far block (outside the full-detail radius from both positions)
        // keeps mask 0 — never regenerates from player motion (the common
        // case: most of the ring is stable).
        assert_eq!(assemble_hole_mask(20, 20, holed((0, 0))), 0);
        assert_eq!(assemble_hole_mask(20, 20, holed((4, 0))), 0);
    }

    /// `all_holed_mask` has exactly the low `LOD_BLOCK_CELLS²` bits set —
    /// the sentinel for an empty (all-hole) block that `stream_lod_blocks`
    /// skips spawning.
    #[test]
    fn all_holed_mask_is_full_block() {
        let always = |_: i32, _: i32| true;
        assert_eq!(all_holed_mask(), assemble_hole_mask(0, 0, always));
        assert_eq!(all_holed_mask(), 0xFFFF); // 4×4 = 16 bits
    }

    #[test]
    fn terrain_block_indices_preserve_nonzero_worldspace_phase() {
        let apocrypha = (-50, -50);
        assert_eq!(block_index((-49, -49), apocrypha), (0, 0));
        assert_eq!(block_cell_origin(0, 0, apocrypha), apocrypha);
        assert_eq!(block_cell_origin(1, 1, apocrypha), (-46, -46));

        let soul_cairn = (-52, -51);
        assert_eq!(block_index((-50, -49), soul_cairn), (0, 0));
        assert_eq!(block_cell_origin(0, 0, soul_cairn), soul_cairn);
        assert_eq!(block_cell_origin(1, 1, soul_cairn), (-48, -47));
    }
}
