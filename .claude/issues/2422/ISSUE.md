# TD4-003: Two audit report filenames use a hyphen instead of the skill-mandated underscore, making them invisible to the glob

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2422
**Finding ID**: TD4-003 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 4 — Audit-Finding Rot
**Location**: `docs/audits/AUDIT_TECH-DEBT_2026-07-16.md`, `AUDIT_TECH-DEBT_2026-08-03.md`, `AUDIT_LEGACY-COMPAT_2026-07-02.md`, `AUDIT_LEGACY-COMPAT_2026-07-16.md`
**Status**: NEW

## Description
Both skills' own naming conventions specify underscores; four saved reports use hyphens instead, silently skipped by the exact glob the Phase-1 setup step runs.

## Impact
Each report's own "Prior report:" prose pointer correctly names its true predecessor regardless of filename, so the narrative chain hasn't broken — the exposure is to programmatic discovery only.

## Suggested Fix
`git mv` the four files to underscore-correct names. Note: `AUDIT_TECH-DEBT_2026-08-07.md` (this finding's own source report) also currently uses the hyphenated convention by explicit user/coordinator instruction — include it in the same rename pass if that instruction is lifted.

## Age
Oldest instance 36 days, newest 4 days — a recurring slip, not a one-off.

## Completeness Checks
- [ ] **TESTS**: N/A (file rename only); confirm the Phase-1 setup glob picks up all renamed files afterward
