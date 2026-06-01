## SAFE-U5: upload_lights SAFETY comment omits pointer-arithmetic in-bounds invariant

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/renderer/src/vulkan/scene_buffer/upload.rs:41`

## Recommended Fix

Add debug_assert!(header_size + light_size * count <= mapped.len()) before the unsafe block. Amend SAFETY comment to document that .add(header_size) is in bounds because buffer capacity covers header_size + MAX_LIGHTS * light_size.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
