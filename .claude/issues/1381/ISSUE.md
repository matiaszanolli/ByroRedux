# PERF-D2-NEW-03: dhat / alloc-counter regression coverage unwired (recurring) — wire dhat-heap feature + cell-load alloc assertion

**Severity**: LOW (process/infra gap, not a runtime defect) · **Dimension**: GPU Memory / CPU Allocations (PERF-D2-NEW-03)
**Location**: workspace-wide; relevant hot paths `crates/renderer/src/vulkan/scene_buffer/upload.rs`, `crates/renderer/src/vulkan/buffer.rs` (StagingPool), `crates/renderer/src/mesh.rs` (pool growth), `crates/core/src/ecs/packed.rs` (dirty sets)
**Status**: NEW (recurring — flagged in 2026-05-04/-06/-10/-16/-19 reports, never wired as a tracked issue)

There is still no dhat/alloc-counter coverage. Every dirty-gate (#878/#1134), scratch-shrink hysteresis, StagingPool eviction, and the alloc findings in THIS audit (PERF-D6-NEW-01/02, PERF-D2-NEW-01) are validated only by pure-predicate unit tests — never by an end-to-end allocation-count assertion. A regression where a caller stops invoking flush_if_needed, or the LOD path leaks sub-allocations, would pass all current tests. The `/audit-performance` brief explicitly calls for tracking this gap and treating new alloc-hot-path findings as warranting a "wire dhat for this site" follow-up. NIF has a partial dhat gate (`crates/nif/tests/heap_allocation_bounds.rs`) but only over a bare NiNode — geometry/particle paths uncovered.

**Fix**: wire `dhat-rs` behind a `--features dhat-heap` cfg in the `byroredux` binary; add ≥1 cell-load→cell-unload integration assertion bounding net retained allocations (StagingPool select_evictions + mesh deferred_destroy are deterministic enough). Lowest-cost first target: bound allocation count across a repeatable interior→interior transition. Extend the NIF fixture to one BSTriShape + one NiPSysEmitter.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
