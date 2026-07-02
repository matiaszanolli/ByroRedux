# PERF-D3-NEW-02: Budget eviction has no rebuild path — evicted static BLAS drop out of RT permanently, and multi-cell load bursts age not-yet-drawn BLAS into candidacy

**Issue**: #1793
**Labels**: medium,vulkan,memory,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-02)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D3-NEW-02)

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

