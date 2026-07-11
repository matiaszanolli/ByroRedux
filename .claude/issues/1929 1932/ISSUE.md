# #1929: REN-D11-01 — triangle.vert.spv compiled to SPIR-V 1.5 while every sibling is 1.0

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/triangle.vert.spv` vs the documented build command

## Description
`triangle.vert.spv` was stamped SPIR-V 1.5 while every other committed shader
(and the documented `glslangValidator -V` recompile command) produces SPIR-V
1.0. Someone recompiled with a stray `--target-env` flag not captured in the
docs — the documented command silently regresses the file if followed
literally. No functional divergence (I/O semantics identical), but a
reproducibility/churn hazard.

## Note
SIBLING sweep of all 20 committed `.spv` files found a second, previously
unreported instance of the same drift: `taa.comp.spv` was also SPIR-V 1.5.
Fixed both.

## Suggested Fix
Recompile with the same plain `-V` invocation as the other shaders; add a
regression test pinning every committed `.spv` to SPIR-V 1.0.

---

# #1932: TAA-D13-01 — Halton jitter gate omits the taa_failed check

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/context/draw.rs:2490` (jitter block)

## Description
The Halton jitter gate was `if self.taa.is_some()` only, unlike the dispatch
gate and `upload_params`, both of which additionally check `!self.taa_failed`.
If `taa_failed` ever latched at runtime, geometry would keep rendering with a
per-frame sub-pixel Halton offset while composite fell back to raw,
temporally-unresolved HDR — full-frame shimmer until the next swapchain
resize clears the latch. Not reachable today (`TaaPipeline::dispatch` is
infallible), but latent for the day it becomes fallible.

## Suggested Fix
Change the gate to `if self.taa.is_some() && !self.taa_failed`.
