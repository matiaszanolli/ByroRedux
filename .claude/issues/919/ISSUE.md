---
issue: 0
title: REN-D10-NEW-05: Composite cloud sample uses driver-derivative LOD for layers 1/2/3 (layer 0 uses textureLod)
labels: renderer, medium, vulkan
---

**Severity**: MEDIUM (cloud-layer aliasing on rotation)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 10)

## Location

- `crates/renderer/shaders/composite.frag` — bindless cloud sample sites for layers 1/2/3

## Why it's a bug

Composite's bindless cloud sample uses `texture()` (driver-derivative LOD) for layers 1/2/3 even though layer 0 explicitly uses `textureLod`. Mismatched LOD selection produces visible cloud-layer aliasing on rotation: layer 0 stays sharp, layers 1/2/3 jitter as the implicit derivative changes per pixel.

## Fix sketch

Change the sample sites for layers 1/2/3 from `texture(...)` to `textureLod(..., 0.0)` to match layer 0's behaviour, OR pick a non-zero LOD that matches the artistic intent for distance-falloff clouds.

## Completeness Checks

- [ ] **SIBLING**: Verify all 4 cloud-layer samples now use the same LOD selection function.
- [ ] **TESTS**: Visual rotation test: pan camera, verify all layers stay sharp / consistent.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
