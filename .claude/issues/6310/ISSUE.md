# #6310: EXT-D4-2026-10-21-04: The uniform overcast term is faded out with the map edge, so overcast ground gets the full above-cloud sun below about 3° of sun elevation

**Labels**: low,terrain-exterior,shaders,bug

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: LOW · **Dimension**: Sky, weather, sun · **Location**: `crates/renderer/shaders/include/bindings.glsl (exteriorCloudSunTransmittance)`

- **Tier Violated**: n/a
- **Game Affected**: all exteriors under overcast-class weathers (coverage > 0.40), at dawn and dusk
- **Location**: `crates/renderer/shaders/include/bindings.glsl:417-429`; `crates/renderer/shaders/cloud_shadow.comp:60-66`; `crates/renderer/shaders/include/cloud_shadow.glsl:25-31`
- **Status**: NEW (interaction of `c231f5b5f`'s edge fade with `b6d6f8320`'s stratiform term)
- **Description**: `b6d6f8320` folds the broad stratiform extinction (`exp(-2.5 * (c - 0.40)/0.60)`, about 8% at full cover) into every texel of the shadow map. `exteriorCloudSunTransmittance` fades the lookup to 1.0 across the map's outer tenth, which skyal.md documents as deliberate for the local deck. Each receiver looks up the point where its sun ray crosses the deck base, offset by `CLOUD_LAYER_BOTTOM / tan(elevation)` = 1500 m / tan(el). That offset passes the 25.6 km fade start at el ≈ 3.3° and the map edge at about 2.7°, earlier for receivers below the viewer. Below that elevation, the uniform overcast term disappears along with the local shadows. The camera-column value used by the froxels and caustics keeps it, so godrays stay dark while the ground lights up. `cloud_shadow.glsl`'s comment ("the result then lands past the map edge, whose texel the clamped sampler returns") also contradicts the fade.
- **Evidence**: The constants `CLOUD_SHADOW_MAP_EXTENT` 64000, `CLOUD_LAYER_BOTTOM` 1500 and the 0.1 fade in `bindings.glsl:426-428`. `compute_sun_arc` puts el ≈ 3.3° about 18 game-minutes after sunrise begins, at about 6% of peak intensity.
- **Impact**: For roughly 18 game-minutes at each end of the day under overcast weather, the terrain's direct sun is about 12× the overcast value. Absolute light is low (about 6% of peak), so this is a dawn/dusk brightening step rather than a blowout.
- **Related**: skyal.md §2.5 ("Beyond the map's edge … fades to unshadowed"; "Overcast cover (2026-10-20)").
- **Suggested Fix**: Keep the uniform term out of the map's texels and apply it in `exteriorCloudSunTransmittance` (and the camera column) outside the edge fade, or fade toward the uniform term instead of 1.0. Correct the `cloud_shadow.glsl` comment either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
