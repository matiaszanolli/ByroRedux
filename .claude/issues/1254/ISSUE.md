# REN-D6-2026-05-24-03: deriveAxAy lacks anisotropic-domain clamp (sqrt(neg) risk)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1254

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md` (Dim 6)
**Severity**: LOW (defense-in-depth gap)
**Dimension**: Shader Correctness

## Description

The Disney aspect formula in `crates/renderer/shaders/triangle.frag:~625` (`deriveAxAy`) is `aspect = sqrt(1 - anisotropic * 0.9)`. For `anisotropic ∈ [0, 1]` the radicand `1 - 0.9·a` ranges from 1.0 down to 0.1 — all positive.

But if a future authoring surface ships `anisotropic > 1.0` (unclamped BGSM v9+ value, or a Starfield `.mat` field with a different range convention):

- `1 - 0.9·a < 0` and `sqrt(...)` becomes NaN on GLSL spec-conformant drivers (implementation-defined / poison value on others)
- Downstream `ax = alpha / aspect` then divides by NaN
- `distributionGGXAniso` returns NaN → black pixel or undefined-color fragment

Same shape concern with `anisotropic < 0`: aspect > 1 → valid sqrt, but `ax / aspect` shrinks ax below the intended floor, producing a sharper-than-intended lobe.

## Impact

No live path today — no importer surfaces `mat.anisotropic`, and the `GpuMaterial::default() → anisotropic = 0.0` keeps the bad branch unreachable. Same defense-in-depth concern as REN-D6-2026-05-24-02 (#TBD). Failure mode is visible black pixels at the boundary fragments where anisotropic authoring crosses the bad range.

## Suggested Fix

Single-line clamp at the top of the helper:

```glsl
void deriveAxAy(float roughness, float anisotropic, out float ax, out float ay) {
    float a = roughness * roughness;
    float aniso = clamp(anisotropic, 0.0, 1.0); // guard against bad importer data
    float aspect = sqrt(1.0 - aniso * 0.9);
    ax = max(0.025 * 0.025, a / aspect);
    ay = max(0.025 * 0.025, a * aspect);
}
```

## Related

- #1250 (`c0374d00`, 2026-05-23): introduced `deriveAxAy`
- Sibling #1253 (`dielectricF0FromIor` IOR clamp): same input-domain-clamp pattern

## Completeness Checks

- [ ] **UNSAFE**: N/A — shader change
- [ ] **SIBLING**: pair with #1253 — same defensive-clamp pattern. Worth landing both in one PR.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic unit test on a host-side mirror with `anisotropic = 1.5` input asserting `ax > 0` and `ay > 0` (no NaN)