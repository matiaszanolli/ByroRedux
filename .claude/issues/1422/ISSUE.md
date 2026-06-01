## R1-MAT-04: upload_materials overflow guard is debug_assert only — not a hard assert

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** material
**File:** `crates/renderer/src/vulkan/scene_buffer/upload.rs:516`

## Recommended Fix

Consider promoting debug_assert to hard assert. The .min(MAX_MATERIALS) provides a safety net but a single-point hard assert is more defensive.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*