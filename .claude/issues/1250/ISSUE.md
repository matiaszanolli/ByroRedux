# REN: adopt anisotropic ax/ay derivation in distributionGGX (prep for hair/brushed-metal)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1250
## Problem

Our `distributionGGX` in [triangle.frag](crates/renderer/shaders/triangle.frag) takes a single scalar roughness and does `a = roughness²; a2 = a²` internally. This:

- Cannot express anisotropic materials (hair shading along the strand axis, brushed metal, vinyl).
- Clamps roughness at 0.025 in `specularAaRoughness()` (vs 0.001 in GLSL-PathTracer) — 25× looser, blurs tight highlights at close range.
- Leaves `mat.roughness` semantically ambiguous between "perceptual" (glTF spec, what authors set) and "linear α" (what GGX wants).

## Reference

GLSL-PathTracer `src/shaders/common/pathtrace.glsl:100-102` (MIT):

```glsl
float aspect = sqrt(1.0 - mat.anisotropic * 0.9);
mat.ax = max(0.001, mat.roughness / aspect);
mat.ay = max(0.001, mat.roughness * aspect);
```

The `* 0.9` cap prevents complete needle degeneracy at anisotropic=1. The 0.001 floor preserves highlight sharpness.

Lobe eval uses `GTR2Aniso(NdotH, HdotX, HdotY, ax, ay)` from `sampling.glsl:90-95`.

## Proposed change

1. Add `anisotropic: f32` to `GpuMaterial`. Default 0.0 → `ax = ay = roughness` → identical to current isotropic behaviour.
2. Compute `ax, ay` at material-load time (in shader, top of fragment body), pass to a new `distributionGGXAniso(NdotH, HdotX, HdotY, ax, ay)` helper.
3. Drop the `specularAaRoughness()` 0.025 clamp to 0.001² = 1e-6 minimum α² (GLSL-PathTracer parity). Validates that our specular-AA was a hack around a missing minimum, not a real visual constraint.
4. Requires tangent (`HdotX`) and bitangent (`HdotY`) at the shading point — we already pass tangent in vertex attrs (see vertex.rs), so bitangent = `cross(N, T) * tangent_w`.

## Why now

Even with `anisotropic = 0` shipped today, the refactor:
- Drops the GGX α² floor to literature-standard
- Sets up hair / brushed-metal lobes for free when we have authored data
- Makes the perceptual-vs-linear roughness semantic explicit in code

## Credit

Reference impl: knightcrawler25/GLSL-PathTracer (MIT). Attribution in module header.