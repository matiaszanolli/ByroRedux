**HEAD**: 4a3e89d56 · **Baseline**: [AUDIT_EXTERIOR_2026-10-19.md](AUDIT_EXTERIOR_2026-10-19.md) (HEAD `807c58890`, area-scoped, so Dims 1/2/5/6 are delta-scoped from it) and [AUDIT_EXTERIOR_2026-10-18.md](AUDIT_EXTERIOR_2026-10-18.md) (HEAD `6f44874ac`, the last full pass for Dims 3/4/7) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain: height maps, Hide Quad, MNAM landscape TXSTs), Dim 4 (sky / weather / sun: cloud-shadow map, stratiform cover, dome decode), Dim 6 (distant LOD: `lodsettings` origin, terrain-LOD texture path) · **Unchanged since baseline (skimmed, guards spot-checked)**: Dim 3 (ground cover: only fixes of earlier findings landed), Dim 5 (WATAL: the WATR SNAM decode only), Dim 7 (gates / harness: no commits)

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-21

**Method.** This pass is part of the `--preset comprehensive` suite. One auditor ran every dimension in turn, with no sub-agents and no engine launch. No Vulkan device was used. Game data was read only through the read-only `dds_mip_scan` example and BA2 name tables. Delta scope per dimension: `git log 807c58890..HEAD` (Dims 1/2/5/6) and `git log 6f44874ac..HEAD` (Dims 3/4/7) over the skill's `Paths:`. The diffs of every exterior commit in scope were read. The SKYAL cloud work dominates: `c231f5b5f` (cloud-shadow map), `fdf4bfc1e`, `b6d6f8320` (stratiform cover, mood chroma) and `ae557b935` (dome colour decode). The rest were the terrain height maps (`5017a64c2`, `ea45af067`, `2975d2906`, `5f1a88929`), Hide Quad (`79477b4a3`), FO4 MNAM landscape TXSTs (`e62e258bd`), the LTEX Havok material (`c53d87679`, part of open #6048), forced weather (`553428296`), the sky-texture flush (`35bd4b75d`), the WTHR cloud/fog decode (`f16741886`), the `lodsettings` lattice origin (`cb5368348`), the Oblivion LAND inheritance (`75b921cf4`), the FO4 arid climate (`1a2330361`), the WATR SNAM decode (`1dd217412`) and five ground-cover fixes (`21b9743e0`, `02b828fff`, `4dfb39410`, `76e4414ec`, `8b97bccdb`).

**Test state (HEAD, repo `target/`, `TMPDIR=/mnt/data/tmp`):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate terrain water lod climate weather sky groundcover fog default_weather tree_lod forced cloud` | 696 passed, 0 failed, 24 ignored |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ cloud terrain water` | 216 passed, 0 failed, 1 ignored |
| `cargo test -p byroredux-terrain-height` | 11 passed |

Not run: the `--ignored` real-data and GPU tests, and every smoke gate (`m-exteriors.sh`, `w1-water-traversal.sh`, `m34-day-night.sh`). None of the findings below was confirmed on screen. Each is a static trace through the code, with the call path cited.

## Executive Summary

**6 findings: 0 CRITICAL · 0 HIGH · 3 MEDIUM · 3 LOW.** All are NEW.

The cloud-shadow map (`c231f5b5f`) moved cloud dimming out of the directional upload and into the receivers. Three findings come from that move:

- **EXT-D4-2026-10-21-01 (MEDIUM).** The effect-lit path in `triangle.frag` (`MAT_FLAG_EFFECT_LIT`) still lights with the raw directional upload. That upload is now the sun **above** the clouds, so lit effect surfaces receive clear-sky sun under a storm deck. Before the change they got the old `exp(-2.5*coverage)` key. This is a regression from `c231f5b5f`, and the site guard lists its sites by literal string, so it cannot see this one.
- **EXT-D4-2026-10-21-03 (MEDIUM).** Caustics multiply every directional light by the camera's cloud column. In an interior, that light is the room's XCLL key, and the column is baked from the outdoor (or default) deck. Interior caustics therefore dim and flicker with weather outside. Surfaces in the same room are not affected, because they get exactly 1.0 in interiors.
- **EXT-D4-2026-10-21-04 (LOW).** The cloud-shadow lookup fades to "unshadowed" past the map edge. Since `b6d6f8320` the map also carries the uniform stratiform overcast term. At a sun elevation below about 3°, every receiver's crossing lands past the edge, so overcast ground jumps to the full above-cloud sun at dawn and dusk.

