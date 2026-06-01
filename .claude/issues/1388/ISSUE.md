## RT-01: water.frag caustic floor-ray: no N-bias origin offset and mismatched tMin=0.001 (project convention is 0.05)

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** rt
**File:** `crates/renderer/shaders/water.frag:554`

## Recommended Fix

Apply N * 0.05 bias to the floor-ray origin and raise tMin to 0.05, matching triangle.frag, caustic_splat.comp, and foamShoreline. The sibling shadow-ray at line 539 already has the N * 0.05 bias.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **SIBLING**: Check other ray-query sites for same self-intersection / bias pattern

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
