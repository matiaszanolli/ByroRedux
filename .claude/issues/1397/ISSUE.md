## NCPS-03: TAA/bloom/volumetrics dispatch() emit redundant HOST to COMPUTE UBO barriers instead of batching with pre-render-pass phase

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** compute
**File:** `crates/renderer/src/vulkan/taa.rs:671`

## Recommended Fix

Lift TAA and bloom UBO uploads into the pre-render-pass upload phase (as SVGF already does), then drop the per-dispatch HOST->COMPUTE barriers.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*