# #5506: REN-D11-2026-10-09-01: #5482's shadow toe still crosses zero whenever the authored contrast exceeds `1 + TOE/PIVOT` (≈ 1.556) — 31 vanilla image spaces re-crush their deepest shades to black

**Labels**: bug, medium, renderer, shaders, test-gap

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-09.md` — finding `REN-D11-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: FSR/Presentation
- **Location**:
  - `crates/renderer/shaders/presentation.frag`, `gradeContrast`.
  - Its mirror `crates/renderer/src/tonemap.rs`, `grade_contrast`.
  - The constants `GRADE_CONTRAST_PIVOT` / `GRADE_CONTRAST_TOE` in `crates/renderer/src/shader_constants_data.rs`.
- **Status**: NEW. It is the residual of #5482, which is CLOSED by `3bcf6c8e8`.
- **Description**:
  - The blend is `mix(toe, stretched, w)` with `w = clamp(toe / TOE, 0, 1)`, which works out to `toe · (1 − (toe − stretched) / TOE)`.
  - As `x → 0`, `toe → 0` and `stretched → PIVOT·(1 − c)`, so `toe − stretched → PIVOT·(c − 1)`.
  - When `PIVOT·(c − 1) > TOE`, i.e. `c > 1.556`, the factor goes negative: the curve dips below zero and both tonemappers floor it to black.
  - The doc comments ("a curve that approaches black instead of crossing it") and the test's claim ("keeps every positive shade positive and the ramp strictly increasing") hold only for `c ≤ 1.556`.
  - `contrast_keeps_shadows_above_black_and_monotone` pins `c = 1.3` alone.
- **Evidence**:
  - A Python mirror of `grade_contrast` gives the exposed-radiance band that still presents as exactly zero, against the pre-fix floor `0.18·(1 − 1/c)`:
    - `c = 1.6`: 0.005 (pre-fix 0.068)
    - `c = 1.7`: 0.017 (pre-fix 0.074)
    - `c = 1.9`: 0.037 (pre-fix 0.085)
    - `c = 2.0`: 0.046 (pre-fix 0.090)
  - The curve is non-monotone for every `c > 1.556`.
  - A census of IMGS cinematic contrast in the vanilla masters (FO3/FNV `DNAM`, Skyrim/FO4 `CNAM`/`ENAM`, using the parser's own float indices) found these records above 1.556:

    | Master | Above 1.556 | Records |
    |---|---|---|
    | FalloutNV.esm | 9 of 67 | `UltraLuxeCasino` 1.9, `CaveTestBrightImageSpace` 1.9, `UnderworldImageSpace` / `CitadelLabImageSpace` / `CaveLLImageSpace` 1.7, four Metro/Urban sets 1.6 |
    | Fallout3.esm | 8 of 48 | `CitadelLabImageSpace` 1.7, `UnderworldImageSpace` 1.7, Metro/Urban 1.6 |
    | Skyrim.esm (SE) | 11 of 270 | `FrostmereCryptImagespace` 2.0, `DA16DreamImageSpace` 1.8, `ISSkyrimOvercastWarDAY` and `ISSkyrimStormSnowDAWN` 1.7, the storm-rain night set 1.65 |
    | Fallout4.esm | 3 of 293 | `ISWorldMapWeatherNIGHT` 1.65, `DiamondCityPastelDUSK` and `IstIS01` 1.6 |

  - IMAD contrast keys compose on top of these values (`value = base × mult + add`) and were not censused, so this count is a lower bound.
- **Impact**:
  - At `c = 1.9–2.0` (the Ultra-Luxe casino and Frostmere Crypt), every shade below about 0.04 exposed presents as exactly zero.
  - That is the same 0.03–0.04 ambient-only band #5482 set out to rescue. Those shades would have shown at roughly 0–15/255 on the toe.
  - At 1.6–1.7 (the FO3 DC Metro sets, Skyrim storm weathers) the residual band is small (0–3/255).
  - The effect is visual only. It reintroduces the "black holes" symptom in exactly the darkest-graded interiors and weathers.
- **Related**: #5482 (closed), #4840 (the ACES negative floor that turns the dip into black).
- **Suggested Fix**:
  - Weight the hand-over on the *stretched* value: `w = clamp(stretched / TOE, 0, 1)`.
  - `stretched ≤ 0` then returns the strictly positive toe, and the result always lies between two non-negative values. A mirror check over `c ∈ {0.5 … 2.0}` is positive and monotone across that range.
  - Extend the Rust test to the census range (`c` up to 2.0) instead of 1.3 alone.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