The other three:

- **EXT-D6-2026-10-21-02 (MEDIUM, FO4).** #6199 routed full-detail LAND through the MNAM/BGSM resolver. The synthesized terrain-LOD block still resolves its base LTEX from `landscape_textures`, which drops every MNAM-only LTEX. On FO4, every boundary LOD block is synth-only, and when its base is an MNAM-only LTEX the block is never spawned. The result is a gap in distant terrain around the loaded area.
- **EXT-D1-2026-10-21-05 (LOW).** A `ForceWeather` / `ReleaseWeatherOverride` issued while the player is in any interior is not applied there. Only the exterior streaming step re-installs it, so Show-Sky interiors and window portals keep the old weather until the next cell install. This is a residual of #6028.
- **EXT-D5-2026-10-21-06 (LOW).** `watal.md`'s surface-sound row still names the deleted `surface_sound` field as Oblivion-only.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| single-boundary | **Broken once.** LTEX → diffuse now has two resolvers: `LandscapeLayerTextures::texture_set` (full detail, MNAM/BGSM-aware) and `spawn_lod_block`'s direct `landscape_textures` lookup (D6-02). The other categories hold. Every environment install still goes through `apply_environment_with_sources`, and forced weather enters through the one `forced_or`. |
| no-fabrication | **Holds.** New SKYAL constants are named in `shader_constants_data.rs` and cited to skyal.md §2.4–§2.5: `CLOUD_OVERCAST_ONSET`/`_OPTICAL_DEPTH` (tied to the removed `exp(-2.5c)`), `CLOUD_STRATIFORM_*`, `CLOUD_MOOD_CHROMA_WEIGHT`, `CLOUD_SHADOW_*`. The `0.58`/`0.62` morphology mixes and the `2.5` chroma cap in `clouds.glsl` are bare, but they predate this delta or are engine art choices. Not filed. |
| no-leak | **Holds, with one defect.** The interior / exterior split is honoured by `exteriorCloudSunTransmittance` (1.0 in interiors) but not by the caustic camera-column multiply (D4-03). |
| no-render-time-fallback | **Holds.** There is no `GameKind` or game token in the sky, cloud, cloud-shadow or ground-cover GLSL. `normal_map_green` is a table-shaped `GameKind` match on the translate side. |

## Per-Category Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Terrain (full detail) | `spawn_terrain_mesh` + `LandscapeLayerTextures` | OK | OK | OK | OK |
| Terrain height | `asset_provider::terrain_height` (`run_job`, `normal_map_green`) | OK | OK (Mishkinis band documented as an engine choice) | OK | OK |
| Sky | `translate_sky`, `outdoor_sky_params` (dome decode) | OK | OK | OK | OK |
| Weather / sun | `translate_weather`, `compute_directional_upload`, cloud-shadow map | OK | OK | **D4-03** | OK |
| Direct-sun consumers | `exteriorCloudSunTransmittance` / camera column | **D4-01** (one site missed) | OK | OK | OK |
| Water | `resolve_water_material`, `water_material_from_mesh` | OK | OK | OK | OK (doc rot D5-06) |
| Ground cover | `groundcover_translate.rs`, `resolve_authored_cover` | OK | OK | OK | OK |
| Distant LOD | `terrain_lod_layout`, `resolve_lod_grid_origin`, `spawn_lod_block` | **D6-02** | OK | OK | OK |

## Findings

### EXT-D4-2026-10-21-01: The effect-lit sun path in `triangle.frag` misses the cloud-shadow transmittance; lit effect surfaces get clear-sky sun under overcast
- **Severity**: MEDIUM
- **Dimension**: Sky, weather, sun (direct-sun consumer completeness)
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

