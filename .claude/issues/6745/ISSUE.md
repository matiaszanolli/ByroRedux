# REN-D5-03: the scene-linear capture staging has no memory-budget row

Labels: low,renderer,memory,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6745

**Severity**: LOW · **Dimension**: Memory/Lifecycle

## Location
`docs/engine/memory-budget.md`; `LinearCaptureState.staging` (`ensure_linear_capture_staging`, GpuToCpu, render extent x 8 B, about 16.6 MB at 1080p)

## Description
The scene-linear capture staging is allocated on first capture and kept until teardown or a larger request; like its screenshot and depth twins it has no ledger row. Teardown is correct (`destroy_linear_capture_staging` is in `Drop`).

## Evidence
`grep -i "capture staging" docs/engine/memory-budget.md` is empty.

## Impact
Documentation only; debug-CLI paths.

## Suggested Fix
Add one "diagnostic capture staging (host, on demand)" row covering screenshot, depth and linear capture. (The local-grid ledger half is REN-D8-01.)

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
