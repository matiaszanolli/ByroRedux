# #3669 — PERF-D6-2026-08-30-01: `SKINNED_BLAS_REFIT_THRESHOLD` has no per-entity stagger, so a cell's NPC cohort drops and rebuilds its skinned BLASes in lockstep every 601 dirty frames

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D6-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,pipeline,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3669

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs:270-281`; `crates/renderer/src/vulkan/acceleration/constants.rs:68`; `crates/renderer/src/vulkan/acceleration/predicates.rs:107-109`; `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:130-200,297-300,366`
- **Status**: NEW
- **Description**: `refit_count` starts at `0` for every skinned BLAS the moment its
  fresh BUILD registers (`blas_skinned.rs:366`) and is advanced by exactly one per
  *dirty* frame. `should_rebuild_skinned_blas_after` is a bare
  `refit_count >= SKINNED_BLAS_REFIT_THRESHOLD` with no per-entity offset, jitter, or
  per-frame rebuild budget. Every entity that first-sights in the same frame and then
  animates continuously therefore reaches 600 on the *same* frame, and
  `record_skinned_blas_refit` drops all of them and re-queues all of them into the
  same frame's `first_sight_builds` batch. An NPC playing any looping idle animation
  is pose-dirty every frame (the idle clip moves bones, so the FNV-1a pose hash
  changes), so "animating continuously" is the normal case for a populated interior,
  not a corner case.
- **Evidence**:
  ```rust
  // skinned_blas_refit.rs:270-281 — unconditional, per entity, per frame
  if accel.should_rebuild_skinned_blas(entity_id) { … accel.drop_skinned_blas(entity_id); }
  let needs_blas = accel.skinned_blas_entry(entity_id).is_none();   // now true → first_sight_builds
  ```
  ```rust
  // acceleration/predicates.rs:107-109 — no stagger term
  pub(super) fn should_rebuild_skinned_blas_after(refit_count: u32) -> bool {
      refit_count >= SKINNED_BLAS_REFIT_THRESHOLD          // = 600, constants.rs:68
  }
  ```
  The rebuild is *not* cheap per entity: `build_skinned_blas_batched_on_cmd` does a
  host `get_acceleration_structure_build_sizes` (`blas_skinned.rs:142`), a fresh
  `GpuBuffer::create_device_local_uninit` for the AS store (`:150`), and a
  `create_acceleration_structure` (`:170`) **per entity**, then records the builds
  with a full `record_scratch_serialize_barrier` between every pair
  (`:297-300`) — the same shared-scratch serialization #1797 documents for refits,
  but with BUILD-sized work instead of UPDATE-sized work.
- **Impact**: A periodic, self-synchronising frame spike roughly every 10 s of
  continuous NPC animation (600 dirty frames @ 60 FPS), scaling with the cohort size.
  Checked-in baselines put the resident skinned population at `skin_pool_live = 248`
  (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`), `206`
  (`fnv-FreesideAtomicWrangler.tsv`) and `83`
  (`skyrim_se-WhiterunDragonsreach.tsv`). The cell loader's spawn budget
  (`byroredux/src/cell_loader/work_budget.rs`) spreads first-sight over the load
  window, so the cohort is several sub-groups rather than one — the burst is spread
  over a handful of frames, not concentrated in one — but it recurs on a fixed
  10 s period and nothing damps it. The spike is already observable without new
  instrumentation: `first_sight_attempted` / `first_sight_succeeded` and
  `cpu_skin_chain_ns` on `SkinCoverageFrame` (`skin.coverage` / `bench-stats
  --break-down skin`) jump together on the rebuild frame, and `gpu_skin_blas_refit_ms`
  covers the device side.
- **Related**: #679 / AS-8-9 (introduced the threshold), #1797 (the shared-scratch
  serialization ceiling this rides on — BUILDs share the same barrier chain as
  refits), #1196 (the refit gate that decides which entities advance `refit_count`),
  #1812 (`built_this_frame`, which correctly suppresses the *redundant refit* after a
  rebuild but does nothing about the rebuild clustering itself).
- **Suggested Fix**: Make the threshold per-entity rather than global — e.g. compare
  against `SKINNED_BLAS_REFIT_THRESHOLD + (entity_id % SKINNED_BLAS_REFIT_JITTER)`
  inside `should_rebuild_skinned_blas_after`, or cap the number of threshold-triggered
  rebuilds admitted per frame and let the rest slip to the next. Both are a change to
  one pure predicate plus a constant, and both are unit-testable exactly the way
  `should_rebuild_skinned_blas_after` already is.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
