//! Exterior LAND heightmap → terrain mesh conversion.
//!
//! Each exterior cell's `LAND` record carries a 33×33 vertex grid spanning
//! 4096×4096 Bethesda units (128-unit vertex spacing) plus up to 8 texture
//! splat layers per UESP's LAND format spec. This module turns that
//! authoring data into a GPU mesh + ECS entity, including per-vertex splat
//! weights packed into 2×RGBA8 attributes for the fragment shader.
//!
//! Coordinate conversion (Bethesda Z-up → renderer Y-up):
//!   `world_x = grid_x * 4096 + col * 128`     → X
//!   `world_z = heights[row][col]`             → Y (up)
//!   `world_y = grid_y * 4096 + row * 128`     → −Z (negate for Y-up)
//!
//! See `#470` for the splat-layer landing.

use std::collections::HashMap;

use byroredux_core::ecs::{GlobalTransform, MeshHandle, TextureHandle, Transform, World};
use byroredux_core::math::coord::{zup_to_yup_pos, EXTERIOR_CELL_UNITS};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::TerrainTileSlot;

/// Number of times a LAND diffuse/splat texture tiles across one exterior
/// cell edge. Bethesda's LAND format carries no per-texture tiling field, so
/// this is a fixed engine constant: the diffuse repeats `2 × quad-textures-
/// per-side` times per cell. Per openmw's ESM4 (Oblivion+) terrain
/// (`Storage::getTextureTileCount` → `2 * ESM4::Land::sQuadTexturePerSide`,
/// with `sQuadTexturePerSide = 6`) that is **12** — 2 quadrants per cell side
/// × 6 texture tiles per quadrant. Pre-fix the UV ran 0→1 across the whole
/// 4096-BU cell (≈1 texel / 16 BU), so every exterior surface read as a
/// blurry gray average regardless of mip level; the texture has to tile so
/// near terrain shows real detail. `Lod` terrain reuses this same factor
/// (`terrain_lod`) so the seam at the full-detail boundary tiles identically.
pub(super) const LAND_TEXTURE_TILES_PER_CELL: f32 = 12.0;

/// Resolved terrain splat layers for one cell — up to 8 cell-global layers,
/// each with its bindless texture handle and the per-quadrant alpha grids
/// contributed by every quadrant that painted that LTEX. Produced by
/// [`build_cell_splat_layers`] and consumed by the vertex packer in
/// [`spawn_terrain_mesh`]. See #470.
#[derive(Default)]
pub(super) struct CellSplatLayers {
    /// 0–8 entries sorted by ascending `layer_sort_key`, then by
    /// `ltex_form_id` for deterministic tiebreak.
    layers: Vec<CellSplatLayer>,
}

pub(super) struct CellSplatLayer {
    /// Bindless texture handle (resolved via LTEX → TXST → diffuse path).
    /// 0 means the texture failed to load; fragment shader skips (index 0
    /// is the fallback checkerboard).
    pub texture_index: u32,
    /// Per-quadrant contribution. `[SW, SE, NW, NE]` — `None` means the
    /// quadrant didn't paint this LTEX. Each `Some` is a 17×17 alpha grid.
    pub per_quadrant_alpha: [Option<Vec<f32>>; 4],
}

/// Per-quadrant alpha grids for one LTEX. `[SW, SE, NW, NE]` matches
/// `CellSplatLayer::per_quadrant_alpha`; `None` means that quadrant
/// didn't paint this LTEX.
type PerQuadrantAlpha = [Option<Vec<f32>>; 4];

