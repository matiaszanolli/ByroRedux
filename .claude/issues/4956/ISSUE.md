# #4956: REN-D3-2026-09-27-05: `GcDrawIndirect` is classified `ShaderLocal`, but the host sizes and strides its buffer from `sizeof(VkDrawIndirectCommand)` — by the table's own rule it is a counterpart

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4956
- **Labels**: low,renderer,shaders,terrain-exterior,test-gap,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-05**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (mirror classification)
- **Location**:
  - `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:1895` (`("GcDrawIndirect", ShaderLocal)` in `every_shader_struct_is_classified`)
  - `crates/renderer/src/vulkan/groundcover.rs:713` (`indirect: GROUNDCOVER_MAX_CHUNKS as u64 * 16 * GROUNDCOVER_INDIRECT_STREAMS`)
  - `groundcover.rs:1817` (`cmd_draw_indirect`, stride 16)
  - GLSL `groundcover_scatter.comp:67` (`struct GcDrawIndirect`)
- **Status**: NEW
- **Description**: `MirrorClass::ShaderLocal`'s doc (#4849) says that a byte-stride constant the host sizes a buffer from IS a counterpart. Such a struct must be `Guarded` by a test that pins its std430 size to that constant. Its sibling `GcDrawIndexed` was moved to `Guarded`, pinned to `size_of::<vk::DrawIndexedIndirectCommand>()`.
  - `GcDrawIndirect` is in the identical position: the scatter writes it and `vkCmdDrawIndirect` consumes it at a literal stride of 16. It stayed `ShaderLocal`.
  - `indirect_stride_matches_the_command` only asserts `size_of::<vk::DrawIndirectCommand>() == 16` (the ash side). No test reflects or parses the GLSL struct.
  - A misplaced doc comment makes this harder to spot. `groundcover.rs:2161` says "`cmd_draw_indirect` is issued with a hard-coded stride of 16…", but it sits on `push_block_fits_the_guaranteed_minimum`, not on the stride test.
- **Evidence**: See the locations. `every_shader_struct_is_classified`'s `ShaderLocal` check looks only for a Rust struct named `GcDrawIndirect` or `GpuGcDrawIndirect`, so the ash counterpart is invisible to it.
- **Impact**: The risk is low: the Vulkan struct is fixed by spec and the GLSL is four `uint`s. Still, the classification vouches for something false, and a field added to the GLSL struct would desynchronise the per-chunk stride with no failing test.
- **Related**: #4849 (closed), `GcDrawIndexed`'s guard.
- **Suggested Fix**: Reclassify it as `Guarded("name_diverging_glsl_rust_mirrors_stay_in_lockstep")`, with a stride row pinning the GLSL std430 size to `size_of::<vk::DrawIndirectCommand>()`. Replace the literal `16` with that `size_of`, and move the misplaced doc comment onto `indirect_stride_matches_the_command`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
