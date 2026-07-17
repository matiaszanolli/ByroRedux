# 2006: NIF-D3-01: nif-parser.md's live-arm-count citation is ~5% stale

https://github.com/matiaszanolli/ByroRedux/issues/2006

Labels: low, nif-parser, documentation

**Severity**: LOW · **Dimension**: Block Dispatch Coverage
**Location**: `docs/engine/nif-parser.md:20,279-280`; ground truth `crates/nif/src/blocks/mod.rs:302-1252`
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D3-01)

## Description
Doc cites "~260 arms / ~309 literals (2026-07-05)"; fresh count gives 248 top-level arms / 315 distinct literals. `blocks/mod.rs` hasn't changed since 2026-07-05 — the delta is a counting-methodology difference, not code drift.

## Impact
None on parser behavior; low risk of a future report citing a slightly-wrong number.

## Suggested Fix
Update the citation to the current arm/literal count next time the file is touched for a dispatch-affecting change.

## Completeness Checks
- [ ] TESTS: N/A (documentation-only fix; no code path to regress)
