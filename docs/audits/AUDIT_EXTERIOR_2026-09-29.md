# Exterior Audit (EXAL / SKYAL / WATAL / Ground Cover / LOD) — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: [AUDIT_EXTERIOR_2026-09-27.md](AUDIT_EXTERIOR_2026-09-27.md) (HEAD `0e0d35b96`, 61 commits since) · **Audited**: Dim 1 (EXAL boundary), Dim 2 (terrain, #4899 only), Dim 5 (WATAL), Dim 6 (LOD), Dim 7 (gates/harness), plus the ESM-audit handoff (Starfield WTHR fog units) · **Unchanged since baseline (skimmed)**: Dim 3 (ground cover: clippy, a generated-ID swap and a debug-view include only), Dim 4 (sky/weather: one comment edit)

**Method.** Run inside `/audit-suite --preset comprehensive`. Every dimension was analysed directly in the main context, with no sub-agents. Each dimension's result was written to `/tmp/audit/exterior/dim_N.md` and reconciled into this report.

Real game data was read straight from the ESM records with small Python scanners (in the same scratch directory):
- `Starfield.esm`: WTHR, CLMT, WRLD, LGTM, WATR;
- `Fallout4.esm`: WTHR, WATR;
- `SeventySix.esm`: WATR.

**Scope limits.**
- No engine launch, GPU process or smoke script was run, because other audits were running at the same time.
- The GPU prefilter test (`gpu_filter_preserves_constant_radiance…`, `--ignored`) was not run.
- `probe_lod_corpus` was not run. Its #4933 change was reviewed as source.

**Test state (first steps, green at HEAD):**

| Suite | Result |
|---|---|
| `cargo test -p byroredux --bin byroredux -- env_translate env_health terrain groundcover weather water lod resident_vwd` | 480 passed, 0 failed, 5 ignored |
| `cargo test -p byroredux-renderer --lib -- groundcover sky_` | 82 passed, 0 failed, 1 ignored |

## Executive Summary

**4 findings: 0 CRITICAL · 1 HIGH · 1 MEDIUM · 2 LOW.** All four are NEW. None duplicates an open issue, a closed issue, or a sibling report dated 2026-09-27 to 2026-09-29.

**The baseline's fixes.** 12 of the baseline's 41 findings were fixed and closed since the baseline: #4898, #4899, #4900, #4904 and #4930–#4937. This pass re-read each fix.

- **Correct and complete (11):**
  - #4898: legacy LOD orientation now comes from the boundary through `TranslatedTerrainLodTexture::uv_at`.
  - #4899: cover affinity is keyed on the LTEX editor ID.
  - #4900: NAM0 no longer writes into the DNAM fields, which are in the record's own frame.
  - #4904: `merge_from` now uses an exhaustive destructure.
  - #4930: dual-phase flow-map advection.
  - #4931: a parse-side `wind_direction_authored` flag, and the #4734 guard is restored.
  - #4933, #4934, #4935 and #4936: probe, LOD comments, exal.md premise and LOD upload tag.
  - #4937: the `run_gate` token scan.
- **Incomplete (1):** #4932 fixed the doc rot it was filed for. But its new per-game frame table states unverified conventions as fact (EXT-D5-2026-09-29-01).

**The ESM handoff is resolved.** Starfield's WTHR `FNAM` fog distances are in **metres**, and nothing lifts them into engine units. `translate_weather` therefore fits the exterior fog over 0.14–43 m instead of 10 m–3 km (EXT-D1-2026-09-29-01, HIGH).

**Missed at baseline.** `63c0aee3b` landed before the baseline HEAD, and the baseline pass did not catch its effect on two exterior acceptance gates. Both now fail on every run (EXT-D7-2026-09-29-01).

### Verdict per tier invariant