### EXT-D6-2026-10-21-02: FO4 terrain-LOD boundary blocks are dropped when their base LTEX is MNAM-only (#6199 fixed full detail but not the LOD synth path)
- **Severity**: MEDIUM
- **Dimension**: Distant LOD and trees
- **Tier Violated**: single-boundary
- **Game Affected**: Fallout 4. Skyrim landscape TXSTs all author TX00. Oblivion, FO3 and FNV take the legacy translated-quad texture.
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:925-947` (`spawn_lod_block` base-texture fallback); `crates/plugin/src/esm/cell/index.rs:443-452` (`resolve_landscape_texture_sets`)
- **Status**: NEW (residual of closed #6199, `e62e258bd`)
- **Description**: `resolve_landscape_texture_sets` removes from `landscape_textures` every LTEX whose TXST has no TX00 diffuse. That is exactly the 32 of FO4's 128 LTEXs that author only `MNAM` (`LCWDefault01Grass`, the `LNFoothills*` family, `LCobblestone01`, …). `e62e258bd` added `LandscapeLayerTextures::texture_set`, which fills these LTEXs from the BGSM chain, but wired it only into full-detail `spawn_terrain_mesh`. `spawn_lod_block` still runs:
  ```rust
  Some(ltex_id) if ltex_id != 0 => match landscape_textures.get(&ltex_id) {
      Some(path) => (resolve_texture(..), Some(path.clone())),
      None => (0, None),
  },
  ...
  if tex_handle == 0 { return None; }   // "Small World / ocean-only quad"
  ```
  The caller records the result as a real miss (`missing_blocks.insert`). On Combined games (Skyrim, FO4), every boundary block (`mask != 0`) is synth-only by design: a baked `.btr` cannot be hole-masked, and `translated_lod` is `None` because `legacy_landscape_lod_supported` is false. So a boundary block whose first base LTEX is MNAM-only never spawns.
- **Evidence**: The `e62e258bd` diff touches `terrain.rs`, `exterior.rs` and `provider.rs` only. `TerrainSpawnCtx` has one construction site (`exterior.rs:2248`), and `terrain_lod.rs:663` still passes `&index.landscape_textures`. The commit's own measurement on Commonwealth 0,1 (37 → 7 unresolved layers) shows how common MNAM-only bases are.
- **Impact**: Around the full-detail ring on FO4, the non-resident cells of affected boundary blocks have no terrain at all. Sky shows through between the loaded area and the `.btr` ring. The default radius puts this about 12 cells out. Like any recorded miss, it is not retried while the mask is unchanged (see #6092).
- **Related**: #6199 (closed), #6092 (known-miss caching), #6053 / #6094 (other terrain-LOD coverage defects).
- **Suggested Fix**: Resolve the synth block's base diffuse through the same `LandscapeLayerTextures` resolver (thread the `MaterialProvider` into `reconcile_lod_rings`), or precompute a BGSM-filled `landscape_textures` at index build so both paths read one table.

### EXT-D4-2026-10-21-03: Interior caustics dim the room's XCLL directional by the outdoor cloud column
- **Severity**: MEDIUM
- **Dimension**: Sky, weather, sun
- **Tier Violated**: no-leak (outdoor weather leaks into interior lighting)
- **Game Affected**: all games, in interiors with a directional key and a refractor or water caustic
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:544`; `crates/renderer/shaders/caustic_splat.comp:448-455`; the bake source is `crates/renderer/src/vulkan/context/frame_params.rs::build_sky_cube_params`
- **Status**: NEW (introduced by `c231f5b5f`)
- **Description**: `caustic.sun_transmittance = self.camera_cloud_sun_transmittance;` is set unconditionally. `caustic_splat.comp` multiplies every `Ltype == 2u` light's `lightReach` by `causticSun.x`. In an interior, the directional `GpuLight` is the room's XCLL key (`render/lights.rs` emits it as type 2.0). The cloud-shadow bake still runs every interior frame (PERF-D5-02). When a `SkyParamsRes` exists it bakes from `portal_outdoor_sky`, which `build_sky_params` always sets for interiors. Otherwise it bakes from the default `SkyParams`, which has the sun above the horizon and coverage 0.35, so procedural puffs still drift over the camera column. Either way the room key's caustics follow the outdoor cloud field. `exteriorCloudSunTransmittance` deliberately returns 1.0 in interiors ("their key light is the room's XCLL directional, not the sun"), so the caustic disagrees with the surfaces it lands on.
- **Evidence**: `begin_frame_recording.rs:47-53` (readback every frame), `post_passes.rs:544` (no `is_exterior` gate), `bindings.glsl:414-418` (the stated interior contract). The froxel arm is different and correct: in interiors it multiplies `portal_sun_radiance`, which really is the outdoor sun.
- **Impact**: Glass and water caustics under an interior's directional key weaken under overcast weather outside (stratiform term down to about 8%) and pulse as clouds drift over the camera column.
- **Related**: EXT-D4-2026-10-21-01; PERF-D5-02 (the bake running in interiors).
- **Suggested Fix**: Pass `1.0` for the caustic sun term when the frame is not exterior (`SkyParams::is_exterior`). If interior caustics should ever follow a portal sun, carry that as its own lane.

