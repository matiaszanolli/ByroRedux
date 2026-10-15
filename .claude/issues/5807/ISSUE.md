# TD1-2026-10-15-09: `effect_kind` is a 95-arm variant-name table pasted into two example binaries, edited by hand on each new `Effect` variant

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-09`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity (match arms >50; Dim 2 overlap, hand to the Dim 2 agent if it already has it)
- **Location**: `crates/scripting/examples/mq101_conformance.rs:117-215` and `crates/scripting/examples/fragment_coverage.rs:187-285`
- **Status**: NEW
- **Age**: 56 commits touch the two examples in three days (M47.2)
- **Effort**: trivial
- **Description**: the two functions are the same 99-line `match` over every `Effect` variant returning its name (only the arm order of `Disable`/`Enable` differs). `Effect` already derives `Debug` and has an `impl Effect` (L1597, `is_placeholder`).
- **Evidence**: `diff` of the two ranges differs only in the parameter name and two arms' position.
- **Impact**: a new variant that misses one copy mis-reports in that harness; the harnesses are the M47 coverage gates.
- **Related**: finding 02 (owns the enum), `mq101_conformance.rs::run` = #5586.
- **Suggested Fix**: `impl Effect { pub fn kind(&self) -> &'static str }` (or `strum::IntoStaticStr` if already a dependency) next to `is_placeholder`; delete both copies.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