/// Collect cell-global splat layers from the 4 quadrants. Dedup by
/// `ltex_form_id`; take the minimum `layer` field as the sort key so seam
/// vertices across quadrants resolve to the same cell-global layer. Caps
/// at 8 per UESP's LAND format spec; excess is dropped with a warning.
pub(super) fn build_cell_splat_layers(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    land: &esm::cell::LandscapeData,
) -> CellSplatLayers {
    use std::collections::hash_map::Entry;

    let mut by_ltex: HashMap<u32, (u16, PerQuadrantAlpha)> = HashMap::new();
    for (q_idx, q) in land.quadrants.iter().enumerate() {
        for l in &q.layers {
            let Some(ref alpha) = l.alpha else {
                // Malformed ATXT without VTXT — nothing to paint. #470.
                log::debug!(
                    "Terrain quadrant {}: ATXT LTEX {:08X} layer {} has no VTXT; skipped",
                    q_idx,
                    l.ltex_form_id,
                    l.layer
                );
                continue;
            };
            match by_ltex.entry(l.ltex_form_id) {
                Entry::Vacant(v) => {
                    let mut slots: [Option<Vec<f32>>; 4] = Default::default();
                    slots[q_idx] = Some(alpha.clone());
                    v.insert((l.layer, slots));
                }
                Entry::Occupied(mut o) => {
                    let (min_layer, slots) = o.get_mut();
                    if l.layer < *min_layer {
                        *min_layer = l.layer;
                    }
                    // Merge into the same quadrant slot — rare; one LTEX
                    // per quadrant is the vanilla pattern.
                    if let Some(existing) = slots[q_idx].as_mut() {
                        for (dst, src) in existing.iter_mut().zip(alpha.iter()) {
                            *dst = dst.max(*src);
                        }
                    } else {
                        slots[q_idx] = Some(alpha.clone());
                    }
                }
            }
        }
    }

    // Sort by (layer_sort_key, ltex_form_id) for deterministic order.
    let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = by_ltex
        .into_iter()
        .map(|(ltex, (layer, slots))| (ltex, layer, slots))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // Bethesda's authoring tool caps at 8 per UESP, but Skyrim
    // routinely ships cells with 9-12 layers and modded content
    // (TTW, Project Nevada, DLC merges) goes higher. The 8-cap is
    // a real shader-side limit — `vertex.rs::Vertex` packs splat
    // weights as 2× RGBA8 = 8 channels per vertex.
    //
    // Pre-fix this dropped the highest `layer` field values, but
    // `layer` is just authoring order — not visual importance. A
    // tiny trim decal authored last would survive while a dominant
    // ground texture authored first got dropped if the budget was
    // hit. Coverage-aware policy picks the 8 layers with the most
    // painted area across all quadrants, then re-sorts for
    // deterministic GPU ordering. #470.
    if sorted.len() > 8 {
        let drop_count = sorted.len() - 8;
        log::warn!(
            "Terrain cell has {} splat layers, capping at 8 (dropping {} with smallest total coverage). #470",
            sorted.len(),
            drop_count,
        );
        select_top_8_by_coverage(&mut sorted);
    }

    let mut layers = Vec::with_capacity(sorted.len());
    for (ltex, _layer_key, per_quadrant_alpha) in sorted {
        let texture_index = if let Some(tex_path) = landscape_textures.get(&ltex) {
            resolve_texture(ctx, tex_provider, Some(tex_path.as_str()))
        } else {
            log::debug!(
                "Terrain splat: LTEX {:08X} not in landscape_textures map; skipping layer",
                ltex
            );
            0
        };
        layers.push(CellSplatLayer {
            texture_index,
            per_quadrant_alpha,
        });
    }

    CellSplatLayers { layers }
}

