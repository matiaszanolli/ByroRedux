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
//!   * builds **no BLAS** at spawn and marks blocks [`IsLodTerrain`] so the
//!     renderer's bounded restore path may later admit camera-local blocks
//!     into the TLAS as RT shadow casters (`LOD_SHADOW_CASTER_DISTANCE`,
//!     #4180) — distant terrain beyond that ring costs zero ray-tracing
//!     budget; see `docs/engine/shadow-pipeline-tradeoffs.md`,
//!   * holes out each actually-resident full-detail cell so the LOD
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

use crate::asset_provider::{
    resolve_linear_texture_with_clamp, resolve_texture, resolve_texture_with_clamp,
    MaterialProvider, TextureProvider,
};
use crate::components::{IsLodTerrain, MaterialTextureHandles};
use crate::env_translate::translate_terrain_lod_textures;
use crate::streaming::LodBlock;

use super::lod_bands::{self, quad_min_chebyshev, LodBandLadder, LodBandSelection};
use super::terrain::LandscapeLayerTextures;
use super::lod_support::{
    classify_lod_attempt, combined_lod_supported, legacy_landscape_lod_supported,
    lod_source_cell_exists, retain_resident, worldspace_cell_bounds, LodAttemptVerdict,
    LodReconcileInput, LodWorkBudget,
};

/// Cells per LOD-block edge. A block is one merged mesh covering
/// `LOD_BLOCK_CELLS²` cells (4×4 = 16 cells = 16384×16384 BU). Bigger
/// blocks → fewer draw entities; smaller → finer hole-out granularity at
/// the full-detail boundary. 4 keeps the block count bounded while still
/// aligning to the 4-quadrant cell layout.
pub(crate) const LOD_BLOCK_CELLS: i32 = 4;

/// LOD ring radius in blocks (Chebyshev). 12 blocks × 4 cells × 4096 BU =
/// ~196 K BU of distant terrain in every direction — 4× the default
/// full-detail view (`DEFAULT_EXTERIOR_RADIUS` 12 cells = 49 152 BU; the
/// ratio was ~9.6× against the radius-5 default this comment was written for).
/// Tunable; the camera far plane (`Camera::default`) is sized to cover the
/// resulting far-corner diagonal.
pub(crate) const LOD_RADIUS_BLOCKS: i32 = 12;

/// Heightmap sample stride within a cell's 33-vertex grid. 8 → 4 quads per
/// cell edge (5 samples incl. the shared seam), 1/64th the triangles.
const STRIDE: usize = 8;
/// Quad samples contributed per cell edge (= 32 / STRIDE).
const SAMPLES_PER_CELL: usize = 32 / STRIDE;
/// World-space spacing between adjacent block-mesh vertices (BU).
const VERT_SPACING: f32 = EXTERIOR_CELL_UNITS / 32.0 * STRIDE as f32; // 1024.0

/// #4898 — shading tangent for baked legacy LOD quads: world east (+X, Y-up
/// engine frame) with a +1 bitangent sign, so `B = cross(N, T)` is world
/// north. The quads' normal maps are world-oriented, not image-oriented.
const LOD_QUAD_WORLD_TANGENT: [f32; 4] = [1.0, 0.0, 0.0, 1.0];

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
#[cfg(test)]
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

