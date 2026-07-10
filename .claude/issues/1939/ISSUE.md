# SKY-D18-01: Effect_Lit shading path negates sun direction, inverting the hemisphere

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1939

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/triangle.frag:606-609` (the `MAT_FLAG_EFFECT_LIT` / #890 Stage 2 block)
**Status**: NEW

## Description
The effect-lit scene-lighting path negates the directional light's `direction_angle` when computing N·L, inverting the sun hemisphere. Every other sun consumer in the codebase treats `direction_angle`/`sun_direction` as "toward the sun" (unnegated); this one path assumes "from the sun."

## Evidence
`triangle.frag:606-609` computes `NdotL = max(dot(N, -Ldir), 0.0)` on the same `direction_angle` that the main directional path (`:2104`+`:2199`) correctly uses unnegated as `dot(N, L)`. `direction_angle` = `CellLightingRes::directional_dir` = `sun_dir` from `compute_sun_arc`, which points toward the sun. `water.frag:114` documents `GpuLight.direction_angle` as "direction TO the sun." Physical check at solar noon: a +Y floor should be fully lit; main path gives `dot(up,+up)=1`, effect-lit path gives `dot(up,-up)=−1→0` (dark). `MAT_FLAG_EFFECT_LIT` confirmed reachable in production via `pack_effect_shader_flags`.

## Impact
On `BSEffectShaderProperty` Effect_Lit surfaces (Skyrim spell FX, FO4 magic / power-armor ambient effects) under an exterior directional sun, the additive scene-lit N·L term is computed against the wrong hemisphere: the sun-facing side goes dark and the shadow side lights up. Narrow surface class and additive-only (base emissive + cell ambient still render), hence not catastrophic, but a definite correctness inversion.

## Related
Same wrong-assumption pattern as VOL-D16-01 (`volumetrics_inject.comp`) — both recent additions guessed "from-sun." #890 (feature that introduced the site, commit `2aa2817a`). The Cornell RT reference harness has no directional-sun variant and could not have caught this (see CORN-D21-01).

## Suggested Fix
Drop the negation so the effect-lit path matches the canonical convention: `float NdotL = max(dot(N, Ldir), 0.0);` (Ldir already = toward-sun). Recompile `triangle.frag.spv`. Needs RenderDoc verification on an exterior cell containing an Effect_Lit surface at solar noon before/after.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
