# #3675 — PERF-D9-2026-08-30-02: `batches_scratch`'s per-frame `reserve()` and its end-of-frame shrink fight each other — two reallocations and a memcpy every frame on four of five baseline cells

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,memory,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3675

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Telemetry & Origin Cost (chronic scratch over-reserve)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:2810-2812` (reserve), `:3978-4006` (shrink), predicate at `crates/renderer/src/vulkan/acceleration/predicates.rs:394-399`
- **Status**: NEW
- **Description**: `batches` is reserved to **`draw_commands.len()`** but filled
  to the post-merge **batch count**, which the repo's own baselines put 13–19×
  lower. At frame end the shrink policy targets `2 × max(batch_count, 512)`.
  For every baseline cell where `draw_commands.len() > 2 × max(batch_count, 512)`
  — four of the five — the shrink fires, and the next frame's `reserve()`
  immediately grows the Vec back. The result is a `shrink_to` realloc (copying
  the live batches) plus a growth realloc, **every frame**, on the render hot
  path — precisely the churn the field's own doc says it eliminates
  ("`mem::take` … amortizing their capacity across frames … See issue #243").
  The other members of the cluster do not thrash: `gpu_instances_scratch` and
  `previous_models_scratch` have working sets ≈ `draw_commands.len()`, so
  `2×working` comfortably exceeds the reserve.
- **Evidence**:
  ```rust
  // draw.rs:2810-2812
  let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
  batches.clear();
  batches.reserve(draw_commands.len());     // ← keyed to the WRONG quantity
  ...
  // draw.rs:3978-4006
  let working_batches = batches.len();      // ← the RIGHT quantity
  self.batches_scratch = batches;
  super::super::acceleration::shrink_scratch_if_oversized(
      &mut self.batches_scratch, working_batches, 512);
  ```
  ```rust
  // predicates.rs:394-399
  let target = 2 * working_set.max(floor);
  if vec.capacity() > target { vec.shrink_to(target); }
  ```
  Worked through with `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`
  (`bench_draws_cmds 3949`, `bench_draws_batches 296`): reserve forces
  `capacity ≥ 3949`; shrink target is `2 × max(296, 512) = 1024`; `3949 > 1024`
  so `shrink_to(1024)` reallocates and copies 296 elements; next frame
  `clear()` leaves `len = 0, cap = 1024` and `reserve(3949)` reallocates again.
  Same arithmetic holds for fnv (2110 vs 1024), fo3 (1581 vs 1024) and skyrim
  (2342 vs 1024). Only oblivion (325 < 1024) escapes.
- **Impact**: Two heap reallocations plus one `memcpy` of the live batch array
  per frame, at 60–150 fps, on every cell dense enough to matter — a permanent
  allocator-traffic floor on the exact path #243 was filed to remove. Byte
  magnitude is `size_of::<DrawBatch>()` × the counts above; I am not quoting a
  byte figure because `DrawBatch`'s layout is unpinned by any size test. Host
  RAM only — no GPU allocation, no leak — hence MEDIUM, not HIGH. It is also
  the single largest `wasted_bytes` contributor `ctx.scratch` would report, so
  the telemetry that exists already points at it.
- **Related**: #243 (the amortization the reserve defeats); #2486 / D5-01 (which
  extended the *shrink* half of the policy to the rest of the cluster but did
  not revisit the reserve arguments); `docs/audits/AUDIT_PERFORMANCE_2026-08-12.md:946`
  marked #243 PASS on the basis of "all `mem::take`+`clear`+`reserve`d", which is
  true and still misses this.
- **Suggested Fix**: Either drop the `reserve` (let `push` amortize from the
  retained capacity — `Vec`'s own growth is already amortized O(1)), or key it
  to the batch count instead: reserve `self.batches_scratch.capacity()`-worth by
  reserving nothing, or track last frame's `working_batches` and reserve that
  with a slack factor. A one-line change either way; the shrink policy is fine
  as is.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
