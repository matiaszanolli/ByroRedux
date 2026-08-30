# CONC-D2-2026-08-30-03: `VolumetricsPipeline::record_neutral_frame`'s clear is not in the next dispatch's `pre_int_write` source scope (WAW)

**Issue**: #3647
**Labels**: bug, renderer, low, sync, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D2-2026-08-30-03 (LOW, D2 · Compute -> AS -> Fragment Chains).

**Location**: `crates/renderer/src/vulkan/volumetrics.rs:2594-2609` (publish) x `crates/renderer/src/vulkan/volumetrics.rs:2314-2329` (`pre_int_write`).

## Description

Exact structural sibling of CONC-D2-2026-08-30-02 on the volumetrics side.

`record_neutral_frame` clears `integrated_volumes[frame]` and publishes `TRANSFER_WRITE -> FRAGMENT_SHADER/SHADER_READ` (composite's `sampler3D`). The next full `dispatch` at that slot guards the integrated volume with `pre_int_write`, whose source scope is `COMPUTE_SHADER | FRAGMENT_SHADER` / `SHADER_READ` — it names the composite *read* but **not** the neutral *clear write*. A repeat `record_neutral_frame` on the same slot has the same hole in its own `to_clear` barrier (`:2569-2584`, src `COMPUTE|FRAGMENT` / `SHADER_READ|SHADER_WRITE`).

## Evidence

```rust
// record_neutral_frame — publishes only to FRAGMENT
2594        let to_sample = vk::ImageMemoryBarrier::default()
2595            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
2596            .dst_access_mask(vk::AccessFlags::SHADER_READ)
2603            vk::PipelineStageFlags::TRANSFER,
2604            vk::PipelineStageFlags::FRAGMENT_SHADER,
```
```rust
// dispatch — pre_int_write src scope omits TRANSFER_WRITE
2314        let pre_int_write = vk::ImageMemoryBarrier::default()
2315            .src_access_mask(vk::AccessFlags::SHADER_READ)
2316            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
2323            vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
2324            vk::PipelineStageFlags::COMPUTE_SHADER,
```

## Trigger Conditions

Slot `f` takes `record_neutral_frame` on frame N (no TLAS / no `cluster_cull` / no global geometry / `!requires_dispatch`), then the same slot takes the full `dispatch` path on frame N+`MAX_FRAMES_IN_FLIGHT`. **Always hit at scene load**, since the first frames have no TLAS.

## Verification Path

Validation layer, same harness as CONC-D2-2026-08-30-02. Expected concrete signal: **`SYNC-HAZARD-WRITE-AFTER-WRITE`** on the `integrated_volumes[f]` `VkImage` at the integration `vkCmdDispatch` (`volumetrics.rs:2349`) with `prior_access = SYNC_COPY_TRANSFER_WRITE`.

## Impact

**LOW, not MEDIUM**, because unlike its sibling this is a write-after-write where **the later write fully covers the image** — `volumetrics_integrate.comp` is dispatched one thread per `(x, y)` column and Z-marches every slice, so every froxel is overwritten. There is no read of stale data; only the *order* of two writes to the same memory is formally undefined. Reported for symmetry, and because sync-val will flag it in the same session as the sibling.

## Related

CONC-D2-2026-08-30-02 (same pattern, RAW instead of WAW).

## Suggested Fix

Add `TRANSFER` / `TRANSFER_WRITE` to the source scope of `pre_int_write` and `record_neutral_frame`'s own `to_clear` — additive-only. Lower priority than the sibling; **fix both in one pass** if the sibling is taken.

## Completeness Checks
- [ ] **VALIDATION**: The `SYNC-HAZARD-WRITE-AFTER-WRITE` signal is observed before the change and absent after it
- [ ] **SIBLING**: Landed together with CONC-D2-2026-08-30-02, which is the same defect shape on the caustic accumulator
