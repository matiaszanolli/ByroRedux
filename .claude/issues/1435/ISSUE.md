## RT-03: water.frag reflection/refraction rays: no origin bias on water surface (only tMin=0.05 guard)

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** rt
**File:** `crates/renderer/shaders/water.frag:436`

## Recommended Fix

Pass vWorldPos + N * 0.05 as origin to both traceWaterRay call sites.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **SIBLING**: Check other ray-query sites for same self-intersection / bias pattern

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*