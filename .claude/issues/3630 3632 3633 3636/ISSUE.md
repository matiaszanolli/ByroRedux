# #3630: `depth.stats` contradicts `analyze_depth_field`'s degenerate-camera contract

- **Severity**: Trivial
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`)

`analyze_depth_field` (`camera.rs:322-325`) returns early on `near <= 0.0 || far <= near`
with `total` populated but `cleared == 0`, `invalid == 0`, `bands` empty. The command
computes `geometry = stats.total - stats.cleared - stats.invalid` (= total on this path)
and ALSO prints "(no geometry in frame — every sample is background)" since every band has
`samples == 0` — two contradictory lines, neither says the camera was rejected.

**Fix**: in `execute`, short-circuit on `stats.bands.is_empty() && stats.total > 0` with an
explicit "degenerate camera (near=…, far=…) — analysis rejected" line before the per-band
table.

---

# #3632: `is_fsr_dispatch_active()` contract broken by `force_native_debug`

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`is_fsr_dispatch_active`) vs
  `crates/renderer/src/vulkan/context/post_passes.rs` (`record_upscale_pass`, ~line 994)

`record_upscale_pass` adds a third FSR-suspension case (`render_debug_requires_raw_output`)
that `is_fsr_dispatch_active()` does not know about: it passes `force_native_blit: true` into
`FrameUpscaler::record`, which bridges to a native blit and returns without dispatching —
while `is_fsr_dispatch_active()` still reports `true`, so the jitter gate keeps applying the
FSR sub-pixel offset to the projection on a frame that's never reconstructed. Because
`dispatched_this_frame` stays false, `mark_dispatch_completed` is never called, freezing the
jitter index and leaving stale reconstruction history for 1-2 frames after returning to
`RENDER_DEBUG_FINAL`.

**Fix**: fold the raw-output predicate into `is_fsr_dispatch_active()` (it already has
`render_debug_flags`/`render_debug_mode` in scope) so the frame is unjittered like every
other non-dispatching frame — the simpler of the two suggested fixes (vs. teaching
`set_render_debug_mode` to call `signal_temporal_discontinuity`).

---

# #3633: `PresentationPipeline::recreate` is dead code

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/presentation.rs:686` (`PresentationPipeline::recreate`)

Nothing calls it — `recreate_swapchain_core` open-codes the same
take/destroy/upscaler.recreate/`PresentationPipeline::new` sequence instead
(`resize.rs`, inside what is now `recreate_taa_and_presentation` post-#3738 split). Found
independently three times in the audit (D11-05, D5-05, D23-03).

**Fix**: delete `recreate` (the smaller change per the issue — the resize site's own
comments already carry both invariants).

---

# #3636: no regression guard pins the presentation/upscaler resize ordering

- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (now `recreate_taa_and_presentation`
  post-#3738 split), test module in the same file

`FrameUpscaler::recreate` unconditionally destroys + recreates the output image/views;
`PresentationPipeline` writes those views into its descriptor sets exactly once, in
`create`/`write_inputs`. The only thing preventing the presentation descriptor from naming a
destroyed view is source ordering: `presentation.take()` → `destroy` → `upscaler.recreate()`
→ `PresentationPipeline::new(..., &upscaled_views, ...)`. No test pins this.

**Fix**: add a static-source-order test to `resize.rs::mod tests`, in the style of
`ssao_recreate_failure_rebinds_binding_7_to_the_placeholder`: assert
`find("unsafe { presentation.destroy(&self.device) }")` < `find("upscaler.recreate(")` <
`find("PresentationPipeline::new(")` inside `production_src()`.

## Completeness Checks (all four)
- [ ] UNSAFE: any new unsafe blocks have safety comments
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: Vulkan object lifecycle correct
- [ ] TESTS: regression test added where applicable
