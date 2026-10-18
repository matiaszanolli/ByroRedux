# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-18

**HEAD**: 6f44874ac · **Baseline**: [AUDIT_EXTERIOR_2026-10-12.md](AUDIT_EXTERIOR_2026-10-12.md) (HEAD `5d31fd637`, 586 commits since; 2026-10-09 and 2026-10-08 read for dedup) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain, the density override only), Dim 3 (ground cover), Dim 6 (distant LOD + trees) · **Unchanged since baseline (skimmed, guards spot-checked)**: Dim 4 (sky / weather: no sky / cloud / SH logic commits), Dim 5 (WATAL: only the #5705 rebuild gate changed), Dim 7 (gates / harness: no commits). This audit's dimensions are all in-area, so it is a full exterior audit, **not** area-scoped. The working tree's uncommitted edits (`byroredux/src/commands/`, `crates/scripting/`) are outside the area and were ignored; HEAD was audited.

**Method.** Part of the `--area exterior` suite. One auditor did every dimension synchronously. No sub-agents, no engine launch, no capture. Delta scope was `git log 5d31fd637..HEAD -- <Paths>` per dimension. Diffs were read for every exterior-relevant commit that landed after the baseline and was never audited:
- Environment install: `34a820ceb` (#5785, climate in Show-Sky interiors), `d1b298921` (ObScript `ForceWeather` -> `apply_forced_weather`), `6eb1f378a` (fog colour radiance), `3b28dca71` (#5463, XCCM game split), `bb57270c3` (#5496 completion).
- Ground cover: `c14dd2b39` (#4906), `592f31b87` (blade density from GRAS), `af8323881` (LOD tiers), `b731b01f5` (reach and view wedge), `a27f7529d` / `39c9e39f0` / `2c86473ff` / `3f1c5e962` (look), `d06ac5844` / `251d491ef` (constants, hygiene).
- LOD and streaming: `a9384f9c0` (#4913, tree LOD, 536 new lines), `eacaaa1dc` (#5705), `83fb9700e` (shared retain body), `02187c7f9` (#5687).

Read-only censuses (scripts in `/tmp/audit/exterior-suite/exterior/scripts/`): Oblivion WRLD walk plus `distantlod\*.lod` name table (see Dim 6).

**Test state (green at HEAD, rustc default toolchain, `TMPDIR=/mnt/data/tmp`):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd sky spawner climate legacy_lod default_weather tree_lod` | 622 passed, 0 failed, 22 ignored (game data / device) |
| `cargo test -p byroredux --bin byroredux -- fog show_sky forced_weather xccm` | 81 passed, 0 failed |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_ water terrain memory_budget shader_constants cloud` | 269 passed, 0 failed, 1 ignored (Vulkan prefilter) |
| `scripts/check-shader-artifacts.sh` | 37 shaders + early-test variant byte-identical to the committed `.spv` (glslang 16.4.0). Confirms `c14dd2b39` / `d06ac5844` shader edits were recompiled. |

Not run: the `--ignored` real-data tests (`tree_lod_corpus_parses_and_resolves_every_species`, the legacy-LOD corpus pins) and the GPU prefilter test. No game data was loaded and no Vulkan device was used.

## Executive Summary

**5 findings, all NEW: 0 CRITICAL · 0 HIGH · 2 MEDIUM · 3 LOW.**

Both MEDIUM findings are the same shape: a recent change is correct against its own tests and wrong in the integration the tests cannot see.

- **D1-01 (MEDIUM).** `34a820ceb` (#5785) installs the outdoor sky inside `InteriorCellApplyJob::finish`, *after* the interior load's only texture flush. The cloud and sun-sprite DDS uploads stay queued. A debug build trips `finish`'s own `debug_assert_eq!(pending_dds_upload_count(), 0)`. A release build leaves the cloud layers on the fallback checkerboard descriptor. The same omission exists on `apply_forced_weather`.
- **D3-01 (MEDIUM).** `592f31b87` made the shared per-cell ground-cover affinity equal to the authored GRAS union density. `c14dd2b39` had already made the authored-model tier accept at that same union density. The model tier multiplies both, so its authored density is squared wherever a load order links textures to grass.
- **D1-02 (LOW), D6-01 (LOW), D3-02 (LOW).** ForceWeather semantic gap and an unread flag; doc rot around tree-LOD / `IsLodTerrain`; a stale constant doc.

Everything else in the delta was re-read and holds. `bb57270c3` closes the two #5496 follow-ups from the 10-12 audit (D6-02, D6-03) correctly. The tree-LOD parsers are bounds-checked and the ring is torn down by the same drain as the other rings. The #5705 gate is correct. The Show-Sky climate resolution and the XCCM split compose correctly. D6-01 of the 10-12 report (Tamriel's `.btr` / `.bto` lattice origin) is **still present at HEAD** and is tracked by the reopened #2586.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds.** `apply_environment_with_sources` is the one installer for exterior bootstrap, XCCM override, forced weather and the new Show-Sky interior path; `translate_sky` / `translate_weather` / `translate_exterior_cell_lighting` have no new caller. The `fog_colour_radiance` decode has the one call site (`render/mod.rs:1368`, pin tracked by #5949). The LOD key is now honoured at the origin and the object spawn (`bb57270c3`). |
| **no-fabrication** | **Dented once, already tracked.** The lattice origin is still `NAM0`, not the `lodsettings\*.lod` descriptor (#2586, reopened 2026-10-13). New constants cite a source or are labelled art direction (`TREE_LOD_ALPHA_CUTOFF` 0.5 unsourced and documented as such; `LOD_WATER_MAX_DEFER` 4 s "art-direction value, not measured"). |
| **no-leak** | **Holds, one soft spot.** `ForcedWeather.locked` is stored and saved but nothing in `byroredux/src` reads it (D1-02). `CellData::sky_region` has no consumer by documented forward scope. |
| **no-render-time-fallback** | **Holds.** No game token in `sky*` / `clouds` / `groundcover_*` GLSL beyond comments; `GameKind` in the new code is table-shaped (`tree_lod_supported`, `tree_load_cells`, `xccm_targets_region`). |

## Per-Category Matrix

✓ = holds, ~ = dented (see Findings).

| Category (boundary fn) | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh`, `build_cell_splat_layers`) | ✓ | ✓ | ✓ | ✓ | none (the GRAS override is read under D3-01) |
| **Sky** (`translate_sky`, `pack_sky_dome`) | ✓ | ✓ | ✓ | ✓ | none |
| **Weather / sun / image space** (`translate_weather`, `resolve_default_weather`, `apply_environment_with_sources`) | ✓ | ✓ | ~ `locked` (D1-02) | ✓ | D1-01, D1-02 |
| **Climate** (`resolve_exterior_climate`, `resolve_cell_climate`, `show_sky_interior_climate`) | ✓ | ✓ | ✓ | ✓ | none (XCCM split verified) |
| **Water** (`resolve_water_material`, `resolve_distant_water`) | ✓ | ✓ | ✓ | ✓ | none (#5705 verified) |
| **Ground cover** (`groundcover_translate.rs`, `resolve_authored_cover`, `gcPickLocalRecord`) | ✓ | ✓ | ✓ | ✓ | D3-01 (logic), D3-02 (doc) |
| **Distant LOD + trees** (`select_lod_quads`, `stream_tree_lod_blocks`, `worldspace_lod_grid_origin`) | ✓ | ~ #2586 (open) | ✓ | ✓ | D6-01 (doc) |

## Findings

### EXT-D1-2026-10-18-01: The Show-Sky interior climate (and the forced-weather re-apply) enqueue cloud / sun-sprite DDS uploads that nothing flushes. Debug builds trip `finish`'s own `debug_assert`; release builds sample the fallback checkerboard for the life of the cell
- **Severity**: MEDIUM. It defeats #5785's purpose (authored clouds in a sky-exposed interior) on every first entry, and it is a debug-build panic on the new path. Contained to TES4 / FO3 / FNV cells that author `XCCM` + Show Sky (Skyrim / FO4 route `XCCM` to `sky_region` since #5463).
- **Dimension**: EXAL boundary discipline
- **Location**:
  - `byroredux/src/cell_loader/load.rs:729` (call) and `:789` (`debug_assert_eq!(pending_dds_upload_count(), 0)`), both in `InteriorCellApplyJob::finish`.
  - `byroredux/src/scene/world_setup.rs:529` (`apply_show_sky_interior_climate`) -> `:299` (`apply_environment_with_sources`) -> `:132` `resolve_cloud_layer` (`:168` `resolve_texture`) and `:909` `resolve_sun_sprite`.
  - Second caller: `world_setup.rs:272` `apply_forced_weather` (`app_step.rs`, new in `d1b298921`).
- **Status**: NEW (introduced by `34a820ceb` and `d1b298921`).
- **Tier Violated**: n/a (a missing batch-flush owner)
- **Game Affected**: Oblivion, FO3, FNV (Show-Sky interiors with `XCCM`); any exterior session using ObScript `ForceWeather`.
- **Description**:
  - `resolve_texture` does not upload. It reserves a bindless slot, points the descriptor at the fallback checkerboard, and queues the DDS (`texture.rs:743-768`, `upload.rs:231-253`). "The engine has no per-frame flush"; the owning loader must drain the queue (`load.rs:1137` doc, `loot_appearance.rs:729` "the frame renderer does not drain it").
  - Interior loads flush once, forced, at `load_references`' tail (`references/mod.rs:902`). `finish` runs after that, and `34a820ceb` put `apply_show_sky_interior_climate` at its top. The clouds' four layers and the CLMT sun sprite are therefore queued with nobody to flush them.
  - The exterior callers of `apply_environment` are followed by a cell load, which flushes (`begin_exterior_streaming`, then the first cell's forced flush). The new interior caller is not followed by one. `apply_forced_weather` is called mid-session from `app_step.rs` and is flushed only if a cell or LOD attempt happens to follow (`flush_pending_lod_textures` runs `if attempted > 0`).
  - `finish`'s own assertion exists for exactly this class (#3320): "a `resolve_texture` call has moved below `load_references`' flush".
- **Evidence**:
  - `finish` order: `apply_show_sky_interior_climate(...)` (`:729`) ... `debug_assert_eq!(ctx.texture_registry.pending_dds_upload_count(), 0, ...)` (`:789`). Nothing between them flushes (`populate_*` load clips, not textures).
  - The only `flush_pending_uploads` callers are `streaming_helpers.rs:485`, `scene.rs:182`, `loot_appearance.rs:740/1056` and `references/mod.rs:919`. None runs after `finish` for an interior.
  - The new test `show_sky_interior_climate_resolves_its_xccm_default_weather` covers only the climate / weather resolution (a `HashMap` lookup); no test reaches a `VulkanContext`.
  - Cache-hit case: if the realm's cloud textures are already resident (second entry), `acquire_by_path` hits and nothing is queued, so the defect shows on first entry per session.
- **Disproof attempted**: could `resolve_cloud_layer` bypass the queue? No: it calls `resolve_texture` (`world_setup.rs:168`). Could the cover logic or a frame hook drain it? `grep flush_pending_uploads` finds no per-frame caller. Not confirmed at runtime (no engine launch allowed).
- **Impact**: Debug build: panic when entering an Oblivion-realm-style Show-Sky interior for the first time. Release build: the cloud layers render the fallback checker (magenta / black) through windows and in the Show-Sky sky until the next cell load flushes. For `ForceWeather` while stationary, the new weather's clouds show the checker until the next cell / LOD apply.
- **Related**: #5785 (closed, introduced the path), #3320 (the invariant), `apply_cell_climate_override` has the same shape but is always followed by a streaming apply.
- **Suggested Fix**: One flush owner. Either call `references::flush_pending_cell_textures(ctx)` after `apply_show_sky_interior_climate` in `finish` and after `apply_forced_weather`, or make `apply_environment_with_sources` end with the same forced flush the loose-load path uses (`scene.rs:172`). Add a source-shape pin that every `apply_environment*` caller is followed by a flush before the `finish` assertion.

### EXT-D3-2026-10-18-01: The authored-model tier squares the authored GRAS density wherever a load order links textures to grass
- **Severity**: MEDIUM. Authored rocks, ferns, kelp and grass cards grow at roughly `d²` of their authored rate on every game with `LTEX.GNAM` links (Skyrim, FO3, FNV; any `AuthoredCover::localised` load order).
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `crates/renderer/shaders/groundcover_models.comp:449-455` (`field = clamp(f.affinity * f.slope * ...)`, accept test `pick.accept * field * fade`) and `:293` `gcPickLocalRecord`.
  - Affinity source: `byroredux/src/cell_loader/terrain.rs:1124-1137` (override) and `:1270-1271` (stored in `TerrainCoverInputs`), consumed via `byroredux/src/render/groundcover.rs` `EntityCell::layer_affinity` / `base_affinity`.
- **Status**: NEW (interaction of `c14dd2b39` and `592f31b87`, both 2026-10-14).
- **Tier Violated**: n/a (a double application of one authored value)
- **Game Affected**: Skyrim, FO3, FNV (and any other title whose load order carries `LTEX.GNAM`).
- **Description**:
  - `c14dd2b39` (#4906, 17:56): the model tier's accept rate is `pick.accept` = the lane-weighted union `Σ share · (1 − Π(1 − d_r))` of the authored densities of the records the ground lists. It then multiplies the §3 field, which still contains `f.affinity` (`byroGcAffinity` of the cell's affinity lanes).
  - `592f31b87` (20:34): when `AuthoredCover::localised`, each splat layer's and the BTXT base's affinity is overwritten with `union_density(grasses)` — the *same* union. The cell record is shared by both tiers, so the model tier's `f.affinity` is now that union mixed by the same ordered alpha chain.
  - Result on pure ground whose texture lists one record of density `d`: `pick.accept = d`, `f.affinity = d`, so the accept probability is `d · d · slope · shelter · clump · fade`. `exal-groundcover.md:1664-1667` promises the opposite ("a lone record keeps its authored density on pure ground").
- **Evidence**:
  - `592f31b87` touched no shader (`git show --stat`): the model tier's `byroGcDensityFactors(..., cell.affinity0, cell.affinity1, cell.baseAffinity, ...)` call is unchanged from `c14dd2b39`.
  - `byroGcAffinity` (`groundcover_density.glsl:129-152`) is `mix(a, aff_i, w_i)` over the lanes, i.e. exactly the lane-share-weighted mix of the overridden affinities.
  - The `c14dd2b39` measurement ("demanded model plants 8789 -> 1656") predates `592f31b87` by 2.6 hours, so it does not include the second factor. No test composes the accept rate: `local_selection_follows_the_ground_and_the_authored_union` (`render/groundcover.rs:1519`) checks the pick only.
  - The blade tier is not affected (it uses `f.affinity` once).
- **Disproof attempted**: is the model tier's field meant to carry the keyword heuristic as a spatial taper? Only before `592f31b87`; the commit message itself says both tiers "read one authored truth", so the multiplication is unintended. The non-localised path (all-ones masks, keyword affinity) is not squared.
- **Impact**: Every authored-model plant in a localised load order is thinned by another factor of the authored density (0.3 -> 0.09, 0.5 -> 0.25). Species mix is unaffected (the pick weights are unchanged); only the total rate is wrong. Not confirmed on screen (no capture allowed).
- **Related**: #4906 (closed), `592f31b87`, `c14dd2b39`.
- **Suggested Fix**: The model tier must not multiply the affinity the host already folded into `pick.accept`. Smallest change: evaluate the field with a neutral affinity for the model tier (pass `1.0` for `f.affinity` when `AuthoredCover::localised`, keep the keyword affinity otherwise, via a push-constant flag), or give `GroundCoverCell` a separate model-tier affinity. Pin it with a CPU mirror test: pure ground, one record of density `d` -> accept `d`.

### EXT-D1-2026-10-18-02: `apply_forced_weather` cross-fades over 8 s although `ForceWeather` is immediate; `ForcedWeather.locked` is stored and never read; the Show-Sky interior path ignores a forced weather
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

### EXT-D6-2026-10-18-01: Doc rot after the tree-LOD ring: it is described as "no BLAS / out of the TLAS" and `IsLodTerrain` points at a function that does not exist
- **Severity**: LOW (doc-rot)
- **Dimension**: Distant LOD and trees
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:519` ("No BLAS, lean static draw, out of the TLAS — shared with every LOD") and `:30` area; `docs/engine/exal.md:419` ("`IsLodTerrain`, no BLAS"); `byroredux/src/components/render.rs:177-185` (`[crate::render::collect_lod_terrain_draws]` "dedicated lean draw path", "skipped by the heavy `collect_static_mesh_draws` loop").
- **Status**: NEW (`a9384f9c0`), the `render.rs` pointer predates it.
- **Tier Violated**: n/a
- **Game Affected**: all (docs)
- **Description**:
  - `IsLodTerrain` entities go through the ordinary `collect_static_mesh_draws` loop (`render/static_meshes.rs:525`). Their TLAS membership is `tlas_exclusion(is_lod, ...)`: admitted when within `LOD_SHADOW_CASTER_DISTANCE` (24,000 BU) plus the entity's bound radius (`static_meshes.rs:46-49,137-140`). A tree-LOD quad's bound spans up to ~16k BU, so quads within roughly 35k BU of the camera build a BLAS and enter the TLAS as alpha-tested (`hitMat.alphaThreshold > 0`, handled by the coverage test in `shadow_transport.glsl:74-92`) architecture-layer casters. `terrain_lod.rs:11-14` states this correctly ("no BLAS **at spawn**"); the tree-LOD comment and `exal.md` do not.
  - `collect_lod_terrain_draws` does not exist in the repo or its history.
- **Impact**: A future reader prices the tree ring at zero RT cost, or looks for a lean path that is not there.
- **Suggested Fix**: Reword the three sites to the `terrain_lod.rs` wording; delete or repoint the `render.rs` link.

### EXT-D3-2026-10-18-02: `GROUNDCOVER_BLADE_FADE_START` is documented as a "long on purpose" fade but equals `GROUNDCOVER_FADE_START` since `b731b01f5`
- **Severity**: LOW (doc-rot)
- **Dimension**: Ground-cover pipeline
- **Location**: `crates/renderer/src/shader_constants_data.rs:573-579` (doc), `:579` (`= 7000.0`, same as `GROUNDCOVER_FADE_START` `:485`); `crates/renderer/shaders/include/groundcover_density.glsl:348-354` ("the procedural blade tier's own, longer fade").
- **Status**: NEW
- **Tier Violated**: n/a
- **Description**: Before the reach change the blade fade began earlier than the model fade; both now start at 7,000 and end at 8,000, so `byroGcBladeDistanceFade` and `byroGcDistanceFade` are the same function. The doc still promises "the stochastic thinning of far points starts well inside the draw distance". Behaviour is correct; the second constant and its comment are now vestigial.
- **Suggested Fix**: Either collapse to one constant or restore a distinct value deliberately; at minimum fix the comment.

## Findings count

| Dimension | CRITICAL | HIGH | MEDIUM | LOW |
|---|---|---|---|---|
| D1 EXAL boundary | — | — | 1 | 1 |
| D2 Terrain (override read) | — | — | — | — |
| D3 Ground cover | — | — | 1 | 1 |
| D4 Sky / weather / sun (skimmed) | — | — | — | — |
| D5 WATAL (skimmed) | — | — | — | — |
| D6 LOD / trees | — | — | — | 1 |
| D7 Gates / harness (skimmed) | — | — | — | — |
| **Total NEW** | **0** | **0** | **2** | **3** |

## Dimension notes (no findings)

- **Dim 1.**
  - `34a820ceb` resolution logic: `show_sky_interior_climate` uses `resolve_cell_climate(xccm, None, ..)` then `resolve_default_weather`, so it shares both boundary rules. With no `XCCM` it declines and keeps the current sky. Skyrim / FO4 never reach it (their `XCCM` is `sky_region`, `xccm_targets_region`, #5463). The install order is safe: `apply_environment_with_sources` inserts an exterior-flagged `CellLightingRes` and resamples with `weather_system(0.0)`, and `finish_interior_cell_load` / the `--cell` boot path overwrite it with the interior lighting and `InteriorSkyExposureRes(show_sky)` in the same step before any frame renders (`transition.rs:620`, `scene.rs:527`). Returning to the exterior goes through `apply_worldspace_weather`, so the realm sky does not leak.
  - `6eb1f378a`: `fog_colour_radiance` has one call site (`render/mod.rs:1368`); `FogMedium` carries no colour, so no second decode path. Guard `fog_colour_radiance_encodes_back_to_the_authored_display_value` runs green (it was outside the 10-12 filter set). The ambient-scale override is a cached diagnostic env var, interior only.
  - `apply_forced_weather` otherwise: the `applied` tri-state, the climate fallback and the `CurrentWeather` mirror (`world_setup.rs:371`) are consistent; `apply_cell_climate_override` routes through the same forced lookup.
  - `3b28dca71` (#5463): `xccm_targets_region` is Skyrim | FO4; FO76 / Starfield keep the CLMT decode and the doc says why. The misplaced doc comment it left on `read_form_id` was repaired by `61b9c5066` (#5965). `sky_region` has no consumer by documented forward scope.
- **Dim 2.** `592f31b87`'s override is bounded by `take(splat_layers.layers.len())`, so unused lanes keep zero affinity and the splat alphas are zero there. The base override reads `base_authored_grass`. Both land in `TerrainCoverInputs` (`terrain.rs:1270`). Its interaction with the model tier is D3-01.
- **Dim 3, other.**
  - `cover_masks_for_cell` / `collect_groundcover_frame`: record indices in the masks are `authored.records` order capped at `GROUNDCOVER_MODEL_MAX_RECORDS`, identical to the GPU record table and the selection weights (`collect_groundcover_model_records`), so masks and weights cannot drift. `cell_covers` is pushed in lock-step with `cells`. A truncated cell leaves a default (inactive) chunk record.
  - `gcPickLocalRecord`: lane shares are `alpha_i · Π_{j>i}(1 − alpha_j)` with the base keeping `Π(1 − alpha)`, matching the diffuse chain. `accept` and the lane `perWeight` split are consistent.
  - `b731b01f5` / `af8323881`: `chunk_in_view_wedge`, the ring (residents never re-slotted, evict-then-place at 24/frame), the 512-slot arena bound and the tier / morph helpers (`byroGcLodMorph`, `byroGcBladeWeight`, `byroGcCoverageWiden`) re-read; all guards run green. `d06ac5844`'s `MATERIAL_DECAL_SLOTS` is a `4u` literal compared to an `int` loop index, value-preserving, and the rebuilt `.spv` match.
- **Dim 4.** No sky / cloud / SH logic commits since the baseline: `1a2c48008` (docs), `d06ac5844`, `c6d36b4e7` (builders), `02187c7f9` (WTHS `DIFF` nesting cap at 64, bounded and tested). Guards `every_sky_cube_consumer_gates_on_the_ready_flag`, `composite_does_not_carry_its_own_copy_of_the_sky`, `image_space_is_promoted_when_the_transition_completes` are plain `#[test]` and passed.
- **Dim 5.** `eacaaa1dc` (#5705): `lod_water_rebuild_due` is `settled || waited >= 4 s`, gated by the 250 ms spacing; `lod_water_dirty_since` clears when nothing is wanted. A dirty flag set while there is no plane stays set (benign: it triggers one immediate rebuild when a plane appears). Guards `wind_angle_conversion_is_scoped_per_game`, `resolve_water_material_sentinels_are_game_invariant`, `lod_water_rebuilds_once_per_settled_crossing` passed.
- **Dim 6, other.**
  - `bb57270c3` fixes the 10-12 D6-02 and D6-03: `worldspace_lod_grid_origin` now reads `lod_worldspace_key()` (`lod_support.rs:112-135`), and `object_lod.rs:410` derives the archive and atlas paths from the same key (the finest band's hole mask stays on the child). The remaining `wctx.worldspace_key` reads in `terrain_lod.rs:449` are the full-detail hole mask, as intended.
  - **The 10-12 note about `placement_lod.rs:450` is closed.** The Oblivion per-cell `.lod` is keyed on the child (`placement_lod_archive_path(&wctx.worldspace_key, ..)`). A name-table census of `Oblivion - Meshes.bsa` shows Oblivion's children ship their own `distantlod\<child>_x_y.lod` (`chorrolworld` 16, `leyawiinworld` 13, `cheydinhalworld` 13, `anvilworld` 12, `bravilworld` 10, `skingradworld` 8, `kvatchplaza` 9, `icthearcaneuniversity`, ...), so keying on the child is right and not a defect.
  - `tree_lod.rs` (`a9384f9c0`): `parse_btt` / `parse_tree_lst` check every declared count against the bytes present, use `core::bytes` readers, and loop bounds are file-length-limited (a group header costs 8 bytes), so a hostile count cannot over-allocate. `build_tree_lod_mesh` drops unlisted species, non-finite and non-positive sizes instead of guessing. The Z-up to Y-up plan directions and the V orientation are consistent; the second card's direction is the negated perpendicular, which is the same line for a two-sided card. Teardown is covered: `state.tree_lod.drain()` feeds `unload_object_lod_block` in `drain_streaming_state` (`streaming_helpers.rs:584,643`), and eviction goes through the shared `retain_resident`. The ring selects by archive name, so it does not depend on the lattice origin (the reason it works on Tamriel while #2586 is open). A whole 4x4 quad is dropped as soon as one cell is resident (documented design, same rule as the `.bto` ring).
  - The Tamriel lattice origin (10-12 D6-01) is unchanged at HEAD: `lod_support.rs:112-135` still returns `usable_cell_bounds().min` and `grep -ri lodsettings byroredux/src crates` finds no reader. Tracked by #2586 (reopened 2026-10-13); not re-filed.
- **Dim 7.** No commits to the harness scripts. `scripts/check-shader-artifacts.sh` was run as the precheck the harnesses use (green).

## Dedup sources

- `/tmp/audit/issues.json` (119 open) plus a fresh `gh issue list --state all --search` for each finding's keywords ("Show-Sky interior cloud texture flush", "ground cover model tier affinity density squared", "ForceWeather crossfade", "BLADE_FADE_START", "tree LOD TLAS BLAS", "pending_dds_upload_count apply_environment"): no hits.
- Open and exterior-adjacent, unchanged by this pass: #2586 (Tamriel lattice origin, reopened), #5505, #3807, #5949 (`fog_colour_radiance` call-site pin), #5627, #3816, #5959, #5351.
- Closed and verified still in place: #5496 (now complete), #5463, #5705, #5785 (resolution half), #4906 (pick half; see D3-01), #4913.
- 10-12 findings: D6-01 -> #2586 (still open, still present). D6-02 / D6-03 -> fixed by `bb57270c3`. D1-01 -> fixed by `88b1f6dc1` (#5731, closed).

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #2586 (Skyrim baked-LOD lattice origin; Tamriel's `lodsettings\*.lod` SW cell (-96,-96) vs `NAM0` min (-57,-43)) | OPEN, present at HEAD, not re-reported. Skyrim Tamriel `.btr` / `.bto` still miss. The live `lod.coverage` check remains owed. |
| #5496 (PNAM `0x02` children's LOD keys) | CLOSED, now complete (`bb57270c3`). |
| #5505 (Oblivion child worldspaces draw no ground outside their own LAND) | OPEN, unchanged. |
| #3807 (ground-cover density streaming) | OPEN, unchanged. The authored-density pieces moved: `592f31b87` (blade tier) and `c14dd2b39` (model tier) landed, with D3-01 as the integration defect. |
| #4906 | CLOSED; the localisation is verified, the density composition is D3-01. |
| #4913 (Skyrim distant trees) | CLOSED; first audited here, no logic defect. Visual check is owed (static crosses, unsourced 0.5 cutoff are documented interpretations). |
| #5949 (fog colour decode call-site pin) | OPEN, unchanged. |
| #5463 `sky_region` consumer | forward scope (interior sky-from-region), no consumer by design. |
| #5222 live FO3 / FNV visual verification | Still owed. |
| Skyrim Tamriel baked-LOD live check; tree-LOD live look (Riverwood west ridge was the author's only check) | Owed. |
| Grass look / LOD-tier / view-wedge on-screen checks (no camera dolly, washout precheck re-mint) | Owed; unit guards cannot see GPU output. |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 1**: add `apply_show_sky_interior_climate` (#5785, called from `InteriorCellApplyJob::finish`) and `apply_forced_weather` (`d1b298921`) as environment installers, and the check "every `apply_environment*` caller is followed by a texture flush" (D1-01). Fold in the `ForcedWeather` lookup in `apply_environment`.
- **Dim 3**: the accept rate is `pick.accept × affinity × ...`; record that, since `592f31b87`, the cell affinity is the authored union when `localised`, and the model tier must not take it a second time (D3-01). Note `GROUNDCOVER_BLADE_FADE_START == GROUNDCOVER_FADE_START` (D3-02).
- **Dim 6**: add the correct TLAS statement for `IsLodTerrain` (BLAS on demand within `LOD_SHADOW_CASTER_DISTANCE`), and the closed `placement_lod.rs` child-key question (children ship their own `.lod`).
- **Dim 4**: unchanged.

## Cross-audit routing

- **`/audit-scripting`**: D1-02 (`ForcedWeather.locked` unread, ForceWeather semantic).
- **`/audit-oblivion` / `/audit-fnv` / `/audit-fo3`**: D1-01 (Show-Sky `XCCM` interiors).
- **`/audit-renderer`**: D1-01 (flush ownership), D6-01 (TLAS membership wording).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-18.md`.

**Labels:**
- D1-01: `bug`, `terrain-exterior`, `medium`, `renderer`.
- D3-01: `bug`, `terrain-exterior`, `medium`, `shaders`.
- D1-02: `bug`, `terrain-exterior`, `low`.
- D6-01: `documentation`, `doc-rot`, `terrain-exterior`, `low`.
- D3-02: `documentation`, `doc-rot`, `terrain-exterior`, `low`.
