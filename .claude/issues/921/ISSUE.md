---
issue: 0
title: REN-D12-NEW-04: Bone palette buffers HOST_VISIBLE | STORAGE_BUFFER — should be DEVICE_LOCAL with HOST_VISIBLE staging
labels: renderer, M29, medium, vulkan, memory, performance
---

**Severity**: MEDIUM (PCIe bandwidth waste — 6 MB host-visible read every frame by every skinned vertex)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 12)

## Location

- `crates/renderer/src/vulkan/scene_buffer.rs:475-480` — bone palette buffer creation (`HOST_VISIBLE | STORAGE_BUFFER`)
- `crates/renderer/src/vulkan/scene_buffer.rs:508-513` — terrain_tile_buffer (the canonical staging pattern to mirror)

## Why it's a bug

Bone palette buffers are created with `HOST_VISIBLE | STORAGE_BUFFER` only — not the audit-checklist DEVICE_LOCAL with HOST_VISIBLE staging. With `MAX_TOTAL_BONES = 32768` × ~192 B per matrix, that's ~6 MB of host-visible mapped storage read every frame by every skinned vertex on PCIe.

The terrain_tile_buffer at scene_buffer.rs:508-513 already uses the staging pattern: HOST_VISIBLE staging buffer + cmd_copy_buffer + DEVICE_LOCAL destination. Bone palette should mirror this.

## Fix sketch

Migrate bone palette to the terrain_tile_buffer staging pattern:
1. Per-frame `bone_palette_staging` (HOST_VISIBLE | TRANSFER_SRC) — keep the current per-frame mapped buffer.
2. New `bone_palette_device` (DEVICE_LOCAL | STORAGE_BUFFER | TRANSFER_DST) — bound to the descriptor.
3. Add `cmd_copy_buffer(bone_palette_staging[frame], bone_palette_device, ...)` early in draw_frame, before the skin-compute dispatch.
4. Add a HOST→TRANSFER barrier on staging and TRANSFER→COMPUTE_SHADER barrier on device.

## Completeness Checks

- [ ] **UNSAFE**: Vulkan barrier additions; verify scope.
- [ ] **SIBLING**: Audit other per-frame SSBO uploads (lights, instances) for the same pattern.
- [ ] **TESTS**: Bench skinned-NPC-heavy scene, measure frame-time delta.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
