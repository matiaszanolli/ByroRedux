# PERF-D3-NEW-06: QueryRead::get re-runs downcast_ref on every access (~391K/frame, ~1.2-2.3ms)

**Severity**: HIGH · **Dimension**: ECS Query / Draw Call (merged PERF-D3-NEW-06 + PERF-D4-NEW-03)
**Location**: `crates/core/src/ecs/query.rs:44-53` (`storage()`/`get()`), `:98-119` (QueryWrite mirror); hot caller `byroredux/src/render/static_meshes.rs:133-648`
**Status**: NEW

`QueryRead<T>` holds the read guard for the whole loop, so `&T::Storage` is invariant once acquired — but `get()` calls `storage()` which re-runs `guard.as_any().downcast_ref::<T::Storage>().expect(...)` (non-inlinable vtable dispatch + 16-byte TypeId compare + never-taken `.expect`) on **every** access. ~17 optional gets × 23K entities ≈ **391K downcasts/frame** at radius-8, ~1.2-2.3ms of the ~8.84ms `brd_ms` — pure overhead. Taxes every `get`/`get_mut`/`contains` engine-wide.

**Fix**: cache the downcast once in `QueryRead::new`/`QueryWrite::new` — store a `*const T::Storage` resolved from the guard (sound: the guard keeps the lock held and the box address stable for the struct's lifetime), return it from `storage()`. `storage()` becomes a field read; `SparseSet/Packed::get` inline. Fixes every caller. Lower-leverage alt: hoist `q.storage()` into a `&T::Storage` local in `collect_static_mesh_draws`. Benchmark `brd_ms` before/after; 1979+ test suite must not regress.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
