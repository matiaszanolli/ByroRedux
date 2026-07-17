# TD4-004: audit-save SKILL.md claims no prior save audit exists — 4 reports now exist

**GitHub Issue**: #2050
**Labels**: medium,tech-debt,documentation

**Severity**: MEDIUM
**Dimension**: 4 (Audit-Finding Rot)
**Location**: `.claude/commands/audit-save/SKILL.md:107-109`

## Description
States this would be the first save audit; `AUDIT_SAVE_2026-06-23.md`, `_07-02.md`, `_07-03.md`, `_07-16.md` all exist. Narrower blast radius than TD4-003 (no stale issue-preload list here), but still tells the agent to skip reading prior reports.

## Evidence
`ls docs/audits/ | grep AUDIT_SAVE` → 4 files (`_2026-06-23.md`, `_07-02.md`, `_07-03.md`, `_07-16.md`), confirmed live. `.claude/commands/audit-save/SKILL.md` still reads "No prior save audit exists (this is the first — `docs/audits/` has no `AUDIT_SAVE_*`)" at Phase 1 setup step 4, confirmed unchanged.

## Impact
A `/audit-save` run wouldn't diff against 3 existing follow-ups, risking re-filing already-triaged findings.

## Suggested Fix
Replace with the standard "read most recent, diff direction" instruction.

**Age**: first report landed 2026-06-23; text unchanged since.
**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — TD4-001/002 (audit-audio), TD4-003 (audit-scripting), TD4-005 (audit-speedtree) share this staleness class
- [ ] **TESTS**: N/A (documentation-only fix)
