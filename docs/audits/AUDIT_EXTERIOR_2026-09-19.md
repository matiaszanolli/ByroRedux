# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-09-19

Seventh-dimension full pass over the outdoors domain: per-game ESM environment
data → one canonical form → terrain, LOD, ground cover, sky, weather/sun, water.
Orchestrator plus seven dimension agents, all concurrent, each writing
`/tmp/audit/exterior/dim_N.md` before this consolidation. **This is the first
run under the current EXAL-doctrine skill** — the only prior exterior report is
[AUDIT_RENDERER_2026-05-03_EXTERIOR.md](AUDIT_RENDERER_2026-05-03_EXTERIOR.md),
an old-format audit from before the boundary doctrine existed; treated as
historical context, not a baseline.

**HEAD**: `7f8d71050` · **Baseline**: 2026-05-03 (historical; 22–187 commits per
dimension since) · **Audited**: Dims 1–7 full (no dimension unchanged) ·
**Unchanged since baseline (skimmed)**: none.
**Test-state caveat**: `cargo test -p byroredux` **does not compile at HEAD**
(EXT-D1-2026-09-19-01) — every bin-crate guard cited below was verified by
source reading and, where noted, patched-worktree probe runs, not by the
project's own gate. Renderer-crate tests ran green on default cargo.

## Executive Summary

