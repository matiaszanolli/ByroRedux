# PERF-D3-NEW-01: Mid-batch BLAS eviction (#510) is structurally a no-op — evict_unused_blas gate ignores the batch's pending bytes (OOM-on-first-huge-cell risk)

**Issue**: #1792
**Labels**: medium,vulkan,memory,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-01)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-01)

## Location
`crates/renderer/src/vulkan/acceleration/blas_static.rs:551-567` (mid-batch trigger, correctly fed `pending_bytes`) + `blas_static.rs:1115,1156` (callee gates, blind to `pending_bytes`); `predicates.rs:382-390`

## Description
The mid-batch trigger `should_evict_mid_batch(total_live_bytes, pending_bytes, budget)` correctly projects `live + pending` against 90% of budget. But `evict_unused_blas` (`:1106-1195`) gates purely on committed state: `if self.static_blas_bytes <= self.blas_budget_bytes { return; }` (`:1115`) and the per-candidate break (`:1156`) test the same committed-only compare. `pending_bytes` is a local in `build_blas_batched`, never threaded into `evict_unused_blas`. The batch's result buffers are created inline (`:621`) but only added to `static_blas_bytes` in Phase 7 (`:1029-1030`), after the batch completes. So mid-batch: either the previous cells were already under budget (evict early-returns, freeing nothing) or they weren't (pre-batch pass at `:533` already handled them). On a fresh single large cell load (`static_blas_bytes == 0` at start), a batch that individually overshoots the whole budget allocates every result buffer with zero intervening eviction — the budget is only ever enforced retroactively on the next cell load.

## Evidence
`should_evict_mid_batch`'s #510 doc comment describes the intended pause-and-evict; the callee's committed-only gate defeats it. `memory-budget.md` documents the trigger but not the (nil) effect.

## Impact
A single large batch (initial exterior grid load, FO4 precombine-heavy cell) allocates its full result-buffer footprint above budget with no mid-batch relief. On the RT-minimum 6 GB device (budget ≈ 2 GB) the intended pause never lands; failure mode is allocator pressure / a graceful cell-load bail (cleanup verified), not device loss. Unreachable on the 12 GB dev card with vanilla content.

## Related
#510, #740, #915, PERF-D3-NEW-02.

## Suggested Fix
Thread `pending_bytes` into `evict_unused_blas` (or add `evict_down_to(target_bytes)`) so both the early-return gate and the loop break test `static + pending` against the 90% line the trigger already computes.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

