# SF-D6-04: opaque-tail capture disables drift telemetry that would have surfaced SF-D6-01 for free

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2625
**Finding ID**: SF-D6-04

**Severity**: MEDIUM
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/blocks/shader.rs:760-778` (`read_starfield_tail`), `crates/nif/src/lib.rs:460-508` (drift accounting)
**Status**: NEW

## Description
`read_starfield_tail` consumes `block_size − consumed` *before* `parse_nif`
compares consumed against `block_size`, so any Starfield shader-block
under-read is converted into tail bytes and never reaches
`drift_histogram`. Measured: shader-block drift is `{}` (empty) on all four
archives while tail lengths were simultaneously bimodal `{38: 1868, 42:
11}` — exactly the signal a drift histogram exists to raise, invisible to
`nif_stats --drift-histogram`.

## Evidence
`crates/nif/src/blocks/shader.rs:760-778` captures the tail before the
drift comparison in `crates/nif/src/lib.rs:460-508` ever runs.

## Impact
Blind spot on precisely the block types with the most Starfield churn;
one-directional (over-reads still surface via `saturating_sub` → empty
tail), but under-read is the failure mode these parsers actually exhibit —
this exact mechanism is why SF-D6-01 went undetected by existing drift
telemetry.

## Suggested Fix
Have `read_starfield_tail` also record captured length into a per-type
`opaque_tail_histogram` sibling of `drift_histogram`, surfaced on
`NifScene`.

## Related
SF-D6-01, SF-D6-02.

## Completeness Checks
- [ ] **TESTS**: A synthetic under-read fixture asserts `opaque_tail_histogram` flags the anomaly
