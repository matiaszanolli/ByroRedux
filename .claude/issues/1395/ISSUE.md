## IOR-01: 1.0 / GLASS_IOR has no clamp guard — ior=0 yields Inf ETA breaking refraction

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** ior
**File:** `crates/renderer/shaders/triangle.frag:2147`

## Recommended Fix

Add float GLASS_IOR = max(mat.ior, 1e-3) at line 2147, mirroring the existing dielectricF0FromIor() clamp from #1253. The companion function has the clamp; ETA_AIR_TO_GLASS does not.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
