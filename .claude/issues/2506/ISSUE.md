# REN-D14-2026-08-07-02: EMA decay pass still floors while the deposit stochastically rounds (#2239 half-fix)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2506
**Finding ID**: REN-D14-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 14 — Caustics
**Location**: `crates/renderer/shaders/caustic_splat.comp`, the `pc.decayOnly == 1u` block
**Status**: NEW (residual of the fix for #2239)

## Description
#2239 identified that the parked-camera EMA drove dim caustics to zero because the per-tap deposit truncated sub-ULP values every frame, and fixed it by stochastically rounding the deposit. The *paired* operation — the decay pass — was not changed and still truncates: `uint(float(v) * pc.decayFactor)` discards a mean 0.5 fixed-point ULP per texel per frame. That is a constant additive drain, so the EMA's steady state is `A* = (D - 0.5) / (1 - decay)` instead of `D / (1 - decay)`, short by `0.5 / (1 - decay)` fixed-point units. At `CAUSTIC_DECAY_MAX = 0.995` that is 100 units ≈ `100/65536 = 0.0015` luminance; any pool texel whose true per-frame deposit is below 0.5 ULP still collapses to exactly zero no matter how many frames pass, reproducing the #2239 symptom on the decay side.

## Evidence
```glsl
if (pc.decayOnly == 1u) {
    uint v = imageLoad(causticAccum, pixel).r;
    imageStore(causticAccum, pixel, uvec4(uint(float(v) * pc.decayFactor), 0u, 0u, 0u));
    return;
}
```
contrasted with the deposit path, which does dither:
```glsl
if (pc.decayFactor > 0.0) {
    float fracPart = depositF - float(fv);
    ...
    if (fracPart > ditherThreshold) { fv += 1u; }
}
```

## Impact
Bounded erosion of the dim outskirts of a parked-camera caustic pool (hard-edged, slightly-too-small pool; sub-0.0015-luminance caustics vanish entirely). Much smaller than the pre-#2239 unbounded collapse, and only while parked.

## Related
#2239, commit `4279c195`; `AUDIT_RENDERER_2026-08-02.md` REN-D14-02.

## Suggested Fix
Apply the same PCG-hash stochastic rounding to the decay `imageStore` (round `v * decayFactor` up when its fraction exceeds a per-(texel, frame) threshold), so the multiply is unbiased in expectation like the deposit now is.

## Completeness Checks
- [ ] **TESTS**: N/A shader-side; document the fix rationale inline mirroring the deposit-path pattern
