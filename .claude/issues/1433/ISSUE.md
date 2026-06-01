## EGUI-04: egui render pass has no outgoing EXTERNAL subpass dependency — relies on implicit Vulkan external dependency

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** debugui
**File:** `crates/renderer/src/vulkan/egui_pass.rs:244`

## Recommended Fix

Add outgoing SubpassDependency: src_subpass=0, dst_subpass=SUBPASS_EXTERNAL, src_stage=COLOR_ATTACHMENT_OUTPUT, dst_stage=BOTTOM_OF_PIPE, src_access=COLOR_ATTACHMENT_WRITE, dst_access=0.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*