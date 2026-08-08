# OBL-D7-02: Doc drift: ROADMAP.md's Oblivion exterior compat-matrix entity/FPS figure is stale against the newer, more thorough readiness-plan bench

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2575
**Finding ID**: OBL-D7-02

**Severity**: LOW
**Dimension**: Exterior Blocker Chain & Game-Specific Quirks
**Location**: `ROADMAP.md:430` vs `docs/engine/exterior-readiness-plan.md`
**Status**: NEW

## Description
`ROADMAP.md` still cites "4,886 entities / 150.6 FPS" for Tamriel `(0,0)` radius-1; the 2026-08-04 EX-01 sweep re-ran the identical profile and recorded 5,709 entities / 2,355 draws with an explicit image-health pass — a denser, more validated measurement of the same scenario, landed in the same commit window that touched `ROADMAP.md` for an adjacent edit but left this figure untouched.

## Evidence
Confirmed directly: `ROADMAP.md:430` still reads "Tamriel `(0,0)` radius 1 recorded 4,886 entities / 150.6 FPS."

## Impact
Documentation-only; risk is a future contributor misreading the delta as a regression.

## Suggested Fix
Update `ROADMAP.md:430` to cite the 2026-08-04 figures and/or point at `docs/engine/exterior-readiness-plan.md` as the live source.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
