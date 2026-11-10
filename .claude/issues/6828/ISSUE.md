# REN-D8-07: `heightFogOpticalDepth` overflows (full-opacity fog or NaN) for a descending ray starting more than ~80 scale heights above the reference height

Labels: medium,renderer,shaders,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: MEDIUM (visual only; reachable by the free camera / `--camera-pos` benches, not by gameplay). **Status**: NEW — regression introduced by `3500d2737` (#6728); the previous form clamped its log-sigma to +/-80 and stayed finite.

**Location**: `heightFogOpticalDepth` and `expRunOpticalDepth` (`crates/renderer/shaders/composite.frag`).

## Description
For `aboveStart >= 0` and a descending ray the shader forms `sigmaStart = sigma0 * exp(-min(aboveStart/H, 80))` and then `expRunOpticalDepth(sigmaStart, slope<0, crossing) = sigmaStart * (1 - exp(-slope*crossing)) / slope`. Here `-slope*crossing = aboveStart/H` (r), so the numerator evaluates `exp(r)`. Above r = 80 the `min(.., 80)` clamp makes `sigmaStart` too large by `e^(r-80)`; at r >= 88.7 `exp(r)` overflows fp32 to `inf`, so the segment's optical depth is `inf` and `exp(-min(tau, 80))` reads that as total opacity. If `sigmaStart` flushes to zero (sigma0 times `e^-80` is subnormal), `0 * inf` is NaN and the NaN goes into the HDR target.

## Evidence
fp32 emulation vs a 200 000-step numeric integral of the clamped profile (sigma0 = 0.002/m, H = 30 m, slope -0.5): r = 66.7 and 80.0 match to 1e-7; r = 86.7 gives 99.5 vs true 5.32; r = 90 gives `inf` vs 5.52; r = 166.7 gives `inf` vs 10.1. With the default 30 m scale height r = 80 is 2.4 km above the reference; with an authored FO4/FO76 143 m scale it is ~11 km. The true integral is bounded by `sigma0 * (1 - e^-r) / |slope|`.

## Impact
A free camera > ~2.4 km above the ground reference looking down at geometry past the grid gets solid fog colour (or a NaN pixel that TAA/bloom then spread) instead of thin fog. The `inf` case follows from IEEE arithmetic; the NaN variant depends on the GPU's fp32 denormal mode (RenderDoc).

## Suggested Fix
Compute the descending-to-reference segment from its end values instead of an overflowing exponent: `tau = (sigma0 - sigmaStart) / abs(slope)` for the segment ending at the reference (end density is exactly `sigma0`), and `(sigmaEnd - sigmaStart) / abs(slope)` with `sigmaEnd = sigma0 * exp(-(aboveStart + dirY*distance)/H)` when it does not reach the reference. Finite for every r, no `80` clamp needed. Pair with REN-D8-08 for an fp32 oracle.

Related: #6728, REN-D8-08. Report: `docs/audits/AUDIT_RENDERER_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the horizontal branch and the cloud/air-column paths in `composite.frag`)
- [ ] **TESTS**: A regression test pins this specific fix

