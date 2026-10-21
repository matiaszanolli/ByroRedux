# #6244: EXT-D4-2026-10-21-01: The effect-lit sun path in `triangle.frag` misses the cloud-shadow transmittance; lit effect surfaces get clear-sky sun under overcast

**Labels**: medium,terrain-exterior,shaders,bug

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Sky, weather, sun (direct-sun consumer completeness) · **Location**: `crates/renderer/shaders/triangle.frag (MAT_FLAG_EFFECT_LIT branch)`

- **Tier Violated**: n/a (consumer contract of the SKYAL cloud-shadow map)
- **Game Affected**: all exterior games. Skyrim and FO4 author `Effect_Lighting` on `BSEffectShaderProperty` (spell FX, waterfall spray, mist cards).
- **Location**: `crates/renderer/shaders/triangle.frag:1135-1150`
- **Status**: NEW (regression introduced by `c231f5b5f`)
- **Description**: `c231f5b5f` removed the weather-wide `exp(-2.5 * cloud_coverage)` factor from `compute_directional_upload` (`byroredux/src/render/mod.rs`). The exterior directional upload is now "the sun above the clouds", and each direct-sun site was moved to multiply by `exteriorCloudSunTransmittance` (in `lighting.glsl`, grass and the water glint). The `MAT_FLAG_EFFECT_LIT` branch is a separate one-light shading pass. It scans for the first directional light and adds `color_type.rgb * NdotL * surf` with no cloud term:
  ```glsl
  if (uint(lights[lIdx].color_type.w) == 2u) {
      vec3 Ldir = normalize(lights[lIdx].direction_angle.xyz);
      float NdotL = max(dot(N, Ldir), 0.0);
      lit += lights[lIdx].color_type.rgb * NdotL * surf;
      break;
  }
  ```
  Before the change, this site inherited the coverage dimming through the upload. Now it gets the undimmed sun.
- **Evidence**: The directional-upload diff in `c231f5b5f` (`render/mod.rs`, `render/lights.rs`). `every_direct_sun_site_applies_the_cloud_shadow` (`crates/renderer/src/vulkan/sky_cube.rs`) asserts literal strings in `lighting.glsl`, `groundcover_blade.frag`, `water.frag`, `caustic_splat.comp` and `post_passes.rs` only, and never scans `triangle.frag`. A grep of `direction_angle` over the shaders finds no other unconverted directional site. The main-loop arm at `triangle.frag:3393` sets `atten = 1.0`, but it only feeds the contribution pre-gate (an upper bound); its radiance comes from `shadowableLightRadiance`.
- **Impact**: Under a storm weather (coverage 0.86) the old key was `exp(-2.15)` ≈ 0.12. Lit effect surfaces are now about 8× brighter than before and than the sunlit ground beside them, which is dimmed by the map. The visible effect is glowing mist and spray, and bright spell FX on dark overcast days.
- **Related**: skyal.md §2.5 ("every direct-sun site multiplies by the map"); EXT-D4-2026-10-21-03.
- **Suggested Fix**: Multiply the branch's contribution by `exteriorCloudSunTransmittance(fragWorldPos, Ldir)`, which already returns 1.0 in interiors. Extend the guard so it derives the site list instead of enumerating it, for example by failing on any `color_type.w) == 2u` / `lightType >= 1.5` arm that reads `.rgb` without the call.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
