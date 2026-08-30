# #3687 — PERF-D6-2026-08-30-02: `update_morph_weights` heap-allocates a fresh `Vec<f32>` per morph slot per frame and unconditionally marks the slot dirty, discarding the right-sized `pending_weights` buffer `MorphSlot` already owns

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D6-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3687

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `byroredux/src/render/skinned.rs:285-292`; `crates/renderer/src/vulkan/morph_compute.rs:175-183,188-191`
- **Status**: NEW — first flagged as the unfixed half of `PERF-D6-2026-08-24-01`
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-24.md:231-281`, re-confirmed still open at
  `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md:119`), which was deferred into #3061
  "one-pass conversion" and never filed on its own. **#3061 is now CLOSED**
  (`c82f4f29`) and its commit body scopes it to the `FxHashMap`/`FxHashSet`
  conversion only — so this half fell through the close. Credit to the 2026-08-24
  audit for the original observation.
- **Description**: `MorphSlot::create` already allocates `pending_weights: vec![0.0;
  target_count]` (`morph_compute.rs:165`) — a permanently right-sized staging buffer.
  `update_morph_weights` nonetheless builds a brand-new `Vec<f32>` via `collect()`
  every frame for every live morph slot and hands it to `stage_weights`, which
  *replaces* `pending_weights` — dropping (freeing) the previous allocation. That is
  one malloc + one free per morphed entity per frame on the per-frame render path,
  for data that is almost always byte-identical to what the slot already holds.
  The same call also sets `pending_weights_dirty = true` unconditionally, so
  `flush_pending_weights` re-executes its mapped-memory `copy_from_slice` +
  `flush_if_needed` for every slot every frame even when no weight changed — the
  early-out at `morph_compute.rs:189` can never fire in steady state.
- **Evidence**:
  ```rust
  // byroredux/src/render/skinned.rs:285-292 — per frame, per morph slot
  for (&entity, slot) in ctx.morph_slots.iter_mut() {
      let Some(weights) = weights_q.get(entity) else { continue; };
      let target_count = slot.target_count() as usize;
      let flat: Vec<f32> = (0..target_count).map(|i| weights.get(i)).collect();   // malloc
      slot.stage_weights(flat);                                                   // free of the old one
  }
  ```
  ```rust
  // crates/renderer/src/vulkan/morph_compute.rs:175-183
  pub fn stage_weights(&mut self, weights: Vec<f32>) {
      …
      self.pending_weights = weights;        // discards the pre-sized buffer
      self.pending_weights_dirty = true;     // unconditional → flush can never early-out
  }
  ```
- **Impact**: Bounded and small per entity, but it is squarely on the per-frame
  per-entity render path the #2923/#3061 hot-path rule exists to keep clean, and it is
  the last remaining allocator traffic on that path. Size is unmeasurable today
  because `SkinCoverageFrame` carries no morph counter — the number of live
  `MorphSlot`s is a strict subset of `skin_pool_live` (248 / 206 / 83 on the FO4 /
  FNV / Skyrim baselines) but is not itself recorded anywhere. This finding is
  deliberately LOW: it is allocator churn and a redundant small mapped write, not a
  per-frame leak.
- **Related**: `PERF-D6-2026-08-24-01` (origin), #3061 (CLOSED — covered only the
  hashing half), #3231 (landed the morph path), #3244 (the dual-fence rule
  `flush_pending_weights` implements — any fix must keep the flush after the fence
  wait, only make it conditional).
- **Suggested Fix**: Change `stage_weights` to take `&[f32]` (or a closure) and write
  in place into `pending_weights`, setting `pending_weights_dirty` only when the new
  values differ from the stored ones. `update_morph_weights` then writes directly into
  the slot's own buffer with no allocation and no `collect()`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
