# FNV-D5-01: Checked-in per-block baseline TSV is stale after #2332's bhkSPCollisionObject dispatch split

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2559
**Finding ID**: FNV-D5-01

**Severity**: LOW
**Dimension**: NIF Parser FNV Regression Guard
**Location**: `crates/nif/tests/data/per_block_baselines/fallout_nv.tsv`
**Status**: NEW (stems from #2332, same session)

## Description
The checked-in per-block baseline TSV is stale after the same-session commit `8ee151e0` correctly split the `bhkSPCollisionObject` dispatch arm out of `bhkCollisionObject` (fixing #2332 — a real, verified-correct parser improvement, not a regression).

## Evidence
Confirmed directly: `fallout_nv.tsv:80` still shows `bhkCollisionObject 12981 0` with no separate `bhkPCollisionObject` line. Root-caused and confirmed conserved (39 blocks move to a new `bhkPCollisionObject` line, 0 unknowns) by regenerating and reverting locally.

## Impact
`cargo test --test per_block_baselines -- --ignored` (the R3 regression gate) currently fails loud for FNV until the baseline is regenerated and checked in. No runtime/gameplay impact — parser behavior is correct.

## Related
#2332 (the fix that created this staleness).

## Suggested Fix
Regenerate + check in the FNV (and check FO3, which shares the block family) baseline TSV.

## Completeness Checks
- [ ] **TESTS**: `cargo test --test per_block_baselines -- --ignored` passes clean for both FNV and FO3 after regeneration
