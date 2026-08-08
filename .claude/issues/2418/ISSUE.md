# TD3-211: ROADMAP.md's Known Issues section still shows REND-#1449/#1450 as open — both closed over two months ago

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2418
**Finding ID**: TD3-211 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 3 — Stale Documentation & Comments
**Location**: `ROADMAP.md:938-939`
**Status**: NEW

## Description
ROADMAP.md's Known Issues section still shows REND-#1449/#1450 as open (`[ ]`) checkboxes. Both issues closed same-day-filed, 2026-06-04 (confirmed `CLOSED` via `gh issue view`), neither reflected in ROADMAP's checkbox/prose.

## Suggested Fix
Flip both checkboxes to `[x]`, prepend "**Closed 2026-06-04** —" per the file's own convention.

## Age
2 months.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
