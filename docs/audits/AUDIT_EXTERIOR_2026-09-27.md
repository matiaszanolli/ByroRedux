# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-09-27

**HEAD**: `0e0d35b96` · **Baseline**: [AUDIT_EXTERIOR_2026-09-21.md](AUDIT_EXTERIOR_2026-09-21.md) (HEAD `f97775ca8`, 251 commits since) · **Audited**: Dims 1–7, delta-first. Every dimension has commits since the baseline. · **Unchanged since baseline (skimmed)**: none at dimension level. Within Dim 2, `terrain_seam.rs` has no commits.

**Method.** Dims 1–6 each ran as one sub-agent, at most three at once. Dim 7 ran in the main context. Every CRITICAL, HIGH and MEDIUM finding was re-read against the code in the main context before it was accepted.

**Scope limits.** No engine launch and no smoke script were run. Tests ran through `cargo test` only. Real game data was read directly from ESM records, archive name tables, decoded DDS texels and Bethesda LOD mesh UVs, using scripts in `/tmp/audit/exterior/` (wiped at the end of the run, per the skill).

**Test state (first steps, all green at HEAD):**

| Suite | Result |
|---|---|
| `env_translate` | 80 passed, 2 ignored |
| `env_health` | 21/0 |
| `terrain` | 76 passed, 2 ignored |
| `groundcover`, renderer | 51/0 |
| `groundcover`, bin | 50/0 |
| `sky_` | 32 passed, 1 ignored |
| GPU `gpu_filter_preserves_constant_radiance_and_broadens_a_lobe` (`--ignored`, Vulkan 1.4.341) | pass |
| `weather` | 67/0 |
| `water` | 135 passed, 3 ignored |
| `lod` | 133/0 |
| `resident_vwd` | 7/0 |

`scripts/check-shader-artifacts.sh` reports that 36 shaders plus the opaque early-test variant are byte-identical under glslang 11:16.2.0. No exterior `.spv` is stale.

## Executive Summary

