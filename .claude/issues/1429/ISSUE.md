## VKC-006: No CI job runs a frame under Vulkan validation layers — errors can go undetected until manual testing

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/instance.rs:41`

## Recommended Fix

Add a CI step running the headless bench in debug build with VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation. The debug messenger already routes callbacks through log::error!.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*