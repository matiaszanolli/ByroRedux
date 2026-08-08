# TD4-001: Crate-count roster regressed to stale again — two skill files still say 22/23 crates, one day after mod-runtime bumped the live count to 24

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2420
**Finding ID**: TD4-001 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 4 — Audit-Finding Rot
**Location**: `.claude/commands/audit-tech-debt/SKILL.md:21`, `.claude/commands/audit-scripting/SKILL.md:35`
**Status**: NEW (regression of the pattern behind closed #2261)

## Description
`_audit-common.md` itself is correct (24, includes `mod-runtime`, added `9f619355` 2026-08-06); two *other* skill files still quote 23/22 crates.

## Related
Regression of #2261 (same underlying pattern, second crate addition in a row to cause it).

## Suggested Fix
Bump both to 24; consider dropping the literal number from pointer sentences entirely so future crate additions don't cause the same drift.

## Age
1 day.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
