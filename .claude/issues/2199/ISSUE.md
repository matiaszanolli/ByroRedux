# TD1-NEW-04: crates/nif/src/anim/tests.rs crossed 2000 LOC (2 lines over threshold)

**Labels**: low, nif, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD1-NEW-04)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2199

## Severity
LOW

## Dimension
1 (File/Function/Module Complexity)

## Location
`crates/nif/src/anim/tests.rs` (2002 LOC)

## Description
Marginal, mechanical crossing (2 lines over threshold) via ordinary test accumulation on the KF-animation import path; no organizational problem, same pattern as the already-fixed `shader_tests.rs`/`particle.rs` test-split precedent from this window.

## Evidence
`wc -l crates/nif/src/anim/tests.rs` → 2002 (confirmed live).

## Impact
None today — purely a threshold-crossing note for future split planning.

## Related
Same pattern class as #2053 (`particle.rs` split) and #2056 (`shader_tests.rs` split).

## Suggested Fix
If/when next touched, split along the existing per-phase boundaries the sibling `anim/` modules already use (`coord`, `controlled_block`, `transform`, `sequence`, `keys`, `channel`, `bspline`). Not urgent.

## Completeness Checks
- [ ] **SIBLING**: When split, mirror the `shader_tests/{mod,legacy,skyrim,fo4,fo76,starfield}.rs` per-era-sibling precedent
- [ ] **TESTS**: N/A — test-only file reorganization, no behavior change
