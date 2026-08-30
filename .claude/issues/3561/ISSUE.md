# #3561 — REN-2026-08-30-D18-01: `--cornell-sun`'s fixed directional sun is overwritten on frame 1 by `apply_neutral_exterior_fallback`, desynchronising `CellLightingRes.directional_dir` from `SkyParamsRes.sun_direction`

**Labels**: `high,renderer,terrain-exterior,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3561 --json state`.

---

- **Severity**: HIGH
- **Dimension**: Sky / weather / exterior lighting
- **Location**: `byroredux/src/systems/weather.rs` (`weather_system`, `apply_neutral_exterior_fallback`), `byroredux/src/cornell.rs` (`install_cornell_lighting`, `sun_dir`), `byroredux/src/scene.rs` (`setup_scene`)
- **Status**: NEW
- **Description**: `cornell.rs`'s module doc states the exterior harness's premise
  verbatim: *"No `WeatherDataRes` is inserted, so `weather_system` stays inert and
  the direction does not drift with TOD."* That is no longer true. `weather_system`
  is registered unconditionally (`boot.rs:765-779`, `Stage::Early`, exclusive), and
  its only early-out before the `WeatherDataRes` branch is the `GameTimeRes` guard
  at `weather.rs:436` — but `setup_scene` calls `world_setup::ensure_game_time(world)`
  at its very top, *for every scene kind*, before any `--cornell` branch
  (`scene.rs:662-665`). So on `--cornell-sun` the clock guard passes, `WeatherDataRes`
  is absent, and control reaches:

  ```rust
  let Some(wd) = world.try_resource::<WeatherDataRes>() else {
      if let Some(mut cell_lit) = world.try_resource_mut::<CellLightingRes>() {
          apply_neutral_exterior_fallback(&mut cell_lit);
      }
      return;
  };
  ```

  `apply_neutral_exterior_fallback` skips only *interior* cells
  (`weather.rs:279-281`), and `--cornell-sun` installs
  `procedural_fallback_cell_lighting(sun_dir())` with `is_interior: false`
  (`env_translate.rs:1264`, `cornell.rs:1403`). It therefore fires and does
  `*cell_lit = procedural_fallback_cell_lighting(compute_sun_arc(6.0, DEFAULT_TOD_HOURS).0)`
  — replacing the harness's authored `SUN_DIR_RAW` direction with a hardcoded
  hour-6.0 sun.
- **Evidence**:
  - `cornell.rs:285` — `const SUN_DIR_RAW: Vec3 = Vec3::new(0.6, 0.84, 0.4);`
    → `sun_dir()` = `[0.530, 0.742, 0.353]` (≈48° elevation).
  - `weather.rs:282` — `let (sun_dir, _intensity) = compute_sun_arc(6.0, DEFAULT_TOD_HOURS);`
    `DEFAULT_TOD_HOURS = FB_TOD_HOURS = [6.0, 10.0, 18.0, 22.0]` (`env_translate.rs:1254`).
    In `compute_sun_arc` (`weather.rs:121-138`), `hour == sunrise_begin` ⇒
    `solar_hour = 0` ⇒ `angle = 0` ⇒ `[cos 0, sin 0, SUN_SOUTH_TILT] = [1, 0, 0.15]`
    normalised = `[0.989, 0.0, 0.148]` — a horizon-grazing due-east sun.
  - `weather_system` `return`s at that branch **before** the `SkyParamsRes` write
    block (`weather.rs:707-722`), so `SkyParamsRes.sun_direction` keeps `sun_dir()`
    while `CellLightingRes.directional_dir` becomes the hour-6 vector.
  - The existing pin `cornell.rs::sun_variant_drives_directional_and_sky_paths`
    (`cornell.rs:2133-2166`) asserts exactly the invariant this breaks
    (*"SkyParamsRes and CellLightingRes must carry the same direction"*), but calls
    `install_cornell_lighting` directly and never runs the scheduler — it pins the
    install-time state only, so the frame-1 clobber is invisible to it.
- **Impact**: The `--cornell-sun` RT oracle — the harness whose stated purpose is
  that *"the sun is then the only light in the scene, so any sign flip / axis swap /
  dropped term in the directional chain shows up as a moved or missing shadow rather
  than a plausible-looking image"* — renders with the shading directional and the
  painted sun disc pointing ~48° apart, from frame 1 onward. Every shadow-direction
  and sun-axis conclusion drawn from that harness is measured against the wrong
  reference. It also silently substitutes the sunrise intensity ramp's geometry for
  the mid-sky vector the probe set was laid out for.
- **Suggested Fix**: Either (a) have `apply_neutral_exterior_fallback` preserve the
  installed `directional_dir` instead of rebuilding the whole `CellLightingRes` from
  a hardcoded `hour = 6.0` (it already receives `&mut CellLightingRes`; the hardcoded
  hour is also inconsistent with the live `GameTimeRes` hour this same function has
  in scope at the call site), or (b) have `install_cornell_lighting(world, true)`
  install a `WeatherDataRes` — the harness's own doc says the intent is for
  `weather_system` to be inert, and its absence is what makes it *not* inert. Extend
  `sun_variant_drives_directional_and_sky_paths` to run `weather_system(&world, 0.0)`
  before its assertions so the pin covers the live path.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D18-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
