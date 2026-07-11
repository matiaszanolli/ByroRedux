# #1916: REN-D2-04 — GpuLight shader-struct-sync enumeration misses volumetrics_inject.comp

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:146-148` (doc-comment above `pub struct GpuLight`)

## Description
The lockstep guard comment enumerated `triangle.frag`, `cluster_cull.comp`,
`caustic_splat.comp` as the shaders declaring `struct GpuLight`. Commit `977eb95a`
added a fourth declaration in `volumetrics_inject.comp`. No automated test pinned
the GLSL copies against each other, so the enumeration going stale meant nothing
would catch a future `GpuLight` field change landing in three of four copies.

## Suggested Fix
Add `volumetrics_inject.comp` to the enumeration; add a static grep/parse test
asserting the four declarations stay field-for-field identical.

---

# #1917: REN-D3-01 — composite.frag.spv is stale

**Severity**: medium
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag.spv` (last built `9c10f14e`) vs
`crates/renderer/shaders/composite.frag:417-445` (changed at `977eb95a`)

## Description
Commit `977eb95a` rewrote the volumetric-apply block in `composite.frag` from a
runtime `if (params.depth_params.z > 0.5) {...} else {...}` branch to the
unconditional `combined = combined * vol.a + vol.rgb;`, but recompiled only
`triangle.frag.spv` and the two volumetrics `.spv`s — not `composite.frag.spv`.
Since shaders ship as committed binaries via `include_bytes!`, the stale binary
is the runtime shader. No behavioral divergence today (`draw.rs` pins
`depth_params.z = 1.0`), but the hazard was latent for the next unrelated
`composite.frag` recompile.

## Suggested Fix
Recompile with `glslangValidator -V composite.frag -o composite.frag.spv` and
commit the `.spv`.
