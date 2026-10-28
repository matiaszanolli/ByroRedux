**HEAD**: fa6be4a6b · **Baseline**: [AUDIT_EXTERIOR_2026-10-26.md](AUDIT_EXTERIOR_2026-10-26.md) (HEAD `3e6f6b66a`; 71 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain), Dim 3 (ground cover), Dim 6 (distant LOD / trees), Dim 7 (gates, static only) · **Unchanged since baseline (skimmed)**: Dim 4 (sky / weather / sun: hygiene-only diffs in `systems/weather.rs`), Dim 5 (water: path hits were non-water — NIFAL dead-param removal, the `LodBlock::rim_mask` field, a deleted unused wrapper)

# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-10-27

**Method.** This audit ran inside `/audit-suite --preset comprehensive` and followed its rules. It used no sub-agents and launched no engine. It built only in the repo `target/`. It created no issues and edited no source. I analysed each dimension myself, one at a time, from the diffs of every commit in its `Paths:` since `3e6f6b66a`. Per-dimension notes are in `/tmp/audit/exterior/dim_<N>.md`. I deduplicated against `/tmp/audit/open_issues_snapshot.txt` (84 open issues), a closed-issue search, yesterday's report and § 6 of the suite rules (the audits already finished this run). Every finding is a static trace; **none was confirmed on screen**.

**Test state (HEAD, repo `target/`, `TMPDIR=/mnt/data/tmp`, all green):**

| Suite | Result |
|---|---|
| `-p byroredux --bin byroredux -- env_translate climate default_weather forced fog weather exterior terrain groundcover water lod sky streaming_helpers pool_reserve` | 809 passed, 0 failed, 25 ignored |
| `-p byroredux-renderer --lib -- groundcover sky_ cloud terrain water` | 242 passed, 1 ignored (GPU) |
| `-p byroredux-terrain-height` | 11 passed |

Not run: the `--ignored` real-data and GPU tests, and every smoke gate (`m-exteriors.sh`, `w1-water-traversal.sh`, `tsd1-height-census.sh`, `m34-day-night.sh`).

## Executive Summary

**8 findings: 1 CRITICAL · 0 HIGH · 0 MEDIUM · 7 LOW.** All 8 are NEW. The CRITICAL is a regression introduced today by the fix for #6368.

