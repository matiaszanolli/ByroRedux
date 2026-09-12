# SKY-2026-09-11-D2-03: only shader_type=0 of thirteen no-trailing-data Skyrim shader types has a wire-level byte-position test

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4252

**Severity**: LOW
**Dimension**: 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/blocks/shader.rs` (shader-type dispatch table); test coverage under `crates/nif/src/import/tests/`
**Status**: NEW

**Description**: Test-coverage gap: only `shader_type = 0` of the thirteen no-trailing-data Skyrim shader types has a wire-level byte-position assertion pinning the `None` fallthrough arm. No live defect; a missing tripwire on a dispatch table whose failure mode is exactly the class of silent byte-drift bug that took four prior audits (#455/#474/#550/#713/#717) to catch.

**Evidence**: Confirmed by inspection of the shader-type dispatch table and its test coverage — the fourteen `None`-mapped shader-type values (0,2,3,4,8,9,10,12,13,15,17,18,19,20) only have a wire-level byte-position pin for value 0.

**Impact**: No live defect. Missing regression coverage means a future edit that accidentally adds trailing-field consumption to one of the other twelve `None` arms would not be caught by a targeted byte-position test — only by broader integration coverage, if any.

**Related**: #455, #474, #550, #713, #717 (prior silent byte-drift bugs in this dispatch table).

**Suggested Fix**: Add wire-level byte-position assertions for the remaining twelve `None`-mapped shader-type values, matching the existing shader_type=0 test's structure.

## Completeness Checks
- [ ] **TESTS**: This finding IS the test-gap fix — add the twelve missing byte-position assertions
