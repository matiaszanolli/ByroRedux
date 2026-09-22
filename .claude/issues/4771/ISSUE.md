# #4771: RT-1: baselines README still cites the pre-split audit-runtime.md skill path

**Severity**: LOW
**Dimension**: audit infrastructure / doc rot
**Location**: `.claude/audit-baselines/runtime/README.md:91`
**Labels**: low, documentation, doc-rot
**Source**: docs/audits/AUDIT_RUNTIME_2026-09-22.md (RT-1)

## Description

The README's closing line reads `See .claude/commands/audit-runtime.md
§Phase 3 for the canonical metric list and direction rules.` The skill lives
at `.claude/commands/audit-runtime/SKILL.md` (a directory + `SKILL.md`, not a
flat `.md` file) — the path in the README does not resolve.

## Evidence

`.claude/audit-baselines/runtime/README.md:91`:
```
See `.claude/commands/audit-runtime.md` §Phase 3 for the canonical metric
list and direction rules.
```
`ls .claude/commands/audit-runtime/` shows `SKILL.md` + `capture.sh`; there is
no `.claude/commands/audit-runtime.md` on disk. Re-verified live at HEAD
`c3f298a24` (2026-09-22). No sibling doc carries the same stale reference —
`.claude/commands/audit-runtime/SKILL.md:106` already names this exact drift
as a known, not-yet-fixed item; `.claude/issues/1283/ISSUE.md:26` is a
historical issue snapshot, not live documentation.

## Impact

Cosmetic only — the README's own inline Schema section already restates the
metric list and direction rules, so no workflow is actually blocked. A reader
following the link gets a 404-equivalent.

## Suggested Fix

One-line edit: `.claude/commands/audit-runtime.md` → `.claude/commands/audit-runtime/SKILL.md`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
