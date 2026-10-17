# PERF-VOL-01: Combustion light-moment atomics from every luminous froxel target host system RAM over PCIe

Labels: medium,performance,renderer,shaders,bug
Source: docs/audits/AUDIT_PERFORMANCE_2026-10-17.md

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline (Dim 5) / GPU Memory Pressure
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp:2741-2845` (`accumulateCombustionLightMoment`, called from `main()` at `:2967`); buffer created at `crates/renderer/src/vulkan/volumetrics/init.rs:447-459` via `GpuBuffer::create_host_readback`
- **Status**: NEW (searched open and closed issues for `combustion moment atomic`, `GpuToCpu atomicAdd`, `host-cached atomics`; #3835, #4535, #4786 and #5772 concern the CPU drain and init cleanup, not the shader-side target memory)
- **Description**: `combustionLightMoments` (binding 18) is the buffer the host later drains to turn the transported flame field into surface lights. It is allocated with `create_host_readback`, i.e. `MemoryLocation::GpuToCpu`. In gpu-allocator 0.28 that preferred-flag set is `HOST_VISIBLE | HOST_COHERENT | HOST_CACHED` (`gpu-allocator-0.28.0/src/vulkan/mod.rs:810-814`), with no `DEVICE_LOCAL`. On a discrete card that selects the cached system-RAM memory type. The inject shader then performs **8 `atomicAdd`s per luminous froxel** (`weight`, `weighted_x/y/z`, `radiant_r/g/b`, `luminous_volume`, lines 2811-2845) straight into it. Every luminous froxel of every fire hits the same 256 bins × 8 words, so the traffic is also heavily same-address contended.
- **Evidence**: `accumulateCombustionLightMoment` early-outs only when `medium.extinction <= 1e-7`, emission luma is ~0, or the froxel is outside the ±32 m / ±16 m light domain (`:2749-2766`). Inside a flame, plume or explosion every froxel passes. `create_host_readback`'s own doc (`buffer.rs:1119-1137`) says `GpuToCpu` is for host-drained data and contrasts it with BAR-resident `CpuToGpu`; nothing there considers a device-side atomic target. The occupancy mask, by contrast, is `CpuToGpu` (BAR or VRAM) and so does not have this problem.
- **Impact**: Cost scales with luminous froxel count: ~8 PCIe round trips each, 8 × N_lum per frame. A candle is a handful of froxels; a bonfire or explosion is hundreds to tens of thousands. Only combustion frames pay (`transportExpired` skips the call entirely, and a fog-only scene never reaches it). I cannot say how many milliseconds this is; PCIe atomic throughput on this part, and whether the driver coalesces same-address atomics within a warp before they leave the SM, are unmeasured. **Confidence: moderate on the mechanism (memory type and atomic count are read from code), low on the magnitude.**
- **Related**: #3835 (CPU drain gate), #4786 (known-empty transport), `docs/audits/PERFORMANCE_FIX_STATUS_2026-09-26.md` §#4784 (this is the part of the combustion cost the occupancy gate does not touch).
- **Suggested Fix**: Do not restructure on this report alone. First A/B on `--combustion-lab` (renderer-static, pinned camera, tier pinned) with `accumulateCombustionLightMoment` stubbed to `return;` and compare `volumetrics_inject_ms`; same-machine control, read the noise floor first. If it moves: accumulate per-workgroup into a shared-memory 256 × 8 table and flush only touched bins (this alone cuts global atomics by roughly the workgroup's luminous-froxel count), and/or accumulate into a `GpuOnly` 8 KB buffer and `cmd_copy_buffer` it into the readback buffer at the end of the dispatch. Either needs the existing zero/drain protocol (`combustion_moment_dirty`, `latched_drain`) kept intact.

## Completeness Checks
- [ ] **SHADER**: GLSL change recompiled with plain `glslangValidator -V` (no `-g0`); `.spv` committed; GpuInstance/UBO lockstep tests still green
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
