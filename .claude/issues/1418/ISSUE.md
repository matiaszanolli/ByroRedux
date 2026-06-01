## VKC-004: TLAS UPDATE primitiveCount shrink-mismatch guard is debug-only in release

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:568`

## Recommended Fix

Force full BUILD when instance_count < tlas.built_primitive_count on the UPDATE path: if use_update && instance_count != tlas.built_primitive_count { use_update = false; }

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
