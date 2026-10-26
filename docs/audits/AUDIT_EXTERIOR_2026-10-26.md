**HEAD**: 3e6f6b66a · **Baseline**: [AUDIT_EXTERIOR_2026-10-21.md](AUDIT_EXTERIOR_2026-10-21.md) (HEAD `4a3e89d56`; 324 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain), Dim 3 (ground cover), Dim 4 (sky / weather / sun), Dim 5 (water), Dim 6 (distant LOD / trees), Dim 7 (gates / harness, static check only) · **Unchanged since baseline (skimmed)**: none. Every dimension had commits in its `Paths:` (Dim 7: only `635330d08`).

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-26

**Method.** Each dimension was audited by its own agent, with at most three running at once. The agents were read-only on the repo and ran no engine, no `--ignored` plugin tests and no smoke gates. They read the diffs of every commit in their `Paths:` since `4a3e89d56`, re-verified every baseline finding, checked that the skill's named guards are live (plain `#[test]`, not `#[ignore]`), and deduplicated against `/tmp/audit/issues.json` (104 open issues), the closed-issue search and prior reports. I spot-checked two claims myself against the code: D1-01 (the exterior `CellLightingRes` insert at `world_setup.rs:544` sits on the mid-session interior path with nothing after it to restore the interior lighting) and D3-01 (`groundcover_models.comp:449` and `:646` both read `gcModelHash(h, 4u)`). Every finding below is a static trace. **None was confirmed on screen.**

**Test state (HEAD, repo `target/`, `TMPDIR=/mnt/data/tmp`, all green):**

| Suite | Result |
|---|---|
| `-p byroredux --bin byroredux -- env_translate climate default_weather forced fog weather exterior` (Dim 1) | 332 passed, 0 failed, 5 ignored |
| `-p byroredux terrain` / `-p byroredux-terrain-height` / `-p byroredux-renderer --lib -- terrain` (Dim 2) | 103 passed, 3 ignored / 11 passed / 35 passed |
| `-p byroredux-renderer groundcover` / `-p byroredux groundcover` (Dim 3) | 71 passed / 68 passed, 1 ignored |
| `-p byroredux-renderer --lib -- sky_ cloud` / `-p byroredux weather` (Dim 4) | 54 passed, 1 ignored (GPU, not run) / 97 passed |
| `-p byroredux water` / `-p byroredux-renderer --lib -- water` / `-p byroredux-core water` / `-p byroredux-physics water` (Dim 5) | 171 passed, 4 ignored / 86 / 18 / 50 |
| `-p byroredux lod` / `-p byroredux tree_lod` (Dim 6) | 208 passed, 6 ignored / 11 passed, 1 ignored |
| `scripts/check-shader-artifacts.sh` | 38 shaders byte-identical to glslang 16.4.0 (no stale `.spv`) |

Not run: the `--ignored` real-data and GPU tests, and every smoke gate (`m-exteriors.sh`, `w1-water-traversal.sh`, `m34-day-night.sh`).

## Executive Summary

