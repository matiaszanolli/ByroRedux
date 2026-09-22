# UI-D5-2026-09-21-05: scene draw batches have no clamp at all against the SSBO's grown capacity after a failed instance-buffer grow

**Issue**: #4726
**Severity**: MEDIUM
**Labels**: medium,renderer,vulkan,bug

## Description
Flagged by `/audit-ui`'s AUDIT_UI_2026-09-21.md as a renderer-domain gap it noticed but didn't own (UI-D5-2026-09-21-01's Related field: "`/audit-renderer` for scene draws past a failed grow (no per-draw clamp exists at all)"). Confirmed independently against current code, and distinct from UI-D5-2026-09-21-01 (which covers only the single UI overlay instance).

#4199 changed the instance SSBO from an eager `MAX_INSTANCES`-sized allocation to a working capacity (`INITIAL_INSTANCE_CAPACITY` = 65,536) that grows on demand via `ensure_instance_capacity`, with `upload_instances` clamping the CPU→GPU upload to `self.instance_capacity[frame_index]` when a grow fails ("the slot keeps its current buffers and capacity, and the upload clamps to it"). But `DrawBatch`es (the per-mesh-per-pipeline groups that become `vkCmdDrawIndexed`/indirect draw calls) are built purely from positions in the CPU-side `gpu_instances` vector, with `first_instance`/`instance_count` set directly from that position — with no check anywhere against `instance_capacity[frame]` at batch-formation time, geometry-pass dispatch time, or anywhere else in the draw path.

So when a scene has more instances than the current slot capacity and the same-frame grow fails, the CPU-side batch list still spans the full (unclamped) instance range, but the GPU-side SSBO was only ever sized/uploaded to the old (smaller) capacity. `geometry_pass.rs`'s `dispatch_direct` closure calls `device.cmd_draw_indexed(cmd, batch.index_count, batch.instance_count, ..., batch.first_instance)` unconditionally for every batch, including ones whose `first_instance + instance_count` extends past the actual allocated buffer size — this is a true out-of-bounds device-buffer read (not just stale CPU-side data), and `robust_buffer_access` is not enabled (`crates/renderer/src/vulkan/device.rs:652`), so it is undefined behavior at the driver level rather than a guaranteed-safe zero read.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`DrawBatch` formation loop, ~lines 520-551): `first_instance: instance_idx, instance_count: 1` (then incremented in place for contiguous runs) — built purely from `gpu_instances`/`instance_idx` positions, with no reference to `instance_capacity` anywhere in the function.
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_instances`): `let capacity = self.instance_capacity[frame_index]; let count = instances.len().min(capacity);` — confirms the upload silently drops the tail past `capacity`, with only a log warning, no signal back to the batch list.
- `crates/renderer/src/vulkan/context/geometry_pass.rs` (`dispatch_direct`, ~lines 379-424): both the `global_bound` and per-mesh-fallback arms call `device.cmd_draw_indexed(cmd, batch.index_count, batch.instance_count, ..., batch.first_instance)` directly from the batch, with no capacity check.
- `crates/renderer/src/vulkan/device.rs:652`: the enabled `vk::PhysicalDeviceFeatures` chain does not request `robust_buffer_access`.
- Contrast: the UI overlay's own single instance at least has a (mis-targeted) `MAX_INSTANCES` guard (UI-D5-2026-09-21-01); ordinary scene geometry batches have none at all.

## Impact
Requires a scene with more visible instances than the current per-frame SSBO capacity (any content denser than the 65,536 initial capacity — the RP-1 comment in the same file cites "~50K REFRs" as a realistic dense-city figure, so this is within reach of real content) AND a failed host-visible grow allocation in that same frame (a memory-pressure-dependent precondition, same rarity class as UI-D5-2026-09-21-01/the original #3601). Unlike the UI overlay case, the blast radius here is every draw batch past the actual uploaded capacity — potentially a large fraction of a dense scene's geometry reading undefined GPU memory in the same frame, which can manifest as visible corruption or, depending on driver/hardware behavior with `robust_buffer_access` off, a device-lost/crash.

## Related
- UI-D5-2026-09-21-01 (issue TBD, same audit-publish run) — the UI overlay's own instance of this exact precondition; that finding is the narrow (1-instance) case, this is the general (all scene draws) case.
- #3601 (closed), #4199 (closed) — the original overflow-clamp bug and the capacity-grow feature that reopened this class of bug on the UI side; #4199 never added an equivalent clamp for ordinary scene draw batches.
- Source report: `/audit-ui`'s AUDIT_UI_2026-09-21.md, §3 UI-D5-2026-09-21-01 Related field ("for scene draws past a failed grow (no per-draw clamp exists at all)") — flagged there as a renderer-domain gap outside that audit's ownership.

## Suggested Fix
After `ensure_instance_capacity` resolves for the frame (success or failed-grow-clamped), drop or clamp any `DrawBatch` (or split it) whose `first_instance + instance_count` exceeds the slot's actual `instance_capacity[frame]`, mirroring the log-and-continue pattern already used for the `MAX_INSTANCES` RP-1 check. Alternatively, size-check at `dispatch_direct` immediately before each `cmd_draw_indexed` call as a last-resort guard.

## Completeness Checks
- [ ] **TESTS**: A regression test simulates a failed grow with `gpu_instances.len()` exceeding the pre-grow `instance_capacity[frame]`, and asserts no `DrawBatch` references an instance index past the actual capacity
- [ ] **SIBLING**: The indirect-draw path (`cmd_draw_indexed_indirect`, when batches merge) gets the same audit — the indirect command buffer's `instanceCount`/`firstInstance` fields are populated from the same unclamped batch data

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D5-2026-09-21-05, renderer-domain finding raised via the UI-D5-2026-09-21-01 Related pointer)
