# REN-D5-01: compute_blas_budget picks the BAR aperture, not VRAM

**Issue**: #3043
**Severity**: HIGH
**Labels**: `high,renderer,vulkan,memory,bug`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RENDERER_2026-08-16.md` (Dimension — Memory/Lifecycle).

**Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` (`compute_blas_budget`, `blas_budget_for_heap`) · `crates/renderer/src/vulkan/device.rs` (`smallest_device_local_heap_bytes`)
**Status note**: NEW — **regression introduced by `9aea0aa0` ("Fix #2928")**.

## Description

#2928 replaced `total_device_local_bytes` (a sum over every `DEVICE_LOCAL` heap) with `smallest_device_local_heap_bytes` (a `min` over the same set), on the stated grounds that *"the common AMD / hybrid layout reports a small `DEVICE_LOCAL | HOST_VISIBLE` BAR window alongside the main VRAM heap"* and that summing therefore over-counts VRAM.

**That premise is correct; the chosen remedy inverts the error instead of removing it.**

`VK_MEMORY_HEAP_DEVICE_LOCAL_BIT` is a *heap* flag and host-visibility is a *memory-type* property, so the small BAR aperture is reported as its own `DEVICE_LOCAL` heap — which means `min()` selects **exactly the window the doc comment is warning about**, and discards the main VRAM heap the BLAS allocations actually land in.

## Evidence

```rust
// device.rs
mem_props.memory_heaps[..count].iter()
    .filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
    .map(|heap| heap.size)
    .min()          // ← on AMD this is the ~256 MB BAR heap, not VRAM
    .unwrap_or(0)
```
```rust
// predicates.rs
pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
    (heap_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)   // MIN = 256 MiB
```

Re-verified 2026-08-17: `compute_blas_budget` calls `smallest_device_local_heap_bytes` and feeds it straight to `blas_budget_for_heap`.

## Impact

On any multi-`DEVICE_LOCAL`-heap GPU (the common AMD and hybrid layout), the BLAS budget is computed from a ~256 MB BAR aperture instead of VRAM. `/3` then floors to `MIN_BLAS_BUDGET_BYTES` (256 MiB), so the acceleration structure budget collapses to its minimum regardless of actual VRAM.

The dev machine is an RTX 4070 Ti (single large `DEVICE_LOCAL` heap), which is precisely why this does not reproduce locally — #2928 was "verified on a single-heap dev GPU", as the suite summary itself notes.

## Suggested Fix

Select the heap the BLAS allocations actually target rather than reducing over all `DEVICE_LOCAL` heaps: pick the **largest** `DEVICE_LOCAL` heap, or better, resolve the heap index from the memory type the AS buffers are allocated from (`gpu-allocator` can report it) so the budget tracks the real pool.

Do **not** revert to `sum` — #2928's original complaint stands.

## Related

- #2928 (whose fix this regressed), and the suite summary's "verified on a single-heap dev GPU" note
- `docs/engine/memory-budget.md` (the budget contract this feeds)

## Completeness Checks
- [ ] **HEAP-IDENTITY**: The budget is derived from the heap BLAS memory actually lands in, not a min/max heuristic over all heaps
- [ ] **MULTI-HEAP**: Verified against a reported multi-heap layout, not only the single-heap dev GPU
- [ ] **NO-REVERT**: #2928's original over-count is still avoided
- [ ] **DROP**: No change to acceleration-structure teardown ordering
- [ ] **TESTS**: A unit test feeds a synthetic multi-heap `MemoryProperties` and asserts the chosen heap

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3043 --json state` when live state is needed.*
