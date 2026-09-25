# #4849 — REN-D3-2026-09-24-04: three kinds of `every_shader_struct_is_classified` entry do not say what is true

**Labels**: bug,renderer,low,test-gap
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: LOW (every affected struct is currently correct by hand diff)
- **Dimension**: GPU-Struct Layout
- **Location**: `scene_buffer/shader_contract_tests.rs` — `every_shader_struct_is_classified` (`GcModelPoint`, `GcDrawIndexed`, `GroundCoverSpecies`, `GroundCoverChunk`, `GroundCoverCell`).
- **Description**:
  1. `GcModelPoint` and `GcDrawIndexed` are `ShaderLocal`, but the host sizes buffers from hand-written byte strides (`POINT_BYTES = 32`, `DRAW_STRIDE = 20` in `groundcover_models.rs`); `host_mirrors_match_the_shader_strides` pins `DRAW_STRIDE` against `vk::DrawIndexedIndirectCommand` but neither against GLSL, so growing `GcModelPoint` in GLSL would write past the slab with no test failure.
  2. `GroundCoverSpecies` is `MirroredPendingGuard("sibling of GroundCoverCell")` although `GpuGroundCoverSpecies` has no pad fields and replaying the comparator's rules gives 5/5 matches, so it can be `Guarded` today.
  3. `GroundCoverChunk` / `GroundCoverCell` name the wrong blocker: GLSL now declares the pads; the real blockers are a missing `vec2`/`uvec2` type row, a name alias (`active` <-> `slotActive`) and `pad1: [f32;2]` vs `pad1, pad2`.
- **Suggested Fix**: Move `GcModelPoint` / `GcDrawIndexed` to `MirroredPendingGuard` (or pin the strides with `std430_struct_size`), flip `GroundCoverSpecies` to `Guarded`, and restate the Chunk/Cell blockers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
