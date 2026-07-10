# REN-D2-04: GpuLight shader-struct-sync enumeration (both in gpu_types.rs doc comment and dim-16 independent discovery) misses the new fourth GLSL copy in volumetrics_inject.comp

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1916

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:146-148` (doc-comment above `pub struct GpuLight`)
**Status**: NEW

## Description
The lockstep guard comment reads "every shader that declares `struct GpuLight` must mirror this layout (currently `triangle.frag`, `cluster_cull.comp`, `caustic_splat.comp`)". Commit `977eb95a` added a fourth declaration in `volumetrics_inject.comp` (that shader's bindings are set-0-numbered and can't `#include` `bindings.glsl` wholesale). No automated test pins the GLSL copies, so the enumeration being stale means the guard itself no longer routes a future `GpuLight` layout change to all copies. The copies are byte-identical today. (This exact gap was independently rediscovered by the Volumetrics/Bloom dimension of the same audit — corroborating evidence, not two separate bugs.)

## Evidence
`grep -l "struct GpuLight" crates/renderer/shaders/**` → `bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`, `volumetrics_inject.comp`; no `.rs` test references `volumetrics_inject` in the scene_buffer test set.

## Impact
A future `GpuLight` field addition following the comment would update three of four copies; the volumetrics copy would silently mis-decode every light (wrong fog glow color/position) — invisible to cargo test.

## Related
Shader Struct Sync precedent (post-#1583/#1590 GpuInstance); commit `977eb95a`

## Suggested Fix
Add `volumetrics_inject.comp` to the enumeration; add a small static grep test asserting the four declarations stay textually identical.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
