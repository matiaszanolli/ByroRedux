# OBL-D1-03: The Oblivion truncation baseline and the ROADMAP parse-rate row are stale by 5 files

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2564
**Finding ID**: OBL-D1-03

**Severity**: LOW
**Dimension**: NIF Version Handling
**Location**: `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv:1-7`, `ROADMAP.md:430`
**Status**: NEW

## Description
The checked-in baseline still lists 6 truncating files (`truncating=6 parsed=8032`); a live run reports 8031/8032 whole, 1 truncating. `ROADMAP.md:430` still says "99.93% (8,026/8,032)"; the true figure is 99.99% (8,031/8,032). The gate still catches regressions (it's a superset), so nothing is broken, but the stated numbers are wrong.

## Evidence
Confirmed directly: `oblivion_truncations.tsv:1` reads "# Oblivion sizeless-truncation baseline truncating=6 parsed=8032"; `ROADMAP.md:430` reads "**99.93%** (8 026 / 8 032)".

## Related
Sibling finding OBL-D6-01 (this session) — a second, independent checked-in baseline has also drifted stale, for a different underlying reason.

## Suggested Fix
Regenerate both baselines together in one commit (after OBL-D1-01 lands) and update `ROADMAP.md` in the same commit.

## Completeness Checks
- [ ] **TESTS**: Regenerated baselines pass the opt-in regression gate cleanly
