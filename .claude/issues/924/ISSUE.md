---
issue: 0
title: REN-D15-NEW-02: Composite distance fog mix removed (M55 Phase 3); volumetric replacement gated OFF — fog silently disabled
labels: bug, renderer, medium, vulkan
---

**Severity**: MEDIUM (distance fog silently disabled across all cells since 2026-05-09 M55 Phase 3)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 15)

## Location

- `crates/renderer/shaders/composite.frag:393-407` — fog mix removed
- `crates/renderer/shaders/composite.frag:362` — volumetric replacement gated OFF (`vol.rgb * 0.0` keep-alive)

## Why it's a bug

M55 Phase 3 (2026-05-09) removed the composite distance fog mix in favour of a volumetric replacement, but the volumetric path is gated off in composite (`vol.rgb * 0.0`). Net effect: **distance fog is silently disabled across all cells** until volumetric is re-enabled.

User-visible: exterior cells lose atmospheric depth cueing; far-distance LOD pops are more obvious.

## Fix sketch

Pick one of:

1. **Restore fog mix** as the primary path; treat volumetric as a future enhancement that overrides when enabled.
2. **Commit to volumetric** — drop the `* 0.0` keep-alive, wire up the volumetric pipeline (also requires REN-AUDIT-CROSS-01 fixed first), and use volumetric output as the fog source.
3. **Hybrid**: keep the fog mix for the cheap distance-fog case, use volumetric for the godrays / participating-media case.

Per the audit, this needs an explicit decision since the M55 Phase 3 commit silently dropped a feature.

## Related

- REN-AUDIT-CROSS-01 (#905) — volumetric pipeline has no `recreate_on_resize`, blocking option 2.
- REN-D15-NEW-04 — `traceReflection` and glass-refraction miss paths read the now-orphaned `fog` UBO.

## Completeness Checks

- [ ] **SIBLING**: Verify all `fog` UBO consumers in triangle.frag are accounted for in the chosen path.
- [ ] **TESTS**: Visual: load Megaton exterior, verify atmospheric fog on distant LOD chunks.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
