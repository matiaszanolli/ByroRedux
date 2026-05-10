---
issue: 0
title: REN-D1-NEW-03: Composite UBO host barrier isolated 750 lines from bulk host barrier
labels: renderer, medium, vulkan, sync
---

**Severity**: MEDIUM (fragile; not currently broken)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 1)

## Location

- `crates/renderer/src/vulkan/context/draw.rs:1896-1907` — composite UBO host barrier (isolated)
- `crates/renderer/src/vulkan/context/draw.rs:1146` — bulk host→shader barrier

## Why it's a bug

The composite UBO host visibility uses a separate isolated barrier 750 lines away from the bulk host barrier. This narrows future evolution: any new host-write that lives between these two sites must remember to either fold into the bulk or add another isolated barrier. The canonical pattern is one consolidated host barrier per command buffer.

## Fix sketch

Fold the composite UBO barrier into the existing instance_barrier at `draw.rs:1146`, widening its `dst_access_mask` to cover composite reads.

## Completeness Checks

- [ ] **UNSAFE**: Vulkan barrier; safety unchanged.
- [ ] **SIBLING**: Audit indirect_barrier, cluster_cull host barrier sites for the same pattern.
- [ ] **TESTS**: Validation layer pass; manual visual check that composite still receives correct UBO.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
