# #5503: NIFAL-D8-2026-10-09-02: #5283 forwards Starfield's physical `LuminousEmittance` raw into `emissive_mult` under the `Lighting` tag, with no §4 measurement, drops `AdaptiveEmittance` / `ExposureOffset`, and never captures the dominant…

**Labels**: bug, game:starfield, medium, nifal, renderer

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-09.md` — finding `NIFAL-D8-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. This is a divergent canonical `Material` field across games. It is latent today, because the arm
  fires on 0 referenced materials (D8-01). It becomes live as soon as D8-01 is fixed.
- **Dimension**: Shader-flags/Effects → Material
- **Tier Violated**: no-fabrication (an unsourced scale claim) + parked-not-leak (the dominant emissive component is
  uncaptured)
- **Game Affected**: Starfield (and, by tag, every consumer that reads `EmissiveSource`)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:352-386`, in particular `:361-362`, the "consumes linear luminance
    natively" comment;
  - `crates/sfmaterial/src/index.rs:521-539` (captures only `Enabled`, `EmissiveTint` and `LuminousEmittance`);
  - `docs/engine/nifal.md:945-1000` (§4 has no Starfield row) and `:718`;
  - `crates/renderer/shaders/triangle.frag:1692` (`min(emissiveColor * emissiveMult * emissiveMask, vec3(64.0))`).
- **Status**: NEW (an incomplete fix of #5283).
- **Description**:
  - §4 measured three emissive sources: FNV `Material` mode 1.0, Skyrim `Lighting` mode 1.0, FO4 `Lighting` mode 0.05. It
    concluded that "no source is systematically offset from the others by a fixed factor".
  - #5283 adds a fourth source under the existing `Lighting` tag without measuring it. Starfield's emittance is a physical
    luminance, offset by orders of magnitude.
  - The comment asserts the HDR pipeline "consumes linear luminance natively", but the rest of every scene is Gamebryo
    monitor-space radiance (*feedback_color_space*). The only scale decision left is the shader's 64 ceiling, which §4
    already records as a render-time material decision doing the canonical tier's work.
  - `EmittanceSettings` carries `AdaptiveEmittance` and `ExposureOffset`. Starfield authors them right beside the luminance
    (for example `requisitionkiosk_splashscreen_ryujin.mat`: `AdaptiveEmittance "true"`, `ExposureOffset "6"`,
    `LuminousEmittance "100"`). Both are dropped.
  - Separately, `BSMaterial::LayeredEmissivityComponent` has 32,817 instances, against 1,058 for `EmissiveSettingsComponent`.
    It has no capture at all.
- **Evidence**:
  - `EmissiveSettingsComponent` `LuminousEmittance`: enabled instances run from 50 to 5,199.2 (mode 50, p90 1,362.9); across
    all instances, p50 is 200 and the maximum 30,000.
  - The 215 `ColorEmissive`-inheriting referenced roots author luminances of 100–500.
  - `LayeredEmissivityComponent` `LuminousEmittance`: on 31,759 instances, mode 55.7299 and maximum 120,000. With `Enabled`
    true, p50 is 437 and the maximum 8,192. `AdaptiveEmittance` is true on 811 instances; `ExposureOffset` is present on
    1,820.
- **Impact**:
  - Once D8-01 resolves inheritance, about 215 or more referenced emissive materials would carry `emissive_mult` of 100–500.
    That is 100× to 10,000× the other games' modes, and every bright texel would saturate at the 64 ceiling.
  - Any later per-source emissive policy keyed on `EmissiveSource::Lighting` would also mix Skyrim/FO4 multipliers with
    Starfield nits.
- **Related**: #5283 (closed), D8-01, D8-03, `nifal.md` §4 (Q2 resolved as a no-op, on measurement).
- **Suggested Fix**:
  - Census Starfield emittance with the §4 tooling, in a row of its own.
  - Decide the physical-luminance → canonical mapping from that measurement (or keep the arm parked), and either give
    Starfield its own `EmissiveSource` or document the conversion.
  - Delete the "natively" sentence unless it can be sourced.
  - Treat `LayeredEmissivityComponent` as the primary capture target, ahead of `EmissiveSettingsComponent`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary (`translate_material` / `Material::resolve_pbr`) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
