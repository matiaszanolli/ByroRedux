## SAFE-U3: Inner unsafe block in debug_callback missing SAFETY comment

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/renderer/src/vulkan/debug.rs:50`

## Recommended Fix

Add SAFETY comment: "Vulkan spec (VK_EXT_debug_utils) guarantees callback_data is valid if non-null, and p_message is a valid NUL-terminated C string or NULL." before the inner unsafe block.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
