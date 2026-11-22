# #7245: INC-2026-11-22-03: The all-slots-wait rider list in `sync.rs` does not name the new patch BLAS set (shared AS refit in place, shared scratch)

**Labels**: low,renderer,sync,test-gap,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-A-02**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: audit-concurrency Dim 1 (cross-frame resource rider list)
- **Location**: `crates/renderer/src/vulkan/sync.rs:97-98` (rider 11), `:693-714` (`frames_in_flight_contract_names_every_dependent_resource`); new resources at `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs:113-121` (`TerrainPatchBlasSet`: one AS per mesh, one `scratch`, not per frame in flight)
- **Changed in**: `crates/renderer/src/vulkan/acceleration/blas_terrain_patch.rs`, `ff339f658` (`sync.rs` was not touched)
- **Status**: NEW
- **Description**: Rider 11 records that the skinned BLAS is "refit in place ... so an earlier frame's readers must retire before the next refit", and rider 10 names the shared skin output buffer, so that a future narrowing of the all-slots fence wait (#4606 / #5365) migrates them first. The patch BLAS set has the same shape: one AS per patch mesh updated in place by `UPDATE` (`src == dst`), one scratch buffer shared by every frame in flight, and a vertex pool written in place. It is not on the list, and the pin test iterates a hand-written resource list that does not include it.
- **Evidence**:
  - `blas_terrain_patch.rs:395-410` records `UPDATE` with `.src_acceleration_structure(entry.accel)` into `entry.accel`.
  - `blas_terrain_patch.rs:130-134` (`ready_address`) and `:288-293` (the BUILD and UPDATE batch) use the one `terrain_patch_blas.scratch`.
  - `sync_and_acquire_frame` waits both slots' fences today, so there is no live hazard. The cross-frame ordering is also covered on-queue: the patch compute's entry barrier has src stages FRAGMENT|COMPUTE (`terrain_patch.rs`, `record`), its exit barrier now has dst `ACCELERATION_STRUCTURE_BUILD_KHR`, and `record_terrain_patch_blas` opens with an AS_BUILD to AS_BUILD barrier. That is a transitive chain, which I derived by reading, not by running validation.
- **Impact**: None today. If the wait is narrowed, the rider list is what tells the implementer which resources to re-derive; this set would be missed and its safety would rest only on a transitive barrier chain nobody pinned.
- **Related**: #4601, #4606, #5365.
- **Suggested Fix**: Add a rider line for the patch BLAS set and scratch (and the patch compute's in-place vertex rewrite) to `sync.rs`, and add the resource name to the pin list in `frames_in_flight_contract_names_every_dependent_resource`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
