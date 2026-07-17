# TD2-104: EMPTY_ABSORBED precombine-absorption fallback duplicated verbatim between interior and exterior cell loaders

**GitHub Issue**: #2063
**Labels**: low,import-pipeline,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `byroredux/src/cell_loader/load.rs:380-386` vs. `byroredux/src/cell_loader/exterior.rs:415-421`

## Description
Both independently declare an identical `static EMPTY_ABSORBED: OnceLock<HashSet<u32>>` plus conditional get_or_init.

## Evidence
Confirmed live: both `byroredux/src/cell_loader/load.rs:380` and `byroredux/src/cell_loader/exterior.rs:415` declare `static EMPTY_ABSORBED: std::sync::OnceLock<std::collections::HashSet<u32>> = ...` with matching `get_or_init(std::collections::HashSet::new)` bodies.

## Suggested Fix
Move to one `absorbed_refs_or_empty()` fn in `precombined.rs`.

**Effort**: trivial

## Completeness Checks
- [ ] **TESTS**: Purely mechanical consolidation — existing interior/exterior precombine tests cover this path already