### EXT-D4-2026-10-21-04: The uniform overcast term is faded out with the map edge, so overcast ground gets the full above-cloud sun below about 3° of sun elevation
- **Severity**: LOW
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: all exteriors under overcast-class weathers (coverage > 0.40), at dawn and dusk
- **Location**: `crates/renderer/shaders/include/bindings.glsl:417-429`; `crates/renderer/shaders/cloud_shadow.comp:60-66`; `crates/renderer/shaders/include/cloud_shadow.glsl:25-31`
- **Status**: NEW (interaction of `c231f5b5f`'s edge fade with `b6d6f8320`'s stratiform term)
- **Description**: `b6d6f8320` folds the broad stratiform extinction (`exp(-2.5 * (c - 0.40)/0.60)`, about 8% at full cover) into every texel of the shadow map. `exteriorCloudSunTransmittance` fades the lookup to 1.0 across the map's outer tenth, which skyal.md documents as deliberate for the local deck. Each receiver looks up the point where its sun ray crosses the deck base, offset by `CLOUD_LAYER_BOTTOM / tan(elevation)` = 1500 m / tan(el). That offset passes the 25.6 km fade start at el ≈ 3.3° and the map edge at about 2.7°, earlier for receivers below the viewer. Below that elevation, the uniform overcast term disappears along with the local shadows. The camera-column value used by the froxels and caustics keeps it, so godrays stay dark while the ground lights up. `cloud_shadow.glsl`'s comment ("the result then lands past the map edge, whose texel the clamped sampler returns") also contradicts the fade.
- **Evidence**: The constants `CLOUD_SHADOW_MAP_EXTENT` 64000, `CLOUD_LAYER_BOTTOM` 1500 and the 0.1 fade in `bindings.glsl:426-428`. `compute_sun_arc` puts el ≈ 3.3° about 18 game-minutes after sunrise begins, at about 6% of peak intensity.
- **Impact**: For roughly 18 game-minutes at each end of the day under overcast weather, the terrain's direct sun is about 12× the overcast value. Absolute light is low (about 6% of peak), so this is a dawn/dusk brightening step rather than a blowout.
- **Related**: skyal.md §2.5 ("Beyond the map's edge … fades to unshadowed"; "Overcast cover (2026-10-20)").
- **Suggested Fix**: Keep the uniform term out of the map's texels and apply it in `exteriorCloudSunTransmittance` (and the camera column) outside the edge fade, or fade toward the uniform term instead of 1.0. Correct the `cloud_shadow.glsl` comment either way.

