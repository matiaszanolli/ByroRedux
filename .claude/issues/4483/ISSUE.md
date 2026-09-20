# EXT-D1-2026-09-19-02: env.health gates no water/weather canonical fields; four WaterMaterial copies are unguarded raw f32

- **ID**: EXT-D1-2026-09-19-02
- **Labels**: medium,terrain-exterior,water,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4483

**Severity**: MEDIUM · **Dimension**: EXAL boundary · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D1-2026-09-19-02)

**Location**: `byroredux/src/commands/env_health.rs:117-211` (imports only `CellLightingRes, SkyParamsRes`); unguarded copies `byroredux/src/env_translate.rs:688` (`sun_specular_power`), `:703-704` (`wave_amplitude/frequency`), `:528-535` (fog/depth/underwater colors+distances)

**Description**
`check_environment` covers `CellLightingRes` + `SkyParamsRes` + one of four `FogMedium` fields. It never inspects: the canonical `WaterMaterial` (~40 f32 fields reaching `water.frag` push constants verbatim), `WeatherDataRes::fog`, the embedded `WeatherSkyState`, or three `FogMedium` fields. `resolve_water_specular`/`resolve_water_noise_and_rain` copy `sun_specular_power`, `wave_amplitude/frequency`, fog near/far and colors verbatim from the WATR record while their neighbours (`roughness`, `opacity`, `alpha_controls`) are finite-guarded — a corrupt WATR's NaN reaches the canonical tier and the shader with no input gate anywhere on the path. (`FogMedium` itself is finite-by-construction at `fog.rs:121-127,212-227`.)

**Impact**
A single NaN field on one WATR poisons water push constants every frame with no `env: FAIL`; the exterior smoke matrix gates on `env.health` and would report PASS. Pixel-level `RenderHealthCommand` is the only backstop.

**Related**: #2368 (the command's charter); WATAL owns clamping policy — this is the gate-coverage half

**Suggested Fix**
Extend `check_environment` (or add a sibling `water.health`) to walk `WaterMaterial`'s scalars + the missing fog fields + `WeatherDataRes::fog`; alternatively finite-guard the four verbatim copy sites like their neighbours in the same functions.

## Completeness Checks
- [ ] **SIBLING**: Inventory every canonical env struct for gate coverage, not just water
- [ ] **TESTS**: A NaN-WATR fixture makes `env.health` (or `water.health`) report FAIL
