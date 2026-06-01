## R1-MAT-01: Stale doc comment: MAX_MATERIALS claims 304 B/entry, actual GpuMaterial struct is 300 B

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** material
**File:** `crates/renderer/src/vulkan/scene_buffer/constants.rs:154`

## Recommended Fix

Update the doc comment to read 16384 x 300 B = 4.8 MB.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*