---
description: "Runtime telemetry regression audit — drives a headless engine on per-game cells, diffs against checked-in baselines, and reports the state of the smoke / golden-frame gate matrix"
argument-hint: "--game <key|all> [--regen] [--cell <EDID>]"
---

# Runtime Telemetry Audit

Read `.claude/commands/_audit-common.md` (dedup, severity, finding format) and `_audit-severity.md` for shared protocol.

The **runtime arm** of the audit suite: the per-game `audit-*` skills inspect code, this one inspects what renders. It drives the engine headless on a representative cell, harvests `stats`, `tex.missing`, `mesh.cache failed`, `light.dump` and the `bench:` summary line, and diffs them against `.claude/audit-baselines/runtime/<game>-<cell>.tsv`. Counts moving the wrong way become findings. It also audits the *integrity of its own harness and baselines* (below).

## Gate matrix (what exists, what each needs)

| Gate | Run | Needs |
|------|-----|-------|
| Telemetry diff (this skill) | `.claude/commands/audit-runtime/capture.sh --game <key> --cell <EDID> --out /tmp/audit/runtime` | release build (`cargo build --release -p byroredux -p byro-dbg`), Vulkan device, `xvfb-run`, game data |
| Harness self-test | `capture.sh --self-test` | nothing (parsers, ±2 % tolerance, PID resolution, survivor sweep) |
| Baseline schema tests | `cargo test -p byroredux bench::` (`every_baseline_carries_the_full_gating_metric_set`, `every_baseline_records_the_harness_bench_mode`, `every_baseline_records_the_bench_camera_pose`, `draw_split_rows_are_internally_ordered`) | nothing |
| Golden frames (game-data-free, not per-game): `cube_demo_golden_frame`, `combustion_lab_golden_frame` | `cargo test --release -p byroredux --test golden_frames -- --ignored`; regen `BYROREDUX_REGEN_GOLDEN=1` | Vulkan device. Default-lane guard `committed_baseline_matches_the_current_invocation` fails when a case's capture args/frames change without a recapture |
| Playable slice `p0-door-interaction`, `p1-character-traversal`, `p2-melee-core`, `p3-hud`, `p3-player-body`, `p5-save-restart`, `p5-f5-f9-quicksave`, `p5-door-transition`, `p5-soak`, `w1-water-traversal` (`docs/smoke-tests/<gate>.sh [game]`); fixed-fixture `p4-quest-route` + `p5-quest-persistence` (Skyrim MS01), `p6-loading-model` (Skyrim + FO4) | game-parameterised, default `skyrim_se` (the two P3 gates are Skyrim-only today); fixtures `docs/smoke-tests/fixtures/{skyrim_se,fnv,fo3,fo4,oblivion}.env` | Vulkan + game data. Exit **77 = SKIP** (data absent), **2 = config error** (unknown game / gate the fixture does not declare, e.g. FIXTURE_GATES fo3: p0, p5-save-restart, p5-f5-f9-quicksave, p2; fo4: p0,p2; oblivion: p0; w1 only where a `W1_*` route is declared, FNV today), never a pass |
| Smoke contracts | `scripts/check-playable-smoke-contracts.sh` (CI job `playable-smoke-contracts`) | nothing: runs each gate with data neutralised and pins SKIP/config-error semantics |
| Milestone smokes | `m-exteriors.sh [game\|all] [static\|boundary\|cycle\|water]` (`water` freezes a fixture for all five LAND-carrying titles), `m41-equip.sh`, `m41-ragdoll.sh`, `m43-quest-runtime.sh`, `m47-triggers.sh`, `m48-menu-load.sh`, `m48-{4,5,6,7}-*-hud.sh`, `m-trees.sh`, `m34-day-night.sh` | Vulkan + game data. `m48-menu-load.sh` and the `m48-4-oblivion` / `5-fo3` / `6-skyrim` / `7-fo4` HUD smokes SKIP (77) without data (HUD smokes since `161ab5ae8`, #4724, all five in the smoke-contract script); `m48-5-fnv-hud.sh` has no SKIP path and is not in the contract loop |
| Renderer-correctness gates | `cargo test --release -p byroredux --test cornell_rt_oracle -- --ignored --test-threads=1` (Cornell L0-L5 oracle), `docs/smoke-tests/interior-godrays.sh` (game-data-free shaft A/B), `scripts/check-bench-determinism.sh` (renderer-static scene-state determinism, Cornell by default), `scripts/material-provider-matrix.sh 3 30` (five-game provider matrix), `scripts/fsr-bench-matrix.sh 3 300` (bench-of-record; per-scene CWD = the game's `Data/`) | Vulkan RT device (+ game data for the matrix and bench) |
| CI lanes | `.github/workflows/`: `playable-smoke.yml` and `rt-correctness.yml` (manual dispatch; self-hosted `byroredux-game-data` / `byroredux-rt` runners), `real-data-gates.yml` (nightly NIF corpus harnesses, `BYROREDUX_REQUIRE_GAME_DATA=1`); `ci.yml` jobs `playable-smoke-contracts`, `shader-artifacts` and `vulkan-validation` (lavapipe bench + `check-bench-determinism.sh`; since `6d5d8fa5f` (#4987) the bench step must also print `Selected GPU:` — a deviceless clean exit fails, guarded in `byroredux/src/scheduler_access_tests.rs`) | hosted / self-hosted runner |

To bless a build, run the playable-slice gates you have data for and report pass / SKIP / fail beside the scalar telemetry; never fold a SKIP into a pass. `docs/smoke-tests/README.md` is the per-script authority (specs `docs/engine/playable-vertical-slice.md`, `docs/engine/p2-combat-fixture.md`).

## Invocation surface

`--game <key>` expands through `assets/debug_profiles.toml` (`expand_game_profile_args`, `byroredux/src/boot/cli.rs`) into `--esm`/`--bsa`/`--textures-bsa` (+ `--materials-ba2` for FO4+) under `<--games-root | $BYROREDUX_GAMES_ROOT | /mnt/data/SteamLibrary/steamapps/common>/<subdir>`. `--cell <EDID>` loads an interior; `--bench-frames N` prints the single `bench:` line; `--bench-mode renderer-static` fixes `dt = 0` and holds the authored camera (`capture.sh` always passes it); `--bench-hold` keeps the engine alive for `byro-dbg` on port 9876. Release builds bind the server only behind the explicit `BYRO_DEBUG_SERVER=1` opt-in (63c0aee3b, #4752) — `capture.sh` sets it and `BYRO_DEBUG_PORT` on the launch and fails fast if the engine log reports the server unavailable instead of waiting out the ping loop. `capture.sh` also points `BYROREDUX_SETTINGS_PATH` at a fresh per-run file so a persisted `render.upscaler` cannot move the draw split (#4947); the `bench:` line reports the effective `upscaler=` / `render_extent=` / `output_extent=` — the TSVs do not store them, so report them beside the diff and treat a non-default upscaler as a harness fault.

**Profile keys** (`[profiles.<key>]`): `fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4`, `starfield`, `fo76`. `fo76` is an asset-level profile with no `sample_cells` ("not a supported cell-load target"): not a runtime-audit game. `.claude/audit-baselines/sf-esm/` holds Starfield ESM resolve-rate baselines for `--sf-smoke`, not this skill.

## Baselines and cells

The table carries **no metric values** (they went stale within weeks): read each TSV's `# regenerated:` headers. The whole set was re-captured in one `renderer-static` run on 2026-09-30 (#5125, `2e402317e`, engine `60cd7028c`), after `b9e961eeb`'s interior spawn ladder moved the authored camera in all five cells; that regen added the `bench_camera_pos` / `bench_camera_forward` rows. Later blocks: the carried `entities_total` moves bisected and attributed in the headers (#5131/#5133, `6fcf313e2`); fo4/skyrim_se draw rows re-captured for `3c197ed8c`'s early-fragment admission (#5235, `8418a1791`); `light_count_spot` added to all five, light rows re-captured at `f75ac66cb` (#5132, `aa075ed35`, 2026-10-05). No known-unattributed row remains: a new unexplained move is a finding. A pose-row mismatch means the draw rows compare two different views: attribute the pose change before reading any draw-split movement.

| Game | Cell | Baseline | Why this cell |
|------|------|----------|---------------|
| `fnv` | `FreesideAtomicWrangler` | yes | Primary guard; densest NPC interior (the `SkinSlotPool` cap case). Fallback profile sample `GSDocMitchellHouse`. |
| `fo3` | `MegatonPlayerHouse` | yes | Exterior-style architecture in an interior shell. |
| `oblivion` | `ICMarketDistrictTheGildedCarafe` | yes | Smallest, cleanest cell; the only one with directional emitters. |
| `skyrim_se` | `WhiterunDragonsreach` | yes | Per-entity hot-path stress; carries 2 corrupted control-char texture paths (AUDIT_RUNTIME_2026-06-14 RT-3). |
| `fo4` | `InstituteBioScience` | yes | BGSM-heavy + precombine CSG (M49). |
| `starfield` | `citycydoniamainlevel` | none | The 2026-08-30 CRITICAL stall was closed (#3540, `0c45e779`), but the residual load-time BLAS churn (~95k built) keeps the run huge, and the last two attempts (2026-09-11) were safety-aborted under memory pressure: **unverified**, neither known-good nor known-stalled. Needs ample free RAM; use `--sf-smoke` for coverage meanwhile. |

Any other `(game, cell)` with no baseline establishes one ("BASELINE CREATED"), not a regression guard. `--game all` runs each baselined game whose data dir resolves.

## Parameters

`--game <key|all>` (required) · `--cell <EDID>` overrides the default cell (re-run a reported symptom) · `--regen` overwrites the baseline with current values (same intent as `BYROREDUX_REGEN_GOLDEN=1`; only after an intentional, eyeballed change).

## Phase 1-2: Setup and capture

`mkdir -p /tmp/audit/runtime`; dedup per `_audit-common.md`; build release binaries. Run `capture.sh` from the repo root, **serially**; never hand-roll the launch or teardown. It launches under `xvfb-run` with `--bench-frames N --bench-mode renderer-static --bench-hold` (default 240 frames), pings `byro-dbg` (90 s), waits for the engine log's `bench:` line and then a `stats` answer (READY_DEADLINE_S in the script, default 180 s: cell load runs on the render thread, so the debug server answers `ping` long before it can answer queries), runs `stats` / `tex.missing` / `mesh.cache failed` / `light.dump` / `quit`, and writes `<out>/<game>-<cell>.engine.log` + `.telem.txt` (with `bench_frame_max_ms`).

- **The bench mode is part of the baseline.** `renderer-static` (authored camera) and `system-live` (live camera) cull different frustums, so the whole draw split moves with the mode, not the code (Oblivion GildedCarafe, same build: `330/20b/2c raster 22` live vs `330/78b/5c raster 132` static). `bench.rs` names `renderer-static` the regression-gate mode. Every TSV has a `bench_mode` row; **check it first** and diff nothing else on a mismatch. The harness refuses to start with `BYROREDUX_FIXED_DT` set (it silently selects a mode) and fails a capture whose `bench:` line reports another `mode=`. Never set `BYROREDUX_FIXED_DT` for this audit.
- **Teardown/attribution assertions** (each fails the capture): pre-flight (no `byroredux` process, port free; `pgrep -x`, never `-f`), real-PID kill (`xvfb-run` runs its command as a child, so signalling the wrapper leaves the engine holding port 9876 and the next game's numbers land under the wrong filename), and an `Entities:` (stats) vs `entities=` (`bench:`) cross-check within ±2 %. Any past `--game all` sweep on the old teardown may carry shifted telemetry. Parallel runs need a distinct `BYRO_DEBUG_PORT` per game for both engine and `byro-dbg`; otherwise serial.
- **Where metrics live**: `wall_fps`, `frame_p50/p95/max_ms`, `draws=N/Mb/Kc`, `entities=` and the trailing `skin=L/M+S` are all on the single `bench:` line (`byroredux/src/app_events.rs`), i.e. in `.engine.log`, not the `byro-dbg` stream. Do not read skin numbers from the once-per-second `engine::stats` line (a frozen `dt` never advances `TotalTime`).

## Phase 3: Extract metrics

Keys must match the committed TSV rows (`byroredux/src/bench.rs` `REQUIRED_METRICS`). Write scalars to `/tmp/audit/runtime/<game>-<cell>.current.tsv`.

| Metric | Source | Direction |
|--------|--------|-----------|
| `bench_mode` | `bench:` `mode=` | exact, checked first |
| `bench_camera_pos` / `bench_camera_forward` | `bench:` `camera_pos=` / `camera_forward=` (capture.sh copies them into the telem as `capture: bench_camera_pos=` / `_forward=`) | exact within 0.5 BU / 0.01 per component, checked second (#5125): a moved pose moves the frustum and the whole draw split, so on mismatch do NOT diff the draw rows — report the stale baseline and attribute the pose change (spawn placement, `--cell` resolution) first |
| `entities_total` | `bench:` `entities=` | within ±2 % (either direction) |
| `tex_missing_base_color` | `tex.missing`: count of `[slot=base_color]` lines | ≤ baseline (strict) |
| `tex_missing_all_slots` | `tex.missing` summary count | informational: report Δ, never a finding |
| `mesh_cache_failed_count` | `mesh.cache failed` summary | ≤ baseline |
| `light_count_point` / `light_count_spot` / `light_count_directional` | `light.dump`: count of `kind=Point` / `kind=Spot` / `kind=Directional` rows (**not** the `emitters: N` tally, which includes directional ones; not the mere presence of a `CellLightingRes` block). #5132 — the Point→Spot reclassification (`b9e961eeb`) makes the split load-bearing: without the spot row, a correct reclassification reads as a point-count regression and a wrong one goes unnoticed | exact |
| `skin_pool_max` | `bench:` `skin=L/M+S` (`M`) | exact |
| `skin_pool_overflow_attempts` | `skin=` (`S`) | `== 0` |
| `skin_pool_live` | `skin=` (`L`) | advisory Δ |
| `bench_draws_cmds` / `_batches` / `_gpu_calls` | `draws=N/Mb/Kc` | ≤ baseline ×1.1 |
| `bench_draws_raster_cmds` | `bench_draws_raster_cmds=R` | report which side of `DRAW_SORT_PARALLEL_THRESHOLD` (3000) it lands |
| `bench_fps_p50` / `_avg` | `wall_fps` (one aggregate serves both) | advisory |
| `frame_p50/p95/max_ms` | `bench:` | advisory, **not stored in the TSVs**: report absolute values only |

- **Advisory means never a finding** (`bench_fps_*`, `frame_*_ms`, `skin_pool_live`, `tex_missing_all_slots`): headless `xvfb-run` wall-clock jitter dominates small cells (a 14 % phantom fps "regression" with every structural metric unchanged, RT-2 #1701). For a real fps question re-run 3× or read `frame_p95_ms`/`frame_max_ms`. `entities_total` counts non-rendering bodies (colliders, rigs, markers) that creep benignly, so it gates only beyond ±2 %; the exact render-load contract is the draw split. `skin_pool_live` tracks population like `entities_total`; only overflow off 0 or a changed `M` gates.
- **Draw-split invariant, check before diffing**: `batches <= raster_cmds <= cmds` and `gpu_calls <= 2 × batches` (the two-sided blend split, `needs_two_sided_blend_split`, can record two draws per batch). A row set breaking it did not come from one run (a partial `--regen`): report a stale baseline and re-capture all four rows together. `draw_split_rows_are_internally_ordered` enforces only `cmds >= batches >= gpu_calls`.
- `tex_missing_base_color` vs `_all_slots`: `tex.missing` walks the full 26-role `MaterialTextureHandles` set; only the base-color bucket is comparable to old baselines.

## Phase 4: Diff and severity

Compare current vs baseline. **Absent baseline** → copy with a `# regenerated: YYYY-MM-DD` header, report "BASELINE CREATED". **`--regen`** → overwrite, "BASELINE UPDATED" (never a finding). **Regressed** (against its direction) → one finding per metric:
- HIGH: `tex_missing_base_color` or `mesh_cache_failed_count` grew; `skin_pool_overflow_attempts` off 0 (an entity renders in bind pose for lack of a slot; `SkinSlotPool` cap #1284).
- MEDIUM: any other count moved against direction; a stale/incoherent baseline (mode mismatch, camera-pose mismatch, draw-split invariant broken).
- LOW: drift within ±5 % on a tolerance metric.
A `≤`-direction metric that **improved** passes but must be reported ("improved: tighten via `--regen`"): a loose `≤ baseline` gate silently lets the old value return (#4420 tightened mesh-cache rows for this reason).

## Phase 5: Report

`docs/audits/AUDIT_RUNTIME_<TODAY>.md` (header per `_audit-common.md` Report finalization): a per-game table (`Game | Cell | Status PASS/REGRESSION/BASELINE CREATED/NOT RUN | Δ vs baseline`, including the advisory Δs), the gate-matrix rows you ran with pass/SKIP/fail, then findings as `### RT-<n>: <metric> <moved> on <game> <cell>` in the base format (`Location` = the baseline TSV row; `Status` NEW / Existing / Regression) plus `Game`, `Cell`, `Baseline`, `Current`. For a `tex_missing_base_color` bump, direct the fix at `tex.missing entities` (responsible REFRs), then the resolution chain `byroredux/src/asset_provider/texture.rs` and the single NIFAL boundary `translate_material` in `byroredux/src/material_translate.rs`; cross-check `crates/nif/tests/translation_completeness.rs` and run `/audit-nifal`. Suggest `/audit-publish docs/audits/AUDIT_RUNTIME_<TODAY>.md`.

## Phase 6: Cleanup

`rm -rf /tmp/audit/runtime` (baselines untouched). Confirm nothing is left running: `pgrep -x byroredux; pgrep -x byro-dbg` and `pkill -x` if either reports (`-x`, not `-f`: `-f` matches your own shell's command line).

## Harness and baseline integrity (audit these every run)

**Paths**: `.claude/commands/audit-runtime/capture.sh`, `.claude/audit-baselines/runtime/`, `byroredux/src/bench.rs`, `byroredux/tests/golden_frames.rs`, `docs/smoke-tests/`, `scripts/check-playable-smoke-contracts.sh`
**First step**: `git log --since=<last AUDIT_RUNTIME date> -- <Paths>`; run `capture.sh --self-test` and the schema tests above.
- Each TSV's newest `# regenerated:` block says why its rows moved and every row came from one capture (partial regens are how the draw-split invariant broke). A baseline moved without a code reason is a finding; committing a TSV diff in the same commit as the engine change is the rule (`.claude/audit-baselines/runtime/README.md`).
- The skill's metric list, `REQUIRED_METRICS` and the TSV keys agree; docs that quote baseline values or the pre-split *.claude/commands/audit-runtime.md* path (#4771 fixed the baselines README's) are doc rot.
- `scripts/check-playable-smoke-contracts.sh` builds its data neutralisation from each fixture's `FIXTURE_DATA_ENV` and self-checks that every data variable the harness references is neutralised (`e04fa1ef6`, #5118; the old hand list missed `BYROREDUX_OBLIVION_DATA`). It also pins fixture-specific values (e.g. the FNV frozen NPC ref/base pair, re-pinned in `d1ec1c57f`): a fixture retarget without a contract update turns the CI job red.
- Data-gated tests elsewhere (`crates/nif` corpus harnesses, `crates/menuxml/tests`, `crates/ui/tests`) return early without data; only `real-data-gates.yml` (with `BYROREDUX_REQUIRE_GAME_DATA=1`) makes an absent corpus a failure.
- Do not launch a windowed engine while the user's own instance runs (*feedback_no_parallel_engine_launch*); the plugin `--ignored` tests can spike 20+ GB (*plugin_ignored_tests_oom*): do not run them alongside a Starfield capture.

## References

Symptom record `docs/audits/FALLOUT_SYMPTOMS_2026-05-26.md`; smoke pattern `docs/smoke-tests/README.md`; golden-frame precedent `byroredux/tests/golden_frames.rs`; import-side sibling `crates/nif/tests/translation_completeness.rs`; `SkinSlotPool` cap #1284; draw-split #1258.
