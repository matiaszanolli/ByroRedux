# FO4-D4-04: bs_sub_index parsed and cloned per mesh with zero readers

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2600
**Finding ID**: FO4-D4-04

**Severity**: LOW
**Dimension**: 4 (NIF Parser)
**Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:208-211`
**Status**: NEW

## Description
`ImportedMesh::bs_sub_index` is parsed and deep-cloned per mesh but has zero
readers anywhere in the codebase. This is deliberate per existing docs
(reserved for a future consumer), but as-is it wastes allocations on every
mesh import.

## Evidence
`crates/nif/src/import/mesh/bs_tri_shape.rs:208-211` parses and clones
`bs_sub_index` into `ImportedMesh`; no call site anywhere reads the field
back out.

## Impact
Minor per-mesh allocation waste, proportional to mesh count on import. Not a
correctness issue.

## Suggested Fix
Either gate the parse+clone behind the eventual consumer landing, or (if the
consumer is not imminent) skip the clone and only retain a cheap presence
flag until it's needed.

## Completeness Checks
- [ ] **TESTS**: N/A — perf/allocation cleanup, no behavior change if done correctly
