# REN-3-2026-09-20-04: the 'dark' multiplicative texture role is a bare sRGB multiply — no neutral, no test, never censused on FO4+

- **ID**: REN-3-2026-09-20-04
- **Labels**: low,renderer,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: GPU-Struct Layout
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-3-2026-09-20-04)

**Location**: `crates/renderer/shaders/include/material_sampling.glsl` (dark role) + producer arm in `byroredux/src/material_translate.rs`

**Description**
The 2026-09-16 report's neutral-value suggestion was implemented for detail (#4422) and tint (#4423); dark remains a bare multiply with no declared neutral. Its producer arm is live but census-zero on four games and never censused on FO4/FO76/Starfield — an FO4+ dark map would darken with no way to detect it.

**Evidence**
Audit D3/D6, 2026-09-20; dark-role census run on Oblivion/FO3/FNV/Skyrim corpora.

**Impact**
Latent wrong-shading class for FO4+ content shipping dark maps.

**Suggested Fix**
Census FO4+ for dark-map authoring; if non-zero, declare a neutral and pin it like detail/tint; else document the empty population at the role's definition.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
