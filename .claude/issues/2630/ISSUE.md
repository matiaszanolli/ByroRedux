# SF-D1-05: no test covers v3-zlib path, LZ4 under-run, or real-data-derived BA2 header fixture

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2630
**Finding ID**: SF-D1-05

**Severity**: LOW
**Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:1009-1470` (`mod tests`)
**Status**: NEW

## Description
`compression_method == 0` on a v3 archive (v3+zlib) has zero test coverage
and does not occur in vanilla; no under-run test exists (see SF-D1-01); the
header-offset tests build their fixture to mirror the parser's own layout
assumption, so a wrong offset would move in lockstep with the bug rather
than being caught.

## Evidence
No v3+zlib fixture; no LZ4 under-run test; header-offset fixtures are
generated from the parser's own assumed layout rather than an independent
byte-literal spec.

## Impact
A future header-layout edit could pass the whole suite while breaking every
v3 archive, surfacing only on a manual run against game data.

## Suggested Fix
Synthesize a v3+method-0 fixture; add the LZ4 under-run test (see
SF-D1-01); add a byte-literal fixture built from the documented v3 header
layout with post-parse content assertions.

## Related
SF-D1-01.

## Completeness Checks
- [ ] **TESTS**: This finding IS the test-coverage fix — add the three fixtures described above
