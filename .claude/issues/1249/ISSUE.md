# REN: port Disney diffuse (Burley retro + sheen) beside current Lambert

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1249
## Problem

Direct-light diffuse in [triangle.frag](crates/renderer/shaders/triangle.frag) is plain Lambert. Misses:
- **Burley retro-reflection** — edge brightening at grazing view angles (cloth, rough wood, sand all read too flat).
- **Sheen lobe** — fabric-specific Fresnel highlight that gives velvet / silk / wool their characteristic look.
- **Hanrahan-Krueger subsurface approximation** — cheap fake-SSS for waxy / marble / skin without a full BSSRDF.

Most visible on FO4 outfits that were authored against a PBR pipeline and currently render as flat matte.

## Reference

GLSL-PathTracer `src/shaders/common/disney.glsl:67-87` (MIT). ~20 lines:

```glsl
vec3 EvalDisneyDiffuse(Material mat, vec3 Csheen, vec3 V, vec3 L, vec3 H, out float pdf) {
    pdf = 0.0;
    float LDotH = dot(L, H);
    float Rr = 2.0 * mat.roughness * LDotH * LDotH;
    float FL = SchlickWeight(L.z), FV = SchlickWeight(V.z);
    float Fretro = Rr * (FL + FV + FL * FV * (Rr - 1.0));
    float Fd = (1.0 - 0.5 * FL) * (1.0 - 0.5 * FV);
    // Fake subsurface (Hanrahan-Krueger)
    float Fss90 = 0.5 * Rr;
    float Fss = mix(1.0, Fss90, FL) * mix(1.0, Fss90, FV);
    float ss = 1.25 * (Fss * (1.0 / (L.z + V.z) - 0.5) + 0.5);
    // Sheen
    float FH = SchlickWeight(LDotH);
    vec3 Fsheen = FH * mat.sheen * Csheen;
    pdf = L.z * INV_PI;
    return INV_PI * mat.baseColor * mix(Fd + Fretro, ss, mat.subsurface) + Fsheen;
}
```

## Proposed change

1. Add `subsurface: f32`, `sheen: f32`, `sheen_tint: f32` to `GpuMaterial`. Defaults 0.0 (matches current Lambert behaviour exactly when unset).
2. Port `EvalDisneyDiffuse` as a helper in triangle.frag, gated on `MAT_FLAG_BGSM_PBR` so legacy NIF content stays on plain Lambert.
3. Wire BGSM `subsurface_*` and `subsurface_color` fields (already in `GpuMaterial` from M30 work — verify they're plumbed) into the new lobe.

## Dependencies

Cleaner if landed *after* #ior-from-F0 (so we have correct dielectric F0 for the sheen Fresnel weight).

## Credit

Reference impl: knightcrawler25/GLSL-PathTracer (MIT). Attribution in module header.