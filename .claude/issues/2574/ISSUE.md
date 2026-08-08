# OBL-D6-01: Checked-in per_block_baselines/oblivion.tsv is stale -- the opt-in regression gate currently FAILS on a benign reclassification

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2574
**Finding ID**: OBL-D6-01

**Severity**: LOW
**Dimension**: Real-Data Validation
**Location**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv` (last regenerated 2026-06-15)
**Status**: NEW

## Description
Live-running the opt-in gate (`per_block_baseline_oblivion -- --ignored`) fails today: `bhkCollisionObject` reads 8,784 in the baseline vs. 8,730 live (−54), with a new `bhkPCollisionObject = 54` row appearing only live. The arithmetic is exact (8,730 + 54 = 8,784) — a pure reclassification into an already-existing, already-correctly-dispatched type (predates the baseline by ~7 weeks, #557), not data loss. Six other types show small increases fully explained by Dimension 1's truncation-recovery finding.

## Evidence
Confirmed directly: `oblivion.tsv:65` — `bhkCollisionObject 8784 0`, with no `bhkPCollisionObject` row present.

## Impact
The gate is opt-in (not CI-wired), so nothing is silently broken, but anyone who actually runs it hits a false "parser regression?" panic.

## Related
Sibling of OBL-D1-03 (this session) — a second, independent checked-in baseline in the same test suite has also gone stale, for a different reason (reclassification vs. truncation-count drift).

## Suggested Fix
Regenerate both Oblivion baselines (`per_block_baselines/oblivion.tsv` and `block_coverage_baselines/oblivion_truncations.tsv`) together in one commit, once OBL-D1-01 (`marker_map.nif`) is fixed too.

## Completeness Checks
- [ ] **TESTS**: `per_block_baseline_oblivion -- --ignored` passes clean after regeneration
