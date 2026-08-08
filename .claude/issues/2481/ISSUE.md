# AS-D1-NEW-02: BLAS registration overwrites an occupied slot without destroying the previous acceleration structure

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2481
**Finding ID**: AS-D1-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — AS Correctness
**Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs::AccelerationManager::build_blas` and `::build_blas_batched` (Phase 7); mirrored in `blas_skinned.rs::AccelerationManager::build_skinned_blas_batched_on_cmd` (Phase 4)
**Status**: NEW

## Description
All three registration sites assign unconditionally: `self.blas_entries[handle] = Some(BlasEntry { ... })` (both static sites), `self.skinned_blas.insert(p.entity_id, BlasEntry { ... })` (skinned site). If the slot/key already holds a live `BlasEntry`, the old value is dropped as plain memory. `GpuBuffer`'s `Drop` safety net reclaims the backing buffer with a warn + debug-assert, but `BlasEntry::accel` is a raw `vk::AccelerationStructureKHR` with no `Drop` — it leaks for the process lifetime. Additionally `total_blas_bytes`/`static_blas_bytes` are incremented for the new entry without decrementing the replaced one, so the eviction budget drifts upward permanently.

## Evidence
The only structural protection today is caller discipline, and it is not uniform. `context/resources.rs::build_global_blas_for_draws` **does** guard (`if !cmd.in_tlas || accel.has_blas(cmd.mesh_handle) { return None; }`), but the general-purpose `build_blas_batched` wrapper does **not** — it filters only on `mesh.rt_capable` and buffer presence. The cell-loader callers happen to be safe because `cell_loader/spawn.rs` pushes to `blas_specs` only inside the fresh-upload branch, and `cell_loader/exterior.rs` batches only freshly-created terrain/water meshes. `AccelerationManager::has_blas` exists and is public, but neither `build_blas` nor `build_blas_batched` consults it.

## Impact
No live path reaches it today, so this is a latent gap, not an active bug. Should a future streaming/hot-reload/LOD-swap path re-register an occupied handle, the symptom is a slow VRAM leak plus a silently-inflated BLAS budget — both easy to misattribute. The symptom is *not* corruption: the new entry's address is correct, so rendering stays right while memory drifts.

## Related
`#1449` / MEM-01 (deferred-destroy on eviction — the pattern this site should reuse); `#372` (`drop_blas` deferred queue).

## Suggested Fix
At each of the three registration sites, `take()` any pre-existing entry first, subtract its `size_bytes` from `total_blas_bytes` (and `static_blas_bytes` on the static path), and push it onto `pending_destroy_blas` with `DEFAULT_COUNTDOWN` — exactly what `drop_blas` already does — before writing the new entry.

## Completeness Checks
- [ ] **TESTS**: A unit test re-registers an occupied BLAS handle and confirms no leak / correct byte accounting
- [ ] **SIBLING**: All three registration sites (2 static + 1 skinned) get the same guard
