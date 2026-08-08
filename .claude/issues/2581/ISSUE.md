# SKY-D2-03: No wire-level Skyrim parse test for shader types 6/7/14 -- HairTint is the second-most-common non-default type with zero byte-layout coverage

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2581
**Finding ID**: SKY-D2-03

**Severity**: LOW
**Dimension**: BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/blocks/shader_tests/skyrim.rs:6-119`
**Status**: NEW

## Description
Skyrim-era wire-parse tests cover shader types 0, 1, 5, 11, 16 only. Types 6 (HairTint, 10,817 vanilla instances — more than SkinTint/EyeEnvmap/MultiLayerParallax combined), 7 (ParallaxOcc, 0 vanilla but mod-reachable) and 14 (SparkleSnow, 19 instances) have no Skyrim wire-level test; the corresponding `apply_shader_type_data` tests construct the enum directly and never exercise the byte reader.

## Impact
Test-coverage gap only — code is currently correct (verified against nif.xml and the zero-drift corpus run). A future field-count regression in arm 6/7/14 would ship silently.

## Related
SKY-D2-01 (this session — shares the same under-tested HairTint surface).

## Suggested Fix
Add three `build_bs_lighting_common(N)` + trailing-bytes tests mirroring the existing `skin_tint` one, keeping the over-read-detecting `stream.position() == data.len()` assertion.

## Completeness Checks
- [ ] **TESTS**: Three new wire-level tests for shader types 6/7/14, mirroring the existing `skin_tint` test's over-read assertion
