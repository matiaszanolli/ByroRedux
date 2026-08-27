# SAFE-2026-08-27-03: The `MorphSlot` unload drain is nested inside the `skin_compute` + `accel_manager` guards it does not need

- **Issue**: [#3374](https://github.com/matiaszanolli/ByroRedux/issues/3374)
- **Finding ID**: `SAFE-2026-08-27-03`
- **Source report**: `docs/audits/AUDIT_SAFETY_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,renderer,memory,safety,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3374 --json state`.

---

- **Severity**: LOW
- **Dimension**: 3 (leaks)
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:65-68` (the enclosing guards), `:772-800` (the morph eviction + `pending_morph_unload_victims` drain); producer `byroredux/src/cell_loader/unload.rs:263-266`; resource `crates/renderer/src/vulkan/context/mod.rs:1478-1481`
- **Status**: NEW
- **Description**: `MorphSlot::destroy` needs only `&ash::Device` and the
  allocator — it owns two plain `GpuBuffer`s and no descriptor sets, by explicit
  design (`morph_compute.rs:1-16`). Its eviction loop and its
  `pending_morph_unload_victims` drain, however, sit inside
  `if let (Some(skin_pipeline), Some(ref mut accel)) = (self.skin_compute.as_ref(), self.accel_manager.as_mut())`,
  a guard inherited from the `SkinSlot` loop it was folded beside. Both of those
  are `None` when `device_caps.ray_query_supported == false`
  (`context/init.rs:428`, `:600`), or when skin-pipeline creation fails.

  `MorphSlot`s themselves are created with **no** such gate
  (`byroredux/src/cell_loader/spawn/mesh_instance.rs:725-746` — the only
  condition is `mesh.skin.is_some()` plus non-empty `morph_targets`). So in any
  configuration where those two options are `None`, morph delta buffers (up to
  `MAX_MORPH_TARGETS_PER_MESH` = 64 × `vertex_count` × 16 B per skinned entity)
  accumulate for the whole session across every cell load and unload, with no
  bound and no drain.

  **Why LOW and not HIGH**, stated plainly: `#2494` already established that
  this drain must not be trapped inside a narrower guard, and a per-cell GPU
  leak would ordinarily be HIGH. But the trigger is not reachable in a
  supported configuration today — `triangle.vert:7` carries
  `#extension GL_EXT_buffer_reference : require`, and `bufferDeviceAddress` is
  enabled only when `ray_query_supported` (`device.rs:743`), so a device that
  makes `accel_manager` `None` cannot create the main geometry pipeline at all.
  This is therefore a latent structural coupling, not a live leak. It is worth
  fixing because it is the `#2494` mistake one nesting level out, and because
  the RT-optional path is a plausible future.
- **Evidence**:
  ```rust
  // skinned_blas_refit.rs:65
  if let (Some(skin_pipeline), Some(ref mut accel)) =
      (self.skin_compute.as_ref(), self.accel_manager.as_mut())
  {
      if let Some(ref alloc) = self.allocator {
          …
          // :782 — needs neither skin_pipeline nor accel
          let mut morph_evictees: Vec<EntityId> =
              std::mem::take(&mut self.pending_morph_unload_victims);
  ```
  Teardown is unaffected — `context/teardown.rs:52-56` drains `morph_slots`
  wholesale on `Drop`, so this is a *session-lifetime* leak, not a
  leak-past-shutdown.
- **Impact**: None in any currently-supported configuration. Under a future
  RT-optional path, or a `skin_compute` creation failure on a live RT device,
  per-cell GPU memory grows without bound until process exit.
- **Related**: #2494 (the same class of over-nesting, one level in), #3231,
  #1003
- **Suggested Fix**: Hoist the `morph_evictees` block out to the
  `if let Some(ref alloc) = self.allocator` level (or above the skin guard
  entirely, taking `alloc` locally), and extend
  `skin_eviction_runs_without_global_vertex_buffer_tests` with a source-position
  assertion for the morph drain, mirroring the one `#2494` added for the skin
  drain.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_SAFETY_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `SAFE-2026-08-27-03`._
