# SAFE-D1-2026-09-21-01: FSR dispatch-failure recovery blit declares `scene_color` `oldLayout = GENERAL`; the image is in `SHADER_READ_ONLY_OPTIMAL`

**Labels**: high, safety, renderer, vulkan, sync, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: HIGH (Vulkan spec violation) · **Dimension**: 1 — FFI call-site contract / 5 — Vulkan spec
**Location**: `crates/renderer/src/vulkan/frame_upscaler.rs`
- `FrameUpscaler::record`, the `if let Err(error) = dispatch` recovery arm: `record_native_blit(…, GENERAL, GENERAL)` (~:629-636)
- `record_native_blit` before-barrier `.old_layout(source_layout)` (~:729-736) and after-barrier `.new_layout(restore_layout)` (~:800-807)

**Status**: NEW (introduced by `ba4c0efcf`, #3572, 2026-09-18)
**Verified against**: HEAD `f97775ca8`

## Description

- #3572 (`ba4c0efcf`) inserted a `source_layout` parameter ahead of `output_layout` in `record_native_blit` and updated its three call sites. At the FSR dispatch-failure recovery call, the existing `vk::ImageLayout::GENERAL` argument, which had been the *output* layout, stayed in place and became `source_layout`. A second `GENERAL` was appended as the new `output_layout`. The call now passes `GENERAL, GENERAL`.
- In FSR mode, `scene_color` is the composite scene image, in `SHADER_READ_ONLY_OPTIMAL`:
  - `record_upscale_pass` (`context/post_passes.rs`) hands the upscaler the TAA output (`GENERAL`) only when `self.post.taa` exists. TAA is built only for `UpscalerMode::Taa` (`context/init.rs`); `set_upscaler_mode` builds or destroys it.
  - `record_fsr_barriers_before` checks `debug_assert_eq!(inputs.scene_color_layout, SHADER_READ_ONLY_OPTIMAL)` (#4538), and its `fsr_input_read_barrier` keeps `scene_color` at `old == new == SHADER_READ_ONLY_OPTIMAL`.
- So the recovery blit records `oldLayout = GENERAL → TRANSFER_SRC_OPTIMAL` on an image that is in `SHADER_READ_ONLY_OPTIMAL`. `restore_layout` is keyed off `source_layout`, so the after-barrier also leaves the image in `GENERAL`, not the layout it arrived in.

## Evidence

```rust
// FrameUpscaler::record: dispatch-failure arm (runs after record_fsr_barriers_before)
self.record_fsr_depth_restore(device, cmd, inputs.depth);
self.record_native_blit(
    device, cmd, frame, inputs.scene_color,
    vk::ImageLayout::GENERAL,   // source_layout: scene_color is SHADER_READ_ONLY_OPTIMAL here
    vk::ImageLayout::GENERAL,   // output_layout: correct (the FSR barriers moved the output to GENERAL)
);
// record_native_blit
.old_layout(source_layout)                       // GENERAL
.new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
```

- `git show ba4c0efcf -- crates/renderer/src/vulkan/frame_upscaler.rs` shows the recovery-call hunk keeping the old `GENERAL` line and adding `+ vk::ImageLayout::GENERAL`. The same commit changes the scene barrier from `.old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)` to `.old_layout(source_layout)`.
- The other two call sites are correct: the bridge path passes `inputs.scene_color_layout`, and the params-absent path passes `SHADER_READ_ONLY_OPTIMAL, SHADER_READ_ONLY_OPTIMAL`.

## Impact

- This path runs on every real FSR dispatch error, and deterministically under the fault injector `BYRO_FSR_FORCE_DISPATCH_FAIL=1` (`crates/fsr3-sys/src/lib.rs`). #3572's sync-validation matrix (Cornell TAA, TAA + raw view, FSR default, FNV Prospector) never exercised the injector.
- The failure frame triggers VUID-VkImageMemoryBarrier-oldLayout-01197. A wrong `oldLayout` is undefined behaviour: on drivers where `SHADER_READ_ONLY_OPTIMAL` and `GENERAL` differ in compression state, that frame's blit reads undefined contents.
- The leftover `GENERAL` layout is harmless after that frame. The composite colour attachment has `initial_layout = UNDEFINED` (`composite.rs`), and bloom and the exposure meter run before the upscale.
- This still needs confirmation from a validation-layer run (`BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1`). The audit did not run one, and no CI lane can: SAFE-D5-2026-09-21-01 (#4596).

## Related

- #4538 (closed) is the inverse, hypothetical case: the FSR barrier hard-codes `SHADER_READ_ONLY_OPTIMAL` while `GENERAL` is representable. Its fix added only the `debug_assert` and did not touch this call.
- #2140, #2145, #2519 and #3632 (closed) cover other parts of this recovery path. #3572 (closed) is the change that introduced this mistake.
- SAFE-D4-2026-09-21-02 (#4600): `record_native_blit`'s `# Safety` still says `scene_color` must be in `SHADER_READ_ONLY_OPTIMAL`, which is part of why the wrong argument reads as plausible.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this finding without re-reporting it.

## Suggested Fix

- At the recovery call, pass `inputs.scene_color_layout` (`SHADER_READ_ONLY_OPTIMAL` in FSR mode) as `source_layout`. Keep `GENERAL` only for `output_layout`.
- Extend `fsr_scene_color_barrier_asserts_the_layout_it_hard_codes`, or add a sibling source pin, so the recovery call's source argument must be `inputs.scene_color_layout` rather than a literal.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D1-2026-09-21-01)

## Completeness Checks
- [ ] **UNSAFE**: the recovery call's `// SAFETY:` and `record_native_blit`'s `# Safety` state the `source_layout` contract (SAFE-D4-2026-09-21-02 (#4600))
- [ ] **SIBLING**: all three `record_native_blit` call sites re-checked (bridge, params-absent, dispatch-failure)
- [ ] **TESTS**: a source pin fails if the recovery call passes a literal source layout; a `BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1` run is clean on the failure frame
