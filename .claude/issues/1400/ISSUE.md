## NCPS-05: TAA first-frame safety relies solely on GPU-side shader guard — no CPU-side protection

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** compute
**File:** `crates/renderer/src/vulkan/taa.rs:513`

## Recommended Fix

Add unit test asserting TaaParams::params.y == 1.0 for frames_since_creation == 0.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*