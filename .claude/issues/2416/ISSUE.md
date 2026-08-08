# TD3-209: feature-matrix.md's three 'Havok .hkx loader ✗' rows are stale — crates/hkx shipped 6 days ago and is wired into a real animation catalog

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2416
**Finding ID**: TD3-209 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 3 — Stale Documentation & Comments
**Location**: `docs/feature-matrix.md:83,117,197` (and `:183`)
**Status**: NEW

## Description
`crates/hkx` (`02c24e4f`, 2026-08-01) is a real, tested reader wired into the animation asset provider to install the MQ101 cart-idle catalog from real game data — deliberately scoped as a vertical slice. `feature-matrix.md`'s three "Havok `.hkx` loader ✗" rows are stale as a result; "Partial" (the convention this file already uses elsewhere) fits better than a blanket ✗.

## Related
5th consecutive cycle of the "feature docs lag feature code" pattern (see Recurring Pattern note in the source audit report).

## Suggested Fix
Change the rows to `~ Partial` with a one-line scope note describing the MQ101 vertical-slice limitation.

## Age
6 days.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
