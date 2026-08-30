# #3690 — PERF-D7-2026-08-30-05: `unload_cells` recomputes the whole-world cinematic-retention set once per victim cell, then hash-probes every victim against it even when it is empty

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D7-2026-08-30-05`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3690

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/unload.rs:18-48` (`cinematic_retained_entities`), `:176-177` (per-cell call + `retain`), driven per victim from `:112-122` (`unload_cells`)
- **Status**: NEW
- **Description**: `unload_cell_inner` opens with
  ```rust
  let retained = cinematic_retained_entities(world);
  victims.retain(|entity| !retained.contains(entity));
  ```
  `cinematic_retained_entities` is a **whole-world** property: it queries
  `HorseTetherState`, `ActorCinematicState` and `Children`, and walks the
  render hierarchy of whatever it finds. `unload_cells` calls
  `unload_cell_inner` once per root (`:118`), so the boundary's three-cell
  eviction ring builds that set three times. Its inputs cannot change between
  those calls — retained entities are explicitly removed from `victims`
  (`:177`) so they are never among the entities `despawn_batch` drops.

  Second, `victims.retain(...)` is an unconditional `std::collections::HashSet`
  (SipHash) probe per victim entity. `retained` is empty in every session that
  is not mid-cinematic, which is the universal case — the vanilla content that
  populates `HorseTetherState` / `ActorCinematicState` is a handful of scripted
  vehicle sequences.
- **Evidence**: `unload.rs:18-48` builds a fresh `HashSet<EntityId>` and takes
  three query read-guards on every call; `unload.rs:118`
  (`timings.absorb(unload_cell_inner(world, ctx, cell_root))`) is inside the
  per-root loop. The `retain` at `:177` has no `retained.is_empty()` guard, and
  the sibling early-out on the very next line (`if !retained.is_empty()`, `:178`)
  shows the author already had the predicate in hand for the other half.
  Charged to `UnloadPhaseTimings::ownership_index` (`:174`), so the fix is
  directly verifiable against `StreamingTelemetry::unload_ownership_index`.
- **Impact**: Small but on the unbudgeted boundary frame, and it scales with
  victim count × victim cells. `drain_streaming_state`'s whole-resident-set
  teardown makes it worse: 49 roots at `--radius 3`, 121 at
  `DEFAULT_TRANSITION_RADIUS = 5` (`app_step.rs:931`) — that is 121 whole-world
  cinematic scans for one door transition, where one would do.
- **Related**: #3380 (the victim-dedup discipline in the same function); #3386
  (the batching this finding extends — `unload_cells` hoisted
  `finish_unload_batch` out of the per-root loop but left this in it).
- **Suggested Fix**: Compute `cinematic_retained_entities` once in
  `unload_cells` and pass `&HashSet<EntityId>` down to `unload_cell_inner`
  (with `unload_cell` computing it for its single root), and skip the `retain`
  + the `CellRoot` removal entirely when the set is empty.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
