## VKC-007: sync1 barriers use ACCELERATION_STRUCTURE_READ_KHR instead of BUILD_INPUT_READ_ONLY_KHR for COMPUTE-to-AS transitions

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/context/draw.rs:795`

## Recommended Fix

When migrating to cmd_pipeline_barrier2 (sync2), replace ACCELERATION_STRUCTURE_READ_KHR with BUILD_INPUT_READ_ONLY_KHR on COMPUTE->AS_BUILD barriers.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*