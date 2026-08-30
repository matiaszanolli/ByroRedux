# #3683 — PERF-D3-2026-08-30-04: post-#2929 doc rot on the TLAS shrink path — two comments still assert `shrink_tlas_to_fit` destroys the slot

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D3-2026-08-30-04`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3683

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:246-252`
  (`shrink_tlas_scratch_to_fit` case-1 doc),
  `crates/renderer/src/vulkan/context/draw.rs:4040-4047` (call-ordering comment)
- **Status**: NEW
- **Description**: #2929 / CON-D1-01 changed `shrink_tlas_to_fit` from
  "`take()` the slot and destroy the AS + its three buffers" to "set
  `tlas_shrink_pending[slot_index] = true` and let `ensure_tlas_state` fold the
  shrink into its allocate-then-swap path". The function body and its own
  `#2929` block comment are correct and the behaviour is verified below. Two
  *other* comments were not updated and now describe the removed behaviour:
  - `shrink_tlas_scratch_to_fit`'s case-1 doc says the arm handles
    "`tlas[slot_index]` is `None` (slot was destroyed by
    [`Self::shrink_tlas_to_fit`])". That producer no longer exists;
    `shrink_tlas_to_fit` never leaves the slot `None`.
  - `draw.rs`'s ordering comment justifies the call order as "run AFTER
    `shrink_tlas_to_fit` so a destroyed slot lets the scratch shrink hit its
    'tlas[slot] is None → drop scratch entirely' arm in one tick". That
    interaction cannot occur; the ordering is now arbitrary.
- **Evidence**:
  ```rust
  // acceleration/memory.rs — what the code actually does now
  let old_max = slot.max_instances;
  self.tlas_shrink_pending[slot_index] = true;   // request, do not destroy
  ```
  ```rust
  // acceleration/memory.rs:246-252 — what the sibling doc still claims
  /// 1. `tlas[slot_index]` is `None` (slot was destroyed by
  ///    [`Self::shrink_tlas_to_fit`]) — drop the scratch entirely.
  ```
- **Impact**: No runtime effect — both arms remain correct in isolation and the
  reserve floors hold (verified below). The cost is to the next reader of the
  shrink path: the stale comments make case 1 look reachable-by-design from the
  sibling call and make the `draw.rs` ordering look load-bearing when it is not,
  on a code path whose whole history (#1782, #2673, #2915, #2929) is
  destroy-ordering bugs. `AUDIT_RENDERER_2026-08-24` already flagged #2774's
  case-2 reachability claim as needing re-verification for the same reason.
- **Related**: #2929, #2915, #2673, #2774 (case-2 reachability, flagged
  2026-08-24 and still open).
- **Suggested Fix**: Reword case 1 to name its real producers (fresh slot at
  startup; a slot never rebuilt after a failed `ensure_tlas_state`) and drop the
  "order matters" claim in `draw.rs`, or state the real reason to keep the order
  if one is wanted.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
