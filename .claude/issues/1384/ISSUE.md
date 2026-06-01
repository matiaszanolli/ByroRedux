## IOR-04: Three distinct bitfields (INSTANCE_FLAG_FLAT_SHADING, MAT_FLAG_MODEL_SPACE_NORMALS, DBG_VIZ_GLASS_PASSTHRU) all assigned value 128u

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ior
**File:** `crates/renderer/shaders/include/shader_constants.glsl:53`

## Recommended Fix

Add section-separator comments in build.rs output clearly grouping defines by their target register.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*