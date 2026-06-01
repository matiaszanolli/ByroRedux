## R1-MAT-03: Release-build FxHash collision silently aliases distinct materials (negligible probability but undetected)

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** material
**File:** `crates/renderer/src/vulkan/material.rs:1045`

## Recommended Fix

No immediate action required. Debug path asserts byte equality. If CI surfaces aliased materials, consider a collision-resistant hash or runtime collision counter.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*