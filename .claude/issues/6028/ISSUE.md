# EXT-D1-2026-10-18-02: `apply_forced_weather` cross-fades over 8 s although `ForceWeather` is immediate; `ForcedWeather.locked` is stored and never read; the Show-Sky interior path ignores a forced weather

Labels: low,terrain-exterior,scripting,bug,game:oblivion,game:fo3,game:fnv
Source: docs/audits/AUDIT_EXTERIOR_2026-10-18.md

- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Location**: `byroredux/src/scene/world_setup.rs:268-296` (doc says "switch ... at once"), `:451` (`duration_secs: 8.0` via `take_weather_crossfade_eligibility`), `:238-262` (`apply_environment` reads `ForcedWeather`), `:529-567` (`apply_show_sky_interior_climate` does not); `crates/scripting/src/obscript_state.rs:101-106` (`locked`).
- **Status**: NEW (`d1b298921`)
- **Tier Violated**: no-leak (the `locked` flag)
- **Game Affected**: FO3, FNV, Oblivion (ObScript)
- **Description**:
  - GECK: `ForceWeather` "forces the weather to immediately switch"; with `OverrideWeatherFlag` 0 "the weather will change back in the normal fashion". The install path reuses the worldspace-change cross-fade, so the switch takes 8 s. The function's own doc says "at once".
  - `ForcedWeather { weather, locked }` is written by the effect (`obscript_effects.rs:517`) but `locked` has no reader outside tests; the engine treats every force as a lock. The engine has no regional weather re-roll, so the unlocked case is currently indistinguishable; the field is still a parsed-but-unconsumed value.
  - `apply_forced_weather` records the first look without applying (`previous.is_none()`). A force issued by a script between `begin_exterior_streaming` and the first `step_streaming` call is recorded as applied and never installed.
- **Impact**: A scripted weather change fades in over 8 s instead of cutting. A Show-Sky interior shows its XCCM default weather even while a script holds another. The bootstrap race is a one-frame window.
- **Suggested Fix**: Snap (collapse the transition to the target) when the install is script-forced; read `locked` or drop it; have `apply_show_sky_interior_climate` take the same `forced.or(default)` rule through a shared helper.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
