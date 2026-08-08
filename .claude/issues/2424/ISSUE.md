# TD7-002: NiSkinPartition's SSE band check uses bare 100/130 where bsver::SKYRIM_SE/bsver::FALLOUT4 already exist with matching semantics

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2424
**Finding ID**: TD7-002 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 7 — Magic Numbers & Hardcoded Constants
**Location**: `crates/nif/src/blocks/skin.rs:226`
**Status**: NEW

## Description
`NiSkinPartition`'s SSE band check uses bare `(100..130).contains(&bsver)` where `bsver::SKYRIM_SE`/`bsver::FALLOUT4` already exist with matching semantics — the surrounding comment *already names the constants in prose* (describing the historical #126/NIF-206 bug) without the code using them. Strongest instance of the three TD7 findings this cycle — the fix is already spelled out in-code.

## Related
Same class as TD7-001/TD7-003, #2343 (OPEN), #2281 (CLOSED).

## Suggested Fix
`let is_sse = (bsver::SKYRIM_SE..bsver::FALLOUT4).contains(&bsver);` — one-line, no behavior change.

## Age
~3.5 months.

## Completeness Checks
- [ ] **TESTS**: Existing NIF skin-partition parse tests still pass unchanged (no behavior change)
- [ ] **SIBLING**: See TD7-001/TD7-003 — same drift class
