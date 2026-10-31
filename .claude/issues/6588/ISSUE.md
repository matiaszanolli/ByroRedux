# REG-2026-10-31-03: Regression of #5332: ROADMAP.md is 808 lines against the 800-line ceiling, and `cargo test` is red on it

Labels: medium,documentation,doc-rot,tech-debt
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM
**Dimension**: CI gates / closed-fix regression (doc-rot)
**Source**: `docs/audits/AUDIT_REGRESSION_2026-10-31.md` REG-2026-10-31-03
**Status**: regression of #5332 (closed 2026-10-12, "ROADMAP back under 800 with the ceiling enforced per commit"), which was itself a regression of #5111. `3b668d713` (10-26) and `43252c859` (10-29) each trimmed it back.

## Location
`ROADMAP.md` (808 lines, re-measured at HEAD `670f81917` with `wc -l`). The ceiling is enforced by `byroredux/src/workspace_hygiene_tests.rs:871` (`readme_and_roadmap_hold_their_line_ceilings`). Introduced by `8985a2e99` ("docs(render): fire lighting visual A/B result", 2026-10-31 12:19), which took the file from 798 to 808 lines.

## Description
ROADMAP sat at 798 lines for the whole M43 series and the next docs commit pushed it 8 lines over.

## Evidence
`ROADMAP.md is 808 lines against its 800-line ceiling (#5111) - trim before committing (#5332)`. It is the only failure in the audit's local `cargo test --workspace` run (11,646 passed) and it fails in both the `cargo test` job and the ABBA lock-order job in CI run 41215450288.

## Impact
`cargo test` on `main` is red, which masks any other test regression and keeps the #6331 release gate shut.

## Suggested Fix
Move the added lines (or older superseded ones) to `HISTORY.md`. The rule is "trim before committing", so the `/session-close` flow, not the individual docs commit, is where this keeps slipping in; consider making session-close re-run the ceiling test before it finishes.

## Related
#5111, #5332, #6375, #6378, #6331

## Completeness Checks
- [ ] **TESTS**: `readme_and_roadmap_hold_their_line_ceilings` is green after the trim
- [ ] **SIBLING**: README.md ceiling checked in the same run