**41 findings: 0 CRITICAL · 3 HIGH · 13 MEDIUM · 25 LOW.** All are NEW, except one regression of a guard (EXT-D5-06, #4734's deleted test). None duplicates an open issue.

All 12 baseline findings were filed as #4727–#4738 and are closed. This pass re-read every one of those fixes:

- **Correct and complete (5):** #4728, #4729, #4731, #4735 and #4738.
- **Correct, but a sibling gap exists (1):** #4732. Its pin now drives the production function.
- **Incomplete (6):**
  - **#4727** double-rotates NAM0-filled layers, and it silently rotates the angles of every game, not just the two its census covered (EXT-D5-01, EXT-D5-02).
  - **#4734**'s only test was deleted seven minutes later by #4727's commit (EXT-D5-06).
  - **#4733** bound `image_space: _` under a false rationale (EXT-D1-01).
  - **#4737**'s probe `TOTAL` still prints 0 (EXT-D6-04).
  - **#4736** left two stale census citations (EXT-D6-05).
  - **#4730**'s contract guard cannot see the *.sh.sh* regression it was written for (EXT-D7-01).

Other features landed since the baseline, and each brought new wrong-output defects:

| Feature | Findings |
|---|---|
| #4413 authored-model ground cover | EXT-D3-01, EXT-D3-03, EXT-D3-04, EXT-D3-08 |
| #4642 GNAM array | EXT-D2-03: the merge drops the whole map |
| #4056 ground-colour coupling | EXT-D3-02 |
| #4416 image spaces | EXT-D1-01 |
| `0572bfd5a` interior godrays / sky apertures | EXT-D1-02, EXT-D4-01, EXT-D4-02, EXT-D4-05 |

Open **#4866 is already fixed in code** by `88c23887b`. The timer bracket encloses all three model-tier dispatches and the stats copy, pinned by `model_timer_encloses_all_phases_and_stats_copy`, so #4866 can be closed.

### The three HIGHs

1. **EXT-D6-01: legacy distant-terrain quads are sampled with the wrong orientation.**
   - Oblivion, FO3 and FNV are affected, for both diffuse and normal maps: FO3/FNV rotated 180°, Oblivion flipped north/south.
   - Three independent measurements agree:
     - Bethesda's own LOD mesh UVs;
     - LAND height gradients correlated with the decoded LOD normal texels;
     - seam continuity across shared quad edges.
   - The engine's `v = 1 − y` convention dates from #1745, which never measured orientation. This affects the population `exal.md` calls the must-have.
2. **EXT-D2-01: LAND ground-cover affinity is classified from the TXST diffuse path.** The keyword table was derived from LTEX editor IDs, so every `…NoGrass` variant (which reuses its grassy sibling's texture) scores as full grass. Every `…Grass` variant of barren ground scores as barren.
3. **EXT-D5-01: #4727's +90° frame conversion also rotates layers the parser already filled from NAM0 in the engine frame.** Those layers slide across the current they were copied from.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds, with dents.** WTHR translate callers are still `scene/world_setup.rs`, `systems/weather.rs` and `cornell.rs`, with no stray `SkyParamsRes` / `WeatherDataRes` literals. Water still has two production composition sites. GRAS enters only through `resolve_authored_cover` (single caller). The dents: (1) `render/sky.rs` is a 4th caller of `procedural_fallback_sky` (EXT-D1-02); (2) `image_space` is patched in after `translate_weather` (EXT-D1-03); (3) `merge_from` drops `landscape_grasses` before the translate (EXT-D2-03); (4) XCWU replaces `WaterFlow` without recomposing scroll (EXT-D5-03); (5) the froxel medium derives a third "sun" (EXT-D4-02); (6) `TranslatedTerrainLodTexture` carries no orientation (EXT-D6-01). |
| **no-fabrication** | **Dented.** Three canonical values are wrong: path-keyed affinity (EXT-D2-01), the 0.15 base affinity stand-in (EXT-D2-02) and the hard-coded LOD UV convention (EXT-D6-01). A game-wide rotation is backed only by a two-game census (EXT-D5-02). The model tier places GRAS by climate keyword, not by the authored LTEX→GRAS link (EXT-D3-01). |
| **no-leak** | **Holds, with dents.** Wind is fixed (#4729), and #4728 unified the scroll sign. The dents: the blade ground colour is not the terrain's colour (EXT-D3-02); the Rapids layer and the BGSM flow-map use raw or pre-#4728 units and signs (EXT-D5-04, EXT-D5-05); env.health gates neither `sunlight_color` nor `image_space` (EXT-D1-05). |
| **no-render-time-fallback** | **Broken, narrowly.** Direct `--cell` interior boots rebuild the procedural sky and sun arc every frame in `render/sky.rs` (EXT-D1-02). The density shader substitutes a default affinity for a base that has a real LTEX (EXT-D2-02). No `game ==` logic was added. No game token appears in the sky, cloud, ground-cover, water, composite or volumetrics GLSL, and `grep GameKind crates/physics/src` is empty. |

## Per-Category Matrix

Boundary function cited per category. ✓ = holds; the Findings column lists the exceptions.

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✗ GNAM dropped at merge (D2-03) | ✗ path-keyed affinity (D2-01); base stand-in (D2-02) | ✓ | ✗ shader default affinity (D2-02) | D2-01..05 |
| **Sky / clouds / bake** (`translate_sky` / `SkyCubeParams::from_composite`) | ✗ procedural sky built in the render loop (D1-02); second hand packer (D4-03) | ✓ | ✓ gated in shaders (#4861 unguarded) | ✗ D1-02 | D1-02, D4-01, D4-03, D4-05 |
| **Weather / sun** (`translate_weather`) | ✗ `image_space` patched in by the caller (D1-03); third sun in the medium (D4-02) | ✓ | ✗ ungated `sunlight_color` / `image_space` (D1-05) | ✓ | D1-01, D1-03..05, D4-02, D4-04, D4-06 |
| **Water, WATAL** (`resolve_water_material` / `attach_mesh_water`) | ✗ XCWU vs scroll (D5-03) | ✗ double rotation (D5-01); game-wide rotation (D5-02) | ✗ D5-04, D5-05 | ✓ | D5-01..07 |
| **Ground cover** (`groundcover_translate` / `resolve_authored_cover` / `WindField`) | ✓ | ✗ authored LTEX→GRAS ignored (D3-01) | ✗ coupling ≠ terrain blend (D3-02) | ✓ | D3-01..08 |
| **Distant LOD / trees** (`terrain_lod_layout` / `object_lod_scheme`) | ✗ orientation not in contract (D6-01); `.btr` clamp dropped (D6-02) | ✗ D6-01; probe false zeros (D6-04); doc premise (D6-06) | ✓ MSN carried as a canonical flag | ✓ | D6-01..07 |

Cross-cutting: **D7-01, D7-02** (acceptance harness).

## Findings

### EXT-D6-2026-09-27-01: Legacy distant-terrain quads are sampled with the wrong image orientation — FO3/FNV rotated 180°, Oblivion flipped N/S, for diffuse and normal
- **Severity**: HIGH. This is a wrong canonical UV contract at the LOD boundary, affecting every legacy exterior, with no render-time fallback.
- **Dimension**: Distant LOD and trees
- **Tier Violated**: single-boundary / no-fabrication
- **Game Affected**: Oblivion, FO3, FNV
- **Location**:
  - `byroredux/src/cell_loader/terrain_lod.rs:800-810` (`u = (wx−ox)/S`, `v = 1 − (wy−oy)/S`, one convention for all three games).
  - `byroredux/src/env_translate.rs:72-81` (`TranslatedTerrainLodTexture` carries no orientation).
- **Status**: NEW. The convention dates from `b29e62751` (#1745), whose message records no orientation check.
- **Description**: Three independent measurements agree, and all contradict the engine's assumption that image-right is east and image-top is north. The texture upload does no row flip, so v = 0 is the first stored row.
  1. **Bethesda's own LOD mesh UVs**, the authoring contract:
     - FNV `wastelandnv.level4.x0.y56` / `x44.y60` and FO3 `wasteland.level4.x0.y0` / `x-20.y16` all map (0,0)→(1,0) and (S,0)→(0,0), which is **u = 1−x, v = y**.
     - Oblivion `60.00.00.32.nif` gives **u = x, v = y**.
  2. **LAND ground truth.** The decoded LOD normal texels were correlated with VHGT height gradients.
     - FNV, 5 quads at u=1−x, v=y: corr(R, east) is +0.92…+0.97 and corr(G, north) is +0.96…+0.99. At the engine mapping the correlation is about 0.
     - Oblivion Tamriel, 5 quads at u=x, v=y: corr(R, east) is +0.55…+0.88. At the engine mapping it is ≤ +0.13.
  3. **Seam continuity.** This is the mean absolute luminance step across shared quad edges.
     - FNV diffuse: 0.2–2.1 under the corrected mapping against 1.6–50.9 under the engine's.
     - Oblivion: v=y gives 8.4–12.1 against 16.3–43.7 for v=1−y.
- **Evidence**:
  - The `/tmp/audit/exterior/` scripts: `land_heights*.py`, `land_vs_lod*.py`, `edge_orient.py`, the `lodnm/` UV fit and `nm_convention.py`.
  - The main-context re-check confirmed the single hard-coded mapping, and that #1745 carried no orientation evidence.
- **Impact**:
  - Every legacy distant-terrain quad paints its baked colour 180° away (FO3/FNV) or mirrored N/S (Oblivion) within its footprint, with hard seams at every quad border.
  - The normal map is sampled at the same wrong texel, so distant relief is lit from an unrelated patch of terrain.
- **Related**: #1745, `b50a8e6a9`, #3100, #2822. This answers the baseline's open NIFAL-D1-21b-01 question: the basis matches, but the sampling location is wrong.
- **Suggested Fix**:
  - Add a per-layout orientation to the translate output: Oblivion u=x, v=y; FO3/FNV u=1−x, v=y.
  - Give the synthesized LOD vertices an explicit tangent: +X east, w = +1, not `new_terrain`'s LAND value of −1. Today they carry a zero tangent and shade through the derivative frame, which flips once the UV is corrected and would invert the relief.
  - Pin both with a corpus test.
  - Confirm with a captured distant-terrain frame (`m-exteriors.sh fnv static`).

### EXT-D2-2026-09-27-01: LAND `cover_affinity` is classified from the TXST diffuse path; the keyword table is an LTEX editor-ID table, so every NoGrass / …Grass variant is inverted
- **Severity**: HIGH. This is a wrong canonical value out of the EXAL LAND translate.
- **Dimension**: Terrain, splatting
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion, FO3, FNV, Skyrim, FO4
- **Location**:
  - `byroredux/src/cell_loader/terrain.rs:324` (`layer_affinity(texture_path.unwrap_or(""))`).
  - `byroredux/src/groundcover_translate.rs` (`SUPPRESSION_KEYWORDS` / `AFFINITY_KEYWORDS`).
  - `byroredux/src/groundcover_translate_tests.rs:79-88`.
- **Status**: NEW
- **Description**:
  - `exal-groundcover.md:590-600` derives the table "by tokenising every editor ID". Its first rule is that a `NoGrass` suffix suppresses cover.
  - Production passes the diffuse path instead. `EsmCellIndex` carries no LTEX editor ID.
  - NoGrass variants reuse the grassy sibling's texture, so suppression never matches a real path.
  - `oblivion_icon_paths_resolve_like_editor_ids` asserts on a made-up path (`Dementia\DementiaMoss01NoGrass.dds`); the real ICON is `Dementia\DementiaMoss01.dds`.
- **Evidence**:
  - A census of editor ID versus real path, run through a replica of `layer_affinity`, finds disagreements on 34/229 Oblivion LTEXs, 6/51 FO3, 15/88 FNV, 18/67 Skyrim and 47/105 FO4.
  - NoGrass records scoring above 0: Oblivion 26, FNV 1, Skyrim 14, FO4 14. Examples:
    - `LFieldGrass01NoGrass` → `Landscape\FieldGrass01.dds`: 0 → 0.95.
    - `CHTerrainGrass01NoGrass`: 0 → 0.95.
  - Reverse cases:
    - `ChemicalBarrenWastes01Grass`: 0.95 → 0.02.
    - `LSnowRocks01wGrass`: 0.95 → 0.03.
- **Impact**:
  - Paths and building pads authored as NoGrass get maximum grass.
  - Grassy variants of barren ground get none.
- **Related**: #4054, EXT-D2-02
- **Suggested Fix**:
  - Carry LTEX editor IDs through the ESM boundary as `EsmCellIndex.landscape_texture_names`, merged in `merge_from`.
  - Classify on the editor ID. Oblivion has editor IDs too, so no per-game split is needed.
  - Replace the made-up-path test with a real editor-ID/path pair.

### EXT-D5-2026-09-27-01: NAM0-filled WATR layers are rotated 90° off the current — #4727's conversion is applied to angles the parser already wrote in the engine frame
- **Severity**: HIGH. The skill rule applies: this is a wrong canonical value out of the WATAL translate. The vanilla reach is one FO4 record, but any NAM0 water with a zero-speed layer is affected.
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: single-boundary + no-fabrication
- **Game Affected**: Skyrim, FO4, FO76, Starfield (all NAM0 games). Vanilla case: FO4 `IntOldGulletWaterSlow` (0x1E214D).
- **Location**:
  - `crates/plugin/src/esm/records/misc/water.rs:1502-1515`: the NAM0 fill writes `(-y).atan2(x)`, which is already in engine XZ, into `noise_wind_directions`.
  - `byroredux/src/env_translate.rs:859-871` (`resolve_water_layer_motion` → `watr_angle_to_engine_xz`, +90° on every layer).
- **Status**: NEW (introduced by #4727's fix, `a6a210eb9`)
- **Description**:
  - `noise_wind_directions` now holds two frames: DNAM layers in record-frame bearings, and NAM0-filled layers already in the engine frame.
  - The translate cannot tell them apart and rotates both, so a filled layer runs perpendicular to the current it was copied from.
  - The fill also puts a BU/s speed into a UV/s slot.
- **Evidence**:
  - `IntOldGulletWaterSlow` has NAM0 (0.08, 0, 0) and layer speeds (0.0072, 0.0, 0.0145), so layer 1 is filled along +X.
  - The record classifies as River, with a flow term of 0.0228 UV/s. The emitted `scroll_b` is about (0, 0.091) UV/s: perpendicular to the current, at 4× the flow term. Before #4727 it ran along the flow.
  - Every translate test builds `WatrRecord` directly, so the parse→translate path is unpinned.
- **Impact**: The normal layer slides across the current on affected water. The same applies to every mod with a NAM0 water and an unset layer.
- **Related**: #4727, EXT-D5-02
- **Suggested Fix**:
  - Store the filled layers in the record frame (φ − 90°), or move the fill into the translate after the conversion.
  - Convert the filled speed to UV/s.
  - Add a parse→translate test asserting that a filled layer is parallel to `WaterFlow.direction`.

### EXT-D1-2026-09-27-01: A WTHR cross-fade never promotes `image_space` — the exterior grade snaps back to the source weather once the transition completes
- **Severity**: MEDIUM (visual; affects the whole session after any weather transition)
- **Dimension**: EXAL boundary discipline × Sky, weather, sun
- **Tier Violated**: n/a. A canonical lane is dropped at the promotion step; this is the fifth recurrence of the #4481 class.
- **Game Affected**: FO3/FNV (worldspace INAM changes, XCCM crossings); Skyrim/FO4 (weathers with differing IMSP)
- **Location**:
  - `byroredux/src/systems/weather.rs:1286-1302` (`image_space: _` in `promote_weather_transition_target`).
  - Consumer: `:854`, `:1213-1234`.
  - Also reached via `scene/world_setup.rs:723-728` (`collapse_weather_transition`).
- **Status**: NEW (incomplete fix of #4416 and #4733)
- **Description**:
  - #4733's exhaustive destructure fired on `image_space`, and the field was bound `_`. The rationale given was "the completion frame lerp-samples the target's image space".
  - That holds only on the completion frame. Afterwards `tr.done` latches and `transition_t` is 0.0 (`:781-782`).
  - From then on the grade is sampled from the unpromoted `wd.image_space` (`:854`) and published to `ImageSpaceBase`.
- **Evidence**:
  - The promotion writes 14 fields but not `image_space` (re-read in the main context).
  - No test drives a transition past completion and then checks `ImageSpaceBase`.
- **Impact**: The grade fades to the target over 8 s, then reverts to the old weather's grade for the rest of the session.
- **Related**: #4416, #4733, #4481
- **Suggested Fix**:
  - Bind `image_space: tr_target_image_space` and assign `wd.image_space = tr_target_image_space`.
  - Add a test that checks `ImageSpaceBase` equals the target's sample on the frame after `done`.

### EXT-D1-2026-09-27-02: `render/sky.rs` builds the procedural exterior sky and sun arc inside the render loop on direct `--cell` interior boots
- **Severity**: MEDIUM. The severity table's "EXAL translation done at render time" row would make this HIGH. It is downgraded because the render-side values equal the canonical fallback today: the same `procedural_fallback_sky`, `compute_sun_arc` and `FB_SUN_COLOR`.
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: no-render-time-fallback (and a single-boundary dent)
- **Game Affected**: all interior-only `--cell` sessions (window portals, Show-Sky interiors, apertures, godrays)
- **Location**:
  - `byroredux/src/render/sky.rs:121-140`: the else-arm of `build_sky_params` calls `procedural_fallback_sky` and `compute_sun_arc` every frame.
  - `:73-98`: `portal_sun` has its own copy of the arc.
  - `env_translate.rs:1554`: `FB_SUN_COLOR` was made `pub(crate)` by `0572bfd5a`.
- **Status**: NEW (introduced by `0572bfd5a`, extended by `a4a68fa92`)
- **Description**:
  - `exal.md` §3 contract 2 requires the no-climate case to be an explicit canonical default, not a render-loop branch.
  - There are now three render-side derivations that must be kept in step with `weather_system` by hand.
- **Evidence**: Main-context re-read of `build_sky_params` and `portal_sun`. `git log -S'procedural_fallback_sky(direction)'` points to `0572bfd5a`.
- **Impact**: Latent divergence. A change to the fallback palette or sun model would silently split the interior-portal sky from the exterior fallback.
- **Related**: #3323, #4839, EXT-D1-04, EXT-D4-02
- **Suggested Fix**:
  - On an interior-only boot, install the canonical defaults once (`procedural_fallback_sky`, `procedural_fallback_weather`, `GameTimeRes`) and let `weather_system` advance them.
  - Delete the render-side arms, and make `FB_SUN_COLOR` private again.
  - Land together with EXT-D1-04, or env.health FAILs every interior.

### EXT-D2-2026-09-27-02: The canonical base LTEX is never translated into the cover inputs — the density field fabricates 0.15 for unpainted base and ignores the base under partial paint
- **Severity**: MEDIUM (visual)
- **Dimension**: Terrain, splatting
- **Tier Violated**: no-render-time-fallback (and no-fabrication)
- **Game Affected**: all LAND games
- **Location**:
  - `byroredux/src/cell_loader/terrain.rs:1036-1043`.
  - `crates/renderer/shaders/include/groundcover_density.glsl:119-129` (`byroGcAffinity`).
- **Status**: NEW. It shares its root cause (no base lane in `TerrainCoverInputs`) with EXT-D3-02.
- **Description**:
  - `coverAffinity0/1`, `layer_affinity` and `authored_grass` describe only the 8 splat lanes. The first-quadrant BTXT base has a real LTEX, but neither its affinity nor its GNAM is emitted.
  - With every lane weight at 0, the shader substitutes 0.15. Its comment says the base "has no LTEX record", which is true only when BTXT is 0.
  - The shader normalises by painted weight, so 30% dirt over tundra reads as pure dirt, while `triangle.frag`'s ordered `mix` shows 70% tundra.
  - The same LTEX gets its true affinity as a non-canonical quadrant base and 0.15 as the canonical one.
- **Evidence**:
  - Vertices sitting unpainted on the canonical base: Skyrim 43.8%, FNV 34.5%, FO4 24.8%.
  - Partially painted vertices: Skyrim 25.6%, FNV and FO4 about 41%.
  - `LSnow01` (true affinity 0.02) gets 0.15, 7.5× too much, across 2.9 M vertices.
- **Impact**:
  - Sparse grass grows on snowfields, beaches and rock.
  - Grass-textured bases are under-vegetated.
  - Density depends on which quadrant was picked as canonical.
- **Related**: EXT-D3-02, EXT-D2-01, EXT-D2-03, #4054
- **Suggested Fix**:
  - Emit the base as an extra lane, with base affinity and base GNAM.
  - Compose affinity in the diffuse loop's order: `a = base; a = mix(a, aff[i], w[i])`.
  - Drop both the substitution and the normalisation, and pin the formula with a host-side test.

### EXT-D2-2026-09-27-03: `EsmCellIndex::merge_from` never merges `landscape_grasses` — every production load has an empty LTEX→GRAS map
- **Severity**: MEDIUM. It is latent because nothing consumes the map yet (EXT-D3-01); it becomes HIGH when the authored-card wiring lands.
- **Dimension**: Terrain, splatting (ESM→EXAL handoff)
- **Tier Violated**: single-boundary (the authored value is dropped before the translate)
- **Game Affected**: FO3, FNV, Skyrim, FO4
- **Location**: `crates/plugin/src/esm/cell/mod.rs:1587-1597`; `byroredux/src/cell_loader/load_order.rs:560,630`
- **Status**: NEW (gap in #4642)
- **Description**:
  - `merge_from` extends 13 of the 14 `EsmCellIndex` maps; `landscape_grasses` is the only one missing.
  - The production path starts from `EsmIndex::default()` and merges every plugin, master included, so the map is empty even with a single ESM.
  - Main-context re-check: the only writer is `crates/plugin/src/esm/records/parse.rs:158`.
- **Evidence**: A scratch probe on `FalloutNV.esm`:
  - `parse_esm` alone gives `landscape_grasses=20`.
  - After `EsmIndex::default().merge_from(..)` it is 0, while `landscape_textures` stays 88.
  - No test goes through the merge.
- **Impact**: #4642's decode is dead at runtime, so `authored_grass` is empty everywhere.
- **Related**: #4642, #4413, EXT-D3-01
- **Suggested Fix**:
  - Add `self.landscape_grasses.extend(other.landscape_grasses)`; the last writer per LTEX wins.
  - Destructure `other` exhaustively so the next new field is a compile error.
  - Add a load-order merge test on a synthetic LTEX+GNAM plugin.

### EXT-D2-2026-09-27-04: The BTXT feather covers only the quadrant edges inside a cell; the same base disagreement across a cell edge stays a hard cut
- **Severity**: MEDIUM (visual)
- **Dimension**: Terrain, splatting
- **Tier Violated**: n/a
- **Game Affected**: all LAND games
- **Location**: `byroredux/src/cell_loader/terrain.rs:216-240` (`base_transition_alpha`); the doc claim is at `:151-158`
- **Status**: NEW
- **Description**:
  - Internal quadrant edges whose bases differ get a 0.5 weight on the shared vertex.
  - The cell's outer edges get nothing, so neighbouring cells with different facing bases meet along a hard line: the seam the comment says is removed.
- **Evidence**: FNV census.
  - Internal quadrant pairs with different bases: 4655/15302 (30%).
  - Cross-cell facing pairs with different bases: 4829/15010 (32%).
  - On those cross-cell pairs, 32,985 of 82,093 edge vertices are under 0.99 ATXT coverage on both sides.
- **Impact**: Hard base-texture lines along cell boundaries. This matches vanilla, so it is an inconsistency in the project's own improvement rather than a regression.
- **Related**: #470
- **Suggested Fix**: Choose one:
  - Feed each neighbour's facing quadrant bases into `base_transition_layers_for_bases`. This needs a lane-budget change, because the cap's at-most-4-transitions premise moves.
  - Document that cell edges are kept at vanilla parity.

### EXT-D3-2026-09-27-01: Phase C ignores the authored LTEX→GRAS association — every load-order GRAS is placed everywhere by climate keyword, and its density is diluted by the record count
- **Severity**: MEDIUM (visual)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion, FO3, FNV, Skyrim, FO4
- **Location**:
  - `byroredux/src/groundcover_translate.rs:354-372,384` (`resolve_authored_cover` over `record_index.grasses`; `classify_species_name`).
  - `byroredux/src/components.rs:458-469` (`authored_grass`, `#[allow(dead_code)]`).
  - `crates/renderer/shaders/groundcover_models.comp:280,305`.
- **Status**: NEW. `exal-groundcover.md:1645-1651` documents only the localisation part.
- **Description**: The widened carrier has no consumer. Main-context re-check: `authored_grass` is read only by its producer and tests. Three consequences:
  - **(a) No localisation.** Species follow worldspace climate only.
  - **(b) Load-order-wide mix.** DLC and mod GRAS appear in every worldspace.
  - **(c) Diluted density.** A candidate first picks one record from the 256-entry table (`h >> 24`), then accepts with probability `rec.density × field × fade`.
    - Every added record therefore thins all the others.
    - Records whose model failed to load keep their table share and place nothing.
- **Evidence**: `groundcover_models.comp:280` (`gcRecordTable[h >> 24]`) and `:305` (`rec.density * field * fade`).
- **Impact**:
  - The wrong species grows per region.
  - Species bleed across DLCs.
  - Visible density depends on how many GRAS records the load order carries.
- **Related**: #4642, #4413, EXT-D2-03 (the map is also empty at runtime, so fix that first)
- **Suggested Fix**:
  - Resolve per-lane GNAM lists into `AuthoredCover` indices at the translate, and carry them in the cell data.
  - Weight record selection by lane splat × climate.
  - Evaluate density per record, or normalise by selection share.

### EXT-D3-2026-09-27-02: §12.3 ground-colour coupling blends toward the renormalized average of the ATXT overlays only — the BTXT base the terrain shows is ignored
- **Severity**: MEDIUM (visual)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: no-leak
- **Game Affected**: all games with splat terrain
- **Location**:
  - `crates/renderer/shaders/groundcover_blade.frag:145-169`.
  - Compare `crates/renderer/shaders/triangle.frag:389-409`.
  - `byroredux/src/cell_loader/terrain.rs:149-150`.
- **Status**: NEW (introduced by `c14f5361a`)
- **Description**:
  - The terrain builds its colour as the BTXT base followed by an ordered `mix(prev, layer, w)`.
  - The blade uses Σw·layer/Σw over the ATXT layers only. On BTXT dirt with a 0.05 grass overlay, the terrain shows about 95% dirt, but the blade roots take 100% of the overlay colour.
  - The response is discontinuous at Σw = 0.
  - The comment's premise ("the blade's base has no BTXT") is false: BTXT is the terrain entity's `TextureHandle`, just not carried into `GroundCoverCell`.
- **Evidence**: Main-context re-read of both loops.
- **Impact**: Blade roots take the colour of faint overlays, with hard seams along overlay edges.
- **Related**: #4056, EXT-D2-02 (same missing base lane)
- **Suggested Fix**:
  - Carry the BTXT diffuse index into the cell data.
  - Share one GLSL mix-chain helper with `triangle.frag`.
  - Pin that the blade's ground colour equals the terrain's.

### EXT-D4-2026-09-27-01: Interior Show-Sky and aperture backgrounds render with no stars, moon or aurora — `weather_sky_details` gates on the room's `depth_params.x`
- **Severity**: MEDIUM (visual)
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: all (interiors with Show Sky / Behave Like Exterior, or authored LightShaft apertures, at night)
- **Location**:
  - `crates/renderer/shaders/include/sky.glsl:84-87`.
  - `crates/renderer/shaders/composite.frag:422`.
  - `crates/renderer/src/vulkan/context/draw.rs` (`build_composite_params` sets `depth_params[0]` = room `is_exterior` = 0; the interior cube arm sets 1).
- **Status**: NEW
- **Description**:
  - `0572bfd5a` made interior composites paint the outdoor palette through Show Sky and bounded apertures. It deliberately kept `depth_params.x = 0` so rain and height fog stay off.
  - The pre-existing `weather_sky_details` returns early on `depth_params.x <= 0.5`, dropping the stars, the moon and the aurora.
  - The interior cube bake packs 1, so the same room's window-portal escape does show them.
- **Evidence**:
  - Main-context re-read of `sky.glsl:84`.
  - `interior_portal_sky_preserves_room_weather_gate` asserts composite 0 and cube 1.
- **Impact**: At night an open-roof interior shows a starless, moonless sky, while the glass in the same room shows stars.
- **Related**: #4861, EXT-D4-05
- **Suggested Fix**:
  - Gate the details on "an outdoor palette is drawn" (`depth_params.x > 0.5 || sky_lower.w > 0.5`), and keep `depth_params.x` for weather and fog.
  - Extend the existing test.

### EXT-D4-2026-09-27-02: The froxel medium's sun ignores cloud cover and the HNAM sunlight dimmer, and uses the sun-disc colour at the raw 0–4 scale
- **Severity**: MEDIUM (visual; a third derivation of the sun)
- **Dimension**: Sky, weather, sun (volumetrics consumer, co-owned by `/audit-renderer`)
- **Tier Violated**: single-boundary
- **Game Affected**: all exteriors; interiors via `portal_sun`
- **Location**:
  - `crates/renderer/src/vulkan/context/post_passes.rs:53-78` (`volumetric_sun`).
  - `byroredux/src/render/sky.rs:73-85` (`portal_sun`).
  - `crates/renderer/shaders/volumetrics_inject.comp:3022`.
- **Status**: NEW. `volumetric_sun` arrived in `0572bfd5a`, and no prior report or issue covers it.
- **Description**: There are three "sun" radiances.
  - **Surface key:** `SKY_SUNLIGHT × sunlight_dimmer × (sun_intensity/4) × exp(-2.5·coverage)`, from `compute_directional_upload`.
  - **Cloud march:** the same `sun_illuminance`. skyal.md explicitly rejects `sun_color * sun_intensity` as "the sun disc colour and its raw 0-4 scale".
  - **Volumetric in-scatter:** still `sky.sun_color * sky.sun_intensity`, with no dimmer and no cloud transmittance.
- **Evidence**: Main-context re-read of `volumetric_sun`'s exterior arm.
- **Impact**:
  - Under full overcast the terrain key drops to about 8% of clear sky, but sunlit haze and godrays stay at clear-sky strength. That is about 12× too bright relative to the key.
  - HNAM-dimmed worldspaces get undimmed shafts.
- **Related**: #4785, #4839, EXT-D1-02
- **Suggested Fix**:
  - Feed the medium from `sun_illuminance`: the exterior's own value, or for interiors the portal palette's value from #4839.
  - Make `portal_sun` return only a direction.
  - Add a test that a coverage=1 weather dims `volumetric_sun`.

### EXT-D5-2026-09-27-02: #4727's +90° "compass bearing" conversion applies to every game — the census supports only Skyrim/FO4, and Oblivion's layer 0 is a Cartesian vector
- **Severity**: MEDIUM. The correct frame for FO3/FNV/Starfield is undetermined rather than proven wrong. For Oblivion the rotation is unsupported by any evidence.
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication
- **Game Affected**: Oblivion, FO3, FNV, FO76, Starfield
- **Location**:
  - `byroredux/src/env_translate.rs:539-556`, applied at `:866` and `:931`.
  - Inputs in `crates/plugin/src/esm/records/misc/water.rs`: Oblivion `:559-565` (`y.atan2(x)` of DATA 28/32), FO3/FNV `:759-763`, FO76/Starfield `:1285-1289`.
- **Status**: NEW (scope gap in #4727)
- **Description**:
  - **Oblivion.** Layer 0 is the angle of the editor's (x, y) scroll pair, not a bearing, so the output changed from (x, y) to (−y, x). This affects e.g. `DefaultWater`.
  - **FO3/FNV.** 71/78 FNV and 47/53 FO3 records carry non-zero layer speeds, and none has a NAM0 to census against. FNV `CreekWater01` layer 0 (0.228 UV/s) now runs along +Z instead of +X.
  - **The re-run census:**

    | Game | Layers | Mean offset after conversion | R |
    |---|---|---|---|
    | Skyrim | 111 | +6.2° | 0.74 |
    | FO4 | 107 | +1.5° | 0.65 |
    | FO76 | 138 | +12.5° | 0.36 (weak) |
    | Starfield | 36 | — | 0.21 (no support either way) |

  - Neither the code doc nor watal.md scopes the conversion by game.
- **Impact**: Authored layer motion on Oblivion, FO3, FNV and Starfield changed direction on 2026-09-24 without evidence. FNV is the reference title.
- **Related**: #4727, EXT-D5-01, #3144
- **Suggested Fix**:
  - Convert per layout at the parse boundary: Skyrim and FO4, FO76 tentatively. Leave Oblivion's Cartesian pair unrotated.
  - Record FO3/FNV and Starfield as open in watal.md §2.
  - Add per-game pins.

### EXT-D5-2026-09-27-03: A REFR XWCU current replaces the `WaterFlow` but not the scroll composed from the WATR flow — pattern and physics current diverge
- **Severity**: MEDIUM (visual)
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: single-boundary
- **Game Affected**: Skyrim placed mesh water via WNAM; any game with REFR XWCU
- **Location**:
  - `byroredux/src/cell_loader/water.rs:684-715` (`merge_placed_water`: `reference_flow.or(watr_flow)`; material untouched).
  - `byroredux/src/env_translate.rs:936-963`.
- **Status**: NEW
- **Description**:
  - `scroll_a`/`scroll_b` are composed from the WATR flow direction.
  - When XWCU swaps the flow, physics, foam streaks and wave B follow XWCU, while the normal-map flow term follows the WATR.
  - The code comment "in vanilla the two are equal" is false.
- **Evidence**: Across 128 Skyrim.esm REFRs with XWCU, one `RiverWaterFlowSE` REFR is 78.7° off its WATR and one `CreekWaterFlow` REFR is 52° off. The rest agree within 3.8°.
- **Impact**: On those placements, ripples run up to about 79° across the current carrying floating bodies.
- **Related**: #3974, #4728
- **Suggested Fix**:
  - Extract a translate helper that composes scroll from a given flow, and call it from `merge_placed_water` when XWCU wins.
  - Pin it with differing axes.

### EXT-D6-2026-09-27-02: `.btr` distant terrain samples its diffuse and normal with WRAP; the NIFs author CLAMP_S_CLAMP_T (#4553 did not reach the texture-only LOD families)
- **Severity**: MEDIUM (visual)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: single-boundary
- **Game Affected**: Skyrim, FO4 (and FO3/FNV synthesized quads)
- **Location**:
  - `byroredux/src/cell_loader/terrain_lod_btr.rs:335,342,360` (`resolve_texture` / `resolve_linear_texture` at WRAP).
  - `:456` (the boundary call carries the MSN bit but not the clamp).
- **Status**: NEW (unfixed sibling of closed #4553)
- **Description**: Every imported land shape authors `texture_clamp_mode = 0`: 5 Skyrim quads and 3 FO4 quads. Only the water plate authors 3. The spawner already reads the imported meshes for the MSN bit but ignores the clamp.
- **Impact**: Filtering bleeds the opposite edge in at quad borders, 2^mip texels wide. That gives seam lines in both colour and normals.
- **Related**: #4553, #2571, #4632, EXT-D6-01
- **Suggested Fix**:
  - Carry the clamp into the `.btr` `Material` the way MSN is carried.
  - Resolve with clamp; this needs a linear-with-clamp variant for the normal map.
  - Extend #4553's source pins to `terrain_lod_btr.rs` and `terrain_lod.rs`.

### EXT-D6-2026-09-27-03: Skyrim distant trees are never drawn — 386 `.btt` tree-LOD files and the `treelod` atlases go unconsumed
- **Severity**: MEDIUM (coverage hole)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: n/a
- **Game Affected**: Skyrim (LE/SE)
- **Location**:
  - `byroredux/src/cell_loader/object_lod.rs` (`.bto` only).
  - `docs/engine/exal-trees.md:83-87,307-315`.
  - `docs/engine/exal.md:156`.
- **Status**: NEW
- **Description**:
  - `Skyrim - Meshes1.bsa` ships 386 `.btt` (Tamriel 329, dlc2solstheimworld 24, …), 9 `.lst`, and the `textures\terrain\<ws>\trees\<ws>treelod.dds` atlases.
  - Vanilla Skyrim `.bto` files carry no trees.
  - No code or probe references `.btt`, `.lst` or `treelod`.
  - FO4 and FO76 bake their trees into `.bto`, so the gap is Skyrim-only.
  - exal-trees.md §7 claims `.bto`/`.btr` cover the distant tier, which is false for Skyrim.
- **Impact**: Skyrim forests stop at the full-detail radius, while the mountains behind them keep drawing.
- **Related**: #3307
- **Suggested Fix**:
  - Register the family: a scheme-table entry plus a probe counter.
  - Correct both docs.
  - Build an instanced-billboard consumer from `.btt` + `.lst` on the object ring's quad residency.

### LOW findings

#### EXT-D1-2026-09-27-03: `translate_weather` emits an identity `image_space` that the orchestration caller patches; the IMGS boundary is undocumented
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: single-boundary, no-leak
- **Game Affected**: FO3, FNV, Skyrim, FO4
- **Location**: `byroredux/src/env_translate.rs:1493-1495`; `byroredux/src/scene/world_setup.rs:366-374`
- **Status**: NEW
- **Description**: Identity is also the legitimate "no IMGS" value, so a second caller would silently render ungraded exteriors. `exal.md` and `skyal.md` never mention `exterior_image_spaces`, IMSP or INAM.
- **Suggested Fix**: Pass the IMGS inputs into `translate_weather`, and list `exterior_image_spaces` in `exal.md` §3.

#### EXT-D1-2026-09-27-04: env.health FAILs `is_interior/is_exterior` on every interior entered after an exterior
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `byroredux/src/commands/env_health.rs:349-358`; `byroredux/src/commands/env_health_tests.rs:175-181`
- **Status**: NEW (check dates from `f90e4eec1`)
- **Description**:
  - `SkyParamsRes` survives into interiors by design (#1199), and every producer sets `is_exterior: true`.
  - The "consistent interior pair" fixture uses a sky that production never creates.
- **Impact**: False `env: FAIL` on interiors. It must land with EXT-D1-02's fix.
- **Suggested Fix**: Fire only when `!lit.is_interior && !sky.is_exterior`, and replace the fixture.

#### EXT-D1-2026-09-27-05: env.health gates neither #4839's `sunlight_color` nor #4416's `image_space`
- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: no-leak (gate coverage)
- **Game Affected**: all
- **Location**: `byroredux/src/commands/env_health.rs:286-338`
- **Status**: NEW
- **Description**: Both are new canonical floats that reach shaders unchanged. The #4731 completeness pin covers `WaterMaterial` only.
- **Related**: #4483, #4731, #4840
- **Suggested Fix**: Check the `WeatherSkyState` colours and scalars and the 4 image-space slots, and reuse #4731's serde-leaf completeness test.

#### EXT-D1-2026-09-27-06: The spawner guard's stripper treats an out-of-line `#[cfg(test)] mod x;` as a block
- **Severity**: LOW (latent guard hole)
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `byroredux/src/material_translate.rs:2606-2618` (`strip_inline_test_modules`)
- **Status**: NEW
- **Description**:
  - The pattern `"\n#[cfg(test)]\nmod "` also matches `mod foo_tests;` and strips up to the next column-0 `}`.
  - There are 34 such declarations in the spawner roots. Each swallows 1–43 following lines, all test code today.
  - A production spawner placed after one would drop out of the scan. This is the #4302/#4856 failure mode.
- **Suggested Fix**: Strip only `mod <ident> {` blocks.

#### EXT-D2-2026-09-27-05: Terrain doc rot
- **Severity**: LOW
- **Dimension**: Terrain, splatting
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `crates/renderer/shaders/include/terrain_sample.glsl:19-22`; `byroredux/src/cell_loader/terrain.rs:57-58,88-92`
- **Status**: NEW (sibling of #4337)
- **Description**:
  - The shader comment still says `GpuTerrainTile` is "24 texture indices and nothing else"; it has been 160 B, carrying affinity rows and more, since #4057.
  - `CellSplatLayers` misstates its order: base transitions come first.
  - The cap of 8 comes from the 2×RGBA8 packer, not UESP.
- **Suggested Fix**: Rewrite the three comments.

#### EXT-D3-2026-09-27-03: The model candidate grid restarts every 512-unit chunk with `ceil()` — density stripes along chunk borders at 80-unit spacing
- **Severity**: LOW (visual)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: n/a
- **Game Affected**: Oblivion, FO3, FNV (Skyrim/FO4 marginally)
- **Location**: `crates/renderer/shaders/groundcover_models.comp:266-294`
- **Status**: NEW
- **Description**:
  - `perSide = ceil(512/80) = 7`, which gives 49 candidates against 40.96 implied (+19.6%).
  - Column 6's centre, 520, reflects to 504. Gaps across a border run 80, 80, 64, 48, 80.
  - The result is a band of about 1.4× density every 512 units.
  - `model_slab_holds_every_vanilla_candidate_grid` checks capacity only.
- **Suggested Fix**: Anchor candidates to a world-space lattice, and pin continuity across a chunk border.

#### EXT-D3-2026-09-27-04: Model-tier overflow drops whole late records and splits multi-shape plants silently; the 128-record cap also truncates silently
- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: n/a
- **Game Affected**: all (only past the caps)
- **Location**: `groundcover_models.comp:361-377`; `byroredux/src/render/groundcover.rs:847-851`; `crates/renderer/src/vulkan/groundcover_models.rs:495-498`
- **Status**: NEW
- **Description**:
  - Past 32,768 instances, the layout phase grants shapes in FormID order, so the last DLC and mod records lose every instance.
  - A multi-shape plant can keep one shape and lose another.
  - No warning or telemetry outside `--bench-*`. The blade tier's equivalent was fixed under #4338.
- **Suggested Fix**: Grant instances per plant or proportionally. Log once when a cap is hit, and add both counts to `DebugStats`.

#### EXT-D3-2026-09-27-05: The authored-model tier is rigid — no `WindField` or interaction-field consumer, and §12.12 does not say so
- **Severity**: LOW (feature gap)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: n/a
- **Game Affected**: all with authored cover
- **Location**: `groundcover_models.comp:398-472`; `docs/engine/exal-groundcover.md:1601-1633`
- **Status**: NEW
- **Description**: Authored clumps are static next to swaying blades and SpeedTree crowns. Motion vectors are consistent with that.
- **Related**: #4729
- **Suggested Fix**: Record the decision in §12.12. If sway is wanted, apply the §8 bend per instance, using previous-frame wind for motion vectors.

#### EXT-D3-2026-09-27-06: Model-tier host path re-derives records and the selection table and allocates about 5 fresh Vecs per frame
- **Severity**: LOW
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: n/a
- **Game Affected**: all exteriors with authored cover
- **Location**:
  - `byroredux/src/render/groundcover.rs:877,904`.
  - `byroredux/src/app_frame.rs:788`.
  - `crates/renderer/src/vulkan/groundcover_models.rs:502,507,692`.
- **Status**: NEW (the class #4607/#4609/#4798 removed from the blade path)
- **Suggested Fix**: Gate on an `AuthoredCover` generation, and keep the Vecs as persistent scratch.

#### EXT-D3-2026-09-27-07: Guard and contract gaps around the model tier and #4729 (bundle)
- **Severity**: LOW (test gap / doc rot)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: see each item
- **Status**: NEW
- **Description**:
  1. `groundcover_seed_hashes_are_integer_and_frame_invariant` (`groundcover.rs:2299-2346`) still pins three seed paths. `gcModelHash` and atomic-free placement are unpinned.
  2. `gcWaterAdmits` switches on bare `0u..5u` (`comp:215-221`) with no generated constant matching `CoverWaterRule::gpu_code`.
  3. The packing `record | (candidate << 8)` (`comp:311,404`) requires `MAX_RECORDS ≤ 256` with no assert.
  4. The third assert of `model_emission_preserves_flat_shading_and_render_layer_flags` (`groundcover_models.rs:948-952`) is tautological.
  5. The #4729 grass pins are source-text only, and the SpeedTree gust-travel term (`billboard.rs:233-234`) is unpinned.
  6. The #4338 doc comment sits on the wrong test (`groundcover.rs:2107-2110`).
- **Suggested Fix**: As listed per item in `/tmp/audit/exterior/dim_3.md`, summarized:
  - extend the seed guard;
  - generate `GROUNDCOVER_WATER_RULE_*`;
  - add a const assert on the packing bounds;
  - drop the tautology;
  - pin the crest travel behaviourally;
  - move the doc comment back to its test.

#### EXT-D3-2026-09-27-08: An alpha-blend-only GRAS shape would render as an opaque quad
- **Severity**: LOW (latent; not measured on data)
- **Dimension**: Ground-cover pipeline
- **Tier Violated**: no-render-time-fallback
- **Game Affected**: any GRAS model with blend on and alpha test off
- **Location**: `crates/renderer/src/vulkan/groundcover_models.rs:886-904,566-572`; `crates/renderer/shaders/triangle.frag:354`
- **Status**: NEW
- **Description**:
  - The tier drops `ALPHA_BLEND` and never sets `DIFFUSE_ALPHA`, so `triangle.frag` pins alpha to 1.0.
  - #4413 verified Skyrim 27/27 and FNV 24/24 models; Oblivion, FO3 and FO4 were not checked.
- **Suggested Fix**:
  - Survey GRAS model alpha across the five games.
  - Warn on blend-only shapes at template spawn, and convert them to alpha test at a documented threshold or skip them.

#### EXT-D4-2026-09-27-03: `88c23887b` hand-copied the SkyDome packing into `build_sky_cube_params`' interior arm, with no equality test
- **Severity**: LOW
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: all interiors
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1033-1150` vs `:883-1023`; `crates/renderer/shaders/sky_cube.comp:36-39` (comment now false)
- **Status**: NEW
- **Description**:
  - It is a ~105-line second packer. The two packers match field for field today.
  - `sky_cube_params_mirrors_the_sky_dome_struct` checks layout only.
  - This is the drift pattern of the #4733 promotion copy.
- **Suggested Fix**: Extract one `pack_sky_dome(...)`, or add a packer-equality test with `sky_lower.w` masked. Fix the shader comment.

#### EXT-D4-2026-09-27-04: The WTHR cross-fade does not blend the sun arc — the sun snaps on completion when the climates' TNAM differ
- **Severity**: LOW
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: worldspace-boundary cross-fades between differing climates
- **Location**: `byroredux/src/systems/weather.rs:1019`; promotion at `:1290`, `:1312`
- **Status**: NEW (#1018 fixed this class for fog only)
- **Description**: The arc uses the source's `tod_hours` for the whole 8 s fade, then jumps. It was not confirmed on real data that a door crosses between two such climates.
- **Suggested Fix**: Blend both arcs by `transition_t`.

#### EXT-D4-2026-09-27-05: skyal.md does not document the interior outdoor-sky lane
- **Severity**: LOW (doc)
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `docs/engine/skyal.md` (the only interior mention is line 413)
- **Status**: NEW. It is distinct from #4875 (volumetrics doc) and #4861 (guard).
- **Description**:
  - The per-frame interior cube/SH bake is unspecified, as are the `sky_lower.w` 1/2 modes, the `jitter.w` gating rule, #4839's portal-cloud lighting and `portal_sun`.
  - The #2226/#3323 isolation rule has been superseded in practice, and nothing specifies its replacement. EXT-D4-01 follows from this gap.
- **Suggested Fix**: Add a SKYAL section with a consumer/gate table.

#### EXT-D4-2026-09-27-06: A duplicated 4-slot TOD fold (`cloud_tod_slot` vs `fold_to_four_tod_slots`) and a stale lock-order comment
- **Severity**: LOW
- **Dimension**: Sky, weather, sun
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/systems/weather.rs:473-484` vs `:704-711`; the comment is at `:1211-1213` (it says `wd` is live, but it was dropped at `:1099`)
- **Status**: NEW
- **Suggested Fix**: Call `fold_to_four_tod_slots` from the cloud path, and fix the comment.

#### EXT-D5-2026-09-27-04: The Rapids third normal layer uses `WaterFlow.speed` (BU/s) directly as a UV/s scroll
- **Severity**: LOW (latent in vanilla)
- **Dimension**: WATAL contract × renderer Dim 8
- **Tier Violated**: no-leak
- **Game Affected**: modded rapids
- **Location**: `crates/renderer/shaders/water.frag:808-810`; `byroredux/src/render/water.rs:198-200,259`
- **Status**: NEW (present since M38)
- **Description**: The layer runs 44× too fast (8 BU/s gives 16 UV/s where the translate gives 0.365) and ignores `scroll_c`.
- **Suggested Fix**: Use `normalScrollC` on this arm.

#### EXT-D5-2026-09-27-05: The BGSM flow-map UV offset kept the pre-#4728 sign and grows without bound with uptime
- **Severity**: LOW (not measured on data)
- **Dimension**: WATAL contract × renderer Dim 8
- **Tier Violated**: no-leak
- **Game Affected**: FO4/Starfield unplaced mesh water with a flow texture
- **Location**: `crates/renderer/shaders/water.frag:697-701,790`
- **Status**: NEW
- **Suggested Fix**: Subtract the offset, switch to a dual-phase flow-map blend, and extend the #4728 source guard.

#### EXT-D5-2026-09-27-06: #4734's regression guard was deleted by #4727's commit; the dead-default check is a game-agnostic exact-90° test
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication (latent)
- **Game Affected**: FO3/FNV (guarded case); Skyrim/FO4/Starfield (check scope)
- **Location**: `byroredux/src/env_translate.rs:903-911`; `crates/plugin/src/esm/records/misc/water.rs:315-324`
- **Status**: Regression of #4734 (the guard)
- **Description**:
  - `dead_default_wind_direction_yields_no_physics_flow_for_named_creeks` was added by `5f73dae24` and removed by `a6a210eb9`. `git log -S` shows only those two commits.
  - `wind_direction_is_dead_default` now has no test.
  - The equality check applies to every game; FO4 authors exactly 90.0 on two layers, with no vanilla misfire today.
  - A missing or short DNAM's 0.0 default is treated as authored, which is the #3185 class.
- **Suggested Fix**:
  - Restore the test with post-#4727 expectations.
  - Replace the equality check with a parse-side "authored" flag on the FO3/FNV/Oblivion arm.

#### EXT-D5-2026-09-27-07: Frame and contract doc rot after #4727/#4735
- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**:
  - `byroredux/src/env_translate.rs:539-541,863,2866-2868`.
  - `docs/engine/watal.md:433`.
  - `crates/core/src/ecs/components/water.rs:272`.
  - `crates/renderer/shaders/water.frag:75`.
  - `crates/plugin/src/esm/records/misc/water.rs:289-292`.
- **Status**: NEW
- **Description**:
  1. "Compass-style bearing" mis-describes φ = β + 90°, which is a wind-FROM bearing.
  2. A test comment says a +Z current "runs north"; +Z is game south.
  3. The scroll doc says "layer-3" and omits the #4734 no-flow arm.
  4. The `water.frag` comment still gives scroll units as world units/s; they are UV/s.
  5. The `noise_wind_directions` doc states no frame, although the field now holds two.
- **Suggested Fix**: Correct all five, and state the frame once on the field and per game in watal.md §2.

#### EXT-D6-2026-09-27-04: `probe_lod_corpus` still reports false zeros — `TOTAL` ignores the Creation family; Starfield's LOD corpus and Skyrim `.btt` are unmatched
- **Severity**: LOW (tooling)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim, FO4, FO76, Starfield
- **Location**: `crates/bsa/examples/probe_lod_corpus.rs:114,180,182`
- **Status**: NEW (incomplete fix of #4737)
- **Description**:
  - Every run ends `TOTAL … 0` right after counting 10,662 (Skyrim), 8,271 + 802 (FO4) or 3,056 (FO76).
  - Starfield's `LODMeshes.ba2` holds 19,535 NIFs under `meshes\lod\generated\` plus 864 `lodsettings\*.lod`, and the probe prints 0.
  - The per-worldspace and per-level `.btr`/`.bto` counts themselves are correct.
- **Suggested Fix**:
  - Use `total += lod + creation`.
  - Add counters for `.btt`, `meshes\lod\generated\` and `lodsettings\*.lod`.

#### EXT-D6-2026-09-27-05: #4736 left two census citations stale
- **Severity**: LOW (doc rot)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: no-fabrication
- **Game Affected**: FO76
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:182-183`; `byroredux/src/cell_loader/object_lod.rs:1177`
- **Status**: NEW (incomplete part of #4736)
- **Description**: The comment's first line still lists the retired level set, and the correction is only appended. The test message is byte-identical to the baseline's and still cites #4488.
- **Suggested Fix**: Say "L4/8/16/32 across GeneratedMeshes01/02", and point the test message at #4736.

#### EXT-D6-2026-09-27-06: exal.md still states the #3321-falsified premise
- **Severity**: LOW (doc rot)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: no-fabrication
- **Game Affected**: FO3/FNV (doc)
- **Location**: `docs/engine/exal.md:259`, `:156-166`, `:801-803`
- **Status**: NEW
- **Description**:
  - §4's table says FO3/FNV have "neither scheme", which §5.2 itself names as the wrong pre-#3321 guess.
  - §2 calls legacy LOD textures unconsumed, contrary to `b50a8e6a9`.
- **Suggested Fix**: Change the §4 row to the `FalloutLegacyBlocks` scheme, retitle §2, and qualify `:801`.

#### EXT-D6-2026-09-27-07: Object-LOD mesh uploads carry no provenance — `.bto` and FO3/FNV `blocks\` meshes census as `Other`
- **Severity**: LOW (telemetry)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: n/a
- **Game Affected**: Skyrim, FO4, FO76, FO3, FNV
- **Location**: `byroredux/src/cell_loader/object_lod.rs:457-466`
- **Status**: NEW (incomplete part of `ff1b48d7c`)
- **Description**: `terrain_lod`, `terrain_lod_btr` and `placement_lod` tag `MeshUploadSource::Lod`; `object_lod`, the largest LOD family, does not.
- **Suggested Fix**:
  - Call `note_mesh_provenance(handle, MeshUploadSource::Lod, false, Some(path))`.
  - Add a source-shape test covering every `cell_loader/*lod*.rs` upload.

#### EXT-D7-2026-09-27-01: #4730's contract guard cannot detect the *.sh.sh* regression it was written for
- **Severity**: LOW (guard quality; the workflow fix itself is correct)
- **Dimension**: Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `scripts/check-playable-smoke-contracts.sh` (the `LITERAL_GATES` scan added by `bb5c2c706`)
- **Status**: NEW (incomplete fix of #4730)
- **Description**:
  - The scan's regex `run_(declared_)?gate [a-zA-Z0-9-]+` stops at `.`.
  - `run_gate m-exteriors.sh "$ext_game" static`, the exact line #4730 removed, therefore extracts `m-exteriors`. `docs/smoke-tests/m-exteriors.sh` exists, so the check passes.
- **Evidence**: Replayed in the main context: piping the pre-fix line through the scan's own `sed | grep -oE | sed` chain prints `m-exteriors`.
- **Impact**: The regression #4730 guards against would pass the contract lane again.
- **Suggested Fix**:
  - Capture the full token (`[^[:space:]"]+`), or assert that no gate token ends in `.sh`.
  - Add a negative fixture line to the check's own self-test.

#### EXT-D7-2026-09-27-02: `63c0aee3b` renamed the Skyrim SE data variable in every harness and fixture, but `playable-smoke.yml` still exports `BYROREDUX_SKYRIM_DATA`
- **Severity**: LOW. The self-hosted runner falls back to the default path, which works only while the runner's install matches it.
- **Dimension**: Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: Skyrim SE
- **Location**:
  - `.github/workflows/playable-smoke.yml:45` (`BYROREDUX_SKYRIM_DATA: ${{ vars.BYROREDUX_SKYRIM_DATA }}`).
  - The readers: `docs/smoke-tests/m-exteriors.sh:57`, `scripts/renderer-eval-groundcover.sh:12`, `docs/smoke-tests/fixtures/skyrim_se.env:12` (all `BYROREDUX_SKYRIMSE_DATA`).
- **Status**: NEW. It is a gap in today's rename, which reads as work on open #4760. `real-data-gates.yml:62` already uses the new name.
- **Description**:
  - The configured repo variable no longer reaches any Skyrim harness. The m-exteriors Skyrim profile, the ground-cover eval's Skyrim poses and the Skyrim p0/p1/p2 gates silently use the hard-coded default.
  - If the runner's install differs, the gate exits 77, which the workflow reports as "data unavailable" rather than as a misconfiguration.
- **Related**: #4760, #3741 (*skyrim_env_var_divergence*)
- **Suggested Fix**:
  - Export `BYROREDUX_SKYRIMSE_DATA: ${{ vars.BYROREDUX_SKYRIMSE_DATA || vars.BYROREDUX_SKYRIM_DATA }}`.
  - Add a contract-check line asserting that the workflow exports each `FIXTURE_DATA_ENV` name.

## Findings count

**41 findings: 0 CRITICAL · 3 HIGH · 13 MEDIUM · 25 LOW.**

| Dimension | HIGH | MEDIUM | LOW |
|---|---|---|---|
| D1 | — | 2 | 4 |
| D2 | 1 | 3 | 1 |
| D3 | — | 2 | 6 |
| D4 | — | 2 | 4 |
| D5 | 1 | 2 | 4 |
| D6 | 1 | 2 | 4 |
| D7 | — | — | 2 |

**Dedup sources:**
- `/tmp/audit/issues.json` (176 open) and `issues_all.json` (the last 1,000 issues);
- targeted `gh issue view` on every baseline and known-open number;
- every `docs/audits/*2026-09-2*.md`.

**Merged across dimensions:**
- The Dim 4 agent independently confirmed EXT-D1-01; it is filed once.
- EXT-D2-02 and EXT-D3-02 share a root cause (no base lane) but are filed separately: density versus colour consumer.
- EXT-D2-03 and EXT-D3-01 are sequenced: fix the merge first.

**Fix-ordering notes:**
- EXT-D1-02 must land with EXT-D1-04.
- EXT-D5-01 and EXT-D5-02 should land together, at the parse boundary.
- EXT-D6-01's UV fix must come with the explicit tangent, or the relief inverts.

## Already covered — cited, not re-filed

| Item | Note from this pass |
|---|---|
| **#4866** (OPEN; model tier outside the GPU timer) | **Already fixed** by `88c23887b`, pinned by `model_timer_encloses_all_phases_and_stats_copy`. It can be closed. |
| #4837 (Starfield oceanness ÷20) | OPEN. The #4285 RGB lanes are correct. |
| #4861 (no interior-isolation guard) | OPEN. Gating holds by shader today (Dim 4). EXT-D4-01 and EXT-D4-05 are adjacent. |
| #4797 (`surface_y_at` linear scan) | OPEN. No correctness defect in `17c01a4e5`'s `WaterSurfaceMesh`. |
| #4872 (model-tier ledger drift) | OPEN. Not re-reported. |
| #4840 (`aces()` fed negatives) | OPEN. EXT-D1-05 would gate the upstream input. |
| #4875, #4806, #4793, #4785 (volumetrics docs and cost) | OPEN. EXT-D4-02 is the correctness side of the sun term. |
| #4760 (Skyrim env var) | OPEN. `63c0aee3b` appears to address it; EXT-D7-02 is its workflow gap. |
| REN-D5-2026-09-26-21 (`sync.rs` rider list) | In the renderer report, unfiled. Not re-reported. |
| Interior beam volumes in `fog.rs:853-1465` (uncited extinction/albedo constants) | Interior content, outside EXAL. Routed to `/audit-renderer` (volumetrics) and `/audit-nifal`. |

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #4056 (Phase 3 LOD chain) | **CLOSED since baseline** (`c14f5361a`). Its coupling defect is EXT-D3-02. |
| #4413 (authored-model tier) | **CLOSED since baseline** (`aabd99a05`). Mechanics are sound; it produced EXT-D3-01, -03, -04, -05, -06, -07 and -08. |
| #4642 (GNAM array) | **CLOSED.** The decode is correct, but the data is dropped at merge (EXT-D2-03). |
| #4304 (ground-cover canonical `Material`) | **CLOSED since baseline.** |
| #4314 (cloud-shape constants) | **CLOSED.** The citations are in skyal.md §2.3. Only the height-gradient breakpoints carry an in-shader marker; the commit message said all were "marked in clouds.glsl". |
| #4552 / #4553 (`.btr` parallax / LOD clamp) | **CLOSED.** #4553's clamp did not reach `.btr` (EXT-D6-02). |
| #4632 / NIFAL-D1-21b-01 (`.btr` MSN) | **CLOSED and verified.** The texel basis was measured independently on 3 Skyrim quads. The baseline's open legacy-LOD basis question is answered: the basis matches, but the sampling location is wrong (EXT-D6-01). |
| #4468 (FO3 `.high.` variant) | **CLOSED.** Documented as intentional; the sibling check was re-run with 0 missing. |
| #3142 (VWD per-entity lock) | **CLOSED** with a live source-shape plus behavioural pin. |
| #3307 (VWD full-model culling) | OPEN, unchanged. |
| #4122 (SPT tail desync) | **CLOSED since baseline.** Skyrim distant trees are a separate gap (EXT-D6-03). |
| #4264 / #4285 | **CLOSED.** |
| skyal §2.3–§3 documented-open items | OPEN, still documented; not re-filed. |
| watal.md register | The #4544 paragraph was corrected by #4727. The layer-frame scope per game is now undocumented (EXT-D5-02, EXT-D5-07). |
| Exterior harnesses in automation | All three wired arms now resolve to real scripts (#4730), and each preflights the shader-artifact check (#4738). The Skyrim data variable no longer reaches them (EXT-D7-02). |
| `renderer-eval-groundcover.sh` washout (*groundcover_reference_capture_washout*) | Not re-measured (no engine launch). |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 3 `Paths:`** omits `crates/renderer/src/vulkan/groundcover_models.rs` (#4413), `groundcover_stats.rs` and `crates/renderer/shaders/groundcover_models.comp`.
- **Dim 3 seed note** (the stale 16 MB / 4,096 comments) is resolved. `groundcover.rs:27` and `shader_constants_data.rs:355-361` now say 64 MiB.
- **Dim 4 unpinned-dimmer note** is resolved. Dimmer-0.5 tests now exist (`weather.rs:2268`, `:2921`).
- **Dim 4's interior rule** ("interiors carry the exterior sky on `exterior_sky_tint` only") is superseded by `0572bfd5a`'s interior cube and SH bake and the aperture modes. See EXT-D4-05.
- **Dim 7 `Paths:`** omits the new `docs/smoke-tests/interior-godrays.sh`.
- **Dim 1 first-step grep** now expects a 4th caller (`render/sky.rs`) until EXT-D1-02 is fixed.

## Cross-audit routing

- **`/audit-renderer`**:
  - Dim 8: water shader halves of EXT-D5-04 and EXT-D5-05; volumetric consumer of EXT-D4-02.
  - Dim 10: interior sky consumers, EXT-D4-01 and EXT-D4-03.
- **`/audit-esm`**:
  - EXT-D5-01 / EXT-D5-02 may land at the WATR decode in `crates/plugin/src/esm/records/misc/water.rs`.
  - EXT-D2-03 is in `EsmCellIndex::merge_from`.
  - EXT-D2-01 needs the LTEX editor ID carried through the ESM index.
- **`/audit-speedtree`**: the SpeedTree gust-travel pin (EXT-D3-07 item 5). Skyrim `.btt` (EXT-D6-03) is tree LOD, not `.spt`.
- **`/audit-physics` Dim 5**: the currents under EXT-D5-01, EXT-D5-02, EXT-D5-03 and EXT-D5-06. Physics stays `GameKind`-free.
- **`/audit-tooling`**: EXT-D7-02 alongside #4760.

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-09-27.md`.

**Labels:**
- `terrain-exterior` on all findings.
- Additional labels by finding:

  | Label | Findings |
  |---|---|
  | `water` | EXT-D5-* |
  | `shaders` | D3-02, D3-03, D4-01, D5-04, D5-05, D6-01 |
  | `doc-rot` | D2-05, D4-05, D5-07, D6-05, D6-06 |
  | `test-gap` | D1-06, D3-07, D5-06, D7-01 |
  | `performance` | D3-06 |
  | `tech-debt` | D6-04, D7-01, D7-02 |
  | `game:skyrim` | D6-03 |
  | `game:fo76` | D6-05 |

**Needs a captured frame to confirm the look:**
- EXT-D6-01: `m-exteriors.sh fnv static` and `m-exteriors.sh oblivion static`, framing distant terrain.
- EXT-D4-01: a night Show-Sky interior.
- EXT-D3-02: the `gc-backlit-*` poses, after the washout check.