### EXT-D1-2026-10-21-05: A script-forced weather change made while the player is in an interior never reaches that interior's sky
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: n/a
- **Game Affected**: Oblivion, FO3, FNV (ObScript `ForceWeather` / `SetWeather` / `ReleaseWeatherOverride`)
- **Location**: `byroredux/src/app_step.rs:114` (the only `apply_forced_weather` call, inside `step_streaming`); `byroredux/src/cell_loader/load.rs:744` (Show-Sky install, at load only)
- **Status**: NEW (residual of closed #6028, `553428296`)
- **Description**: #6028 made every *install* honour `ForcedWeather` through `forced_or`. A *change* of the forced weather is only acted on by `apply_forced_weather`, and that runs only from `step_streaming`, which returns early unless an exterior `WorldStreamingState` exists. A door transition into an interior drains that state. So a `ForceWeather` issued inside a Show-Sky interior leaves the interior sky on the climate default until the next cell load. The same holds for an ordinary interior's window-portal sky (`portal_outdoor_sky` from `SkyParamsRes`), and for the release. GECK documents `ForceWeather` as an immediate switch.
- **Evidence**: `apply_show_sky_interior_climate` has one caller (`load.rs:744`). `step_streaming` returns when `self.streaming.is_none()`.
- **Impact**: Quest scripts that force weather while the player is indoors in a Show-Sky cell do not show the change until the next cell load. Exterior behaviour is correct.
- **Suggested Fix**: Track the applied forced weather outside the streaming state, for example on a resource. When it changes in an interior, re-run the Show-Sky install, or re-apply the worldspace environment for the portal palette, through the same `forced_or` plus `collapse_weather_transition`.

### EXT-D5-2026-10-21-06: `watal.md`'s surface-sound row names a deleted field and the wrong game coverage after #6045
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: n/a (doc rot)
- **Game Affected**: documentation only
- **Location**: `docs/engine/watal.md:746`
- **Status**: NEW
- **Description**: `1dd217412` (#6045) replaced `WatrRecord::surface_sound: String` with `sound_form: u32`. The new field is remapped like XNAM and decoded on every schema (SOUN on Oblivion/FO3/FNV, SNDR on Skyrim). The row still reads "Oblivion `SNAM` → `surface_sound`", with FO3/FNV and Skyrim+ marked `n/a`. `surface_sound` no longer exists in the tree.
- **Suggested Fix**: Re-point the row to `sound_form`, mark FO3/FNV and Skyrim+ AUTHORED, and keep "(consumer pending)".

## Dimension notes

- **Dim 1 (EXAL boundary).** These items were read and hold: `forced_or` covers `apply_environment` and the XCCM re-apply (`apply_cell_climate_override` → `apply_environment`). The `apply_forced_weather` snap (`collapse_weather_transition`) and the bootstrap seed `applied_forced_weather = Some(bootstrap_forced_weather)` are correct. The `#6026` flush pin now cuts with `source_scan::production_text`. `CurrentWeather` takes the forced form. Commit `f16741886` promotes Oblivion DATA cloud speeds into `cloud_layer_velocities` with the `_authored` flag, so `cloud_scroll_vectors` reads them. FO4 `commonwealth` arid is recorded as an art-direction choice. The only new finding is D1-05.
- **Dim 2 (terrain).** Hide Quad handles the all-hidden tile through an early path (`terrain.rs:1159-1167`): collider kept, splat refs released. Patched height handles are released by `GpuTerrainTile::texture_indices` (`base_height_index`, `layer_height_index`). `normal_map_green` is a translate-side data table. The terrain-height concurrency defects are already reported this suite and are not re-filed: **CONC-2026-10-21-01** (no panic handler on the height pool) and **CONC-2026-10-21-02** (shared temp file). The cache-hit decode and the missing memory-budget row are **PERF-D3-01**. The LTEX Havok material (`c53d87679`) stays under open #6048.
- **Dim 3 (ground cover).** Only fixes landed: #6027, #6030, #6031, #6032 and #6033, each with its guard live. The new GLSL is the height-blend call (same `byroTerrainHeightBlend` as terrain), the Ross-G constant diffuse (`GROUNDCOVER_CANOPY_EXTINCTION_K`, cited) and the cloud term. No new bare literals. Cross-references: **PERF-D5-01** (blade root-colour fetch cost) and **CONC-2026-10-21-03** (the #6031 ping-pong residual).
- **Dim 4 (sky / weather / sun).** Checked and OK:
  - Cloud-shadow addressing: bake and consumers share `cloud_shadow.glsl`. `cloud_origin` and `cameraPos` are both absolute. `vWorldPos` and `fragWorldPos` are absolute at every consumer.
  - Barriers: `UNDEFINED → GENERAL`, then compute → fragment read, then compute → host for the readback. Every consumer of binding 24 is a fragment shader. Binding 24 is `PARTIALLY_BOUND` (binding ≥ 5) and gated on `exteriorSkyTint.w`.
  - Dome decode: `dome_colour_radiance` is applied once to zenith, horizon and lower. No other CPU consumer reads the raw colours. The night gate keys on `sun_dir.y`, not on palette luminance.
  - `cloud_authored_colour`'s mean-at-LOD-12 assumes full mip chains. `dds_mip_scan` over the FNV, FO3 and Oblivion sky archives found every cloud DDS with a full chain (6, 10 and 18 files). Not a finding.
  - Already reported this suite: **PERF-D5-02** (bake runs every frame including interiors) and **PERF-D4-01** (SVGF key folds the TOD sun).
- **Dim 5 (WATAL).** Unchanged apart from the SNAM decode (D5-06 doc rot). `sound_form` has no consumer, which watal.md already marks as pending.
- **Dim 6 (distant LOD).** `cb5368348`: confirmed that FO4's `Fallout4 - Meshes.ba2` ships `LODSettings\Commonwealth.LOD` / `DiamondCity.LOD` / `SanctuaryHillsWorld.LOD`, so `extract_mesh_exact` reaches them on FO4 as well. The 16-byte layout is pinned only for Skyrim (`lod_descriptor_tiles_the_shipped_quad_names`); FO4's is assumed the same, with the NAM0 fallback if truncated. New finding: D6-02.
- **Dim 7 (gates / harness).** No commits. One comparability caveat: frames captured before `c231f5b5f`/`ae557b935` (2026-10-20) are not comparable, because the sky cube dimmed about 2.3× and cloud dimming moved from the key to the shadow map.

## Known-Open Register

| Item | State after this pass |
|---|---|
| #6053 terrain-LOD overlap (root cause in EXT-D6-2026-10-19-03) | open, no code change since 10-19 |
| #6092 capacity-refused LOD upload cached as a permanent miss | open, unchanged. D6-02's missing blocks go through the same `missing_blocks` cache. |
| #6093 `Starved` never settles telemetry | open, unchanged |
| #6094 coarse terrain-LOD bands assume a ~6-cell radius | open, unchanged |
| #6095 terrain/LOD doc rot | open, unchanged |
| #6089 FO3 "Use Land Data" children draw no ground | open, unchanged (`75b921cf4` is Oblivion-gated by design) |
| #6048 terrain colliders and LTEX Havok data | open. `c53d87679` landed the material half; friction and restitution are still unapplied. |
| #6106 full-detail pool refusal never retried | open. `0188ce132` only counts the refusals. |
| #3307 VWD full-model culling | open |
| #6195 / #6196 / #6197 / #6198 terrain height epics | #6195 / #6196 landed in part (`5017a64c2`, `ea45af067`, `2975d2906`, `5f1a88929`); #6197 / #6198 not started |
| #6201 per-species blade length | open enhancement |
| Earlier exterior findings #6026, #6027, #6028, #6029, #6030 | all fixed. Their guards are live: `the_environment_installer_flushes_the_sky_textures_it_queues`, `the_model_field_applies_the_authored_density_once`, `a_forced_weather_replaces_the_default_in_every_install`, plus the #6029 / #6030 doc and constant fixes. D1-05 is a residual of #6028. |
| skyal.md open items (cloud-type taxonomy, noise frequencies, atmosphere LUT) | unchanged. Positional cloud shadows shipped (§2.5); D4-04 records one edge interaction. |

Publish with `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-21.md`. Suggested labels: `terrain-exterior`, plus `shaders` (D4-01, D4-03, D4-04), `game:fo4` (D6-02), `doc-rot` (D5-06), `water` (D5-06).
