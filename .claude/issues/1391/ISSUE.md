## EGUI-05: EguiPassConfig is dead code — defined with pub fields but never constructed; forces unused ash dependency

**Severity:** INFO | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** debugui
**File:** `crates/debug-ui/src/lib.rs:208`

## Recommended Fix

Remove EguiPassConfig struct from crates/debug-ui/src/lib.rs. Check if ash and gpu-allocator deps can then be removed from Cargo.toml.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*
