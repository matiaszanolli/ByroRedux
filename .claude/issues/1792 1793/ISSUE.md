# #1792: PERF-D3-NEW-01: Mid-batch BLAS eviction (#510) is structurally a no-op

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-01)
**Labels**: bug, medium, vulkan, memory, performance
**State**: OPEN

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

---

# #1793: PERF-D3-NEW-02: Budget eviction has no rebuild path — evicted static BLAS drop out of RT permanently, and multi-cell load bursts age not-yet-drawn BLAS into candidacy

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-02)
**Labels**: bug, medium, vulkan, memory, performance
**State**: OPEN

## Location
`crates/renderer/src/vulkan/acceleration/tlas.rs:150-158` (missing rigid BLAS → count + skip only); `blas_static.rs:425-431` (per-batch `frame_counter` bump) + `:1129,1156-1175` (`idle ≥ MIN_IDLE_FRAMES` candidacy + slot clear)

## Description
Two coupled gaps. (1) No recovery: `build_blas_batched` is invoked only from cell-load / scene-load sites (`cell_loader/exterior.rs:349`, `cell_loader/spawn.rs:1179`, `scene/nif_loader.rs:1043`, `cornell.rs`, `scene.rs:371`) — never per-frame. In `build_tlas` a rigid draw whose `blas_entries[mesh_handle]` is `None` hits `missing_rigid_blas += 1; … continue;` — warns (rate-limited) and skips forever; the mesh keeps rasterizing but permanently vanishes from shadows/reflections/GI until its cell unloads and reloads. (2) Burst aging: a synchronous multi-cell load (`--grid` radius 3 = 49 batched calls before the first frame) leaves cell #1's just-built, never-yet-drawn entries at high idle on the first `build_tlas` — prime LRU victims if cumulative `static_blas_bytes` crosses budget mid-load, since eviction picks oldest-first.

## Evidence
`tlas.rs`'s own comment — "an LRU eviction got something the draw still references; should be near-zero in steady state" — the #1228 counter exists precisely because there is no re-acquire path.

## Impact
Silent RT-correctness degradation (missing occluders → wrong shadows/GI), gated on `static_blas_bytes > budget` — unreachable on the 12 GB dev card with vanilla content, plausible on 6-8 GB devices with heavy exteriors / mod load-orders. Crash-safe (deferred destroy, #1449); recovery requires a cell round-trip. Steady-state single-cell-per-frame streaming is protected (drawn entries idle ≤2 < MIN_IDLE); exposure is multi-batch bursts between frames.

## Related
#920, #740, #1228, #1449, PERF-D3-NEW-01.

## Suggested Fix
On a `missing_rigid_blas` hit in `build_tlas`, queue the mesh handle for a lazy `build_blas_batched` next frame (mirroring the skinned first-sight path). Cheaper stopgap: stamp the batch's own entries with a post-batch tick so an in-burst load cannot age its own cells into victims.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
