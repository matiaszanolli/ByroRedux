# TD4-003: audit-scripting SKILL.md's Phase-1 baseline is fully stale — no prior audit / all preloaded issues closed

**GitHub Issue**: #2044
**Labels**: high,tech-debt,documentation

**Severity**: HIGH
**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-scripting/SKILL.md:103,146-152`

## Description
The skill claims "No prior scripting audit exists" — false, six reports exist (`docs/audits/AUDIT_SCRIPTING_2026-06-23.md` through `_2026-07-16.md`). It also preloads `#1663, #1664, #1665, #1666, #1667, #1668, #1316` as "known open" scripting-domain issues to dedup against. All seven are CLOSED — the condition evaluator now implements the full 13-function catalog with real match arms, confirmed by `AUDIT_SCRIPTING_2026-07-16.md`.

## Evidence
`for n in 1663 1664 1665 1666 1667 1668 1316; do gh issue view $n --json state -q .state; done` → `CLOSED` ×7. Verified live against the repo: all 7 issues confirmed CLOSED, and `docs/audits/` contains 6 `AUDIT_SCRIPTING_*.md` reports predating this claim.

## Impact
An agent following the SKILL.md literally skips the "read prior report, diff direction" dedup step (believing this is greenfield) and treats any re-discovered condition-stub behavior as a dedup-skip against a closed issue rather than correctly recognizing fixed code or filing a regression.

## Related
Same pattern class as TD4-001/TD4-002/TD4-004/TD4-005 (this same report).

## Suggested Fix
Delete the "no prior audit" sentence and the hardcoded issue-preload list; replace with the standard `_audit-common.md` "read the most recent `docs/audits/AUDIT_SCRIPTING_*.md`, diff direction" instruction.

**Age**: SKILL.md dates to ~2026-06-23; all 7 issues closed 2026-06-29→07-04, text never updated.
**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `audit-*` SKILL.md files — TD4-001/002/004/005 cover audit-audio/audit-save/audit-speedtree; worth a sweep of the remaining `audit-*` skills for the same staleness class)
- [ ] **TESTS**: A regression test/lint pins this specific fix (e.g. a CI check that no `SKILL.md` "latest report" pointer is older than the newest matching `docs/audits/AUDIT_*.md` file)
