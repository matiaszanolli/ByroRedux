# TD4-2026-09-29-02: session-close SKILL's README (<120 lines) and ROADMAP (~500) budgets are both broken

**Labels**: low,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: small
- **Location**: `.claude/commands/session-close/SKILL.md:258` and `:317` (last edited `da3a437bf` 09-01)
- **Evidence**:
  - README.md is 533 lines. It was 382 on 08-01, 492 on 09-01 and 527 on 09-15.
  - ROADMAP.md is 759 lines, trimmed from 1657 on 09-22, and still 1.5× its budget.
  - The ritual runs every session (`8b334c102` today) and never enforces either rule.
- **Suggested Fix**:
  - Either trim README to quick start + pointers, or restate the budgets as measured ceilings.
  - Add a `wc -l` check to Step 6.

**Validated at HEAD 9fcfdc3fc**: `.claude/commands/session-close/SKILL.md` still says "README should stay < 120 lines" and "Don't grow ROADMAP past ~500 lines"; `wc -l` gives README.md 533, ROADMAP.md 759.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
