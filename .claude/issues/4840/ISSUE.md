# #4840: REN-D11-2026-09-24-01: `aces()` is fed negative values by the grade stage — with authored image-space contrast > 1 (now decoded, `afd6a73f7`) the darkest pixels render grey instead of black

**Labels**: bug,renderer,medium,game:fnv,game:fo3,shaders

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-01**._

- **Severity**: MEDIUM. Visual artifact on the default display transform. Blast radius argues for the top of the band (58 of 67 FNV and 41 of 48 FO3 `IMGS` records author contrast > 1.0, max 1.9). HIGH is defensible; the merger's call.
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/presentation.frag` — `main` (grade block: `graded = (graded - vec3(0.18)) * max(params.grade.z, 0.0) + vec3(0.18); … tonemap(graded * exposure)`) and `aces()`; mirror `crates/renderer/src/tonemap.rs` `aces`.
- **Status**: NEW (trigger `afd6a73f7`, 2026-09-24; the ACES + grade code itself is older). The orchestrator confirmed that the grade block has no clamp before `tonemap`.
- **Description**: The grade pivots contrast around 0.18: `graded = c*(s - 0.18) + 0.18`, which is negative for `s < 0.18*(1 - 1/c)` (0.042 at c = 1.3, 0.085 at c = 1.9) and exactly `0.18*(1-c)` for a black pixel. Nothing clamps it before the display transform. The Narkowicz fit is not sign-safe: its numerator `x*(2.51x + 0.03)` has a second root at x = -0.012, so for x < -0.012 the output is *positive* (reaching 1.0 by x ≈ -0.3), and the denominator never vanishes, so there is a wrong-sign lobe, not a pole. The output-side `clamp(…, 0, 1)` only catches negative results. Before `afd6a73f7` the base grade was the identity and only rare IMAD modifiers could reach this; now every interior `XCIM` and every FO3/FNV exterior `INAM` supplies the grade. Saturation > 1 (8 FNV / 2 FO3 records, max 1.45) can also drive a minor channel negative. AgX is immune (`max(val, 1.0e-10)` before `log2`).
- **Evidence** (independent evaluation of the shader coefficients at exposure 0.85, sRGB8 output):

  | contrast | scene s | ACES linear | sRGB8 |
  |---|---|---|---|
  | 1.3 | 0.00 | 0.0331 | 51 |
  | 1.3 | 0.01 | 0.0164 | 34 |
  | 1.3 | 0.03 | 0.0002 | 1 |
  | 1.6 | 0.00 | 0.1731 | 116 |
  | 1.9 | 0.00 | 0.4146 | 172 |

  The response is non-monotonic: at c = 1.3 a pixel at s = 0 renders brighter (51) than one at s = 0.03 (1), so deep shadows are lifted and ringed by a dark contour. Census (decoding `DNAM` by the parser's documented layout): FNV 67 IMGS, contrast 0.8..1.9, 58 above 1.0 (top: `UltraLuxeCasino` 1.9, `CaveTestBrightImageSpace` 1.9, `UnderworldImageSpace` 1.7); FO3 48 IMGS, 41 above 1.0. Transparent-black interior clears (`0572bfd5a`) make exact-zero pixels more common. The Rust mirror has the same defect and no test feeds it a negative input.
- **Impact**: Cells with authored contrast > 1 (most of them) show lifted, haloed shadows and a washed black level on the default operator; `tonemap agx` does not show it. Visual only.
- **Suggested Fix**: Floor the graded value at zero before the display transform (or inside `aces()`), as AgX already does, and pin it where `cargo test` can see it: a negative-input assertion (output 0, monotone across the sign change) on `tonemap::aces`, plus a source pin that the GLSL carries the same floor. Shader math only, no pipeline or barrier change; recompile `presentation.frag.spv` and re-run `scripts/check-shader-artifacts.sh`. Whether the contrast pivot should be exposure-relative is a separate design question. One screenshot of an `IMGS` contrast > 1 interior (FNV `UltraLuxeCasino`) would put a visual on it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

