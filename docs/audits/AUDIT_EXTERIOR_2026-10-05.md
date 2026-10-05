# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: [AUDIT_EXTERIOR_2026-10-02.md](AUDIT_EXTERIOR_2026-10-02.md) (HEAD `c9f95283a`, 124 commits since) · **Audited**: Dim 1–6 (every one of their `Paths:` changed) · **Unchanged since baseline (skimmed)**: Dim 7 (Acceptance gates and harness; guards spot-checked)

**Method.** This run is part of `/audit-suite --preset comprehensive`.
- One auditor did every dimension synchronously. No sub-agents were used.
- No engine launch, smoke script or capture (suite rule). The GPU prefilter test was not run: no sky shader changed since the baseline. `probe_lod_corpus` was not run.
- About 26 exterior fix commits landed since the baseline, closing nearly the whole 2026-10-02 report. Most of this pass is adversarial verification of those fixes, plus a first read of the new per-cell distant water (#5243 / #5244 / #5245).

**Game-data censuses** (Python, read-only, scripts in the suite scratch dir):
- WRLD `DNAM` water height vs `NAM3`/`NAM4` LOD water for Oblivion, FO3, FNV, Skyrim SE and FO4.
- Per-worldspace XCLW tri-state × LAND minimum (inherit cells that are wet at the DNAM vs the NAM4 height) for FO3, FNV, Skyrim SE and FO4.

**Test state (green at HEAD, rustc 1.96.0):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd sky spawner` | 560 passed, 0 failed, 17 ignored (all 17 need real game data; none is a muted guard) |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ water terrain memory_budget` | 195 passed, 0 failed, 1 ignored (the Vulkan prefilter test) |
| `--ignored --exact` exterior real-data tests: `water_sentinels_hold_on_real_watr_per_game`, `riverwater_flowne_real_record_layers_run_downstream`, `default_land_textures_exist_in_vanilla_archives`, `oblivion_ltex_paths_exist_in_vanilla_archives`, **`distant_water_mesh_is_per_cell_on_real_fnv`** (new) | 5 passed against installed game data (peak RSS 1.6 GB) |
| `scripts/check-shader-artifacts.sh` | 36 shaders + the opaque early variant reproduce byte-identically (glslang 11:16.2.0) |
| `scripts/check-byro-dbg-harness-contracts.sh` | OK (50 scripts scanned) |

## Executive Summary

**7 findings: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 5 LOW.** All 7 are NEW. Some of them name the closed issue whose fix they extend.

**The fix wave holds.**
- 26 issues closed since `c9f95283a` touch these paths. Each was re-read against its diff.
  - From the baseline report: #5169, #5172–#5186.
  - Older: #5135, #5136, #4929.
  - New WATAL / other work: #5243, #5244, #5245, #5220, #5122, #5225, #5226, #4441.
- **Correct and complete**: every Dim 2, 3, 4 and 6 fix. These are #5172–#5181, #5186, #5220, #5122 and #5225.
- **Correct but leaving a sibling**:
  - #5169 left FO76's pigment lanes on Starfield's scale (D1-01).
  - #5178's parity test compares the seed with itself (D4-01).
  - #5136 left the parser doc unchanged, and its new §2 row re-settles the Oblivion frame that #5182 marked OPEN (D5-05).
- **Built on a false premise**: #5244 (D5-01).
- **Missed a whole game**: #5243 (D5-02).

**Headline.** EXT-D5-01: #5244 made the distant-water hole "exactly the streaming boundary". It assumed full-detail cells exist out to `radius_unload`. They do not:
- `compute_streaming_deltas` loads only `radius_load`. The `radius_unload` ring is only hysteresis residency.
- The terrain-LOD code already says so ("`radius_unload` bounds possible hysteresis residency but does not populate its outer ring", `terrain_lod.rs:113-118`, guard `only_actual_residency_is_holed_from_lod`).
- So at every worldspace entry, and along the leading edge of travel, the ring at `radius_load + 1` has no full-detail water and no distant water. LOD terrain is drawn there with no water over it.

The one-cell waterless ring that #5244 set out to remove is still there; it moved one cell inward. #5244's own live evidence ("40–48 planes at rings 0–4" at `--radius 3`) fits within a 7 × 7 = 49-cell load, i.e. rings 0–3 only.

**Already filed in this suite, not re-filed:**
- PHYS-D5-2026-10-05-01: #5245's flowing-kind weather damping reaches the renderer and the camera, but not buoyancy or `player_water_state`. Exterior's ruling on the policy value is in the matrix below.
- PERF-D7-2026-10-05-02: distant-water rebuild cost.
- ESM D5-02 LOW: Starfield WTHR fog lift not gated on an authored FNAM.
- REN-D10-01: #5192 transmission lobes unshadowed. That is sky/sun *consumption*, which `/audit-renderer` owns.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds for the translate sites.** The callers are still only `scene/world_setup.rs::apply_environment` plus `procedural_fallback_*` in world_setup, weather and cornell. No production `SkyParamsRes {` / `WeatherDataRes {` literal exists outside `env_translate.rs`, `components.rs` or test modules. #5179 publishes the effective TOD quad once, and `render/water.rs` and `commands/time.rs` read it. #5178 removed the third TOD fold. **One dent:** the distant-water builder re-derives "effective cell water height" inline and takes its default from `NAM4` (D5-03). The authoritative rule is `resolved_exterior_water_height` + `default_water_for_worldspace`. |
| **no-fabrication** | **Mostly holds.** New constants are documented engine choices: `FLOWING_WATER_WEATHER_TRANSPORT` 0.35 (constant doc + watal.md, measured on the White River fixture) and `BASE_FOG_STRENGTH` (now exal.md §3.1). **Dents:** FO76's pigment lanes still take Starfield's 0..20 normalisation, with no census and no OPEN entry (D1-01). watal.md §2's Oblivion row re-asserts a frame that the same doc marks OPEN (D5-05). |
| **no-leak** | **Holds.** #5183 makes an XWCU current on a Calm WATR promote the kind, as the WATR arm does. The 8 FO4 Calm-plus-`WaterFlow` placements are gone. `concentration_lane3_is_oceanness` keeps FO76's lane 3 at the zero sentinel. |
| **no-render-time-fallback** | **Holds.** The only new `GameKind` use is `concentration_lane3_is_oceanness`, a table-shaped `matches!`. There is no `game ==` in new logic. Sky, cloud and ground-cover GLSL carry game names only in comments. The physics crate is still `GameKind`-free. |

## Per-Category Matrix

✓ = holds; findings are listed in the last column.

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ (#5173 lane chain sized from `TERRAIN_SPLAT_LAYERS`) | ✓ | ✓ | ✓ | none |
| **Sky** (`translate_sky`, `pack_sky_dome`, `install_interior_outdoor_defaults`) | ✓ (#5180 derived guard) | ✓ | ✓ | ✓ | none |
| **Weather / sun / fog** (`translate_weather`, `translate_exterior_cell_lighting`, `fit_legacy_fog_extinction`, `spatial_units::normalize`) | ✓ (#5178, #5179) | ✓ (#5135 documented) | ✓ | ✓ | D4-01 |
| **Water** (`resolve_water_material`, `water_material_from_mesh`, `compose_flow_scrolls`, `build_distant_water_mesh`) | ~ D5-03 | ~ D1-01, D5-05 | ✓ (#5183) | ✓ | D1-01, D5-01 … D5-05 |
| **Ground cover** (`groundcover_translate.rs`, `resolve_authored_cover`) | ✓ | ~ #4906 (baseline D3-01, still open) | ✓ | ✓ | none new |
| **Distant LOD / trees** (`select_lod_quads`, `terrain_lod_layout`, `object_lod_scheme`) | ✓ | ✓ (#5186 layout verified) | ✓ | ✓ | none new (#4913, #5222 open) |
| **Acceptance gates** | — | — | — | — | none new. There is no distant-water assertion, which is folded into D5-01's fix. |

**`FLOWING_WATER_WEATHER_TRANSPORT` policy (routed here by AUDIT_RENDERER / AUDIT_PHYSICS 2026-10-05):**
- The value 0.35 is a documented engine choice. It is applied only on `WaterKind::is_flowing()` kinds, which come from the canonical boundary, so it is not a render-time game branch. Policy: holds.
- The defect is the sampler parity (PHYS-D5-2026-10-05-01). The fix belongs in one helper that all four samplers call. The renderer comment at `render/water.rs:112-119` already names `weather_wave_adjustment` as the single source that #3207 established; #5245 put a second per-plane step after it in two of the four consumers.

## Fix verification (since baseline)

| Fix | Verdict |
|---|---|
| #5169 FO76 absorption ÷70 + lane-3 gate (`40de6da5d`) | Correct for the absorption triplet and lane 3, and pinned. Pigment lanes 0–2 were left on Starfield semantics (D1-01). watal.md line 352, edited later by #5185, now says "FO76 untouched" four lines above the #5169 sentence (D5-05). |
| #5172 / #5203 / #5221 `GpuTerrainTile` 176 B prose | Correct. `gpu_terrain_tile_is_176_bytes` is live. |
| #5173 affinity lane chain (`d1100a80e`) | Correct. Host arrays are sized from `TERRAIN_SPLAT_LAYERS`. `byroGcAffinity` is a lane loop with paired indexing, and the guard pins the loop shape. SPIR-V reproduces. |
| #5174 tile-less blade coupling (`abed5f61a`) | Correct. `base_diffuse_index` takes the old `pad2` slot (still 64 B), and the blade fragment couples tile-less cells, including the full-tile-table case. |
| #5175 placeable bitset (`7814feea8`) | Correct. An exact `u128` key with a const-assert of `GROUNDCOVER_MODEL_MAX_RECORDS <= 128`. `count_ahead` is clamped, so no shift overflows. No per-frame Vec. |
| #5176 nearest-first LAYOUT (`226f2dd4c`) | Correct. The permutation is built in reused scratch. The shader skips an out-of-range entry. |
| #5177 ground-cover docs | Correct. §12.12 now states the `density / share` bake. Its saturation defect is still open on #4906. |
| #5178 seed through the shared fold (`017ed5847`) | Correct code. The pinning test's "seed == sampler" assertion compares the seed with itself (D4-01). |
| #5179 published TOD breakpoints (`80fd879bf`) | Correct. They are republished every tick from the same blend `compute_sun_arc` uses, and seeded by both `translate_sky` paths. There is no new `WeatherDataRes` field, so no promotion hazard. |
| #5180 derived sky-copy guard (`38d3b61d3`) | Correct. The forbidden set (≥ 15 names) is parsed from `sky.glsl` + `clouds.glsl`. Both consumers are scanned; they are the only `sky.glsl` includers. |
| #5181 sky/weather doc repoints | Correct. All four pointers and the rustdoc placement were verified. |
| #5182 Oblivion frame OPEN / #5185 Starfield boundary docs | Correct where they landed. #5136's §2 rewrite then re-settled the Oblivion row (D5-05). |
| #5183 Calm WATR + XWCU promotes (`cee426dd7`) | Correct. It mirrors `classify_water_kind_and_flow`, with Lava kept. Texture paths stay equivalent, because a Calm WATR has no NAM5. |
| #5184 sentinel guard over every `GameKind` (`5b5a18152`) | Correct. |
| #5186 `.btt`/`.lst` layout (`62a31d6de`) | Correct. It matches the baseline census. |
| #5135 `BASE_FOG_STRENGTH` docs + `gpu_extinction` (`fd855dd98`) | Correct. |
| #5136 watal.md wind table (`f75ac66cb`) | Table updated. The parser doc at `water.rs:312-322` was not touched, and the Oblivion row contradicts #5182 (D5-05). |
| #5243 per-cell distant water (`98061ec58`) | Correct for FNV/FO3/Skyrim/FO4 main worldspaces. Oblivion gets nothing (D5-02). The builder duplicates the tri-state rule with a `NAM4` default (D5-03). Failure paths are untidy (D5-04). |
| #5244 hole = `radius_unload` (`3e258fa36`) | **False premise** (D5-01). |
| #5245 / #4929 flowing transport (`37cc637e9`) | Renderer and camera: correct. Buoyancy and player sampler: PHYS-D5-2026-10-05-01. Rapids third layer: correct (reads `normalScrollC`). |
| #5220 GCModels stats (`da9405d09`) | Correct. The reset comes after `harvest` on both nothing-to-place paths. |
| #5122 `ModelPush: NoUninit` (`589084d3f`) | Correct. Nit, owned by `/audit-safety`: the SAFETY text says a padding-breaking field "fails the `NoUninit` gate here". `NoUninit` is a plain `unsafe` marker (`buffer.rs:40`), so only the size test catches it. |
| #5226 `--wrld` typo errors (`c6cbbe9f9`) | Correct. |

**Guard liveness:** every guard the skill names is a live plain `#[test]`. #5179/#5180/#5184/#5244 added these guards, all live: `the_published_breakpoints_ease_with_the_crossfade`, `composite_does_not_carry_its_own_copy_of_the_sky` (now derived), `resolve_water_material_sentinels_are_game_invariant` (all six games), `distant_water_hole_is_exactly_the_streaming_boundary` (live but pins D5-01's false premise), `flowing_kinds_damp_the_weather_transport_but_calm_keeps_it`.

## Findings

### EXT-D5-2026-10-05-01: The distant-water hole is the `radius_unload` disc, but full-detail cells are only guaranteed out to `radius_load` — every worldspace entry and the leading edge of travel leave a waterless ring at `radius_load + 1` over LOD terrain
- **Severity**: MEDIUM (a coverage gap built in by construction, the same class and severity as #5244)
- **Dimension**: Water translation (WATAL); this is the distant-water seam
- **Location**:
  - `byroredux/src/cell_loader/water.rs:851-865` (`distant_water_hole_radius` and its doc); `:932` (`if distance <= hole_radius … continue`).
  - `byroredux/src/streaming.rs:1995-2040` (`compute_streaming_deltas`: the desired set is `-radius_load..=radius_load`; a cell is unloaded only when `d > radius_unload`); `:907` (`radius_unload: radius_load + 1`).
  - Contrast `byroredux/src/cell_loader/terrain_lod.rs:113-118` (`cell_is_full_detail`, set-based) and its test `only_actual_residency_is_holed_from_lod` (`:1056`).
- **Status**: NEW. #5244 (CLOSED) is incomplete: its fix rests on a premise the terrain-LOD code had already disproved.
- **Tier Violated**: n/a (coverage)
- **Game Affected**: every game with distant LOD water (FO3, FNV, Skyrim, FO4)
- **Description**:
  - The new doc says "Full-detail water exists for loaded cells (Chebyshev ≤ `radius_unload`)", so the distant mesh skips `≤ radius_unload` and the two sets are "contiguous by construction".
  - Streaming never requests a cell beyond `radius_load`. A cell at exactly `radius_unload` is resident only if it was inside `radius_load` earlier and has not yet left `radius_unload`.
  - At worldspace entry (and after a teleport or load), the whole ring at `radius_load + 1` is not resident. It is still inside the hole, so neither water source covers it.
  - After a grid crossing, the same holds for the leading edge and for the lateral cells that were never within `radius_load`. Only the trailing edge keeps hysteresis residents there.
  - Terrain does not have this gap. Synthesized LOD blocks hole out only actually-resident cells, so LOD terrain (the lake bed) is drawn in that ring with no water over it.
- **Evidence**:
  - `streaming.rs:2009-2014`: `for dx in -radius_load..=radius_load { for dy in -radius_load..=radius_load { desired.insert(...) } }`.
  - `terrain_lod.rs:113-115`: "This is deliberately set-based: `radius_unload` bounds possible hysteresis residency but does not populate its outer ring."
  - #5244's live evidence ("a settled `water.dump` at grid (19,13) shows 40-48 planes at rings 0-4", `--radius 3`) fits a 7 × 7 load. 49 cells is rings 0–3; rings 0–4 would be 81.
  - The unit guard `distant_water_hole_is_exactly_the_streaming_boundary` asserts only `distant_water_hole_radius(4) == 4`, so it pins the premise rather than testing contiguity.
  - The acceptance gate cannot see this: `m-exteriors.sh … water` asserts only `Water dump: planes=` and no-sentinel (`:526-533`), and #5243's own issue notes that its frozen poses are near-field.
- **Impact**:
  - A one-cell (4096 BU) dry band in distant lakes, rivers and coast at `radius_load + 1` on entry, and on the forward side whenever the player moves.
  - At the default radius it sits ~16–20k BU out, inside the normal view. It shows where LOD terrain dips below the water line (Lake Mead, Potomac, Skyrim coast).
- **Related**: #5244 (closed, incomplete); #5243; PERF-D7-2026-10-05-02 (same rebuild path).
- **Suggested Fix**:
  - Hole the distant mesh by the actual resident set, as `block_hole_mask` does. Pass `state.loaded`'s key set to `build_distant_water_mesh` and skip only resident cells.
  - Trigger the rebuild on residency change, not only on `center_grid` change. Or do it in the same reconcile that regenerates terrain-LOD hole masks.
  - Replace the radius guard with a set-based contiguity test: every non-resident wet cell within reach gets a quad.
  - Add a distant-water assertion (quad count at a fixed pose) to `m-exteriors.sh water`.

### EXT-D5-2026-10-05-02: Oblivion never gets distant water — `spawn_lod_water` needs WRLD `NAM3`/`NAM4`, which Oblivion does not author, although #5243's per-cell model now has every input it needs
- **Severity**: MEDIUM (visible missing content on every Oblivion exterior water vista beyond the streaming radius)
- **Dimension**: Water translation (WATAL); distant water
- **Location**:
  - `byroredux/src/streaming.rs:950-982` (`spawn_lod_water`: `let (Some(height), lod_water_form) = translate_lod_water(..) else { return; }`).
  - `byroredux/src/env_translate.rs:239-276` (`translate_lod_water`, whose doc says Oblivion has neither field).
  - Compare `env_translate.rs:208-237` (`default_water_for_worldspace` gives Oblivion Z = 0 when NAM2 is present).
  - Docs: `docs/engine/watal.md:788-790` ("Older games naturally use the same path").
- **Status**: NEW. It predates #5243: the single-sheet era had the same gate. But #5243's census table lists Oblivion Tamriel as "99% inherit … coastal thousands wet-at-distance" and calls it "the least-affected worldspace", while the engine draws no distant water for it at all. Searched open and closed issues for "Oblivion distant water", "LOD water": no tracker.
- **Tier Violated**: n/a (a coverage gap; the canonical inputs exist)
- **Game Affected**: Oblivion
- **Description**:
  - The per-cell builder needs only the cell table (XCLW tri-state, LAND) plus a default height and a WATR form for the appearance.
  - Oblivion has both, through the full-detail resolver `default_water_for_worldspace`: sea level 0 when `NAM2` is authored, and `NAM2` as the form.
  - `spawn_lod_water` takes its default and form only from `translate_lod_water` (`NAM4`/`NAM3`), and returns early when they are absent. On Oblivion that is every worldspace.
- **Evidence**:
  - Census of `Oblivion.esm` WRLD: Tamriel, SEWorld and every test world author `NAM2` and no `NAM3`/`NAM4`/`DNAM`.
  - The test `translate_lod_water_is_none_when_unauthored` (`cell_loader/water.rs:2113`) pins the `None`. No test pins what Oblivion should draw instead.
  - #5243's census (`.claude/issues/5243/ISSUE.md`) gives Oblivion Tamriel 14,563 inherit cells at default 0, plus ~123 override lakes at 500–5200.
- **Impact**:
  - Beyond `radius_unload`, Lake Rumare, the Niben, Topal Bay and every override lake show as dry LOD lake bed, including from the Imperial City vistas.
  - The flagship Oblivion water is invisible at distance. Vanilla draws the world water plane to the horizon.
- **Related**: EXT-D5-2026-10-05-03 (same height-source question); #5243.
- **Suggested Fix**:
  - When `NAM3`/`NAM4` are absent, fall back to `default_water_for_worldspace` for the default height and the form. That is one table-shaped rule at the translate boundary, not a game branch in the streamer.
  - Pin it with an Oblivion-shaped fixture: NAM2 only, inherit cells wet at 0.
  - Correct watal.md §5.2.

### EXT-D5-2026-10-05-03: `build_distant_water_mesh` re-derives the effective cell water height inline, and its default is `NAM4` where the full-detail cell uses `DNAM` / `default_water_for_worldspace`
- **Severity**: LOW (latent in vanilla; a single-boundary dent)
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:935-939` (inline `if cell.explicit { cell.water_height } else { default_height }`); `:1069` / `:1216` (`Some(lod_height)` / `Some(default_height)` = `translate_lod_water`'s `NAM4`).
  - The authoritative rule is `byroredux/src/cell_loader/exterior.rs:71-81` (`resolved_exterior_water_height`), called with `wctx.default_water_height` at `:1955-1959`.
- **Status**: NEW
- **Tier Violated**: single-boundary (two derivations of one quantity, from two different default sources)
- **Game Affected**: FO3 (MegatonWorld); any mod or child worldspace with inherit cells whose `NAM4` ≠ inherited DNAM water
- **Description**:
  - A cell with no XCLW is drawn near at the worldspace default from `DNAM` (Oblivion: 0 via NAM2), and far at `NAM4`.
  - #5243's doc calls `NAM4` "the worldspace default". exal.md §5.4 says the two are "genuinely distinct … on real content (NAM4 ≠ DNAM water on 22/30 Skyrim.esm)". Whenever they differ, an inherit cell's water steps in height at the streaming boundary.
- **Evidence** (census):
  - The main worldspaces agree: Wasteland 10500/10500, WastelandNV −2300/−2300, Tamriel −14000/−14000, Commonwealth 450/450.
  - Skyrim.esm, FalloutNV.esm and Fallout4.esm have **zero** inherit cells in any worldspace.
  - In Fallout3.esm only Wasteland (equal) and MegatonWorld have inherit cells: 6 of them. MegatonWorld inherits Wasteland's DNAM 10500 through PNAM 0x01 but owns `NAM4` = 0.
- **Impact**: None visible in the vanilla main worlds. It is a trap for mods and for the D5-02 Oblivion fallback, which must not copy the inline rule.
- **Suggested Fix**:
  - Have the builder call `resolved_exterior_water_height` with the same `default_water_for_worldspace` default the near cells use.
  - Keep `NAM3`/`NAM4` for what is genuinely LOD-only (the LOD water form, and a sheet for cells absent from the table), or document why `NAM4` is the right inherit height.

### EXT-D5-2026-10-05-04: A failed distant-water rebuild leaves the stale mesh and retries a full-worldspace scan every frame; an emptied rebuild leaves a stale `water.dump` histogram
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:1237-1255` (`None => return false` and the upload-error `return false`, both before `plane.center_grid = player_grid`); `:1222-1230` (emptied path).
  - `byroredux/src/streaming.rs:998-1000` (`if plane.center_grid == player_grid { return; }`).
- **Status**: NEW (introduced by #5243's rebuild path; distinct from PERF-D7-2026-10-05-02's per-crossing cost)
- **Tier Violated**: n/a
- **Game Affected**: all with LOD water
- **Description**:
  - On an upload failure (e.g. a device-memory OOM), `rebuild_lod_water_mesh` returns `false` without advancing `center_grid` and keeps the old mesh. The old mesh's hole sits around the previous grid, so the old ring now overlaps the new full-detail cells.
  - `recenter_lod_water` sees `center_grid != player_grid` on every following frame. Each retry re-walks every worldspace cell, folds 1,089 heights per cell, and attempts another blocking upload.
  - Separately, the emptied path removes `MeshHandle` but never touches `WaterLodInfo.quad_heights` / `quad_height_count`, so `water.dump` keeps reporting the last non-empty histogram.
- **Evidence**: Code path as cited. The info update at `:1266-1272` runs only on the success path.
- **Impact**:
  - Under memory pressure: a per-frame CPU spike plus failing allocations, and the old ring double-drawing over full-detail water at mismatched hole positions.
  - In the empty case: misleading diagnostics.
- **Suggested Fix**:
  - On failure, record the attempted grid, or back off, so the rebuild runs again only on the next real crossing.
  - Consider dropping the stale mesh rather than keeping a mis-holed one.
  - Zero `quad_height_count` on the emptied path.

### EXT-D5-2026-10-05-05: WATAL / EXAL doc rot after the fix wave (bundle)
- **Severity**: LOW (doc rot)
- **Dimension**: Water translation (WATAL), plus one Dim 1 doc and one census tool
- **Location / Status**: per item
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Oblivion, FO3/FNV, FO76, Starfield (docs only)
- **Description**:
  1. **watal.md §2 Oblivion row re-settles an OPEN frame** (Regression of #5182 by #5136's `f75ac66cb`).
     - The rewritten per-game table (`docs/engine/watal.md:421`) gives Oblivion "Conversion: none applied" with "frame follows from the field's definition, not a census".
     - The #5182 table about 100 lines later (`:518`) says "**un-rotated, OPEN** … does not establish φ = θ". The doc now contradicts itself, which is exactly the defect #5182 removed.
  2. **The parser field doc was never updated** (incomplete #5136).
     - `crates/plugin/src/esm/records/misc/water.rs:312-322` still says the translate boundary "applies the one +90° rotation to either" convention.
     - It also lists FO76, Starfield and FO3/FNV `DNAM[100]` as wind-FROM bearings. Code (`env_translate.rs:603-610`) and watal.md now say otherwise. The baseline's D5-01 fix suggestion named this exact site.
  3. **watal.md:352** (#5185 text, landed after #5169) says "(`spatial_units::normalize`, Starfield-gated, FO76 untouched)". The #5169 sentence four lines below says FO76's absorption *is* lifted. `spatial_units.rs:1-4` and `:85-101` agree with #5169.
  4. **exal.md §5.4** (`docs/engine/exal.md:516-526`) still describes "a single worldspace-wide LOD water quad (a hole-cut annulus …) … a fixed entry-time snapshot". Since #5243 it is a per-cell mesh rebuilt on every grid crossing.
  5. **watal.md:788-790** "Older games naturally use the same path" is false for Oblivion (D5-02).
  6. **`crates/plugin/examples/xclw_census.rs:89-91`**: the #5244 per-ring bucket computes `(dx).abs().max(dy)`, with no `.abs()` on the y term. Cells south of the probe grid land in the wrong ring or are dropped by the `d > 0` filter. #5244's issue cites this tool's per-ring counts as its reconciliation evidence.
- **Suggested Fix**:
  - Make the §2 Oblivion row "none applied, **OPEN**" and point it at the #5182 row.
  - Rewrite the parser doc to say "per-game; see watal.md §2", and drop the FO76/Starfield/FO3-FNV bearing claim.
  - Fix line 352 to "Starfield-gated, FO76 absorption only (#5169)".
  - Rewrite exal.md §5.4 to describe the per-cell rebuild model.
  - Add `.abs()` to the census.

### EXT-D1-2026-10-05-01: #5169 left FO76's pigment lanes 0–2 on Starfield's 0..20 scale — `fo76_fourth_concentration_lane_is_not_promoted_to_oceanness` pins FO76 pigments equal to Starfield's, and watal.md records no OPEN item
- **Severity**: LOW (latent while FO76 is parse-only)
- **Dimension**: EXAL boundary discipline (unit/semantic completeness at the WATAL translate)
- **Location**:
  - `byroredux/src/env_translate.rs:914-919` (pigment lanes ÷ `STARFIELD_WATER_CONCENTRATION_REFERENCE` for every game); the test is at `:3564` (`assert_eq!(fo76.concentration[..3], sf.concentration[..3])`).
  - `docs/engine/watal.md:356-360`.
- **Status**: NEW. It is the second suggested-fix bullet of baseline EXT-D1-2026-10-02-01 / #5169 (closed), which the fix did not take up.
- **Tier Violated**: no-fabrication (Starfield's semantic applied to an uncensused FO76 lane)
- **Game Affected**: Fallout 76
- **Description**:
  - #5169 established that FO76 lane 3 is "a different quantity whose meaning is unestablished", and gated it to the zero sentinel.
  - The same census (baseline) found FO76's lanes 16/20/24 authored at 9e-5–0.52, not Starfield's 0–20. They still go through `/20` and reach `water.frag`'s pigment term as 0–0.026, applying Starfield's meaning at an unmeasured scale.
  - The new test asserts the FO76 and Starfield results are identical on lanes 0–2, which locks this in.
- **Impact**: An unvalidated FO76 pigment contribution, near-zero today. A future FO76 water pass would inherit it as if it had been decided.
- **Suggested Fix**:
  - Either census FO76 lanes 16–24 and give them a meaning, or gate them like lane 3 (zero sentinel for FO76) through the same table-shaped predicate.
  - Record the decision as OPEN in watal.md, and change the test to assert the chosen FO76 rule rather than equality with Starfield.

### EXT-D4-2026-10-05-01: #5178's "seed == per-frame sampler" assertion compares `weather_sky_state` with itself; the seed clamps alpha where the sampler does not
- **Severity**: LOW (test hygiene; the fold half is pinned)
- **Dimension**: Sky, weather, sun
- **Location**:
  - `byroredux/src/env_translate.rs:4927-4965` (`weather_sky_state_seed_folds_high_noon_onto_day`).
  - Seed alpha at `env_translate.rs:1428` (`.clamp(0.0, 1.0)`); sampler at `byroredux/src/systems/weather.rs:491-497` (`lerp1(alpha_a, alpha_b, t)`, unclamped).
- **Status**: NEW. Related: #5178 (closed).
- **Tier Violated**: single-boundary (the parity claim is unpinned)
- **Game Affected**: all
- **Description**:
  - The test builds `wd = translate_weather(&w, …)` and asserts `wd.weather.cloud_tints[0] == seeded.cloud_tints[0]` "the seed and the per-frame sampler must derive one quantity one way".
  - `wd.weather` is itself `weather_sky_state(wthr, TOD_DAY)`, so this is seed@DAY == seed@HIGH_NOON. That is a fold check, not a sampler check. `sample_weather_sky` is never run.
  - The two rules still differ: the seed clamps the JNAM alpha to [0, 1] and the sampler does not. A JNAM outside the unit range (mod or corrupt data) gives a first frame that differs from every later frame.
- **Suggested Fix**:
  - Run `weather_system` one tick on the translated `WeatherDataRes` at DAY and compare its `SkyParamsRes.weather.cloud_tints` with the seed.
  - Apply the same clamp in both places, or move it to `translate_weather`, so the table carries canonical [0, 1] alphas.

## Findings count

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| D1 EXAL boundary | — | — | — | 1 |
| D2 Terrain | — | — | — | — |
| D3 Ground cover | — | — | — | — |
| D4 Sky / weather / sun | — | — | — | 1 |
| D5 WATAL | — | — | 2 | 3 |
| D6 LOD / trees | — | — | — | — |
| D7 Gates / harness (skimmed) | — | — | — | — |
| **Total** | **0** | **0** | **2** | **5** |

**Dedup sources:**
- `/tmp/audit/issues.json` (open set).
- `gh issue list --state all` searches: "Oblivion distant water", "EXT-D3-2026-10-02-01", plus a state check of #5169–#5186, #4913, #4906, #4929, #5135, #5136, #5222, #5225, #5243–#5245, #3307, #3207, #4760.
- Sibling reports: AUDIT_EXTERIOR_2026-10-02, and the 2026-10-05 suite reports PHYSICS / PERFORMANCE / RENDERER / ESM / SAFETY.

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #5169, #5172–#5186, #5135, #5136, #4929, #5243–#5245, #5220, #5122, #5225, #5226 | **CLOSED since baseline; fixes re-read** (table above). Incomplete ones are cross-referenced to D1-01, D4-01, D5-01, D5-02 and D5-05. |
| #4913 (Skyrim `.btt` tree LOD) | **Reopened** (OPEN). No consumer yet. The layout is now documented (#5186). |
| #4906 (LTEX→GRAS keying + baseline D3-01 `density / share` saturation) | OPEN. Code unchanged (`render/groundcover.rs:1003`). §12.12 now states the bake honestly. |
| #5170 (Starfield WATR noise-UV tile unit) | OPEN, unchanged. watal.md §2 now lists it OPEN. |
| #5171 (Starfield LGTM lift unpinned) | OPEN, unchanged. There is still no LGTM/XCLL case in `spatial_units_tests.rs`. |
| #5222 (FO3/FNV LOD grid origin) | OPEN, unchanged. |
| #3307 (VWD full-model culling) | OPEN. `.btt` REFR FormIDs are now documented in `object_lod.rs`. |
| skyal §2.3–§3 documented-open items | Still documented; not re-filed. |
| `renderer-eval-groundcover.sh` washout / backlit reference re-mint | Not re-measured (no engine launch). The precheck code is unchanged (0.039). |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 1**:
  - Add `crates/plugin/src/esm/records/spatial_units.rs` to `Paths:` (third report asking).
  - Note that `SkyParamsRes::tod_hours` is the published TOD quad (#5179). Readers of "effective dawn/dusk" must use it, not `WeatherDataRes.tod_hours`.
- **Dim 3**: add `crates/renderer/src/vulkan/groundcover/` to `Paths:`.
- **Dim 4**: add `crates/renderer/src/vulkan/context/frame_params.rs` to `Paths:`.
- **Dim 5**:
  - The text "its hole exactly the streaming boundary (#5244, `distant_water_hole_is_exactly_the_streaming_boundary`)" restates the false premise (D5-01). Reword it once D5-01 is fixed.
  - Add `byroredux/src/streaming.rs` (`spawn_lod_water` / `recenter_lod_water`) to `Paths:`.
  - Add the `FLOWING_WATER_WEATHER_TRANSPORT` sampler-parity rule: one helper, four samplers.
- **Dim 7**:
  - Add `crates/debug-server/src/system.rs` and `scripts/check-byro-dbg-harness-contracts.sh` (second report asking).
  - Record that no gate covers distant water.

## Cross-audit routing

- **`/audit-physics`**: PHYS-D5-2026-10-05-01 owns the sampler parity. Exterior's verdict is that the 0.35 policy is a documented engine choice. Fix it as one helper next to `weather_wave_adjustment`, called by `render/water.rs`, `systems/water.rs`, `systems/character.rs` and `crates/physics/src/water.rs`.
- **`/audit-performance`**: PERF-D7-2026-10-05-02. D5-01's set-based fix adds residency-change rebuilds, so cache the cell projection (`distant_water_cells`) at worldspace entry when fixing either.
- **`/audit-esm`**: ESM D5-02 (Starfield WTHR FNAM-less lift). Not re-filed.
- **`/audit-safety`**: the #5122 SAFETY text overclaims the "fails the `NoUninit` gate" sentence (nit; the padding proof itself is true).
- **`/audit-renderer`**: REN-D10-01 (sun/transmission shadowing) is sky *consumption*. Not exterior-owned.
- **`/audit-oblivion`**: D5-02 (no distant water on Oblivion).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-05.md`.

**Labels:**
- All findings: `terrain-exterior`.
- D5-01: `medium`, `bug`, `water`.
- D5-02: `medium`, `bug`, `water`, `game:oblivion`.
- D5-03, D5-04: `low`, `bug`, `water`.
- D5-05: `low`, `documentation`, `doc-rot`, `water`.
- D1-01: `low`, `bug`, `water`, `game:fo76`.
- D4-01: `low`, `test-gap`.
