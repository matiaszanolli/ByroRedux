# SF-D1-04: log_v2_v3_extra_bytes doc claims name-table-size field that is always constant 1, dead heuristic

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2629
**Finding ID**: SF-D1-04

**Severity**: LOW
**Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:431-474` (`log_v2_v3_extra_bytes`)
**Status**: NEW — sibling of #2360 (SF-BA2-02, OPEN, LOW), different defect in the same helper

## Description
`log_v2_v3_extra_bytes` documents a "compressed name-table size" field that
is always the constant `1` on every real archive — the malformed-header
heuristic built on it is dead code. All 129 archives have
`hdr[24..32] == 0100000000000000` byte-identical. A value of 1 is not a
size; the `stream_pos + size > name_table_offset` malformed-header branch
derived from reading it as one can never fire on real data.

## Evidence
129/129 archives byte-identical on this field.

## Impact
Documentation/diagnostic only.

## Suggested Fix
Rename to `unknown_1`/`unknown_2` (or `name_table_format`), recording the
observed constant; drop or replace the dead heuristic.

## Related
#2360 (SF-BA2-02).

## Completeness Checks
- [ ] **TESTS**: N/A — doc/diagnostic-only change
