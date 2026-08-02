# REN-D1-01: SHADOW_MASK_OPAQUE is used as a single-mask ray-query test that silently excludes glass

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2227

**Dimension**: 1 (AS masks)
**Location**: `crates/renderer/shaders/caustic_splat.comp:431`, `crates/renderer/shaders/volumetrics_inject.comp:498` (both use `SHADOW_MASK_OPAQUE` directly as the ray-query mask); `crates/renderer/src/vulkan/acceleration/predicates.rs:594` (`shadow_mask_for_instance` — the mask split origin)
**Status**: NEW

**Description**: `SHADOW_MASK_OPAQUE` and `SHADOW_MASK_GLASS` are disjoint bits assigned per-instance by `shadow_mask_for_instance`. Any ray-query consumer that tests only `SHADOW_MASK_OPAQUE` (as the caustic-splat and volumetrics-injection shadow rays currently do) will pass straight through glass instances without registering them as occluders — not a live bug in either of today's two consumers (both want glass-transparent shadow rays), but an undocumented footgun for any future single-mask consumer that expects "opaque" to mean "everything that isn't literally invisible."

**Impact**: Latent — no live incorrect rendering today, but a future ray-query consumer that assumes `SHADOW_MASK_OPAQUE` covers all non-glass geometry will silently misbehave on any scene containing glass.

**Suggested Fix**: add a doc comment at the `SHADOW_MASK_*` constant definitions (and at `shadow_mask_for_instance`) stating explicitly that `OPAQUE` and `GLASS` are disjoint and a full-scene occlusion test must OR both bits.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
