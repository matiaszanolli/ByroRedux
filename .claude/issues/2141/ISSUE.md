# 2141: RL-D6-01: SSAO recreate failure on resize leaves scene descriptor binding 7 pointing at a destroyed AO image view

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2141
**Labels**: bug, medium, vulkan

---

## Severity
MEDIUM

## Dimension
Resource Lifecycle (GPU teardown ordering) — `/audit-concurrency` 2026-07-25

## Location
`crates/renderer/src/vulkan/context/resize.rs:392-453`

## Description
`recreate_texture_ssao_bindings` destroys the old `SsaoPipeline` (and its per-FIF AO images/views) before attempting to build the replacement. If `SsaoPipeline::new` fails, the `Err` arm only logs a warning and leaves `self.ssao = None`; `scene_buffers.write_ao_texture` is never called, so scene descriptor set 1 / binding 7 (`aoTexture`) still holds the destroyed `vk::ImageView` + `vk::Sampler`. This failure does not propagate, so `recreate_screen_passes` completes and rebuilds framebuffers — the `#1211` `framebuffers.is_empty()` bail-out does not catch it, and the next frame binds the stale set.

## Evidence
`resize.rs:401` destroys `ao_image_views`/`ao_sampler`; `:439-446` — `write_ao_texture` only in the `Ok` arm; `:449-452` — `Err` arm logs and returns with no rebind, no propagate. `scene_buffer/descriptors.rs:16-35` — `write_ao_texture` is the sole writer of binding 7; `triangle.frag` samples `aoTexture` unconditionally.

## Impact
On a resize where SSAO re-creation fails (realistic trigger: VRAM pressure during a drag-resize with a large cell loaded), every subsequent frame binds a descriptor referencing freed GPU memory. Validation layers report an invalid/destroyed imageView; on release drivers this reads freed memory → garbage AO, corruption, or device loss. Failure-path-only, hence MEDIUM.

## Related
Success-path twin already fixed as #33 (LIFE-H2, closed); the failure arm was never covered. Sibling of RL-D6-02 (filed separately, same pattern).

## Suggested Fix
Keep a 1×1 white "AO = 1.0" placeholder image + sampler owned by `VulkanContext` and rebind binding 7 to it for all frame-in-flight slots in the `Err` arm (also needed for the init-time `self.ssao = None` case at `mod.rs:2149-2152`, which leaves binding 7 entirely unwritten today).

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (force `SsaoPipeline::new` to fail and assert binding 7 is rebound to the placeholder)
