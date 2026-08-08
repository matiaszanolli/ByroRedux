# EXAL-05: climate_tod_hours — a canonical environment default — lives outside the EXAL boundary module

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2453
**Finding ID**: EXAL-05 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — EXAL
**Location**: `byroredux/src/scene/world_setup.rs:190-214`, consumed at `env_translate.rs:363`
**Status**: NEW

## Description
The CLMT `TNAM` decode + its hardcoded no-data fallback and corruption guard live in `world_setup.rs`, which `env_translate::translate_weather` reaches back out to — a single implementation (not a duplicate-producer finding) but living outside the module exal.md §3 designates as home, inverting the intended dependency direction.

## Suggested Fix
Move `climate_tod_hours`+`FALLBACK` into `env_translate.rs` verbatim; behavior-preserving, ~20 lines.

## Completeness Checks
- [ ] **TESTS**: Existing `climate_tod_hours` tests still pass unchanged after the move
