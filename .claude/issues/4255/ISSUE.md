# SKY-D7-2026-09-11-01: classify_glass_into_material silently overwrites authored Skyrim BSLightingShaderProperty shader types with glass

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4255

**Severity**: HIGH
**Dimension**: 7 — NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `byroredux/src/helpers.rs:99-135`, called from `byroredux/src/material_translate.rs:649`
**Status**: NEW

**Description**: `Material.material_kind` is an overloaded union: low range `0..=20` is the verbatim authored Skyrim `BSLightingShaderProperty.shader_type`; `>= 100` is engine-synthesized. `classify_glass_into_material` protects only the synthesized range (`material_kind >= 100`) — every authored low-range shader type falls straight through to a bare keyword+coverage+metalness heuristic promotion to `MATERIAL_KIND_GLASS`, with no way back. The function's own doc comment (`helpers.rs:59-61`) asserts a keyword alone cannot override an authored shader type — true only for the effect-shader carrier, not the lit carrier this bug affects.

**Evidence**: Confirmed in current code — `helpers.rs:108-113`: `if material.material_kind >= 100 && material.material_kind != MATERIAL_KIND_GLASS && !effect_glass_carrier { return; }` only early-returns for the synthesized range; a low-range authored value (0-20) falls through to the `is_mirror_pane` check (which unconditionally zeroes `material_kind`) and the keyword-match glass promotion below it. Confirmed reachable by two real Skyrim populations: `MultiLayerParallax` (11) on layered ice surfaces (`icefrozen01`/`icecavewall01`/`icelakesurface`, widened into keyword reachability by #3359) loses its inner-layer-parallax dispatch in `triangle.frag` and takes flat glass refraction instead; and glowing soul gems (`gem` keyword) lose their glow dispatch to flat glass — the exact bug shape #2710 already fixed for the effect-shader carrier but not for the lit carrier. The `is_mirror_pane` arm additionally hard-zeroes `material_kind` unconditionally, destroying an authored `EnvironmentMap` (1) dispatch.

**Impact**: Ice surfaces and soul gems (real, named vanilla Skyrim assets) render as flat glass instead of their authored parallax/glow shader dispatch. Mirror surfaces lose their `EnvironmentMap` dispatch unconditionally.

**Related**: #2710 (fixed the same bug shape for the effect-shader carrier only); SKY-D7-2026-09-11-02 (structural root cause, filed as a companion MEDIUM finding).

**Suggested Fix**: Gate `classify_glass_into_material`'s override on the same external-material provenance check the effect-shader carrier already requires, so an authored low-range lit shader type is protected the same way a synthesized one is. Content-side reachability sizing (a BSA-wide BSLSP scan or `--bench-hold`/`byro-dbg` census) would quantify the full blast radius.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix stays at the NIFAL parser→`Material` boundary (`classify_glass_into_material`/`translate_material`), not pushed into the renderer — see `/audit-nifal`.
- [ ] **SIBLING**: Verify `is_mirror_pane`'s unconditional zero of `material_kind` is fixed alongside the keyword-glass-promotion path
- [ ] **TESTS**: Regression tests for `icefrozen01`/`icecavewall01`/`icelakesurface` (MultiLayerParallax) and a `gem`-keyword soul gem (glow) assert `material_kind` survives glass classification
