# REN-D3-2026-08-07-01: GpuTerrainTile is a hand-mirrored GPU struct with no size, offset, or lockstep pin

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2463
**Finding ID**: REN-D3-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 3 — GPU-Struct Layout
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:9` (`GpuTerrainTile`) ↔ `crates/renderer/shaders/include/bindings.glsl:322` (`struct GpuTerrainTile`)
**Status**: NEW

## Description
`GpuTerrainTile` is a `#[repr(C)]` struct uploaded to the set 1 / binding 10 SSBO and hand-mirrored in GLSL, exactly like `GpuInstance`, `GpuMaterial`, `GpuLight` and (since a prior finding was fixed) `GpuFogVolume`. Unlike all of those it has **no** `size_of` pin, **no** `offset_of!` pin, **no** GLSL↔Rust lockstep test, and **no** `.spv` reflection pin. `grep -rn "GpuTerrainTile" crates/renderer/src` returns only use sites — buffer sizing, upload, and a debug scratch row — never an assertion. The only thing coupling the two declarations today is a comment.

## Evidence
Rust `[u32;8] × 3` = 96 B; shipped `triangle.frag.spv` carries `OpDecorate %_runtimearr_GpuTerrainTile_0 ArrayStride 96` with members at 0 / 32 / 64 — currently correct. The buffer is sized from the **Rust** side (`buffers.rs:456 size_of::<GpuTerrainTile>() * MAX_TERRAIN_TILES`) and the upload memcpy uses the Rust stride (`upload.rs:786`), while the shader indexes with the **GLSL** stride. Adding a 4th layer role (e.g. `layer_glow_index: [u32;8]`, a natural next step for LAND splatting) on the Rust side alone makes the two strides 128 vs 96 and every tile from index 1 onward reads misaligned bindless texture indices.

## Impact
Silent per-tile corruption of terrain splat texture indices across every exterior cell (wrong/garbage diffuse-normal-specular layers, or index-0 placeholder). Fails no test, fails no validation layer — the SSBO byte count is legal either way. Blast radius = all outdoor rendering.

## Related
Same defect class as `GpuFogVolume` (fixed, `AUDIT_RENDERER_2026-08-02.md`) and #1657 / SF-D8-01 (`GpuMaterial` order guard).

## Suggested Fix
Add `size_of::<GpuTerrainTile>() == 96` + `offset_of!` pins in `gpu_instance_layout_tests.rs`, and extend the existing `strip_struct_body`/`extract_struct_body` helpers already in that file to cross-check the GLSL `struct GpuTerrainTile` field list against the Rust one.

## Completeness Checks
- [ ] **TESTS**: New `size_of`/`offset_of!` pin test added, plus GLSL↔Rust lockstep cross-check
