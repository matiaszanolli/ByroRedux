## ANIM-02: B-spline dispatcher comment says Skyrim/FO4 only but NiBSplineCompTransformInterpolator is reachable on FO3/FNV too

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** anim
**File:** `crates/nif/src/blocks/mod.rs:832`

## Recommended Fix

Update comment at mod.rs:832 and anim/transform.rs:47 to include FO3/FNV. Do not gate B-spline dispatch on game era.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*