| Invariant | Verdict |
|---|---|
| **single-boundary** | **Holds.** The WTHR translate callers are unchanged: `scene/world_setup.rs`, `systems/weather.rs`, `cornell.rs`, and `render/sky.rs:138` (#4902, still open). No `SkyParamsRes` or `WeatherDataRes` literal exists outside `env_translate.rs`, `components.rs` and tests. Water still has two production composition sites; every other `WaterMaterial {` literal is under `#[cfg(test)]`. There is one `layer_affinity` production caller. The LTEX name map is built once and merged exhaustively. |
| **no-fabrication** | **Dented.** Starfield's fog ramp is 70× too short (EXT-D1-01). `BASE_FOG_STRENGTH` 0.8 is a documented engine choice in code but appears in no exterior spec (EXT-D1-02). The watal.md frame table states conventions for FO3/FNV/FO76/Starfield that no census backs (EXT-D5-01). |
| **no-leak** | **Holds**, with one telemetry dent: env.health reports the unscaled medium, not what the GPU receives (EXT-D1-02). |
| **no-render-time-fallback** | **Holds, narrowly.** The only render-loop rebuild is still #4902. `BASE_FOG_STRENGTH` is applied at the frame boundary, but it is the same for every game and involves no `GameKind`. No game token appears in the sky, cloud, ground-cover or water GLSL. `grep GameKind crates/physics/src` is empty. |

## Per-Category Matrix

The boundary function is cited per category. ✓ means the invariant holds; the last column lists this pass's findings.

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Findings |
|---|:---:|:---:|:---:|:---:|---|
| **Terrain / splatting** (`spawn_terrain_mesh` + `build_cell_splat_layers`; `cover_affinity_key`) | ✓ (#4904 closed) | ✓ #4899 closed (#4903 open) | ✓ | ✓ (#4903 shader default, open) | — |
| **Sky / clouds / bake** (`translate_sky`) | ✗ #4902 (open) | ✓ | ✓ | ✗ #4902 (open) | — |
| **Weather / sun / fog** (`translate_weather`, `translate_exterior_cell_lighting`, `FogMedium::from_legacy_ramp`) | ✓ (#4914 open) | ✗ Starfield metres (D1-01) | ✗ env.health unscaled (D1-02) | ✓ | D1-01, D1-02 |
| **Water, WATAL** (`resolve_water_material` / `attach_mesh_water`) | ✓ | ✗ #4910 (open), doc (D5-01) | ✓ | ✓ | D5-01 |
| **Ground cover** (`groundcover_translate` / `resolve_authored_cover`) | ✓ | ✗ #4906 (open) | ✗ #4907 (open) | ✓ | — |
| **Distant LOD / trees** (`terrain_lod_layout`, `TranslatedTerrainLodTexture::uv_at`, `object_lod_scheme`) | ✓ (#4898 closed) | ✓ | ✓ | ✓ | — |

Cross-cutting: **D7-01** (the acceptance harness).

## Findings

### EXT-D1-2026-09-29-01: Starfield WTHR fog distances reach `translate_weather` in metres — every Starfield exterior with a resolved climate fogs out at ~43 m

- **Severity**: HIGH (a wrong canonical value out of an EXAL `translate_*`)
- **Dimension**: EXAL boundary discipline (resolves the `/audit-esm` 2026-09-29 handoff)
- **Tier Violated**: no-fabrication (a unit is mistranslated before the boundary)
- **Game Affected**: Starfield
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:66-133`: `normalize` never touches `index.weathers`.
  - `byroredux/src/env_translate.rs:1159-1167` (`translate_exterior_cell_lighting`) and `:1470-1502` (`translate_weather`).
  - `crates/plugin/src/esm/records/weather.rs:504-533` (FNAM arm).
- **Status**: NEW. It was an open question in AUDIT_ESM_2026-09-29 § Disproved, and no issue exists.
- **Description**:
  - `b9e961eeb`'s `spatial_units::normalize` lifts Starfield's metric scene data by `BETHESDA_UNITS_PER_METER` (70) at the parse boundary. That covers CELL XCLL fog, LGTM fog, REFR, SCOL, WRLD and NAVM distances and heights, but not WTHR.
  - `translate_exterior_cell_lighting` and `translate_weather` read `fog_day_near/far` and `fog_night_near/far` as engine units. They feed them to `FogMedium::from_legacy_ramp` → `fit_legacy_fog_extinction` (`fog.rs:230`), which divides by 70 to get metres.
  - A Starfield ramp of 10 / 3000 m is therefore fitted as 0.14 / 42.9 m, which gives roughly 70× the extinction.
- **Evidence (units settled from data):**
  - **Records.** `Starfield.esm` has exactly 3 WTHR:
    - `DefaultWeather` 0x15E: FNAM = 10 / 3000 / 10 / 3000;
    - `NewAtlantisWeather50` 0x27CF9B: the same values;
    - `SpaceWeather`: 10 / 500000.
  - **Same plugin, same author convention.** LGTM fog is unambiguously metric, and `normalize` already lifts it:
    - `ShipInteriorLT` 1 / 5;
    - `VecteraMineLT` 10 / 50;
    - `DefaultLightingTemplate` 750 / 12000.
  - **Magnitude match with FO4, which is in Bethesda units:**
    - FO4 WTHR fog far has a median of 180,000 BU (p90 250,000; n = 71). Starfield's 3000 m equals 210,000 BU.
    - FO4's day height range is 10,000 BU (the median over 63 records with the 72-B tail). Starfield's 120 equals 8,400 BU when read as metres.
    - Read as BU instead, Starfield's values would be a 43 m fog wall and a 1.7 m height band.
  - **Reach:**
    - All 21 Starfield CLMTs that carry a `WLST` point at one of the two 3000-far weathers (17 → 0x15E, 4 → 0x27CF9B).
    - 19 WRLDs author a `CNAM` climate.
    - Nothing in the exterior load path (`scene/world_setup.rs`, `cell_loader/exterior.rs`) gates on Starfield, so `resolve_default_weather` → `translate_weather` runs for every one of them.
- **Impact**:
  - Any Starfield exterior whose climate resolves renders fully fogged beyond ~43 m.
  - The same too-dense medium drives the composite height fog and the froxel volumetrics, since both use the `fog_extinction_per_meter` producer.
  - Starfield exterior support is still a named policy skip in `m-exteriors.sh:1079-1080`, so no gate would catch this.
- **Related**:
  - **Secondary, decode (route to `/audit-esm`):** `parse_wthr` gates the FNAM power/max/height tail on `Fallout4 | Fallout76` (`weather.rs:510-533`). Starfield's 72-byte FNAM has the FO4 shape (power 0.4/0.4, max 0.9/0.9, height mid 10 / range 120, far ranges 220/900), but it decodes only the first four floats. The authored 0.9 max opacity and the height profile are dropped, and the defaults 1.0 / 1.0 / engine scale height are used instead. Once decoded, the height fields also need the lift.
  - **Sibling, not verified:** Starfield WATR DNAM distance-like fields (`underwater_fog_near/far`, `noise_falloff`, `depth_amount`) are not lifted either. Their values are consistent with metres (`noise_falloff` 100 against FO76's 4096 BU; underwater far 75 against FO76's 850). FO76 shares `decode_dnam_starfield` and is not metric, so a lift must key on `GameKind::Starfield`. See open #4837 for the Starfield WATAL scope.
- **Suggested Fix**: In `spatial_units::normalize`, lift `index.weathers[*].fog_{day,night}_{near,far}`, and the `WeatherHeightFog` fields once Starfield decodes them. Add a spatial_units test like the XCLL/LGTM ones. Keep `translate_weather` unchanged, because the units must be settled before the boundary.

### EXT-D7-2026-09-29-01: Two exterior acceptance gates cannot pass since `63c0aee3b` — `m-exteriors` cycle/water use directory screenshot paths the debug server now rejects, and `m34-day-night` never opts the release debug server in

- **Severity**: MEDIUM
- **Dimension**: Acceptance gates and harness
- **Tier Violated**: n/a
- **Game Affected**: all (cycle/water profiles); Skyrim (m34)
- **Location**:
  - `docs/smoke-tests/m-exteriors.sh:330-392` (12 `screenshot $profile_dir/<name>.png` commands), `:517-527`, `:655`, `:661`.
  - `docs/smoke-tests/m34-day-night.sh:48-59`, `:76`, `:113-119`.
  - `crates/debug-server/src/system.rs:199-225` (`write_screenshot`).
  - `byroredux/src/main.rs:78-80`, `:1022-1039` (`debug_server_allowed`).
- **Status**: NEW. Both behaviours came from `63c0aee3b` (2026-09-27), which is an ancestor of the baseline HEAD `0e0d35b96`. The baseline's EXT-D7-02 looked only at that commit's env-var rename.
- **Description**:
  1. **Screenshot path rule.** `write_screenshot` accepts only a single bare filename and writes it under `./screenshots/`. Its own test asserts that `nested/out.png` is an error. `m-exteriors.sh` sends `screenshot $profile_dir/sunrise.png`, and so on, through byro-dbg:
     - Each capture is rejected.
     - The heredoc ends in `|| true`, so the rejection is swallowed.
     - The script then HARD FAILs `image_health` on every phase frame (cycle) and on the surface and underwater frames (water).
     - The `composite_term` captures (#4491) are never written. They are skyal.md §4's only pixel evidence for sky assembly.
  2. **Debug server opt-in.** In release builds `debug_server_allowed(false, None)` is false. `m34-day-night.sh` runs `cargo run --release` without `BYRO_DEBUG_SERVER=1`, so byro-dbg cannot attach, and all seven `require_output` checks FAIL. `w1-water-traversal.sh` received the opt-in in `db8351587`; m34 did not.
- **Evidence**: `m-exteriors.sh:293` sets `BYRO_DEBUG_SERVER=1`, so the cycle/water failure is only the path rule. `grep -L BYRO_DEBUG_SERVER` over the byro-dbg harnesses lists m34-day-night.sh.
  - The fix pattern already exists in `p3-hud.sh` and `p3-player-body.sh` (a070baaad): pass a bare name, then `mv "$SMOKE_DATA/screenshots/$name" "$LOG_DIR/"`.
  - `docs/smoke-tests/README.md` describes the `--bench-hold` → byro-dbg pattern that the skill and CLAUDE.md point to. It mentions neither the opt-in nor the bare-filename rule; only `docs/engine/debug-cli.md:16` does.
- **Impact**:
  - The only captured-frame gates for sky assembly (cycle), water shading (water) and the Skyrim sun response (m34) are permanently red, so they carry no signal.
  - #4898 still needs a captured distant-terrain frame. The CI arm (`m-exteriors static`, `playable-smoke.yml:100-107`) uses the CLI `--screenshot` path (`boot/mod.rs:209` → `app_events.rs:1463`) and is unaffected, which is why nothing noticed.
- **Related**: The same two regressions hit harnesses outside this audit. Route them:
  - `m48-4/5/6/7` HUD scripts (directory screenshot paths) → `/audit-ui`.
  - `m-trees.sh` → `/audit-speedtree`.
  - `m43-quest-runtime.sh`, `m47-triggers.sh` → `/audit-scripting`.
  - `r6a_stale_15_bench.sh`, `scripts/material-provider-matrix.sh` → `/audit-tooling`.
  - All of these are release builds with no opt-in.
- **Suggested Fix**: Capture by bare filename in m-exteriors and move each file into `$profile_dir`, as `p3-hud.sh` does. Add `BYRO_DEBUG_SERVER=1` (and `BYRO_DEBUG_PORT`) to m34's engine launch. Document both rules in `docs/smoke-tests/README.md`. A cheap static guard: a test that scans `docs/smoke-tests/*.sh` for a `screenshot <path-with-/>` line or for a release byro-dbg harness without the opt-in.

### EXT-D1-2026-09-29-02: `BASE_FOG_STRENGTH` (0.8) scales every medium's extinction at the frame boundary — absent from the exterior specs, and env.health reports the unscaled value

- **Severity**: LOW
- **Dimension**: EXAL boundary discipline
- **Tier Violated**: no-fabrication (documentation only), no-leak (telemetry)
- **Game Affected**: all
- **Location**: `byroredux/src/fog.rs:29-38`; `byroredux/src/app_frame.rs:596-602`; `byroredux/src/commands/env_health.rs:406-418`
- **Status**: NEW
- **Description**:
  - What it is:
    - `a070baaad` ("Implement player body attachment…") added a global 0.8 multiplier on `fog_medium.extinction_per_meter`, applied where `FrameInputs` is built. The in-code rationale is a 2026-09-28 presentation direction.
    - It is game-invariant, so it is not a `GameKind` fallback. The composite height fog, the froxel volumetrics (`draw.rs:914`) and `assemble_camera_and_lights` all receive the scaled value consistently.
  - Why it is a finding:
    1. The engine choice sits outside `FogMedium`, and no exterior spec records it: `exal.md`, `skyal.md` and `docs/engine/*` say nothing about it. That breaks the "constants cite a source or are a documented engine choice" rule where readers look for it.
    2. `env.health` (`env: fog … extinction=`) and the translate tests report the unscaled medium. Every live readout is 1.25× what the GPU integrates.
    3. It also thins interior XCLL/LGTM fog, although the rationale cites exterior haze only.
- **Impact**: Tuning or debugging fog from `env.health` is off by 25%. A future fix to `fit_legacy_fog_extinction` (the comment implies the fit reads denser than vanilla) would stack with an undocumented global scale.
- **Related**: EXT-D1-2026-09-29-01 (the Starfield units defect is ~70×, not something a 0.8 trim addresses).
- **Suggested Fix**: Document the choice in exal.md / skyal.md, including whether it is exterior-only. Either fold it into `FogMedium` at the translate, so `env.health` shows what the GPU gets, or have `env.health` print both values.

### EXT-D5-2026-09-29-01: #4932's per-game wind-angle frame table asserts the wind-FROM convention for FO3/FNV/FO76/Starfield without a census, and records Oblivion's different convention without noting that the translate still rotates it (#4910)

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Oblivion, FO3, FNV, FO76, Starfield
- **Location**: `docs/engine/watal.md` §2, "Wind-angle frame per game (#4932)"; `byroredux/src/env_translate.rs:586-588` (`watr_angle_to_engine_xz`, +90° for every game)
- **Status**: NEW (incomplete part of #4932). Related to open #4910.
- **Description**:
  - The new table lists FO3/FNV, FO76 and Starfield layers as the "same wind-FROM bearing" as Skyrim/FO4. The census that justified the +90° conversion covered only Skyrim and FO4.
  - The Oblivion row says layer 0 is a direction-of-travel angle measured counter-clockwise, not a bearing. The code nevertheless applies the single +90° bearing rotation to it.
  - The table reads as settled fact and never cites #4910, so a #4910 fix would contradict the canonical doc.
- **Evidence** (census this pass). Method: raw layer angle minus the engine-frame NAM0 heading `atan2(−y, x)`, circular mean, speed > 0 layers only.
  - FO4: −88.5° (n = 107, R = 0.65, Rayleigh p ≈ 2e-20). This reproduces the #4727 census.
  - FO76: −77.5° (n = 138, R = 0.36, p ≈ 1e-8). This supports the convention, though more weakly.
  - Starfield: +50.3° (n = 36, R = 0.21, p ≈ 0.2). This gives **no** support.
  - FO3/FNV: no NAM0, so this method cannot test them.
- **Impact**: The doc states an unverified frame as fact for three games and hides a known-open defect. There is no runtime change beyond #4910.
- **Related**: #4910. The numbers above are new evidence for it: FO76 is now supported, Starfield is not.
- **Suggested Fix**: Mark the FO3/FNV/FO76/Starfield rows with their evidence (FO76 supported; Starfield unsupported; FO3/FNV untestable by NAM0). Flag the Oblivion row as currently mis-rotated, citing #4910. Fix the rows together with #4910.

## Findings count

| Dimension | HIGH | MEDIUM | LOW |
|---|---|---|---|
| D1 EXAL boundary | 1 | — | 1 |
| D2 Terrain | — | — | — |
| D3 Ground cover | — | — | — |
| D4 Sky / weather / sun | — | — | — |
| D5 WATAL | — | — | 1 |
| D6 LOD / trees | — | — | — |
| D7 Gates / harness | — | 1 | — |
| **Total** | **1** | **1** | **2** |

**Dedup sources:**
- `/tmp/audit/issues.json` (163 open) and `issues_all.json`;
- `gh issue list --state all` searches for "Starfield WTHR fog", `BASE_FOG_STRENGTH`, "screenshot path smoke" and `BYRO_DEBUG_SERVER`, with no match;
- all sibling reports dated 2026-09-27 to 2026-09-29;
- PHYS-D5-2026-09-29-01 (water samplers disagree on currents): already filed by the physics audit, not re-reported.

## Known-Open Register (dated; what this pass changed)

| Item | Status this pass |
|---|---|
| #4898, #4899, #4900, #4904, #4930–#4937 | **CLOSED since baseline, fixes verified.** #4932's table is EXT-D5-01. |
| #4866 (model tier outside the GPU timer) | Still OPEN. It is fixed in code (`88c23887b`, pinned by `model_timer_encloses_all_phases_and_stats_copy`). This is the second report recommending closure. |
| #4760 (Skyrim env var) | Still OPEN. `playable-smoke.yml:45` now exports `BYROREDUX_SKYRIMSE_DATA` (falling back to the old variable), so baseline EXT-D7-02 looks addressed. Candidate for closure. |
| #4928 (duplicated TOD fold + stale lock-order comment) | **Half fixed.** `6d05c2bc0` (#4990) corrected the comment. `cloud_tod_slot` (`weather.rs:473`) and `fold_to_four_tod_slots` (`:704`) are both still present. |
| #4910 (+90° applied to every game) | OPEN. New census evidence in EXT-D5-01: FO76 supported, Starfield not, FO3/FNV untestable. |
| #4901, #4902, #4914–#4917 (Dim 1) | OPEN, unchanged (`weather.rs:1304` `image_space: _`; `render/sky.rs:138`). |
| #4903, #4905, #4918 (Dim 2) | OPEN, unchanged. |
| #4906, #4907, #4919–#4924 (Dim 3) | OPEN, unchanged. `landscape_grasses` is now merged (#4904), so #4906 is unblocked. |
| #4908, #4909, #4925–#4927 (Dim 4) | OPEN, unchanged. |
| #4911, #4929 (Dim 5), #4837 (Starfield oceanness) | OPEN, unchanged. See EXT-D1-01 for the Starfield WATR unit sibling. |
| #4912, #4913, #3307 (Dim 6) | OPEN, unchanged. |
| skyal §2.3–§3 documented-open items | Still documented; not re-filed. |
| `renderer-eval-groundcover.sh` washout | Not re-measured (no engine launch). The script is unchanged and uses the CLI `--screenshot`, so it is unaffected by EXT-D7-01. |

## Skill drift (for the next `/audit-exterior` edit)

- **Dim 7 `Paths:`** should include `crates/debug-server/src/system.rs` (`write_screenshot`) and the `debug_server_allowed` gate in `byroredux/src/main.rs`. Both were changed by a commit outside the listed paths, and that change broke two gates.
- **Dim 1 `Paths:`** does not list `crates/plugin/src/esm/records/spatial_units.rs`. A Starfield unit lift decides canonical exterior values, so it belongs to Dim 1's boundary check.
- **Dim 5 known-open text** ("+90° applied to every game on a Skyrim/FO4 census", #4910) can add FO76 as supported and Starfield as unsupported.

## Cross-audit routing

- **`/audit-esm`**:
  - The EXT-D1-01 fix lands in `spatial_units.rs`.
  - Starfield WTHR FNAM tail decode (`weather.rs:510-533` gates on FO4/FO76 only).
  - Starfield WATR distance-field units (`decode_dnam_starfield`, shared with FO76).
- **`/audit-ui`**: m48-4/5/6/7 HUD harnesses send directory screenshot paths (EXT-D7-01 class).
- **`/audit-speedtree`**: `m-trees.sh` lacks `BYRO_DEBUG_SERVER=1`.
- **`/audit-scripting`**: `m43-quest-runtime.sh` and `m47-triggers.sh` lack the opt-in.
- **`/audit-tooling`**: `r6a_stale_15_bench.sh` and `scripts/material-provider-matrix.sh` lack the opt-in; smoke README doc rot; #4760 closure.
- **`/audit-renderer`**: `BASE_FOG_STRENGTH` consumers (composite, volumetrics) for EXT-D1-02; no shader change is needed.

Suggested next step: `/audit-publish docs/audits/AUDIT_EXTERIOR_2026-09-29.md`.

**Labels:**
- `terrain-exterior` on all four findings.
- EXT-D1-01: `high`, `bug`, `esm-plugin`, `game:starfield`.
- EXT-D7-01: `medium`, `bug`, `test-gap`.
- EXT-D1-02: `low`, `doc-rot`.
- EXT-D5-01: `low`, `documentation`, `water`, `doc-rot`.
