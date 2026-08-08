# SF-D4-06: sf_smoke unresolved-REFR report conflates by-design exclusions with real gaps, overstates ~5x

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2637
**Finding ID**: SF-D4-06

**Severity**: LOW
**Dimension**: 4 (Starfield ESM Resolve-Rate Baseline)
**Location**: `byroredux/src/sf_smoke.rs:154-176`
**Status**: NEW

## Description
`sf_smoke`'s "unresolved" report conflates by-design exclusions with real
gaps, overstating the headline number ~5×. The tool's hint text ("parser
gap — schema diverged or record type missing") applies uniformly, but this
run's 2,461 unresolved `citycydoniamainlevel` REFRs decompose into ~140
(0.5%) real #1576 gap, 1,846 (6.6%) intentionally-unconsumed PDCL, and ~369
(1.3%) by-design-excluded audio markers.

## Evidence
Breakdown of the 2,461 unresolved REFRs on `citycydoniamainlevel` into the
three buckets above.

## Impact
Purely a diagnostic-tool clarity gap — but it's exactly the failure mode
the tool exists to catch: a real regression (PDCL doubling, or the BFCB gap
widening) would today hide inside the same undifferentiated bucket as two
already-understood causes.

## Suggested Fix
Thread the FourCC through the existing skip telemetry into a per-type
unresolved-REFR counter; separate known-tracked buckets from the residual
"unattributed" count in the report.

## Related
SF-D4-05, #1576, #1568.

## Completeness Checks
- [ ] **TESTS**: A fixture with a mix of the three bucket types asserts the report separates them correctly
