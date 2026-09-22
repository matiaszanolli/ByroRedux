# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-09-21

**HEAD**: `f97775ca8` (`73aaed7b9` landed mid-audit; it touches only HISTORY.md and ROADMAP.md, so no finding is affected) · **Baseline**: [AUDIT_EXTERIOR_2026-09-19.md](AUDIT_EXTERIOR_2026-09-19.md) (HEAD `7f8d71050`, 104 commits since) · **Audited**: Dims 1–7, delta-first (every dimension has commits since the baseline) · **Unchanged since baseline (skimmed)**: none at dimension level. Within Dim 4, the sky bake / clouds / SH / `byroredux/src/render/sky.rs` files have zero commits and were guard-spot-checked only.

Run as one leg of `/audit-suite --preset comprehensive`. Every dimension was analysed in the main context, one at a time, with no sub-agents. Scratch notes are in `/tmp/audit/exterior/dim_1.md` … `dim_7.md`, and the evidence scripts are in the same directory. No engine launch and no smoke script were run. Tests ran through `cargo test` only. Real game data was read directly from archive name tables and ESM records.

**Test state (first steps, all green at HEAD):** `env_health` 20/0 · `env_translate` 79 passed, 1 ignored · `terrain` 73 passed, 2 ignored · `groundcover` renderer 44/0, bin 45/0 · `sky_` 28 passed, 1 ignored · `weather` 63/0 · `water` 132 passed, 2 ignored · `lod` 130/0. The baseline's HIGH EXT-D1-01 (the bin crate did not compile) is fixed: #4480 landed in `010266416` / `7c8008bc4`.

## Executive Summary

**12 NEW findings: 0 CRITICAL · 1 HIGH · 3 MEDIUM · 8 LOW.** None is a regression, and none duplicates an open issue. All 30 findings from the 2026-09-19 baseline were filed as #4480–#4509 and are closed. This pass re-read every one of those fixes, because they are the newest and least-reviewed code in the domain:

- **25 of the fixes are correct and complete.**
- **Five are incomplete**:
  - #4496's budget pin re-derives the production arithmetic and cannot fail (EXT-D2-01).
  - #4488's FO76 census is already stale against the installed data (EXT-D6-01).
  - #4502 made the LOD probe open BA2s, but it still counts nothing BA2 games ship (EXT-D6-02).
  - #4492 wired the exterior matrix into CI through a path that cannot exist (EXT-D7-01).
  - #4490's structural half does not stop stale SPIR-V (EXT-D7-02).
- **One adjacent fix, #4544, patched a symptom whose cause is a frame error** (EXT-D5-01).

The new HIGH and two of the MEDIUMs are one family. **Direction data crosses the exterior boundary without a defined frame.** Each consumer then picks its own:

- **WATR noise-layer angles** are read about 90° rotated from the frame of the same record's own `NAM0` current. This is a census result over 159 layers in two games, not a guess.
- **Flat water's visible pattern moves along `-scroll`**, so it runs upstream on rivers.
- **The canonical `WindField.direction`** is read in four mutually incompatible ways by grass, trees, water and physics.

Unit guards pass throughout, because no test pins a sign.

### The HIGH

