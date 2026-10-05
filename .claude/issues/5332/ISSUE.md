# #5332: TD4-2026-10-05-03: ROADMAP.md is 818 lines against the 800-line cap that #5111 restored; fix commits between closes crossed it (regression of #5111)

Labels: low,tech-debt,bug
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD4-2026-10-05-03) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `ROADMAP.md` (818 lines); the cap is at `.claude/commands/session-close/SKILL.md:264,328`
- **Status**: Regression of #5111 (CLOSED 2026-10-01, "budgets restated as measured ceilings (README ≤ 600,
  ROADMAP ≤ 800) with a wc -l check")
- **Effort**: small
- **Evidence**:

  | Commit | Date | ROADMAP lines |
  |---|---|---|
  | `86a883251` (session 93 close) | 10-01 | 797 |
  | `273692be5` (slice P2 gates) | 10-02 | 801 |
  | `876ad2ae9` (session 94 close, re-trimmed) | 10-03 | 800 |
  | `1dc538108` (Fix #5067) | 10-04 | 818 |
  | `7fab80a26` | 10-04 | 818 |

  README is at 580, within its cap.
- **Description**: the `wc -l` check lives only in session-close Step 6. Any `Fix #N` commit that edits ROADMAP
  between closes can breach the cap silently, and the next close inherits the trim.
- **Suggested Fix**: trim to ≤ 800, and add the two `wc -l` ceilings to an always-run gate
  (`scripts/check-text-source-integrity.sh` or a `workspace_hygiene_tests` case) so the cap holds per commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
