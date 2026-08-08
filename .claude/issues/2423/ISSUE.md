# TD7-001: NiGeomMorpherController/NiMorphData legacy-field gates use bare bsver literals 9/10 instead of a named constant

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2423
**Finding ID**: TD7-001 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 7 — Magic Numbers & Hardcoded Constants
**Location**: `crates/nif/src/blocks/controller/morph.rs:103` (`bsver > 9`), `:207` (`bsver < 10`)
**Status**: NEW

## Description
`NiGeomMorpherController`/`NiMorphData` legacy-field gates use bare `bsver` literals `9`/`10` instead of a named constant. `version.rs`'s `bsver` module explicitly documents the project convention of using named constants over bare decimal literals. The numerically-adjacent `bsver::RIGID_BODY_EXTRA_FLOATS = 9` is semantically unrelated (its own doc comment warns against misattribution) — needs new constants, not a repoint.

## Related
Same drift class as #2343 (OPEN), #2281 (CLOSED), #1336/#1319/#1630/#1042 — none of which cover `morph.rs`.

## Suggested Fix
Add `bsver::MORPHER_TRAILING_INTS: u32 = 9` / `bsver::MORPH_DATA_LEGACY_WEIGHT: u32 = 10`, point both call sites at them.

## Age
`:103` 2 weeks, `:207` ~3 months.

## Completeness Checks
- [ ] **TESTS**: Existing NIF morph-controller parse tests still pass with named constants substituted
- [ ] **SIBLING**: See TD7-002/TD7-003 — same drift class, file together if convenient
