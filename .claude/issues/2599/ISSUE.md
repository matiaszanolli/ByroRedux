# FO4-D4-03: third independent half_to_f32 copy in crates/facegen

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2599
**Finding ID**: FO4-D4-03

**Severity**: LOW
**Dimension**: 4 (NIF Parser)
**Location**: `crates/facegen/src/lib.rs:72`
**Status**: NEW

## Description
A third independent copy of `half_to_f32` exists in `crates/facegen/src/lib.rs:72`,
alongside the canonical implementation at
`crates/nif/src/import/mesh/decode.rs:18` (re-exported at
`crates/nif/src/blocks/tri_shape/mod.rs:93`). The facegen copy is not covered
by the canonical decoder's test pins, so it can silently drift from the
canonical half-float conversion behavior.

## Evidence
`crates/facegen/src/lib.rs:72` defines its own `half_to_f32` rather than
depending on the canonical one in `crates/nif/src/import/mesh/decode.rs:18`.

## Impact
Low — half-float conversion is a well-defined bit operation unlikely to need
behavior changes, but any future fix to the canonical decoder (e.g. a
subnormal or NaN-handling correction) would not propagate to the facegen
copy, and there is no test that would catch the divergence.

## Suggested Fix
Have `crates/facegen` depend on the canonical `half_to_f32` re-export instead
of maintaining its own copy.

## Completeness Checks
- [ ] **TESTS**: If not unified, add a regression test asserting the facegen copy matches the canonical decoder bit-for-bit on the same inputs