- **EXT-D6-2026-10-27-01 (CRITICAL, regression from `a1327ad3a`, the #6368 fix, landed 2026-10-27 16:42).** The new shared `vertex_bound` is called in `spawn_lod_block` with `|i| holes[i]`. That call comes after `append_lod_skirts`, which pushes skirt vertices onto `vertices` but adds nothing to `holes`. Every synth LOD block that borders resident full-detail ground has skirts, so the first skirt vertex indexes past `holes` and panics. This runs on the main thread with no `catch_unwind`, inside `stream_lod_blocks`, at world-setup bootstrap and on every streaming step. The expected result is an engine panic on any exterior session with terrain LOD (Oblivion, FO3, FNV, Skyrim, FO4). The deleted `block_bound` used `zip`, which silently stopped at the hole list's length. No test drives a skirted block through the bound. **The suite's closing runtime audit should expect to hit this.**
- The seven LOWs cover the following:
  - The #6409 skirt key stops at the finest band (D6-02).
  - The legacy rows of the new Skyrim albedo-gain table can never take effect (D6-03).
  - Two more spliced doc blocks that #6373's new scan cannot see (D6-04).
  - Distance-softening constants and a wrong doc (D2-01).
  - `exal.md` silent on the 2026-10-26/27 LOD-join work (D2-02).
  - Stale grass sizes after the temperate retune (D3-01).
  - A count-only FO4 seam exemption (D7-01).

**Baseline findings (2026-10-26) since `3e6f6b66a`:**

| Baseline | Issue | State at HEAD |
|---|---|---|
| D1-01 interior forced weather overwrites the room's lighting | #6406 | Fixed, `8e20ced6b`. I traced the funnel's writes (`CellLightingRes` insert, `InteriorSkyExposureRes`, the `ImageSpaceBase` publish); all three are set aside and restored, and the in-funnel `weather_system(world, 0.0)` advances nothing at dt 0. |
| D1-02 `exterior_ready` vs the processing gate | #6407 | Fixed, `d32e2ade8`. One `needs_exterior_processing` predicate; `the_cache_and_the_processing_step_share_one_gate` is live. |
| D3-01 model scale shares the pick's stream | #6408 | Fixed, `187fd7f0e`. `GROUNDCOVER_MODEL_SCALE_STREAM` 512 with a const-assert; the distinct-streams pin is live. |
| D6-01 stale skirts at block seams / `.btr` without skirts | #6409 | Fixed for the finest band, `bf304851f`. The coarse-band residual is D6-02. |
| D6-02 #6053 overlap root cause | #6053 | Fixed, `0d5431193` (`l2 > level`, coarse-first). |
| D2-01 / D6-03 `spawn_lod_block` doc + `allow` | #6410 | **Still present.** `fde8836dd` added a reasoned `allow` on `fn spawn_lod_block` (:949). The bare `allow` and the spawn doc still sit on `const LOD_SKIRT_DROP` (:890-899). `every_clippy_allow_carries_a_reason` accepts the bare one because a doc sits above it. |
| D2-02, D2-03, D3-02, D3-03, D4-01..03, D5-01..03, D6-04 | #6411–#6421 | Open, unchanged at HEAD. |

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| single-boundary | **Holds, one soft note.** No new `translate_*` / `SkyParamsRes{` / `WeatherDataRes{` / `WaterMaterial{` site. All installs still go through `install_environment`. The parse-cache gate and the processing step now share one predicate (#6407). Note D6-03: `spawn_lod_block` re-gates the EXAL albedo-gain table on `combined`, a consumer-side game-family decision that shadows the table's legacy rows (no effect today). |
| no-fabrication | **Holds, small notes.** `terrain_lod_albedo_gain` (Skyrim 0.36) is a documented one-scene measurement; FNV and FO4 were measured at 1.0. Distance softening's onset ratio (0.5) and amount scale (0.35) are bare, and the setting's doc misstates the onset (D2-01). |
| no-leak | **Holds.** #6406 closes yesterday's leak. The interior's lighting, sun gate and base image space survive a mid-session forced-weather re-apply. |
| no-render-time-fallback | **Holds.** No `GameKind` or game token in the sky, cloud, ground-cover, water, terrain or composite GLSL. The softening branch keys on tile data (`lodSoftenEnd > 0`), not on the game. `grep -rn GameKind crates/physics/src` is empty. |

## Per-Category Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Terrain (LAND, splat, GNAM cover) | `build_cell_splat_layers`, `spawn_terrain_mesh` | OK | OK | OK | OK |
| Terrain shading (height blend / parallax / softening) | `set_terrain_lod_soften_end`, `push_terrain_lod_soften` | OK | **D2-01** (bare onset / scale) | OK | OK |
| Sky (bake, clouds, SH) | `translate_sky`, `pack_sky_dome` | OK | OK | OK | OK |
| Weather / sun | `translate_weather`, `weather_system` | OK | OK | OK (#6406 fixed) | OK |
| Climate / landscape inheritance | `resolve_exterior_climate`, `needs_exterior_processing` | OK (#6407 fixed) | OK | OK | OK |
| Water | `resolve_water_material`, `water_material_from_mesh` | OK (#6419 open) | OK (#6418 open) | OK | OK |
| Ground cover | `groundcover_translate.rs`, `resolve_authored_cover` | OK | OK (D3-01 doc only) | OK | OK |
| LOD (terrain) | `spawn_lod_block`, `btr_quad_texture`, `terrain_lod_albedo_gain` | **D6-03** (gain re-gated) | OK | OK | OK — **D6-01 crash**, **D6-02** coarse seam |
| LOD (objects / placement / trees) | `object_lod_scheme`, `placement_lod_cell_path` | OK | OK | OK | OK |

## Findings

Finding IDs are `EXT-D<dim>-2026-10-27-NN`.

### EXT-D6-2026-10-27-01: `vertex_bound(&vertices, |i| holes[i])` indexes past `holes` for every skirted synth LOD block — main-thread panic
- **Severity**: CRITICAL. It is a deterministic panic that kills the engine on the common exterior path. It is not memory-unsafe.
- **Dimension**: 6 Distant LOD and trees
- **Tier Violated**: n/a
- **Game Affected**: every game with terrain LOD: Oblivion, FO3, FNV, Skyrim, FO4. FO76 and Starfield have no terrain LOD layout.
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1105` (`append_lod_skirts`) → `:1113` (`let bound = vertex_bound(&vertices, |i| holes[i]);`); `byroredux/src/cell_loader/lod_support.rs` `vertex_bound`; `holes` built at `terrain_lod.rs:975-1000`
- **Status**: NEW. A regression introduced by `a1327ad3a` (the fix for #6368), not a regression of any earlier issue.
- **Description**: `holes` has one entry per grid sample (n·n). `append_lod_skirts` appends two vertices per skirt edge to `vertices` and nothing to `holes`. The deleted `block_bound` walked `vertices.iter().zip(holes.iter())`, so it stopped at n·n and never saw the skirt vertices. The shared `vertex_bound` walks `vertices.iter().enumerate()` and calls `skip(i)` for every index. Its terrain caller passes `|i| holes[i]`, so the first skirt vertex (i = n·n) is an out-of-bounds index.
- **Evidence**:
  ```rust
  append_lod_skirts(&mut vertices, &mut indices, &skirt_edges);   // :1105, vertices.len() = n*n + 2*E
  ctx.allocator.as_ref()?;
  let bound = vertex_bound(&vertices, |i| holes[i]);              // :1113, holes.len() = n*n
  ```
  ```rust
  // deleted by a1327ad3a:
  for (v, &hole) in vertices.iter().zip(holes.iter()) { ... }
  ```
  Skirt edges are pushed for every drawn quad whose orthogonal neighbour cell is resident full detail (`neighbour_is_full_detail`, `:1077-1098`). That covers every finest-band boundary block, and also legacy coarse synth blocks that border resident ground. `stream_lod_blocks` is called from `streaming_helpers.rs:219` on the main thread (world-setup bootstrap and per-frame streaming). No `catch_unwind` wraps it; the only ones in the binary guard worker and pre-parse paths.
- **Impact**: The engine is expected to panic as soon as the terrain-LOD ring spawns its first boundary block. That happens at `--grid` boot, or on the first door or load into an exterior, for all five LAND games. The unit tests stay green: `vertex_bound_ignores_skipped_vertices` passes a mask exactly as long as its vertex list, and nothing runs the bound over a skirted block (`spawn_lod_block` needs a `VulkanContext`).
- **Related**: #6368 (closed by `a1327ad3a`), #6409 / `7eac91637` (skirts), the suite's runtime audit.
- **Suggested Fix**: `vertex_bound(&vertices, |i| holes.get(i).copied().unwrap_or(false))`. The skirt vertices then count toward the sphere, which is harmless: they hang 640 units under real edges. Alternatively, compute the bound before `append_lod_skirts`. Add a pure test that builds a skirted block's vertex and hole lists, for example by extracting the mesh build out of `spawn_lod_block`, and calls the bound.

### EXT-D6-2026-10-27-02: #6409's rim key covers only the finest band, so a coarse quad can border a hysteresis-resident cell with no skirt or a stale one
- **Severity**: LOW (visual, intermittent, specific lattice alignments)
- **Dimension**: 6
- **Tier Violated**: n/a
- **Game Affected**: Skyrim and FO4 (coarse `.btr`, no skirt code); FO3, FNV and Oblivion (coarse synth blocks built with skirts at spawn but keyed `(0, 0)`)
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:673-680` (`(mask, rim) = if level == k { .. } else { (0, 0) }`); `byroredux/src/cell_loader/lod_bands.rs` refine skip (`quad_min_chebyshev(..) <= refine_within`); `byroredux/src/streaming_helpers.rs:197` (`full_detail_radius: state.radius_unload`); test `coarse_btr_bands_stay_out_of_the_default_streaming_radius` (`terrain_lod.rs:1807`)
- **Status**: NEW. This is the coarse-band residual of closed #6409.
- **Description**: Hysteresis keeps cells resident out to Chebyshev `radius_unload`. #6094 admits a coarse quad once its nearest cell is at `radius_unload + 1`. So when the player steps back from a lattice-aligned coarse edge, the cell at `radius_unload` stays resident directly against the coarse quad. On the Combined games that quad is a prebaked `.btr` with no skirts. On the legacy games it is a synth block whose skirts were decided at spawn but whose rebuild key is `(0, 0)`, so they never follow residency. The #6094 test asserts only `> radius_unload`, which allows this adjacency.
- **Impact**: The sky-coloured ribbon #6409 fixed can reappear along a coarse-quad edge in the hysteresis band.
- **Suggested Fix**: Refine coarse quads within `radius_unload + 1`. That one-cell margin guarantees a finest-band block between any coarse quad and resident ground. Then extend the #6094 test to assert the margin.

### EXT-D6-2026-10-27-03: `terrain_lod_albedo_gain`'s legacy rows are unreachable, because the synth consumer re-gates on `combined`
- **Severity**: LOW (latent; every legacy row is 1.0 today)
- **Dimension**: 6 (consumer of the Dim 1 table)
- **Tier Violated**: single-boundary (a consumer-side game-family gate shadows the EXAL table)
- **Game Affected**: Oblivion, FO3, FNV
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1323` (`if translated_lod.is_some() && combined { material.diffuse_color = [terrain_lod_albedo_gain(game); 3]; }`); table `byroredux/src/env_translate/lod.rs:72-77`
- **Status**: NEW
- **Description**: Every legacy LOD block is a synth block wearing its authored baked quad (`translated_lod = translate_terrain_lod_textures(..)`). That is exactly the case the comment says takes the gain ("a block wearing its quad's baked diffuse"). The table documents FNV as "1.0, measured" and Oblivion as "unmeasured, keep until measured". But because of the `&& combined` term, a future measured value for those games would never reach a block. The tiled-LTEX fallback is already excluded by `translated_lod.is_some()`.
- **Suggested Fix**: Drop `&& combined`, or state in the table's doc that it applies to the Combined family only.

### EXT-D6-2026-10-27-04: Two more doc blocks spliced onto the wrong item since `3e6f6b66a`, missed by #6373's new scan
- **Severity**: LOW (doc-rot)
- **Dimension**: 6
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Location**: `byroredux/src/streaming_helpers.rs:559-604`; `byroredux/src/cell_loader/terrain_lod.rs:1979-1987`
- **Status**: NEW. Same class as closed #6373.
- **Description**:
  1. `ab9d46d18` / `22d6e1a80` inserted `GROUND_VERTICES_PER_CELL` .. `exterior_pool_reserve` between the two halves of `drain_lod_reclaim_targets`'s doc. "Drain every rootless distant-LOD owner ... (#1536)." now documents `const GROUND_VERTICES_PER_CELL`, and the function keeps only its "Mirrors the `collect_victim_gpu_handles` ..." half.
  2. `398a6b525` inserted `synth_blocks_decode_vnml_with_the_terrain_decoder` under the "#6409 — the streaming step keys both the rebuild and the attempted-miss cache on the pair ..." doc. That doc now heads the VNML test, and `the_ring_rebuilds_and_gates_on_the_hole_and_rim_pair` (`:2004`) has none.
- **Evidence**: `source_scan::spliced_doc_blocks` (`546f5e2d4`) trips only on a doc that leads with a sibling's name, and its commit says so. Neither splice opens with one.
- **Related**: #6373 (closed), #6410 (open; its site is still present, see the baseline table).
- **Suggested Fix**: Re-attach both doc halves to their items.

### EXT-D2-2026-10-27-01: Distance softening's shape is two bare literals, and the setting's doc states the wrong onset
- **Severity**: LOW
- **Dimension**: 2
- **Tier Violated**: no-fabrication (engine-choice constants unnamed; one documented incorrectly)
- **Game Affected**: all LAND games
- **Location**: `byroredux/src/app_frame.rs:86-90` (`ring_edge * (1.0 - 0.35 * amount)`); `crates/renderer/shaders/triangle.frag:399-406` (`smoothstep(0.5 * lodSoftenEnd, lodSoftenEnd, …)`); `crates/core/src/settings/builtin.rs:53-55`
- **Status**: NEW (`e71451d08`)
- **Description**: The setting's doc says "1 = soften from about a fifth of the ring out". At amount 1 the full-softening distance is 0.65 × the ring edge, and the onset (half of it) is 0.325 × the ring edge, so about a third, not a fifth. `TERRAIN_LOD_SOFTEN_MIPS` is a named generated constant. The 0.5 onset ratio in GLSL and the 0.35 host scale are not named. No test covers `push_terrain_lod_soften`'s mapping: `lod_soften` is pinned only by the offset-220 layout assert. `exal.md` does not record the feature (D2-02).
- **Suggested Fix**: Name both values (the onset fraction in `shader_constants_data.rs`, a host constant for 0.35), correct the doc, and add one pure-function test of the setting → distance mapping.

### EXT-D2-2026-10-27-02: `exal.md` records none of the 2026-10-26/27 full-detail ↔ LOD join work
- **Severity**: LOW (doc-rot)
- **Dimension**: 2 / 6 (spec doc)
- **Tier Violated**: n/a
- **Game Affected**: all LAND games
- **Location**: `docs/engine/exal.md`
- **Status**: NEW. Same class as open #6416 (`skyal.md` silent on the drift cap).
- **Description**: `exal.md` has no entry for any of the following:
  - `terrain_lod_albedo_gain`, a per-game EXAL table whose measured values (Skyrim 0.36 from one Whiterun scene, FNV and FO4 1.0) live only in rustdoc.
  - The synth boundary block wearing its quad's baked `.btr` diffuse and normal (`74e1b0788`, `btr_quad_texture`).
  - The skirts and rim mask (#6409).
  - Distance softening (`e71451d08`).
  `git log --since=2026-10-25 -- docs/engine/exal.md` lists only `d32e2ade8`. The only prose mention of softening is `shader-pipeline.md` (lane 220) and `debug-cli.md`.
- **Suggested Fix**: Add one `exal.md` § "terrain-LOD join" paragraph citing the four, with the gain's measurement caveat.

### EXT-D3-2026-10-27-01: The temperate retune (`c9433d501`) left the old 9–21 BU envelope in the rustdoc and in `exal-groundcover.md`
- **Severity**: LOW (doc-rot)
- **Dimension**: 3
- **Tier Violated**: n/a
- **Game Affected**: all (built-in temperate species)
- **Location**: `crates/core/src/ecs/components/groundcover.rs:232-246`; `docs/engine/exal-groundcover.md:1056-1058`, `:1900`, `:2003-2008`
- **Status**: NEW
- **Description**:
  - The `DEFAULT_TEMPERATE` rustdoc still says "so a 9–21 unit blade is shin-height".
  - The in-body comment still opens with "The fallback stays deliberately short", after a sentence cut off mid-clause ("…for its four-ribbon tuft to"). It sits directly above the new "1.25x taller" paragraph.
  - §12.16 states the temperate draw as "15 units, CV 0.231". The shipped draw is `moment_matched_length((11, 26))`: mean 18.5, CV ≈ 0.234.
  - `:1900` presents "temperate 9-21" as current.
  - `:1056-1058` derives `GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY = 0.4` from "the ~10-unit canopy the default temperate species stands up", a premise that moved with the ~1.23x height.
- **Suggested Fix**: Update the rustdoc and the §12.16 numbers, mark `:1900` as historical, and re-state or re-derive the LAI premise. The width growth is already tracked as performance in #6248.

### EXT-D7-2026-10-27-01: The FO4 seam exemption matches the vanilla crack's counts, not its cells
- **Severity**: LOW (test-gap)
- **Dimension**: 7
- **Tier Violated**: n/a
- **Game Affected**: FO4
- **Location**: `docs/smoke-tests/m-exteriors.sh:835-845`; `crates/core/src/ecs/resources/mod.rs:1440-1452` (`TerrainSeamStats::machine_line`)
- **Status**: NEW (`6209c213e`)
- **Description**: The `fo4` profile passes on `pairs_dirty == 1 && height_mismatch_vertices == 3`. The `terrain-seams:` machine line carries no pair coordinates, so the gate cannot check that the dirty pair is (3,0)/(4,0). The script comment says "a new defect cannot hide behind the known one". That holds only when the known pair is resident in the run. The boundary scenario moves the player, so a run without (3,0)/(4,0) resident that contains one new 3-vertex crack passes as the known crack. The same happens if a mod masks or fixes the vanilla pair.
- **Suggested Fix**: Add the dirty pair list (or the first pair's cells) to the machine line and match `(3,0)/(4,0)` in the exemption.

## Dedup notes (not re-filed)

- CloudSimState ↔ SkyParamsRes lock-order cycle (`weather_system` vs `build_sky_params`, Dim 4 path): **Dup of AUDIT_ECS_2026-10-27 ECS-2026-10-27-03** (REG-02).
- Underwater god rays: REN-D8-01/02/03 this run, plus open #6415 / #6418.
- Swimmer current and wind-swell wake: PHYS-D5-2026-10-27-01/-02.
- Grass blade area cost after `c9433d501`: PERF / #6248.
- `moved_surface_buffers` leak (lavapipe red): SAFE-2026-10-27-01. It is not exterior code.
- Pool-reserve sizing (`exterior_pool_reserve`) is renderer- and performance-owned. I checked only its exterior wiring: it is set once per session in `assemble_exterior_streaming`, which all four exterior entries go through.
- Not filed: the model tier's surface id `6u + s` reuses the wind-phase stream 7 for shape 1. The id is only a temporal identity hash, so the correlation is harmless; `GROUNDCOVER_MODEL_SCALE_STREAM`'s doc ("every per-plant quantity reads its own stream") slightly overstates it.

## Known-Open Register (what this pass changed)

| Item | Owner doc / issue | This pass |
|---|---|---|
| Terrain collider LTEX Havok material | #6048 | unchanged |
| Height maps / height blend / parallax epics | #6195 / #6196 / #6197 | softening added on #6197's former pad lane (D2-01, D2-02) |
| Blade-length sourcing | #6201, #6413 | numbers shifted by the retune (D3-01) |
| VWD full-model culling | #3307 | unchanged |
| Full-detail refusal never retried | #6106 | unchanged |
| WTHR decoder duplication | #6223 | unchanged |
| FNV `lod.coverage` 384 overlaps | #6053 | **closed** (`0d5431193`), partition pin live |
| Skirt staleness across block seams | #6409 | **closed** for the finest band; coarse residual D6-02; **the bound refactor after it crashes** (D6-01) |
| `spawn_lod_block` doc / bare `allow` on `LOD_SKIRT_DROP` | #6410 | still present (`fde8836dd` added a second, reasoned `allow` on the function) |
| Interior forced weather / FO3 parse cache / model stream | #6406 / #6407 / #6408 | **closed**, verified |
| skyal.md drift cap, duplicate `#[cfg(test)]`, god-ray sun / IOR, shore literals / border direction, w1 README row | #6415–#6421 | unchanged |

Next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-10-27.md` (labels `terrain-exterior`, plus `doc-rot` / `test-gap` / `shaders` where noted). **D6-01 should be fixed before the runtime audit runs or anyone boots an exterior.**
