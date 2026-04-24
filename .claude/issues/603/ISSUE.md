# #603: FO4-DIM6-09: CTDA variant — FO4 32-byte stride not verified across parsers

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/603
**Labels**: bug, low, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: LOW
**Location**: CTDA consumers in `crates/plugin/src/esm/records/actor.rs`, `misc.rs` (PACK, QUST, PERK)

## Description

FO4 CTDA (condition data) is 32 bytes (vs 28 in Skyrim, 24 in FO3/FNV) — adds a `reference` FormID at offset 24 + 4 bytes padding. Existing parsers were designed around Skyrim 28-byte CTDAs; need to verify they walk the 32-byte stride on FO4.

## Evidence

`rg 'b"CTDA"' crates/plugin/src/esm/` surfaces parsers in `actor.rs`, `misc.rs`, etc. FO4 stride not explicitly gated in any parser I inspected during audit.

## Impact

If stride is wrong, FO4 quest conditions, package conditions, and perk conditions may read garbage for the `reference` field, causing quest-stage triggers to evaluate incorrectly or silently. No runtime consumer yet (no quest/perk system), but any future condition evaluator will be corrupt.

## Suggested Fix

1. Verify CTDA parse stride against FO4 records (grep for `32` vs `28` byte reads in condition paths).
2. Gate on `GameKind` or on record version to select stride.
3. Add a corpus regression test parsing a known FO4 QUST with multiple CTDAs.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same stride check for Skyrim SE conditions (28-byte) and Starfield (if CTDA shape changed again)
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Corpus regression on FO4 QUST + PACK + PERK condition parse.

## Related

- Deferred from Dim 6 to a dedicated condition-system audit.
