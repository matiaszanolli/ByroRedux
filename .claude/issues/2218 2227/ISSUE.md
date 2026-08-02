# Issues 2218 + 2227

## #2218: REN-2026-07-28-BLOCK-01 — FO3 Megaton exterior geometry renders pure white (suspected non-finite shading term) — needs RenderDoc
- Severity: HIGH
- Labels: bug, renderer, high, legacy-compat
- State: OPEN
- Dimension: 18 (sky/weather/exterior)
- Needs-RenderDoc: explicit "do NOT patch speculatively" instruction in the issue body.
- Next step per issue: add isnan/isinf debug visualization around direct/indirect/shadow/GI terms in triangle.frag, bisect which term first goes non-finite, THEN capture in RenderDoc, THEN design a fix.

## #2227: REN-D1-01 — SHADOW_MASK_OPAQUE is used as a single-mask ray-query test that silently excludes glass
- Severity: MEDIUM
- Labels: bug, renderer, medium, vulkan
- State: OPEN
- Dimension: 1 (AS masks)
- Location: crates/renderer/shaders/caustic_splat.comp:431, crates/renderer/shaders/volumetrics_inject.comp:498, crates/renderer/src/vulkan/acceleration/predicates.rs:594
- Not a live bug (both current consumers want glass-transparent shadow rays) — latent footgun for a future single-mask consumer.
- Suggested fix: doc comment at SHADOW_MASK_* constants + shadow_mask_for_instance stating OPAQUE/GLASS are disjoint bits and a full-scene occlusion test must OR both.
