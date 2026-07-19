# Batch: 4 CLOSED issues — verification pass (audit-hygiene first)

All four are CLOSED. Verified each premise against current code:

## #229 AR-07 (LOW) — ALREADY FIXED
`find_key_pair<F>(key_count, time_at: F, time)` now takes an index accessor closure
(`|i| keys[i].time`), no Vec alloc. interpolation.rs:11 doc: "See #240". Fixed under #240.
→ No action.

## #256 R-01 (HIGH) — ALREADY FIXED
cluster_cull.comp:59 now has `mat4 prevViewProj; // Must match GpuCamera... See #256.`
Offsets aligned. Fixed under #256 itself. → No action.

## #262 LC-02 (MEDIUM) — ALREADY FIXED
anim/sequence.rs:87 NiGeomMorpherController resolves `target_idx` via
`resolve_morph_target_index(scene, cb)` and emits `MorphWeight(target_idx)` per target,
not hardcoded 0. Comment: "See #262". Fixed under #262. → No action.

## #203 RL-11..15 (MEDIUM) — PARTIALLY FIXED
- RL-11 SsaoPipeline::new — FIXED (new_inner + partial.destroy() on every err path, #1163).
- RL-12 ClusterCullPipeline::new — FIXED (partial + try_or_cleanup! macro).
- RL-13 SceneBuffers::new — buffers RAII-safe (GpuBuffer impls Drop, buffer.rs:979), so the
  light/camera/bone/instance buffers the issue lists do NOT leak. RESIDUAL: raw
  descriptor_set_layout + descriptor_pool (from create_scene_descriptors) leak if the
  subsequent dalc `write_mapped(...)?` (buffers.rs:799) fails.
- RL-14 TextureRegistry::new — NOT FIXED. Raw descriptor_set_layout/pool/samplers leak on any
  `?` after their creation (pool-create, set-alloc, 4× make_sampler). texture_registry.rs:263+.
- RL-15 VulkanContext::new — later pipeline-create failures clean up (partial pipe.destroy),
  but the early instance→device→surface→allocator→swapchain chain has no cleanup if a
  mid-chain create (e.g. TextureRegistry::new @1691) fails.

Residuals are near-zero impact: fatal init-failure paths, OS-reclaimed at process exit (the
issue itself notes this). Error paths are untestable without Vulkan fault injection.
