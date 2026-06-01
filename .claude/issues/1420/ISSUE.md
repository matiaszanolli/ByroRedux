## EGUI-01: egui set_textures uses main draw command pool instead of transfer pool

**Severity:** MEDIUM | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** debugui
**File:** `crates/renderer/src/vulkan/context/draw.rs:2970`

## Recommended Fix

Pass self.transfer_pool instead of self.command_pool to EguiPass::dispatch. All other one-shot uploads already use transfer_pool; egui is the sole exception.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*