# TD4-005: audit-speedtree SKILL.md — stale latest-report pointer AND stale still-unfiled claim

**GitHub Issue**: #2051
**Labels**: medium,tech-debt,documentation

**Severity**: MEDIUM
**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-speedtree/SKILL.md:97-103`

## Description
Cites `_2026-07-02.md` as latest (actual latest is `_07-16.md`) and calls three findings (SPT-NEW-01/06/07) "still-unfiled" — all three were filed (`#1820`/`#1821`/`#1822`) and two are now closed (`#1820` fixed 2026-07-04, `#1821` fixed 2026-07-04/07-16). Only `#1822` remains open.

## Evidence
`for n in 1820 1821 1822; do gh issue view $n --json state -q .state; done` → `CLOSED CLOSED OPEN` (confirmed live). `ls docs/audits/ | grep AUDIT_SPEEDTREE` → 7 files, latest `_2026-07-16.md`, confirmed 5 reports newer than the hardcoded `_07-02.md` pointer (`_07-01.md` predates it but `_07-03.md` and `_07-16.md` postdate it).

## Impact
An agent told these are "still-unfiled" would waste effort re-deriving/re-filing two already-closed findings, or file duplicates.

## Related
TD4-001 (identical staleness pattern — likely shared-template origin).

## Suggested Fix
Replace with the standard "read most recent, diff direction" instruction; drop the hardcoded finding-status list.

**Age**: source pair from 07-01/02; issues closed 07-04, re-confirmed 07-16.
**Effort**: small

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — TD4-001/002 (audit-audio), TD4-003 (audit-scripting), TD4-004 (audit-save) share this staleness class
- [ ] **TESTS**: N/A (documentation-only fix)
