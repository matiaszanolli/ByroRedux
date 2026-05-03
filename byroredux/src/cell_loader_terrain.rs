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
use byroredux_plugin::esm;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::TerrainTileSlot;

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

    let mut by_ltex: HashMap<u32, (u16, [Option<Vec<f32>>; 4])> = HashMap::new();
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
    let mut sorted: Vec<(u32, u16, [Option<Vec<f32>>; 4])> = by_ltex
        .into_iter()
        .map(|(ltex, (layer, slots))| (ltex, layer, slots))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // Bethesda's authoring tool caps at 8 per UESP, but modded content
    // (TTW, Project Nevada, DLC merges) has been observed going higher.
    if sorted.len() > 8 {
        log::warn!(
            "Terrain cell has {} splat layers, capping at 8 (dropping {} with highest `layer` field). #470",
            sorted.len(),
            sorted.len() - 8,
        );
        sorted.truncate(8);
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
pub(super) fn spawn_terrain_mesh(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    grid_x: i32,
    grid_y: i32,
    land: &esm::cell::LandscapeData,
    blas_specs: &mut Vec<(u32, u32, u32)>,
) -> Option<usize> {
    const CELL_SIZE: f32 = 4096.0;
    const GRID: usize = 33;
    const SPACING: f32 = CELL_SIZE / 32.0; // 128.0

    let origin_x = grid_x as f32 * CELL_SIZE;
    let origin_y = grid_y as f32 * CELL_SIZE;

    // Collect cell-global splat layers before the vertex loop — we need
    // all 8 resolved before we can pack per-vertex weights. #470.
    let splat_layers = build_cell_splat_layers(ctx, tex_provider, landscape_textures, land);

    // Build vertices (33×33 = 1089).
    let mut vertices = Vec::with_capacity(GRID * GRID);
    for row in 0..GRID {
        for col in 0..GRID {
            let idx = row * GRID + col;

            // World-space position (Z-up → Y-up conversion).
            let bx = origin_x + col as f32 * SPACING;
            let by = origin_y + row as f32 * SPACING;
            let bz = land.heights[idx];
            let position = [bx, bz, -by];

            // Normal: VNML bytes are unsigned 0–255, center at 128 = zero.
            // Bethesda Z-up: X, Y, Z → convert to Y-up: (nx, nz, -ny).
            let normal = if let Some(ref nml) = land.normals {
                let ni = idx * 3;
                let nx = (nml[ni] as f32 - 128.0) / 127.0;
                let ny = (nml[ni + 1] as f32 - 128.0) / 127.0;
                let nz = (nml[ni + 2] as f32 - 128.0) / 127.0;
                let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
                [nx / len, nz / len, -ny / len]
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

            let uv = [col as f32 / 32.0, 1.0 - row as f32 / 32.0];

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

    let allocator = ctx.allocator.as_ref()?;
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
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
