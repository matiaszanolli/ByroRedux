# TD1-007: shader_tests.rs crossed 2000 LOC — test file, lower priority

**GitHub Issue**: #2056
**Labels**: low,nif-parser,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/nif/src/blocks/shader_tests.rs`

## Description
Already-split test module (per TD1-004's precedent) has organically grown past 2000 LOC via per-game-era regression tests. Loosely grouped by era in file order already; not disorganized, just accumulated volume. Not on any hot edit path.

## Evidence
Confirmed live: `crates/nif/src/blocks/shader_tests.rs` is 2055 LOC total, matching the report's figure.

## Suggested Fix
If/when next touched, split along existing era boundaries (legacy/Skyrim/FO4/FO76/Starfield). Not urgent.

**Effort**: small, deferrable

## Completeness Checks
- [ ] **TESTS**: Purely mechanical split if/when performed — no behavior change
