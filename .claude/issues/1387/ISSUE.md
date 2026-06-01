## RT-04: Skin output buffer missing VERTEX_BUFFER usage flag (M29.3 deferred) with no tracking comment at creation site

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** rt
**File:** `crates/renderer/src/vulkan/skin_compute.rs:405`

## Recommended Fix

Add TODO(M29.3) comment with issue reference at the buffer creation site to prevent the deferred flag from being forgotten.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **SIBLING**: Check other ray-query sites for same self-intersection / bias pattern

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*