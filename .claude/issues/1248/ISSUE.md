# REN: derive Fresnel F0 from IOR instead of hardcoded vec3(0.04)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1248
## Problem

Every fragment-shader F0 site in [triangle.frag](crates/renderer/shaders/triangle.frag) uses the hardcoded literal `vec3(0.04)` for dielectric Fresnel. This is the *consequence* of assuming IOR=1.5, not a physical constant. As a result:

- Glass (`MATERIAL_KIND_GLASS`, IOR=1.45 hardcoded elsewhere) gets the wrong F0 (0.04 vs the correct 0.0337).
- Water, gemstones, ice, polished stone — anything with an authored IOR ≠ 1.5 — is inaccessible.
- BGSM v9+ and future Starfield materials that ship explicit IOR cannot be honoured.

## Reference

GLSL-PathTracer `src/shaders/common/disney.glsl:56-57` (MIT, knightcrawler25/GLSL-PathTracer):

```glsl
F0 = (1.0 - eta) / (1.0 + eta);
F0 *= F0;
```

With η=1.5 → F0=0.04 (reproduces current behaviour). With η=1.45 → F0=0.0337. Diamond (η=2.42) → F0=0.172.

## Proposed change

1. Add `ior: f32` to `GpuMaterial` in [scene_buffer/gpu_types.rs](crates/renderer/src/vulkan/scene_buffer/gpu_types.rs). Default 1.5. 4 bytes — bumps struct from 280 → 284 (one padding slot will need re-shuffling; `material::layout_test` at material.rs:796 will catch the size change).
2. Replace `vec3(0.04)` at every F0 derivation site in triangle.frag (~9 sites: 1313, 1469-1474, 1669, 1671, 1724, 1728, 1753, 2035 per 2026-05-23 research) with:
   ```glsl
   float eta = material.ior;
   float f0Dielectric = pow((1.0 - eta) / (1.0 + eta), 2.0);
   vec3 F0 = mix(vec3(f0Dielectric), baseColor, metalness);
   ```
3. Drop the hardcoded 1.45 inside the `MATERIAL_KIND_GLASS` branch — it becomes data.
4. Default IOR=1.5 means the test suite output is unchanged for legacy NIF content that doesn't author IOR.

## Why now

Unblocks:
- Correct glass / water / gemstone rendering
- BGSM v9 PBR material support
- Any future Starfield material with authored IOR

## Credit

Reference impl: knightcrawler25/GLSL-PathTracer (MIT). Attribution will land in `crates/renderer/src/vulkan/material.rs` module header.