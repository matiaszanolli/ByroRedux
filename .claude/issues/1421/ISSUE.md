## EGUI-02: egui texture upload acquires graphics_queue Mutex, copies raw vk::Queue, drops guard, then calls vkQueueSubmit without the Mutex held

**Severity:** LOW | **Audit:** [AUDIT_SAFETY_2026-06-01](docs/audits/AUDIT_SAFETY_2026-06-01.md) | **Dimension:** debugui
**File:** `crates/renderer/src/vulkan/context/draw.rs:2962`

## Recommended Fix

Restructure EguiPass::dispatch to accept Arc<Mutex<vk::Queue>> and hold the lock across set_textures + cmd_draw, matching the main queue_submit pattern.

## Completeness Checks

- [ ] **TESTS**: Regression test or reference added for this specific fix
- [ ] **DROP**: If Vulkan objects change, verify Drop impl ordering is correct

---
*Filed by audit-publish from AUDIT_SAFETY_2026-06-01.md — findings verified against live code during audit.*