# REN-D15-10: Dead SHORELINE_RAY_MAX constant and no-op reflColor mix in water.frag

Labels: low, renderer, bug

## Description

Three dead items. `SHORELINE_RAY_MAX = 256.0` is the misleading one — it reads as the cap on `foamShoreline`'s ray, but that function's `tMax` is `push.tune.z` (`shoreline_width`, default 32.0, never overwritten), so the 256 has no effect and contradicts the live value by 8×. The `mix(reflectionMiss, reflColor, reflHit ? 1.0 : 0.0)` is a provable no-op (`traceWaterRay` already returns `missFallback == reflectionMiss` when `reflHit` is false). All fold away; the cost is reviewer time.

## Location

`crates/renderer/shaders/water.frag` (`PI`, `SHORELINE_RAY_MAX`, the `reflColor` `mix`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-10).

https://github.com/matiaszanolli/ByroRedux/issues/2804
