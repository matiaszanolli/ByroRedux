# EXT-D3-2026-10-26-02: "Moment-matched" blade-length draw is not moment-matched after the clamp; ~5.6 % of blades pin at the species maximum

Labels: low,terrain-exterior,shaders,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all (built-in `DEFAULT_TEMPERATE` / `DEFAULT_ARID`)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert` `byroGcBladeLength` (~l.239-261); `crates/core/src/ecs/components/groundcover.rs` `moment_matched_length` doc; `docs/engine/exal-groundcover.md` 12.16
- **Status**: NEW (7430cc14c, issue #6201 still open)
- **Description**: the doc and the field comment say the built-ins carry "exactly the first two moments of the uniform draw it replaces, so only the shape changes". That holds for the
  untruncated log-normal. The shader clamps to `height_range`, which is only +-1.73 sigma of the matched distribution, so the clamp changes the moments and creates two point masses.
- **Evidence**: numeric replay of the shader's formula (Box-Muller + log-normal + clamp, 400k draws): temperate (9, 21): mean 14.89 (target 15.0), sigma 3.13 (target 3.46, -9.5 %), 5.6 % of blades exactly 21 and 1.7 % exactly 9;
  arid (6, 13.5): mean 9.68 (9.75), sigma 1.96 (2.17), 5.6 % / 1.8 % at the clamps.
- **Impact**: a few percent of blades share one identical (maximum) height, a faint flat-top; the stated "no number invented, spread unchanged" provenance claim is slightly wrong. Cosmetic.
- **Related**: #6201.
- **Suggested Fix**: either correct the wording to "the moments of the unclamped draw; the clamp trims ~6 % to the limits", or resample/rescale the cv so the clamped moments match (or reflect rather than clamp).

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
