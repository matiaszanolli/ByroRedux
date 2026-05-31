//! Distant-terrain LOD blocks — engine-generated coarse meshes that extend
//! exterior view distance far beyond the full-detail streamed ring.
//!
//! Each LOD block merges a `LOD_BLOCK_CELLS × LOD_BLOCK_CELLS` square of
//! exterior cells into ONE decimated mesh sampled from the worldspace
//! heightmaps already parsed at load time (`ExteriorWorldContext`). A block:
//!   * samples each cell's 33×33 grid at `STRIDE` (`SAMPLES_PER_CELL` quads
//!     per cell edge) — ~1/64th the triangles of full-detail terrain,
//!   * uses a single base ground texture (no per-vertex splat blend),
//!   * builds **no BLAS** and spawns with [`IsLodTerrain`] so the renderer
//!     keeps it out of the TLAS — distant terrain needs no RT shadows/GI,
//!     costing zero ray-tracing budget,
//!   * holes out any cell inside the full-detail load radius so the LOD
//!     never overlaps / z-fights the streamed near terrain.
//!
//! Slice 1 (this module) loads the ring **once** around the spawn cell via
//! [`spawn_lod_ring`]. Streaming the ring as the player walks (load/unload
//! blocks on cell-boundary crossings) is the follow-up slice.

use std::collections::HashMap;

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::{
    GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::cell::CellData;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::IsLodTerrain;

use super::exterior::ExteriorWorldContext;

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

/// Spawn the distant-terrain LOD ring around the player's spawn cell
/// `(cx, cy)`. Builds one merged mesh per `LOD_BLOCK_CELLS²` block out to
/// `LOD_RADIUS_BLOCKS`, holing out cells inside `full_radius_load` (those
/// render at full detail). Returns the number of LOD blocks spawned.
pub(crate) fn spawn_lod_ring(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &ExteriorWorldContext,
    cx: i32,
    cy: i32,
    full_radius_load: i32,
) -> usize {
    let index = &wctx.record_index.cells;
    let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
        return 0;
    };
    let k = LOD_BLOCK_CELLS;
    // Block containing the player. `div_euclid` floors toward negative
    // infinity so blocks tile consistently across the origin.
    let pbi = cx.div_euclid(k);
    let pbj = cy.div_euclid(k);

    let mut spawned = 0;
    for bj in (pbj - LOD_RADIUS_BLOCKS)..=(pbj + LOD_RADIUS_BLOCKS) {
        for bi in (pbi - LOD_RADIUS_BLOCKS)..=(pbi + LOD_RADIUS_BLOCKS) {
            if spawn_lod_block(
                world,
                ctx,
                tex_provider,
                &index.landscape_textures,
                cells_map,
                bi,
                bj,
                cx,
                cy,
                full_radius_load,
            ) {
                spawned += 1;
            }
        }
    }
    log::info!(
        "LOD terrain ring: {} blocks ({}×{} cells each) out to {} blocks (~{:.0}K BU)",
        spawned,
        k,
        k,
        LOD_RADIUS_BLOCKS,
        (LOD_RADIUS_BLOCKS * k) as f32 * EXTERIOR_CELL_UNITS / 1000.0,
    );
    spawned
}

/// Build + spawn one LOD block at block-coords `(bi, bj)`. Returns `false`
/// (nothing spawned) when the block is entirely holes — every cell either
/// missing, landscape-less, or inside the full-detail radius.
#[allow(clippy::too_many_arguments)]
fn spawn_lod_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    cells_map: &HashMap<(i32, i32), CellData>,
    bi: i32,
    bj: i32,
    cx: i32,
    cy: i32,
    full_radius_load: i32,
) -> bool {
    let k = LOD_BLOCK_CELLS;
    let bx0 = bi * k; // SW cell column of the block
    let by0 = bj * k; // SW cell row of the block
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
            let full_detail = chebyshev((gx, gy), (cx, cy)) <= full_radius_load;
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
                    [world_x, 0.0, -world_y_zup],
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
            // path (center at 128, then (nx, nz, -ny)).
            let normal = if let Some(ref nml) = land.normals {
                let ni = li * 3;
                let nx = (nml[ni] as f32 - 128.0) / 127.0;
                let ny = (nml[ni + 1] as f32 - 128.0) / 127.0;
                let nz = (nml[ni + 2] as f32 - 128.0) / 127.0;
                let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
                [nx / len, nz / len, -ny / len]
            } else {
                [0.0, 1.0, 0.0]
            };

            // Tile the diffuse `LAND_TEXTURE_TILES_PER_CELL` times per cell,
            // matching the full-detail terrain (`terrain.rs`) so the LOD seam
            // at the full-detail boundary tiles identically. `c /
            // SAMPLES_PER_CELL` is the cell-fraction along the block edge.
            let uv = [
                c as f32 / SAMPLES_PER_CELL as f32 * super::terrain::LAND_TEXTURE_TILES_PER_CELL,
                (1.0 - r as f32 / SAMPLES_PER_CELL as f32) * super::terrain::LAND_TEXTURE_TILES_PER_CELL,
            ];

            vertices.push(Vertex::new(
                [world_x, height, -world_y_zup],
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
        return false; // entirely holes — nothing to draw
    }

    if ctx.allocator.is_none() {
        return false;
    }

    // World-space bound from the emitted (non-hole) vertices for frustum
    // culling. Computed over all pushed vertices; hole placeholders sit at
    // y=0 inside the block footprint, so they never enlarge the sphere
    // beyond the real terrain extent.
    let bound = block_bound(&vertices, &holes);

    // Base ground texture: first BTXT base LTEX among the block's cells,
    // resolved via LTEX → TXST → diffuse path. `Some(0)` = UESP "default
    // dirt"; absent → dirt fallback (matches the full-detail terrain base).
    let base_ltex = (0..k)
        .flat_map(|dy| (0..k).map(move |dx| (dx, dy)))
        .find_map(|(dx, dy)| {
            cells_map
                .get(&(bx0 + dx, by0 + dy))
                .and_then(|cell| cell.landscape.as_ref())
                .and_then(|land| land.quadrants.iter().find_map(|q| q.base))
        });
    let tex_handle = match base_ltex {
        Some(0) | None => {
            resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
        }
        Some(ltex_id) => match landscape_textures.get(&ltex_id) {
            Some(path) => resolve_texture(ctx, tex_provider, Some(path.as_str())),
            None => resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds")),
        },
    };

    // Upload into the global geometry pool with `rt_enabled = false`: no
    // per-mesh BLAS-input buffers, no BLAS spec pushed → the mesh rasterizes
    // from the global vertex/index buffer but never enters the TLAS.
    let allocator = ctx.allocator.as_ref().unwrap();
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &vertices,
        &indices,
        false,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("Failed to upload LOD terrain block ({},{}): {}", bi, bj, e);
            return false;
        }
    };

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    world.insert(entity, bound);
    // Architecture layer (zero depth bias) — same canonical baseline the
    // full-detail terrain uses.
    world.insert(entity, RenderLayer::Architecture);
    // Marker: routes through the static draw loop with `in_tlas = false`
    // and is skipped by any TLAS-membership logic.
    world.insert(entity, IsLodTerrain);

    true
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

    #[test]
    fn chebyshev_distance() {
        assert_eq!(chebyshev((0, 0), (0, 0)), 0);
        assert_eq!(chebyshev((3, 1), (0, 0)), 3);
        assert_eq!(chebyshev((-2, 5), (0, 0)), 5);
    }
}
