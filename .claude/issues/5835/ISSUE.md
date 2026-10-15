# TD3-2026-10-15-11: The #5470 space-run gate still exempts `byroredux/src/main.rs`, hiding four collapsed-continuation messages (carried, third report, never filed)

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-11`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments (text hygiene)
- **Location**: `byroredux/src/main.rs:338, 344, 415, 423`; exemption `byroredux/src/workspace_hygiene_tests.rs:397`
- **Status**: Regression of #5470 (CLOSED); carries TD3-2026-10-12-02 / 10-11 TD3-01
- **Age**: `f8ea9d46f` (2026-08-31)
- **Effort**: trivial
- **Description**: unchanged since the 10-12 report: `grep -nE '"[^" ][^"]*[^ "] {6,}[^ "][^"]*"' byroredux/src/main.rs | grep -v '\\n'` returns the same four lines (line numbers shifted by +6).
- **Evidence**: command above.
- **Impact**: cosmetic; the exemption is a blind spot.
- **Related**: #5470, #5481.
- **Suggested Fix**: re-wrap with `\` continuations; delete the `main.rs` exemption.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
