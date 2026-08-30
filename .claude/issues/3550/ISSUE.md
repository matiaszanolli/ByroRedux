# RT-4: the `tex_missing_unique_paths` baseline contract is broken — #3349 widened the metric after all five baselines were captured

**Issue**: #3550
**Labels**: bug, medium, tech-debt, test-gap
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-4.

## Description

`ff177576` (2026-08-28, **#3349** "per-slot tex.missing") changed `tex.missing` from walking the single `TextureHandle` (base-color only) to walking the full **26-role** `MaterialTextureHandles` set. **Every committed runtime baseline predates it**: oblivion 2026-08-26, fnv 2026-08-26/27, fo3 2026-06-14, fo4 2026-08-22, skyrim_se 2026-08-06.

The gate compares a 26-slot number against a 1-slot number. **The naive diff produces five false HIGH regressions.**

## Evidence

Re-scored against the *pre-#3349* surface (base-color slot only), the picture inverts:

| Game | total (new surface) | base_color only | baseline | true verdict |
|---|---|---|---|---|
| fnv | 6 | 1 | 1 | **EXACT PASS** |
| fo3 | 12 | 0 | 0 | **EXACT PASS** |
| fo4 | 16 | 1 | 1 | **EXACT PASS** |
| skyrim_se | 10 | 0 | 0 | **EXACT PASS** |
| oblivion | 8 | 1 | 0 | **+1 real** (tracked separately as RT-9) |

## Impact

The `tex_missing_unique_paths` row is currently unassertable: it will report a regression on every game on every future sweep until the surfaces are reconciled. A metric that always fails is a metric nobody reads — and it will mask the one genuine miss (oblivion) inside four false ones.

## Suggested Fix

Either:
1. regenerate all five `tex_missing_unique_paths` rows against the current surface with `--regen`; **or** (preferred)
2. split the metric into `tex_missing_base_color` (the strict gate, comparable across the surface change) and `tex_missing_all_slots` (informational).

Do **not** file the raw deltas as regressions.

## Completeness Checks
- [ ] **SIBLING**: Every other runtime baseline row checked for a metric-definition change since its own capture date (this is a class, not a one-off)
- [ ] **TESTS**: The baseline TSV carries the metric-surface generation/date so a future surface change is detectable rather than silent
