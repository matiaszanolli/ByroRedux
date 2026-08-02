# REN-D21-01: Cornell RT harness has no FogVolume probe and its global fog medium rounds to ~0 optical depth at Cornell scale

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2248

**Dimension**: 21 (Cornell harness coverage)
**Location**: `byroredux/src/cornell.rs` (no `FogVolume` entity spawned; the harness's global fog medium is fit to Bethesda-cell-scale numbers, not the ~14-unit Cornell box)
**Status**: NEW

**Description**: Local fog volumes (the other big Session 62 feature besides fire-refraction) have no dedicated Cornell probe, and the harness's existing global fog medium is authored at Bethesda-cell scale, which rounds to approximately zero optical depth across the Cornell box's ~14-unit extent. `--cornell` therefore currently returns a false all-clear for any fog regression — the same trap `#1942` fixed for the sun path.

**Evidence**: `grep -n "FogVolume" byroredux/src/cornell.rs` returns no matches.

**Impact**: Any future regression in the volumetric fog pipeline (global or local) will not be caught by the Cornell harness, the project's primary fast RT-correctness regression check.

**Related**: #1942 (the analogous sun-path Cornell-scale trap, already fixed)

**Suggested Fix**: add a `FogVolume` probe entity to the Cornell scene, and scale up (or add a Cornell-specific override for) the global fog medium's extinction so it produces measurable optical depth across the box.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
