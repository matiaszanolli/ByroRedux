## RT-02: water.frag caustic shadow-ray: N-bias (0.05) and tMin (0.001) are inconsistent — 50x mismatch

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** rt
**File:** `crates/renderer/shaders/water.frag:536`

## Recommended Fix

Raise tMin to 0.05 on the caustic shadow ray to match the N * 0.05 bias distance.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **SIBLING**: Check other ray-query sites for same self-intersection / bias pattern

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*