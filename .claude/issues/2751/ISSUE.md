# REN-D12-2026-08-12-01: indirect draw loop not clamped to MAX_INDIRECT_DRAWS, batch overflow reads past the indirect buffer

- **Severity**: MEDIUM
- **Dimension**: 12 — Command buffer recording
- **Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs` (`record_geometry_pass`, the `while i < batches.len()` draw loop) vs. `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_indirect_draws`)
- **Description**: `upload_indirect_draws` clamps its write to `draws.len().min(MAX_INDIRECT_DRAWS)` and logs a one-shot warn on overflow (same policy as `upload_instances`/`MAX_INSTANCES`, #647/RP-1). The consumer has no matching clamp: the draw loop walks the full `batches` slice, so when `batches.len() > MAX_INDIRECT_DRAWS` the recorded call names a range beyond the buffer's allocation. `indirect_buffers[frame]` is sized exactly `size_of::<VkDrawIndexedIndirectCommand>() * MAX_INDIRECT_DRAWS`, and `MAX_INDIRECT_DRAWS == MAX_INSTANCES == 0x40000`. Same failure class #2504 closed on the upload-failure axis, left open on the overflow axis.
- **Evidence**: Producer: `let count = draws.len().min(MAX_INDIRECT_DRAWS);` with a one-shot warn. Consumer: `while i < batches.len() { … cmd_draw_indexed_indirect(...); }` with no bound check. `should_use_indirect_draws(global_bound, multi_draw_indirect_supported, indirect_upload_ok)` gates the path but has no count limb.
- **Impact**: If reached, violates VUID-vkCmdDrawIndexedIndirect-offset-00556, GPU fetches from unallocated memory — device-lost class. Reachability needs >262,144 post-merge rasterized batches in one frame (~20× the densest cell cited in the codebase's own comments, "12k DrawCommands"). MEDIUM (defence-in-depth at an already-declared overflow ceiling) not HIGH.
- **Related**: #2504, #647/RP-1, #309, #1581/F1.
- **Suggested Fix**: Clamp the loop bound to `batches.len().min(MAX_INDIRECT_DRAWS)` when `use_indirect` is true, or fold a count limb into `should_use_indirect_draws` so an overflowing frame falls back to direct draws.

## Completeness Checks
- [ ] TESTS: A regression test pins this specific fix

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2751