**18 findings: 0 CRITICAL · 1 HIGH · 4 MEDIUM · 13 LOW.** 17 are NEW. One is Existing (#6053), and one NEW LOW is another instance of the open #6373 class.

**What matters:**

- **EXT-D1-2026-10-26-01 (HIGH, regression of #6309).** The fix for baseline finding D1-05 makes a mid-session `ForceWeather` in an ordinary interior reinstall the *exterior* lighting over the cell's own `CellLightingRes`. From then on `weather_system` treats the room as an exterior. It writes sun, ambient and fog into it, overwrites `ImageSpaceBase` and advances rain/snow state, until the cell is left. At cell load this is hidden because `apply_interior_cell_lighting` runs afterwards; the mid-session path has nothing after it. No test asserts the interior survives.
- **EXT-D1-2026-10-26-02 (MEDIUM, regression of #6123/#6089).** #6089 widened `apply_exterior_processing` to `Oblivion | Fallout3NV`, but `SharedParseCache::get_or_parse` still computes `exterior_ready` as Oblivion-only. An FO3/FNV index parsed for an interior is served unprocessed to the later exterior session, so the Use-Land-Data towns draw no ground again. The existing cache test encodes the stale behaviour.
- **EXT-D3-2026-10-26-01 (MEDIUM).** In `groundcover_models.comp` the record pick and the plant height scale both read hash stream 4 of the same `h`. Each record's height variation is truncated to its pick bucket whenever more than one record shares the ground. The code predates the baseline.
- **EXT-D6-2026-10-26-01 (MEDIUM).** The new terrain-LOD skirts (`7eac91637`) look at neighbour cells across block borders, but a block only rebuilds when its own 16-cell mask changes. The crack returns when the resident region advances against a block edge. `.btr` blocks (Skyrim/FO4) get no skirts at all.
- **EXT-D6-2026-10-26-02 (MEDIUM, Existing #6053).** The root cause of the 384 FNV/FO3 terrain-LOD overlaps (`swallowed_by_coarser` only matches `level * 2`) is still in the code. `m-exteriors.sh fnv boundary` is therefore still red.

**Baseline findings (2026-10-21):** all six are closed. D1-05 is closed in effect, but its fix introduced D1-01.

| Baseline | Fix | Verified |
|---|---|---|
| D4-01 effect-lit sun site | `e7eb57a3b` (#6244) | guard derives every `color_type.w) == 2u` arm, non-empty |
| D4-03 interior caustics | `7180d3109` (#6245) | `caustic_sun_transmittance_applies_the_cloud_column_only_outdoors` live |
| D4-04 cloud-shadow edge | `6181e233f` (#6310) | `the_cloud_shadow_edge_fades_to_the_overcast_term_not_to_unshadowed` live |
| D6-02 FO4 synth MNAM-only base | `6528e110b` (#6246) | `synth_block_base_resolves_an_mnam_only_ltex_through_its_bgsm` live |
| D5-06 watal.md surface-sound row | `5a1e98589` (#6311) | `watal.md:951` now names `WatrRecord::sound_form` |
| D1-05 interior forced weather | `fb51f5987` (#6309) | sky now changes, but see D1-01 |

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| single-boundary | **Broken twice.** (1) The parent-landscape gate and the `exterior_ready` predicate are two predicates for one rule (D1-02). (2) Mild: the shore-refraction tuning literals are retyped in `water.vert` and the CPU crest sampler (D5-02). No second `translate_sky` / `translate_weather` / `SkyParamsRes{` / `WeatherDataRes{` / `WaterMaterial{` site. All environment installs go through `install_environment`. LTEX → diffuse is now one resolver again (`LandscapeLayerTextures::texture_set`, #6246). |
| no-fabrication | **Holds, with small notes.** The 20 m/s cloud drift cap, parallax range/depth and `LOD_SKIRT_DROP` (640) are named engine choices, though the cap is not in skyal.md (D4-02). Small blemishes: the underwater god-ray water index is a bare `1.333` (D5-01) and the "moment-matched" blade length is not moment-matched after its clamp (D3-02). |
| no-leak | **Broken once.** D1-01: the exterior environment overwrites the interior's `CellLightingRes` / `ImageSpaceBase` on the #6309 path. The caustic and cloud-shadow interior gates (#6245, #6244) hold. |
| no-render-time-fallback | **Holds.** No `GameKind` or game token in the sky, cloud, cloud-shadow, ground-cover, water or composite GLSL. `grep -rn GameKind crates/physics/src` is empty. Every `GameKind` use in the LOD rings is a scheme table. |

## Per-Category Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Terrain (LAND, splat, GNAM cover) | `build_cell_splat_layers`, `build_terrain_vertices`, `cover_density_inputs`, `spawn_terrain_mesh` | OK; `GpuTerrainTile` is declared in one GLSL file | OK (D2-02: default-LTEX affinity under `localised` undecided) | OK | OK |
| Height maps / blend / parallax | `apply_ready_heights`, `set_terrain_parallax`, `push_terrain_parallax` | OK (unit conversion in one site) | OK (engine choices documented) | OK | OK |
| Sky (bake, clouds, SH) | `translate_sky`, `pack_sky_dome`, `cap_cloud_layer_drift` | OK | OK | OK | OK |
| Weather / sun | `translate_weather`, `weather_system`, `exteriorCloudSunTransmittance` | OK for the scanned sites; **D4-01** (god rays unscanned) | OK | **D1-01** (interior re-apply) | OK |
| Climate / landscape inheritance | `resolve_exterior_climate`, `apply_exterior_processing` | **D1-02** (gate vs `exterior_ready`) | OK | OK | OK |
| Water (ESM / NIF) | `resolve_water_material`, `water_material_from_mesh` | OK (two sites); **D5-02** shape literals | **D5-01** (IOR literal) | OK | OK |
| Water bed / shore / swell / river | `bed_field_from_land` → `bed_vertex_color` + `WaterBedField` | OK for depth; **D5-03** (shore direction at cell borders) | OK | OK | OK |
| Ground cover | `groundcover_translate.rs`, `resolve_authored_cover` | OK | OK (D3-02, D3-03) | OK | OK (**D3-01** is a correlation bug, not a fallback) |
| LOD (terrain) | `spawn_lod_block`, `synth_base_texture_path`, `translate_terrain_lod_textures` | OK | OK | OK (**D6-01** stale skirts) | OK |
| LOD (objects / placement / trees) | `object_lod_scheme`, `placement_lod_cell_path`, `lod_worldspace_key`, `build_tree_lod_mesh` | OK | OK | OK (**D6-02**/#6053 double-draw on FNV/FO3) | OK |

## Findings

Finding IDs are `EXT-D<dim>-2026-10-26-NN`. Severity is stated inside each block.

### EXT-D1-2026-10-26-01: #6309's interior forced-weather re-apply replaces the interior's `CellLightingRes` with exterior lighting and un-gates the weather system on it
- **Severity**: HIGH (a scripted `ForceWeather` / `ReleaseWeatherOverride` while indoors turns the whole interior into an exterior-lit one until the cell is left; a ~7-line restore fixes it)
- **Dimension**: 1 EXAL boundary discipline
- **Tier Violated**: no-leak (the exterior environment overwrites interior-owned state)
- **Game Affected**: Oblivion, FO3, FNV (ObScript `ForceWeather`/`SetWeather`/`ReleaseWeatherOverride`; any cell with `is_interior`)
- **Location**: `byroredux/src/scene/world_setup.rs:389-432` (`apply_forced_weather_in_interior`), `world_setup.rs:482-484 / 543` (`translate_exterior_cell_lighting` -> `world.insert_resource(lighting)` in `apply_environment_with_sources`), `byroredux/src/app_step.rs` (`step_interior_forced_weather`), `byroredux/src/systems/weather.rs:1295,1316` (the `!is_interior` write gates)
- **Status**: NEW (regression introduced by fb51f5987, the fix of baseline EXT-D1-2026-10-21-05). Static trace; **not confirmed on screen**.
- **Description**: `apply_forced_weather_in_interior` re-runs `install_environment` -> `apply_environment_with_sources`, the same function the cell-load path uses. That function unconditionally does `world.insert_resource(lighting)`, where `lighting = translate_exterior_cell_lighting(..)` (`is_interior == false`, asserted by `env_translate/sky_weather.rs:866`'s test), and then ends with `weather_system(world, 0.0)`. At cell load this is harmless because `finish_interior_cell_load` -> `apply_interior_cell_lighting` (`cell_loader/transition.rs:623`) runs *after* `apply_show_sky_interior_climate` and re-installs the cell's XCLL/LGTM `CellLightingRes`, `InteriorSkyExposureRes` and `ImageSpaceBase`. The #6309 path is a mid-session call with nothing after it: no interior lighting is restored. Afterwards `CellLightingRes.is_interior` is false, so `weather_system` treats the interior as an exterior: it writes the weather's ambient, directional colour/direction, fog colour/near/far/medium into `CellLightingRes` every frame (`weather.rs:1295-1304`), publishes the exterior image space over the interior's `ImageSpaceBase` (`weather.rs:1316`), and advances `WeatherSurfaceState` (rain/snow accumulation, `weather.rs:1104`). `interior_forced_weather_pending` also stops matching (it requires `is_interior`), so the state is never corrected.
- **Evidence**: `world_setup.rs:389-432` ends with `collapse_weather_transition(world); true` after `install_environment(..)` / `apply_show_sky_interior_climate(..)`; neither restores `CellLightingRes`. The #6309 tests (`a_forced_weather_change_is_a_difference_from_what_was_installed`, `a_forced_weather_waits_for_a_fully_loaded_interior`, `only_the_install_funnel_calls_the_bare_environment_installer`) test the change detector, the gate and the funnel call count only; none asserts that `CellLightingRes.is_interior` / the XCLL values survive the re-apply (the commit message says "Not run live"). `interior_boot_installs_the_canonical_outdoor_environment_once` asserts "the room keeps its own lighting" only for `install_interior_outdoor_defaults`, not for this path.
- **Impact**: the very scenario #6309 targets (a quest script forcing weather while the player is indoors) now lights the room with the sun/ambient/fog of the forced weather and drops the cell's authored XCLL, IMAD grade and the sealed-interior sun gate (#782/#1282). Worse for ordinary (non-Show-Sky) interiors than before the fix, where the only defect was a stale window-portal palette.
- **Related**: #6309 (closed, introduced), #6028, EXT-D1-2026-10-21-05, #782 (interior gate), `render/mod.rs` reads of `InteriorSkyExposureRes`.
- **Suggested Fix**: in `apply_forced_weather_in_interior`, snapshot `CellLightingRes`, `InteriorSkyExposureRes` and `ImageSpaceBase` before the install and put them back after it (the outdoor lanes `SkyParamsRes`/`WeatherDataRes` keep the new weather), or give `apply_environment_with_sources` an `interior: bool` that skips the `CellLightingRes` insert and the `weather_system` lighting writes. Add a test that installs an interior `CellLightingRes` plus a forced weather and asserts `is_interior` and the XCLL ambient survive.

### EXT-D1-2026-10-26-02: #6089 widened `apply_exterior_processing` to FO3/FNV but left the shared-parse `exterior_ready` predicate Oblivion-only, so an FO3/FNV interior-first parse is served to the exterior session unprocessed
- **Severity**: MEDIUM
- **Dimension**: 1 EXAL boundary discipline (parse-boundary handoff)
- **Tier Violated**: single-boundary (two predicates, `apply_exterior_processing`'s game gate and `get_or_parse`'s `exterior_ready`, now disagree about which games need exterior processing)
- **Game Affected**: FO3 and FNV (`GameKind::Fallout3NV`): Oasis, ParadiseFalls, GNRroofWorld, MegatonWorld and any "Use Land Data" child
- **Location**: `byroredux/src/cell_loader/load_order.rs:614-626` (`apply_exterior_processing`, now `Oblivion | Fallout3NV`) vs `load_order.rs:702` (`let exterior_ready = exterior || index.game != esm::reader::GameKind::Oblivion;`); doc at `load_order.rs:683-686`
- **Status**: Regression of #6123 (`588c87bc4`) caused by a9472103b (#6089). Static trace; unconfirmed live (needs FO3 data).
- **Description**: #6123 made `SharedParseCache` serve one parse to both interior and exterior sessions, and marked an index `exterior_ready` when parsed for an exterior *or* when its game needs no exterior processing (then only Oblivion did). #6089 added `Fallout3NV` to `apply_exterior_processing` but did not touch line 702. A session that boots into an FO3/FNV interior (`--cell`, or a load into an interior) parses with `exterior == false`; `inherit_parent_landscapes` is not run, yet `exterior_ready` is computed as `true` for `Fallout3NV`. The later exterior session (door to the Wasteland, `build_exterior_world_context`) calls `get_or_parse(.., exterior = true)`, finds `shared.exterior_ready`, and reuses the unprocessed index. The "Use Land Data" children then have no ground again, which is exactly the FO3-SD-03 symptom #6089 closed, but only for interior-first sessions.
- **Evidence**: `a_second_transition_over_the_same_plugins_does_not_parse` parses `GameKind::Fallout3NV` with `exterior=false` first, then asserts `Arc::ptr_eq` for `exterior=true` requests, i.e. it encodes the now-wrong behaviour; only the Oblivion test (`an_oblivion_interior_parse_is_not_served_to_an_exterior_session`) covers the re-parse. The #6089 census pin (`fo3_use_land_data_children_inherit_the_wasteland_landscape`) is `#[ignore]` and calls `inherit_parent_landscapes` directly, so nothing in the default suite pins the gate or the predicate.
- **Impact**: FO3/FNV Use-Land-Data towns (Oasis 34, ParadiseFalls 11, GNRroofWorld 58, MegatonWorld 5 LAND-less cells) draw no terrain when the first cell loaded was an interior. Workaround: boot with `--grid`/exterior first.
- **Related**: #6123, #6089 (closed), #5505, FO3-SD-03.
- **Suggested Fix**: one `fn needs_exterior_processing(game: GameKind) -> bool` (table-shaped) used by both `apply_exterior_processing` and the `exterior_ready` expression; add an FO3 case to the cache test (an `exterior=false` parse of `Fallout3NV` must be re-parsed by an `exterior=true` request) and flip the existing FO3 assertion in `a_second_transition_over_the_same_plugins_does_not_parse` to a non-processed game.

### EXT-D2-2026-10-26-01: `spawn_lod_block` lost its doc comment and `#[allow(clippy::too_many_arguments)]` to a new const (7eac91637)
- **Severity**: LOW / **Dimension**: 2 (terrain-LOD, brief-named commit) / **Tier Violated**: n/a
- **Game Affected**: all LAND games (tooling / docs) / **Location**: `byroredux/src/cell_loader/terrain_lod.rs:842-851` and `:901` / **Status**: NEW
- **Description**: The skirt commit inserted `LOD_SKIRT_DROP` and `append_lod_skirts` between `spawn_lod_block`'s attribute+doc block and the
  function. The 5-line "Build + spawn one finest-band LOD block ..." doc and `#[allow(clippy::too_many_arguments)]` (line 847) now sit
  above `const LOD_SKIRT_DROP` (line 851); `fn spawn_lod_block` (line 901, 12 parameters) has neither.
- **Evidence**: lines 842-851 read `/// Build + spawn one finest-band LOD block...` / `#[allow(clippy::too_many_arguments)]` / `/// How far a skirt hangs...` /
  `const LOD_SKIRT_DROP: f32 = 640.0;`; line 901 is a bare `fn spawn_lod_block(`.
- **Impact**: rustdoc shows the spawn doc on the skirt constant and nothing on the function; clippy `too_many_arguments` fires on
  `spawn_lod_block` (static; clippy not run here - workspace clippy aborts at the first failing crate). Same defect class #6095 / open #6373
  describe, introduced after #6373 was filed (that issue's list does not include `terrain_lod.rs`).
- **Related**: #6373 (open, class), #6095 (closed), #5622 (reasonless allows), #6193.
- **Suggested Fix**: Move the doc block and the `allow` to directly above `fn spawn_lod_block`; keep `LOD_SKIRT_DROP` / `append_lod_skirts` above them.

### EXT-D2-2026-10-26-02: #6256's default-LTEX cover classification is overridden to 0.0 on any localised load order
- **Severity**: LOW (possibly intended; unconfirmed on game data) / **Dimension**: 2 / **Tier Violated**: n/a (ground-cover input, not a canonical translate value)
- **Game Affected**: FO4 (the case #6256 targets), Skyrim/FO3/FNV for BTXT-absent bases / **Location**: `byroredux/src/cell_loader/terrain.rs` `cover_density_inputs` (override block after `base_authored_grass`) and `authored_grass_for_splat_layers` / **Status**: NEW (adjacent to closed #6256)
- **Description**: #6256 maps an ATXT form-0 layer to `ltex_form_id = None` and classifies it by the default LTEX editor ID
  (`layer_cover_key` -> e.g. `LCWDefault01Grass01`, "a grass layer" on FO4). But when `AuthoredCover::localised` is true,
  `cover_density_inputs` replaces every lane's affinity with `cover.union_density(&authored_grass[lane])`, and
  `authored_grass_for_splat_layers` yields an empty list for `ltex_form_id == None` (its own doc: "empty = ... the executable's default land
  texture"). `union_density(&[])` is `0.0` ("authored no grass"), so the editor-ID classification only survives when the load order has no
  texture->grass links at all.
- **Evidence**: `resolve_authored_cover` sets `localised` whenever any `LTEX.GNAM` names a kept GRAS (true for vanilla Skyrim/FNV; FO4 not
  checked against data - no game-data run allowed). The #6256 test `atxt_form_zero_resolves_the_default_land_texture` asserts only
  `layer_cover_key` / `layer_affinity`, not the composed `CoverDensityInputs`.
- **Impact**: An FO4 default-texture layer that the fix intends to classify as grass grows no procedural blades on a localised load order
  (same for an absent/zero BTXT base). Whether the engine's built-in default texture should inherit a GNAM list is undecided: the
  default is an exe-side texture (no source for its grass links), so this may be correct; the fix and the override just disagree silently.
- **Related**: #6256 (closed), #4906, #4903, #6271 (the extraction that moved this code).
- **Suggested Fix**: Decide the semantics and pin them with a test on `cover_density_inputs` (localised + `ltex_form_id None`): either skip the
  override for `None` lanes (keep the editor-ID classification) or state in `layer_cover_key`'s doc that it is the non-localised fallback only.

### EXT-D2-2026-10-26-03: Stale GpuTerrainTile layout comments survive #6352's prose sweep
- **Severity**: LOW / **Dimension**: 2 / **Tier Violated**: n/a
- **Game Affected**: n/a (docs) / **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:468-470`; `gpu_types.rs:78-81`; `shaders/include/bindings.glsl:535` / **Status**: NEW
- **Description**: The numeric asserts are right (224 B, offsets to 220), but comments describe older strides: the offsets test says it
  "Matches `ArrayStride 160` + members at 0 / 32 / 64 / 96 / 112 / 128 / 136 / 140 / 144" (the stride is 224, members go to 220);
  `gpu_types.rs` says the band slot kept "the stride stays 208"; `bindings.glsl` says "Appended at the tail; stride 176 -> 208".
  The last two are history phrased as present tense after the record grew to 224.
- **Evidence**: `grep -n "208\|ArrayStride 160" ...` hits above; `gpu_terrain_tile_is_224_bytes` and the shipped-SPIR-V stride test say 224.
- **Impact**: Misleads the next person changing the record (the exact audience of the #6352 sweep).
- **Related**: #6352 (closed, fixed 208->224 in other places), #6206.
- **Suggested Fix**: Reword the three comments to "was 208 at #6196, 224 since #6197" / "ArrayStride 224 + members to 220".

### EXT-D3-2026-10-26-01: Authored-model height scale is correlated with the record pick (both read seed stream 4)
- **Severity**: MEDIUM / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all games with authored GRAS models on ground that lists more than one placeable record (Skyrim/FO4/FO3/FNV)
- **Location**: `crates/renderer/shaders/groundcover_models.comp:449` (pick) and `:646` (scale); `gcPickLocalRecord` `:312-400`
- **Status**: NEW (code predates the baseline: the same two lines are at `:431` / `:593` in 4a3e89d56; not in `docs/audits/AUDIT_EXTERIOR_*` or open issues)
- **Description**: SCATTER picks the record with `gcPickLocalRecord(..., gcUnit(gcModelHash(h, 4u)))`, which does `target = u * accept` and walks records in a fixed lane/bit order. EMIT then
  draws the plant's height scale from `gcUnit(gcModelHash(h, 4u))` of the same `h` (`point.meta.w`). Same hash, same stream, same value: a record that owns the bucket `[a, b)` of `u` can only ever
  receive scale values from that bucket.
- **Evidence**: `pick = gcPickLocalRecord(cellIdx, ground, gcUnit(gcModelHash(h, 4u)));` ... `float scale = 1.0 + (gcUnit(gcModelHash(h, 4u)) * 2.0 - 1.0) * rec.heightRange;`
  Streams in use in this file: 1, 2, 3, 4 (pick), 4 (scale), 5, 7 (new wind phase); 6 is free.
- **Impact**: with two or more records on the same ground, each record's height variation is truncated to a sub-range: the first-ordered record is systematically shorter, the last taller
  (only a single dominant record gets the full `+-heightRange`). This is exactly the "natural height variance" the field exists for, silently biased per species mix. Deterministic, so no flicker. Static trace only; unconfirmed on screen.
- **Related**: #4906 (per-cell record selection), #6027 (density applied once). Skill "Determinism" bullet (streams are independent hashes) is the rule broken.
- **Suggested Fix**: draw the scale from an unused stream (`gcModelHash(h, 6u)`), and add a source-shape test that no two `gcModelHash(h, Nu)` uses in `groundcover_models.comp` share `N`.

### EXT-D3-2026-10-26-02: "Moment-matched" blade-length draw is not moment-matched after the clamp; ~5.6 % of blades pin at the species maximum
- **Severity**: LOW / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all (built-in `DEFAULT_TEMPERATE` / `DEFAULT_ARID`)
- **Location**: `crates/renderer/shaders/groundcover_blade.vert` `byroGcBladeLength` (~l.239-261); `crates/core/src/ecs/components/groundcover.rs` `moment_matched_length` doc; `docs/engine/exal-groundcover.md` 12.16
- **Status**: NEW (7430cc14c, issue #6201 still open)
- **Description**: the doc and the field comment say the built-ins carry "exactly the first two moments of the uniform draw it replaces, so only the shape changes". That holds for the
  untruncated log-normal. The shader clamps to `height_range`, which is only +-1.73 sigma of the matched distribution, so the clamp changes the moments and creates two point masses.
- **Evidence**: numeric replay of the shader's formula (Box-Muller + log-normal + clamp, 400k draws): temperate (9, 21): mean 14.89 (target 15.0), sigma 3.13 (target 3.46, -9.5 %), 5.6 % of blades exactly 21 and 1.7 % exactly 9;
  arid (6, 13.5): mean 9.68 (9.75), sigma 1.96 (2.17), 5.6 % / 1.8 % at the clamps.
- **Impact**: a few percent of blades share one identical (maximum) height, a faint flat-top; the stated "no number invented, spread unchanged" provenance claim is slightly wrong. Cosmetic.
- **Related**: #6201.
- **Suggested Fix**: either correct the wording to "the moments of the unclamped draw; the clamp trims ~6 % to the limits", or resample/rescale the cv so the clamped moments match (or reflect rather than clamp).

### EXT-D3-2026-10-26-03: Model-tier yaw uses a bare 6.28318530718 next to `GROUNDCOVER_TWO_PI` in the same shader
- **Severity**: LOW / **Dimension**: 3 / **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `crates/renderer/shaders/groundcover_models.comp:649`
- **Status**: NEW (literal is pre-baseline; 1ca5032ae now uses `GROUNDCOVER_TWO_PI` ~30 lines away in `gcWindLean`, so the shader is inconsistent)
- **Description**: `float yaw = gcUnit(gcModelHash(h, 5u)) * 6.28318530718;` is the only tau literal left in the `groundcover_*` set; the skill rule treats a bare literal in `groundcover_*` GLSL as a finding.
- **Evidence**: `grep -n 6.2831 crates/renderer/shaders/groundcover_*.{vert,frag,comp}` -> only this line; `GROUNDCOVER_TWO_PI` is a generated define (`shader_constants_data.rs:798`).
- **Impact**: none functional (tau to 11 digits). Tech debt / drift risk only.
- **Suggested Fix**: replace with `GROUNDCOVER_TWO_PI`.

### EXT-D4-2026-10-26-01: Underwater god rays take the raw sun colour x intensity and never apply the cloud-shadow transmittance
- **Severity**: LOW
- **Dimension**: 4 (sky/weather/sun; sun-site contract in skyal.md §2.5)
- **Tier Violated**: n/a (a direct-sun consumer missing the shared transmittance; not a translation-boundary leak)
- **Game Affected**: all exteriors with water (game-invariant)
- **Location**: `crates/renderer/shaders/composite.frag:925-945` (call site), `:569-605` (`underwaterGodRays`); guard gap in `crates/renderer/src/vulkan/sky_cube.rs:1265-1299`
- **Status**: NEW. Static trace only, unconfirmed on screen.
- **Description**: `296d21912` (and `067ce38ef` before it) added an exterior sun consumer in composite. It scales `mix(params.underwater.rgb, params.sun_color.rgb, 0.35) * params.sun_dir.w * 0.12`, where `sun_color` / `sun_dir.w` are the TOD sun-disc colour and intensity (`sky.sun_color`, `sky.sun_intensity`). Those carry the HNAM sunlight dimmer but not the cloud deck. `c231f5b5f` moved cloud dimming out of the sun and into a per-site multiply by `exteriorCloudSunTransmittance` or the camera column, and skyal.md §2.5 says every direct-sun site must apply it ("a new direct-sun site without it ... is a finding"). The god-ray march does not, and composite has no camera-column input. `camera_cloud_sun_transmittance` reaches `volumetric_sun` and `caustic_sun_transmittance` in `post_passes.rs` but not `CompositeParams`. Under a storm deck an underwater camera still gets clear-sky-strength shafts, while the caustics and terrain in the same view are dimmed. `every_direct_sun_site_applies_the_cloud_shadow` scans `lighting.glsl`, grass, water, caustics and `triangle.frag`, but not `composite.frag`, so the guard cannot see it (the same blind spot that hid #6244).
- **Evidence**: `composite.frag:942-944`: `combined += shaftColor * params.sun_dir.w * 0.12 * underwaterGodRays(...)`; `grep -n "cloud\|transmittance"` over that block finds nothing. The gate is `params.depth_params.x > 0.5` (exterior only) `&& params.underwater.w > 0.0 && params.sun_dir.w > 0.0`. The nearby `volumetric_sun` doc comment says the haze and godrays dim with the terrain key through `cloud_transmittance`, but that is the froxel path, not this one.
- **Impact**: Visible only underwater in an exterior under heavy overcast. Over-bright light shafts, up to roughly the 1/transmittance ratio (~12x at full cover, minus the HNAM dimmer already in `sun_color`). Narrow case, small amplitude (0.12 factor), so LOW.
- **Related**: EXT-D4-2026-10-21-01 (same class, fixed by #6244); skyal.md §2.5.
- **Suggested Fix**: Pass `camera_cloud_sun_transmittance` into `CompositeParams` (the reserved `caustic_flags` / `underwater` lanes or a new tail field, with the std140 offset pin) and multiply the god-ray term by it. Extend the guard to scan `composite.frag` for the underwater sun consumer.

### EXT-D4-2026-10-26-02: skyal.md is silent on the 20 m/s textured-cloud drift cap, which is documented only in watal.md
- **Severity**: LOW
- **Dimension**: 4 (doc-rot)
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `byroredux/src/systems/weather.rs:692-720` (`CLOUD_LAYER_MAX_DRIFT_M_PER_S`, `cap_cloud_layer_drift`); `docs/engine/watal.md:609-632` (the only prose); `docs/engine/skyal.md` §2.3-§2.5
- **Status**: NEW (documentation placement)
- **Description**: The cap is a user-chosen calibration ("near real life", 2026-10-26), not an authored value. It bounds every authored textured layer to 20 m/s of ground drift over the 9 km shell, silently overriding the WTHR scroll rate. Code is clean: it is a named constant derived from `CLOUD_UPPER_PLANE_ALTITUDE`, with a pinning test. But the sky's source-of-truth doc (skyal.md, the one the skill says to cite) does not mention it. A future sky audit that sees authored scroll rates (e.g. the Oblivion layer-0 0.0143 UV/s) not matching the rendered motion has no skyal.md entry to cite, and the rule "documented, do not re-file" can only be applied from watal.md. skyal.md §2.5 also does not list the effect-lit directional arm (#6244) in its sun-site enumeration ("lighting.glsl's two directional arms, grass, the water glint"), though the guard and skill do.
- **Evidence**: `grep -n "20 m/s\|drift\|scroll" docs/engine/skyal.md` -> no cap entry (one unrelated "drift speed" line at 294). `grep -rn CLOUD_LAYER_MAX_DRIFT` hits only `weather.rs` plus the watal.md prose, which names `cap_cloud_layer_drift` but not the constant.
- **Impact**: Doc-only; the calibration constant is easy to rediscover as a "magic number" and the sun-site list misleads a reader about guard coverage.
- **Related**: 4f2596bf6.
- **Suggested Fix**: Add a short "Textured-layer drift cap" paragraph to skyal.md §2.4 (constant name, 20 m/s, formula `scroll / tile_scale * CLOUD_UPPER_PLANE_ALTITUDE`, chosen-not-authored, deck advection untouched) and add "the effect-lit pass" to the §2.5 sun-site list; keep the watal.md cross-reference.

### EXT-D4-2026-10-26-03: Duplicate `#[cfg(test)]` attribute on `mod cloud_scroll_vectors_tests`
- **Severity**: LOW
- **Dimension**: 4 (tech-debt)
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/systems/weather.rs:1545-1546`
- **Status**: NEW (introduced by 4f2596bf6, which inserted `cloud_drift_cap_tests` plus a stray `#[cfg(test)]` above the existing one)
- **Description**: Two consecutive `#[cfg(test)]` lines gate one module. Harmless at runtime, but it trips `clippy::duplicated_attributes`, and it is a merge artefact that suggests the hunk was spliced by hand.
- **Evidence**: `sed -n 1545,1547p` -> `#[cfg(test)]` / `#[cfg(test)]` / `mod cloud_scroll_vectors_tests {`.
- **Impact**: None functional; adds one lint site to the workspace clippy backlog.
- **Related**: 4f2596bf6.
- **Suggested Fix**: Delete one of the two lines.

### EXT-D5-2026-10-26-01: Underwater god-ray march hard-codes the water IOR (`1.333`) that #4010 removed from the caustic path
- **Severity**: LOW
- **Dimension**: Water translation (WATAL) / composite
- **Tier Violated**: no-fabrication (a canonical authored field is re-derived as a literal at the consumer)
- **Game Affected**: all (cosmetic, underwater only)
- **Location**: `crates/renderer/shaders/composite.frag:574` (`underwaterGodRays`)
- **Status**: NEW (same class as closed #4010; different site)
- **Description**: `refract(-sunDirection, vec3(0.0, 1.0, 0.0), 1.0 / 1.333)` fixes the air-to-water index at the clean-water default. #4010 fixed exactly this in `water.frag`'s caustic ray, which now reads the authored `WaterMaterial::ior` (`water.frag:1463-1468`). Its comment says a hard-coded 1.33 made "the caustic focus disagree with the refraction seen through the same surface". The camera's `UnderwaterView.surface` already ships depth and fog span through `caustic_flags.zw`, and `UnderwaterView` has no IOR lane.
- **Evidence**: `composite.frag:574`; no `WATER_IOR` or equivalent in `shader_constants_data.rs` or core. `grep 1.333` over the shaders finds this line only.
- **Impact**: The shaft convergence and focus do not follow an authored or tuned index (Skyrim, FO4 and Starfield author it; FO3/FNV/Oblivion use the default). Visual only, and small, since the default is physically correct for water.
- **Related**: #4010; `watal.md` (god-rays section) already calls the march's scales/sharpness/energy factor "chosen values".
- **Suggested Fix**: Pass the submerged plane's `ior` through the reserved `UnderwaterView.surface` lanes and use it in the `refract()`. Otherwise name `1.333` in `shader_constants_data.rs` as the clean-water default and cite it. The other chosen march values (`g = 0.6`, 24 steps, 70/37 BU focus scales, ridge^6, 0.12) could be named in the same pass for the #6392 discipline.

### EXT-D5-2026-10-26-02: Shore-refraction tuning literals are retyped in `water.vert` and the CPU crest sampler instead of owned in core
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: single-boundary (the skill's "retyped literal in the shader and the CPU crest sampler is a finding" rule)
- **Game Affected**: all (calm water near a shore)
- **Location**: `crates/renderer/shaders/water.vert:204-216,266-277` vs `crates/physics/src/water.rs:489-496,522-528,576-577`; `crates/core/src/ecs/components/water.rs:891,899`
- **Status**: NEW
- **Description**: The bed-shape constants are owned in core (`WATER_SHORE_REFRACT_DEPTH` / `_MAX`, `WATER_SWELL_*`, `WATER_RIVER_*`). Four shape parameters sit beside them as bare literals, mirrored by hand:
  - The `0.6` share by which the ripple pair (`dirA`/`dirB`) turns toward the shore: `refraction * 0.6` in `water.vert` against `refraction * 0.6` in `physics/water.rs:576-577`. It is the only tuning knob without a named const.
  - The `0.5` upper edge of the swell's "travelling toward shore" ramp: `smoothstep(0.0, 0.5, dot)` against `cosine / 0.5` in `refract_swell`.
  - The `0.1` minimum shoreward-vector length: `uphillLen > 0.1` in `water.vert:204,210` against `shore_len > 0.1` in `physics/water.rs:522`, plus `bank_len > 0.1` in `WaterFlow::over_bed`.
  - The `0.25` inner-edge fraction (`0.25 * WATER_SHORE_REFRACT_DEPTH`, `0.25 * WATER_RIVER_BANK_DEPTH`). This one is structural to the smoothstep form.
- **Evidence**: The text pin `swell_shore_and_river_shaders_keep_their_contract` checks only the swell phase/shoal needles, not these. A retune on one side would silently desync the visible and buoyancy crest direction.
- **Impact**: No current mismatch; the values agree today. It is a drift trap on exactly the CPU/GPU parity the #5582 rule exists to prevent.
- **Related**: #5582 (`7cf2bd086`).
- **Suggested Fix**: Add `WATER_SHORE_RIPPLE_TURN_SHARE`, `WATER_SWELL_SHORE_TOWARD_RAMP` and `WATER_SHORE_MIN_UPHILL_LEN` to core `water.rs`, re-export them through `shader_constants_data.rs`, and use the names in both places.

### EXT-D5-2026-10-26-03: CPU shore direction diverges from the mesh's baked direction near cell borders; the parity test pins depth only
- **Severity**: LOW (static trace, not confirmed on screen)
- **Dimension**: Water translation (WATAL) / physics seam
- **Tier Violated**: n/a (CPU/GPU coherence)
- **Game Affected**: exterior cell water planes over LAND (all games with LAND water)
- **Location**: `byroredux/src/cell_loader/water.rs:244-262` (bake) vs `:621-629` and `crates/core/.../water/bed_field.rs:77-107` (physics sample); test `the_physics_bed_field_agrees_with_the_baked_vertices` (`:2043-2062`)
- **Status**: NEW
- **Description**: `53df201ea` zeroes `uphill` on the mesh's border vertices so neighbouring cells do not disagree and crack. The physics `WaterBedField` is built from the same function but keeps the full bilinear gradient at every point, including the border strip. The GPU also interpolates per-vertex forward-difference gradients across triangles, while the CPU evaluates the in-cell bilinear gradient. So in the outermost 128-unit strip of a cell, `water.vert` refracts the swell and the ripple pair by a fading or zero shore direction, while `authored_wave_height_over_bed` refracts them fully. The module doc claims the visible and buoyancy crest "cannot disagree about how deep the water is". That holds for depth, and the test only asserts depth (`sample.depth` against the baked `r` channel). Shoreward direction is unchecked.
- **Evidence**: Test body compares only `vertex.color[0] * WATER_BED_DEPTH_MAX`. `bed_vertex_color` and the border zeroing are in the mesh bake path only.
- **Impact**: Buoyancy, camera submersion and the player see a slightly different crest phase from the rendered one within shallow border strips (swell amplitude up to 28 BU, shallow-capped). Small and local, since refraction only applies where the bed is shallower than 900 BU.
- **Related**: `53df201ea`, `7dd294ee3`.
- **Suggested Fix**: Either make `WaterBedField::sample` take an `edge_zero` flag mirroring the bake, or add a test that compares `uphill` as well and document the border case. The first is preferred because the mesh and CPU then share one rule.

### EXT-D6-2026-10-26-01: Terrain-LOD skirts go stale at block seams, and a `.btr` block never gets any
- **Severity**: MEDIUM / **Dimension**: 6 (Distant LOD and trees) / **Tier Violated**: n/a
- **Game Affected**: all games with terrain LOD; the FO4 and Oblivion crack the commit fixes are the reported ones. Skyrim/FO4 `.btr` blocks are affected differently (see below).
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1026-1050` (skirt edge discovery), `:205-218` (`block_hole_mask`), `:~625-660` (staleness test `prev_mask == mask`), `:~700-712` (`.btr` path).
- **Status**: NEW. Static trace; **unconfirmed on screen**.
- **Description**: `7eac91637` decides a skirt per LOD quad edge by asking whether the *neighbouring cell* is resident, including cells in the next block (`neighbour_is_full_detail`, "the neighbour may be in the next block, so go by cell"). But the only rebuild key is `LodBlock::hole_mask` = `block_hole_mask(...)`, which covers the block's own 16 cells. So:
  1. A block whose own mask is unchanged is never rebuilt when a resident cell *in the adjacent block* appears or leaves. The edge between them keeps whatever skirts it had at spawn time: when the resident region advances against a block edge, the crack returns on that edge (about every fourth step along each axis, where the newly resident cell sits on a block boundary).
  2. A finest-band block with `mask == 0` on a Combined game (Skyrim/FO4) takes the prebaked `.btr` path (`spawn_btr_block`), which has no skirt code at all. If its neighbour block holds the edge resident cells, that join is the same chord-above-edge geometry the commit describes.
- **Evidence**:
  ```rust
  // spawn_lod_block: edge belongs to the quad, neighbour looked up by cell, across blocks
  let neighbour_is_full_detail = |dr, dc| { ... cell_is_full_detail(bx0 + nc.div_euclid(spc), by0 + nr.div_euclid(spc), resident_full_cells) };
  // stream_lod_blocks: rebuild only when this block's own mask moved
  let mask = if level == k { block_hole_mask(cells_map, qx, qy, resident_full_cells) } else { 0 };
  Some(m) if m == mask => { missing_blocks.remove(..); continue; }
  ```
  The capture in the commit message / `watal.md` is a static pose at spawn time, which is the one case that works. The unit test covers `append_lod_skirts` geometry only; nothing pins the rebuild key.
- **Disproof attempted**: could another trigger rebuild the neighbour? `desired` only changes with the player's band selection, and the other invalidation path is the same `hole_mask` compare. No neighbour-residency term exists anywhere in the staleness check. A stale skirt after the neighbour *unloads* is harmless (the wall hangs under the neighbouring LOD surface); only the missing-skirt direction is visible.
- **Impact**: the pale-blue sky ribbon along the full-detail / LOD join reappears intermittently while streaming, and on `.btr` joins permanently. Visual only.
- **Related**: `7eac91637`, `docs/engine/watal.md` (2026-10-26 note), #6094.
- **Suggested Fix**: extend the rebuild key with the 16 outer-neighbour cell bits (a second `u16`/`u32` "skirt mask" stored on `LodBlock` and compared with `hole_mask`), and decide on `.btr` boundary blocks either by forcing the synth path when any outer neighbour is resident or by adding the same skirt to `terrain_lod_btr`. Add a pin that moves one cell across a block edge and asserts the neighbour is invalidated.

### EXT-D6-2026-10-26-02: Root cause of #6053 (FNV/FO3 terrain-LOD 384 overlapping pairs) is still in the code
- **Severity**: MEDIUM / **Dimension**: 6 / **Tier Violated**: n/a
- **Game Affected**: FNV, FO3 (authored quad tables with three or more levels)
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:624-633` (`swallowed_by_coarser`), test `:1538` (`authored_selection_partitions_between_levels`)
- **Status**: Existing: #6053 (open). This pass re-verified EXT-D6-2026-10-19-03 against HEAD: unchanged by any commit since (`git log 4a3e89d56..HEAD -- lod_bands.rs` shows only `25f1d78ce`, which touched `refine_within`, not this test).
- **Description**: A finer authored quad is dropped only when a selected quad of exactly `level * 2` contains its centre. With L4/8/16/32 all authored, a selected L32 swallows its L16 children (never selected), so nothing selected at L16 swallows the L8 grandchildren; the same for L16 over L4. The L8/L4 quads are past their own refine thresholds (12 / 18 / 27 cells), so they are selected beside the coarse parent. The partition test still uses a two-level table (4 and 8).
- **Evidence**: arithmetic matches #6053's census exactly: 12 L32 x 16 L8 + 12 L16 x 16 L4 = 384 pairs, e.g. `(8,-64,24) vs (32,-64,0)`.
- **Impact**: double-drawn far terrain after any traversal; `m-exteriors.sh fnv boundary` hard-fails the `lod.coverage` gate on the reference title (red for 8+ days).
- **Related**: #6053, #5222, #5387, #5520.
- **Suggested Fix**: drop a finer quad when *any* selected coarser quad contains its centre (`l2 > level`), and add a four-level tiling case to `authored_selection_partitions_between_levels`.

### EXT-D6-2026-10-26-03: Displaced doc blocks and a displaced `#[allow]` from `7eac91637` and `125a21c2b`; stale premise in the skirt allowance comment
- **Severity**: LOW / **Dimension**: 6 / **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:841-851` and `:136-139`; `byroredux/src/streaming_helpers.rs:835-843`
- **Status**: Existing class: #6373 (eight doc blocks spliced onto the wrong item); these sites are not in its table (`7eac91637` landed after its HEAD `c7463c04d`; `125a21c2b` is not listed).
- **Description**:
  1. `7eac91637` inserted `LOD_SKIRT_DROP` and `append_lod_skirts` between `spawn_lod_block`'s doc comment plus `#[allow(clippy::too_many_arguments)]` and the `fn`. The "Build + spawn one finest-band LOD block..." doc and the `allow` now sit on `LOD_SKIRT_DROP`; `spawn_lod_block` (12 parameters) has neither. rustdoc is wrong, and clippy's `too_many_arguments` would now fire on the function.
  2. `125a21c2b` left the `/// salvage is whether a cancel mid-FinishImports ...` paragraph (it documents `cancel_streaming_apply_job`'s new `salvage` parameter) glued above `salvage_one_queued_import`'s own doc; `cancel_streaming_apply_job` has none.
  3. The new skirt allowance comment (`terrain_lod.rs:136-139`) sizes against "at most 15 x 15 cells (240 edges)". The default streaming radius is 12 (25 x 25 cells = about 400 edges, 2,400 indices); the 4096/8192 allowance still covers it with margin, so no overflow, but the stated premise is the same stale ~radius-7 one #6094/#6095 retired.
- **Impact**: documentation and lint only.
- **Suggested Fix**: move the doc and `#[allow]` back onto `spawn_lod_block` (put the skirt helpers above its doc), move the salvage paragraph onto `cancel_streaming_apply_job`, correct the comment to the radius-12 count.

### EXT-D6-2026-10-26-04: `docs/smoke-tests/README.md` row for `w1-water-traversal.sh` still says Skyrim declares no route
- **Severity**: LOW / **Dimension**: 7 (docs) / **Tier Violated**: n/a
- **Game Affected**: Skyrim
- **Location**: `docs/smoke-tests/README.md:300`
- **Status**: NEW
- **Description**: `ceaf85a54` (2026-10-26) gave Skyrim a measured W1 route (`fixtures/skyrim_se.env` `W1_*`, Tamriel `(-9,-16)`) and rewrote the README paragraph at line ~141, but the table row still reads "`skyrim_se` declares no route and is SKIPped, with the measured reason in its fixture". The two README statements now contradict each other.
- **Impact**: a reader trusts the table and skips a gate that now runs.
- **Suggested Fix**: replace the clause with "`skyrim_se` runs the northern sea shore at Tamriel (-9,-16) (the river banks that wedge the controller are recorded in its fixture)".

## Known-Open Register

| Item | State after this pass |
|---|---|
| Baseline D1-05, D4-01, D4-03, D4-04, D5-06, D6-02 (2026-10-21) | Closed in code, guards live (table in the Executive Summary). D1-05's fix regressed: D1-01 |
| #6089 (FO3 "Use Land Data" children draw no ground) | Fixed by `a9472103b`; its parse-cache half is incomplete: D1-02 |
| #6291, #6309, #6246, #6256, #6290, #6314, #6233, #6311, #6244, #6245, #6310 | Closed in code, guards live |
| #6092 / #6093 / #6094 / #6095 / #6098 / #6332 | Closed in code, guards live (Dim 6) |
| #6053 (FNV/FO3 terrain-LOD 384 overlaps) | **Open.** Root cause re-verified at `lod_bands.rs:624-633` (D6-02) |
| #6106 (full-detail pool refusal never retried) | Open, unchanged (counted only) |
| #3307 (VWD full-model culling) | Open, unchanged |
| #6373 (spliced doc blocks) | Open; two more sites found (D2-01 / D6-03) |
| #6368 (`block_bound` / `world_bound` duplicate) | Open; `block_bound` zips against the `n*n` hole mask, so skirt vertices are excluded from the bound (harmless) |
| #6201 (per-species blade length) | Open; landed `7430cc14c`; wording finding D3-02 |
| #6195 / #6196 / #6197 / #6198 / #6048 | Open epics / design note, unchanged; parallax visuals unconfirmed on a device |
| #6316 (cloud-shadow bake timing) | Open; `994620123` added the `cloud_shadow_ms` bracket (slots 48/49, `QUERIES_PER_FRAME` 60) |
| Cloud-type taxonomy, noise frequencies, atmosphere LUT, upper plane casts no shadow | Documented open (skyal.md §2.3-§3, §2.5); not re-filed |
| Water-walking, freezing, Skyrim DNAM tail, FO76/Starfield scope, `sound_form` consumer | Open per watal.md, unchanged |
| Hide Quad ground-cover gap, collider friction/restitution (#6048) | Unchanged |
| Skyrim tree LOD has no smoke harness (`m-trees.sh` is the pre-Skyrim SpeedTree test) | Gap, not a bug (Dim 7) |

Residual weakness, not filed: `every_direct_sun_site_applies_the_cloud_shadow` is a text-presence scan over a hand-listed file set plus the derived `triangle.frag` arm scan, so a new sun consumer in any unlisted shader passes silently (D4-01 is the live example). Deriving the file list from the `exteriorCloudSunTransmittance` / `sun_dir` uses would close the class.

Observations, not findings:
- The one weather wind byte is converted three ways (`WIND_TO_SCROLL_RATE` for textured cloud layers, `dome.weather_wind` for the deck, `WindField::from_weather_byte` for grass and water). Only the 20 m/s cap and the water calibration are in m/s.
- A NaN normal still gives a NaN UV out of `terrainParallaxDisplaceUV`; that fragment's lighting normal is NaN too, so nothing new fails.
- Duplicated `#[cfg(test)] #[cfg(test)]` also at `world_setup.rs:1517-1518` (cosmetic; the `weather.rs` instance is D4-03).
- `apply_exterior_processing`'s game gate is a bare inline `matches!`; D1-02's suggested fix would make it a table-shaped fn.
- Test-state claim for D5: the vert/frag `WaterParams` declarations differed only by comments/formatting in a quick check that was not fully normalised; the renderer reflect tests pass.

## Appendix A: Skill drift (proposed edits to `.claude/commands/audit-exterior/SKILL.md`)

Not applied: this pass is read-only on the skill. Cross-cutting items first, then the per-dimension lists the agents produced.

- **Add to Dim 5** the bed field, shore/swell law and underwater shaft march (full proposed text in the Dim 5 list below); add `w2-dynamic-water.sh` to Dim 7.
- **Add to Dim 6** the terrain-LOD skirts; **Dim 3** the #6201 blade length and the shared `groundcover_wind.glsl`; **Dim 4** the 20 m/s drift cap and the god-ray site.
- **Stale statements to correct:** Dim 1 "#6089 open" and "bare `GameKind::Oblivion` check"; Dim 1 forced-weather bullet (`applied_forced_weather` was deleted by `fb51f5987`; the funnel is `install_environment` + `InstalledOutdoorEnvironment`); Dim 4 guard pattern is `color_type.w) == 2u`; Dim 7 "the harnesses run `check-shader-artifacts.sh`" is false for `m34-day-night.sh`.
- **Doc rot outside the skill:** `docs/engine/exal.md:144-150` still says "for Oblivion only"; skyal.md lacks the drift cap and the effect-lit sun site; `docs/smoke-tests/README.md:300` (D6-04).

### Dim 1

1. **#6089 "open" is stale.** Bullet "Climate resolution ...": replace "gated by a bare `GameKind::Oblivion` check because the co-located grids were measured only there, so FO3 "Use Land Data" children still draw no ground, open #6089" with "gated to `GameKind::Oblivion | Fallout3NV` (FO3 `Oasis` median |dz| 166 u vs 2,480 for a shifted control, `ParadiseFalls` 46 vs 4,126; `GNRroofWorld`/`MegatonWorld` consistent but not separately proven; #6089, `a9472103b`, census pin `fo3_use_land_data_children_inherit_the_wasteland_landscape`, `--ignored`, 100-120 cells); the `exterior_ready` predicate in `SharedParseCache::get_or_parse` must use the same gate (EXT-D1-2026-10-26-02)". Also "so an Oblivion index parsed for an interior is not served" becomes "an Oblivion or FO3/FNV index ...".
2. **Forced-weather bullet** ("`apply_forced_weather` snaps it ... and the streaming state is seeded with the bootstrap's forced value so an early force is not lost (#6028, `553428296` ...)"): `WorldStreamingState::applied_forced_weather` was deleted by fb51f5987. Replace with: every install goes through `install_environment`, which records `InstalledOutdoorEnvironment { climate, default_weather, forced_weather }`; `forced_weather_changed` compares it with the live `ForcedWeather`; `apply_forced_weather` (exterior, `step_streaming`) and `apply_forced_weather_in_interior` (`step_interior_forced_weather`, #6309, `fb51f5987`) both re-install on a difference and snap with `collapse_weather_transition`. Guards: `a_forced_weather_change_is_a_difference_from_what_was_installed`, `a_forced_weather_waits_for_a_fully_loaded_interior`, `only_the_install_funnel_calls_the_bare_environment_installer`. Add "the interior re-apply must leave `CellLightingRes`/`ImageSpaceBase`/`InteriorSkyExposureRes` untouched (currently broken, EXT-D1-2026-10-26-01)".
3. **"all install through `apply_environment_with_sources`"**: now the funnel is `install_environment` (which calls `apply_environment_with_sources`). Update the "Sole producers" bullet.
4. **`app_step.rs` path entry**: Paths line says `app_step.rs (per-tick XCCM override call)`; add "and `step_interior_forced_weather` (#6309)".
5. **Guard `installed_fo3_statesman_roof_inherits_wasteland_lod`** is `#[ignore = "needs FO3 game data on disk"]`; the skill lists it as a plain guard. Mark it "(`--ignored`)". The live guards are the other two (`lod_grid_origin_of_an_inheriting_child_is_the_ancestors`, `object_lod_paths_of_an_inheriting_child_name_the_ancestor`).
6. **Source-scan pin name**: the skill says `exterior::production_source`; the test calls `crate::cell_loader::exterior_production_source()` (a `#[cfg(test)]` re-export in `cell_loader.rs` of `exterior::production_source`, covering `exterior.rs` + `exterior/{persistent,apply_job,worldspace}.rs`). Fine as written for the split; optionally name the alias.
7. **`climate_override_grid`** is a `WorldStreamingState` field (`streaming/mod.rs:422`), not a function; the skill reads as a symbol "(`climate_override_grid`, #5529)" which is fine, but the `grep -B3 'fn <name>'` Guard check in RULES.md will not find it.
8. **`docs/engine/exal.md:144-150`** (Landscape #5505): "once after the load order is merged and for Oblivion only — the co-located grid was measured there, not on the FO3+ children" is stale after #6089 (Oblivion and FO3/FNV). Doc rot outside the skill; list it for the doc-sync pass.
9. **First step**: callers list `scene/world_setup.rs`, `systems/weather.rs`, `cornell.rs` is still correct; `scene.rs:216` is a `use` re-export (matches the grep, not a call). Add a note so the next pass does not chase it.
10. **Dim 4 / skyal.md**: the 20 m/s `CLOUD_LAYER_MAX_DRIFT_M_PER_S` cloud ground-drift cap (4f2596bf6) is recorded in `watal.md` only; add to the Dim 4 clouds bullet and to skyal.md §2.x.

### Dim 2

Edit `.claude/commands/audit-exterior/SKILL.md` Dimension 2:
1. **First step** - add the renderer leg: `cargo test -p byroredux terrain && cargo test -p byroredux-terrain-height && cargo test -p byroredux-renderer --lib -- terrain` (35 renderer tests carry the tile size/offset/lockstep and parallax/blend guards; the current command never runs them).
2. **Skirts** - `terrain_lod.rs` is outside Dim 2's Paths, but the skirt commit (7eac91637) is not described in any dimension bullet; either add one line to Dim 3 ("`append_lod_skirts` hangs a 640-unit wall under LOD quad edges that border a resident full-detail cell; guard `lod_skirts_hang_below_the_edge_and_face_the_hole`") or note that Dim 2 owns it.
3. **First bullet** - "`spawn_terrain_mesh` + `build_cell_splat_layers` are the one LAND translate site" is now three helpers after #6271: `build_terrain_vertices` (33x33 grid), `cover_density_inputs` (per-lane/base affinity + authored-grass resolution, including the `AuthoredCover::union_density` override that the text currently attributes to `spawn_terrain_mesh`), and `spawn_terrain_mesh` (texture resolve + tile spawn). Replace "`spawn_terrain_mesh` overrides each lane's ..." with "`cover_density_inputs` overrides ...".
4. **Parallax paragraph** - append the three landed guards: "#6346 (`c3b84ec32`): the march's step count is clamped after the float-to-int conversion and all three marchers (`terrainParallaxDisplaceUV`, `parallaxDisplaceUV`, `resolveRayHitUV`) share `parallaxSecantWeight` (`ray_hit.glsl`); guards `degenerate_inputs_keep_the_march_bounded_and_finite`, `every_parallax_march_uses_the_guarded_secant_weight`. #6345 (`94acf0bff`): the terrain base layer's reads use the pre-march `splatUVdx/dy` / `splatPosdx/dy` (`triangle_frag_reads_the_terrain_base_layer_with_the_pre_march_gradients`). #6347 (`6d372a0af`): the banned implicit-derivative tokens are checked in `byroTerrainBlendCore` / `byroTerrainBlendedHeight` too (`glsl_terrain_height_chain_uses_only_explicit_gradients`). Known residual: NaN input still gives a NaN UV (lighting normal is NaN too)." Also note the conversion site by name: `app_frame.rs::push_terrain_parallax`.
5. **#6246** is not mentioned anywhere in Dim 2: add "FO4 MNAM-only base LTEX in the synth LOD path resolves through `LandscapeLayerTextures::texture_set` (`synth_base_texture_path`; guard `synth_block_base_resolves_an_mnam_only_ltex_through_its_bgsm`)".
6. **Height-map pool** - add #6290: cache writes go through `core::atomic_file::atomic_temp_path` + `atomic_write` (unique temp; guard `concurrent_writers_of_one_cache_key_all_succeed_and_readers_see_whole_files`) and #6314's guard name `warm_cache_hit_does_not_decode_the_source`.
7. All other cited symbols/commits (`592f31b87`, `177774890`, `79477b4a3`, `c53d87679`, `5f1a88929`, `6efda8bc4`, guard names, `docs/engine/shader-pipeline.md` § GpuTerrainTile at line 709, 224 B ledger row) still resolve.

### Dim 3

`.claude/commands/audit-exterior/SKILL.md` Dimension 3:
- **Missing coverage for #6201 / 7430cc14c.** No bullet mentions `GroundCoverSpecies::blade_length_mean/cv`, `byroGcBladeLength` or the repurposed species lane. Add to the "Look" bullet:
  "per-blade length is a log-normal around `blade_length_mean` with `blade_length_cv`, clamped to `height_range` (`byroGcBladeLength`, `groundcover_blade.vert`; the GPU species record's former `card_atlas` lane is now `blade_length`, `GpuGroundCoverSpecies` 80 B unchanged; built-ins are `moment_matched_length(height_range)` placeholders, unsourced, `exal-groundcover.md` 12.16, #6201). Guard: `species_collection_carries_the_blade_length_distribution` + the lockstep test."
- **Wind sharing is described only for the model tier.** In the FSR/Look bullets add that the blade ribbons call the same `byroGcWindBendFraction` (`include/groundcover_wind.glsl`), pinned by `groundcover.rs` tests (`wind.contains("baseXZ - windDir")`, `blade.contains("byroGcWindBendFraction(")`), and that `ModelPush` is 96 B (`wind`, `gustTiming`) with `model_push_wind_block_mirrors_the_blade_pass`.
- **Guard location:** `blade_motion_and_fsr_mask_contract_stay_material_driven` is in `groundcover/construct.rs` (the skill gives no path; fine, but note it for the next reader).
- "Determinism" bullet: add "and the model tier draws every per-plant quantity from a distinct `gcModelHash(h, N)` stream (see EXT-D3-2026-10-26-01)".
- Everything else cited (symbols `pending_field_commit`, `promote_field_commit`, `promote_skip_clear_latches`, constants, ledger numbers, 27 guards) resolves and is current.

### Dim 4

Dim 4 bullets in `.claude/commands/audit-exterior/SKILL.md` (lines 64-73). All cited symbols, tests and commits resolve, and no guard is `#[ignore]`d. Items to correct:
1. Bullet "Weather -> sun" says the guard "scans every `color_type.w == 2u` arm in `triangle.frag`". The real pattern is `color_type.w) == 2u` (`uint(lights[i].color_type.w) == 2u`). Corrected text: "scans every `color_type.w) == 2u` arm".
2. Same bullet: add `composite.frag`'s underwater god-ray consumer (`296d21912`) to the list of direct-sun sites, noting it is NOT yet covered (EXT-D4-2026-10-26-01), or add it once fixed. Add: "the guard does not scan `composite.frag`."
3. "Clouds" bullet: add the textured-layer drift cap: "`cap_cloud_layer_drift` bounds each textured layer to `CLOUD_LAYER_MAX_DRIFT_M_PER_S` (20 m/s over the 9 km shell, `weather.rs`, `4f2596bf6`, `cloud_drift_cap_tests::fast_layers_are_capped_and_slow_or_unused_layers_are_not`); a chosen calibration, not authored; the deck's own wind advection (`cloud_wind_offset`) is separate and untouched." Only watal.md records it today.
4. "First step" / perf: `cloud_shadow_ms` nested bracket (#6316, `994620123`, `QUERIES_PER_FRAME` 60) is not mentioned in Dim 4. Optional one-liner: "the bake has its own nested GPU bracket `cloud_shadow_ms` inside `sky_cube_ms`".
5. First step: `cargo test -p byroredux-renderer sky_` also catches the cloud-shadow guards only through the module path (`sky_cube::tests::...`); `cloud` is a useful second filter. Suggest `cargo test -p byroredux-renderer --lib -- sky_ cloud` (54 tests today; the 1 ignored is the GPU test).

### Dim 5

The Dim 5 section of `SKILL.md` (lines 75-83) predates the 2026-10-26 water work and says nothing about the bed field, the shore/swell law, the river-over-bed rule or the underwater shaft march. All the symbols it does cite still exist: 14 guards, `compose_flow_scrolls`, `watr_angle_to_engine_xz`, `water_damage_system`, `resolve_distant_water`, `default_water_for_worldspace`, `lod_water_rebuild_due`, `LOD_WATER_MAX_DEFER`, `water_volume_depth`, `water_material_from_mesh` and `water_volume_from_phantom`.

1. **Paths line.** Append: `crates/core/src/ecs/components/water/bed_field.rs`, `crates/physics/src/water.rs` (crest sampler and buoyancy), `crates/renderer/shaders/water.vert`, `water.frag` and `composite.frag` (`underwaterGodRays`).
2. **Add a bullet after the "Mesh water depth" bullet.** Proposed text:
   > Bed field (`7dd294ee3`, `8647c1128`, `4a56fd9c7`, `53df201ea`): the LAND heightfield under a cell plane is sampled once by `bed_field_from_land` (`cell_loader/water.rs`) into a `WaterBedField` (core `water/bed_field.rs`: signed depth plus shoreward direction, bilinear). The same field is (a) baked into the water mesh's vertex colour behind the exact sentinel alpha `WATER_BED_VALID_ALPHA` (`bed_vertex_color`; `r` = depth / `WATER_BED_DEPTH_MAX`, `gb` = the shoreward direction; border vertices carry no direction) for `water.vert` / `water.frag`, and (b) attached as the `WaterBedField` component for physics. Any second derivation of depth or shore direction is a finding. The shape law has one owner in core: `WaterSwell::amplitude_over` (shoal and breaking cap), `WATER_SHORE_REFRACT_*`, `WaterFlow::over_bed` / `WATER_RIVER_*` (river current over the bed, shared by `water.frag` and the buoyancy current). `water.vert` and `authored_wave_height_over_bed` mirror it, so a retyped literal in either is a finding (open: 0.6 / 0.5 / 0.1 tuning literals, EXT-D5-2026-10-26-02). Mesh grid: LAND lattice (`LAND_BED_SEGMENTS` 32 -> 128 BU) over LAND, 16 segments without. Guards: `the_mesh_bakes_bed_depth_and_the_shoreward_direction_into_vertex_colour`, `the_physics_bed_field_agrees_with_the_baked_vertices` (depth only), `swell_shore_and_river_shaders_keep_their_contract` (`vulkan/water.rs`), `river_current_on_a_floating_body_follows_the_bed` (physics), `swell_follows_the_wind_shoals_and_breaks` (core).
3. **Add a bullet for the underwater shafts.**
   > Underwater god rays (`296d21912`, `99475136b`, `baf2b78fd`): `composite.frag::underwaterGodRays` is a 24-step march. Camera depth below the surface and the fog span arrive in `UnderwaterView.surface` -> `caustic_flags.zw`. Its art values (focus scales, ridge exponent, energy factor, HG `g`) are chosen, not authored (watal.md). The refraction index must read the authored plane IOR, not a literal (EXT-D5-2026-10-26-01, #4010 class).
4. **"First step" line.** Add `cargo test -p byroredux-renderer --lib -- water` and `cargo test -p byroredux-core water`. The bed/swell/river guards live in those two crates, plus `cargo test -p byroredux-physics water`.
5. **Dim 5 "Frames" bullet.** The skill's "weather transport damp ... calm keeps it" wording is now also governed by `4f2596bf6` / `8dc470d44` (calm water drifts with the weather wind, +50%). Add one clause: the calm drift constant is `WEATHER_SCROLL_PER_BU_PER_S` in core.
6. **Dim 7.** It names `m-exteriors.sh ... water` and `w1-water-traversal.sh` but not `docs/smoke-tests/w2-dynamic-water.sh`. That script was edited by `7dd294ee3` (the gate asserts only floats are awake on wind-driven water) and is indexed in `docs/smoke-tests/README.md`. List it, or point at the README.
7. **Minor, outside the skill.** The module doc at `crates/core/src/ecs/components/water.rs:33-36` says per-game WATR parsing "uses the `GameKind` axis already plumbed there". That is a doc-only mention and not a code violation. It is worth a one-line check by whoever next edits that header.

### Dim 6

`.claude/commands/audit-exterior/SKILL.md`:
1. Dimension 6, bullet 2: add the skirt behaviour. Suggested text after the `.btr`/synth sentence: "Synth blocks hang a `LOD_SKIRT_DROP` (640) wall under every quad edge that borders a resident full-detail cell (`append_lod_skirts`, `7eac91637`, `lod_skirts_hang_below_the_edge_and_face_the_hole`); the skirts are synth-only and their cross-block neighbours are not part of the rebuild key (EXT-D6-2026-10-26-01)."
2. Dimension 6, bullet 1 (the `#5387`/`#5520` selector passage): append "`select_authored_lod_quads` still drops a finer quad only for an exactly-one-level-coarser parent (`swallowed_by_coarser`, `l2 == level * 2`), the root cause of open #6053 (EXT-D6-2026-10-19-03); `authored_selection_partitions_between_levels` is two-level only."
3. Dimension 6, bullet 1: `an_abandoned_group_leaves_no_hole_and_admission_stays_closed` lives in `crates/renderer/src/mesh/geometry_ssbo.rs`, not `byroredux`; say so, since `cargo test -p byroredux lod` does not run it.
4. Dimension 7, last bullet: "The harnesses run `scripts/check-shader-artifacts.sh` before capture (#4738)" holds for `renderer-eval-groundcover.sh` (line 97), `m-exteriors.sh` (110) and `w1-water-traversal.sh` (97) (and `w2-dynamic-water.sh`), but **not** `m34-day-night.sh`, which captures no frame (CPU sun-value gates only). Reword: "the capturing harnesses (groundcover, m-exteriors, w1, w2) run ...; `m34-day-night.sh` gates CPU values and does not".
5. Dimension 7, second bullet: `m-exteriors.sh` `lod.coverage` gate now includes `held_back` (zero required, #6093); `m-trees.sh` is the SpeedTree (pre-Skyrim) smoke, not the Skyrim `.btt` tier, so Skyrim tree LOD has no harness coverage (a gap, not a bug).

No cited symbol, path or commit in either dimension was found missing.

## Appendix B: Per-dimension evidence

### Dim 1: Dim 1 summary (EXAL boundary discipline, HEAD 3e6f6b66a, baseline 4a3e89d56)

**Commits since baseline touching Dim 1 Paths** (`git log 4a3e89d56..HEAD -- <Paths>`): fb51f5987 (#6309), a9472103b (#6089), 1d383cc5a (#6271 split), 4f2596bf6 (wind-driven water + cloud drift cap), 9634aa362 (#6088), 635330d08 (#6092), dcad3669d (#6135), plus unrelated touches (4cfcee4b9, 3e4bfab51, 335994b4c, a99b2f461, 0539e07f8, cafd3ccab, 051179b7b). I read the diffs of the first six in full for these Paths. #6088, #6092 and #6135 are Dim 6 / Dim 5 (one-line parameter threading in `exterior/apply_job.rs`, `load.rs`, `world_setup.rs`) with no Dim 1 content.

**First-step grep** (`translate_sky|translate_weather|translate_exterior_cell_lighting|procedural_fallback_` outside `env_translate/` and tests): production callers are only `scene/world_setup.rs` (482, 518, 572, 1117, 1164, 1169), `systems/weather.rs:334` (`apply_neutral_exterior_fallback`) and `cornell.rs:607-608`. Every other hit is a doc comment or sits inside `#[cfg(test)]`: `render/sky.rs:484` (test mod starts at 264), `world_setup.rs:1584,1623` (tests start at 1519), `weather.rs:2511+`. No second `translate_*` site.
**Literal grep** (`SkyParamsRes {` / `WeatherDataRes {`): every hit outside `env_translate/` and `components/environment.rs` is in a test module, except `systems/weather.rs:1388`, which is #4733's `let &WeatherDataRes {` destructure. No second construction site.

**`promote_weather_transition_target`**: no `WeatherDataRes` field has been added since baseline (`git diff 4a3e89d56..HEAD -- components/environment.rs` only adds `InstalledOutdoorEnvironment`, which is not a `WeatherDataRes` field). The promotion is still an exhaustive `let &WeatherDataRes { .. }` destructure of all 15 fields with no `..`, so a new field is a compile error. Clean.

**Funnel**: `apply_environment_with_sources` is now called only from `install_environment` (`world_setup.rs:278`). `only_the_install_funnel_calls_the_bare_environment_installer` pins it. `a_forced_weather_replaces_the_default_in_every_install`, `the_environment_installer_flushes_the_sky_textures_it_queues` and `only_a_seen_weather_cross_fades` are live plain `#[test]`s.

**Cargo**: `cargo test -p byroredux --bin byroredux -- env_translate climate default_weather forced fog weather exterior` -> **332 passed, 0 failed, 5 ignored** (3057 filtered), default `target/`, `TMPDIR=/mnt/data/tmp`. The 5 ignored are real-data tests, for example `installed_fo3_statesman_roof_inherits_wasteland_lod`. No test failed, but the two findings below are not caught by any of the 332 (see Evidence).

**D1-05 verification**: the #6309 fix (`install_environment` + `InstalledOutdoorEnvironment` + `step_interior_forced_weather` -> `apply_forced_weather_in_interior`) does *reach* both Show-Sky interiors (re-runs `apply_show_sky_interior_climate`) and ordinary interiors' window-portal palette (re-installs from the recorded climate and default weather). But the re-install is destructive for the interior itself: see EXT-D1-2026-10-26-01. D1-05 is functionally closed (the sky changes) but the fix introduced a regression.

### Dim 2: Dim 2 summary

**Delta read** (diffs read, not just subjects): b81a43443 (#6197 parallax), c3b84ec32 (#6346), 94acf0bff (#6345), 6d372a0af (#6347),
1d88bfe85 (#6271 pure extraction: `build_terrain_vertices` + `cover_density_inputs`; no behaviour change found), 85ef6efc3 (#6256),
6528e110b (#6246), aa10b9bd6 (#6314), d9ddbe5d5 (#6290), 04e146b1d (#6233), 86f9f6689 (#6352 prose), 3a0f1e8ca (#6392, parallax clamp
names only; terrain `minSteps/maxSteps` literals untouched and still pinned), 7eac91637 (terrain-LOD skirts; file is `terrain_lod.rs`,
outside the Dim 2 Paths, covered here because the brief named it).

**Tests (all green, repo default target dir):**
- `cargo test -p byroredux terrain`: bin unit tests **103 passed, 0 failed, 3 ignored** (the 3 need vanilla game data on disk:
  `dcworld03_terrain_diffuse_quads_index_off_lattice`, `default_land_textures_exist_in_vanilla_archives`,
  `oblivion_ltex_paths_exist_in_vanilla_archives`); every integration target 0 matched.
- `cargo test -p byroredux-terrain-height`: **11 passed, 0 failed**.
- `cargo test -p byroredux-renderer --lib -- terrain`: **35 passed, 0 failed, 0 ignored** (includes `gpu_terrain_tile_is_224_bytes`,
  `gpu_terrain_tile_field_offsets_match_shader_contract`, `shipped_triangle_frag_spv_carries_the_terrain_tile_stride`,
  `memory_budget_ledger_pins_the_terrain_tile_row`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`, all of
  `terrain_parallax_tests` / `terrain_height_blend_tests`).
- `bash scripts/check-shader-artifacts.sh` (read-only, mktemp scratch): 38 shaders + early variant byte-identical to glslang 16.4.0.

**Guards named in the skill - all live** (plain `#[test]`, no `#[ignore]`, still asserting): `gpu_terrain_tile_is_224_bytes` (asserts 224),
`gpu_terrain_tile_field_offsets_match_shader_contract` (offsets 0/32/64/96/112/128/136/140/144/160/164/168/172/204/208/212/216/220),
`gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`, `named_splat_offset_reads_keep_their_unorm_recovery`,
`terrain_blend_stays_off_disk_until_the_slider_is_edited`, `a_panicking_height_job_is_reported_failed`,
`btxt_transition_cell_edges_stay_hard_vanilla_parity`, `land_tile_indices_face_up`, `land_tile_collider_grounds_from_above`,
`the_footstep_probe_reads_the_terrain_material`, `groundcover_affinity_composes_the_base_in_diffuse_loop_order`,
`glsl_blend_fades_each_lane_with_its_share`, `groundcover_density_sites_read_the_painted_splat`.

**Rust/GLSL lockstep (verified):** `GpuTerrainTile` is declared in exactly one GLSL file (`include/bindings.glsl:492`); every other shader
(`triangle.frag`, `material_sampling.glsl`, `groundcover_blade.frag`, `terrain_sample.glsl`) reaches it by `#include`. Field-for-field
match with `gpu_types.rs` incl. the four #6197 lanes (`parallaxDepth/FadeStart/FadeEnd/Reserved` at 208..224); std430 stride 224 is
carried by the shipped `triangle.frag.spv` (pinned). The "5 standalone copies" rule (memory `feedback_shader_struct_sync`) is about
`GpuInstance`, not the terrain tile: `GPU_INSTANCE_GLSL_MIRRORS` = bindings.glsl + 5 standalone (`triangle.vert`, `ui.vert`, `water.vert`,
`caustic_splat.comp`, `groundcover_models.comp`); `gpu_instance_glsl_copies_stay_in_lockstep` + `assert_mirror_list_is_complete` pass
and `skin_vertices.comp` / `volumetrics_inject.comp` only mention the struct in comments. No drift.

**Parallax march (`terrainParallaxDisplaceUV`, `material_sampling.glsl`):** loop bound `steps` is `clamp(int(...), 6, 20)` after the
conversion (#6346, pinned by `glsl_parallax_uses_only_explicit_gradients` + `degenerate_inputs_keep_the_march_bounded_and_finite`), so no
unbounded loop. All three marchers share `parallaxSecantWeight` (`ray_hit.glsl`): finite for every finite input (guard
`every_parallax_march_uses_the_guarded_secant_weight`). Fade branch handles `end <= start` and `inf` ranges; `relief <= 0` early-out;
`det`/`g11*g22` degenerate-quad early-out; `max(vz, 0.05)` bounds the lateral slide; `byroTerrainBlendCore` divides only by
`heightShare > 1e-6`, `total` (>= fade_top*max(band,1e-4) > 0) and `composed > 1e-6`. Residual (not a finding): NaN *input*
(`normalize(fragNormalEffective)` of a zero interpolated normal) still yields a NaN UV out of the helper, since the clamp bounds the loop
but nothing sanitises `planarSlide`; that fragment's lighting normal is NaN as well, so the helper adds no new failure. Unconfirmed on
screen (shader not run on a device, as the #6346 commit itself states).

**World-unit conversion is single-site:** `BETHESDA_UNITS_PER_METER` is applied to the parallax settings only in
`byroredux/src/app_frame.rs::push_terrain_parallax` (called from `render_one_frame`); `VulkanContext::set_terrain_parallax`
(`context/resources.rs:217`) takes world units and writes them to every live tile; `terrain.rs:1485` seeds new tiles with 0 and the
per-frame broadcast fills them (same one-frame-lag pattern as the #6196 band, documented at the site). Settings are registered sliders
(range 0..60 m range; validated by the registry), so inf/NaN depth cannot reach the GPU through the UI or `terrain.parallax`.

### Dim 3: Dim 3 summary: Ground-cover pipeline (HEAD 3e6f6b66a, baseline 4a3e89d56)

**Changed since baseline (Dim 3 paths):** 96ab07189 (#6291 field flip staged, promoted after submit), 1ca5032ae (shared
`include/groundcover_wind.glsl`, model-tier foliage lean, `ModelPush` 64 -> 96 B, `GROUNDCOVER_MODEL_RECORD_FLAG_SWAYS`),
7430cc14c (#6201 `blade_length_mean/cv`, `byroGcBladeLength`, `card_atlas` lane repurposed to `blade_length`),
b81a43443 (terrain parallax: `GpuTerrainTile` 208 -> 224 B, rebuilt `groundcover_blade.frag.spv`), 6181e233f (rebuilt blade frag spv only),
3a0f1e8ca / c3b84ec32 (non-groundcover constants, no `GROUNDCOVER_*` change), 3e6f6b66a (frame/driver split).
Everything else in the path-filtered log is water/sky and out of Dim 3.

**Read:** the diffs of 1ca5032ae, 7430cc14c, 96ab07189 wiring (`post_passes.rs`, `frame.rs`, `draw.rs`), the b81a43443 / 3e6f6b66a stats,
`groundcover_models.comp` (pick / emit), `groundcover_blade.vert` (length draw, wind), `exal-groundcover.md` 12.16, `memory-budget.md` rows.

**Tests**
- `cargo test -p byroredux-renderer groundcover`: **71 passed, 0 failed, 0 ignored** (1424 filtered).
- `cargo test -p byroredux groundcover`: **68 passed, 0 failed, 1 ignored** (`installed_fo4_worldspaces_resolve_their_climate`, needs FO4 data); other bins 0 matched.
- Extra: `cargo test -p byroredux-renderer --lib vulkan::context`: 239 passed, 0 failed (covers the 3e6f6b66a source-shape pins).
- `scripts/check-shader-artifacts.sh`: 38 shaders + early variant match glslang (no stale spv after the 7430cc14c / b81a43443 / 6181e233f rebuilds).

**Guards:** all 27 `Guard:` names in the skill resolve to a plain `#[test]` (only `installed_fo4_worldspaces_resolve_their_climate` is `#[ignore]`, as documented).
`blade_motion_and_fsr_mask_contract_stay_material_driven` now lives in `groundcover/construct.rs`.

**Checklist from the brief**
- *New bare literals in `groundcover_*` GLSL:* the delta adds `1.0e-6` (Box-Muller `log` guard) and seed-stream ids `13u` (blade length) / `7u` (model wind phase).
  Both classes are already bare in these files (`1.0e-3`/`1.0e-4` guards, `0u..12u` stream ids), so I do not file them. Everything else in 1ca5032ae / 7430cc14c routes
  through `GROUNDCOVER_*` / `byroGcWindBendFraction` (shared by blade and model tier, no private copy). One pre-existing bare tau is filed as D3-03.
- *Per-species blade-length constants outside `shader_constants_data`:* none in GLSL (values flow through the species record lanes). The only literal is
  `0.288_675_13` (`1/sqrt(12)`) inside `moment_matched_length` in core, a named math constant with a doc comment. Clean.
- *Push-block lockstep:* `GcModelPush` (`wind`, `gustTiming`) vs `ModelPush` (96 B, `host_mirrors_match_the_shader_strides`) and `GroundCoverSpecies.bladeLength` vs
  `GpuGroundCoverSpecies.blade_length` (five vec4, 80 B, unchanged size) are covered by `name_diverging_glsl_rust_mirrors_stay_in_lockstep` (passes). The `gust_timing` mapping
  `[gust_and_timing[0], origin_time[3], gust_and_timing[1]]` equals the blade shader's `GC_GUST_FREQUENCY / GC_TIME / GC_PREV_TIME` (`groundcover_blade.vert:99-101`).
- *Clump-normaliser rule:* no clump constant changed (`GROUNDCOVER_CLUMP_MEAN` 0.468 untouched; `the_clump_mean_constant_matches_the_measured_field` and
  `clumping_preserves_the_average_density_per_cell_area` pass). The clump height scale still multiplies on top of the new length draw.
- *Determinism (wind/lean):* the model lean is a pure function of `(point.position, h = point.meta.w, wind push, the two wind-clock samples)`; `h` is the frame-invariant candidate hash. No frame serial
  enters. The blade length draw uses seed streams 0 and 13 of the frame-invariant blade seed. `groundcover_seed_hashes_are_integer_and_frame_invariant` passes. Clean.
- *FSR / motion vectors:* the model tier writes `gcPreviousModels[slot] = placementPrev * shape.local` with the previous wind-clock sample, so motion vectors carry the sway
  (`foliage_lean_uses_the_shared_wind_and_keeps_motion_vectors_honest`); a calm sky returns the identity; submerged plants do not lean. Blade reactive/T&C contract unchanged.
  Not confirmed on screen (no engine launch allowed).
- *Arena budget ledger:* still true. `GpuGroundCoverSpecies` stays 80 B; `ModelPush` is a push constant (no VRAM). `memory-budget.md` rows 1154/1155 (134,831,492 B = 128.6 MiB,
  17,418,288 B = 16.6 MiB) are asserted by `memory_budget_ledgers_the_sky_and_ground_cover_owners`, which passes. 96 B push < 128 B Vulkan minimum guarantee.
- *Frame/driver split (3e6f6b66a):* groundcover is still wired: `record_scatter` / `record_groundcover_models` / `record_groundcover_bench` in `dispatch_skin_and_cluster.rs`,
  `draw_groundcover_models` in `geometry_pass.rs`, `discard_pending_field_commit` in `discard_pending_skip_clear_latches` and `promote_field_commit` in `promote_skip_clear_latches`
  (`post_passes.rs:121-134`), and the model tier still records after the final instance grow (`build_and_upload_instances.rs:2560` pin). All pins green.
- *Terrain parallax (b81a43443):* the blade fragment's ground colour still goes through the shared splat chain (`blade_ground_colour_uses_the_shared_terrain_splat_chain` passes);
  parallax is applied only in `triangle.frag` (UV displacement), not to blades, so there is no relief/colour disagreement to find. `GpuTerrainTile` 224 B has no standalone copy in any `groundcover_*` shader.

### Dim 4: Dim 4 summary

Sky, weather, sun. Baseline `docs/audits/AUDIT_EXTERIOR_2026-10-21.md` (4a3e89d56) -> HEAD 3e6f6b66a (2026-10-26).

**Changed since baseline (Dim 4 paths):** `e7eb57a3b` (#6244), `7180d3109` (#6245), `6181e233f` (#6310), `994620123` (cloud-shadow GPU timer, part of #6316), `acab976c1` (test-only: two source-scan tests re-pinned to the `sampled_image_info` helper), `4f2596bf6` (cloud drift cap, plus water; the water half is Dim 5), `3e6f6b66a` (touches only `cloud_noise.rs`, a size-pin refactor with no behaviour change). The other 2026-10-26 `shaders/` commits (dc8091139, 16df8e6a6, ddaf122df, 4427ce44f, baf2b78fd, 99475136b, 296d21912, 067ce38ef, 7dd294ee3, e4e85a3cc, 04faabca2) are water or composite-underwater. None touch `sky.glsl`, `clouds.glsl`, `sky_cube.comp` or `cloud_shadow.*`. `296d21912` / `067ce38ef` add a new sun consumer, see EXT-D4-2026-10-26-01.

**Read:** the diffs of the commits above; current `triangle.frag` effect-lit arm (1180-1198), `bindings.glsl::exteriorCloudSunTransmittance`, `cloud_shadow.comp`, `include/cloud_shadow.glsl`, `post_passes.rs` (`caustic_sun_transmittance`, `volumetric_sun`), `sky_cube/cloud_shadow.rs` sampler, `gpu_timers.rs`, `systems/weather.rs` (`cap_cloud_layer_drift` + its call site), `composite.frag` underwater god rays, skyal.md §2.5 and the watal.md 2026-10-26 entry.

**Baseline fixes re-verified (all FIXED, guards live):**
- EXT-D4-2026-10-21-01 (effect-lit sun site): `triangle.frag:1194-1195` multiplies by `exteriorCloudSunTransmittance(fragWorldPos, Ldir)`. FIXED. The guard `every_direct_sun_site_applies_the_cloud_shadow` (`sky_cube.rs:1265`, plain `#[test]`, passes) now derives every `color_type.w) == 2u` arm in `triangle.frag`. It asserts the arm list is non-empty (not vacuous) and requires the call before the arm's `break;`. The scan finds exactly one arm today (line 1185). `triangle.frag.spv` and `triangle_early.frag.spv` were rebuilt in the same commit.
- EXT-D4-2026-10-21-03 (interior caustics): `caustic_sun_transmittance(is_exterior, column)` (`post_passes.rs:75`) returns 1.0 indoors, called at `post_passes.rs:569`. FIXED. The guard `caustic_sun_transmittance_applies_the_cloud_column_only_outdoors` is a plain `#[test]` and passes. It also asserts the call-site source text passes `sky_params.is_exterior`.
- EXT-D4-2026-10-21-04 (cloud-shadow map edge): the bake blends the outer tenth to `cloud_shadow_overcast` (`cloud_shadow.comp:104-114`, `cloud_shadow_edge_weight`). The lookup is a bare `textureLod(...).r`, and the sampler is `clamp_sampler` (CLAMP_TO_EDGE). FIXED. The guard `the_cloud_shadow_edge_fades_to_the_overcast_term_not_to_unshadowed` is live and passes. It is a source-text scan (see the residual guard weakness in the Known-open register; not a new finding).

**994620123 (cloud-shadow timer):** `Q_CLOUD_SHADOW_START/END` = 48/49, `BIT_CLOUD_SHADOW`, `QUERIES_PER_FRAME` = 60 (matches the skill and the audit-renderer text), nested in `sky_cube_ms`, listed in `NESTED_GPU_BRACKETS`, and `BENCH_GPU_KEYS` was appended last. Clean. The doc numbers (0.21-0.23 / 0.034 ms) are in skyal.md:252. **acab976c1:** test-only, clean.

**4f2596bf6 (sky half):**
- `CLOUD_LAYER_MAX_DRIFT_M_PER_S` = 20.0 is a named, documented `const` in `systems/weather.rs:699`. The altitude comes from `shader_constants::CLOUD_UPPER_PLANE_ALTITUDE`, not a retyped 9000. The test literal `9000.0` is test-only.
- The formula matches the shader: `sky.glsl:313` `uv = shell_uv * tile_scale + params.xy` with `shell_uv` in shell-altitude units, so metres/s = `scroll / tile_scale * 9000`. `tile_scale` is the same `SkyParamsRes::cloud_tile_scale*` that `pack_sky_dome` uploads (no intermediate rescale). The cap runs after the `SkyParamsRes` write in the same `weather_system` pass, so it sees the current frame's scales.
- Locks: `CloudSimState` write plus `SkyParamsRes` read inside one system; `SkyParamsRes` is already declared `writes_resource` in `boot/schedule/early.rs:149`, so the access declaration covers it. Only the weather system writes either.
- No second textured-layer scroll site: `cloud_scroll_vectors` is the only producer, `advance_cloud_scroll` the only consumer, and `render/sky.rs` copies the accumulators through. Guard: `cloud_drift_cap_tests::fast_layers_are_capped_and_slow_or_unused_layers_are_not` (plain `#[test]`, asserts heading preserved, slow layer untouched, untextured layer skipped).
- Volumetric deck wind advection is a separate path by design (`cloud_wind_offset` in `clouds.glsl:194`, shared with `cloud_shadow.comp` so the shadow drifts with the drawn deck). The bare `0.00002` / `0.00008` there are the open cloud-noise frequencies that skyal.md §2.3-§3 documents, so do not re-file.
- Observation, not a finding: the one weather wind byte is converted in three places: `WIND_TO_SCROLL_RATE` (textured layers), `dome.weather_wind` (deck), `WindField::from_weather_byte` (grass and water, including the new wind-driven water). They are consistent in direction (same `wind_direction`) but each has its own speed scale, so there is no single wind-to-m/s definition. Only the 20 m/s cap and the water 2-25 cm/s calibration are expressed in m/s.
- **GLSL game tokens:** `grep -iE` for game/kind tokens across `sky.glsl clouds.glsl sky_sh.glsl sky_cube_direction.glsl medium_transport.glsl cloud_shadow.glsl sky_cube.comp sky_prefilter.comp sky_irradiance.comp cloud_shadow.comp` hits only comments (`sky.glsl:143,440,522-523`, `cloud_shadow.comp:15-16`) describing authored values. No branch keys on a game. Clean.

**Tests (HEAD, default `target/`, `TMPDIR=/mnt/data/tmp`):**
- `cargo test -p byroredux-renderer --lib -- sky_ cloud`: **54 passed, 0 failed, 1 ignored** (the ignored one is `sky_cube::filter::tests::gpu_filter_preserves_constant_radiance_and_broadens_a_lobe`, a Vulkan-device test; not run per brief).
- `cargo test -p byroredux weather`: **97 passed, 0 failed, 0 ignored** (bin target; the other 6 targets match 0 tests).
- All 24 named guards in the skill's Dim 4 bullets exist, are plain `#[test]`, and none is `#[ignore]` (checked by name and attribute): `every_sky_cube_consumer_gates_on_the_ready_flag`, `sky_visibility_pin`, `composite_does_not_carry_its_own_copy_of_the_sky`, `the_cloud_density_profile_uses_radial_altitude`, `the_cloud_march_steps_adaptively`, `every_cloud_layer_samples_with_its_own_lod`, `cloud_layers_composite_far_to_near_over_the_celestial_sky`, `chapman_column_matches_numerical_integration`, `the_cloud_field_is_world_anchored`, `creation_cloud_slots_skip_layers_disabled_by_nam1`, `fo4_jnam_alphas_use_the_eight_slot_stride`, `parse_wthr_oblivion_cloud_colours_come_from_nam0_groups_2_and_9`, `sunlight_dimmer_translates_from_the_hnam_block`, `consumer_multiplies_sunlight_and_grass_dimmers_without_a_transition`, `promotion_carries_the_dimmers_and_the_steady_state_uses_them`, `exterior_upload_is_the_sun_above_the_clouds`, `caustic_sun_transmittance_applies_the_cloud_column_only_outdoors`, `the_portal_sun_is_the_sun_above_the_clouds`, `sun_arc_eases_between_the_two_climates_arcs`, `the_published_breakpoints_ease_with_the_crossfade`, `stale_exterior_sky_params_res_does_not_leak_into_interior`, `night_sky_details_follow_the_outdoor_palette_not_the_weather_flag`, `interior_sky_cube_packs_the_outdoor_palette_like_an_exterior_composite`, `tod_keys_hold_sunset_through_the_suns_descent`. Every symbol the bullets cite (`SKY_SH_COEFFICIENTS`, `cloud_column_threshold`, `compute_directional_upload`, `select_creation_cloud_layers`, `CLOUD_SHADOW_MAP_SIZE/EXTENT`, `pack_sky_dome`, `build_tod_keys`, `ensure_game_time`, `translate_weather`, `CLOUD_UPPER_PLANE_ALTITUDE`) still resolves.

### Dim 5: Dim 5 summary

**Scope.** Water translation (WATAL), delta `4a3e89d56..HEAD` (HEAD `3e6f6b66a`, 2026-10-26). About 40 water commits touch the Dim 5 Paths plus `water.vert` / `water.frag` / `composite.frag`, `vulkan/water.rs`, `crates/physics/src/water.rs` and `docs/engine/watal.md`.

**What changed since baseline.**
- `7dd294ee3`: new `WaterBedField` (`crates/core/src/ecs/components/water/bed_field.rs`). It also adds the calm-water `WaterSwell` (shoaling, shore refraction, breaking cap) and Rapids/River bed-aware current.
- `8647c1128`: `WaterFlow::over_bed` is shared by `water.frag` and the buoyancy current force.
- `4a56fd9c7`: water mesh uses `LAND_BED_SEGMENTS = 32`, i.e. LAND's 128-unit lattice.
- `53df201ea`: shared cell-border vertices drop the baked shoreward direction.
- `4427ce44f` / `ddaf122df` / `16df8e6a6` / `dc8091139`: shore merge, caustic deposit and edge-on faces, all in GLSL.
- `296d21912` / `99475136b` / `baf2b78fd` / `067ce38ef`: underwater god rays, now a 24-step march in `composite.frag::underwaterGodRays`.
- `4f2596bf6` / `8dc470d44`: calm water drifts with weather wind.
- `dcad3669d` (#6135, volume depth), `c2939a24f` (#6391) and `3a0f1e8ca` (#6392): constants named.

**Baseline finding EXT-D5-2026-10-21-06 is FIXED.**
- `5a1e98589` (#6311) re-pointed the row. `docs/engine/watal.md:951` now reads `SNAM` -> `WatrRecord::sound_form` (#6045), a remapped FormID, `SOUN` on Oblivion/FO3/FNV and `SNDR` on Skyrim+, "consumer pending".
- `grep surface_sound crates byroredux` finds no `WatrRecord` field. The only hit is the unrelated `AudioConfig.surface_sounds` in `systems/audio.rs`.

**Checks requested.**
- **Third production `WaterMaterial { .. }` site: none.**
  - Production sites are `env_translate/water.rs:637` (`WaterMaterial::default()` plus mutation, `resolve_water_material`) and `material_translate.rs:223` (`water_material_from_mesh`).
  - Every other hit sits after a `#[cfg(test)]` boundary. I checked `cell_loader/water.rs` (test module from line 1408), `systems/water.rs` (from 672), `systems/character.rs` (from 1801), `commands/water.rs` (from 311), `crates/physics/src/water.rs` (from 1551) and `crates/core/.../water.rs` (from 1222).
- **`WATER_WAVE_*` / `wave_scrolls` single owner: holds.**
  - `water.vert` reads the generated header (`shader_constants_data.rs` re-exports the core `pub const`s). The render upload (`render/water.rs:219`), `commands/water.rs:212` and the CPU crest sampler (`physics/water.rs:565`) all call `WaterMaterial::wave_scrolls`.
  - No retyped wave literal in those three. The only literal is the shore-refraction one in finding 02 below.
- **`weather_scroll_for` single owner: holds.** Callers are `render/water.rs:215`, `physics/water.rs:1149`, `systems/water.rs:237`, `systems/character.rs:1318` and `commands/water.rs:212`. `FLOWING_WATER_WEATHER_TRANSPORT` is referenced only in core and in tests.
- **Bed field is one source of truth, with two caveats (findings 01 and 03).**
  - `bed_field_from_land` (`cell_loader/water.rs:183`) is the only derivation. It feeds the mesh bake (`build_full_detail_water_grid`, line 244, `bed_vertex_color`) and the physics `WaterBedField` component (line 621, attached to the primary plane and every pond component entity).
  - CPU consumers are `authored_wave_height_over_bed` and `WaterFlow::over_bed`. The latter is called from `physics/water.rs:1237-1240` and mirrored in `water.frag:841-856`.
  - The shared shape constants (`WATER_SWELL_*`, `WATER_SHORE_REFRACT_*`, `WATER_RIVER_*`) live in core. `shader_constants_data.rs` re-exports them (8647c1128 moved the river ones to core).
  - `swell_shore_and_river_shaders_keep_their_contract` pins the vert/frag text. `the_physics_bed_field_agrees_with_the_baked_vertices` pins depth only (see 03).
- **`crates/physics` stays game-invariant.** `grep -rn GameKind crates/physics/src` is empty.
- **GLSL game tokens.** `water.vert`, `water.frag` and `composite.frag` contain no `GameKind` or per-game branch. The only hits are comments naming Skyrim, Starfield or FO4 as sources of authored fields.
- **Push-block lockstep.**
  - `WaterDrawPush` is one `uint waterIndex` in both stages. Rust `WaterPush` is 16 B, with a `const` assert and `size_of`/`align_of` tests.
  - `GpuWaterParams.optical.zw` now carries swell amplitude and heading. `water.vert` and `water.frag` read it, the SSBO struct is unchanged, and the renderer tests that guard it pass.
  - `WaterParams` is declared separately in vert and frag; a text diff of the two declarations differed only by comments/formatting in my quick check, which I did not fully normalise. The renderer reflect/lockstep tests pass (86).
- **Guards named in the skill are live.** All 14 I grepped exist as plain `#[test]` with no `#[ignore]` and no `ignore` in the 3 lines above. Only 4 water tests are `#[ignore]`d in the `byroredux` water run: `distant_water_mesh_is_per_cell_on_real_fnv`, `riverwater_flowne_real_record_layers_run_downstream`, `water_sentinels_hold_on_real_watr_per_game` and `real_fallout3_water_survives_disk_load_overlay`. All need game data; none is a named skill guard.

**Tests (default `target/`, `TMPDIR=/mnt/data/tmp`).**

| Command | Result |
|---|---|
| `cargo test -p byroredux water` | 171 passed, 0 failed, 4 ignored (game data); other bins 0 matched |
| `cargo test -p byroredux-renderer --lib -- water` | 86 passed, 0 failed |
| `cargo test -p byroredux-core water` | 18 passed, 0 failed |
| `cargo test -p byroredux-physics water` (extra) | 50 passed, 0 failed |

### Dim 6: Dim 6 summary

HEAD `3e6f6b66a`, baseline `4a3e89d56` (AUDIT_EXTERIOR_2026-10-21). Audited: Dim 6 (distant LOD and trees) plus the Dim 7 harness check.

**Read.** Diffs of `7eac91637`, `c21235577`, `021e1b824`, `635330d08` (lod_support / rings / m-exteriors.sh), `25f1d78ce` (bands), `125a21c2b`, `3324c23b1`, `e9eab676e`, `792ee5763`, `6528e110b`. Current code of `terrain_lod.rs` (`spawn_lod_block`, `stream_lod_blocks`), `lod_bands.rs` (`select_authored_lod_quads_refining`), `lod_support.rs` (`retain_resident`, `classify_lod_attempt`), `placement_lod.rs`, `tree_lod.rs`, `streaming_helpers.rs`.

**Tests** (`export TMPDIR=/mnt/data/tmp`, default `target/`):
- `cargo test -p byroredux lod`: **208 passed, 0 failed, 6 ignored** (the 6 are real-data `--ignored`: 3 FO3 `legacy_lod_index_tests`, `lod_descriptor_tiles_the_shipped_quad_names`, `tree_lod_corpus_parses_and_resolves_every_species`, `installed_fo3_statesman_roof_inherits_wasteland_lod`).
- `cargo test -p byroredux tree_lod`: **11 passed, 0 failed, 1 ignored** (`tree_lod_corpus_parses_and_resolves_every_species`, needs Skyrim SE data).
- Every guard the skill names that lives in the `byroredux` crate was found in the passing list, plain `#[test]`. The renderer-crate guard `an_abandoned_group_leaves_no_hole_and_admission_stays_closed` (`crates/renderer/src/mesh/geometry_ssbo.rs:1525`) exists but was not run here (different crate).
- `probe_lod_corpus` run read-only on `Skyrim - Meshes0/Meshes1/Textures0.bsa` (name tables only): Meshes0 = 12 `lodsettings`, Meshes1 + Textures0 = 11,057 family entries total, `.btt` 329 Tamriel L4 + 4 + 24 + 8 + 12 across the other worldspaces. No surprise against the skill's tree-LOD claims.

**Baseline finding EXT-D6-2026-10-21-02 (FO4 synth block MNAM-only base): FIXED and guarded.** `6528e110b` landed after the baseline snapshot. `spawn_lod_block` now resolves its base through `synth_base_texture_path` -> `LandscapeLayerTextures::texture_set` (the full-detail resolver, now `pub(super)`); `stream_lod_blocks` builds one `LandscapeLayerTextures` from `state.mat_provider`. Guard `synth_block_base_resolves_an_mnam_only_ltex_through_its_bgsm` (`terrain_lod.rs:1314`) is a plain `#[test]` and passes; it asserts the MNAM-only, ICON-only, `Some(0)` and `None` arms.

**Delta verdicts**

| Commit | Verdict |
|---|---|
| `7eac91637` skirts | Per-game branching: none. "Block is `.btr` or synthesized, never both": holds (skirts are inside the synth builder only; the `.btr`-vs-synth choice at `terrain_lod.rs:~700` is untouched). `lod_coverage` double-draw: no effect (a skirt is a vertical wall with zero plan-view area; coverage counts quad footprints). Drop constant `LOD_SKIRT_DROP = 640.0` is a named const with an empirical rationale, not a measurement. Winding unit-tested. **Three issues: D6-01, D6-03.** |
| `c21235577` #6098 | OK. `placement_lod_cell_path` takes the child's own `.lod` if the archives have it, else `lod_worldspace_key()`'s, never both; a self-sourced worldspace probes once. Guards `a_child_cell_prefers_its_own_lod_and_inherits_the_rest`, `a_lod_source_worldspace_does_not_probe_itself_twice` pass. The commit message itself says the "parent's list inside a city worldspace" policy is inferred, not observed. |
| `021e1b824` #6332 | OK. `begin_/abandon_scene_geometry_group` used by object and placement rings; `drop_meshes` only as fallback when abandon refuses (`object_lod.rs:685`, `placement_lod.rs:729`). Terrain and tree upload one mesh per attempt so need no group. Source pin `multi_mesh_lod_rings_abandon_a_refused_group_instead_of_dropping_it` live. |
| `635330d08` #6092/#6093 | OK. All four rings check `scene_geometry_admission_open()` before an attempt and call `classify_lod_attempt(false, admission_after)` after a no-block result; `HeldBack` inserts nothing (no `missing_blocks` entry, no empty sentinel) and breaks out as starved. `Miss` is the only arm that caches. `app_step.rs:139` re-arms via `lod_should_rearm_after_starvation`. `held_back` reaches the `lod-coverage:` line (`crates/core/src/ecs/resources/mod.rs:1189-1230`, field `held_back=`); `m-exteriors.sh:793-803` parses it and hard-fails on non-numeric or non-zero. Consistent. |
| `25f1d78ce` #6094 | OK. `refine_within` fed `full_detail_radius`; both guards pass. Note the legacy authored selector still has the `l2 == level * 2` seam test (D6-02). |
| `125a21c2b` #5715 | Streaming salvage queue; not a LOD logic change. One displaced doc block (D6-03). |
| `3324c23b1` #6300 | OK. `note_scene_health` added at the three LOD importers (`object_lod`, `placement_lod`, `terrain_lod_btr`). No behaviour change. |
| `e9eab676e` #6321 | OK. `build_tree_lod_mesh` uses `zup_to_yup_pos`; the workspace scan test passes. |
| `792ee5763` #6095 | Doc-only; the displaced blocks it fixed are fine, but see D6-03 for the same class re-introduced by `7eac91637`. |

**Skill-bullet checks**
- All four rings evict through `lod_support::retain_resident` (terrain 614, object 272, placement 387, tree 418); `every_lod_ring_routes_eviction_through_retain_resident` passes.
- `GameKind` in logic: none. Every non-test use is in a scheme table or data function (`object_lod_scheme`, `tree_lod_supported`, `placement_lod_supported`, `combined_lod_supported`, `legacy_landscape_lod_supported`, `LodBandLadder::for_*`, `tree_load_cells`, `btr_normal_path_candidates`' FO4 `_msn` suffix table). The two legacy rings and the index scan gate on `TerrainLodLayout::FalloutLegacy` / `ObjectLodScheme::FalloutLegacyBlocks`. `spawn_lod_block` takes `game` only to pass to `legacy_landscape_lod_supported` / `translate_terrain_lod_textures`.
- PNAM `0x02` child owns no quads: every distant-ring key resolves on `wctx.lod_worldspace_key()` (terrain 520, object 223/241/434, lod_support 138-250, tree 368/405, placement fallback). The only direct `wctx.worldspace_key` uses are the child's own full-detail cell map (`terrain_lod.rs:502`), the placement own-`.lod` probe (by design, #6098) and seam stats (full-detail residency). Holds.
- "A refused cell is still never retried (open #6106)": still true. #6106 is open; `0188ce132` only counts (`full_detail_refusals=`, `app_step.rs:282`). No later commit touches it.
- #6053 (FNV `m-exteriors.sh fnv boundary` 384 terrain-LOD overlaps) is still open and its root cause is still in the code (D6-02).

### Dim 6: Dim 7 summary

Files exist and keep their contracts. Findings: **D6-04** (README row stale). Nothing else changed for the gates.
- `scripts/renderer-eval-groundcover.sh` exists (tracked mode `100644`, not executable: it must be run as `bash scripts/...`; docs only link it, no workflow invokes it, unchanged since 2026-09-27). Washout precheck still `BYROREDUX_GC_WASHOUT_SD` default 0.039 (line 73). It runs `check-shader-artifacts.sh` at line 97, before the engine build and capture.
- `docs/smoke-tests/m-exteriors.sh`: modes `static|boundary|soak|cycle|water` (usage lines 34, 70, 1138). Runs `check-shader-artifacts.sh` at line 110 before build and capture. `635330d08`'s change (`cov_held_back` parsed from `held_back=N`, non-numeric -> HARD FAIL "incomplete lod.coverage line", non-zero -> HARD FAIL "LOD coverage violation") matches the producer (`LodCoverageStats::machine_line`, `crates/core/src/ecs/resources/mod.rs:1218-1230`) and the unit assertions `streaming_tests.rs:915/926`. Consistent; an older binary without `held_back=` now HARD FAILs rather than WARNs (intended by the commit).
- `docs/smoke-tests/w1-water-traversal.sh`: exists, runs `check-shader-artifacts.sh` at line 97. Skyrim route now declared (see D6-04).
- `docs/smoke-tests/m34-day-night.sh`: exists, honours the SKIP-77 contract, does **not** run `check-shader-artifacts.sh` and captures no frame (skill drift item 4).
- `docs/smoke-tests/README.md` indexes all of the above (rows 300, 302, 320). `m-exteriors.sh` row does not yet mention the `held_back` condition of the `lod.coverage` gate (minor).
- Not run (needs Vulkan + engine): none of these scripts were executed, per RULES.

