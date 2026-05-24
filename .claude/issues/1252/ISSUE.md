# REN-D6-2026-05-24-01: Disney sheen lobe over-amplified by π at per-light BRDF

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1252

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md` (Dim 6)
**Severity**: MEDIUM
**Dimension**: Shader Correctness
**Regression of**: #1249 (`005eba25`, 2026-05-23)

## Description

`disneyDiffuseTerm` (in `crates/renderer/shaders/triangle.frag`, ~line 647-680) returns
```glsl
albedo * mix(Fd+Fretro, ss, subsurface) * (1.0 / PI) + Fsheen
```
— the diffuse component is correctly normalised by `1/PI` per Lambert / Burley convention, but `Fsheen` is intentionally NOT divided by PI per Disney 2012's specification (sheen is a layered Fresnel-shaped edge highlight, not a Lambertian term).

The per-light BRDF call site at `triangle.frag:~2604` then multiplies the entire result by `PI`:

```glsl
diffuseBrdf = disneyDiffuseTerm(
    albedo, roughness, mat.subsurface, mat.sheen, mat.sheenTint,
    NdotL, NdotV, HdotL
) * (1.0 - metalness) * PI;
```

The `* PI` exists to compensate the legacy `kD * albedo` (no `/PI`) scaling shape at this site — but applying it to the whole Disney return ALSO scales the sheen component by π, over-amplifying it ~3.14×.

The fallback-directional site at the top of the BRDF section is correct — it uses `disneyDiffuseTerm(...) * (1.0 - metalness)` WITHOUT the `* PI` because that site keeps the legacy `kD * albedo / PI` scaling shape. Asymmetric per-site fix is the regression vector.

## Impact

Visible only on materials that author `sheen > 0` AND set `MAT_FLAG_BGSM_PBR`:
- Cloth / silk / velvet / fabric surfaces

Currently **ZERO corpus impact** because (a) default `sheen = 0` keeps every legacy NIF on the Lambert branch, (b) no preset in `pub mod presets` sets sheen, (c) no BGSM importer surfaces sheen yet. Latent regression that activates the moment a BGSM v9+ material with authored sheen lands or a future preset enables it.

The over-amplification is on the per-LIGHT loop, so the artifact compounds per scene light — a cluster with 4 lights at the cloth surface produces 4 × π ≈ 12.5× over-bright sheen edge.

## Suggested Fix

Split `disneyDiffuseTerm` into two helpers returning the diffuse and sheen parts separately so each call site can apply its own scaling:

```glsl
vec3 disneyDiffuseLobe(vec3 albedo, float roughness, float subsurface,
                      float NdotL, float NdotV, float HdotL) {
    // ... Burley Fd + Fretro + HK ss ...
    return albedo * mix(Fd+Fretro, ss, subsurface) / PI;
}

vec3 disneySheenLobe(vec3 albedo, float sheen, float sheenTint, float HdotL) {
    float FH = pow(clamp(1.0 - HdotL, 0.0, 1.0), 5.0);
    vec3 sheenColor = mix(vec3(1.0), albedo, sheenTint);
    return FH * sheen * sheenColor; // never divided by PI
}
```

Per-light call site:

```glsl
diffuseBrdf = (disneyDiffuseLobe(...) * PI + disneySheenLobe(...))
            * (1.0 - metalness);
```

Fallback-directional site:

```glsl
diffuseBrdf = (disneyDiffuseLobe(...) + disneySheenLobe(...) / PI)
            * (1.0 - metalness);
```

Alternative: return a struct `{ vec3 diffuse; vec3 sheen; }` so the compositional shape is explicit at every call site.

## Related

- #1249 (`005eba25`): regression source — introduced `disneyDiffuseTerm` + both call sites
- knightcrawler25/GLSL-PathTracer (MIT): reference impl `disney.glsl:67-87`
- Disney 2012 "Physically-Based Shading at Disney" — sheen NOT divided by PI

## Completeness Checks

- [ ] **UNSAFE**: N/A — shader change
- [ ] **SIBLING**: only 2 BRDF call sites in triangle.frag (fallback-directional + per-light loop) — both swept in the fix
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: visual A/B on a BGSM-PBR material with `sheen = 1.0` (no in-tree fixture today — RenderDoc gating). Unit-level pin: the `disneyDiffuseLobe` + `disneySheenLobe` split returns should match the current `disneyDiffuseTerm` output when the per-site scaling is composed correctly — pinnable via a host-side reference impl + a synthetic GpuMaterial fixture.