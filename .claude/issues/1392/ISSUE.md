## RT-05: instance_custom_index 24-bit overflow guard is debug_assert only — release silently truncates

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** rt
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:207`

## Recommended Fix

Promote to hard assert! or add compile-time assertion: const _: () = assert!(MAX_INSTANCES < (1 << 24));

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **SIBLING**: Check other ray-query sites for same self-intersection / bias pattern

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*