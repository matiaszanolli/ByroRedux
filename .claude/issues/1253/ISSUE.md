# REN-D6-2026-05-24-02: dielectricF0FromIor lacks IOR-domain clamp

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1253

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md` (Dim 6)
**Severity**: LOW (defense-in-depth gap)
**Dimension**: Shader Correctness

## Description

The Schlick F0 formula `((1-η)/(1+η))²` in `crates/renderer/shaders/triangle.frag:~600` (`dielectricF0FromIor`) produces non-negative output for any real η (the square absorbs sign), but the formula is physically meaningful only for η > 0.

If `mat.ior == 0` (uninitialized garbage from a future importer that forgets to set it, or a BGSM parser regression), the formula yields `((1-0)/(1+0))² = 1.0` — mirror-like F0 on what should be a dielectric.

The current `GpuMaterial::default() → ior = 1.5` blocks the bad state from materialising on legacy NIF content. No live path produces `ior = 0` today.

## Impact

Defense-in-depth gap only. Failure mode is silvered-glass / chrome-style Fresnel on a surface that should be dielectric. Visible immediately on the affected mesh — easy to bisect when it ever happens, but cheap to prevent.

## Suggested Fix

Single-line clamp inside the helper:

```glsl
float dielectricF0FromIor(float eta) {
    float e = max(eta, 1e-3); // guard against importer-side zeros
    float r = (1.0 - e) / (1.0 + e);
    return r * r;
}
```

Alternative: clamp at the importer / `to_gpu_material` boundary so the bad value never reaches the shader. Either works; the shader-side guard is cheaper insurance.

## Related

- #1248 (`454b7a26`, 2026-05-23): introduced `dielectricF0FromIor`
- Sibling REN-D6-2026-05-24-03 (#TBD): `deriveAxAy` has same defense-in-depth gap

## Completeness Checks

- [ ] **UNSAFE**: N/A — shader change
- [ ] **SIBLING**: see REN-D6-2026-05-24-03 — same input-domain-clamp pattern on `deriveAxAy`. Worth landing both in one PR.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic unit test on a host-side mirror of the formula with `eta = 0.0` input asserting bounded F0