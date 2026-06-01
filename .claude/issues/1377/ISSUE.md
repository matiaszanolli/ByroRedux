# PERF-D3-NEW-08: static-mesh probes precede transform presence gate; cache-cold unsorted SparseSet gets

**Severity**: LOW · **Dimension**: Draw Call Overhead (PERF-D3-NEW-08)
**Location**: `byroredux/src/render/static_meshes.rs:136-158` (vis/wb probed before tq.get gate at :158); `crates/core/src/ecs/sparse_set.rs:112-115`
**Status**: NEW

`vis_q`/`wb_q` are probed for every entity *before* the `if let Some(transform) = tq.get(entity)` presence gate, so entities lacking a GlobalTransform pay two gets before being skipped. Separately, the loop drives `mq.iter()` (unsorted SparseSet) and every sibling `.get(entity)` is a cache-cold `sparse[entity]` → `data[idx]` double indirection into 16 different storages with no iteration locality.

**Fix**: (1) trivial — hoist the `tq.get(entity)` presence gate to the top of the loop body (~4 LOC). (2) the locality angle is largely subsumed by PERF-D3-NEW-06 (storage hoist) and ultimately the M40 RenderExtract stage. File (1); note (2) as M40 design context.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
