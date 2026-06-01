## VKC-002: AS scratch alignment enforcement is debug_assert only — release builds silently violate VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03715

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/acceleration/mod.rs:243`

## Recommended Fix

Upgrade debug_assert_scratch_aligned to a runtime check, or enforce alignment explicitly: let aligned = (raw_addr + (scratch_align as u64 - 1)) & !(scratch_align as u64 - 1);

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*