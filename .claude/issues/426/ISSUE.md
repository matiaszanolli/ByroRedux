# REN-PIPE-C1: No VkPipelineCache reuse — 50-300ms avoidable cold-start cost on every engine launch

**Issue**: #426 — https://github.com/matiaszanolli/ByroRedux/issues/426
**Labels**: bug, renderer, critical, performance

---

## Finding

Every `vkCreateGraphicsPipelines` / `vkCreateComputePipelines` call passes `vk::PipelineCache::null()`. No single shared cache is constructed in `VulkanContext::new`.

Call sites:
- `crates/renderer/src/vulkan/pipeline.rs` (main + UI pipelines)
- `crates/renderer/src/vulkan/context/mod.rs` (wiring)
- `crates/renderer/src/vulkan/svgf.rs`
- `crates/renderer/src/vulkan/composite.rs`
- `crates/renderer/src/vulkan/ssao.rs`
- `crates/renderer/src/vulkan/compute.rs`

## Impact

- Every cold start re-parses / recompiles SPIR-V → driver IR for ~7 pipelines (triangle, UI, composite, SVGF temporal, SSAO, cluster cull, plus on-demand specials).
- On NVIDIA/AMD drivers, this is **50-300 ms of avoidable startup work**.
- Live swapchain recreation currently rebuilds pipelines under format-match variants (see PIPE-H2); with no cache, each resize pays the full compile cost again.
- Blocks a future on-disk cache for instant warm-starts.

## Fix

Two-phase:

1. **Shared in-process cache** (unblocks warm resize immediately):
   ```rust
   let pipeline_cache = device.create_pipeline_cache(
       &vk::PipelineCacheCreateInfo::default()
           .initial_data(&[]),
       None,
   )?;
   // Pass pipeline_cache to every create_graphics_pipelines / create_compute_pipelines
   ```

2. **On-disk persistence** (instant second-launch):
   - Read `$XDG_CACHE_HOME/byroredux/pipeline.cache` at startup, pass as `initial_data`.
   - Key the file by `(vendor_id, device_id, driver_version, app_version)`.
   - After pipeline creation, call `vkGetPipelineCacheData` and write to disk.
   - Invalidate the file if any key changes (driver update, device swap).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Every pipeline-create call site must be updated in lockstep. Grep for `create_graphics_pipelines` and `create_compute_pipelines` in the renderer to confirm complete coverage.
- [ ] **DROP**: `VkPipelineCache` destroyed in `VulkanContext::Drop` before device destruction.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Startup timing comparison — cold start (fresh cache) vs warm start (cached file). Expect the warm start to skip most compile time.

## Related

- Not the same as #91 (pipeline cache loaded from untrusted CWD path). That issue flags security of an existing cache file; this issue is that there is no cache at all.

## Source

Audit: `docs/audits/AUDIT_RENDERER_2026-04-18.md`, Dim 3 C1.