**30 findings: 0 CRITICAL · 3 HIGH · 10 MEDIUM · 17 LOW.** The boundary
doctrine itself survives a full fresh-eyes sweep of ~5 months of development
(since 2026-05-03) essentially intact — all four tier invariants hold in all
six translation categories. The two HIGHs are outside the doctrine's happy
path but inside its blast radius: the domain's verification gate is currently
**dead** (a test-only compile break), and the one place the HNAM
`sunlight_dimmer` chain crosses a state transition, **the canonical value is
dropped** (Oblivion).

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **HOLDS.** Every canonical struct has exactly one production construction site: `SkyParamsRes`/`WeatherDataRes`/`CellLightingRes` → `env_translate.rs` (`translate_sky` :1085, `translate_weather` :1288, `translate_exterior_cell_lighting` :1025, `procedural_fallback_*`, `resolve_*_climate`); water → exactly two sites (`resolve_water_material` :943 ESM, `water_material_from_mesh` material_translate.rs:218 NIF, census of all 16 literals); terrain splats → `spawn_terrain_mesh`/`build_cell_splat_layers`; ground cover per-game data → `groundcover_translate.rs` only. Bulk `--grid`, streaming and XCCM re-resolve all funnel through `apply_environment`; no per-frame climate re-decide. The one seam breach is the WTHR transition promotion (EXT-D4-02). |
| **no-fabrication** | **HOLDS with LOW dents.** Load-bearing constants verified cited against the specs (`SUN_SOUTH_TILT` → exal.md §9 Q1; coverage 0.86/0.80/0.70/0.40/0.55 → skyal.md §2.3 value-exact; `SUN_INTENSITY_PEAK` three-consumer contract; scroll calibration). Dents: the `WeatherSkyState` fallback quartet is uncited (EXT-D1-04); three stale terrain docs contradict deliberate code (EXT-D2-03); the blade-arena docs still claim the pre-4× budget (EXT-D3-02); FO76's shipped `.bto` family contradicts the scheme table's "none" (EXT-D6-02); VTXT opacity's documented [0,1] guarantee is not enforced (EXT-D2-02). |
| **no-leak** | **HOLDS.** Only documented sentinels remain (`skyrim_dalc_per_tod: None`, no default water, `fog_clip: None`, LOD water render-only). `fog_power` capture-but-unconsume verified genuine. LOD materials: all spawn sites individually routed through the material boundary (per-site check; the file-granular guard's known blind spot did not hide anything this pass). |
| **no-render-time-fallback** | **HOLDS.** Zero `game ==` in environment logic outside the sanctioned data tables (`terrain_lod_layout`, `object_lod_scheme`, `*_lod_supported`, `DefaultLandTexture::for_game`); zero game tokens in sky/cloud/groundcover/water GLSL (the single game-named token, `STARFIELD_WATER_CONCENTRATION_REFERENCE`, is an unconditional documented units constant through the lockstep header); runtime blends canonical `FogMedium` only, never rebuilds ramps; ground-cover tier crossover is resolution-invariant by construction (projected pixels). |

### The three HIGHs

1. **EXT-D1-2026-09-19-01 — the bin-crate test suite does not compile at HEAD.**
   `d574d9bd1` (2026-09-19, the FO3/FNV HUD commit that bundled the #4173 XEZN
   fix) added `CellData::encounter_zone_form` without updating six test-fixture
   initializers. E0063 ×6 on the AGENTS.md-mandated verify command; ~2,166
   tests unrunnable; production code unaffected. Independently flagged by Dims
   1, 2, 3, 4, 5 and 6 — every dimension's first step failed the same way.
   *This report's green claims rest on source reading plus patched-worktree
   probes, not on the gate.*
2. **EXT-D4-2026-09-19-02 — the HNAM dimmers are dropped at the WTHR
   transition seam (Oblivion).** The cross-fade eases toward an *undimmed*
   target sun, and `promote_weather_transition_target` copies 11 fields but not
   `sunlight_dimmer`/`grass_dimmer` — after any Oblivion weather change between
   WTHRs with differing HNAM dimmers, exterior sun brightness and ground-cover
   dimmer run on the source weather's values indefinitely, with a one-frame pop
   at fade end. Verified in main context: the only dimmer multiply
   (`weather.rs:805-807`) applies the source's value; the promotion
   field-copy list omits both.
3. **EXT-D7-2026-09-19-01 — the ground-cover eval harness re-mints washed-out
   reference frames with exit 0.** The 2026-09-15 washout protocol (luminance
   precheck, backlit framing, veil check) lives only in the audit skill's
   prose; the script's sole hard check is "PNG non-empty", and the committed
   `gc-backlit-*` baseline *is* the veil (sd ≈2.3–2.7/255). Every ground-cover
   visual claim — including #4056's pending acceptance — is judged through a
   harness that cannot distinguish its known-bad baseline from a fixed one.

## Per-Category Matrix

Boundary fn cited per category. ✓ = holds; the finding column lists exceptions.

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ one LAND site | ✗ VTXT [0,1] doc unenforced (D2-02); stale docs (D2-03) | ✓ handle-0-no-contribution, never checker | ✓ GLSL game-free; load-time-only splats | D2-01..04 |
| **Sky / clouds / bake** (`translate_sky` / `SkyCubeParams::from_composite`) | ✓ one bake, same `CompositeParams` | ✓ constants = skyal §2.3 | ✓ ready-flag gate; no second march | ✓ | D4-01 (gate) |
| **Weather / sun** (`translate_weather`) | ✗ dimmers dropped at the transition seam (D4-02) | ✗ fallback quartet uncited (D1-04) | ✓ sentinels documented | ✓ | D4-02, D4-03, D1-04 |
| **Water (WATAL)** (`resolve_water_material` / `attach_mesh_water`) | ✓ exactly two composition sites (census) | ✗ sentinel guard pins 5 of ~15 fields (D5-02); DNAM/NAM4 ungated (D5-03); MATT unregistered (D5-05) | ✓ LOD water render-only | ✓ physics `GameKind`-free (grep empty) | D5-01..05 |
| **Ground cover** (`groundcover_translate::layer_affinity` etc.) | ✓ per-game data enters only at the boundary | ✗ budget doc rot (D3-02); one cited-math arrival (D3-04) | ✓ GRAS decline intact (§12.12) | ✓ zero game tokens downstream | D3-01..04 |
| **Distant LOD / trees** (`terrain_lod_layout` / `object_lod_scheme`) | ✓ one partitioning ring, structural | ✗ FO76 `.bto` corpus vs table (D6-02); probe blind on BA2 (D6-03); TLAS doc rot (D6-04) | ✓ per-site material boundary | ✓ tables are data | D6-01..06 |

Cross-cutting: **D1-01..04** (boundary discipline + `env.health` + compile
break), **D7-01..09** (acceptance harness).

## Findings

### HIGH

#### EXT-D1-2026-09-19-01: Bin-crate test suite does not compile at HEAD — six `CellData` fixtures miss `encounter_zone_form`
- **Severity**: HIGH · **Dimension**: EXAL boundary (all-dimension gate) · **Tier Violated**: none (verification infrastructure) · **Game Affected**: all
- **Location**: `byroredux/src/cell_loader/exterior.rs:562,737,990`, `cell_loader/lod_support.rs:346`, `cell_loader/lgtm_fallback_tests.rs:48`, `scene/world_setup.rs:1436` (all `#[cfg(test)]`); root cause `crates/plugin/src/esm/cell/mod.rs:265`
- **Status**: NEW (Related: #4173). Independently flagged by Dims 1–6.
- **Description**: `d574d9bd1` (2026-09-19, "feat(hud): add FO3/FNV per-game profiles…", which bundled the #4173 XEZN parse) added `CellData::encounter_zone_form` and updated production consumers but missed six test-fixture literals. `cargo test -p byroredux` (any filter) fails with 6 × E0063 on the 1.96 toolchain — the exact command AGENTS.md/`docs/contributing.md` §Tests mandate before pushing any `byroredux/src/` change. `cargo check -p byroredux` (production) is clean, so nothing but a test run reveals it (note: piping through `tail` masks cargo's exit code).
- **Evidence**: verified at HEAD `7f8d71050` in main context: 6 × `error[E0063]: missing field 'encounter_zone_form' in initializer of 'CellData'`. Patched-worktree probes ran green (terrain 72/0/2, water 128/0/1), so the underlying suites are healthy — the gate itself is dead.
- **Impact**: Zero test feedback for the entire bin crate (~2,166 tests — every EXAL boundary, terrain, water, weather, LOD guard) until fixed; a silent-regression window opened 2026-09-19.
- **Suggested Fix**: Add `encounter_zone_form: None` to the six fixtures (mechanical; probe-verified), or convert them to a `..Default::default()`-style fixture so the next field addition cannot break six sites at once. Run the 1.96 test target in CI so this class fails the gate, not an auditor.

#### EXT-D4-2026-09-19-02: WTHR cross-fade and transition promotion drop the HNAM dimmers — post-fade exterior sun and ground-cover dimmer run on the source weather's values (Oblivion)
- **Severity**: HIGH · **Dimension**: Weather/sun · **Tier Violated**: single-boundary (EXAL-translated values dropped at the state seam) · **Game Affected**: Oblivion (per-WTHR HNAM; latent 1.0 elsewhere)
- **Location**: `byroredux/src/systems/weather.rs:798-808` (source dim), `:863-871` (target never dimmed), `:1188-1226` (`promote_weather_transition_target` — 11 fields copied, the two dimmers not); target construction `byroredux/src/scene/world_setup.rs:369-374`
- **Status**: NEW (orchestrator-verified at HEAD)
- **Description**: `translate_weather` correctly carries each WTHR's `sunlight_dimmer`/`grass_dimmer` (guarded), and `weather_system` multiplies the *source* palette (`:805-807`). But (a) the cross-fade's `target_sunlight` is sampled from `tr.target.sky_colors` without applying `tr.target.sunlight_dimmer`, so the fade eases from `src×d_src` to an undimmed target; (b) on completion the promotion copies sky/fog/TOD/wind/DALC/clouds but neither dimmer, so the promoted target palette is thenceforth multiplied by the *source's* dimmer and `GroundCoverDimmer` reverts to the source's — permanently, until the next transition or worldspace reload.
- **Evidence**: the only dimmer multiply is at `weather.rs:805-807` against `wd` (source); the transition block contains no `tr.target.sunlight_dimmer`; `grep dimmer` in the promotion body (`:1188-1260`) → no hit.
- **Impact**: Wrong exterior directional sun brightness and wrong ground-cover dimmer after every qualifying Oblivion weather change (dimmers are per-WTHR, so ordinary same-worldspace changes qualify); one-frame pop at fade end. Visual-only, no crash.
- **Related**: EXT-D4-2026-09-19-03 (the unpinned multiply is what let this land); `sunlight_dimmer_translates_from_the_hnam_block` covers translation only.
- **Suggested Fix**: Dim `target_sunlight` by `tr.target.sunlight_dimmer` in the transition block; add both dimmers to the promotion field copy; pin both with a dimmer ≠ 1.0 fixture.

#### EXT-D7-2026-09-19-01: Ground-cover eval harness never learned the washout lesson — re-mints plausible washed-out frames as references with exit 0
- **Severity**: HIGH (harness credibility — the workaround for "unit guards cannot see GPU output" is itself silently fragile) · **Dimension**: Acceptance harness · **Game Affected**: FNV, Skyrim SE
- **Location**: `scripts/renderer-eval-groundcover.sh:119-131`; protocol exists only in the audit skill prose; `docs/engine/exal-groundcover.md` §11.5 lacks it
- **Status**: NEW (the skill's 2026-09-15 washout note, verified)
- **Description**: The 2026-09-15 washout protocol (luminance-std precheck; backlit-only framing; veil check before A/B) is encoded nowhere in the script — its sole hard check is "PNG non-empty". Measured with the repo's own ImageMagick invocation: the committed `gc-backlit-*` references at `00d4ef5d0` measure sd 0.0092–0.0106 (≈2.3–2.7/255, far under the ~10/255 contrast line) — the ground-framing poses in the accepted baseline ARE the veil. A re-run today re-mints them as sha256-pinned references, exit 0; even `m-exteriors.sh`'s `image_health` sd floor (>0.005) would pass them.
- **Impact**: Every ground-cover visual claim (tier cross-fade #4056 acceptance, palette, wind A/Bs) is judged through a harness that cannot distinguish its known-bad baseline from a fixed one; the failure mode is invisible in exit code and manifest.
- **Related**: EXT-D7-2026-09-19-03 (no stats anywhere), #4056, #3807
- **Suggested Fix**: Add the luminance-std probe to `capture()` (fail or stamp `washed_out=yes` when a `gc-backlit-*` frame lands under 0.039 sd); record the protocol in §11.5.

### MEDIUM

#### EXT-D1-2026-09-19-02: `env.health` gates a subset of the canonical surfaces — water/weather/fog mostly ungated; four `WaterMaterial` copy sites are verbatim unguarded f32
- **Severity**: MEDIUM · **Dimension**: EXAL boundary · **Tier Violated**: none (gate-coverage gap) · **Game Affected**: all
- **Location**: `byroredux/src/commands/env_health.rs:117-211` (imports only `CellLightingRes, SkyParamsRes`); unguarded copies `byroredux/src/env_translate.rs:688` (`sun_specular_power`), `:703-704` (`wave_amplitude/frequency`), `:528-535` (fog/depth/underwater colors+distances)
- **Status**: NEW
- **Description**: `check_environment` covers `CellLightingRes` + `SkyParamsRes` + one of four `FogMedium` fields. It never inspects `WaterMaterial` (~40 f32 fields reaching `water.frag` push constants), `WeatherDataRes::fog`, the embedded `WeatherSkyState`, or three `FogMedium` fields. `resolve_water_specular`/`_noise_and_rain` copy the listed fields verbatim from the WATR record while their neighbours (`roughness`, `opacity`, `alpha_controls`) are finite-guarded — so a corrupt WATR NaN reaches the shader with no input gate anywhere on the path. (`FogMedium` itself is finite-by-construction at `fog.rs:121-127,212-227`.)
- **Impact**: A single NaN field on one WATR poisons water push constants every frame with no `env: FAIL`; the exterior smoke matrix gates on `env.health` and would report PASS. Pixel-level `RenderHealthCommand` is the only backstop. (Clamping policy itself is WATAL's; this is the gate half.)
- **Suggested Fix**: Extend `check_environment` (or add `water.health`) to walk `WaterMaterial`'s scalars + the missing fog fields; alternatively finite-guard the four verbatim copy sites like their neighbours.

#### EXT-D2-2026-09-19-02: Hostile VTXT opacity is an unvalidated wire f32 — falsifies `select_top_by_coverage`'s "NaN cannot appear" premise
- **Severity**: MEDIUM · **Dimension**: Terrain/splatting · **Tier Violated**: no-fabrication (documented guarantee not provided) · **Game Affected**: all LAND games
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:1315-1318` (raw decode); `byroredux/src/cell_loader/terrain.rs:420-427` (false-premise comment), `:439-446` (`total_coverage`)
- **Status**: NEW
- **Description**: VTXT opacity is `f32::from_le_bytes` with no finite/[0,1] gate, stored verbatim, while both the parser doc ("values 0.0–1.0") and the comparator comment ("NaN cannot appear") assert the guarantee. A NaN alpha makes `total_coverage` NaN and the `partial_cmp().unwrap_or(Equal)` comparator non-total — std documents `sort_by` as may-panic on such input; probe at HEAD shows the NaN layer is instead **silently, nondeterministically dropped** and coverage ranking is corrupted. Out-of-range (±Inf) opacities are ordered but skew the 8-lane importance cap.
- **Impact**: A corrupt/merged-plugin VTXT silently changes which splat lanes survive — per-run, per-platform nondeterministic terrain texturing, with an in-code comment telling the next maintainer this input is impossible. No vanilla content trips it (corpus VTXT measured finite [0,1]).
- **Suggested Fix**: Gate at the decode choke point (`is_finite()` + `clamp(0.0, 1.0)` in `parse_land_record`, matching the VHGT `sanitize_land_height` pattern one layer up); a `total_cmp` comparator is the secondary defense.

#### EXT-D4-2026-09-19-03: The HNAM `sunlight_dimmer` consumer multiply is unpinned — every fixture builds dimmer 1.0
- **Severity**: MEDIUM · **Dimension**: Weather/sun (test-gap) · **Tier Violated**: single-boundary (boundary value unpinned through its consumer) · **Game Affected**: Oblivion (latent elsewhere)
- **Location**: `byroredux/src/systems/weather.rs:805-807` (the multiply); fixtures `:2060-2061`, `:2309-2310`, `:2418-2419`
- **Status**: NEW (the skill's 2026-09-19 seed, verified)
- **Description**: The only guard (`sunlight_dimmer_translates_from_the_hnam_block`) pins the translation; the consumer-side multiply onto `CellLightingRes.directional_color` is exercised exclusively at 1.0, so it is an identity in every test — a dropped, doubled, or misplaced dimmer passes the suite on all games. This is the gap EXT-D4-02 slipped through (landed 2026-09-18 unpinned).
- **Suggested Fix**: One `weather_system` test with `sunlight_dimmer: 0.5` asserting `directional_color == 0.5 × SKY_SUNLIGHT`, plus the promotion pin from EXT-D4-02's fix.

#### EXT-D5-2026-09-19-02: Sentinel-game-invariance guard pins 5 of ~15 sentinel fields and never exercises per-game decode
- **Severity**: MEDIUM · **Dimension**: WATAL · **Tier Violated**: single-boundary (the §4 table's guard cannot catch its violation) · **Game Affected**: all
- **Location**: `byroredux/src/env_translate.rs:3075-3150` (guard), `:2511-2537` (`calm_watr` helper)
- **Status**: NEW
- **Description**: `resolve_water_material_sentinels_are_game_invariant` builds both "game-shaped" records via a hand-rolled literal (`normal_encoding: Default::default()`, no `GameKind`, parser never involved) — so per-game sentinel sets decided at parse (FO3/FNV `OffsetNoise`, Oblivion NNAM-vs-Skyrim TNAM roles) are structurally untestable there — and pins only `ior`, `shoreline_width`, `uv_scale_a/b`, `foam_strength`, `normal_map_index`. Unpinned: `noise_map_indices [u32::MAX;3]`, wave sentinels, underwater fog/depth/alpha/absorption zeros, `blend_normals`, specular zeros, deep-tint fallback.
- **Impact**: A sentinel promoted to authored (or vice versa) for any unpinned field — the exact "silently changes underwater rendering for a whole game" case — compiles and passes. Implementation itself verified correct against watal.md §4 row by row.
- **Suggested Fix**: The watal §8 "per-game translate-up sentinel harness" (still open) is the fix vehicle: decode one real WATR per game behind the `BYROREDUX_*_DATA` fixture pattern and assert each game's full resolved sentinel set against `WaterMaterial::default()`.

#### EXT-D5-2026-09-19-03: WRLD `DNAM`/`NAM4` water heights parsed with no finite/sentinel gate — NaN propagates into spawn
- **Severity**: MEDIUM · **Dimension**: WATAL · **Tier Violated**: no-fabrication (non-finite must not become canonical) · **Game Affected**: FO3/FNV/Skyrim/FO4/FO76/Starfield (Oblivion has no WRLD DNAM)
- **Location**: `crates/plugin/src/esm/cell/wrld.rs:149-153` (`DNAM` → `default_water_height`), `:173-178` (`NAM4` → `lod_water_height`); consumers `byroredux/src/cell_loader/exterior.rs:72-81`, `cell_loader/water.rs:888-921`
- **Status**: NEW
- **Description**: `xclw_water_height` rigorously gates (finite + |h| < 1e9), but the worldspace-default and LOD heights are raw `f32::from_le_bytes`. A corrupt `DNAM`/`NAM4` yields `Some(NaN)` which flows into every water-less cell of the worldspace: NaN `Transform`/`WaterVolume` (NaN `<=` compares cull every triangle → silently dry ocean), and NaN vertex Y on the LOD plane.
- **Impact**: Hostile/corrupt plugin data only; no panic — whole-worldspace dead water or garbage LOD geometry, while the identical hazard one record over is fully gated.
- **Suggested Fix**: Route both reads through the `xclw_water_height` gate; add a parse test mirroring `xclw_short_or_nonfinite_is_none`.

#### EXT-D6-2026-09-19-02: FO76 ships the Creation `.bto` object-LOD family the scheme table calls "none"
- **Severity**: MEDIUM · **Dimension**: Distant LOD · **Tier Violated**: no-fabrication (inverse: assets exist, table claims none) · **Game Affected**: FO76
- **Location**: `byroredux/src/cell_loader/object_lod.rs:653-662` (`object_lod_scheme` → `_ => None`); corpus: `SeventySix - GeneratedMeshes01.ba2`
- **Status**: NEW (corpus census: 1007 `meshes\terrain\appalachia\objects\appalachia.<L>.<x>.<y>.bto` — L4 ×795 / L16 ×164 / L32 ×48, **no L8**)
- **Description**: The table's own comment invites archive evidence before adding an arm; the evidence now exists and shows FO76 ships the exact `BakedBto` naming family Skyrim/FO4 use. FO76 distant objects therefore never render from baked LOD — the #3321 false-premise shape, one game over. The skipped level-8 band would exercise the #3502 coarsen path on a mixed 4/16/32 ladder if wired; `LodBandLadder::for_game(FO76) = None` and the `!combined_lod_supported(FO76)` pin (lod_support.rs:398) need a joint decision.
- **Suggested Fix**: Scoped feature issue carrying the census; decide the ladder+scheme shape before touching the table.

#### EXT-D7-2026-09-19-02: m-exteriors and m34-day-night exit 0 "PASS" when every profile SKIPs — violating the documented 77 contract
- **Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all
- **Location**: `docs/smoke-tests/m-exteriors.sh:788,919-923`; `m34-day-night.sh:31-36`; contract `docs/smoke-tests/README.md:7-8`
- **Status**: NEW
- **Description**: README: "Missing game data is an explicit `SKIP` with exit code `77`, never a pass." w1 honours it; m-exteriors records SKIP and `return 0` per profile (all-skip → "PASS - every installed selected profile passed", exit 0); m34 `exit 0`s its SKIP. A data-less run (runner variable typo) is indistinguishable from green to anything consuming exit codes.
- **Suggested Fix**: Exit 77 when the summary contains SKIP rows and no profile ran; mirror the `playable-smoke.yml` 77-to-error promotion.

#### EXT-D7-2026-09-19-03: No harness guards the stale-binary / stale-SPIR-V trap before judging shader claims
- **Severity**: MEDIUM · **Dimension**: Acceptance harness · **Tier Violated**: no-fabrication (a capture attributed to shader X may be shader X-1) · **Game Affected**: all
- **Location**: `docs/smoke-tests/m-exteriors.sh:89-92`, `w1-water-traversal.sh:89-92`, `scripts/renderer-eval-groundcover.sh:80`; trap documented at skyal.md §4
- **Status**: NEW
- **Description**: skyal.md §4 records the measured trap ("a recompiled `.spv` does not reliably trigger a cargo rebuild … a ~6.9/255 mean 'regression' that did not exist"). None of the four harnesses encode the documented `touch crates/renderer/src/lib.rs` fix or recompile shaders, and m-exteriors/w1 skip `cargo build` entirely when the binary exists — so even Rust-side constant edits are not picked up. A smoke can validate a shader "fix" against the old shader.
- **Suggested Fix**: Replace `-x` existence checks with a real (cheap-when-fresh) build; perform or instruct the touch after any shader edit.

#### EXT-D7-2026-09-19-04: `composite_term` is fully wired but no script uses it; cycle/m34 gate the CPU sun value, never the rendered sky
- **Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all (Skyrim fixture most exposed)
- **Location**: `m-exteriors.sh:437-489` (cycle), `m34-day-night.sh` (whole file); wiring `render_debug.rs:13-30` → `composite.frag:752` → CLI/console — all present
- **Status**: NEW
- **Description**: skyal.md §4 prescribes `--render-debug-mode composite_term` (pre-bloom, pre-tonemap) plus a noise-floor double capture for sky claims. The plumbing exists end to end, yet cycle and m34 pin only `sun: intensity=4.000/0.000` — a `SkyParamsRes` CPU value (`commands/time.rs:132`) — plus a loose `image_health` floor. A sky that ignores the sun, a repeat of the §1.1 bloom-gain wash, or a sun-direction bug all pass every gate; m34 captures zero pixels.
- **Suggested Fix**: Capture each cycle phase under `composite_term` and gate a cheap pixel invariant (noon-vs-night mean luminance delta above a floor; noon sd above the washout line).

#### EXT-D7-2026-09-19-05: None of the four harnesses runs in any automated lane
- **Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all
- **Location**: `.github/workflows/playable-smoke.yml:17-24` (only p0/p1/p2 × skyrim_se/fnv); no workflow references any of the four scripts
- **Status**: NEW
- **Description**: The gates that see GPU output — the only gates that can — run exactly when someone remembers. The manually-dispatched workflow exposes neither the exterior matrix, W1 traversal, day/night, nor the ground-cover eval (nor fo3/fo4 game choices despite fixtures existing).
- **Suggested Fix**: Add `w1-water-traversal` and `m-exteriors static` as gate choices; schedule the ground-cover eval on the game-data runner so references are re-minted on purpose.

### LOW

#### EXT-D1-2026-09-19-03: Stale intra-doc link `translate_climate_sky` on the `SUN_INTENSITY_PEAK` contract doc
- **Severity**: LOW (doc rot) · **Location**: `byroredux/src/env_translate.rs:992` · **Status**: NEW
- `translate_climate_sky` does not exist anywhere; the bootstrap seed is `apply_environment` → `compute_sun_arc` + `translate_sky`/`translate_exterior_cell_lighting`. Dead producer name on the one doc whose job is keeping three producers in sync; broken rustdoc link. Fix: rename the bullet.

#### EXT-D1-2026-09-19-04: Uncited fallback constants in `weather_sky_state` / `WeatherSkyState::default`
- **Severity**: LOW · **Tier Violated**: no-fabrication (citation missing) · **Location**: `byroredux/src/env_translate.rs:1210-1216,1231-1235`; `components.rs:1195-1210` · **Status**: NEW (Related: #4314)
- moon_glare fallback 0.35, `sun_glare == 0 → 1.0`, default coverage 0.35, stars color: no comment, no spec cite, and 0.35 duplicated verbatim across two files citing neither. Fix: one doc note each + a cross-reference, mirroring the `FB_TOD_HOURS` pattern.

#### EXT-D2-2026-09-19-03: Three stale docs contradict deliberate terrain behavior
- **Severity**: LOW (doc rot) · **Location**: `crates/plugin/src/esm/cell/mod.rs:180-182`; `crates/renderer/shaders/include/bindings.glsl:473-475`; `crates/renderer/src/vertex.rs:56-58` vs `:178` · **Status**: NEW
- (1) `LandscapeData.normals` still teaches the pre-#4059 unsigned VNML decode. (2) `bindings.glsl` says `GpuTerrainTile` stride 144 — real 160 (guard + shipped SPIR-V pin it). (3) `Vertex.tangent`'s doc says terrain is zero-tangent while `new_terrain` deliberately emits `[1,0,0,-1]` (#2474/#2822) — item 3 invites re-flipping terrain normal-map orientation. Fix: three one-line corrections, #2474/#2822 referenced.

#### EXT-D2-2026-09-19-04: Test-gap — the ≤8-lane packer bound and the terrain tangent constant are unpinned
- **Severity**: LOW (test-gap) · **Location**: `byroredux/src/cell_loader/terrain.rs:848-855`, `:173-184,415-433`; `crates/renderer/src/vertex.rs:146-168` · **Status**: NEW
- `splat1[i-4]` panics for `i ≥ 8`; only cross-function arithmetic prevents it (traced airtight, no `debug_assert`). No test pins `new_terrain`'s tangent (the Path-1 TBN selector) while the field doc says the opposite (EXT-D2-03). Fix: `debug_assert!(layers.len() <= 8)` + a one-line tangent pin referencing #2822.

#### EXT-D3-2026-09-19-02: Blade-arena doc rot — comments/design log say 16 MB / 4,096 blades; code allocates 64 MiB / 16,384
- **Severity**: LOW · **Tier Violated**: no-fabrication (docs assert an allocation the code outgrew) · **Location**: `crates/renderer/src/vulkan/groundcover.rs:23-27`; `shader_constants_data.rs:321-332` (also cites a 2000-unit draw distance; real 3000); `docs/engine/exal-groundcover.md` §12.13 · **Status**: NEW (the skill's 2026-09-19 seed, verified)
- `7996edf61` (2026-09-16) raised candidates 64→256; the test pin and the memory-budget ledger were updated, three doc sites were not. Fix: update both comments; append a dated §12.13 line recording the second 4× rise.

#### EXT-D3-2026-09-19-03: Packed LOD word loses f32 integer precision after ~19.4 h uptime — tier bit corrupts into wrong-geometry draws
- **Severity**: LOW (clock-bounded, visual-only, restart-recovers) · **Location**: `crates/renderer/src/vulkan/groundcover.rs:1374-1381` (pack), `:1854-1862` (`+ tier as f32`); `groundcover_blade.vert:104-106` (unpack) · **Status**: NEW
- `(time_seconds × 60).floor() × 4 + tier` exceeds f32's exact-integer range (2^24) at t > 69,905 s; beyond it the odd-tier addition rounds away and the mid ribbon stream decodes as tier 0 while its indirect command carries mid vertex counts → indices land in neighbouring slabs. Same corruption shape as #4056, via float rounding. Fix: mask the serial to ≤22 bits before packing on both sides (the blue-noise tile rotation already wraps); extend the round-trip guard past `t > 2^24/4/60`.

#### EXT-D3-2026-09-19-04: Post-#4378 GLSL literal census — one arrival, cited-math class
- **Severity**: LOW (bookkeeping) · **Location**: `crates/renderer/shaders/include/groundcover_candidate.glsl:31-32` · **Status**: NEW
- Full sweep of all 15 `groundcover_*` GLSL against the #4378 census (diff from `4fc8ab8b2`): exactly one numeric arrival since 2026-09-15 — the R2 sequence's generalised-golden constants A1/A2, in-file-cited (Roberts 2018), exempt-by-class. Recorded so the next census sweep has the disposition; no uncited arrivals; no game tokens downstream of the boundary.

#### EXT-D5-2026-09-19-04: LOD render-only guard is a whitespace-sensitive source-substring scan
- **Severity**: LOW (guard quality; behavior verified sound independently) · **Location**: `byroredux/src/cell_loader/water.rs:1200-1213` · **Status**: NEW
- The guard `find`s function text and asserts the body lacks the exact string `"world.insert(\n        entity,\n        WaterVolume"` — any reformat or helper-mediated insert voids it silently. Underlying behavior verified correct (no `WaterVolume` on LOD entities; submersion/physics gate on `WaterVolume`). Fix: structural assert or a headless spawn assertion.

#### EXT-D5-2026-09-19-05: `WatrRecord::material_type_form` (MATT impact material) decoded, unconsumed, absent from the watal open-items register
- **Severity**: LOW · **Location**: `crates/plugin/src/esm/records/misc/water.rs:123-128`; zero consumers; watal.md §2/§4 silent · **Status**: NEW · **Game Affected**: Skyrim/FO4/FO76 (9 vanilla records)
- Unlike its siblings `surface_sound` and `effect_form` (both registered open), MATT is unlisted — a reader concludes the WATR decode is fully accounted for. Fix: add a §2/§4 row or consume it when water audio lands.

#### EXT-D6-2026-09-19-03: `probe_lod_corpus` cannot open BA2 archives — blind for FO4/FO76/Starfield
- **Severity**: LOW (tooling) · **Tier Violated**: no-fabrication (the anti-fabrication tool fabricates "zero") · **Location**: `crates/bsa/examples/probe_lod_corpus.rs:15-20` (`BsaArchive::open` hard-rejects non-BSA magic) · **Status**: NEW
- Every BA2 silently prints `skip`, so its output rows mean "not opened", not "none" — the exact #3321-class false-premise trap it was written to prevent (this audit's first table was such a misread before re-census via `ba2_grep`). Fix: dispatch on magic to `BsaArchive`/`Ba2Archive` or print an explicit BSA-only error.

#### EXT-D6-2026-09-19-04: terrain_lod docs claim LOD "never enters the TLAS" while the renderer admits camera-local LOD as shadow casters
- **Severity**: LOW (doc rot; behavior deliberate and coherent) · **Location**: `byroredux/src/cell_loader/terrain_lod.rs:11-13,811-817` vs `byroredux/src/render/static_meshes.rs:45-48,124-143` · **Status**: NEW
- The renderer's bounded restore path (`LOD_SHADOW_CASTER_DISTANCE` 24000 BU) admits near LOD blocks as RT shadow casters with on-demand BLAS from global buffers; the spawn-module comments predate the policy. Fix: reword to "no BLAS at spawn; the renderer's bounded restore path may admit camera-local blocks".

#### EXT-D6-2026-09-19-05: Partition guards never run over a mixed-depth availability pattern
- **Severity**: LOW (test-gap; property verified structural) · **Location**: `byroredux/src/cell_loader/lod_bands.rs:588-608,1085-1107` · **Status**: NEW
- Both overlap guards use uniform availability; real worldspaces mix depths per quad. The emit-OR-recurse descent makes the partition property structural and `lod_coverage::find_overlaps` audits live residency, so this is hardening: one mixed-pattern partition test closes it.

#### EXT-D6-2026-09-19-06: Coarse-band/hole-mask boundary safety unpinned for the FO3/FNV legacy ladder at `--radius 7`
- **Severity**: LOW · **Game Affected**: FO3, FNV · **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1179-1203` (guard is Skyrim/FO4-only, radius 6); `lod_bands.rs:122` · **Status**: NEW
- Traced safe today by one cell of margin (a level-8 quad persists to distance 8 = `radius_unload` at radius 7; hysteresis forces subdivision before overlap). A future `FALLOUT_LEGACY_REFINE_BU` retune could silently overlap with no failing guard. Fix: extend the reach guard to `Fallout3NV` with `radius_unload` swept to 8.

#### EXT-D7-2026-09-19-06: Test-gap ledger — Dim 3/4/5 claims whose final acceptance is human-only
- **Severity**: LOW (test-gap) · **Status**: NEW
- Tier cross-fade visuals (#4056, pending); FSR mask behavior on a live frame (m-exteriors hardcodes `--upscaler taa`; shader-text guard only); rendered cloud response (coverage → density, WTHR layer mips); the HNAM dimmer multiply (EXT-D4-03); the waterline delta oracle (near-vacuous: >0.01 full-frame diff across a 250-unit camera move — any scene change clears it). Covered twice, not a gap: sentinel water (unit + harness FLT_MAX/INT_MIN hard fail). Fix: prioritize the dimmer unit test and the ground-cover luminance floor; the rest should say "manual" at the claim site.

#### EXT-D7-2026-09-19-07: Exterior matrix covers 5 of 7 games; W1 traversal route exists for 1 of 7
- **Severity**: LOW · **Game Affected**: FO76, Starfield (matrix); all but FNV (W1 route) · **Location**: `m-exteriors.sh:896-913`; `fixtures/` (only `fnv.env` declares `W1_WATER_SOURCE`) · **Status**: NEW
- Partially by design (FO76/Starfield exterior scope is documented open). The default `w1-water-traversal.sh` invocation on a fresh checkout is a skip — "W1 passed" in a session summary usually means FNV only.

#### EXT-D7-2026-09-19-08: Missing textures / failed NIFs are WARN-only while a chrome frame satisfies every hard gate
- **Severity**: LOW · **Location**: `m-exteriors.sh:361-369,764-769`; protocol in AGENTS.md · **Status**: NEW
- The checkerboard × normal-map "chrome" failure mode has sd far above the image floor and full population — it passes every hard gate with a WARN. (The FNV static baseline sd is itself only 0.0160, thin headroom over the washout regime.) Fix: keep content-drift WARNs, add a ceiling (e.g. hard-fail at ≥2× the profile's calibrated missing-texture baseline).

#### EXT-D7-2026-09-19-09: Ground-cover eval records manifest rows with absent or annihilated telemetry
- **Severity**: LOW · **Location**: `scripts/renderer-eval-groundcover.sh:124-131`; emission `groundcover.rs:334` · **Status**: NEW
- Empty `bench:`/`groundcover:` awk results still produce a manifest row, exit 0; a documented `chunks=0 blades=0` annihilated-product signature is likewise recorded without comment. Fix: fail or stamp the manifest when the row is missing or zeroed on a backlit case.

## Findings count

**30 findings: 0 CRITICAL · 3 HIGH · 10 MEDIUM · 17 LOW.** All NEW (deduped
against `/tmp/audit/issues.json` — 111 open — and all prior `AUDIT_*` reports;
no prior EXTERIOR-doctrine report exists). The 2026-05-03 report's sun findings
are all **FIXED with no regression** (SUN-N1 intensity ramp, SUN-N2 south tilt,
SUN-N3 glow, SUN-N4 disc gate — re-verified in code by Dim 4).

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #4056 (Phase 3 LOD chain / tier cross-fade) | OPEN — visual acceptance still pending (blocked by EXT-D7-01's washed references); palette `ground_coupling` field verified live in the GPU record |
| #4056-family | NEW sibling filed: EXT-D3-03 (f32 LOD-word precision, clock-bounded) |
| #4378 (ground-cover GLSL literals) | OPEN — census refreshed: one cited-math arrival since 2026-09-15 (EXT-D3-04), no uncited |
| #4304 (no canonical ground-cover `Material`) | OPEN, unchanged |
| #3982 (`MirroredPendingGuard` on `GroundCoverCell/Chunk/Species`) | OPEN, unchanged (blade record itself is lockstep-pinned) |
| #3307 (VWD full-model culling) | OPEN, accurate — chain half verified intact |
| #3142 (VWD per-entity read-lock) | OPEN, still present at HEAD (streaming_helpers.rs:394) |
| #1731 (VWD flag parse) | CLOSED — chain verified intact end to end |
| #3321 / #3502 | CLOSED — fixes verified; **new sibling EXT-D6-02 (FO76 `.bto`)** |
| #3808 / #4122 (`.spt` geometry) | Re-scoped design / OPEN in the SPT dimension; distant trees ride `.bto` here |
| watal §2/§4 register | XNAM/SPEL hazard + `surface_sound` tracked-open; Skyrim DNAM tail closed as nothing-decodable; **NEW: MATT unregistered (EXT-D5-05)**; §8 per-game sentinel harness still open (fix vehicle for EXT-D5-02) |
| skyal §2.3–§3 documented-open (taxonomy, noise frequencies, atmosphere LUT, positional cloud shadows) | OPEN, still documented — not re-filed |
| *groundcover_reference_capture_washout* (2026-09-15) | Verified real and **nowhere encoded** outside the skill prose → EXT-D7-01 |
| *unpinned `sunlight_dimmer` multiply* (2026-09-19 seed) | Verified → EXT-D4-03; and the related seam bug found → EXT-D4-02 |

## Cross-audit routing

- ESM byte decode (WATR/XCWT/WTHR/CLMT/LAND/VTXT choke-point gates): `/audit-esm` — EXT-D2-02 and EXT-D5-03 both fix at a parse boundary.
- Water shading/sync, SVGF, `water.frag` consumption (#4285), `composite_term` plumbing: `/audit-renderer`.
- Buoyancy; the physics wind-gust `WaterFlow` built without the translate-side clamp (`crates/physics/src/water.rs:245`): `/audit-physics` Dim 5.
- Submersion-system sampling under storage guards: Existing #4183 (`/audit-concurrency`).
