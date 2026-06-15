**Severity**: LOW · **Dimension**: SF ESM Resolve-Rate (doc accuracy)
**Location**: `byroredux/src/sf_smoke.rs:9,132`; `docs/engine/starfield-esm-phase0-baseline.md:132`
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-04)

## Description
Several docstrings state an unindexed-base REFR "will spawn the 3D-unit-cube placeholder." The actual cell-loader behaviour on a `statics.get` miss is a silent `continue` — no placeholder mesh is created.

## Evidence
`byroredux/src/cell_loader/references.rs:362-378` — `None => { stat_miss += 1; … continue; }`. No spawn/cube on the miss branch.

## Impact
Misleading diagnostics only — an operator would expect to *see* unit cubes for the 11.2% gap and wrongly conclude rendering is fine when content is invisibly missing.

## Related
SF-D4-02 (the silent-skip behaviour these docstrings misdescribe).

## Suggested Fix
Reword to "silently skipped (no geometry spawned)"; optionally add a debug-only placeholder-cube spawn behind a flag so the documented behaviour is actually available for visual triage.

## Completeness Checks
- [ ] **SIBLING**: All three doc sites (`sf_smoke.rs:9`, `:132`, `starfield-esm-phase0-baseline.md:132`) are corrected together
- [ ] **TESTS**: N/A (doc-only) — if a debug placeholder flag is added, a test pins it off by default
