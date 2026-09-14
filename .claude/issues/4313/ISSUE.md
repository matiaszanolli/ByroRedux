# #4313: REN-2026-09-14-D18-02: `cloud_march` bounds the ray against a spherical shell but derives `height_fraction` from flat `position.y`, compressing the cloud's vertical profile toward the horizon

- **Labels**: low,renderer,shaders,terrain-exterior,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4313
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/renderer/shaders/include/clouds.glsl` (`cloud_march`, `cloud_shell_distance`)
- **Status**: NEW
- **Description**:
  - `start` and `end` come from `cloud_shell_distance`, which works in radial altitude (r = R + h on a planet of radius `CLOUD_PLANET_RADIUS`). But each sample's height is `position = dir * t; height_fraction = (position.y - BOTTOM)/(TOP - BOTTOM)`, the flat-plane height of the viewer frame.
  - For a ray with elevation `dir.y`, the flat height at the shell crossing is below the true altitude, and the gap grows as `dir.y` falls.
  - `cloud_height_gradient` is 0 at `height_fraction` 0, so the start of low rays has zero density, and the upper taper is never reached.
  - The light march mixes the two conventions: `max(position.y, 0)` is passed as the radial `height` into `cloud_shell_distance`, and `lh` uses flat `light_pos.y`.
- **Evidence**: Re-derived by solving `|t·d + R·ŷ| = R + h` for the shell crossings.
  - `dir.y = 0.16`: crossings at t ≈ 9.3 km and 30.7 km, flat y ≈ 1490 m and 4919 m. Under 2% error, and `horizon_fade` is 1.
  - `dir.y = 0.05`: t ≈ 28.7 km and 87.9 km, flat y ≈ 1436 m and 4393 m, so `height_fraction` spans [0, 0.83] instead of [0, 1]. `horizon_fade` ≈ 0.15.
  - `dir.y = 0.02`: t ≈ 60.6 km and 155.3 km, flat y ≈ 1212 m and 3106 m, so `height_fraction` spans [0, 0.46]. The first ~8% of the ray sits below the flat base.
- **Impact**:
  - Clouds in the 0.015–0.16 elevation band are drawn with a truncated, bottom-heavy vertical profile.
  - This is inherited by the composite background and by every cube texel near the horizon, including RT reflection misses toward the horizon.
  - Most of it is hidden by `horizon_fade`, and it is negligible above ~9° elevation. Visual only, no NaN.
- **Related**: `564d0d2f`, `9ac8a929`, `5d5d6ddd`.
- **Suggested Fix**:
  - Use the sample's radial altitude, `length(position + vec3(0, CLOUD_PLANET_RADIUS, 0)) - CLOUD_PLANET_RADIUS`, for both `height_fraction` and the light-march `height`/`lh`, so the density profile matches the shell the march is bounded by.
  - Verify with the skyal.md §4 screenshot harness.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
