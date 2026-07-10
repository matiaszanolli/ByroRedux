# VOL-D16-01: Sun visibility ray cast in the wrong hemisphere (host/shader sun_direction convention mismatch)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1937

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/volumetrics_inject.comp:270,313-327` (`light_in`, `ray_dir = light_in`, `traceOcclusion`) fed by `crates/renderer/src/vulkan/context/draw.rs:695-698` (`sun_dir`, no negation)
**Status**: NEW

## Description
The inject shader documents `sun_dir` as pointing "FROM the sun toward the ground" and computes `light_in = -normalize(sun_dir)`, which it then uses both as the HG propagation vector and as the shadow-ray direction (`ray_dir = light_in`). But the host actually populates `sun_direction` as the direction TO the sun (light-incoming) — the same convention `water.frag`, `composite.frag`'s sun-disc, and the main directional lighting path all use. At runtime `light_in = -(toward-sun) = away-from-sun` (downward), and the sun-visibility ray is cast away from the sun, into the ground/floor instead of toward it.

## Evidence
The codebase convention for `sun_direction` is unambiguously "toward the sun": `composite.frag:242` draws the sun disc where the outward sky view-ray aligns with `sun_direction`; `water.frag:114` explicitly documents `sunDirection` as "direction TO the sun (light-incoming)"; the same `SkyParams::sun_direction` is copied unnegated into both the GpuScene and volumetrics UBOs; runtime defaults point +Y up = toward sun. Consequence: for any froxel with terrain/floor below (essentially all of them), the `SHADOW_MASK_OPAQUE` ray hits immediately → `visibility = 0` → sun in-scatter is zeroed. The Henyey-Greenstein phase term is coincidentally correct through the same double-negation — this is purely a ray-direction defect, not a phase defect (explicitly checked and disproved as a phase issue).

## Impact
Daytime exterior volumetric sun shafts and interior sun-through-window god-rays — the primary reason `VOLUMETRIC_OUTPUT_CONSUMED` was flipped `true` in `977eb95a` — produce ~zero sun in-scatter. Only the Phase 2b point/spot lantern glow (which does not use `sun_dir`) survives, which can mask the regression (fog looks "on"). Interiors with `sun_intensity <= 0` are unaffected.

## Related
Introduced/exposed by the `977eb95a` Phase 2b rewrite + the `VOLUMETRIC_OUTPUT_CONSUMED=true` flip. Same bug class independently found in the Effect_Lit shading path (see the sky/weather dimension finding, SKY-D18-01). The Cornell RT reference harness has no directional-sun variant and could not have caught this (see CORN-D21-01).

## Suggested Fix
Cast the visibility ray toward the sun, i.e. use `-light_in` (equivalently `normalize(sun_dir)` under the actual host convention) as `ray_dir`, while keeping `light_in` as-is for the HG cosine so the (correct) phase result is preserved. Do NOT "fix" by negating the host value — that would keep the ray correct but break the HG cosine. Verify with RenderDoc on a daytime exterior cell (expect sun shafts to appear) before committing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
