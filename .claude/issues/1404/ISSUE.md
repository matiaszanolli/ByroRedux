## NCPS-04: R32_UINT storage image atomic format assumed without device VK_FORMAT_FEATURE_STORAGE_IMAGE_ATOMIC_BIT query

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** compute
**File:** `crates/renderer/src/vulkan/caustic.rs:62`

## Recommended Fix

Add vkGetPhysicalDeviceFormatProperties(R32_UINT) check in device.rs or VulkanContext::new before enabling the caustic pipeline.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*