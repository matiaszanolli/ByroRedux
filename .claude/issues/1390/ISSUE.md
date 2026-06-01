## VKC-003: TLAS resize destroys live resources with no code-level fence-wait enforcement — use-after-destroy risk on future refactor

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** vulkan
**File:** `crates/renderer/src/vulkan/acceleration/tlas.rs:289`

## Recommended Fix

Accept a _fence_waited: &FenceWaitProof zero-size token at the resize entry point (type-checked invariant), or add a defensive device_wait_idle() inside the if need_new_tlas block.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*