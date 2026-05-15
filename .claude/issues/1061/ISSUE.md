## Description

13 inline `vk::MemoryBarrier::default()` + `cmd_pipeline_barrier(...)` sites in the renderer, 6 of them in `context/draw.rs` alone. Each spells out the same `(src_stage, src_access) → (dst_stage, dst_access)` ladder with minor variations on the access flags.

The WriteDescriptorSet half of prior **TD3-008** closed via `#1046` (`descriptors.rs` got proper builders). The MemoryBarrier half was deferred and is still scattered.

Verified live: `grep -nE "vk::MemoryBarrier::default|cmd_pipeline_barrier" crates/renderer/src/vulkan/context/draw.rs | wc -l` → **12 hits** (6 barrier creates + 6 barrier submits in draw.rs alone). Workspace-wide count is 13.

Lifted from Dim 3 of `AUDIT_TECH_DEBT_2026-05-14.md` (TD3-103, MEDIUM).

## Sites enumerated

In `crates/renderer/src/vulkan/context/draw.rs`:

| Line | Pair | Purpose |
|------|------|---------|
| 744  | `compute_to_blas` | SkinCompute COMPUTE_WRITE → BLAS_BUILD_READ |
| 855  | `blas_to_tlas` | BLAS_BUILD_WRITE → TLAS_BUILD_READ |
| 959  | (untitled) | TLAS_BUILD_WRITE → RT_READ in fragment / compute |
| 994  | `host_barrier` | HOST_WRITE → upload flush |
| 1009 | (untitled) | per-frame SSBO write → shader read |
| 1493 | (untitled) | post-pass color → present |

Plus 6 more sibling sites in `volumetrics.rs`, `bloom.rs`, `caustic.rs`, `taa.rs` for compute-pass-output barriers.

## Proposed consolidation

`crates/renderer/src/vulkan/descriptors.rs` (post-#1046) is the canonical home for descriptor-write helpers. Add a sibling module / set of helpers there:

```rust
/// Common compute → AS-build barrier (`#860` SkinCompute → BLAS).
pub(crate) unsafe fn record_compute_to_blas_barrier(device: &ash::Device, cmd: vk::CommandBuffer) {
    let barrier = vk::MemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
        .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR
            | vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::COMPUTE_SHADER,
        vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
        vk::DependencyFlags::empty(),
        std::slice::from_ref(&barrier),
        &[],
        &[],
    );
}

pub(crate) unsafe fn record_blas_to_tlas_barrier(...) { ... }
pub(crate) unsafe fn record_tlas_to_rt_read_barrier(...) { ... }
pub(crate) unsafe fn record_compute_output_to_fragment_sample(...) { ... }
```

Or — if every site truly has a unique flag combination — a generic `record_memory_barrier(device, cmd, src_stage, src_access, dst_stage, dst_access)` thin wrapper that at least normalizes the spelling.

## Why this matters

Vulkan barrier flags are spec-strict (validation layers catch mismatches at runtime, NOT at compile time). Each new pass that lands today does its own barrier dance, which is exactly how `#860` had to surgically tighten the SkinCompute path months after the original draft. Centralizing the canonical barrier shapes:

1. Means the next pass added doesn't reinvent the access-mask combination
2. Surfaces barrier-shape changes in one diff (easier to bisect via RenderDoc)
3. Pairs with `feedback_speculative_vulkan_fixes` — barriers are exactly the class of change that requires RenderDoc baselines, so centralizing them concentrates the risk-zone

## Completeness Checks

- [ ] **UNSAFE**: All barrier helpers are `unsafe fn` (caller responsibility for command-buffer state). Each helper's safety comment must document the assumed stage / access mask of the surrounding code.
- [ ] **SIBLING**: Audit every renderer pipeline (`volumetrics.rs`, `bloom.rs`, `caustic.rs`, `svgf.rs`, `taa.rs`, `composite.rs`, `water.rs`) for inline barriers and consolidate.
- [ ] **DROP**: N/A (no Vulkan object lifecycle change).
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: A test that compiles each helper and exercises its argument shape via a fake `ash::Device` (or just confirms it links + the Vulkan flag combos are spec-valid via SPIR-V reflection — out of scope here, but worth a follow-up).

## Effort
medium (~1 day — extract 6-8 named barrier helpers + replace 13 call sites + verify with RenderDoc that the timeline shape is identical pre/post)

## Cross-refs

- Audit report: `docs/audits/AUDIT_TECH_DEBT_2026-05-14.md` (TD3-103)
- Prior #1046 (descriptor / barrier boilerplate — closed; this issue picks up the barrier half left open in the same audit family)
- `feedback_speculative_vulkan_fixes.md` — barrier changes need RenderDoc baselines before/after
