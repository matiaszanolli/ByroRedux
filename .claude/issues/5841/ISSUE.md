# TD5-2026-10-15-01: `scene.rs` carries a past-tense "TODO" that re-matches the marker grep on every audit, naming a closed issue

Labels: low,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD5-2026-10-15-01`).

- **Severity**: LOW
- **Dimension**: 5 Stale Markers
- **Location**: `byroredux/src/scene.rs:1290`
- **Age**: `e6192cc573`, 2026-05-14 (5 months)
- **Effort**: trivial
- **Status**: NEW (flagged as noise in the 10-12 report's Dim 5 line, never actioned)
- **Description**: The comment "Closes the #242 consumer-side TODO (#1055)" documents a *finished* piece of work (the geometry-SSBO StagingPool reuse), but contains the literal token `TODO` and a closed issue number. Every run of the Dim 5 recipe (and the Phase-1 marker baseline) re-surfaces it and an auditor must re-triage it as "past tense".
- **Evidence**: `// rebuilds. Closes the #242 consumer-side TODO (#1055).` - #242 CLOSED, #1055 is the fix.
- **Impact**: Pure grep noise, 1 of the 24 baseline hits; no behaviour.
- **Related**: `AUDIT_TECH_DEBT_2026-10-12.md` Dim 5 summary.
- **Suggested Fix**: Reword to "Closes #242's consumer side (#1055)." so the Dim 5 baseline drops to 23 real-or-false-positive hits and the grep means something.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
