# REN-D3-01: GpuFogVolume has a size/align pin but no field-order Rust↔GLSL lockstep test

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2228

**Dimension**: 3 (GPU structs)
**Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`GpuFogVolume`, line 127; size/align asserts at line 2009-2010); `crates/renderer/shaders/volumetrics_inject.comp` (`struct GpuFogVolume`, line 124)
**Status**: NEW

**Description**: The new `GpuFogVolume` struct (64 B) has tests pinning `size_of`/`align_of` but nothing checking field ORDER against the GLSL struct — the exact gap class `feedback_shader_struct_sync.md` exists to close (this repo has been burned before by GLSL/Rust struct drift post-split, per `GpuInstance`/`GpuMaterial` history). Fields currently agree (`center_shape`, `half_extents_extinction`, `inverse_rotation`, `albedo_edge` in both), but nothing would catch a future reorder.

**Evidence**: `volumetrics.rs:2009-2016` only asserts `size_of`/`align_of`/`size_of::<GpuFogVolumeUpload>` — no per-field offset check against the GLSL layout at `volumetrics_inject.comp:124-129`.

**Impact**: A future field reorder on either side (Rust or GLSL) would compile clean and pass the existing size/align asserts while silently corrupting every fog-volume sample on the GPU.

**Suggested Fix**: add a lockstep test (or reuse whatever mechanism guards `GpuInstance`/`GpuMaterial`) asserting each `GpuFogVolume` field's byte offset matches its GLSL struct position.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