/// Whether cell `(gx, gy)` is actually resident at full detail and must be
/// holed out of the LOD mesh. This is deliberately set-based: `radius_unload`
/// bounds possible hysteresis residency but does not populate its outer ring.
fn cell_is_full_detail(gx: i32, gy: i32, resident_full_cells: &HashSet<(i32, i32)>) -> bool {
    resident_full_cells.contains(&(gx, gy))
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
// Skirts (`append_lod_skirts`) hang only along the edges that border a
// resident full-detail cell, so they scale with the perimeter of the resident
// region (at most 15 x 15 cells, 4 samples a cell: 240 edges, 2 vertices and 6
// indices each), not with the ring's area. The allowance below is ~8x that.
const LOD_SKIRT_VERTEX_ALLOWANCE: usize = 4096;
const LOD_SKIRT_INDEX_ALLOWANCE: usize = 8192;
const LOD_RING_VERTEX_FOOTPRINT: usize = {
    let blocks_per_side = 2 * LOD_RADIUS_BLOCKS as usize + 1;
    let verts_per_edge = LOD_BLOCK_CELLS as usize * SAMPLES_PER_CELL + 1;
    blocks_per_side * blocks_per_side * verts_per_edge * verts_per_edge + LOD_SKIRT_VERTEX_ALLOWANCE
};
const LOD_RING_INDEX_FOOTPRINT: usize = {
    let blocks_per_side = 2 * LOD_RADIUS_BLOCKS as usize + 1;
    let quads_per_edge = LOD_BLOCK_CELLS as usize * SAMPLES_PER_CELL;
    blocks_per_side * blocks_per_side * quads_per_edge * quads_per_edge * 6
        + LOD_SKIRT_INDEX_ALLOWANCE
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

// The rim mask is a `u16` too: `LOD_BLOCK_CELLS` cells along each of four sides.
const _: () = assert!(
    4 * LOD_BLOCK_CELLS <= 16,
    "LOD rim mask is u16 — four sides of LOD_BLOCK_CELLS cells must fit"
);

/// OR a bit for every cell just outside the finest-level block whose SW corner is
/// `(bx0, by0)` — the `LOD_BLOCK_CELLS` cells beyond each of its four edges
/// (west, east, south, north, in that bit order per step along the edge) — for
/// which `resident(gx, gy)` is true. These are the cells a skirt can look at across
/// a block border, so the pattern is part of what a synth block was built from.
fn assemble_rim_mask(bx0: i32, by0: i32, resident: impl Fn(i32, i32) -> bool) -> u16 {
    let k = LOD_BLOCK_CELLS;
    let mut mask: u16 = 0;
    for d in 0..k {
        let outside = [(bx0 - 1, by0 + d), (bx0 + k, by0 + d), (bx0 + d, by0 - 1), (bx0 + d, by0 + k)];
        for (side, (gx, gy)) in outside.into_iter().enumerate() {
            if resident(gx, gy) {
                mask |= 1u16 << (d * 4 + side as i32);
            }
        }
    }
    mask
}

/// The rim pattern of the finest-level block at `(bx0, by0)` against actual
/// full-detail residency (#6409). Together with [`block_hole_mask`] it is the
/// block's rebuild key: a skirt exists exactly where a LOD quad edge borders a
/// resident cell, wherever that cell is.
fn block_rim_mask(bx0: i32, by0: i32, resident_full_cells: &HashSet<(i32, i32)>) -> u16 {
    assemble_rim_mask(bx0, by0, |gx, gy| cell_is_full_detail(gx, gy, resident_full_cells))
}

/// Whether the finest-band block with this hole/rim pattern may be drawn from a
/// prebaked `.btr` mesh: only when no cell of it is holed and none of its
/// neighbours is resident. A `.btr` carries per-quad texturing but neither
/// per-cell holes nor skirts, so a block that touches full-detail terrain takes
/// the synth path, like a block that contains it (#6409).
fn block_may_use_prebaked(hole_mask: u16, rim_mask: u16) -> bool {
    hole_mask == 0 && rim_mask == 0
}

/// Mask with every cell holed — an empty block (no geometry to draw).
fn all_holed_mask() -> u16 {
    ((1u32 << (LOD_BLOCK_CELLS * LOD_BLOCK_CELLS)) - 1) as u16
}

/// The 16-bit per-cell hole pattern for the finest-level block at SW cell
/// `(bx0, by0)`, given actual full-detail residency. A cell is holed when
/// streamed near terrain renders it or when it has no landscape. Stored on
/// [`LodBlock`] so a boundary
/// block is regenerated only when its pattern actually changes.
///
/// Only the finest band needs this. Coarser bands are kept out of the
/// resident ring by selection — a coarse quad whose nearest cell is within
/// `radius_unload` subdivides to the finest band (#6094), which this masks — and
/// are drawn from prebaked `.btr` meshes (or authored quads) that already bake in
/// the game's own coverage, so their mask is always 0 — see [`stream_lod_blocks`].
/// The band thresholds alone do not guarantee that: they were tuned against a
/// ~6-cell streaming radius, and the default is 12.
fn block_hole_mask(
    cells_map: &HashMap<(i32, i32), CellData>,
    bx0: i32,
    by0: i32,
    resident_full_cells: &HashSet<(i32, i32)>,
) -> u16 {
    assemble_hole_mask(bx0, by0, |gx, gy| {
        cell_is_full_detail(gx, gy, resident_full_cells)
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
pub(crate) const MAX_LOD_RING_REACH_CELLS: i32 = 64;

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
    LodBandLadder::for_terrain_game(game)
        .map(|ladder| ladder.max_cells())
        .unwrap_or(LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS)
}

/// The `(level, qx, qy)` quads the terrain ring wants resident this
/// reconcile, closest-first so a budgeted initialization grows outward
/// deterministically instead of following HashMap iteration order.
///
/// Two regimes, by whether the game uses the combined `.btr` quadtree (#2371):
///
/// * **FO3 / FNV / Skyrim / FO4** descend [`super::lod_bands`]' 4/8/16/32
///   ladder. A coarse band is only offered where the game actually baked an
///   authored terrain asset;
///   where it did not, the descent subdivides instead of leaving a hole,
///   and the finest band always reports available because heightmap synth
///   can cover any footprint.
/// * **Oblivion** keeps the single synthesized [`LOD_RADIUS_BLOCKS`]-deep ring.
///
/// #5222 — the Fallout-legacy terrain selection: the authored colour
/// quads the band ring touches
/// ([`super::lod_bands::select_authored_lod_quads`]), plus level-4 synth
/// completion for ring ground no authored quad covers.
///
/// The completion tiles are only added where a candidate tile overlaps NO
/// selected quad at all — zero overlap, not zero coverage. A tile that
/// half-overlaps an authored quad is left alone: drawing it would
/// double-draw (and z-fight) the covered half with coplanar distant
/// terrain, while leaving it leaves at most an authoring-seam hole, the
/// same posture as the object ring's empty sentinel. Vanilla's LOD
/// generator bakes full per-worldspace coverage, so the completion is a
/// safety net for worldspace edges and modded gaps, not the normal path.
///
/// #6094 — `full_detail_radius` is the Chebyshev radius within which a
/// full-detail cell may be resident. A coarse authored quad reaching it is not
/// selected; the finer authored quads and this completion tile its ground
/// instead, and the finest band is holed against actual residency.
fn legacy_desired_lod_quads(
    authored: &[(i32, i32, i32)],
    ladder: &super::lod_bands::LodBandLadder,
    player: (i32, i32),
    world_bounds: Option<((i32, i32), (i32, i32))>,
    cell_exists: &dyn Fn(i32, i32) -> bool,
    full_detail_radius: i32,
) -> Vec<(i32, i32, i32)> {
    let selected = super::lod_bands::select_authored_lod_quads_refining(
        authored,
        ladder,
        player,
        world_bounds,
        cell_exists,
        full_detail_radius,
    );
    // Cell-coverage window around the player, padded one finest tile so
    // boundary tiles are evaluated whole. max_cells is 64 and the tile is
    // 4 cells, so the window edges stay tile-aligned.
    let reach = ladder.max_cells() + LOD_BLOCK_CELLS;
    let size = 2 * reach + 1;
    let cell_covered = |cx: i32, cy: i32| {
        let (ix, iy) = (cx - player.0 + reach, cy - player.1 + reach);
        (0..size).contains(&ix) && (0..size).contains(&iy)
    };
    let mut covered = vec![false; (size * size) as usize];
    for &(level, qx, qy) in &selected {
        for dy in 0..level {
            for dx in 0..level {
                let (ix, iy) = (
                    (qx + dx - player.0 + reach) as usize,
                    (qy + dy - player.1 + reach) as usize,
                );
                if ix < size as usize && iy < size as usize {
                    covered[iy * size as usize + ix] = true;
                }
            }
        }
    }
    let mut out = selected.clone();
    let (bx, by) = (
        player.0.div_euclid(LOD_BLOCK_CELLS) * LOD_BLOCK_CELLS - reach,
        player.1.div_euclid(LOD_BLOCK_CELLS) * LOD_BLOCK_CELLS - reach,
    );
    let tiles_per_side = (2 * reach) / LOD_BLOCK_CELLS + 1;
    for tj in 0..tiles_per_side {
        for ti in 0..tiles_per_side {
            let (tx, ty) = (
                bx + ti * LOD_BLOCK_CELLS,
                by + tj * LOD_BLOCK_CELLS,
            );
            if !super::lod_bands::quad_intersects_bounds(tx, ty, LOD_BLOCK_CELLS, world_bounds)
            {
                continue;
            }
            // Same ring bound as the authored selection — the reach-padded
            // window's rim cells sit beyond `max_cells` and stay undrawn,
            // exactly as the descent's outer-band prune left them.
            if quad_min_chebyshev(tx, ty, LOD_BLOCK_CELLS, player) > ladder.max_cells() {
                continue;
            }
            // Any uncovered cell inside the tile?
            let mut has_uncovered = false;
            'cells: for dy in 0..LOD_BLOCK_CELLS {
                for dx in 0..LOD_BLOCK_CELLS {
                    let (cx, cy) = (tx + dx, ty + dy);
                    if !cell_covered(cx, cy) {
                        continue;
                    }
                    let (ix, iy) = (
                        (cx - player.0 + reach) as usize,
                        (cy - player.1 + reach) as usize,
                    );
                    if !covered[iy * size as usize + ix] {
                        has_uncovered = true;
                        break 'cells;
                    }
                }
            }
            if !has_uncovered {
                continue;
            }
            // Zero-overlap guard: skip any tile a selected quad touches.
            let overlaps = out.iter().any(|&(level, qx, qy)| {
                tx < qx + level && qx < tx + LOD_BLOCK_CELLS && ty < qy + level && qy < ty + LOD_BLOCK_CELLS
            });
            if !overlaps {
                out.push((LOD_BLOCK_CELLS, tx, ty));
            }
        }
    }
    out
}

/// [`desired_lod_quads_refining`] without the forced subdivision — the pre-#6094
/// shape, kept for the tests that pin the ladder's own behaviour.
#[cfg(test)]
fn desired_lod_quads(
    ladder: Option<&LodBandLadder>,
    player_grid: (i32, i32),
    grid_origin: (i32, i32),
    max_full_cell_radius: i32,
    world_bounds: Option<((i32, i32), (i32, i32))>,
    resident: impl Fn(i32, i32, i32) -> bool,
    has_btr: impl FnMut(i32, i32, i32) -> bool,
) -> Vec<(i32, i32, i32)> {
    desired_lod_quads_refining(
        ladder,
        player_grid,
        grid_origin,
        max_full_cell_radius,
        -1,
        world_bounds,
        resident,
        has_btr,
    )
}

/// `full_detail_radius` is #6094's forced subdivision (see
/// [`LodBandSelection::refine_within`]); `-1` disables it.
#[allow(clippy::too_many_arguments)]
fn desired_lod_quads_refining(
    ladder: Option<&LodBandLadder>,
    player_grid: (i32, i32),
    grid_origin: (i32, i32),
    max_full_cell_radius: i32,
    full_detail_radius: i32,
    world_bounds: Option<((i32, i32), (i32, i32))>,
    resident: impl Fn(i32, i32, i32) -> bool,
    mut has_btr: impl FnMut(i32, i32, i32) -> bool,
) -> Vec<(i32, i32, i32)> {
    let k = LOD_BLOCK_CELLS;
    let mut desired = match ladder {
        Some(ladder) => {
            let selection = LodBandSelection {
                ladder,
                player: player_grid,
                grid_origin,
                exclude_within: max_full_cell_radius,
                refine_within: full_detail_radius,
                world_bounds,
                // Terrain has no use for the #3502 coarsen escape: the
                // predicate below reports the finest level available
                // unconditionally, so the descent always lands on a quad
                // that draws (baked or synthesized).
                coarsen_to_available: false,
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
///   * finest-band boundary blocks whose hole mask changed (actual full-cell
///     residency changed) are regenerated so the LOD never
///     overlaps or gaps against the streamed near terrain.
///
/// Resident full-detail cells are holed out. Reclaims and stale-mask
/// invalidations happen immediately; entering/regenerated blocks consume
/// [`LodWorkBudget`] units and may be deferred. Returns `true` once every
/// desired coordinate is either resident or recorded in `missing_blocks`.
///
/// `mat_provider` fills a synth block's base layer from its LTEX texture
/// set's BGSM (an FO4 MNAM-only TXST authors no TX00), the same resolver
/// full-detail terrain uses (#6199 / #6246).
#[allow(clippy::too_many_arguments)]
pub(crate) fn stream_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    mat_provider: Option<&mut MaterialProvider>,
    lod_blocks: &mut HashMap<(i32, i32, i32), LodBlock>,
    missing_blocks: &mut HashMap<(i32, i32, i32), (u16, u16)>,
    available_cache: &mut rustc_hash::FxHashMap<(i32, i32, i32), bool>,
    budget: &mut LodWorkBudget,
) -> bool {
    let tex_provider = input.tex_provider;
    let wctx = input.wctx;
    let player_grid = input.player_grid;
    let resident_full_cells = input.resident_full_cells;
    let index = &wctx.record_index.cells;
    let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
        return true;
    };
    let mut layer_textures = LandscapeLayerTextures {
        landscape_textures: &index.landscape_textures,
        landscape_texture_sets: &index.landscape_texture_sets,
        mat_provider,
    };
    // Combined `.btr` distant terrain is a Skyrim/FO4 scheme. Legacy games
    // keep the heightmap-synth geometry, but EXAL can now supply their
    // authored NIF-era diffuse quads to that common path (#3100).
    let game = wctx.record_index.game;
    // #5496 — every distant-ring key resolves on the LOD source
    // worldspace (the PNAM "Use LOD Data" ancestor for an inheriting
    // child): the quad index, the `.btr` paths, the generated distant
    // textures and the worldspace form id that keys them. The
    // `cells_map` above stays on the child — it feeds the finest band's
    // full-detail hole mask and per-cell LTEX.
    let worldspace_key = wctx.lod_worldspace_key();
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
    let ladder = LodBandLadder::for_terrain_game(game);

    // #5423 — gate on the layout table, not the GameKind: a future title
    // adopting the Fallout legacy layout takes the authored name-table
    // path in this ring exactly like object_lod gates on its scheme.
    let desired = if matches!(
        crate::env_translate::terrain_lod_layout(game),
        crate::env_translate::TerrainLodLayout::FalloutLegacy
    ) {
        // #5222 — the Fallout legacy family selects from the archive
        // name-table index instead of the `(0,0)`-anchored descent: 835 of
        // its 2,231 authored terrain quads sit on lattices no derived
        // origin reproduces, so their diffuse/normal DDS never resolved
        // and the terrain fell back to the tiled base LTEX everywhere
        // off-grid. Authored quads now draw with their real colour; the
        // level-4 synth completion below keeps the old coverage guarantee
        // for ground no authored quad covers.
        let authored = input
            .legacy_lod_quads
            .map(|index| index.terrain_for(worldspace_key))
            .unwrap_or_default();
        ladder
            .as_ref()
            .map(|ladder| {
                legacy_desired_lod_quads(
                    &authored,
                    ladder,
                    player_grid,
                    worldspace_cell_bounds(wctx),
                    &lod_source_cell_exists(wctx),
                    input.full_detail_radius,
                )
            })
            .unwrap_or_default()
    } else {
        desired_lod_quads_refining(
            ladder.as_ref(),
            player_grid,
            grid_origin,
            // Boundary quads must be selected so their per-cell mask can fill
            // every coordinate that is not actually resident at full detail.
            -1,
            // #6094 — and any coarse quad reaching the resident region
            // subdivides to that finest, hole-masked band.
            input.full_detail_radius,
            worldspace_cell_bounds(wctx),
            |level, qx, qy| lod_blocks.contains_key(&(level, qx, qy)),
            // #3385 — memoised: the probe allocates several `String`s and hashes
            // one lookup per open archive, and its answer cannot change while
            // this `WorldStreamingState` lives.
            |level, qx, qy| {
                *available_cache.entry((level, qx, qy)).or_insert_with(|| {
                    if combined_lod_supported(game) {
                        tex_provider.has_mesh(&super::terrain_lod_btr::btr_archive_path(
                            worldspace_key,
                            level,
                            qx,
                            qy,
                        ))
                    } else {
                        translate_terrain_lod_textures(
                            game,
                            worldspace_key,
                            world_form_id,
                            level,
                            qx,
                            qy,
                        )
                        .is_some_and(|lod| tex_provider.has_texture(&lod.diffuse_path))
                    }
                })
            },
        )
    };
    let desired_set: HashSet<_> = desired.iter().copied().collect();

    let mut spawned = 0usize;
    let mut btr_spawned = 0usize;
    let mut invalidated = 0usize;
    let mut unloaded = 0usize;

    // Unload blocks that left the ring or changed band. No empty
    // sentinels on this ring — every evicted block carries resources.
    unloaded += retain_resident(&mut *lod_blocks, &desired_set, |block| {
        unload_lod_block(world, ctx, block);
        true
    });
    missing_blocks.retain(|coord, _| desired_set.contains(coord));

    let all_holed = all_holed_mask();
    let mut candidates = Vec::new();

    // First invalidate every stale block, regardless of the spawn budget.
    // This prevents old hole masks from overlapping full-detail terrain while
    // replacement geometry waits for an idle frame.
    for &(level, qx, qy) in &desired {
        // Only the finest band can touch the full-detail region or need
        // per-cell holes: selection subdivides any coarse quad that would
        // reach the resident ring (`LodReconcileInput::full_detail_radius`,
        // #6094), so the baked coarse meshes stay outside it.
        let (mask, rim) = if level == k {
            (
                block_hole_mask(cells_map, qx, qy, resident_full_cells),
                block_rim_mask(qx, qy, resident_full_cells),
            )
        } else {
            (0, 0)
        };
        // Copy the prior masks out before mutating the map (no borrow held).
        let prev_masks = lod_blocks
            .get(&(level, qx, qy))
            .map(|b| (b.hole_mask, b.rim_mask));

        // Empty block (every cell holed) — ensure no stale entry remains.
        if mask == all_holed {
            if let Some(block) = lod_blocks.remove(&(level, qx, qy)) {
                unload_lod_block(world, ctx, &block);
                unloaded += 1;
            }
            missing_blocks.remove(&(level, qx, qy));
            continue;
        }

        match prev_masks {
            Some(m) if m == (mask, rim) => {
                missing_blocks.remove(&(level, qx, qy));
                continue;
            }
            Some(_) => {
                // Hole or rim pattern moved — drop the stale block now; its
                // replacement may be deferred below.
                if let Some(block) = lod_blocks.remove(&(level, qx, qy)) {
                    unload_lod_block(world, ctx, &block);
                }
                invalidated += 1;
            }
            None => {} // new block
        }

        if missing_blocks.get(&(level, qx, qy)) == Some(&(mask, rim)) {
            continue;
        }
        missing_blocks.remove(&(level, qx, qy));
        candidates.push((level, qx, qy, mask, rim));
    }

    let candidate_count = candidates.len();
    let mut attempted = 0usize;
    // #6092 — set when the geometry pool stops admitting uploads. The rest of
    // the ring is then held back rather than attempted or cached as a miss.
    let mut starved = false;
    for (level, qx, qy, mask, rim) in candidates {
        // #6092 — a closed pool refuses every upload, so decoding and
        // rejecting each remaining quad is wasted work (an archive read, a
        // parse, a texture resolve) for a certain refusal.
        if !ctx.mesh_registry.scene_geometry_admission_open() {
            budget.note_held_back(candidate_count - attempted);
            starved = true;
            break;
        }
        if !budget.try_take() {
            break;
        }
        attempted += 1;

        // Prefer the game's prebaked `.btr` distant-terrain mesh for a
        // fully-distant block (`mask == 0` → every cell has landscape and
        // none lie inside the full-detail ring, and `rim == 0` → no resident
        // neighbour to hang a skirt against): it carries per-quad
        // texturing instead of the synth path's single flat base texture.
        // Boundary blocks (`mask != 0`) need per-cell hole-masking a baked
        // mesh can't do, blocks beside the resident region (`rim != 0`) need
        // the synth path's skirts (#6409), and missing `.btr` / older games
        // fall through — the synth path handles all of those. Exactly one block per coordinate,
        // so the textured LOD never double-draws the synth LOD (EXAL §5).
        //
        // Coarse bands (`level > k`) exist *only* as `.btr`: the descent
        // above only proposes them where one is baked, and the synth builder
        // is finest-band-only (its hole mask is a `u16`, one bit per cell).
        let btr_block = if block_may_use_prebaked(mask, rim) && combined_lod_supported(game) {
            super::terrain_lod_btr::spawn_btr_block(
                world,
                ctx,
                tex_provider,
                worldspace_key,
                level,
                qx,
                qy,
                mask,
                game,
            )
        } else {
            None
        };
        let from_btr = btr_block.is_some();
        let block = btr_block.or_else(|| {
            if level != k && !legacy_landscape_lod_supported(game) {
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
                &mut layer_textures,
                cells_map,
                qx,
                qy,
                resident_full_cells,
                game,
                worldspace_key,
                world_form_id,
                level,
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
            match classify_lod_attempt(
                false,
                ctx.mesh_registry.scene_geometry_admission_open(),
            ) {
                // Remember a real miss at this exact hole mask so a budgeted
                // reconcile can continue outward instead of retrying forever.
                LodAttemptVerdict::Miss => {
                    missing_blocks.insert((level, qx, qy), (mask, rim));
                }
                // #6092 — the pool refused this quad. It says nothing about
                // the asset, so nothing is remembered: this quad and the rest
                // of the ring wait for admission to reopen.
                LodAttemptVerdict::HeldBack => {
                    budget.note_held_back(candidate_count - attempted + 1);
                    starved = true;
                    break;
                }
                LodAttemptVerdict::Keep => unreachable!("no block was produced"),
            }
        }
    }

    // A starved ring has nothing left it can try: report it complete, and let
    // the caller's verdict say it was held back rather than settled.
    let complete = starved || attempted == candidate_count;
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

/// A synth block's base diffuse for its first base LTEX, through the
/// full-detail layer resolver (#6246). An MNAM-only LTEX is absent from
/// `landscape_textures`, so a direct lookup dropped every boundary block
/// whose base it was: synth-only on Combined games, so recorded as a real
/// miss and never retried.
fn synth_base_texture_path(
    layer_textures: &mut LandscapeLayerTextures<'_>,
    base_ltex: Option<u32>,
) -> Option<String> {
    let ltex = base_ltex.filter(|&ltex| ltex != 0)?;
    layer_textures.texture_set(ltex)?.diffuse
}

/// Build + spawn one finest-band LOD block whose SW-corner cell is
/// `(bx0, by0)`. Returns `None`
/// (nothing spawned) when the block is entirely holes — every cell either
/// missing, landscape-less, or actually resident at full detail — or the
/// upload fails. On success returns the [`LodBlock`] for streaming tracking.
#[allow(clippy::too_many_arguments)]
/// How far a skirt hangs below the LOD edge it hides a seam for, world units.
/// Terrain's deviation from a straight 1 024-unit chord between two LOD samples
/// is tens to a few hundred units on rugged ground; 640 covers that with room.
const LOD_SKIRT_DROP: f32 = 640.0;

/// Hang a vertical wall under each `(a, b, toward_hole)` edge: `a` and `b` index
/// the top vertices (already in `vertices`), `toward_hole` is the horizontal
/// direction, in renderer axes, from the LOD quad into the full-detail cell the
/// edge borders.
///
/// The LOD block shares its corner vertices with the full-detail cells, but
/// between them (1 024 units apart) the full-detail edge follows the real
/// terrain every 128 units while the LOD edge is a straight chord. Where the
/// chord sits above the true edge the LOD quad has no front face toward the
/// player and sky shows through the join — a pale-blue ribbon along the
/// boundary, most visible seen through water. The wall is wound to face the
/// hole side (the viewer's side), so it fills that gap without being a surface
/// anything can stand on.
fn append_lod_skirts(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    edges: &[(usize, usize, [f32; 3])],
) {
    for &(a, b, toward_hole) in edges {
        let (va, vb) = (vertices[a].clone(), vertices[b].clone());
        let mut down_a = va.clone();
        let mut down_b = vb.clone();
        down_a.position[1] -= LOD_SKIRT_DROP;
        down_b.position[1] -= LOD_SKIRT_DROP;
        let ia = vertices.len() as u32;
        let ib = ia + 1;
        let (pa, pb, pd) = (va.position, vb.position, down_a.position);
        vertices.push(down_a);
        vertices.push(down_b);
        // Face normal of (a, b, a_down) under the terrain's counter-clockwise
        // front-face convention; flip the wall if it faces away from the hole.
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pd[0] - pa[0], pd[1] - pa[1], pd[2] - pa[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let faces_hole = n[0] * toward_hole[0] + n[1] * toward_hole[1] + n[2] * toward_hole[2] >= 0.0;
        let (a, b) = (a as u32, b as u32);
        if faces_hole {
            indices.extend_from_slice(&[a, b, ia, b, ib, ia]);
        } else {
            indices.extend_from_slice(&[a, ia, b, b, ia, ib]);
        }
    }
}

fn spawn_lod_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    layer_textures: &mut LandscapeLayerTextures<'_>,
    cells_map: &HashMap<(i32, i32), CellData>,
    bx0: i32,
    by0: i32,
    resident_full_cells: &HashSet<(i32, i32)>,
    game: GameKind,
    worldspace_key: &str,
    world_form_id: u32,
    level: i32,
) -> Option<LodBlock> {
    let k = level;
    let origin_x = bx0 as f32 * EXTERIOR_CELL_UNITS;
    let origin_y = by0 as f32 * EXTERIOR_CELL_UNITS;

    let n = (k as usize) * SAMPLES_PER_CELL + 1; // vertices per block edge

    let mut vertices: Vec<Vertex> = Vec::with_capacity(n * n);
    // Parallel validity mask. Resident full-detail cells still contribute
    // their authored seam vertices: the neighboring LOD cell needs those
    // shared positions to meet it exactly. Ownership is decided per quad
    // below; only genuinely missing landscape poisons a vertex.
    let mut holes: Vec<bool> = Vec::with_capacity(n * n);

    for r in 0..n {
        let (cdy, lrow) = sample_to_cell(r, k as usize);
        for c in 0..n {
            let (cdx, lcol) = sample_to_cell(c, k as usize);
            let gx = bx0 + cdx as i32;
            let gy = by0 + cdy as i32;

            let world_x = origin_x + c as f32 * VERT_SPACING;
            let world_y_zup = origin_y + r as f32 * VERT_SPACING;

            let land = cells_map
                .get(&(gx, gy))
                .and_then(|cell| cell.landscape.as_ref());

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

    // Indices: 2 triangles per LOD-owned quad. Ownership follows the cell
    // containing the quad's top-left sample, not the four vertices it
    // touches. The old `holes[tl] || ...` rule discarded the first 1024-BU
    // strip of every LOD cell adjacent to full detail because its shared seam
    // vertices belonged to both representations, producing a bright moat.
    // Same CW winding as full-detail terrain (the Z negate flips to CCW).
    let mut indices: Vec<u32> = Vec::with_capacity((n - 1) * (n - 1) * 6);
    let mut skirt_edges: Vec<(usize, usize, [f32; 3])> = Vec::new();
    for r in 0..(n - 1) {
        let (cdy, _) = sample_to_cell(r, k as usize);
        for c in 0..(n - 1) {
            let (cdx, _) = sample_to_cell(c, k as usize);
            let gx = bx0 + cdx as i32;
            let gy = by0 + cdy as i32;
            if cell_is_full_detail(gx, gy, resident_full_cells)
                || cells_map
                    .get(&(gx, gy))
                    .and_then(|cell| cell.landscape.as_ref())
                    .is_none()
            {
                continue;
            }
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
            // A skirt under every edge that borders a resident full-detail
            // cell (the neighbour may be in the next block, so go by cell).
            let spc = SAMPLES_PER_CELL as i32;
            let neighbour_is_full_detail = |dr: i32, dc: i32| {
                let (nr, nc) = (r as i32 + dr, c as i32 + dc);
                cell_is_full_detail(
                    bx0 + nc.div_euclid(spc),
                    by0 + nr.div_euclid(spc),
                    resident_full_cells,
                )
            };
            // Renderer axes: +x is east; LOD row r grows north (world +y), which
            // is renderer -z.
            if neighbour_is_full_detail(-1, 0) {
                skirt_edges.push((tl, tr, [0.0, 0.0, 1.0]));
            }
            if neighbour_is_full_detail(1, 0) {
                skirt_edges.push((bl, br, [0.0, 0.0, -1.0]));
            }
            if neighbour_is_full_detail(0, -1) {
                skirt_edges.push((tl, bl, [-1.0, 0.0, 0.0]));
            }
            if neighbour_is_full_detail(0, 1) {
                skirt_edges.push((tr, br, [1.0, 0.0, 0.0]));
            }
        }
    }

    if indices.is_empty() {
        return None; // entirely holes — nothing to draw
    }
    append_lod_skirts(&mut vertices, &mut indices, &skirt_edges);

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
    let combined = combined_lod_supported(game);
    let translated_lod = if legacy_landscape_lod_supported(game) {
        translate_terrain_lod_textures(game, worldspace_key, world_form_id, k, bx0, by0)
    } else if combined {
        // A boundary block stands in for its quad's prebaked `.btr`, so it
        // wears that quad's baked colours — not one tiled terrain texture,
        // which read as a differently coloured ring right at the join.
        Some(super::terrain_lod_btr::btr_quad_texture(
            worldspace_key,
            k,
            bx0,
            by0,
            game,
        ))
    } else {
        None
    };
    // #4912 — the authored legacy quad (Oblivion's FormID-keyed / FO3-FNV's
    // EditorID-keyed DDS) maps exactly its own block footprint, so it
    // samples with the authored `0 = CLAMP_S_CLAMP_T` — the WRAP default
    // bled each block's opposite edge in at its borders. The BTXT base-LTEX
    // fallback below is *tiled* like full-detail terrain and stays WRAP.
    let lod_quad_tex = translated_lod
        .as_ref()
        .map(|lod| resolve_texture_with_clamp(ctx, tex_provider, Some(lod.diffuse_path.as_str()), 0))
        .unwrap_or(0);
    let translated_lod = translated_lod
        .filter(|_| lod_quad_tex != 0 && lod_quad_tex != ctx.texture_registry.fallback());
    // Normal maps are vector data: linear upload, like the material slot — and
    // the authored quad's clamp (#4912). The combined family lists fallbacks
    // (FO4 `_msn`, then `_n`); the legacy families author exactly one.
    let normal_candidates: Vec<String> = match translated_lod.as_ref() {
        Some(_) if combined => super::terrain_lod_btr::btr_normal_path_candidates(
            worldspace_key,
            k,
            bx0,
            by0,
            game,
        ),
        Some(lod) => vec![lod.normal_path.clone()],
        None => Vec::new(),
    };
    let mut resolved_normal = 0;
    for candidate in &normal_candidates {
        let handle =
            resolve_linear_texture_with_clamp(ctx, tex_provider, Some(candidate.as_str()), 0);
        if handle != 0 && handle != ctx.texture_registry.fallback() {
            resolved_normal = handle;
            break;
        }
    }
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
        match synth_base_texture_path(layer_textures, base_ltex) {
            Some(path) => (resolve_texture(ctx, tex_provider, Some(path.as_str())), Some(path)),
            None => (0, None),
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
    //
    // #4898 — the image orientation over the footprint comes from the
    // translate contract (`uv_at`), not a hard-coded convention: FO3/FNV
    // quads run U west, Oblivion's U east, and both run V north.
    //
    // The baked normal maps are world-oriented (R = east, G = north), so the
    // shading frame must not follow the UV orientation. The explicit tangent
    // pins T = world east (+X); with w = +1 `perturbNormal` builds
    // B = cross(N, T) = -Z = world north for an up-facing N. That is the frame
    // the screen-derivative fallback used to build from the old
    // u = x, v = 1 - y mapping. Under the corrected mapping that fallback would
    // flip to (west, south) and invert the relief. (LAND's `new_terrain` uses
    // w = -1 because its own V is row-flipped; that sign does not apply here.)
    if let Some(lod) = translated_lod.as_ref() {
        for v in vertices.iter_mut() {
            v.uv = lod.uv_at(v.position[0], -v.position[2]);
            v.tangent = LOD_QUAD_WORLD_TANGENT;
        }
    }

    // Upload into the global geometry pool only (#1370). LOD blocks
    // rasterize from the global vertex/index buffer and enter the TLAS
    // only through the renderer's bounded camera-local shadow-caster
    // restore path (which builds BLAS on demand, #4180), so the per-mesh
    // buffers `upload_scene_mesh` would create are pure boot-time waste —
    // ~2 synchronous fence-waits + 2 tiny device-local sub-allocations
    // per block, ×hundreds of blocks. The geometry rides the single
    // `rebuild_geometry_ssbo` the frame loop already runs for the
    // resident scene.
    let mesh_handle = match ctx
        .mesh_registry
        .upload_scene_mesh_global_only(&vertices, &indices)
    {
        Ok(h) => h,
        Err(e) => {
            // #3406 — `{:#}` keeps anyhow's source chain.
            log::warn!(
                "Failed to upload LOD terrain block at cell ({},{}): {:#}",
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
    ctx.mesh_registry.note_mesh_provenance(
        mesh_handle,
        byroredux_renderer::MeshUploadSource::Lod,
        false,
        Some(&format!("lod-terrain({bx0},{by0})")),
    );

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
                // The combined family's DXT5 maps carry a signed authored blue
                // axis (alpha constant 255); `resolve_msn_z_source` keys off the
                // bound DDS, exactly as the prebaked `.btr` spawner does.
                normal_has_alpha: combined && ctx.texture_registry.handle_has_alpha(normal_texture_handle),
                // #4423 — synthetic paths bind no tint texture; see the field doc.
                tint_has_alpha: false,
                // #4444 — canonical defaults, not literals (#3073 doctrine:
                // a retune can't leave synthetic paths on the old numbers).
                parallax_height_scale:
                    byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE,
                parallax_max_passes:
                    byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES,
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
        // #4912 — the Material's clamp matches whichever arm resolved: the
        // authored legacy quad's CLAMP_S_CLAMP_T, or the tiled base LTEX's
        // WRAP (the plain wrapper's default).
        crate::material_translate::translate_texture_only_material_with_authored_msn(
            base_texture_path,
            // Every vanilla `.btr` normal map is model-space (see
            // `btr_normal_path_candidates`' census); only a bound map can be.
            combined && normal_texture_handle != 0,
            if translated_lod.is_some() { 0 } else { 3 },
        ),
    );
    if combined && normal_texture_handle != 0 {
        crate::material_translate::resolve_msn_z_source(world, entity);
    }
    // Architecture layer (zero depth bias) — same canonical baseline the
    // full-detail terrain uses.
    world.insert(entity, RenderLayer::Architecture);
    // Marker: routes through the static draw loop with `in_tlas = false`
    // and is skipped by any TLAS-membership logic.
    world.insert(entity, IsLodTerrain);

    let (hole_mask, rim_mask) = if level == LOD_BLOCK_CELLS {
        (
            block_hole_mask(cells_map, bx0, by0, resident_full_cells),
            block_rim_mask(bx0, by0, resident_full_cells),
        )
    } else {
        (0, 0)
    };
    Some(LodBlock {
        entity,
        mesh_handle,
        texture_handle: tex_handle,
        normal_texture_handle,
        hole_mask,
        rim_mask,
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

    /// #4898 — the baked-quad tangent must yield the world-oriented frame the
    /// legacy LOD normal maps encode (R = east, G = north), whatever the UV
    /// orientation. Mirrors `perturbNormalGrad`'s Path 1:
    /// `B = sign(w) * cross(N, T)`.
    #[test]
    fn lod_quad_tangent_frame_is_world_east_north() {
        let t = Vec3::new(
            LOD_QUAD_WORLD_TANGENT[0],
            LOD_QUAD_WORLD_TANGENT[1],
            LOD_QUAD_WORLD_TANGENT[2],
        );
        let sign = if LOD_QUAD_WORLD_TANGENT[3] < 0.0 { -1.0 } else { 1.0 };
        let up = Vec3::from(zup_to_yup_pos([0.0, 0.0, 1.0]));
        let east = Vec3::from(zup_to_yup_pos([1.0, 0.0, 0.0]));
        let north = Vec3::from(zup_to_yup_pos([0.0, 1.0, 0.0]));
        assert!((t - east).length() < 1e-6, "T {t:?} is not world east {east:?}");
        let b = sign * up.cross(t);
        assert!((b - north).length() < 1e-6, "B {b:?} is not world north {north:?}");
    }

    /// #6246 — an FO4 MNAM-only base LTEX (no TX00, so absent from
    /// `landscape_textures`) resolves its synth-block diffuse from the BGSM,
    /// like full-detail terrain; an ICON path still resolves directly.
    #[test]
    fn synth_block_base_resolves_an_mnam_only_ltex_through_its_bgsm() {
        use byroredux_bgsm::template::ResolvedMaterial;
        use byroredux_bgsm::BgsmFile;
        use byroredux_plugin::esm::cell::TextureSet;

        let bgsm = r"landscape\ground\lcwdefault01grass.bgsm";
        let mut provider = MaterialProvider::new();
        provider.insert_bgsm_for_test(
            bgsm,
            ResolvedMaterial {
                file: BgsmFile {
                    diffuse_texture: "Landscape/Ground/LCWDefault01Grass_d.dds".into(),
                    ..Default::default()
                },
                parent: None,
            },
        );
        const MNAM_ONLY: u32 = 0x0001_0001;
        const ICON_ONLY: u32 = 0x0001_0002;
        let sets = HashMap::from([(
            MNAM_ONLY,
            TextureSet { material_path: Some(bgsm.into()), ..TextureSet::default() },
        )]);
        let paths = HashMap::from([(ICON_ONLY, r"landscape\dirt01.dds".to_string())]);
        let mut layers = LandscapeLayerTextures {
            landscape_textures: &paths,
            landscape_texture_sets: &sets,
            mat_provider: Some(&mut provider),
        };
        assert_eq!(
            synth_base_texture_path(&mut layers, Some(MNAM_ONLY)).as_deref(),
            Some("Landscape/Ground/LCWDefault01Grass_d.dds")
        );
        assert_eq!(
            synth_base_texture_path(&mut layers, Some(ICON_ONLY)).as_deref(),
            Some(r"landscape\dirt01.dds")
        );
        assert_eq!(synth_base_texture_path(&mut layers, Some(0)), None);
        assert_eq!(synth_base_texture_path(&mut layers, None), None);
    }

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

    /// Skirts hang the drop below each bordering edge and face the hole side
    /// whichever way the edge runs.
    #[test]
    fn lod_skirts_hang_below_the_edge_and_face_the_hole() {
        let top = |x: f32, z: f32| Vertex::new([x, 100.0, z], [1.0, 1.0, 1.0], [0.0, 1.0, 0.0], [0.0, 0.0]);
        let face_normal = |vertices: &[Vertex], tri: &[u32]| {
            let p = |i: u32| vertices[i as usize].position;
            let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]
        };
        // The same physical edge given both ways round: the wall must face the
        // hole (+Z here) either way.
        for (a, b) in [(0, 1), (1, 0)] {
            let mut vertices = vec![top(0.0, 0.0), top(1024.0, 0.0)];
            let mut indices = Vec::new();
            append_lod_skirts(&mut vertices, &mut indices, &[(a, b, [0.0, 0.0, 1.0])]);
            assert_eq!(vertices.len(), 4, "two skirt vertices");
            assert_eq!(indices.len(), 6, "one wall of two triangles");
            for new in &vertices[2..] {
                assert_eq!(new.position[1], 100.0 - LOD_SKIRT_DROP);
            }
            for tri in indices.chunks(3) {
                let n = face_normal(&vertices, tri);
                assert!(n[2] > 0.0, "wall must face the hole: {n:?}");
            }
        }
        // And toward -X for a north-south edge.
        let mut vertices = vec![top(0.0, 0.0), top(0.0, -1024.0)];
        let mut indices = Vec::new();
        append_lod_skirts(&mut vertices, &mut indices, &[(0, 1, [-1.0, 0.0, 0.0])]);
        for tri in indices.chunks(3) {
            assert!(face_normal(&vertices, tri)[0] < 0.0);
        }
    }

    #[test]
    fn chebyshev_distance() {
        assert_eq!(chebyshev((0, 0), (0, 0)), 0);
        assert_eq!(chebyshev((3, 1), (0, 0)), 3);
        assert_eq!(chebyshev((-2, 5), (0, 0)), 5);
    }

    #[test]
    fn only_actual_residency_is_holed_from_lod() {
        let radius_load = 5;
        let radius_unload = radius_load + 1;
        let player = (3, -2);
        let resident: HashSet<_> = (-radius_load..=radius_load)
            .flat_map(|dx| {
                (-radius_load..=radius_load).map(move |dy| (player.0 + dx, player.1 + dy))
            })
            .collect();

        assert!(cell_is_full_detail(player.0, player.1, &resident));
        let never_loaded_hysteresis_cell = (player.0 + radius_unload, player.1);
        assert!(
            !cell_is_full_detail(
                never_loaded_hysteresis_cell.0,
                never_loaded_hysteresis_cell.1,
                &resident,
            ),
            "the possible unload-radius band is not full-detail ownership"
        );
    }

    #[test]
    fn hysteresis_resident_is_still_holed_until_it_unloads() {
        let stale_edge = (-6, 2);
        let resident = HashSet::from([stale_edge]);
        assert!(cell_is_full_detail(stale_edge.0, stale_edge.1, &resident));
        assert!(!cell_is_full_detail(-5, 2, &resident));
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

        // Oblivion keeps the fixed synthesized ring. FO3/FNV use their
        // authored legacy NIF/DDS ladder; Creation titles use `.btr`.
        assert_eq!(
            lod_ring_reach_cells(GameKind::Oblivion),
            LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS
        );
        assert!(lod_ring_reach_cells(GameKind::Skyrim) > LOD_RADIUS_BLOCKS * LOD_BLOCK_CELLS);
        assert!(lod_ring_reach_cells(GameKind::Fallout3NV) >= 64);
    }

    #[test]
    fn fallout_legacy_terrain_reaches_authored_coarse_bands() {
        let ladder = LodBandLadder::for_terrain_game(GameKind::Fallout3NV).unwrap();
        let quads = desired_lod_quads(
            Some(&ladder),
            (0, 0),
            (0, 0),
            6,
            Some(((-64, -64), (63, 63))),
            |_, _, _| false,
            |_, _, _| true,
        );
        assert!(quads.iter().any(|&(level, _, _)| level > LOD_BLOCK_CELLS));
        assert!(quads.iter().any(|&(level, _, _)| level == 32));
    }

    /// #2371 / #3100 / #3203 — the `None` arm: a game with no band ladder at
    /// all keeps the geometry ring it had before band selection existed — one
    /// finest-level ring, `LOD_RADIUS_BLOCKS` blocks deep, centred on the
    /// player's block. A regression here is a silently shrunken horizon.
    ///
    /// **Oblivion is now the only game that takes it.** This used to say
    /// "Oblivion / FO3 / FNV", which was true when `for_terrain_game`
    /// delegated straight to `for_game`; FO3/FNV do ship a baked 4/8/16/32
    /// `landscape\lod` ladder out to +/-64 cells and now get
    /// `LodBandLadder::fallout_legacy()` instead (#3203). The `None` passed
    /// below is therefore the arm under test, not a stand-in for those two —
    /// `fallout_legacy_terrain_reaches_authored_coarse_bands` covers them.
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

    /// The ladder honours `exclude_within`: nothing is emitted inside it. The
    /// production ring passes `-1` there (the finest band is holed, not
    /// dropped) and keeps coarse bands out through `refine_within` instead, which
    /// `coarse_btr_bands_stay_out_of_the_default_streaming_radius` and its legacy
    /// twin pin at the default radius (#6094).
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

        // #4505 — same premise for the FO3/FNV legacy NIF/DDS ladder
        // (`FALLOUT_LEGACY_REFINE_BU`: the finest band refines at 12
        // cells). The tightest configuration is `--radius 7`, where
        // `radius_unload` reaches 8 and an emitted level-8 quad persists
        // down to the full-detail boundary by one cell of margin; sweep
        // every playable radius so a future threshold retune that puts
        // coarse geometry over hysteresis-band cells fails here, not in
        // the world.
        let legacy = LodBandLadder::for_terrain_game(GameKind::Fallout3NV).unwrap();
        for radius_unload in 6..=8 {
            for py in 0..8 {
                for px in 0..8 {
                    let quads = desired_lod_quads(
                        Some(&legacy),
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
                            "legacy level-{level} quad ({qx}, {qy}) reaches inside \
                             radius_unload {radius_unload}"
                        );
                    }
                }
            }
        }
    }

    /// How many selected quads claim each cell of the window `±reach` around
    /// `player`, as `(cell, count)` for every cell not claimed exactly once.
    fn cells_not_covered_exactly_once(
        quads: &[(i32, i32, i32)],
        player: (i32, i32),
        reach: i32,
    ) -> Vec<((i32, i32), usize)> {
        let mut bad = Vec::new();
        for cy in (player.1 - reach)..=(player.1 + reach) {
            for cx in (player.0 - reach)..=(player.0 + reach) {
                let claims = quads
                    .iter()
                    .filter(|&&(level, qx, qy)| {
                        cx >= qx && cx < qx + level && cy >= qy && cy < qy + level
                    })
                    .count();
                if claims != 1 {
                    bad.push(((cx, cy), claims));
                }
            }
        }
        bad
    }

    /// #6094 — the band thresholds measure to a quad's centre and were tuned
    /// against a ~6-cell streaming radius. At the default radius 12
    /// (`radius_unload` 13) a coarse `.btr` quad whose centre clears its
    /// threshold still reaches into the resident ring, where it is drawn over
    /// full-detail terrain and — its hole mask being 0 — never holed. The
    /// forced subdivision must keep every coarse band out of the region and
    /// still tile the ground exactly once (no moat, no double draw).
    #[test]
    fn coarse_btr_bands_stay_out_of_the_default_streaming_radius() {
        let radius_unload = 13;
        for game in [GameKind::Skyrim, GameKind::Fallout4] {
            let ladder = LodBandLadder::for_game(game).unwrap();
            let mut reached_before_fix = false;
            for py in 0..16 {
                for px in 0..16 {
                    let player = (px, py);
                    let desired = |refine: i32| {
                        desired_lod_quads_refining(
                            Some(&ladder),
                            player,
                            (0, 0),
                            -1,
                            refine,
                            None,
                            |_, _, _| false,
                            |_, _, _| true,
                        )
                    };
                    // The premise: without the forced subdivision some position
                    // really does emit a coarse quad inside the ring.
                    reached_before_fix |= desired(-1).iter().any(|&(level, qx, qy)| {
                        level > LOD_BLOCK_CELLS
                            && quad_min_chebyshev(qx, qy, level, player) <= radius_unload
                    });

                    let quads = desired(radius_unload);
                    for &(level, qx, qy) in &quads {
                        assert!(
                            level == LOD_BLOCK_CELLS
                                || quad_min_chebyshev(qx, qy, level, player) > radius_unload,
                            "{game:?} level-{level} quad ({qx}, {qy}) reaches inside \
                             radius_unload {radius_unload} from {player:?}"
                        );
                    }
                    let bad = cells_not_covered_exactly_once(&quads, player, radius_unload + 10);
                    assert!(
                        bad.is_empty(),
                        "{game:?} from {player:?}: cells not tiled exactly once \
                         (moat or double draw): {:?}",
                        &bad[..bad.len().min(6)]
                    );
                }
            }
            assert!(
                reached_before_fix,
                "{game:?}: the radius-13 premise is gone — without the fix no coarse \
                 quad reaches the ring, so this test no longer exercises #6094"
            );
        }
    }

    /// #6094 — the FO3 / FNV authored ladder has no hysteresis and no
    /// full-detail exclusion, so a level-8 quad was selected once its centre was
    /// 12+ cells away while its nearest cell sat ~10 away. A coarse authored quad
    /// reaching the ring must be skipped and its ground tiled by the finer
    /// authored quads, with the level-4 synth completion as the net.
    #[test]
    fn coarse_authored_bands_stay_out_of_the_default_streaming_radius() {
        let radius_unload = 13;
        let ladder = LodBandLadder::for_terrain_game(GameKind::Fallout3NV).unwrap();
        // A full authored tiling at every level, on the (0,0) lattice.
        let mut authored = Vec::new();
        for level in [4, 8, 16, 32] {
            let span = 96 / level;
            for j in -span..span {
                for i in -span..span {
                    authored.push((level, i * level, j * level));
                }
            }
        }
        let mut reached_before_fix = false;
        for py in [0, 3, 5, 7, 11] {
            for px in [0, 2, 6, 9, 13] {
                let player = (px, py);
                let select = |refine: i32| {
                    legacy_desired_lod_quads(&authored, &ladder, player, None, &|_, _| false, refine)
                };
                reached_before_fix |= select(-1).iter().any(|&(level, qx, qy)| {
                    level > LOD_BLOCK_CELLS
                        && quad_min_chebyshev(qx, qy, level, player) <= radius_unload
                });
                let quads = select(radius_unload);
                for &(level, qx, qy) in &quads {
                    assert!(
                        level == LOD_BLOCK_CELLS
                            || quad_min_chebyshev(qx, qy, level, player) > radius_unload,
                        "legacy level-{level} quad ({qx}, {qy}) reaches inside \
                         radius_unload {radius_unload} from {player:?}"
                    );
                }
                // The ring itself is tiled exactly once, and nothing just past it
                // is left bare (a moat). The legacy selection also double-covers
                // ground from ~14 cells out, with or without this fix (#6053, its
                // immediate-parent seam test), so exactness is asserted only where
                // this change acts.
                let ring = cells_not_covered_exactly_once(&quads, player, radius_unload);
                assert!(
                    ring.is_empty(),
                    "legacy from {player:?}: ring cells not tiled exactly once: {:?}",
                    &ring[..ring.len().min(6)]
                );
                let moat: Vec<_> =
                    cells_not_covered_exactly_once(&quads, player, radius_unload + 6)
                        .into_iter()
                        .filter(|&(_, claims)| claims == 0)
                        .collect();
                assert!(
                    moat.is_empty(),
                    "legacy from {player:?}: ground left bare past the ring: {:?}",
                    &moat[..moat.len().min(6)]
                );
            }
        }
        assert!(
            reached_before_fix,
            "the radius-13 premise is gone: without the fix no coarse authored quad \
             reaches the ring, so this test no longer exercises #6094"
        );
    }

    /// #6409 — a skirt looks at the neighbouring CELL, which can sit in the next block, so a block
    /// must regenerate when a resident cell appears just outside it even though none of its own
    /// 16 cells changed. The hole mask alone cannot see that; the rim mask does.
    #[test]
    fn a_resident_cell_in_the_next_block_changes_the_rim_not_the_hole_mask() {
        let k = LOD_BLOCK_CELLS;
        let resident_at = |cell: (i32, i32)| move |gx: i32, gy: i32| (gx, gy) == cell;
        // Block (0,0); the resident region advances onto the first cell of the block east of it.
        let before = (
            assemble_hole_mask(0, 0, resident_at((-50, -50))),
            assemble_rim_mask(0, 0, resident_at((-50, -50))),
        );
        let after = (
            assemble_hole_mask(0, 0, resident_at((k, 1))),
            assemble_rim_mask(0, 0, resident_at((k, 1))),
        );
        assert_eq!(before.0, after.0, "none of the block's own cells changed");
        assert_eq!(before.1, 0);
        assert_ne!(before.1, after.1, "the rim sees the neighbour block's cell");
        // Every outside cell has its own bit, and no cell inside the block sets one.
        let mut seen = 0u16;
        for d in 0..k {
            for cell in [(-1, d), (k, d), (d, -1), (d, k)] {
                let bit = assemble_rim_mask(0, 0, resident_at(cell));
                assert_eq!(bit.count_ones(), 1, "{cell:?}");
                assert_eq!(seen & bit, 0, "{cell:?} shares a bit");
                seen |= bit;
            }
        }
        assert_eq!(seen, u16::MAX, "all 16 rim cells are tracked");
        for dy in 0..k {
            for dx in 0..k {
                assert_eq!(assemble_rim_mask(0, 0, resident_at((dx, dy))), 0, "inside ({dx},{dy})");
            }
        }
        // Corner-diagonal cells never face a quad edge.
        assert_eq!(assemble_rim_mask(0, 0, resident_at((-1, -1))), 0);
        assert_eq!(assemble_rim_mask(0, 0, resident_at((k, k))), 0);
    }

    /// #6409 — a prebaked `.btr` block has no skirts, so it is used only when no cell of it is
    /// holed and no neighbour is resident.
    #[test]
    fn a_block_beside_resident_ground_takes_the_synth_path() {
        assert!(block_may_use_prebaked(0, 0));
        assert!(!block_may_use_prebaked(1, 0), "a holed cell needs the synth path");
        assert!(!block_may_use_prebaked(0, 1), "a resident neighbour needs a skirt");
        assert!(!block_may_use_prebaked(3, 8));
    }

    /// #6409 — the streaming step keys both the rebuild and the attempted-miss cache on the pair,
    /// and gates the `.btr` path on it.
    #[test]
    fn the_ring_rebuilds_and_gates_on_the_hole_and_rim_pair() {
        let production = byroredux_core::source_scan::production_text(include_str!("terrain_lod.rs"));
        let stream = &production[production.find("pub(crate) fn stream_lod_blocks(").unwrap()..];
        for needle in [
            "(b.hole_mask, b.rim_mask)",
            "Some(m) if m == (mask, rim)",
            "Some(&(mask, rim))",
            "missing_blocks.insert((level, qx, qy), (mask, rim))",
            "block_may_use_prebaked(mask, rim)",
        ] {
            assert!(stream.contains(needle), "stream_lod_blocks lost: {needle}");
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
    /// #5222 — the legacy terrain completion: ground no authored quad
    /// touches gets a level-4 synth tile, ground an authored quad covers
    /// does not (no overlap, no double-draw).
    #[test]
    fn legacy_completion_covers_only_unauthored_ground() {
        let ladder = LodBandLadder::for_terrain_game(byroredux_plugin::esm::reader::GameKind::Fallout3NV)
            .expect("FO3/FNV terrain ladder");
        let player = (0, 0);
        // One authored level-16 quad well east of the player, on an
        // off-(0,0) lattice position for good measure.
        let authored = vec![(16, 20, 4)];
        let selected = legacy_desired_lod_quads(&authored, &ladder, player, None, &|_, _| false, -1);

        assert!(selected.contains(&(16, 20, 4)), "the authored quad is selected");
        // The player's own ground is served by a completion tile at the
        // finest level — the pre-#5222 synth guarantee.
        assert!(
            selected
                .iter()
                .any(|&(level, qx, qy)| level == LOD_BLOCK_CELLS
                    && player.0 >= qx
                    && player.0 < qx + level
                    && player.1 >= qy
                    && player.1 < qy + level),
            "uncovered ground must get a level-4 synth tile"
        );
        // No completion tile overlaps the authored quad.
        for &(level, qx, qy) in &selected {
            if level == LOD_BLOCK_CELLS {
                assert!(
                    qx + level <= 20 || 20 + 16 <= qx || qy + level <= 4 || 4 + 16 <= qy,
                    "completion tile ({qx}, {qy}) overlaps the authored quad — \
                     that would double-draw distant terrain"
                );
            }
        }
        // And when the authored set covers everything in reach, no
        // completion tile fires: every selection is an authored quad.
        let mut full = Vec::new();
        for qy in -20..20i32 {
            for qx in -20..20i32 {
                full.push((4, qx * 4, qy * 4));
            }
        }
        let selected_full = legacy_desired_lod_quads(&full, &ladder, player, None, &|_, _| false, -1);
        assert!(
            selected_full.iter().all(|q| full.contains(q)),
            "a complete authored tiling needs no invented completion tiles"
        );
        // Every in-reach authored quad is selected (the ones beyond the
        // ring's 64-cell reach are legitimately absent).
        assert_eq!(
            selected_full.len(),
            full.iter()
                .filter(|&&(l, qx, qy)| quad_min_chebyshev(qx, qy, l, player) <= ladder.max_cells())
                .count(),
            "the in-reach authored tiling must be selected exactly"
        );
    }
}

