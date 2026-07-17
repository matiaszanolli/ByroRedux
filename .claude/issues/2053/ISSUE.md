# TD1-004: particle.rs crossed 2000 LOC — 867 lines of embedded tests, unlike its shader.rs sibling

**GitHub Issue**: #2053
**Labels**: low,nif-parser,tech-debt,bug

**Severity**: LOW
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/nif/src/blocks/particle.rs` (production ~1400 LOC, tests 867 LOC)

## Description
Production code is well-organized; the file only trips threshold on test volume. The sibling `shader.rs`/`shader_tests.rs` split already establishes the pattern; `particle.rs` hasn't received it.

## Evidence
Confirmed live: `crates/nif/src/blocks/particle.rs` is 2273 LOC total, matching the report's figure.

## Suggested Fix
Extract `mod tests` verbatim into `particle_tests.rs`, mechanical, no logic change.

**Age**: file created 2026-04-05, last touched 2026-07-06.
**Effort**: trivial

## Completeness Checks
- [ ] **SIBLING**: Mirrors the already-proven `shader.rs`/`shader_tests.rs` split — follow the exact same mechanical pattern
- [ ] **TESTS**: Purely mechanical split — no behavior change, so the existing test suite passing unchanged is the regression check
