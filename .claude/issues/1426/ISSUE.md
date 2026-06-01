## VKC-005: Allocator Arc leak early-return skips device_wait_idle before exit

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/context/mod.rs:2787`

## Recommended Fix

Call let _ = self.device.device_wait_idle(); before the early return in the Arc::try_unwrap failure path.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
