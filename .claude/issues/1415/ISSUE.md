## SAFE-U4: CStr::from_ptr in check_validation_layer_support has no SAFETY comment

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** unsafe
**File:** `crates/renderer/src/vulkan/instance.rs:71`

## Recommended Fix

Add SAFETY comment: "VkLayerProperties::layerName is a null-terminated C string of at most 256 bytes per the Vulkan spec; pointer is valid for the lifetime of the iteration."

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **UNSAFE**: Safety comment explains the invariant
- [ ] **SIBLING**: Same unsafe pattern checked in related files

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
