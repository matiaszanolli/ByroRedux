# NIF-D3-01: nif-parser.md's live-arm-count citation is ~5% stale

**Labels**: documentation, nif-parser, low

**Severity**: LOW
**Dimension**: Block Dispatch Coverage
**Location**: `docs/engine/nif-parser.md:20,279-280`; ground truth `crates/nif/src/blocks/mod.rs:302-1252`

## Description
Doc cites "~260 arms / ~309 literals (2026-07-05)"; fresh count gives 248 top-level arms / 315 distinct literals. `blocks/mod.rs` hasn't changed since 2026-07-05 — the delta is a counting-methodology difference (the doc's naive count double-counts 9 arrows belonging to two nested `match` blocks used internally to re-derive `&'static str`s), not code drift.

Confirmed: a fresh `awk`+`grep -c '=>'` count over `blocks/mod.rs:302-1252` gives 258 (close to but not exactly either cited figure, consistent with the report's point that naive `=>` counting is methodology-sensitive) — the doc's cited figure is stale regardless of exact recount value.

## Impact
None on parser behavior; low risk of a future report citing a slightly-wrong number.

## Suggested Fix
Update the citation to the current arm/literal count next time the file is touched for a dispatch-affecting change.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix; no code path to regress)

