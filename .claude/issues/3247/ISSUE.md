# 3247: D23-01: Bloom-relocation onto FSR color-input path introduced unvalidated barriers around scene_color

**Severity**: MEDIUM (HYPOTHESIS) · **Report**: `docs/audits/AUDIT_RENDERER_2026-08-24.md` (D23-01, Needs-RenderDoc)

## Description

Needs validation-layer confirmation before any code change; no fix proposed on reasoning alone.

Commit `5bab2fed` (2026-08-22, fixing `#2796`) moved bloom to run after `record_composite_pass` rather than before it, so its pyramid source/write target both changed to `composite.scene_image_views[frame]` — the same image `record_upscale_pass` reads as FSR's primary color input one call later in the same command buffer. This is new synchronization code: a `SHADER_READ_ONLY_OPTIMAL → GENERAL` barrier before the new `bloom_apply.comp` dispatch and the reverse restore after it, both introduced by this commit. The commit's own message states plainly that this sync code is "exercised by no automated test ... not been validated with RenderDoc or the Vulkan validation layers in this session." The source-level reasoning is careful and internally consistent (mirrors `frame_upscaler.rs::record_native_blit`'s existing pattern), but this is source-read confidence, not validation-run confidence.

## Location
- `crates/renderer/src/vulkan/bloom.rs::BloomPipeline::apply_to_scene` (`:760-851`)
- `crates/renderer/src/vulkan/context/post_passes.rs::record_bloom_pass` (`:880-905`)
- immediately upstream of `record_upscale_pass` (`:950`)

## Impact

If the barrier reasoning is wrong, FSR's dispatch or the native-blit fallback could sample `scene_color` mid-write, or trip a `VUID-VkImageMemoryBarrier-oldLayout` validation error every frame under the engine's default configuration (FSR Quality is the default upscaler). If correct, this closes with zero further action.

## Related

Fixes `#2796` (bloom color-injection correctness, verified correct by source read); closed precedent `#2139` (same "asserted but not validation-confirmed" pattern, resolved via `BYRO_VALIDATION=1`).

## Suggested Fix

Run `BYRO_VALIDATION=1` for a few hundred frames under both `--upscaler fsr3` (default) and `--upscaler taa`, interior and exterior scenes, grep for `VUID-VkImageMemoryBarrier-oldLayout`/`SYNC-HAZARD-*` against `scene_image`. If clean, close as verified with a one-line note in `fsr3-upscaler-integration-plan.md`; if not, the fix is a barrier correction, not a re-derivation from scratch.

## Completeness Checks
- [ ] **TESTS**: RenderDoc/`BYRO_VALIDATION=1` confirmation run, not a `cargo test` regression (GPU-invisible to static test)
