# SKY-2026-09-11-D2-01: Skyrim BSEffectShaderProperty import writes env_map_scale = 0.0 where MaterialInfo's own default is 1.0

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4250

**Severity**: LOW
**Dimension**: 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/import/material/dedicated_shader.rs:578`
**Status**: NEW

**Description**: Skyrim's `BSEffectShaderProperty` import path constructs `env_map_scale = 0.0` for the absent-on-Skyrim field (BSVER < 130), then copies it unconditionally onto `MaterialInfo`, whose own declared default is `1.0` — three sites in one pipeline disagree on what "no value" means.

**Evidence**: Confirmed in current code at `dedicated_shader.rs:578`; `MaterialInfo`'s own field declares a `1.0` default while the Skyrim import path writes `0.0` unconditionally for the not-present-on-Skyrim case.

**Impact**: Currently latent — affected meshes are tagged `material_kind = 101`, short-circuiting lit shading before the field is read. No live rendering defect today, but a future consumer of `env_map_scale` reading a Skyrim-imported `MaterialInfo` directly would see 0.0 instead of the neutral 1.0 default.

**Suggested Fix**: Use `MaterialInfo`'s own declared default (`1.0`) for the not-present-on-Skyrim case instead of hardcoding `0.0`, or make the "absent" state `Option<f32>` so the three sites can't silently disagree.

## Completeness Checks
- [ ] **SIBLING**: Check other Skyrim-absent-field defaults in the same import path for the same 0.0-vs-declared-default mismatch
- [ ] **TESTS**: A regression test asserts a Skyrim-imported `BSEffectShaderProperty` with `material_kind` not shorting shading reads `env_map_scale == 1.0`, not `0.0`
