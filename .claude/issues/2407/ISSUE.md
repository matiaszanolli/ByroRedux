# TD1-004: save_io.rs crossed 2000 LOC via ~1890-LOC inline mod tests, production code healthy at ~970 LOC

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2407
**Finding ID**: TD1-004 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `byroredux/src/save_io.rs:1-970` (production), `:971-2860` (tests)
**Status**: NEW

## Description
`save_io.rs` is 2860 LOC, crossing the 2000-LOC threshold, but production code is a healthy ~970 LOC — no production function exceeds cognitive complexity 25. The file crossed the threshold via test bulk: a 209-LOC completeness-guard test (#2295) and an 81-LOC round-trip test, both inline in a `mod tests` block.

## Related
Same pattern as the `material.rs` finding (#2257) and partially the `crates/scripting/src/scene.rs` finding — inline test bulk inflating a file-size metric that doesn't reflect production complexity.

## Suggested Fix
Extract `mod tests` into sibling `*_tests.rs` files by topic — the repo already has this convention (`cell_loader/{...}_tests.rs`, `scene_buffer/{...}_tests.rs`). Zero behavior risk, mechanical move.

## Completeness Checks
- [ ] **TESTS**: All extracted tests still compile and pass unchanged after the move
