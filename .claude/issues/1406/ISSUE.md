## MEM-03: GPU allocator Arc leak on Drop path leaves VkDevice/VkSurfaceKHR/VkInstance handles dangling

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** memory
**File:** `crates/renderer/src/vulkan/context/mod.rs:2760`

## Recommended Fix

Audit all SharedAllocator Arc clone sites to ensure they drop before VulkanContext::drop. Add device_wait_idle() before the early-return leak path. Pass by reference where possible.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*