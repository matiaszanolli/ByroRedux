# D12-2026-08-07-02: upload_indirect_draws failure is warn-swallowed, but the draw loop still executes cmd_draw_indexed_indirect over the un-updated buffer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2504
**Finding ID**: D12-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 12 — Command-buffer recording
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2672-2685` (upload site) → `crates/renderer/src/vulkan/context/geometry_pass.rs:408-439` (`cmd_draw_indexed_indirect`)
**Status**: NEW

## Description
The indirect-command upload uses the same `unwrap_or_else(|e| log::warn!(...))` soft-fail pattern as the neighbouring `upload_instances` / `upload_materials` / `upload_previous_models` calls. For data SSBOs that is correct — a stale or zero buffer only misrenders. For the **indirect** buffer it is qualitatively different: `index_count` / `first_index` / `vertex_offset` / `first_instance` are *fetched and executed by the GPU*. On a failed upload the draw loop still issues `cmd_draw_indexed_indirect(indirect_buffer, i*stride, group_size, stride)` sized from **this** frame's `batches`, reading commands that belong to a previous frame's global-geometry layout (or, on the first use of a FIF slot, never-written host-visible memory). `upload_indirect_draws` correctly declines to stamp `last_uploaded_indirect_hash` on failure, so the *next* frame re-uploads — but the current frame has already recorded the draw.

## Evidence
```rust
// draw.rs:2682
self.scene_buffers.upload_indirect_draws(&self.device, frame, indirect_scratch)
    .unwrap_or_else(|e| log::warn!("Failed to upload indirect draws: {e}"));
```
no flag is set, and `geometry_pass.rs:428` unconditionally issues the indirect draw whenever `use_indirect` (`global_bound && multi_draw_indirect_supported`).

## Impact
Requires `mapped_slice_mut()` or `flush_range()` to fail (rare — host-visible, persistently mapped). When it does: stale `first_index`/`vertex_offset` after a `rebuild_geometry_ssbo` shrink is an out-of-range index fetch; uninitialised memory on a slot's first frame yields arbitrary `index_count`/`instance_count`. Both are GPU page-fault / TDR class, i.e. the failure mode is much louder than the warn suggests.

## Related
#309 (indirect path), #1809 (`upload_indirect_draws` dirty gate), #1587 (partial flush), #2215 (open indirect-grouping regression).

## Suggested Fix
Have the upload set a per-frame `indirect_upload_ok` flag (or return the `Result` up to `draw_frame`) and force the direct-draw fallback for that frame when it is false — `dispatch_direct` already handles every batch correctly.

## Completeness Checks
- [ ] **TESTS**: A regression test simulates an upload failure and confirms the frame falls back to direct draws instead of issuing stale indirect commands
