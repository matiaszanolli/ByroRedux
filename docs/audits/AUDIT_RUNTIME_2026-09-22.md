# Runtime Telemetry Audit — 2026-09-22

**HEAD**: `ee6d3fb39` · **Baseline**: [AUDIT_RUNTIME_2026-09-16.md](AUDIT_RUNTIME_2026-09-16.md) · **Audited**: telemetry diff (fnv, fo3, oblivion, skyrim_se, fo4), harness/baseline integrity, bench schema tests, capture.sh self-test · **Unchanged since baseline (skimmed)**: golden-frame gate (touched 2026-09-21 for an unrelated combustion-lab fixture, not this leg's per-game cells), playable-slice smoke contracts (touched 2026-09-16/17 for legitimate FO3/FO4 fixture additions, verified doc claims still match code)

Final leg of the `/audit-suite --preset comprehensive` run for 2026-09-22, executed serially, no sub-agents spawned. Preflight was clean before the build and re-verified clean before every capture and at teardown (`pgrep -x byroredux; pgrep -x byro-dbg` — no PID, every time). Release build (`cargo build --release -p byroredux -p byro-dbg -j 4`, memory-constrained per instructions) finished clean in 41 s. `capture.sh --self-test` passed. `cargo test -p byroredux bench::` passed all 9 tests, including the three `runtime_baseline_schema_tests` (full gating metric set, `bench_mode` presence, draw-split row ordering).

All five baselined games (`fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4`) were captured serially via `capture.sh`, one at a time, with a clean teardown (real-PID kill, port free, `Entities:`-vs-`entities=` cross-check OK) after every run. Starfield was **not** run — see Not Run below.

## Per-game status table

| Game | Cell | Status | Δ vs baseline (gating rows first) |
|------|------|--------|------------------------------------|
| fnv | FreesideAtomicWrangler | PASS | entities 7414→7414 (exact); tex_missing_base_color 1→1; mesh_cache_failed 0→0; lights 30 pt/0 dir → 30/0 (exact); draws 2204/167b/36c raster=283 — **bit-identical** to baseline on all four draw rows; skin 217/1364+0 (exact); advisory: fps 79.8→88.5, frame p50/p95/max = 8.02/16.85/**33649.48** ms |
| fo3 | MegatonPlayerHouse | PASS | entities 3543→3543 (exact); tex_missing_base_color 0→0; mesh_cache_failed 0→0; lights 11/0 → 11/0 (exact); draws cmds 1579→1579 (exact), raster 123→121 (informational, −2), batches 114→112 (−2, inside ×1.1), gpu_calls 12→11 (−1, inside ×1.1); skin 7/1364+0 (exact); advisory: fps 74.7→79.0, frame p50/p95/max = 9.11/20.57/11244.09 ms |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS | entities 745→757 (+1.61%, inside ±2% band); tex_missing_base_color 0→0; mesh_cache_failed 0→0; lights 8 pt/2 dir → 8/2 (exact); draws cmds 330→335 (+1.5%, inside ×1.1), raster 132→132 (exact), batches 78→78 (exact), gpu_calls 5→5 (exact); skin_pool_max/overflow exact/0, skin_pool_live (advisory) 4→9; advisory: fps 109.0→98.5, frame p50/p95/max = 10.94/11.48/41.24 ms |
| skyrim_se | WhiterunDragonsreach | PASS | entities 9461→9461 (exact); tex_missing_base_color 0→0; mesh_cache_failed 0→0; lights 28/0 → 28/0 (exact); draws 2458/13b/3c raster=14 — **bit-identical** to baseline on all four draw rows; skin 133/1364+0 (exact); advisory: fps 110.9→93.5, frame p50/p95/max = 9.87/13.22/43.87 ms. No unexpected move — the 2026-09-02 archive rewrite noted in CLAUDE.md did not explain anything here because nothing moved. |
| fo4 | InstituteBioScience | PASS | entities 18969→18999 (+0.16%, inside band); tex_missing_base_color 1→1; mesh_cache_failed 0→0; lights 685/0 → 685/0 (exact); draws 3969/196b/13c raster=256 — **bit-identical** to baseline on all four draw rows; skin 264/1364+0 (exact); advisory: fps 79.5→81.0, frame p50/p95/max = 11.59/13.29/85.84 ms |
| starfield | citycydoniamainlevel | NOT RUN | insufficient headroom — see Not Run |

`tex_missing_all_slots` (informational, never gating) held at its 2026-09-16 floor on every game: fnv 1, fo3 0, fo4 2, oblivion 0, skyrim_se 2. `bench_draws_raster_cmds` stayed far below the 3000 parallel-sort threshold on all five (max 283).

Gates that held exact on every one of the five games: `light_count_point`, `light_count_directional`, `skin_pool_max` (1364), `skin_pool_overflow_attempts` (0), `bench_mode` (`renderer-static`, matching every baseline's `bench_mode` row and the harness's `BENCH_MODE` pin). `tex_missing_base_color` and `mesh_cache_failed_count` held at 0/baseline on all five — no regression, and nothing to tighten (09-16's #4420 already brought both to their true floor). Three of five games (`fnv`, `skyrim_se`, `fo4`) reproduced their **entire** four-row draw split bit-for-bit against the 2026-09-16 baseline. The draw-split invariant (`batches ≤ raster ≤ cmds`, `gpu_calls ≤ 2×batches`) held in every capture.

Zero HIGH or MEDIUM findings. One LOW harness-hygiene finding (RT-1, below).

## Gate matrix rows run

| Gate | Result |
|------|--------|
| Telemetry diff, 5 games | PASS (all five, see table above) |
| Harness self-test (`capture.sh --self-test`) | PASS |
| Baseline schema tests (`cargo test -p byroredux bench::`) | PASS — 9/9, incl. `every_baseline_carries_the_full_gating_metric_set`, `every_baseline_records_the_harness_bench_mode`, `draw_split_rows_are_internally_ordered` |
| Golden frame / playable-slice / smoke-contract gates | NOT RUN — this sweep was not blessing a build (same posture as the 2026-09-16 report); harness paths were inspected (below) but the scripts themselves were not executed |

## Harness and baseline integrity

- `git log --since=2026-09-16` on the exact integrity paths: `capture.sh`, `.claude/audit-baselines/runtime/`, and `byroredux/src/bench.rs` all show **only** the 2026-09-16 `#4417` commit (`49ea8ab96`) that produced the baseline this run diffed against — no drift since the last report. `byroredux/tests/golden_frames.rs` was touched once (`7cdc0c7ff`, 2026-09-21) to gate an unrelated combustion-lab volumetric-fire fixture; it does not touch the per-game cells this skill runs. `scripts/check-playable-smoke-contracts.sh` was touched by `613ad3124` (2026-09-17), which is legitimate feature work — it added FO3/FO4 fixtures and `FIXTURE_GATES` entries, not a harness-integrity regression. Verified the SKILL.md's documented `FIXTURE_GATES` claims (`fo3: p0,p5,p2`; `fo4: p0,p2`; `oblivion: p0`) still match `docs/smoke-tests/fixtures/{fo3,fo4,oblivion}.env` exactly, and confirmed the SKILL.md's own caveat that `BYROREDUX_OBLIVION_DATA` is still absent from the script's neutralized-variable list — still true, still just the known caveat the skill already names, not a new finding.
- Every committed baseline TSV's newest header is the single `# regenerated: 2026-09-16 (#4417 — every row re-captured in ONE renderer-static run)` block (plus per-file follow-on notes for #4418/#4420 on the same date) — one capture produced every row, consistent with today's finding that the draw-split invariant held cleanly on all five.

### RT-1: Baselines README still cites the pre-split `audit-runtime.md` skill path
- **Severity**: LOW
- **Dimension**: audit infrastructure / doc rot
- **Location**: `.claude/audit-baselines/runtime/README.md:91`
- **Status**: NEW (not previously filed; SKILL.md's own Harness-integrity section has flagged this class of doc rot as a thing to check "every run," but no prior AUDIT_RUNTIME report names this specific line, and `gh issue list` has no open or closed issue matching "audit-runtime.md" / "stale path" / "baselines README")
- **Description**: The README's closing line reads `See .claude/commands/audit-runtime.md §Phase 3 for the canonical metric list and direction rules.` The skill lives at `.claude/commands/audit-runtime/SKILL.md` (a directory + `SKILL.md`, not a flat `.md` file) — the path in the README does not resolve.
- **Evidence**: `ls .claude/commands/audit-runtime/` shows `SKILL.md` + `capture.sh`; there is no `.claude/commands/audit-runtime.md`.
- **Impact**: Cosmetic only — the README's own inline Schema section already restates the metric list and direction rules, so no workflow is actually blocked. A reader following the link gets a 404-equivalent.
- **Related**: None open.
- **Suggested Fix**: One-line edit: `.claude/commands/audit-runtime.md` → `.claude/commands/audit-runtime/SKILL.md`.

## Not Run

- **Starfield** (`citycydoniamainlevel`): no baseline exists for this cell regardless (a run would only produce "BASELINE CREATED," never a regression check), and this exact cell has twice safety-aborted under memory pressure (2026-09-11). Memory across all five completed captures today ran tight the whole time: physical free memory sat at 4.0–5.3 GB and swap held steady at ~16 GB of 31 GB in use throughout, on a shared machine with other agents active concurrently. That is well short of the "ample free RAM" this cell's ~95k-BLAS cold load needs, per the skill's own note. Per instruction, judgment call made **not** to force it rather than risk destabilizing the shared session. Recommend re-attempting when the box is quieter (`--sf-smoke` remains the interim coverage per the skill).
- **Playable-slice / smoke-contract / golden-frame gates**: not executed. As with the 2026-09-16 sweep, this run was scoped to the telemetry diff + harness integrity, not blessing a build.

## Cleanup

`rm -rf /tmp/audit/runtime` completed (baselines under `.claude/audit-baselines/runtime/` untouched — nothing there needed `BASELINE CREATED`/`BASELINE UPDATED`, since all five have current, non-stale baselines from 2026-09-16 and no `--regen` was requested). Final `pgrep -x byroredux; pgrep -x byro-dbg` confirmed clean — no engine or debug-CLI process left running.

Suggest `/audit-publish docs/audits/AUDIT_RUNTIME_2026-09-22.md` for the one LOW finding.
