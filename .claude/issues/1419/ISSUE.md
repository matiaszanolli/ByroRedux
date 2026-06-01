## NCPS-02: Volumetrics tlas_written latch never resets per frame — stale from frame 2 onwards

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** compute
**File:** `crates/renderer/src/vulkan/volumetrics.rs:764`

## Recommended Fix

Reset self.tlas_written[frame] = false at the start of dispatch() after the assert, or at frame start in draw.rs.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*