# REN-D5-01: Batched texture flush releases pooled staging buffers with the upload size, not the allocation size — StagingPool's 128 MB budget is enforced against an under-counted ledger

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1921

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/texture_registry.rs:803-827,884` + `crates/renderer/src/vulkan/texture.rs:186,201,401`
**Status**: NEW

## Description
`StagingGuard::release_to`'s contract says `capacity` should be `allocation.size()` from the acquire call. The synchronous path honors it, but the batched cell-load path `TextureRegistry::flush_pending_uploads` passes `staging_capacity` = the requested upload size, not the (possibly much larger) reused allocation's size. `StagingPool::acquire` is best-fit (`capacity >= size`), so a 22 MB pooled buffer legitimately serves a 5 MB texture — and is then re-recorded in the free list as a 5 MB entry. Recorded capacity shrinks monotonically on every reuse.

## Evidence
`record_dds_upload` returns `Ok((..., staging, image_size))`; `flush_pending_uploads` destructures it as `staging_capacity` and calls `staging.release_to(pool, staging_capacity)`.

## Impact
(1) The documented "128 MB retained" staging cap is enforced against shrunken numbers — real retained CpuToGpu memory can exceed the budget by the sum of (real − recorded) across entries, worst-case several hundred MB after repeated cell transitions with mixed texture sizes. CpuToGpu maps to the host-visible BAR heap (~256 MB on non-ReBAR NVIDIA parts), so silent over-retention pressures a scarce heap. (2) Best-fit reuse degrades over time. Not a leak (buffers stay tracked and destroyed on trim/shutdown) and not per-frame — bounded, hence MEDIUM not HIGH.

## Related
#239 / #511 (pool + budget origin)

## Suggested Fix
In `flush_pending_uploads`, compute the release capacity the same way the sync path does — `staging.allocation.as_ref().map(|a| a.size()).unwrap_or(staging_capacity)` — or change `record_dds_upload` to return `allocation.size()` as the third element.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
