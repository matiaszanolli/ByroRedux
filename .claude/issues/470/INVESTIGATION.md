# #470 Investigation (M32 Phase 2.5)

## Root cause confirmed

- `crates/plugin/src/esm/cell.rs:855-961` produces a rich
  `LandscapeData` with per-quadrant BTXT + up to N additional
  `TerrainTextureLayer`s, each carrying a 17×17 `alpha: Vec<f32>`.
- `byroredux/src/cell_loader.rs::spawn_terrain_mesh` collapsed the
  entire data structure down to
  `quadrants.iter().find_map(|q| q.base)` — one texture per cell.

## Fix (full description in `/home/matias/.claude/plans/twinkly-percolating-wigderson.md`)

Splat-map rendering via:

1. `Vertex` grew 76 → 84 bytes with two `R8G8B8A8_UNORM` attributes at
   locations 6/7 (8 weight channels total).
2. `GpuInstance.flags` reused — bit 3 = terrain splat, bits 16-31 =
   tile index. No `GpuInstance` resize, zero Shader Struct Sync churn.
3. New `GpuTerrainTile { layer_texture_index: [u32; 8] }` SSBO at
   scene set 1, binding 10 — `MAX_TERRAIN_TILES = 1024` (32 KB).
4. Fragment shader sequentially alpha-mixes up to 8 layers on top of
   BTXT when the splat bit is set; static meshes fall through the
   pre-#470 path cost-free.
5. Cell loader packs per-vertex splat weights via
   `quadrant_samples_for_vertex` + `splat_weight_for_vertex`. Seams
   resolve via max-of-two across contributing quadrants.
6. `VulkanContext::allocate_terrain_tile` /
   `VulkanContext::free_terrain_tile` manage the 1024-slot free list;
   `drain_terrain_tile_uploads` replicates the slab across every
   frame-in-flight via a countdown.
7. `unload_cell` frees tile slots **before** mesh / BLAS drop so late
   frames-in-flight read stale-but-valid data.

## Tests

Six unit tests in `cell_loader.rs::terrain_splat_tests`:
- `splat_quantization_full_and_empty_map_to_boundary_bytes`
- `splat_seam_reconciliation_takes_max_across_quadrants`
- `quadrant_samples_classify_corner_as_four_way`
- `quadrant_samples_interior_vertex_belongs_to_single_quadrant`
- `splat_round_trip_through_u8_preserves_half_alpha_within_tolerance`
- `splat_absent_quadrant_yields_zero`

Plus the updated `Vertex` size + offset tests (84 B, new splat offsets
76 and 80).

Total workspace: **901 tests pass**, zero failures.

## Files changed

7 source files + 2 SPIR-V recompiles:

- `crates/renderer/src/vertex.rs` — grow Vertex, new `new_terrain` ctor.
- `crates/renderer/src/vulkan/scene_buffer.rs` — new `GpuTerrainTile`,
  binding 10, `upload_terrain_tiles`, `INSTANCE_FLAG_*` constants.
- `crates/renderer/src/vulkan/context/mod.rs` —
  `DrawCommand::terrain_tile_index`, terrain-tile registry fields.
- `crates/renderer/src/vulkan/context/resources.rs` —
  `allocate_terrain_tile` / `free_terrain_tile` /
  `drain_terrain_tile_uploads`.
- `crates/renderer/src/vulkan/context/draw.rs` — INSTANCE_FLAG_*
  consumption + per-frame terrain-tile SSBO upload.
- `crates/renderer/shaders/triangle.vert` — splat attribute inputs.
- `crates/renderer/shaders/triangle.frag` — STRIDE 19→21, splat
  blend branch, binding 10 SSBO.
- `crates/renderer/shaders/ui.vert` — `flags` doc comment lockstep.
- `byroredux/src/components.rs` — `TerrainTileSlot` component.
- `byroredux/src/render.rs` — `terrain_tile_index` plumb-through.
- `byroredux/src/cell_loader.rs` — `build_cell_splat_layers`,
  `splat_weight_for_vertex`, `quadrant_samples_for_vertex`,
  `Vertex::new_terrain` in `spawn_terrain_mesh`, tile-slot free in
  `unload_cell`, 6 regression tests.

## Deferred (noted in plan)

- Per-layer normal maps (Skyrim+ content only).
- Dynamic splat painting (CK/editor tool).
- LOD terrain splat handling (M35 Phase).
- Texture address mode audit (REPEAT vs CLAMP_TO_EDGE).
- Per-quadrant BTXT disagreement → implicit layers (D7 in plan — edge
  case on modded cells; SW BTXT currently chosen as cell-global).
