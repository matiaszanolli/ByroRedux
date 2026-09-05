# #3842: PERF-D3-2026-09-05-04: the `compute_blas_budget` doc comment is orphaned onto `build_instance_map`, and its "`VRAM / 3`" phrasing survived #3043

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-04) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3842 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-04), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:271-314`, `:477`
- **Status**: NEW
- **Description**: Lines 271-276 open with "Compute the BLAS memory budget
  as `VRAM / 3` with a 256 MB floor. … See #387." — but line 277 continues
  the *same* `///` run with "Build the shared `draw_idx → ssbo_idx`
  mapping…", so the whole block is one doc comment attached to
  `build_instance_map` (line 298). The real `compute_blas_budget` sits 430
  lines further down at 707 with a different, correct doc. Separately, the
  phrasing is now inaccurate: #3043 deliberately changed the derivation from
  "sum of device-local heaps" to "the specific DEVICE_LOCAL heap backing a
  BLAS-usage buffer, selected by `memory_type_bits`", precisely to avoid
  summing aliased heaps or mistaking a small BAR aperture for main VRAM.
  "`VRAM / 3`" (line 271) and "the budget itself is VRAM/3" (line 477, in
  `should_evict_mid_batch`) both re-assert the superseded model.
- **Evidence**: `predicates.rs:271` `/// Compute the BLAS memory budget as
  \`VRAM / 3\`…` immediately followed at `:277` by `/// Build the shared
  \`draw_idx → ssbo_idx\` mapping that`, with no intervening item. First
  `pub fn` after the block is `build_instance_map` at `:298`.
- **Impact**: Documentation only, but it's the class of drift the project's
  own path/symbol gate exists to catch, and it puts a wrong VRAM model in
  front of the exact audience reading the eviction predicates for
  PERF-D3-2026-09-05-01.
- **Related**: #3043, #387, #3824 (`STATIC_BLAS_FLAGS` doc naming a deleted
  function — same file family, same drift class).
- **Suggested Fix**: Move lines 271-276 down to `compute_blas_budget` at
  `:707` (merging with the doc already there); reword both it and `:477` to
  "one third of the BLAS-capable DEVICE_LOCAL heap (#3043)".
- **Confidence**: High.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
