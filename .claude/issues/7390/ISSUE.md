# #7390 — REN-D3-2026-11-23-01: stale terrain-tile and ray-budget size prose after the 224 → 240 B growth (names a test that no longer exists)

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D3-01). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (doc-rot)
- **Dimension**: GPU-Struct Layout
- **Location**: `include/terrain_tile.glsl` (the std430 comment above `coverAffinity0` names `gpu_terrain_tile_is_224_bytes` and "208..224"; the `baseCoverAffinity` doc says "the 224-byte std430 array stride"; the `lodSoftenEnd` doc "keeps the record at its 224-byte … stride"), `scene_buffer/upload.rs` (`upload_terrain_tiles`, the SAFETY comment: "whose std430 size `gpu_terrain_tile_is_224_bytes` pins"), `scene_buffer/constants.rs` (`MAX_TERRAIN_TILES` doc: "1024 × 224 B = 224 KB"; `RAY_BUDGET_STRIDE` doc: "payload is currently 68 bytes"), `scene_buffer/buffers.rs` (`allocate_scene_render_buffers`: "Terrain tile SSBO: 224 B per slot"), `scene_buffer/gpu_instance_layout_tests.rs` (`gpu_terrain_tile_field_offsets_match_shader_contract`: "Matches `OpDecorate … ArrayStride 224` + members at …"), `docs/engine/shader-pipeline.md` (the terrain-tile blend paragraph: `byroTerrainHeightBlend` (`include/bindings.glsl`)), `scripts/gen_env_brdf_lut.py` + generated `scene_buffer/env_brdf_lut.rs` header ("the same text as the GLSL table": `env_brdf_lut.glsl` carries no table since the table moved to binding 26).
- **Status**: NEW (the 11-16 report's REN-D3-01 was a different rustdoc and is closed #7031)
- **Description**: `d4fcb4c69` (TSD-4.3, 224 → 240 B) renamed the size test to `gpu_terrain_tile_is_240_bytes` and updated the doc tables, the struct doc, `memory-budget.md` and the SKILL, but left the prose above. `GpuRayBudget` is 80 B (20 words) per `shader_contract_fits_in_the_aligned_slot`, the doc says 68. `byroTerrainHeightBlend` moved to `include/terrain_tile.glsl` in `84f04205f`.
- **Evidence**: `grep -rn "gpu_terrain_tile_is_224\|is_224_bytes"` hits only `terrain_tile.glsl` and `upload.rs`; `grep -rn "224" scene_buffer/*.rs context/shrink_frame_scratch.rs` lists the rest (`shrink_frame_scratch.rs` also carries "x 224 B" in a descriptive table string).
- **Impact**: none at runtime; the next person following a SAFETY comment looks for a test that is gone and the shader header teaches the wrong stride.
- **Related**: #7271 (same class for `GpuCamera`, closed); the `gpu_material_size_claims` scanner reads `.claude/commands/` and anchored doc rows only, not Rust/GLSL comments for `GpuTerrainTile`.
- **Suggested Fix**: replace the 224 spellings with 240 (or "history" phrasing with an arrow), rename the test reference, change "68 bytes" to 80, point the doc at `include/terrain_tile.glsl`, and reword the generator's header comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