1. **EXT-D5-2026-09-21-01 — Skyrim/FO4 WATR `DNAM` noise-layer angles are read about 90° rotated from the frame the same record's `NAM0` current uses.**
   - **The census.** Across all 17 Skyrim and 36 FO4 records that author a current (159 layers), the offset between layer and flow has a circular mean of −84° / −88° (R = 0.72 / 0.65; Rayleigh p ≈ 6e-12 / 1e-19). Mirror readings collapse to R ≈ 0.2, so this is a rotation of the zero axis.
   - **Independent check.** Rotated by +90°, layer 0 of four of the five direction-named rivers lands within 6–25° of its editor-ID compass name. Under the current reading the error is 65–102°.
   - **What #4544 did.** It recorded the offset as authoring and attenuates it. That discards about 70% of the authored speed profile, and the residue still points mostly sideways. Its pin passes only because the engine's own synthesized flow term dominates.
   - **Physics impact.** Five generic river records (Skyrim `RiverWaterFlow` / `CreekWaterFlow`, three FO4) also take their physics current from the rotated angle.

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **HOLDS.** Callers of `translate_sky` / `translate_weather` / `translate_exterior_cell_lighting` / `procedural_fallback_*` are still `byroredux/src/scene/world_setup.rs`, `byroredux/src/systems/weather.rs` and `byroredux/src/cornell.rs` only. Water still has exactly two production composition sites: every other `WaterMaterial { .. }` literal sits in a `#[cfg(test)]` module. WTHR promotion now carries all 14 `WeatherDataRes` fields (#4481). It is still a hand-kept list, and the drop has recurred four times (EXT-D4-01). |
| **no-fabrication** | **HOLDS with dents.** #4494's constants are now single-declaration and cited. Dents: a rotated WATR layer frame emitted as canonical scroll (EXT-D5-01); FO3/FNV creek currents taken from a zero-variance editor default (EXT-D5-03); a stale FO76 census in code (EXT-D6-01); an anti-fabrication probe that still reports false zeros (EXT-D6-02). |
| **no-leak** | **HOLDS.** Sentinels are still documented. `env.health` now walks all 58 `f32`-typed `WaterMaterial` fields, the weather fog and all four `FogMedium` fields (#4483). LOD water is still render-only, and #4500 made its guard structural. The texture-only lowering that drops the authored MSN bit is confined to `.btr` (NIFAL-D1-2026-09-21b-01, confirmed below). |
| **no-render-time-fallback** | **HOLDS.** The delta adds no `game ==` logic: the only `GameKind` additions are the table-shaped `object_lod_scheme` arm and `for_object_game`'s FO76 data return. No game token appears in the delta's ground-cover, sky or water GLSL; the `water.frag` hits are comment provenance only. The direction-frame defects are canonical-meaning defects (EXT-D5-01/02, EXT-D3-01), not per-game branches. |

## Per-Category Matrix

Boundary function cited per category. ✓ = holds; the Findings column lists the exceptions.

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`) | ✓ | ✓ VTXT/WRLD wire floats gated (#4484/#4487) | ✓ | ✓ | D2-01 (guard quality) |
| **Sky / clouds / bake** (`translate_sky` / `SkyCubeParams::from_composite`) | ✓ | ✓ | ✓ | ✓ | none (no delta) |
| **Weather / sun** (`translate_weather`) | ✓ dimmers now promoted | ✓ fallback quartet cited (#4494) | ✓ | ✓ | D4-01 (hardening) |
| **Water, WATAL** (`resolve_water_material` / `attach_mesh_water`) | ✓ two sites | ✗ layer frame rotated (D5-01); dead-default creek axis (D5-03) | ✓ | ✓ | D5-01..04 |
| **Ground cover** (`groundcover_translate` / `WindField`) | ✓ | ✓ #4378 census: one lockstep-pinned literal | ✗ canonical wind frame undefined, consumers diverge (D3-01) | ✓ | D3-01 |
| **Distant LOD / trees** (`terrain_lod_layout` / `object_lod_scheme`) | ✓ | ✗ stale FO76 census (D6-01); probe false zeros (D6-02) | ✓ MSN drop confined to `.btr` | ✓ | D6-01, D6-02 |

Cross-cutting: **D1-01** (gate completeness) and **D7-01, D7-02** (acceptance harness).

## Findings

### EXT-D5-2026-09-21-01: Skyrim/FO4 WATR noise-layer angles are read ~90° rotated from the record's own NAM0 frame — #4544 attenuated the symptom
- **Severity**: HIGH. The skill rule applies: this is a wrong canonical value out of the WATAL translate, with a whole-game blast radius on Skyrim and FO4 and no render-time fallback.
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication (a mis-framed authored value is emitted as canonical scroll and current)
- **Game Affected**: Skyrim (LE/SE), FO4. Starfield uses the same decode shape (DNAM 84/88/92 degrees) but was not verified.
- **Location**:
  - The canonical vector is built by `resolve_water_layer_motion` (`byroredux/src/env_translate.rs:829-838`) as `[cos β, sin β]` in engine XZ.
  - The layer angles are decoded in `crates/plugin/src/esm/records/misc/water.rs:939-949` (Skyrim DNAM 100/104/108 degrees → radians, then `wind_direction = noise_wind_directions[0]`) and `:1144-1154` (FO4 DNAM 128/132/136, same copy).
  - The #4544 attenuation is at `env_translate.rs:533` (`WATER_CROSS_STREAM_SCROLL`) and `:900-918` (`confine_to_flow`).
  - The "authoring" claim is recorded in `docs/engine/watal.md:432-442`.
- **Status**: NEW (root cause of the symptom closed as #4544)
- **Description**: The `NAM0` current is decoded as `[x, -y]`, the engine's Z-up→Y-up convention, which agrees with REFR `XWCU` (`byroredux/src/cell_loader/water.rs:698`). Its bearings match all five direction-named Skyrim records to within 10°: NE 62°, NW 315.6°, SE 140.7°, CreekSE 135.8°, CreekSW 234.9°. The per-layer DNAM angles are raw degrees read as math angles in engine XZ, with no documented frame. Measured against the same record's `NAM0`, they are rotated by about 90°. #4544 treated this as authoring ("every direction-named vanilla record authors its per-layer wind directions ~90° off its own NAM0 flow"). It confines each layer to the flow axis, keeping the ~cos 84° ≈ 10% along-flow projection of a mis-rotated vector plus 25% of the cross remainder.
- **Evidence**: Census scripts are in `/tmp/audit/exterior/` (`watr_census.py`, `watr_census_fo4.py`, `watr_fallback.py`); they read the shipped records directly.

  **Skyrim.esm.** 17 WATR records carry a non-zero `NAM0` (51 layers).
  - Layer-minus-flow circular mean −84.1°, R = 0.72; speed-weighted −82.5°, R = 0.81.
  - 48 of 51 offsets are negative. Rayleigh p ≈ 6e-12.

  **Fallout4.esm.** 36 records (108 layers, a different DNAM layout).
  - Mean −87.7°, R = 0.65; 96 of 108 negative. p ≈ 1e-19.

  **Convention sweep.** Every rotation `φ = β + c` keeps R = 0.72 (Skyrim) / 0.65 (FO4). Every mirror `φ = c − β` (the missed-y-flip shape) collapses to R = 0.19–0.36. Under `φ = β + 90°` (a compass "wind-from" bearing) the mean offset is +5.9° (Skyrim) / +2.3° (FO4): the layers run downstream.

  **Independent check against editor IDs.** Error of layer 0 against its editor-ID compass name, current reading → +90° reading:

  | Record | Current reading | +90° reading |
  |---|---|---|
  | `RiverWaterFlowNE` | 82° | 8° |
  | `RiverWaterFlowNW` | 84° | 6° |
  | `RiverWaterFlowSE` | 65° | 25° |
  | `CreekWaterFlowSE` | 45° | 45° |
  | `CreekWaterFlowSW` | 102° | 12° |

  **What survives #4544.** `RiverWaterFlowNE`'s layer 0 residual points 122° off the flow at 0.29× magnitude. `riverwater_flowne_scroll_runs_downstream_not_sideways` passes because the synthesized `flow.speed × WATER_SCROLL_UV_PER_BU_PER_S` term (2.88 BU/s → 0.131 UV/s) swamps the ~0.026 residual.
- **Impact**: Every Skyrim and FO4 water surface with authored layer motion scrolls on a ~90°-rotated field.
  - **Rivers.** Since #4544, about 70% of the authored speed profile is discarded; what remains still points mostly sideways. Under the corrected frame the fastest authored layer (0.30 UV/s on `RiverWaterFlowNE`) would run within about 10° of the current.
  - **Physics.** River-classified records without a usable `NAM0` take their physics current from the rotated layer-0 angle via `wind_direction`, so floating bodies are pushed about 90° off course. The records are Skyrim `RiverWaterFlow` (Update.esm), `CreekWaterFlow` (Skyrim.esm + Update.esm), and FO4 `ExtCreekSanctuaryWaterUVFlow`, `DLC04QuantumRiverWater` and `DLC04QuantumRiverWaterInt`. `RiverWaterFlow` authors the same 233° layer 0 as `RiverWaterFlowNE`, whose explicit `NAM0` runs ENE.
- **Related**: #4544 (closed; symptom patch), EXT-D5-2026-09-21-02 (must land together), EXT-D5-2026-09-21-04, #2872, #3144
- **Suggested Fix**:
  - **The frame.** Read the layer angle as `[-sin β, cos β]` (φ = β + 90°) at the translate/parse boundary, with a Creation Kit or shader citation per the no-guessing rule. The census and the editor-ID names fix the rotation. The engine's own contract fixes the sign: scroll is a world-space layer velocity, and FO76/Starfield `NAM0` fills are parallel to the current.
  - **Follow-ups.** Drop or relax `WATER_CROSS_STREAM_SCROLL`, correct watal.md's #4544 paragraph, and pin the census relation with one real-record test.
  - **Ordering.** Land this together with EXT-D5-02; otherwise the restored downstream layers render running upstream.

### EXT-D5-2026-09-21-02: Flat water's visible surface motion runs along −scroll — upstream against the physics current and the rapids foam streaks
- **Severity**: MEDIUM (visual; a canonical-contract divergence between consumers)
- **Dimension**: Water translation (WATAL) contract × renderer Dim 8 consumption
- **Tier Violated**: no-leak (the canonical meaning of `scroll_*` / `WaterFlow` is not honoured by one consumer)
- **Game Affected**: all games with River/Rapids water; the wind term affects all water
- **Location**:
  - `crates/renderer/shaders/water.frag:303` and `:347`: `uv = … + scroll * time` in both `sampleScrollingNormal` branches.
  - `:764-770`: the waterfall branch alone negates, citing "A sampled texture moves opposite its UV scroll".
  - `crates/renderer/shaders/water.vert:209-212`: the wave-A phase `+ t·ω` moves its crest along −dirA, while wave B's `− t·ω` moves along +dirB. The CPU mirror is `crates/physics/src/water.rs:378-383`.
  - Producer: `byroredux/src/env_translate.rs:911` (`scroll_a = flow·rate + …`).
  - Contract: `crates/core/src/ecs/components/water.rs:254-258` ("World-space scroll vectors… vector 0 is `flow.direction * flow.speed`") and `:463` ("the dominant wave layer travels along direction").
- **Status**: NEW
- **Description**: A texture sampled at `T(x·s + v·t)` shows a feature moving at −v/s. On every River/Rapids plane the translate sets `scroll_a ≈ +flow·rate`, so the normal-map ripples and the 60%-weight wave-A crest travel upstream. The other two motion cues of the same current travel downstream:
  - floating bodies (physics `current_force` along +direction);
  - the rapids foam streaks (`water.frag:635`, `u = dot(pos, flowDir) − speed·t`).

  The weather term (`byroredux/src/render/water.rs:273-280` adds +wind) inherits the same sign, so wind ripples run upwind (see EXT-D3-01). The waterfall arm shows the convention is known; the flat arm never applied it.
- **Evidence**: The shader arithmetic above, with the source lines cited. No guard pins pattern direction: the `crates/renderer/src/vulkan/water.rs` source guards cover only the waterfall text. Confirming the look needs a captured frame: `docs/smoke-tests/m-exteriors.sh` in `water` mode from a river viewpoint, or `docs/smoke-tests/w1-water-traversal.sh` on FNV.
- **Impact**: Every flowing water surface shows its main visible motion opposing the current that carries bodies and foam. The mismatch is conspicuous on rapids, where streaks and ripples cross.
- **Related**: EXT-D5-2026-09-21-01, EXT-D3-2026-09-21-01; owner of the shader half: `/audit-renderer` Dim 8
- **Suggested Fix**: Choose one of two conventions, then pin pattern direction against flow in a unit or source test.
  - **Keep scroll as a world-space velocity** (the documented contract). Subtract `scroll * time` in both flat-water branches and flip wave A's time sign, keeping the CPU mirror in lockstep.
  - **Declare scroll a UV-offset rate.** Negate the synthesized flow terms and the weather term in the translate.

### EXT-D3-2026-09-21-01: The canonical `WindField.direction` has no defined frame, and its consumers disagree — grass leans against its own gust waves on N-S winds and opposite the trees on E-W winds
- **Severity**: MEDIUM (visual; a canonical-meaning divergence across five consumers)
- **Dimension**: Ground-cover pipeline (canonical `WindField`), cross-consumer
- **Tier Violated**: no-leak (one canonical value, several incompatible interpretations downstream)
- **Game Affected**: all games with ground cover; FO3/FNV/Oblivion additionally for SpeedTree sway
- **Location**:
  - Definition: `crates/core/src/ecs/components/groundcover.rs:377-387` ("Unit horizontal direction", no axis or sense) and `:452` (`normalise_or_east`).
  - Producer: `byroredux/src/env_translate.rs:1299` (`[cos, sin]` of the WTHR angle), fed through `byroredux/src/systems/weather.rs:1087`.
  - Consumers:
    - `crates/renderer/shaders/groundcover_blade.vert:241-242`: gust advection, `base.xz − windDir·v·t`.
    - The same file, `:256-257`: lean, `vec3(w.x, 0, −w.y)`.
    - `byroredux/src/systems/billboard.rs:230,240`: SpeedTree sway axis `(−w.y, 0, w.x)`.
    - `crates/physics/src/water.rs:246`: body drag `[w.x, 0, w.y]`.
    - `byroredux/src/render/water.rs:273-280` feeding `water.frag` (EXT-D5-02).
  - Spec: `docs/engine/exal-groundcover.md` §8 says "advected along direction" and never states the frame.
- **Status**: NEW (distinct from #3191, which fixed the SpeedTree bend's object-vs-world frame, not its sign against other consumers)
- **Description**: No single reading of the vector satisfies all five consumers. Four of them read it as engine (x, z): grass advection, SpeedTree's along-wind phase, physics drag and the water scroll. Only the grass lean negates y. Under that majority "toward" reading, three consumers are wrong: the grass lean is mirrored on z, and both the SpeedTree crown and the water ripples point upwind.
- **Evidence**: A glam 0.29.3 probe (`/tmp/audit/exterior/probe_wind`) mirrors each consumer's arithmetic:

  | Consumer | direction `[1,0]` | direction `[0,1]` |
  |---|---|---|
  | Tree crown | −x | −z |
  | Grass lean | +x | −z |
  | Grass gusts | +x | +z |
  | Water ripples | −x | −z |
  | Body drag | +x | +z |

  No test pins a lean sign. `billboard.rs:357-367` asserts only that the rotation changes; the blade shader has no direction guard.
- **Impact**: This shows in every windy exterior with ground cover.
  - **Wind with a north-south component.** Blades lean one way while §8's headline gust waves roll the mirrored way.
  - **FO3/FNV/Oblivion, east-west wind.** SpeedTree billboards lean opposite the grass beneath them.

  The effect is visual only.
- **Related**: EXT-D5-2026-09-21-02, #3191, #4186; co-owners: `/audit-speedtree` (sway), renderer Dim 8 (water), `/audit-physics` (drag)
- **Suggested Fix**:
  - Define the frame and sense once on `WindField`: engine xz, "blows toward".
  - Correct the consumers that break it:
    - Remove the grass lean's `−w.y`.
    - Reverse the SpeedTree axis (or the angle's) sign.
    - Fix the water sign under EXT-D5-02.
  - Add one pin per consumer asserting lean or motion along +direction.

### EXT-D7-2026-09-21-01: The `m-exteriors-static` CI gate executes *docs/smoke-tests/m-exteriors.sh.sh* — #4492's exterior-matrix lane can never run
- **Severity**: MEDIUM (the only automated lane for the cross-game exterior readiness matrix is dead on arrival)
- **Dimension**: Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: all
- **Location**: `.github/workflows/playable-smoke.yml:103` (`run_gate m-exteriors.sh "$ext_game" static`), with `run_gate` at `.github/workflows/playable-smoke.yml:51-63` building the path as docs/smoke-tests/&lt;gate&gt;.sh
- **Status**: NEW (defect in #4492's fix; #4492 closed)
- **Description**: `run_gate` appends `.sh`, but the new arm passes a name that already ends in `.sh`, so the step executes a path that does not exist, *m-exteriors.sh.sh*. The w1-water-traversal arm (`*)`) and the groundcover-eval arm are wired correctly. Only the exterior matrix, the gate the Dim 1–6 verdicts lean on, is broken. It fails red rather than green, but every dispatch measures nothing. `scripts/check-playable-smoke-contracts.sh` does not inspect workflow gate paths, so nothing caught it.
- **Evidence**: The workflow's own `run_gate` body was replayed with an `xvfb-run` stub that only checks the path. Output: `bash: docs/smoke-tests/m-exteriors.sh.sh: No such file or directory`, status 127. `set -e` then ends the step.
- **Impact**: The cross-game exterior matrix (WATR provenance, waterline delta, image health, the new #4491 sky pixel gate and #4508 chrome ceiling) never runs in automation. #4492's stated outcome is not achieved for the harness that matters most.
- **Related**: #4492 (closed), #4491, #4508, EXT-D7-2026-09-21-02
- **Suggested Fix**: `run_gate m-exteriors "$ext_game" static`. Add a contract-check line asserting every workflow gate resolves to an existing script.

### EXT-D1-2026-09-21-01: env.health's `WaterMaterial` walk is a hand-kept field list with no struct-completeness pin
- **Severity**: LOW (test gap; the gate is complete today)
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: n/a (gate coverage)
- **Game Affected**: all
- **Location**: `byroredux/src/commands/env_health.rs:149-228` (`check_water_plane`); the struct is at `crates/core/src/ecs/components/water.rs:158-360`
- **Status**: NEW (hardening of #4483)
- **Description**: #4483's fix names 10 radiance fields and 48 finite fields. Diffed against the struct, that is all 58 `f32`-typed fields. Nothing ties the list to the struct, so a newly added `WaterMaterial` float reaches the `water.frag` push constants ungated. That silently reintroduces the #4483 class.
- **Evidence**: `git log -S` shows five of the current fields (`sun_specular_power`, `rain_response`, `angular_velocity`, `roughness`, `concentration`) arrived within two days (2026-08-19/20), so the struct grows in bursts.
- **Impact**: The next WATAL field lands unchecked by the `env: FAIL` gate that the exterior smoke matrix consumes.
- **Related**: #4483 (closed)
- **Suggested Fix**: Add a test that serialises `WaterMaterial::default()` (it derives `Serialize` under `inspect`) and asserts every numeric leaf name appears in `check_water_plane`'s list. Alternatively, pin a field-count const beside the list.

### EXT-D2-2026-09-21-01: #4496's budget pin re-derives the production arithmetic, and the packer assert is unreachable from tests
- **Severity**: LOW (test quality; the bound itself is correct)
- **Dimension**: Terrain, splatting
- **Tier Violated**: n/a
- **Game Affected**: all LAND games
- **Location**: `byroredux/src/cell_loader/terrain.rs:1647-1668` (`splat_layer_budget_never_exceeds_the_packer_bound`); `:852-856` (the `debug_assert!` inside `spawn_terrain_mesh`, which takes `World` + `VulkanContext`); `:173` (production `authored_budget = 8 - base_transitions.len()`)
- **Status**: NEW (guard gap in #4496's fix)
- **Description**: The pin's second half recomputes `8 - transitions.len()` and `min(20, budget)` itself, so `transitions + capped <= 8` holds by construction and cannot fail if the production formula changes. The commit says the assert "now fails at the first terrain-spawning test", but no unit test reaches `spawn_terrain_mesh`; the assert fires only in a debug engine run. The real premise, at most 4 base transitions, is pinned. Production reaches at most 3, because `canonical_base_ltex` is the first quadrant BTXT (`terrain.rs:767`). The budget formula itself is not pinned.
- **Evidence**: Source reading of both functions; `cargo test -p byroredux terrain` gives 73 passed, 2 ignored.
- **Impact**: A future budget edit can compile and pass, then index `splat1[i - 4]` out of bounds in the first exterior cell with more than 8 layers.
- **Related**: #4496 (closed)
- **Suggested Fix**: Extract the budget and truncation step into a pure function that `build_cell_splat_layers` calls. Pin it with more than 8 authored layers plus 3 transitions.

### EXT-D4-2026-09-21-01: WTHR transition promotion is still a hand-kept field copy — the drop has shipped four times
- **Severity**: LOW (hardening; correct today)
- **Dimension**: Sky, weather, sun
- **Tier Violated**: single-boundary (latent; the translated value is dropped at the state seam)
- **Game Affected**: all WTHR games
- **Location**: `byroredux/src/systems/weather.rs:1199-1254` (`promote_weather_transition_target`); the struct is `WeatherDataRes` at `byroredux/src/components.rs:1378` (no derives)
- **Status**: NEW (hardening of #4481)
- **Description**: All 14 `WeatherDataRes` fields are now promoted, which I verified by sweeping the struct. They are promoted by 14 hand-written copies, and the same omission has shipped four times: #1101 `wind_speed`, #1102 `skyrim_dalc_per_tod`, #4481 both HNAM dimmers, and `cloud_layer_velocities_authored` (found only by `4cd98e4e2`'s sibling sweep). The guards are per-field regression tests written after each drop.
- **Evidence**: The promotion body and the struct field list, compared field by field.
- **Impact**: The next `WeatherDataRes` field is dropped at every weather transition, leaving the source value in place permanently, with no failing test.
- **Related**: #1101, #1102, #4481, #3985
- **Suggested Fix**: Destructure `&tr.target` exhaustively, with no `..`, before the #3263 lock-order drop, so adding a field becomes a compile error at the promotion site.

### EXT-D5-2026-09-21-03: FO3/FNV River-by-name water gets a physics current along a dead editor default
- **Severity**: LOW (small magnitude: `SPEED_MIN` 0.5 BU/s plus a small synthesized scroll)
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication
- **Game Affected**: FNV, FO3
- **Location**: `byroredux/src/env_translate.rs:870-884` (`WaterFlow::for_kind(kind, [cos θ, 0, sin θ])` when no `NAM0`); provenance doc at `crates/plugin/src/esm/records/misc/water.rs:208-218`
- **Status**: NEW
- **Description**: Several creek records classify as River by name, so they carry a current. With no `NAM0`, the current axis comes from the prefix `wind_direction`. That field is a dead editor default: 90° on 53 of 53 FO3 records and 78 of 78 FNV records, per the field's own doc. Every such creek therefore flows toward engine +Z (game-world south).
  - #2872 already stopped trusting the co-located speed, because a zero-variance field "cannot be an authored per-water velocity".
  - #3185 ruled that "a name can establish the kind but not the axis — emit no physics flow".
- **Evidence**: A census of shipped masters. FalloutNV.esm: `CreekWater01`, `CreekWater02nv`, `CreekWater02AVGnv`, `CreekWater02nvbetter` and `RockCreekEstatesWater` are all River-by-name, all at 90.0°. Fallout3.esm: `CreekWater01` and `RockCreekEstatesWater`, same. Oblivion has none.
- **Impact**: Floating bodies and the water pattern in FO3/FNV creeks drift due south whatever the creek's real course.
- **Related**: #2872, #3185, #3144, EXT-D5-2026-09-21-01
- **Suggested Fix**: When neither `NAM0` nor `XWCU` is authored and the direction is the dead default, emit no `WaterFlow`, mirroring #3185.

### EXT-D5-2026-09-21-04: `WaterMaterial.scroll_*` doc describes the pre-#4544 composition in the wrong units
- **Severity**: LOW (doc rot)
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication (the contract text asserts a composition the code does not do)
- **Game Affected**: all
- **Location**: `crates/core/src/ecs/components/water.rs:254-258`
- **Status**: NEW
- **Description**: The doc says "(xy = m/s)" and "vector 1 is a perpendicular shear at half speed". In the code:
  - The translate emits UV/s (`flow.speed × WATER_SCROLL_UV_PER_BU_PER_S`).
  - Vector 0 also carries the confined authored layer.
  - #4544 made the shear 0.25×.
  - The doc still says "the cell loader" where the translate now lives in `env_translate.rs`.
- **Evidence**: `byroredux/src/env_translate.rs:911-918`
- **Impact**: This is the canonical-contract text EXT-D5-02's fix has to choose a convention against. Right now it is wrong on units and composition.
- **Related**: #4544, EXT-D5-2026-09-21-01, EXT-D5-2026-09-21-02
- **Suggested Fix**: Rewrite the doc as part of EXT-D5-01/02's contract fix: units, composition, shear factor and sign convention.

### EXT-D6-2026-09-21-01: #4488's FO76 census is stale against the installed archives — 3,056 `.bto` over two BA2s, including 590 level-8 quads
- **Severity**: LOW (doc and census; behaviour is availability-driven and correct)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: no-fabrication (a data claim in code that the data contradicts)
- **Game Affected**: FO76
- **Location**: `byroredux/src/cell_loader/object_lod.rs:658-662` (scheme comment) and `:1138-1142` (test message); `byroredux/src/cell_loader/lod_bands.rs:182-191` (ladder comment)
- **Status**: NEW (Related: #4488, closed)
- **Description**: The code says FO76 ships 1,007 `.bto` in `GeneratedMeshes01` (L4 ×795 / L16 ×164 / L32 ×48, "no level 8"). It also says the missing level-8 band "rides the #3502 coarsen-to-available escape". The FO76 archives were rewritten on 2026-09-20 at 11:56. A name-table census today finds the family in two archives:

  | Archive | `.bto` files | By level |
  |---|---|---|
  | `SeventySix - GeneratedMeshes01.ba2` | 1,001 | L4 789 / L16 164 / L32 48 |
  | `SeventySix - GeneratedMeshes02.ba2` | 2,055 | L4 1,465 / L8 590 |

  Behaviour is fine. Selection is availability-driven, and `…01.ba2` auto-loads `…02.ba2`: `crates/bsa/src/naming.rs` handles the two-digit series start, pinned by `siblings_starfield_two_digit_zero_start_offers_02_through_09`. The doc on `open_with_numeric_siblings` in `byroredux/src/asset_provider/archive.rs` still describes only the unsuffixed and `…0` cases. The problems are:
  - the premise a future ladder retune would read is wrong;
  - the census method (the `…01` archive only) is the #3321 shape.
- **Evidence**: A BA2 name-table lister (`/tmp/audit/exterior/ba2_names.py`) run over both archives; archive mtimes are 2026-09-20.
- **Impact**: A future FO76 ladder or refine tune would be justified by a missing band that exists.
- **Related**: #4488, #3502, #3321, EXT-D6-2026-09-21-02
- **Suggested Fix**: Replace the three comments with the two-archive census and drop the "no level 8" reasoning. Update the `archive.rs` sibling doc for two-digit series.

### EXT-D6-2026-09-21-02: `probe_lod_corpus` now opens BA2s (#4502) but still counts none of the LOD family BA2 games ship
- **Severity**: LOW (tooling)
- **Dimension**: Distant LOD and trees
- **Tier Violated**: no-fabrication (the anti-fabrication tool reports a false zero)
- **Game Affected**: Skyrim, FO4, FO76
- **Location**: `crates/bsa/examples/probe_lod_corpus.rs:55-118` (the matchers at `:78-88` recognise only `_far.nif`, `distantlod\` and `landscape\lod\`)
- **Status**: NEW (the outcome #4502 was filed to remove persists; #4502 closed)
- **Description**: The Creation family, `meshes\terrain\<ws>\<ws>.<L>.<x>.<y>.btr` and `meshes\terrain\<ws>\objects\*.bto`, is never matched. For Skyrim (BSA), FO4 and FO76 the tool therefore still prints zero. Starfield's zero is genuine, per #4488's re-check.
- **Evidence**: I ran the skill's own Dim 6 step (`cargo run -p byroredux-bsa --example probe_lod_corpus`) on both FO76 GeneratedMeshes archives and `Fallout4 - Meshes.ba2`. Each prints `landscape\lod entries=0 _far.nif=0 distantlod=0` (TOTAL 0). Those archives actually hold:
  - FO76: 3,056 `.bto`.
  - `Fallout4 - Meshes.ba2`: 6,205 `.btr` and 497 `.bto`.
  - FO4 in total, with DLCCoast and DLCNukaWorld: 8,271 `.btr` and 802 `.bto`.
- **Impact**: Anyone following the skill's "census before touching `object_lod_scheme`" rule gets a false "none" for exactly the games #4502 targeted.
- **Related**: #4502 (closed), #3321, EXT-D6-2026-09-21-01
- **Suggested Fix**: Add a fourth matcher for the `meshes\terrain\` family (`<ws>.<L>.<x>.<y>.btr` and `objects\*.bto`), per worldspace and per level. Normalise `/` to `\`, since BA2 names use `/`.

### EXT-D7-2026-09-21-02: The harnesses' stale-SPIR-V defence is still a comment — and the comment misdescribes the landed build.rs change
- **Severity**: LOW
- **Dimension**: Acceptance gates and harness
- **Tier Violated**: no-fabrication (a capture attributed to shader X may be shader X−1)
- **Game Affected**: all
- **Location**: `docs/smoke-tests/m-exteriors.sh:102-108`, `docs/smoke-tests/w1-water-traversal.sh:89-95` and `scripts/renderer-eval-groundcover.sh:90-95` (each says "A renderer build.rs rerun-if-changed on shaders/** is the structural fix, tracked separately"); `crates/renderer/build.rs:24-31`
- **Status**: NEW (the premise of #4490 persists; #4490 closed)
- **Description**: `a04ccaec3` landed that `rerun-if-changed`. It regenerates `shader_constants.glsl`, write-if-changed, so there is no rebuild loop, and it dirties the crate. It cannot recompile a `.spv`, so an edited-but-not-recompiled shader still ships stale SPIR-V. The repo already has the real check, `scripts/check-shader-artifacts.sh`, which `ci.yml` uses. None of the three harnesses runs it before capturing.
- **Evidence**: I ran `scripts/check-shader-artifacts.sh` today (glslang 11:16.2.0, matched). Result: `DRIFT crates/renderer/shaders/composite.frag.spv`, which is REN-D12-2026-09-21-01. Two commits (`09d9bc6f8`, `f97775ca8`) changed its generated constants without recompiling it, and CI has been red since 2026-09-20 22:29. Every exterior-owned `.spv` (water, sky, clouds, ground cover) reproduces byte-identically today.
- **Impact**: A visual claim can still be judged through a stale shader, the #4490 premise. The m-exteriors cycle mode captures through `composite.frag`.
- **Related**: #4490 (closed), REN-D12-2026-09-21-01, EXT-D7-2026-09-21-01
- **Suggested Fix**: Call `scripts/check-shader-artifacts.sh` in each harness preflight, failing or stamping the manifest on DRIFT. Update the three comments.

## Findings count

**12 findings: 0 CRITICAL · 1 HIGH · 3 MEDIUM · 8 LOW**, all NEW. None is a regression, and none matches an open issue. Dedup used `/tmp/audit/issues.json` (4,488 issues, 124 open) and every prior exterior-relevant audit report in `docs/audits/`, including today's siblings. Five findings (D2-01, D6-01, D6-02, D7-01, D7-02) identify incomplete fixes of closed baseline issues. EXT-D5-01 re-diagnoses closed #4544.

## Already covered today — cited, not re-filed

| ID | EXAL-side note from this pass |
|---|---|
| **NIFAL-D1-2026-09-21b-01** (HIGH; `.btr` model-space normals through the tangent path) | **Confirmed**, and confined: the only texture-only-lowered LOD family with a model-space normal map is `.btr` (`terrain_lod_btr.rs:412`). `.bto` (`object_lod.rs:333`) and Oblivion `_far.nif` (`placement_lod.rs:546`) use `translate_material`, which carries the authored MSN bit. The other texture-only family with an authored normal map is the synthesized legacy terrain LOD (`terrain_lod.rs:754-863`): Oblivion `landscapelod\generated\*_fn.dds` and FO3/FNV `landscape\lod\<ws>\normals\*`. I decoded 12 quads (6 Oblivion including Tamriel `60.*`, 6 FNV `WastelandNV` level 4; DXT1 mip 0). They are blue-up: mean RGB ≈ (119–131, 89–129, 225–255), with 89–100% of texels B-dominant, not the Skyrim green-up MSN encoding. So the ~69° lean does not extend to them. **Open question**: whether their G axis matches the synthesized mesh's bitangent. `new_terrain`'s w = −1 was derived for LAND splat UVs (#2822), not the LOD quad remap. That needs a texel-vs-heightfield basis check like the NIFAL leg's `btr_basis`. `docs/engine/exal.md` still lists the `.btr` normal map as deferred. |
| **ESM-2026-09-21-D2-03** (`LTEX.GNAM` keeps only the last grass) | **EXAL-side extension**: the canonical carrier `TerrainCoverInputs.authored_grass: [Option<u32>; 8]` (`byroredux/src/components.rs:459`) is itself one GRAS per lane. A parser-only fix therefore cannot carry the list across the boundary; widen both together. There is no live visual impact yet: the field is `#[allow(dead_code)]` until #4413's authored-card tier. |
| **PERF-D1-2026-09-21-01 / -02** (ground-cover per-frame SipHash maps and atlas rebuild) | Not re-examined; no overlapping finding here. |
| **CONC-D1-2026-09-21-01 / CONC-D2-2026-09-21-01** (ground-cover `prepare()` before the fence wait; counter readback without host visibility) | Not re-examined; the #4568 split moved the readback layout constants only (`groundcover_stats.rs`), not the barrier code. |
| **#4572** (placed-water subtree walk, no cycle guard) | Not re-examined; same function (`apply_placed_water_type`) that feeds EXT-D5-01's placed-water path. |
| **#4576** (blend→EFFECT divert removal; blended FX cards, ground fog included, become shadow blockers) | Exterior content affected: FO3/FNV/Oblivion ground-fog and mist cards. No LOD- or terrain-specific change in `f97775ca8`'s `predicates.rs` diff. |
| **#4589** (water-caustic visibility ray mask) | Renderer-owned; not re-examined. |

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #4056 (Phase 3 LOD chain / tier cross-fade) | OPEN. The visual-acceptance harness now fails washed-out backlit frames (#4482); acceptance is still human-only. |
| #4378 (ground-cover GLSL literals) | **CLOSED since baseline.** Census this pass: one literal arrival (`0x3FFFFFu`), derived from and pinned against `GROUNDCOVER_FRAME_SERIAL_MASK`. |
| #4304 (no canonical ground-cover `Material`) | OPEN, unchanged |
| #3982 (`MirroredPendingGuard` on ground-cover structs) | **CLOSED since baseline** |
| #4413 (authored-model tier) | OPEN. It will inherit ESM-D2-03 plus the one-GRAS-per-lane carrier (see above). |
| #3307 / #3142 (VWD culling / per-entity lock) | OPEN, unchanged (no delta in `streaming_helpers.rs`) |
| #4468 (FO3 DLC `.high.` LOD variant) | OPEN, unchanged |
| #4264 (texture-only spawners' module doc) | OPEN. Relevant to the NIFAL `.btr` fix, which should also update the doc. |
| #4285 (Starfield water concentration normalised in `water.frag`) | OPEN, unchanged |
| #4314 (cloud-shape constants uncited) | OPEN, unchanged (no delta in `clouds.glsl`) |
| #4552 / #4553 (`.btr` parallax literals; LOD clamp-unaware textures) | OPEN, same files as NIFAL-D1-21b-01 |
| #4122 (SPT tail desync) | OPEN; distant trees still come via `.bto` |
| skyal §2.3–§3 documented-open items | OPEN, still documented; not re-filed |
| watal.md §2/§4 register | MATT now registered (#4501). §8 per-game sentinel harness landed as an `#[ignore]` real-data test (#4486). The #4544 paragraph (`watal.md:432-442`) records the frame error as authoring, per EXT-D5-01. |
| The four exterior harnesses in automation (#4492) | Two of the three wired arms work (w1, groundcover-eval); the m-exteriors arm is dead (EXT-D7-01). |

## Cross-audit routing

- **`/audit-renderer` Dim 8**: the `water.frag` / `water.vert` sign fix for EXT-D5-02, and the water consumer of EXT-D3-01.
- **`/audit-speedtree`**: the SpeedTree sway sign (EXT-D3-01, `billboard.rs:240`).
- **`/audit-physics` Dim 5**: current and drag directions under EXT-D5-01, EXT-D5-03 and EXT-D3-01. Physics stays `GameKind`-free, so the fixes belong at the translate boundary.
- **`/audit-esm`**: the DNAM layer-angle frame is a decode-convention question at `crates/plugin/src/esm/records/misc/water.rs:939-949` / `:1144-1154`. EXT-D5-01's fix may land there instead of in the translate.
- **`/audit-nifal`**: NIFAL-D1-2026-09-21b-01 is confirmed and confined to `.btr` (above).

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-09-21.md` (labels: `terrain-exterior`; add `water` for EXT-D5-*, `shaders` for EXT-D5-02 and EXT-D3-01, `doc-rot` for EXT-D5-04 and EXT-D6-01, `test-gap` for D1-01, D2-01 and D4-01, and `game:fo76` for D6-01). The findings that need a captured frame to confirm the look are EXT-D5-02 and EXT-D3-01, via `docs/smoke-tests/m-exteriors.sh` in `water` mode and `docs/smoke-tests/w1-water-traversal.sh`.
