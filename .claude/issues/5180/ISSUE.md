# #5180: EXT-D4-2026-10-02-03: `composite_does_not_carry_its_own_copy_of_the_sky` names two retired functions and never names `sky_radiance` / `cloud_march`

**Labels**: low,terrain-exterior,shaders,test-gap,bug
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (test hygiene; the guard is partially vacuous)
- **Dimension**: Sky, weather, sun
- **Location**: `crates/renderer/src/vulkan/sky_dome.rs:117-135`
- **Status**: NEW. No prior report names it.
- **Tier Violated**: n/a (guard integrity)
- **Game Affected**: n/a
- **Description**:
  - The audit skill cites this guard as the "one `cloud_march` for background and bake" pin.
  - Two of its four forbidden needles no longer exist anywhere: `vec3 compute_sky(` and `vec4 weather_procedural_cloud(`. Both were retired when 564d0d2fe (2026-09-13) made the entry point `sky_radiance` (`include/sky.glsl:195`) and the cloud body `cloud_march` (`include/clouds.glsl:274`).
  - Neither live name is checked, and `sky_cube.comp` (the other consumer) is not scanned at all.
  - Mitigation: a same-name re-declaration alongside the `#include` is a GLSL redefinition error, so the guard's effective teeth are only the `#include "include/sky.glsl"` assertion. A renamed fork in `composite.frag` or `sky_cube.comp` passes both the compiler and the guard.
- **Evidence**:
  - `sky_dome.rs:124-129` lists `compute_sky(`, `weather_procedural_cloud(`, `weather_sky_details(`, `weather_star_field(`.
  - `grep -rn "compute_sky\|weather_procedural_cloud" crates/renderer/shaders` returns nothing.
  - `git log -S "weather_procedural_cloud("` → 564d0d2fe.
- **Impact**:
  - The single-implementation claim rests on the include line alone.
  - A copy of `cloud_march`/`sky_radiance` under a new name in either consumer would ship green.
- **Suggested Fix**:
  - Derive the forbidden set from the function declarations in `sky.glsl` + `clouds.glsl`: parse `^(vec[234]|float|bool|void) name(`.
  - Assert that `composite.frag` and `sky_cube.comp` each include `sky.glsl` and declare none of those names.
  - Also assert that `cloud_march(` is called only from `sky.glsl`.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
