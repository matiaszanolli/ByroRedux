# #7376 — REN-D3-2026-11-23-02: no explicit offset assertion for the four TSD-4.3 patch lanes (@224..236); the SPIR-V stride test reads one of five shaders

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D3-02). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (test gap; layout is correct today, held implicitly)
- **Dimension**: GPU-Struct Layout
- **Location**: `gpu_terrain_tile_field_offsets_match_shader_contract`, `shipped_triangle_frag_spv_carries_the_terrain_tile_stride` (`scene_buffer/gpu_instance_layout_tests.rs`)
- **Status**: NEW
- **Description**: the offset test ends at `lod_soften_end` @220 and still says "ArrayStride 224"; `patch_amplitude`, `patch_fade_start`, `patch_fade_end` and `patch_parallax` are checked only through `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep` (22 names in order) + the 240 B size. That does fix the offsets (four same-width scalars in the same order at the tail), so there is no hole today, but an insertion that keeps 22 fields and 240 B (e.g. swapping a pad) would not fail an offset test. The SPIR-V stride test only opens `triangle.frag.spv`; `groundcover_blade.frag`, `water.frag` and `terrain_patch.comp` read the same array (I measured 240 in all four plus `triangle_early`).
- **Evidence**: `offset_of!(GpuTerrainTile, lod_soften_end) = 220` is the last assertion; `grep patch_amplitude gpu_instance_layout_tests.rs` = only the size message.
- **Impact**: a future reorder inside the TSD-4.3 block is caught only by name order, and a stale `terrain_patch.comp.spv` would only be caught by the CI shader-artifact script.
- **Related**: #7152, #7221.
- **Suggested Fix**: add `assert_eq!(offset_of!(GpuTerrainTile, patch_amplitude), 224)` … `patch_parallax = 236`, and loop `shipped_*_spv_carries_the_terrain_tile_stride` over the five `include_bytes!` SPIR-V files.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix
