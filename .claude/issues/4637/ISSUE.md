# NIFAL-D9-2026-09-21b-01: #4523 dark-role census false-FAILs whenever Oblivion data is absent but another game resolves

**Issue**: #4637
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: LOW
**Dimension**: Completeness
**Tier Violated**: harness-gap
**Game Affected**: all (the harness)
**Location**: `crates/nif/tests/translation_completeness.rs:889-975` (`dark_texture_role_population_matches_the_documented_census`, `#[ignore]`, #4523). Skips only when `probed == 0` (`:955-958`); otherwise unconditionally asserts `total_dark == 8` (`:963`) and `oblivion_dark == 8` (`:970`).

## Description
All 8 "dark" hits in the pinned census come from Oblivion alone. Any single-game run that resolves data for a game other than Oblivion sees `total_dark == 0` and fails, even though the population hasn't changed. Confirmed: an FO4-only run panics `left: 0, right: 8`; an Oblivion-only run passes with 8 DARK HITs in 6 files. This leg's own one-game-at-a-time Dim 9 invocation hit exactly this failure mode.

## Impact
Any `--ignored` run on a lane without Oblivion data reports a false population "drift", inviting a wrong fix chasing a regression that doesn't exist.

## Suggested Fix
Assert zero per resolved non-Oblivion game individually; assert `== 8` only when Oblivion resolved, and print SKIP otherwise (mirroring the existing `probed == 0` pattern).

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D9-2026-09-21b-01)
