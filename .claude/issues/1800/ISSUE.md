# PERF-D5-NEW-02: One-bounce-GI hit irradiance samples the first 8 lights in upload order, not the 8 relevant to the hit point

**Issue**: #1800
**Labels**: medium,renderer,pipeline,performance,bug
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-02)

**Severity**: MEDIUM
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (PERF-D5-NEW-02)

## Location
`crates/renderer/shaders/include/lighting.glsl:178-192` (`giHitIrradiance` fixed-prefix loop + post-hoc `dist > radius` skip), `include/shader_constants.glsl:39` (`GI_HIT_LIGHT_CAP = 8u`); light upload order `byroredux/src/render/lights.rs:73-160`

## Description
`giHitIrradiance` loops `count = min(lightCount, GI_HIT_LIGHT_CAP)` over the global light SSBO prefix in upload order. `collect_lights` uploads the cell directional first, then point lights in arbitrary ECS sparse-set iteration order — proximity-blind. A `dist > radius` skip exists but only after the fixed-prefix selection, so lights past index 8 are never considered. In any cell with >8 lights (taverns, Whiterun/Dragonsreach-class interiors), the GI bounce permanently ignores every light past index 8 while still firing up to 8 shadow rays against a fixed prefix that may be entirely out of range of the hit point. The primary-fragment path solved this exact problem with clustered culling + RIS; the bounce path regressed to an unsorted prefix.

## Evidence
`lighting.glsl:178` `count = min(lightCount, GI_HIT_LIGHT_CAP)`; index order = upload order, confirmed by `lights.rs`'s plain ECS-iteration push loop.

## Impact
Two-sided — quality (bounce lighting in >8-light cells is systematically wrong/dim and can flicker across cell reloads as ECS iteration order changes which 8 lights exist for GI) and efficiency (up to 8 ray-query traces per lit fragment — the single largest per-pixel ray budget in the frame — spent on an unprioritized set). The per-pixel cap is doing its perf job; this is about spending that budget on the wrong lights. Confidence: HIGH on the premise (both sides code-verified); impact magnitude is scene-dependent.

## Related
`GI_HIT_LIGHT_CAP` comment.

## Suggested Fix
Prioritize the prefix CPU-side (sort `gpu_lights[1..]` by intensity·radius, one small sort/frame) so "first 8" approximates "8 most influential"; or select per-hit by distance with an early-out after 8 contributing lights. Keep the ray-count cap unchanged.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader passes / other reservoir arrays)
- [ ] **TESTS**: A regression test pins this specific fix

