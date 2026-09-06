# #3998 — REN-2026-09-06-D1-04: `memory-budget.md`'s AS constants ledger is one value wrong and one constant short — the skinned-refit rebuild threshold is no longer flat 600, and `MAX_STATIC_BLAS_RESTORES_PER_FRAME` has no row

**Labels**: low, memory, renderer, tech-debt, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (doc-rot)
- **Location**: `docs/engine/memory-budget.md` §"Acceleration Structures (BLAS /
  TLAS)" (the closing `SKINNED_BLAS_REFIT_THRESHOLD` paragraph, and the
  "Reserve floors" / "Scratch buffers" tables). Ground truth:
  `crates/renderer/src/vulkan/acceleration/constants.rs`
  (`SKINNED_BLAS_REFIT_JITTER`, `MAX_STATIC_BLAS_RESTORES_PER_FRAME`),
  `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`skinned_blas_refit_limit`, `should_rebuild_skinned_blas_after`,
  `plan_static_blas_restore`)
- **Status**: NEW (not in the OPEN cache; `#3866` covers only the budget formula
  row)
- **Description**: The section opens by linking `acceleration/constants.rs` and
  presents itself as that file's ledger. Two entries are now wrong or missing:
  1. *"BLAS refit count before a forced rebuild: `SKINNED_BLAS_REFIT_THRESHOLD` =
     600 frames (~10 seconds at 60 FPS). **After 600 refits** the BLAS is rebuilt
     from scratch."* Since `931241a7` (#3669, 2026-09-03) the effective limit is
     `skinned_blas_refit_limit(entity_id) = 600 + (entity_id % SKINNED_BLAS_REFIT_JITTER)`
     with `SKINNED_BLAS_REFIT_JITTER = 60` — i.e. a per-entity value in
     **600..=659**, deliberately staggered so a continuously animated cohort does
     not all drop and rebuild in the same frame. `931241a7` touched six files,
     none of them under `docs/`. This is a numeric value in an authoritative
     tuning table, not prose: an operator reading "600" and measuring a rebuild
     at frame 641 has no way to tell an expected stagger from a bug.
  2. `MAX_STATIC_BLAS_RESTORES_PER_FRAME = 256` (#3540, `0c45e779`, 2026-08-30)
     is a live per-frame bound on a GPU-memory-driven pass — it is what stops
     Starfield's `citycydoniamainlevel` sitting single-threaded on frame 0 for
     ten minutes with RSS oscillating 12→20.6 GB — and it has no row anywhere in
     the doc. `0c45e779` likewise touched no `docs/` file. Its companion policy
     function `plan_static_blas_restore` (the fit projection that declines the
     pass entirely when the visible set cannot fit the budget) is also
     undocumented, which means the doc gives no account of the one code path
     that can silently drop RT geometry on an over-budget cell.
- **Evidence**:
  - `predicates.rs`:
    `SKINNED_BLAS_REFIT_THRESHOLD.saturating_add(if SKINNED_BLAS_REFIT_JITTER == 0 { 0 } else { entity_id % SKINNED_BLAS_REFIT_JITTER })`,
    consumed by `should_rebuild_skinned_blas_after` → `should_rebuild_skinned_blas`.
    Pinned by `skinned_blas_rebuild_jitter_repeats_only_after_one_full_window`.
  - `git show --stat 931241a7` → 6 files, all under
    `crates/renderer/src/vulkan/acceleration/`; no `docs/`.
  - `git show --stat 0c45e779 | grep docs/` → empty.
  - `grep -n "MAX_STATIC_BLAS_RESTORES_PER_FRAME\|SKINNED_BLAS_REFIT_JITTER" docs/engine/memory-budget.md`
    → no matches.
- **Impact**: Documentation only, but of the class `_audit-common.md` singles out
  — *"a wrong number in a GPU layout contract, not a typo"*. The refit-threshold
  figure is the one an operator would use to reason about a skinned-BLAS rebuild
  spike, and the missing restore cap is the one that explains an
  RT-geometry-missing-on-a-huge-cell report.
- **Related**: `#3669` / `931241a7` (the jitter), `#3540` / `0c45e779` (the
  restore cap), `#679` / `AS-8-9` (the original threshold), `#3866` (the sibling
  budget-formula rot in the same section).
- **Suggested Fix**: Change the closing paragraph to
  `SKINNED_BLAS_REFIT_THRESHOLD` = 600 **plus** a stable per-entity
  `SKINNED_BLAS_REFIT_JITTER` (60) offset, effective 600–659, with the
  cohort-stagger rationale (#3669). Add a row for
  `MAX_STATIC_BLAS_RESTORES_PER_FRAME` = 256 and a sentence on
  `plan_static_blas_restore`'s two bounds (fit projection, then per-frame cap) to
  the "LRU eviction" or a new "Per-frame BLAS recovery" subsection.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
