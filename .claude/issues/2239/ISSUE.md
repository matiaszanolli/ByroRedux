# REN-D14-02: Parked-camera caustic EMA truncates dim caustics toward zero via fixed-point atomic underflow

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2239

**Dimension**: 14 (Caustics)
**Location**: `crates/renderer/shaders/caustic_splat.comp` (`emaWeight = 1.0 - pc.decayFactor;` and the fixed-point `imageAtomicAdd` deposit, ~line 502-520)
**Status**: NEW

**Description**: When the camera is parked, each frame's caustic deposit is scaled by `emaWeight = 1 - decayFactor` before being added via a fixed-point (scaled-integer) `imageAtomicAdd`. For a dim caustic, `contrib * emaWeight` can round below one fixed-point ULP and add zero every frame — the running average never converges to its true steady-state brightness and instead decays toward zero, the same class of bug `#1942` fixed for the sun path.

**Impact**: Dim caustics (weak light, high travel distance, low albedo) fade to invisible under a parked camera instead of converging to their correct (dim but nonzero) steady-state brightness.

**Related**: #1942 (prior fix for the analogous sun-path truncation issue)

**Suggested Fix**: apply the same fix pattern as #1942 — accumulate in a higher-precision intermediate before the fixed-point quantization, or floor-compensate the EMA weight so sub-ULP dim contributions aren't silently dropped.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
