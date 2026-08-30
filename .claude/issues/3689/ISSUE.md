# #3689 — PERF-D7-2026-08-30-04: `PackedStorage::remove_entities_erased` reallocates and moves every *surviving* row per unload batch, so eviction cost is O(all resident rows), not O(victims)

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D7-2026-08-30-04`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,ecs,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3689

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `crates/core/src/ecs/packed.rs:256-286`, driven from `byroredux/src/cell_loader/unload.rs:334` (`world.despawn_batch(victims)`) via `crates/core/src/ecs/world.rs:181-186`
- **Status**: NEW
- **Description**: The merge-compaction that #2397 introduced (correctly —
  the prior `Vec::remove` loop was quadratic) rebuilds both backing vectors
  from scratch on every call:
  ```rust
  let old_entities = std::mem::take(&mut self.entities);
  let old_data = std::mem::take(&mut self.data);
  let mut retained_entities = Vec::with_capacity(old_entities.len());
  let mut retained_data = Vec::with_capacity(old_data.len());
  ```
  `despawn_batch` calls it once per registered storage
  (`world.rs:181-186`), so a boundary eviction of three cells pays
  `2 × sizeof(row) × live_rows` of allocate-plus-move for **each**
  `PackedStorage` component type — `Transform`, `GlobalTransform`,
  `SceneFlags`, `WorldBound` in production — regardless of how few entities
  the three victim cells actually own. The retained 90-plus percent of the
  exterior population is copied out and back on every crossing, and eight
  allocations of the full live size are handed to the allocator and freed.
- **Evidence**: `packed.rs:265-268` above; the four production `PackedStorage`
  declarations are `crates/core/src/ecs/components/transform.rs:69`,
  `global_transform.rs:152`, `scene_flags.rs:118`, `world_bound.rs:109`. The
  cost is already isolated by telemetry: it is precisely
  `UnloadPhaseTimings::despawn` (`unload.rs:332-352`), aggregated into
  `StreamingTelemetry::unload_despawn`.
- **Impact**: Boundary-frame CPU that scales with the *resident* world rather
  than with what is being torn down, and allocator churn proportional to the
  same. This sits on the boundary frame **outside** any budget (the unload at
  `app_step.rs:141` runs before `streaming_deadline` is even computed at
  `:196`), so it cannot be yielded away.
- **Related**: #2397 (introduced the merge pass — this is a refinement of that
  fix, not a regression of it); #2396 (its sort-order / dirty-marking test);
  #2148 (`shrink_storages`, the sibling pass in `finish_unload_batch`).
  Storage internals are `/audit-ecs` territory; filed here because exterior
  eviction is the sole hot caller and it is the streaming boundary's cost.
- **Suggested Fix**: Do the compaction **in place** with a read/write cursor
  over `self.entities` / `self.data` (the `Vec::retain` shape, but driven by
  the sorted victim cursor so the `mark_dirty` call is preserved). Same single
  pass, same output order, zero allocations, and half the memory traffic. Both
  existing tests (`remove_entities_erased_preserves_ascending_order`,
  `remove_entities_erased_marks_exactly_the_removed_ids_dirty`) pin the
  observable contract and should pass unchanged.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
