# TD4-001: audit-audio SKILL.md's latest-report pointer is 4 reports stale

**GitHub Issue**: #2048
**Labels**: medium,tech-debt,documentation

**Severity**: MEDIUM
**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-audio/SKILL.md:78-80`

## Description
Hardcodes "the latest is `_2026-07-02.md`" — four newer reports now exist (`_07-03.md`, `_07-14.md`, `_07-16.md`). The instruction to "sort by date, do not hardcode" is violated by the very next sentence.

## Evidence
`ls docs/audits/ | grep AUDIT_AUDIO` → 7 files: `AUDIT_AUDIO_2026-05-05.md`, `_06-14.md`, `_06-23.md`, `_07-02.md`, `_07-03.md`, `_07-14.md`, `_07-16.md`. Latest is `_2026-07-16.md`, four newer than the hardcoded pointer. Confirmed directly against the live `.claude/commands/audit-audio/SKILL.md` text.

## Impact
A future `/audit-audio` run trusting the prose reads a 2-week-stale baseline.

## Related
TD4-002 (same block), TD4-005 (identical pattern in audit-speedtree).

## Suggested Fix
Delete the hardcoded filename/supersession list; keep only the "sort by date" instruction.

**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — TD4-002 (same file, adjacent block), TD4-004 (audit-save), TD4-005 (audit-speedtree) all share this class
- [ ] **TESTS**: N/A (documentation-only fix)
