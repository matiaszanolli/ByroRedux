## R1-MAT-02: gpu_material_glsl_field_names_pinned test omits 11 GLSL field needles

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** material
**File:** `crates/renderer/src/vulkan/material.rs:1197`

## Recommended Fix

Add sparkleR, sparkleG, sparkleB, eyeLeftCenterX/Y/Z, eyeRightCenterX/Y/Z, multiLayerInnerScaleU, multiLayerInnerScaleV to the needle list. Confirm punctuation against triangle.frag lines 145-149.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*