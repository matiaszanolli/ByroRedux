## EGUI-03: EguiPass::destroy() does not flush pending_free before Renderer drop — descriptor pool accounting left mismatched

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** debugui
**File:** `crates/renderer/src/vulkan/egui_pass.rs:194`

## Recommended Fix

Add if !self.pending_free.is_empty() { let _ = self.renderer.free_textures(&self.pending_free); } at the start of EguiPass::destroy before framebuffer/render-pass destruction.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*