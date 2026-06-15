# TD7-001: bsver()==0 / >0 should use the existing unused bsver::PRE_BETHESDA

_Filed as #1630 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Magic Numbers · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD7-001)
**Status**: NEW (adjacent to CLOSED #1336, which covered decimal *threshold* literals, not the `0` sentinel)

## Description
`bsver::PRE_BETHESDA: u32 = 0` (`crates/nif/src/version.rs:289`) was added expressly to name this sentinel but has **zero usages** repo-wide. Three sites that test exactly "is this pre-Bethesda?" compare against the bare `0` instead; `controller/mod.rs:465` even spells out the nif.xml `#BSVER# #EQ# 0` mapping in a comment.

## Evidence
`version.rs:289 pub const PRE_BETHESDA: u32 = 0;` (no callers). Sites:
- `crates/nif/src/blocks/controller/mod.rs:465` — `if stream.bsver() == 0 {`
- `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:408` — `} else if stream.bsver() > 0 && …`
- `crates/nif/src/blocks/particle.rs:1131` — `… && stream.bsver() > 0;`

## Impact
A named constant added to document a sentinel sits dead while three readers re-spell the bare literal — the exact dedup trap #1336 addressed for decimal thresholds, recurring for the `0` sentinel.

## Suggested Fix
Replace the three literals with `bsver::PRE_BETHESDA` (`== bsver::PRE_BETHESDA` / `> bsver::PRE_BETHESDA`), retiring the dead constant into use.

## Related
#1336 (CLOSED — bare BSVER decimal literals).

## Completeness Checks
- [ ] **SIBLING**: All three `bsver()` vs `0` comparisons routed through `PRE_BETHESDA`; no other bare-`0` bsver comparison left behind
- [ ] **TESTS**: Existing per-version parse tests still pass (behavior-preserving substitution)
