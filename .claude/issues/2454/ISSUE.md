# EXAL-08: WRLD OFST cell-offset table captured as raw words with no interpretation and no consumer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2454
**Finding ID**: EXAL-08 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — EXAL
**Location**: `crates/plugin/src/esm/cell/wrld.rs:170-190`; `cell/mod.rs:862-874`
**Status**: NEW (sub-finding under #2371)

## Description
`cell_offsets` (OFST, #1849) is stored raw with the parser's own comment deferring interpretation "to a future LAND streamer" — that streamer now exists and doesn't use OFST (enumerates `index.exterior_cells` keys instead). Zero readers outside the parser's own tests.

## Impact
Low — current approach works; cost is a per-worldspace `Vec<u32>` up to ~44k entries held for no benefit, and a parsed field that reads as a live capability when it isn't.

## Related
#2371 (OPEN).

## Suggested Fix
Drop the capture with a note in exal.md §5 that OFST was superseded, or gate behind a feature flag until a consumer exists.

## Completeness Checks
- [ ] **TESTS**: If dropped, existing OFST parse tests are removed/updated accordingly (`crates/plugin/src/esm/cell/tests/wrld.rs`)
