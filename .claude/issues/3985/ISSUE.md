# #3985 — REN-2026-09-06-D18-01: an authored-still cloud layer scrolls — `cloud_scroll_vectors` uses a zero vector as its "no data" sentinel, and its fallback ignores the authored wind direction the same record supplies

**Labels**: medium, renderer, shaders, terrain-exterior, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D18-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` (`cloud_scroll_vectors`, `cloud_scroll_rate_from_wind`, `weather_system`'s `CloudSimState` block); source: `crates/plugin/src/esm/records/weather.rs` (`b"ONAM"`, `b"RNAM"`, `b"QNAM"` arms, `WeatherRecord::cloud_layer_velocities`)
- **Status**: NEW
- **Description**: Two independent problems in one helper.

  **(a) Authored zero is indistinguishable from absent.** `cloud_scroll_vectors`
  decides per layer:

  ```rust
  if velocities[layer][0].abs() > 1.0e-5 || velocities[layer][1].abs() > 1.0e-5 {
      result[layer] = [velocities[layer][0] * 0.16, velocities[layer][1] * 0.16];
  }
  ```

  falling back otherwise to a wind-derived synthetic vector. `WeatherRecord::
  cloud_layer_velocities` defaults to `[[0; 2]; 4]` and is filled by the ONAM
  (FO3/FNV, one byte per layer) and RNAM/QNAM (Skyrim, the two components) arms.
  A record that authors `0` for a layer — a deliberately motionless deck — lands
  on the exact value that means "this record shipped no motion sub-record at
  all", and the engine substitutes motion the artist did not ask for. There is
  no presence flag on the struct to tell the two apart. This is the same
  presence-vs-value sentinel class as the `[1,1,1]` specular default that
  produced the chrome-flyer bug (#1873).

  **(b) The fallback drops the authored wind direction.** The fallback is a fixed
  sign/ratio table scaled only by speed:

  ```rust
  let fallback = [
      [fallback_rate, fallback_rate * 0.3],
      [-fallback_rate * 1.35, fallback_rate * 0.5],
      [fallback_rate * 0.85, fallback_rate * 0.45],
      [-fallback_rate * 1.15, fallback_rate * 0.6],
  ];
  ```

  `fallback_rate` is `cloud_scroll_rate_from_wind(wd.wind_speed)` — speed only.
  The authored WTHR wind *direction* (DATA `WTHR_WIND_DIRECTION_OFFSET`, turned
  into `[cos θ, sin θ]` by `byroredux/src/env_translate.rs`) never reaches this
  function, so the four textured decks always drift on the same hardcoded
  headings regardless of the weather's authored wind.
- **Evidence**:
  - `crates/renderer/shaders/composite.frag` proves the direction *is* plumbed and
    consumed elsewhere: `weather_procedural_cloud` computes
    `vec2 drift = wind * time * (0.0012 + wind_speed * 0.0065)` from
    `params.weather_wind.xz`, and the precipitation term uses the same vector for
    `rain_drift`. So in a single frame the procedural cloud body drifts along the
    authored wind while the four authored WTHR layers composited on top of it
    drift along `[+x, -x, +x, -x]` constants — visibly crossing on any weather
    whose wind is not roughly +X.
  - `crates/plugin/src/esm/records/weather.rs`, `b"ONAM"` arm:
    `record.cloud_layer_velocities[layer] = [sub.data[layer], 0]` — one authored
    byte, zero Y. A `0` byte is a legal authored value and reaches the sentinel
    branch.
  - `advance_cloud_scroll` is applied to all four layers in `weather_system`, so
    every layer is affected.
- **Impact**: Per-weather, exterior-only, visual. On Oblivion/FO3/FNV (ONAM,
  single scalar) any layer authored at rest drifts; on Skyrim (RNAM/QNAM) the
  same holds per component pair. Independently, every fallback layer on every
  game ignores the record's own wind heading, so cloud drift and both
  precipitation and the procedural cloud body disagree on wind direction.
- **Related**: #1033 (`WIND_TO_SCROLL_RATE` calibration), #529 (cloud tile
  scale), `feedback_chrome_means_missing_textures` / #1873 (the same
  authored-value-vs-struct-default sentinel class).
- **Suggested Fix**: Carry presence explicitly — e.g. make
  `WeatherRecord::cloud_layer_velocities` an `Option<[[u8; 2]; 4]>` set by the
  ONAM/RNAM/QNAM arms, so `cloud_scroll_vectors` branches on "the record
  authored motion data" rather than on the value. Separately, pass the authored
  `wind_direction` into the fallback and rotate the per-layer ratio table by it,
  keeping the magnitudes as the documented `WIND_TO_SCROLL_RATE` calibration.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
