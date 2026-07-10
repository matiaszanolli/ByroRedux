# CORN-D21-02: Cornell glass-probe docstring misstates finalAlpha and the refraction-path gate

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1943

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `byroredux/src/cornell.rs:238-251` (glass block comment) and `:320-332` (`glass()` helper + docstring)
**Status**: NEW

## Description
Two inaccuracies in the glass-probe documentation: (1) the block comment asserts "opaque neutral texture → finalAlpha 1.0", but `glass()` sets `alpha: 0.25`, which flows through to `GpuMaterial.materialAlpha` and is multiplied into `texColor.a` — finalAlpha for the glass probes starts at ~0.25, not 1.0. (2) The `glass()` docstring claims `alpha` is "a transmissive alpha so the IOR refraction path engages" — the refraction gate actually keys on `mat.materialKind == MATERIAL_KIND_GLASS && roughness < 0.35`, not alpha; `alpha: 0.25` does not engage refraction.

## Evidence
`triangle.frag:186` (`texColor.a *= mat.materialAlpha`), `:1027` (isGlass gate on materialKind+roughness, not alpha), `:3030-3032` (finalAlpha computation). `static_meshes.rs:652` (`material_alpha: mat.map(|m| m.alpha)`). `Material::alpha` default is `1.0`, so only Cornell's explicit `0.25` triggers this.

## Impact
Harmless today — the alpha marker is currently unconsumed downstream (confirmed via `taa.comp`/`composite.frag`). Latent-fragile: if a future composite branch gates on that alpha marker for glass/decal classification, Cornell's own glass probes would be mis-tagged where opaque surfaces are expected to write 1.0. Being a reference scene, a docstring that mis-states renderer alpha semantics is itself a risk.

## Related
#676 / DEN-6 alpha-marker plumbing

## Suggested Fix
Either drop `alpha` back to the `1.0` default in `glass()` (the refraction path doesn't need it) and correct the two comments, or keep `0.25` and fix the docstrings to accurately state finalAlpha and the real refraction gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
