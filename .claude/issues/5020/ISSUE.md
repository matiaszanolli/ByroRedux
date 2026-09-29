# REN-D2-2026-09-29-01: #4942's ratio accumulator is a single whole-reservoir ratio — mixed-visibility clusters still low-pass flicker; fixture covers one light

**Labels**: low,bug,renderer,shaders,test-gap

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-29.md`
**Severity**: LOW
**Dimension**: Ray Queries / Light Animation
**Location**: `crates/renderer/shaders/triangle.frag`, ReSTIR finalize (`ratioFrame = … frameContribution / max(restirUnshadowedSum, vec3(1e-6)) …`, `accum = mix(prevAccum, ratioFrame, alpha)`, `Lo += restirUnshadowedSum * accum`) and the comment "(flicker, pulse, toggles) reaches the pixel undelayed". Test: `direct_history_accumulates_a_shadow_ratio_not_radiance` (`crates/renderer/src/vulkan/restir.rs`).

## Description
Residual of CLOSED #4942. The accumulated quantity is one ratio, Σrad·V / Σrad, for all of the fragment's streamed lights. Heitz 2018 applies the ratio per light. When one light's intensity animates, the true ratio changes with it, and the EMA'd ratio lags.

## Evidence
Light A visible, flickering at 12 Hz with intensity 1 ± 0.5; light B equal intensity, fully occluded.
- Truth: A(t) ∈ [0.5, 1.5].
- Parked camera (α = 0.025): the EMA ratio settles near mean(A/(A+1)) ≈ 0.48, so output (A+1)·0.48 ∈ [0.72, 1.20] — ~48 % of authored amplitude (vs ~2 % before #4942).
- A light switched off next to an occluded one still leaves a lit ratio for the EMA time constant.

The test mirrors one light under constant shadow, and the shader comment holds only when all streamed lights share a visibility.

## Impact
Torch flicker still lags in mixed-visibility clusters (far less than before #4942). Visual only.

## Related
#4942 (closed), #4940, REN-D7-2026-09-27-01.

## Suggested Fix
Soften the comment to state the limit and add a two-light, mixed-visibility fixture. A per-light ratio would need per-light history; weigh it against the 32-B reservoir.

Validated at HEAD 9fcfdc3fc: `triangle.frag` accumulates a single `ratioFrame` over `restirUnshadowedSum` and multiplies `Lo += restirUnshadowedSum * accum`; the "reaches the pixel undelayed" comment is present.

## Completeness Checks
- [ ] **SIBLING**: other ReSTIR/SVGF history paths that aggregate across lights
- [ ] **TESTS**: A two-light mixed-visibility fixture pins the documented behaviour
