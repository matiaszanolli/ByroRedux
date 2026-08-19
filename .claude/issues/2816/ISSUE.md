# REN-D18-02: the Skyrim DALC ambient cube is excluded from the WTHR cross-fade blend

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` — `weather_system`, the `let dalc_cube = …` binding between `drop(wd)` and the `SkyParamsRes` write; the blend it is missing from is the `if transition_t > 0.0 { … }` tuple above
- **Status**: NEW (raised in the quarantined 11:44 pass; never filed, **re-verified still present at `e4ab12e8`**)
- **Description**: `weather_system` blends ten quantities across an in-flight `WeatherTransitionRes` cross-fade (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far, fog_medium). The eleventh per-weather field written into `SkyParamsRes` — `current_dalc_cube` — is computed **outside** that blend: it re-reads the live `WeatherDataRes` (the *source* weather only) and never samples `tr.target.skyrim_dalc_per_tod`. The target's cube therefore arrives as a single-frame snap when `promote_weather_transition_target` runs at `t >= 1.0`, instead of easing in over the 8-second fade.
- **Evidence**: the binding reads `world.try_resource::<WeatherDataRes>()` → `.skyrim_dalc_per_tod` → `DalcCubeYup::lerp(&cubes[fold(slot_a)], &cubes[fold(slot_b)], t)`; no `tr.target` read occurs anywhere in it. The field has a live consumer — `byroredux/src/render/sky.rs::build_sky_params` does `interior_cube.or_else(|| sky_res.current_dalc_cube.map(renderer_dalc_cube))`. Contrast `promote_weather_transition_target`, which *does* promote `skyrim_dalc_per_tod` (#1102) — so the field is understood to be per-weather, just not blended.

  Confirmed against current code (`byroredux/src/systems/weather.rs`):
  ```rust
  let dalc_cube = world
      .try_resource::<WeatherDataRes>()
      .and_then(|wd| wd.skyrim_dalc_per_tod)
      .map(|cubes| {
          use byroredux_plugin::esm::records::weather::*;
          let fold = |slot: usize| match slot {
              TOD_HIGH_NOON => TOD_DAY,
              TOD_MIDNIGHT => TOD_NIGHT,
              s => s,
          };
          crate::components::DalcCubeYup::lerp(&cubes[fold(slot_a)], &cubes[fold(slot_b)], t)
      });
  ```
  — sits after `drop(wd)`, outside the `if transition_t > 0.0` cross-fade block, and never touches `tr.target`.
- **Impact**: Skyrim-only (`skyrim_dalc_per_tod` is `None` on FNV/FO3/Oblivion). Across any worldspace change between two Skyrim weathers with differing DALC cubes, the six-axis ambient cube holds the old weather's value for the whole fade and pops on the completion frame while every other sky/lighting quantity eases. Visual only.
- **Related**: same asymmetry class as #1018 (target night-factor for fog distance) and #1101 / #1102 (wind + DALC promotion).
- **Suggested Fix**: Inside the existing `if transition_t > 0.0` block, sample `tr.target.skyrim_dalc_per_tod` at the target's own `(b_a, b_b, b_t)` slots and `DalcCubeYup::lerp` against the source cube by `transition_t`, mirroring the `target_fog_*` treatment directly above. Decide the `Some`/`None` mismatch explicitly — source-with-DALC → target-without should fade out, not snap.

## Completeness Checks
- [ ] **SIBLING**: Same blend treatment applied to `current_dalc_cube` as the other nine quantities in `weather_system`'s cross-fade block (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far, fog_medium)
- [ ] **TESTS**: A regression test pins the DALC cube easing across a Skyrim→Skyrim weather transition with differing `skyrim_dalc_per_tod`, not just the instantaneous-snap case

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D18-02).
