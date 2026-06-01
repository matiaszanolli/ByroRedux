## SAFE-U2: slice::from_raw_parts on WaterPush has no SAFETY comment

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/renderer/src/vulkan/water.rs:466`

## Recommended Fix

Add SAFETY comment: "WaterPush is #[repr(C)] + Copy with only [f32;4] fields (no padding, no invalid byte patterns). push is a valid shared reference; byte slice bounded by size_of::<WaterPush>()." immediately before from_raw_parts.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