/// In-place coverage-aware selection of the top 8 splat layers from
/// `sorted`. Computes total painted alpha across all quadrants per
/// layer, keeps the 8 highest-coverage layers, then re-sorts those 8
/// by `(layer, ltex_form_id)` so the GPU vertex-attribute layer-index
/// ordering stays deterministic across runs.
///
/// Pure function — no Vulkan, no allocator — so it's unit-testable
/// without a real cell. #470.
///
/// Precondition: `sorted.len() > 8` (called only when the cap is
/// exceeded; the no-op case is gated at the call site).
fn select_top_8_by_coverage(sorted: &mut Vec<(u32, u16, PerQuadrantAlpha)>) {
    // Coverage = sum of alpha values across all painted quadrants.
    // f64 accumulator handles the worst case (4 quadrants × 17×17 =
    // 1156 floats × 1.0 = 1156.0) without precision drift even when
    // many cells stack up across a session.
    sorted.sort_by(|a, b| {
        let ca = total_coverage(&a.2);
        let cb = total_coverage(&b.2);
        // Descending coverage; partial_cmp is safe because alpha values
        // from the ATXT parser are finite [0, 1] f32s — NaN cannot
        // appear. Default to Equal on the impossible None branch.
        cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted.truncate(8);
    // Re-sort by (layer, ltex) so the shader's per-layer-index access
    // pattern stays consistent with the no-cap path — pre-fix every
    // caller assumed `(layer ascending, ltex ascending)` ordering.
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
}

/// Sum of alpha values across all painted quadrants for one layer.
/// Higher = more painted area = more visually important. Used by the
/// coverage-aware splat cap to drop the least-impactful layers when
/// a cell exceeds the 8-channel vertex budget. #470.
fn total_coverage(per_quadrant_alpha: &PerQuadrantAlpha) -> f64 {
    per_quadrant_alpha
        .iter()
        .filter_map(|q| q.as_ref())
        .flat_map(|q| q.iter())
        .map(|&v| v as f64)
        .sum()
}

/// Map a global 33×33 `(row, col)` to the list of contributing
/// `(quadrant_index, local_row_in_17, local_col_in_17)` tuples. Most
/// vertices belong to exactly one quadrant; edges belong to two, corners
/// to four. Sentinel `0xFF` in slot 0 of `q` means "unused" — caller
/// checks `q < 4` to decide whether to sample.
pub(super) fn quadrant_samples_for_vertex(row: usize, col: usize) -> [(u8, u8, u8); 4] {
    let mut out = [(0xFFu8, 0u8, 0u8); 4];
    let mut n = 0;
    // SW (0): rows [0..=16], cols [0..=16].
    if row <= 16 && col <= 16 {
        out[n] = (0, row as u8, col as u8);
        n += 1;
    }
    // SE (1): rows [0..=16], cols [16..=32]. Local col = col-16.
    if row <= 16 && col >= 16 {
        out[n] = (1, row as u8, (col - 16) as u8);
        n += 1;
    }
    // NW (2): rows [16..=32], cols [0..=16]. Local row = row-16.
    if row >= 16 && col <= 16 {
        out[n] = (2, (row - 16) as u8, col as u8);
        n += 1;
    }
    // NE (3): rows [16..=32], cols [16..=32].
    if row >= 16 && col >= 16 {
        out[n] = (3, (row - 16) as u8, (col - 16) as u8);
        n += 1;
    }
    let _ = n;
    out
}

/// Sample one splat weight for a global vertex by taking the max across
/// every contributing quadrant's alpha grid. Absent quadrants contribute
/// 0. Returns a u8 ready to pack into the vertex attribute.
pub(super) fn splat_weight_for_vertex(layer: &CellSplatLayer, row: usize, col: usize) -> u8 {
    let samples = quadrant_samples_for_vertex(row, col);
    let mut best = 0.0_f32;
    for (q, lr, lc) in samples {
        if q >= 4 {
            continue;
        }
        let Some(ref alpha) = layer.per_quadrant_alpha[q as usize] else {
            continue;
        };
        let local_idx = (lr as usize) * 17 + (lc as usize);
        if local_idx < alpha.len() {
            best = best.max(alpha[local_idx]);
        }
    }
    (best.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Generate a terrain mesh from LAND heightmap data and spawn it as an
/// entity. The mesh participates in the global geometry SSBO so RT
/// reflection / GI rays sample the right vertex data — using
/// `upload_scene_mesh` (not plain `upload`) is mandatory; see #371.
#[allow(clippy::too_many_arguments)]
/// Which splat-layer texture indices [`release_splat_layer_textures`] drops:
/// skips `0` (LTEX not resolved → never acquired) and the registry
/// `fallback` slot (shared placeholder, never per-cell refcounted) — the
/// same skip rule the unload-side `free_terrain_tile` → `push_tex_drop`
/// sweep uses. Pure so the release set is unit-testable without a
/// `VulkanContext`. (#1343)
fn splat_indices_to_release(indices: &[u32], fallback: u32) -> Vec<u32> {
    indices
        .iter()
        .copied()
        .filter(|&i| i != 0 && i != fallback)
        .collect()
}

/// Release the per-layer splat texture refcounts acquired by
/// [`build_cell_splat_layers`]. Called only on `spawn_terrain_mesh`'s
/// early-return paths (no allocator / mesh-upload failure) — the success
/// path hands these handles to a `TerrainTileSlot` whose `free_terrain_tile`
/// drops them on cell unload, so calling this there would double-release.
/// (#1343)
fn release_splat_layer_textures(ctx: &mut VulkanContext, indices: &[u32]) {
    let fallback = ctx.texture_registry.fallback();
    for idx in splat_indices_to_release(indices, fallback) {
        ctx.texture_registry.drop_texture(&ctx.device, idx);
    }
}

/// Renderer-side borrows shared by terrain spawning: the Vulkan context,
/// texture provider, the cell's landscape-texture lookup, and the BLAS spec
/// sink the caller batches builds through. Grouped to keep
/// [`spawn_terrain_mesh`]'s argument count in check.
pub(super) struct TerrainSpawnCtx<'a> {
    pub ctx: &'a mut VulkanContext,
    pub tex_provider: &'a TextureProvider,
    pub landscape_textures: &'a HashMap<u32, String>,
    pub blas_specs: &'a mut Vec<(u32, u32, u32)>,
}

pub(super) fn spawn_terrain_mesh(
    world: &mut World,
    spawn: TerrainSpawnCtx,
    grid_x: i32,
    grid_y: i32,
    land: &esm::cell::LandscapeData,
) -> Option<usize> {
    let TerrainSpawnCtx {
        ctx,
        tex_provider,
        landscape_textures,
        blas_specs,
    } = spawn;
    const GRID: usize = 33;
    const SPACING: f32 = EXTERIOR_CELL_UNITS / 32.0; // 128.0

    let origin_x = grid_x as f32 * EXTERIOR_CELL_UNITS;
    let origin_y = grid_y as f32 * EXTERIOR_CELL_UNITS;

    // Collect cell-global splat layers before the vertex loop — we need
    // all 8 resolved before we can pack per-vertex weights. #470.
    let splat_layers = build_cell_splat_layers(ctx, tex_provider, landscape_textures, land);
    // #1343 — `build_cell_splat_layers` acquired (refcounted) one texture per
    // splat layer above, but those handles only reach an unload-droppable
    // owner at `allocate_terrain_tile` below. If we bail before that (no
    // allocator / mesh-upload failure), release them here so the refcount +
    // bindless slot don't leak. Snapshot the indices now so the release
    // doesn't re-borrow `splat_layers` (still needed by the vertex loop).
    let splat_tex_indices: Vec<u32> = splat_layers
        .layers
        .iter()
        .map(|l| l.texture_index)
        .collect();

    // Build vertices (33×33 = 1089).
    let mut vertices = Vec::with_capacity(GRID * GRID);
    for row in 0..GRID {
        for col in 0..GRID {
            let idx = row * GRID + col;

            // World-space position (Z-up → Y-up conversion via the
            // canonical helper, #1753).
            let bx = origin_x + col as f32 * SPACING;
            let by = origin_y + row as f32 * SPACING;
            let bz = land.heights[idx];
            let position = zup_to_yup_pos([bx, by, bz]);

            // Normal: VNML bytes are unsigned 0–255, center at 128 = zero.
            // Bethesda Z-up → Y-up via the canonical helper; per-component
            // normalise commutes with the axis swap (#1753).
            let normal = if let Some(ref nml) = land.normals {
                let ni = idx * 3;
                let nx = (nml[ni] as f32 - 128.0) / 127.0;
                let ny = (nml[ni + 1] as f32 - 128.0) / 127.0;
                let nz = (nml[ni + 2] as f32 - 128.0) / 127.0;
                let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
                zup_to_yup_pos([nx / len, ny / len, nz / len])
            } else {
                [0.0, 1.0, 0.0]
            };

            let color = if let Some(ref vc) = land.vertex_colors {
                let ci = idx * 3;
                [
                    vc[ci] as f32 / 255.0,
                    vc[ci + 1] as f32 / 255.0,
                    vc[ci + 2] as f32 / 255.0,
                ]
            } else {
                [1.0, 1.0, 1.0]
            };

            // Tile the diffuse/splat textures `LAND_TEXTURE_TILES_PER_CELL`
            // times across the cell (REPEAT sampler) so near terrain shows
            // real texel detail instead of one stretched texture. The
            // per-vertex splat WEIGHTS (splat0/splat1 below) are unaffected —
            // they're interpolated attributes, not UV-sampled.
            let uv = [
                col as f32 / 32.0 * LAND_TEXTURE_TILES_PER_CELL,
                (1.0 - row as f32 / 32.0) * LAND_TEXTURE_TILES_PER_CELL,
            ];

            // Pack up to 8 splat weights into 2× RGBA8 unorm (#470).
            let mut splat0 = [0u8; 4];
            let mut splat1 = [0u8; 4];
            for (i, layer) in splat_layers.layers.iter().enumerate() {
                let w = splat_weight_for_vertex(layer, row, col);
                if i < 4 {
                    splat0[i] = w;
                } else {
                    splat1[i - 4] = w;
                }
            }

            vertices.push(Vertex::new_terrain(
                position, color, normal, uv, splat0, splat1,
            ));
        }
    }

    // Indices: 32×32 quads × 2 triangles. The Z-up → Y-up transform
    // negates Z, flipping winding — emit CW so it becomes CCW (Vulkan
    // front face) after the coordinate conversion.
    let mut indices = Vec::with_capacity(32 * 32 * 6);
    for row in 0..32u32 {
        for col in 0..32u32 {
            let tl = row * GRID as u32 + col;
            let tr = tl + 1;
            let bl = (row + 1) * GRID as u32 + col;
            let br = bl + 1;
            indices.push(tl);
            indices.push(tr);
            indices.push(bl);
            indices.push(tr);
            indices.push(br);
            indices.push(bl);
        }
    }

    if ctx.allocator.is_none() {
        // #1343 — release the splat-layer refcounts before bailing; no
        // `TerrainTileSlot` will be allocated to carry them to unload.
        release_splat_layer_textures(ctx, &splat_tex_indices);
        return None;
    }
    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator: ctx.allocator.as_ref().unwrap(), // non-None checked just above
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        upload_ctx,
        &vertices,
        &indices,
        ctx.device_caps.ray_query_supported,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!(
                "Failed to upload terrain mesh ({},{}): {}",
                grid_x,
                grid_y,
                e
            );
            // #1343 — release the splat-layer refcounts before bailing.
            release_splat_layer_textures(ctx, &splat_tex_indices);
            return None;
        }
    };

    // Resolve terrain base texture: pick the first available BTXT from
    // any quadrant, resolve via LTEX → texture path. Per-quadrant BTXT
    // disagreement is handled best-effort — the chosen base wins on its
    // own quadrants and the ATXT splat layers paint the rest. See #470
    // (D7 follow-up).
    let tex_handle = {
        let base_ltex = land.quadrants.iter().find_map(|q| q.base);
        if let Some(ltex_id) = base_ltex {
            if ltex_id == 0 {
                // BTXT with form ID 0 = "default dirt" per UESP.
                resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
            } else if let Some(tex_path) = landscape_textures.get(&ltex_id) {
                resolve_texture(ctx, tex_provider, Some(tex_path.as_str()))
            } else {
                log::debug!(
                    "Terrain ({},{}): LTEX {:08X} not in landscape_textures map",
                    grid_x,
                    grid_y,
                    ltex_id,
                );
                0
            }
        } else {
            resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
        }
    };

    // Allocate a terrain tile slot only when the cell actually has splat
    // layers. BTXT-only cells skip this and render with the pre-#470
    // single-texture path for free. The slot is freed in `unload_cell`
    // via `VulkanContext::free_terrain_tile_slot`.
    let terrain_tile_index = if !splat_layers.layers.is_empty() {
        let mut indices_arr = [0u32; 8];
        for (i, layer) in splat_layers.layers.iter().enumerate() {
            indices_arr[i] = layer.texture_index;
        }
        ctx.allocate_terrain_tile(indices_arr)
    } else {
        None
    };

    // Queue BLAS build into the caller's batched-spec list — terrain
    // must be in the TLAS for RT shadows/GI, but we collapse N submits
    // into one batched build downstream of the loop. See #382.
    if ctx.device_caps.ray_query_supported {
        blas_specs.push((mesh_handle, vertices.len() as u32, indices.len() as u32));
    }

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    if let Some(slot) = terrain_tile_index {
        world.insert(entity, TerrainTileSlot(slot));
    }
    // #renderlayer — terrain LAND tiles ARE the architectural floor
    // everything else stacks on. Explicit Architecture (zero bias) so
    // the depth-bias ladder treats them as the canonical baseline,
    // not as defaulted-by-omission entities (which would also yield
    // Architecture but obscures the intent).
    world.insert(
        entity,
        byroredux_core::ecs::components::RenderLayer::Architecture,
    );

    // ...and being the floor, it is also collision. Terrain goes through the
    // exact same collider synthesis as every other static mesh — Gamebryo
    // treated exterior landscape as a separate physics subsystem from
    // interior `bhk` bodies, but that distinction buys us nothing and only
    // created a class of geometry that rendered without being solid.
    //
    // The tile's vertices are already world-space Y-up (`zup_to_yup_pos`
    // above) and the render entity sits at `Transform::IDENTITY`, so the
    // ghost takes an identity placement at unit scale and its collider
    // lands exactly on the drawn surface.
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();
    if !crate::cell_loader::spawn::spawn_trimesh_collider_ghost(
        world,
        &positions,
        &indices,
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
    ) {
        log::warn!(
            "Terrain ({},{}): collider synthesis produced no triangles — \
             tile renders but is not solid",
            grid_x,
            grid_y,
        );
    }

    log::debug!(
        "Terrain mesh ({},{}): {} verts, {} tris, height range {:.0}–{:.0}",
        grid_x,
        grid_y,
        vertices.len(),
        indices.len() / 3,
        land.heights.iter().cloned().fold(f32::INFINITY, f32::min),
        land.heights
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max),
    );

    Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1343 / D3-02 — on a `spawn_terrain_mesh` early return (no allocator /
    /// mesh-upload failure) the acquired splat-layer textures must be
    /// released, but `0` (unresolved LTEX, never acquired) and the registry
    /// `fallback` slot must be skipped so we don't over-release a shared
    /// slot. Same skip rule as the unload-side `free_terrain_tile` sweep.
    #[test]
    fn splat_release_skips_zero_and_fallback() {
        let fallback = 99u32;
        // 8 layers: real handles, one unresolved (0), one fallback.
        let indices = [10u32, 0, 11, fallback, 12, 0, 13, fallback];
        let mut got = splat_indices_to_release(&indices, fallback);
        got.sort_unstable();
        assert_eq!(
            got,
            vec![10, 11, 12, 13],
            "only real, non-fallback splat handles are released"
        );
    }

    /// A BTXT-only cell (no ATXT splat layers) acquired nothing → releases
    /// nothing on an early return.
    #[test]
    fn splat_release_empty_is_empty() {
        assert!(splat_indices_to_release(&[], 99).is_empty());
        assert!(splat_indices_to_release(&[0, 0, 0, 0], 99).is_empty());
    }

    /// Build a layer tuple with one painted quadrant filled to a
    /// constant alpha value. `alpha = 0.0` produces a zero-coverage
    /// layer; `alpha = 1.0` produces full coverage in that quadrant.
    fn layer(
        ltex: u32,
        layer_field: u16,
        q_idx: usize,
        alpha: f32,
    ) -> (u32, u16, PerQuadrantAlpha) {
        let mut slots: PerQuadrantAlpha = Default::default();
        slots[q_idx] = Some(vec![alpha; 17 * 17]);
        (ltex, layer_field, slots)
    }

    #[test]
    fn coverage_drops_zero_paint_layers_first() {
        // 12 layers — 8 with full coverage (alpha=1.0) and 4 with
        // zero coverage (alpha=0.0). Coverage-aware policy should
        // drop the 4 zero-coverage layers and keep the 8 painted
        // ones, regardless of the `layer_field` (authoring order).
        //
        // Pre-fix this dropped by `layer_field` ascending — the 4
        // zero-coverage layers at field=8..11 would have been kept
        // and 4 of the painted layers at field=4..7 would have been
        // dropped, producing a visually-broken terrain cell.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        for i in 0..8u32 {
            // High-coverage layers, low `layer_field` values.
            sorted.push(layer(0xC000_0000 + i, i as u16, (i % 4) as usize, 1.0));
        }
        for i in 0..4u32 {
            // Zero-coverage layers, high `layer_field` values — pre-fix
            // these would have survived the truncation.
            sorted.push(layer(
                0xD000_0000 + i,
                (8 + i) as u16,
                (i % 4) as usize,
                0.0,
            ));
        }
        // Sort matches the call-site state at entry to select_top_8_by_coverage.
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        assert_eq!(sorted.len(), 12);

        select_top_8_by_coverage(&mut sorted);

        assert_eq!(sorted.len(), 8);
        // Every survivor should be a high-coverage layer (LTEX 0xC0..).
        for (ltex, _, _) in &sorted {
            assert!(
                *ltex >= 0xC000_0000 && *ltex < 0xD000_0000,
                "zero-coverage layer 0x{:08X} survived the cap",
                ltex
            );
        }
    }

    #[test]
    fn coverage_keeps_dominant_layers_drops_trim() {
        // Realistic Skyrim pattern: 4 dominant ground textures
        // (grass, dirt, rock, snow) at high coverage + 6 trim
        // decorations (paths, decals, edge blends) at low coverage.
        // Total = 10 layers; 2 must drop. Expect both dropped to
        // be from the trim group.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        // 4 dominant: full coverage in one quadrant each.
        for i in 0..4u32 {
            sorted.push(layer(0xA000_0000 + i, i as u16, (i % 4) as usize, 1.0));
        }
        // 6 trim: 10% coverage in one quadrant each.
        for i in 0..6u32 {
            sorted.push(layer(
                0xB000_0000 + i,
                (4 + i) as u16,
                (i % 4) as usize,
                0.1,
            ));
        }
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        assert_eq!(sorted.len(), 10);

        select_top_8_by_coverage(&mut sorted);

        assert_eq!(sorted.len(), 8);
        // All 4 dominant layers must survive.
        let surviving_dominant = sorted
            .iter()
            .filter(|(ltex, _, _)| *ltex >= 0xA000_0000 && *ltex < 0xB000_0000)
            .count();
        assert_eq!(surviving_dominant, 4, "a dominant layer was dropped");
        // 4 of the 6 trim layers should survive (the policy is
        // order-insensitive within the trim group since they all
        // have identical coverage — any 4 is correct).
        let surviving_trim = sorted
            .iter()
            .filter(|(ltex, _, _)| *ltex >= 0xB000_0000 && *ltex < 0xC000_0000)
            .count();
        assert_eq!(surviving_trim, 4, "wrong number of trim layers survived");
    }

    #[test]
    fn output_is_resorted_by_layer_after_truncation() {
        // After the coverage-based selection, the survivors must be
        // re-sorted by (layer_field, ltex) for deterministic GPU
        // ordering — the shader's per-layer-index access pattern
        // expects this. Verifies the second sort runs.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        // 9 layers, distinct layer_field values 100..108, all full
        // coverage. The selection by coverage is a tie — every
        // layer has identical coverage — so the dropped one is
        // implementation-defined, but the survivors must be sorted
        // ascending by layer_field on output.
        for i in 0..9u16 {
            sorted.push(layer(0xE000_0000 + i as u32, 100 + i, 0, 1.0));
        }
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        select_top_8_by_coverage(&mut sorted);

        assert_eq!(sorted.len(), 8);
        // Verify sorted ascending by layer_field.
        for w in sorted.windows(2) {
            assert!(
                w[0].1 < w[1].1 || (w[0].1 == w[1].1 && w[0].0 < w[1].0),
                "output not sorted by (layer_field, ltex_form_id)"
            );
        }
    }

    #[test]
    fn total_coverage_sums_across_quadrants() {
        // Layer painted in 3 of 4 quadrants — 0.5 alpha each, 17×17
        // cells per quadrant. Expected = 3 × 17 × 17 × 0.5 = 433.5.
        let mut slots: PerQuadrantAlpha = Default::default();
        slots[0] = Some(vec![0.5; 17 * 17]);
        slots[1] = Some(vec![0.5; 17 * 17]);
        slots[3] = Some(vec![0.5; 17 * 17]);
        // slots[2] = None — unpainted quadrant contributes zero.

        let cov = total_coverage(&slots);
        let expected = 3.0 * (17 * 17) as f64 * 0.5;
        assert!(
            (cov - expected).abs() < 1e-9,
            "expected {expected}, got {cov}"
        );
    }

    #[test]
    fn total_coverage_zero_for_empty_layer() {
        let slots: PerQuadrantAlpha = Default::default();
        assert_eq!(total_coverage(&slots), 0.0);
    }
}
