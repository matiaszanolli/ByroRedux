## IOR-02: MATERIAL_KIND_GLASS/EFFECT_SHADER/NO_LIGHTING are local shader consts not covered by the #1190 lockstep framework

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ior
**File:** `crates/renderer/shaders/triangle.frag:1948`

## Recommended Fix

Add MATERIAL_KIND_GLASS=100, MATERIAL_KIND_EFFECT_SHADER=101, MATERIAL_KIND_NO_LIGHTING=102 to shader_constants_data.rs and emit via build.rs into shader_constants.glsl. Add a numeric-equality test.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
