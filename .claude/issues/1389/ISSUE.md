## NCPS-01: Volumetrics survives failed initialize_layouts leaving froxel images UNDEFINED

**Severity:** HIGH | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** compute
**File:** `crates/renderer/src/vulkan/context/mod.rs:1801`

## Recommended Fix

Mirror the SVGF/caustic take()-and-destroy pattern: on initialize_layouts failure, take and destroy the pipeline so self.volumetrics becomes None. Triggers VUID-vkCmdDispatch-None-04115 when VOLUMETRIC_OUTPUT_CONSUMED gate flips.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
