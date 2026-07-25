# 2139: CHAIN-D2-02: FSR SDK output-image layout contract is asserted but never validation-confirmed — HYPOTHESIS

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2139
**Labels**: bug, medium, sync

---

## Severity
MEDIUM

## Dimension
Compute → AS → Fragment Chains — `/audit-concurrency` 2026-07-25

## Status note
HYPOTHESIS — needs validation-layer confirmation before any code change, per the project's standing speculative-Vulkan-fix guardrail. No fix is proposed here on reasoning alone.

## Location
`crates/renderer/src/vulkan/frame_upscaler.rs:592-663` (`record_fsr_barriers_before`), `:700-741` (`record_fsr_barriers_after`)

## Description
The engine hand-declares the layout the vendored FFX Vulkan backend will leave every SDK-touched image in (output → `GENERAL` before dispatch, asserted `old_layout = GENERAL`/`SHADER_WRITE` after). Nothing in the repo pins the FFX backend's internal resource-state tracking to those assumptions; if the SDK leaves the output in a different layout, the after-barrier's `old_layout` is a lie and the transition is UB (VUID-VkImageMemoryBarrier-oldLayout-01197).

## Evidence
`frame_upscaler.rs:640-646` declares `old_layout(SHADER_READ_ONLY_OPTIMAL) → new_layout(GENERAL)`; `:720-726` declares the exact inverse. The only cross-check is a `SAFETY` comment asserting the conclusion, not code that verifies it.

## Impact
If wrong — corrupted/black upscaled output or a hard validation error every frame. If right — zero cost, this row closes.

## Trigger Conditions
Every FSR frame (`--upscaler fsr3`, the default per `5c7acfe2`).

## Verification Path
Run `BYRO_VALIDATION=1` (sync validation) for ~200 frames in FSR mode; grep for `VUID-VkImageMemoryBarrier-oldLayout-01197` / `SYNC-HAZARD-WRITE-AFTER-WRITE` / `SYNC-HAZARD-READ-AFTER-WRITE` naming the `upscale_output_*` image. A clean 200-frame run across both FIF slots is meaningful evidence this closes as a non-issue; a RenderDoc capture of the output image's layout timeline is the definitive artifact.

## Related
CHAIN-D2-03 (same boundary, failure-path variant, filed separately), commit `33d6a18e`, `5c7acfe2`.

## Suggested Fix
None proposed on reasoning alone. If validation is clean, land a comment on `record_fsr_barriers_after` recording the validated SDK contract + version, so a future SDK bump re-triggers the check.

## Completeness Checks
- [ ] **DROP**: N/A pending validation result
