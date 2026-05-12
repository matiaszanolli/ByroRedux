# Renderer Audit — 2026-05-11 (Dimension 5 focus)

**Scope**: Dimension 5 — Command Buffer Recording (pool flags, reset/begin/end balance, render-pass begin/end, AS-build placement, compute-vs-graphics placement, per-draw state coalescing, screenshot path).
**Depth**: deep.
**Method**: orchestrator + single dimension agent.

## Executive Summary

- **Findings**: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 2 LOW.
- **Pipeline areas affected**: hot-path `draw_frame()` — micro-optimisations only.
- **Net verdict**: **CLEAN.** Command-buffer recording in `draw_frame` is structurally correct. Pre-existing OPEN issues (#910 acquire-error semaphore leak, #911 first-sight skin prime stall) are out of scope for this re-audit and remain tracked. The historical drift surface has been hardened by #506 / #507 / #912 / #930 / #647 / #909 / #906.

## Rasterization & Compute Recording Assessment (positive checks)

- **Command pool flags** (`helpers.rs:469-510`): draw pool `RESET_COMMAND_BUFFER`; transfer pool `TRANSIENT` only (one-shot allocate/free). Both wired correctly from `context/mod.rs:1024-1025`.
- **Reset-before-record** (`draw.rs:233-246`): per-frame `cmd` is reset with `CommandBufferResetFlags::empty()` and re-begun with `ONE_TIME_SUBMIT` after the both-slots fence wait at lines 144-156 (which guarantees the previous use has retired).
- **Begin/end balance** (`draw.rs:245, 2067`): only ONE `?` propagation between `begin_command_buffer` and `end_command_buffer` — on `end_command_buffer` itself. Every fallible call in between uses `.unwrap_or_else(|e| log::warn!(...))`. Begin/end is balanced on every reachable release-build control-flow path.
- **Render pass begin/end balance**: 2 render passes per frame, both balanced — main G-buffer (`draw.rs:1390, 1825`, UI overlay inside the same pass at 1773-1823) + composite (`composite.rs:826, 859`).
- **TLAS build placement** (`draw.rs:853-891`): `cmd_build_acceleration_structures` recorded at line 856, well before `cmd_begin_render_pass` at 1390. Followed by an explicit `AS_WRITE → AS_READ` barrier (873-882) targeting FRAGMENT_SHADER | COMPUTE_SHADER.
- **Compute placement**: all dispatches verified outside any render pass instance — skin compute (705-787), cluster cull (918), SVGF temporal (1854), Caustic splat (1890), Volumetrics inject+integrate (1975), TAA (1995), SSAO (2017), Bloom downsample (2036). Skin first-sight prime runs on a separate one-time-submit cmd buffer.
- **Composite ordering** (`draw.rs:2054-2057`): runs after every compute consumer it depends on; targets swapchain via `composite_framebuffers[swapchain_image_index]` with `final_layout = PRESENT_SRC_KHR`.
- **Per-draw state coalescing** (`draw.rs:1462-1622`): all five dynamic-state setters gated on change — `cmd_bind_pipeline` (1564), `cmd_set_depth_bias` (1598), `cmd_set_depth_test_enable / depth_write_enable / depth_compare_op` (1610-1622), `cmd_set_cull_mode` via `set_cull` closure with `Option<…>` sentinel (1654-1659). Two-sided alpha-blend split fires `set_cull(FRONT)` then `set_cull(BACK)` per batch.
- **Two-level batch coalescing** (`draw.rs:1093-1123, 1731-1748`): adjacent draws sharing `(mesh_handle, pipeline_key, two_sided, render_layer, z_test, z_write, z_function)` AND consecutive `first_instance + instance_count == instance_idx` merge to one batch; batches sharing `(pipeline_key, render_layer)` group into one `cmd_draw_indexed_indirect` call when `multiDrawIndirect` is supported. Descriptor sets bound once per frame.
- **Screenshot path** (`screenshot.rs:73-172`, called from `draw.rs:2063`): PRESENT_SRC_KHR → TRANSFER_SRC_OPTIMAL → copy → PRESENT_SRC_KHR. Barriers balanced. `srcAccess=COLOR_ATTACHMENT_WRITE` / `srcStage=COLOR_ATTACHMENT_OUTPUT` correctly references the composite pass's last write.
- **Pass affinity sweep**: every `cmd_draw_indexed*` inside the main render pass; every `cmd_dispatch` outside; AS-builds outside; buffer/image copies (`record_bone_copy` at draw.rs:441; screenshot copy) all outside any active render pass.

## Findings

### [LOW] Redundant initial `cmd_set_depth_bias(0,0,0)` before draw loop
**Dimension**: Command Recording
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1511`
**Severity**: LOW
**Observation**:
```rust
self.device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);   // unconditional pre-loop
// ...
if last_render_layer != Some(batch.render_layer) {     // fires on first batch (None)
    let (bias_const, clamp, bias_slope) = batch.render_layer.depth_bias();
    self.device.cmd_set_depth_bias(cmd, bias_const, clamp, bias_slope);
}
```
**Why bug**: The pre-loop unconditional set is dominated by the per-batch helper that fires on the first batch (`Option::None` sentinel). Same one-state-change-per-frame pattern that #912 fixed for `cmd_set_cull_mode` was already correct there but left behind on depth-bias. Pure host-side cost, no GPU impact.
**Fix**: Drop the unconditional set — the per-batch helper covers Vulkan's "must be set before first draw" requirement (mirrors the `last_cull_mode: Option<…>` pattern at `draw.rs:1497`).
**Confidence**: HIGH
**Dedup**: New — sibling of closed #912 (REN-D5-NEW-03).

### [LOW] `debug_assert!` on instance count panics inside an active recording
**Dimension**: Command Recording
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1154-1160`
**Severity**: LOW
**Observation**: `debug_assert!(gpu_instances.len() <= 0x7FFF, ...)` fires between `begin_command_buffer` (245) and `end_command_buffer` (2067). Release builds skip the assert and silently wrap into the alpha-blend mesh-id bit (already documented inline at the R16_UINT mesh-id ceiling).
**Why bug**: A debug panic here leaks the cmd buffer in pending state (no `end_command_buffer`). The process aborts shortly after, so practically harmless, but the debug-only path leaves the pool in a state that would block any cleanup attempt. Developer-facing only.
**Fix**: Promote to a `log::error!` + return / break before recording any draws when the instance count exceeds the ceiling, or convert mesh_id format to R32_UINT as the comment already prescribes.
**Confidence**: MED
**Dedup**: Adjacent to #647 / RP-1 (mesh_id format upgrade plan).

## Prioritized Fix Order

Neither is urgent.

1. **LOW** — Promote the `debug_assert!` to a recoverable `log::error!` + skip path. Closes a developer-only debug-build hazard.
2. **LOW** — Drop the redundant pre-loop `cmd_set_depth_bias(0,0,0)`. Mirrors #912's depth-bias-via-Option-sentinel pattern.

## Notes

- Dimensions 1, 3, 4, 5, 8, 9 are all CLEAN as of 2026-05-11. Broad 2026-05-09 sweep covered Dims 2, 6, 7, 10–16. The hot path is well-defended.
- Pre-existing OPEN issues touching this dimension (verified still valid, not re-raised):
  - #910 (REN-D5-NEW-01): acquire→submit error-path image_available semaphore leak
  - #911 (REN-D5-NEW-02): first-sight skin compute prime + sync BLAS BUILD stalls per-frame cmd — documented inline at `draw.rs:534-576` with the deferred fix design
- The next likely drift vector is **inside-recording panic** (the `debug_assert!`) becoming reachable on a dense city cell when REFR counts cross 32 768 — tracked via the inline comment and the R16→R32 mesh_id format upgrade.
