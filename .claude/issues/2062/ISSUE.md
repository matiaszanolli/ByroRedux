# TD2-102: DalcCubeYup::from_skyrim_zup hand-derives the same axis-permutation knowledge as zup_to_yup_pos

**GitHub Issue**: #2062
**Labels**: low,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `byroredux/src/components.rs:656-682` (`DalcCubeYup::from_skyrim_zup`)

## Description
Same `(x,y,z)→(x,z,-y)` permutation as `zup_to_yup_pos`, applied to named cube faces instead of array indices — not found by grepping `zup_to_yup_pos` callers. Borderline finding: single call site, well-commented.

## Evidence
Confirmed live: `DalcCubeYup::from_skyrim_zup` in `byroredux/src/components.rs` hand-maps `cube.pos_z`→`pos_y`, `cube.neg_z`→`neg_y`, `cube.pos_y`→`neg_z`, `cube.neg_y`→`pos_z` (the same Z-up "up"/"down"/"north"/"south" permutation `zup_to_yup_pos` implements for positions) without calling or cross-referencing `crates/core/src/math/coord.rs::zup_to_yup_pos`.

## Suggested Fix
At minimum, add a cross-reference comment; full refactor optional.

**Effort**: trivial (comment) to small (refactor)

## Completeness Checks
- [ ] **SIBLING**: Companion finding to TD2-101 — same underlying permutation knowledge duplicated a third way (named cube faces vs. array-index swizzle)
- [ ] **TESTS**: N/A if only a cross-reference comment is added; if refactored, existing weather-cube tests should catch any regression
