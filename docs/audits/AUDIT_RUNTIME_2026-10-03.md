# Runtime Telemetry Audit — 2026-10-03

**HEAD**: `2c36c29d8` · **Baseline**: [AUDIT_RUNTIME_2026-09-29.md](AUDIT_RUNTIME_2026-09-29.md) (HEAD `9fcfdc3fc`) · **Audited**: telemetry diff (fnv, fo3, oblivion, skyrim_se, fo4), harness/baseline integrity, `capture.sh --self-test`, bench schema tests, golden-frame default-lane guards, commit attribution of every moved draw row · **Unchanged since baseline (skimmed)**: `byroredux/src/bench.rs`, `byroredux/tests/golden_frames.rs` (no commits since 2026-09-29)

The audit ran serially, with no sub-agents. Every engine launch went through `capture.sh`. The binaries were built from a **detached worktree at HEAD** (`../gamebyro-redux-rtaudit`, sharing `target/`). The main checkout's uncommitted work (`cell_loader/precombined.rs`, `loading_screen.rs`, `load_screen.rs`) is not in any number below. Pre-flight was clean before every launch: no `byroredux` process (`pgrep -x`) and port 9876 free. After every teardown the same check found no survivor.

## Headline

1. **No regression.** Every gating row passes on all five baselined games. The camera pose and bench mode match exactly on all five, so the draw rows are comparable.
2. **The 2026-09-30 regen (#5125) holds.** FNV, FO3 and Skyrim are row-exact on every gating metric except Skyrim's improved `gpu_calls` (below). FO4 is exact except two improved draw rows. Oblivion is exact except `entities_total` −1 (in band).
3. **Two improvements, both bisected to `3c197ed8c` (Fix #5057, early fragment tests for lighting-shader material kinds 1–16).** On fo4, `batches` went 723→700 and `gpu_calls` 110→108. On skyrim_se, `gpu_calls` went 35→34. The commit carried no TSV refresh, so the loose `≤` gate would let the old values return (RT-1).
4. **Oblivion TSV narrative is incoherent.** The two newest regen blocks are contract-derived one-row edits, buried mid-file under an older block that contradicts them (RT-2). The live capture confirms their value.
5. **Skill doc rot (RT-3).** The skill presents three fixed issues as open, and its gate matrix is missing the six newest P4–P6 smokes.
6. **Starfield: NOT RUN** (no baseline; free RAM was 13 GB available, and the skill requires ample headroom after two safety-aborted runs).

## Per-game status table

Every run reported:
- `mode=renderer-static`
- `upscaler=fsr3/quality`, render extent 853x480, output extent 1280x720. This is the engine default, not a harness fault.
- An entities cross-check with delta 0 between the `byro-dbg` `stats` stream and the `bench:` line.

| Game | Cell | Status | Δ vs baseline (gating first; advisory last) |
|------|------|--------|---------------------------------------------|
| fnv | FreesideAtomicWrangler | PASS | pose exact; entities 6985→6985; tex_missing_base_color 1→1 (`textures/grey.bmp`); mesh_cache_failed 0→0; lights 30 pt / 0 dir exact; skin max/overflow 1364/0; draws 2069/500b/120c raster 1284 exact (raster < 3000 threshold); advisory: skin_live 106→106, fps 133.5→133.2, frame p50/p95/max 6.82/8.13/**57872.79** ms (see §Observations), all_slots 1→1 |
| fo3 | MegatonPlayerHouse | PASS | pose exact; entities 3626→3626; tex/mesh 0/0; lights 11/0 exact; skin 1364/0; draws 1514/511b/65c raster 1285 exact (< 3000); advisory: skin_live 3→3, fps 87.3→93.6, frame 10.49/12.91/71.61 ms, all_slots 0→0 |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS | pose exact; entities 929→928 (−0.1 %, in band; see RT-2); tex/mesh 0/0; lights 8 pt / **0 dir** (live-confirms the #5189 contract-derived row); skin 1364/0; draws 335/18b/2c raster 20 exact (< 3000); advisory: skin_live 9→9, fps 172.9→194.0, frame 5.57/6.66/7.73 ms, all_slots 0→0 |
| skyrim_se | WhiterunDragonsreach | PASS (1 row improved) | pose exact; entities 9499→9499; tex/mesh 0/0; lights 28/0 exact; skin 1364/0; draws cmds 2494, raster 1786 (< 3000), batches 643 exact, **gpu_calls 35→34 (improved, `3c197ed8c`)**; advisory: skin_live 122→122, fps 163.6→178.9, frame 4.94/6.59/62.55 ms, all_slots 2→2 (the RT-3 control-char `textures/\bnor` + `ore_ebony_e.dds`) |
| fo4 | InstituteBioScience | PASS (2 rows improved) | pose exact; entities 16885→16885; tex_missing_base_color 1→1; mesh 0→0; lights 680 pt / 0 dir exact (+5 Spot; the bench `lights=685`, schema gap Existing #5132); skin 1364/0; draws cmds 3971, raster 3618 (**above** the 3000 `DRAW_SORT_PARALLEL_THRESHOLD`, as at the baseline), **batches 723→700**, **gpu_calls 110→108** (both improved, `3c197ed8c`); advisory: skin_live 132→132, **fps 24.1→44.8**, frame 21.52/23.37/147.60 ms (09-29 report: 39.23/40.50/160.50), all_slots 2→2 |
| starfield | citycydoniamainlevel | NOT RUN | no baseline; not launched (memory headroom, see Headline 6) |

**Draw-split invariant:** holds in every current capture and every committed TSV (`batches ≤ raster ≤ cmds`, `gpu_calls ≤ 2 × batches`).

**FO4 fps +86 % is advisory only.** Every structural row except the two `3c197ed8c` draw rows is identical, and the three FO4 captures (HEAD, `8dbe179ad`, `3c197ed8c`) were not compared for frame time. The improvement is not attributed here. If the bench-of-record wants it, re-run 3× and read `frame_p95_ms`.

## Gate matrix rows run

| Gate | Result |
|------|--------|
| Telemetry diff (`capture.sh`, 5 games) | **pass** (5/5 captured, 0 regressions; Starfield NOT RUN) |
| Harness self-test (`capture.sh --self-test`) | **pass** |
| Baseline schema tests (`cargo test -p byroredux --bin byroredux bench::`) | **pass** (10/10, including the four `runtime_baseline_schema_tests`) |
| Golden frames, default lane (`--test golden_frames`) | **pass** (4/4: `committed_baseline_matches_the_current_invocation` + 3 manifest tests); the two `--ignored` Vulkan cases were not run |
| Playable-slice / milestone smokes, Cornell oracle, determinism, CI lanes | not run this pass |

## Attribution probes

The probes ran in the worktree, using HEAD's `capture.sh` copied to scratch, with the same renderer-static camera, 240 frames and cross-check as the main captures.

| Commit | fo4 InstituteBioScience | skyrim_se WhiterunDragonsreach |
|--------|-------------------------|--------------------------------|
| `8dbe179ad` (= `3c197ed8c^`) | 16885 · 3971/**723b/110c** r3618 · same pose | 9499 · 2494/643b/**35c** r1786 · same pose |
| `3c197ed8c` Fix #5057 | 16885 · 3971/**700b/108c** r3618 | 9499 · 2494/643b/**34c** r1786 |
| `2c36c29d8` HEAD | 16885 · 3971/700b/108c r3618 | 9499 · 2494/643b/34c r1786 |

`8dbe179ad` reproduces the committed baselines bit-for-bit on both cells. The entire move lands at `3c197ed8c`.

## Findings

### RT-1: `bench_draws_batches` / `bench_draws_gpu_calls` improved on fo4 InstituteBioScience (723→700, 110→108) and `bench_draws_gpu_calls` on skyrim_se WhiterunDragonsreach (35→34): `3c197ed8c` landed without a TSV refresh
- **Severity**: LOW
- **Dimension**: Baseline integrity
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` rows `bench_draws_batches`, `bench_draws_gpu_calls`; `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` row `bench_draws_gpu_calls`
- **Status**: NEW
- **Game / Cell**: fo4 / InstituteBioScience; skyrim_se / WhiterunDragonsreach
- **Baseline**: fo4 723b/110c; skyrim 35c
- **Current**: fo4 700b/108c; skyrim 34c
- **Description**: Fix #5057 admits lighting-shader material kinds 1–16 to early fragment tests. That changes which pipeline a draw binds, so adjacent draws merge into fewer batches and indirect calls. The rows passed (`≤ baseline ×1.1`), but the loose gate now has headroom: FO4 could slide back to 723b/110c, and the ×1.1 tolerance would allow up to 795b/121c, all read as green. The rule in `.claude/audit-baselines/runtime/README.md` is to commit the TSV diff with the engine change. `3c197ed8c` touches no TSV.
- **Evidence**: see §Attribution probes. The same pose, entity count, `cmds` and raster prefix appear on both sides; only batch/call merging moved.
- **Impact**: None on rendering. The cost is that the gate cannot see a future batching regression of up to ~3 %, or the return of #5057's pre-fix pipeline split.
- **Related**: #5057 (closed by `3c197ed8c`); #4420 (the precedent for tightening improved rows)
- **Suggested Fix**: `/audit-runtime --game fo4 --regen` and `--game skyrim_se --regen`. Write one header block per TSV naming `3c197ed8c`, and keep all four draw rows from the same capture.

### RT-2: Oblivion GildedCarafe TSV: the two newest regen blocks are contract-derived one-row edits buried below an older block that contradicts them, and `entities_total` lags the live capture by the removed light entity
- **Severity**: LOW
- **Dimension**: Baseline integrity
- **Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`: header lines 1–24 (09-30 block), 38–44 (10-03 #5189 block), 45–58 (09-29 #5123 block); rows `light_count_directional`, `entities_total`
- **Status**: NEW
- **Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe
- **Baseline**: `light_count_directional 0`, `entities_total 929`
- **Current**: `light_count_directional 0`, `entities_total 928`
- **Description**: The skill's integrity rule is that each TSV's newest `# regenerated:` block explains the rows and that every row comes from one capture. This file breaks both parts:
  - **Header order.** Line 1 is still the 2026-09-30 #5125 block. Its text says "light_count_directional stays 1 (RT-2, #5123, fixed)", but the row says 0.
  - **Buried blocks.** The two blocks that actually set the row are #5189 (2026-10-03, 1→0) and #5123 (2026-09-29, 2→1). They are inserted at lines 38 and 45, inside the 09-16 narrative. A reader taking `head` (as this audit's first pass did) sees the wrong newest block.
  - **No live capture behind the row.** Both edits are self-described "CONTRACT-DERIVED regen, not live-captured", so `light_count_directional` came from no capture.
  - **Stale entity count.** Today's live run confirms the directional row at 0. It also shows the one fewer spawned light entity (`entities_total` 929→928) that the one-row edit left behind. The −1 is consistent with #5189's zero-spawn, but that is inferred, not bisected.
- **Evidence**: `grep -n '^# regenerated' …oblivion-ICMarketDistrictTheGildedCarafe.tsv` lists 2026-09-30 (l.1), 2026-09-16 (l.25), **2026-10-03 (l.38)**, **2026-09-29 (l.45)**. The current capture's `light.dump` shows 8 `kind=Point`, 0 `kind=Directional`, and `bench: entities=928`.
- **Impact**: None on the gate today (−0.1 % is in band and directional is exact). The risk is that the next reader attributes the directional row from the wrong block. It also sets the precedent of editing gate rows without a capture.
- **Related**: #5189, #5123 (both closed); #5125 (09-30 regen)
- **Suggested Fix**: Live-regen the Oblivion TSV (`--game oblivion --regen`). Put a single newest block at line 1 that cites #5123/#5189 for the directional row and #5189 for the −1 entity. Correct the 09-30 block's "stays 1" sentence, or move it below.

### RT-3: `audit-runtime/SKILL.md` presents three fixed issues as open and omits the newest playable-slice / milestone smokes from its gate matrix
- **Severity**: LOW
- **Dimension**: Harness integrity (doc rot)
- **Location**: `.claude/commands/audit-runtime/SKILL.md:20`, `:24`, `:36`, `:108`
- **Status**: NEW
- **Description**:
  - **:108 (Oblivion neutralisation).** Says `scripts/check-playable-smoke-contracts.sh` does not neutralise `BYROREDUX_OBLIVION_DATA`. `e04fa1ef6` (Fix #5118) now derives the neutralisation list from each fixture's `FIXTURE_DATA_ENV`, and `oblivion.env` declares `BYROREDUX_OBLIVION_DATA`. The script also fails if any harness `*_DATA` variable is left un-neutralised.
  - **:24 (#4987).** Calls #4987 "known-open". It was closed by `6d5d8fa5f` ("make vulkan-validation prove it selected a device").
  - **:36 (#5131–#5133).** Says the TSVs carry "still-unattributed" moves of #5131–#5133. `6fcf313e2` attributed #5131 and #5133, and both are closed. Only #5132 (the FO4 spot-light schema row) is open.
  - **:20 (gate matrix).**
    - The FO3 `FIXTURE_GATES` example ("p0,p5,p2") omits `p5-f5-f9-quicksave` (the fixture declares `p0-door-interaction p5-save-restart p5-f5-f9-quicksave p2-melee-core`).
    - The playable-slice row omits `p4-quest-route`, `p5-door-transition`, `p5-f5-f9-quicksave`, `p5-quest-persistence`, `p5-soak` and `p6-loading-model`, all of which CLAUDE.md lists as gates.
    - The milestone row omits `m48-5-fnv-hud.sh`.
- **Evidence**: `grep FIXTURE_DATA_ENV docs/smoke-tests/fixtures/*.env`; `gh issue view 4987/5131/5133` (CLOSED), `gh issue view 5132` (OPEN); `ls docs/smoke-tests/`.
- **Impact**: An auditor could file a duplicate for the Oblivion contract, under-run the bless-a-build gate set (the skill says to run the gates you have data for), or treat the vulkan-validation lane as inert.
- **Related**: #5118, #4987, #5131, #5133, #5132
- **Suggested Fix**: Delete the :108 bullet. Replace the #4987 parenthetical with a pointer to `6d5d8fa5f`. Rewrite :36 as "#5132 open (spot row)". Extend the :20 gate lists from `docs/smoke-tests/` and the fixtures, then run `.claude/commands/_audit-validate.sh`.

## Observations (advisory; not findings)

- **FNV `frame_max_ms=57872.79` is a cold-driver-cache pipeline compile, not the scene.**
  - **Where the time went.** The bench line's `pipeline_compile_ms=241.10` is a per-frame mean over n=240, which totals 57 864 ms ≈ the frame max. `draw_ms=246.03` against `wall_ms=7.51` shows the same.
  - **Why only FNV.** FNV was the first launch of the freshly built binary. Every later capture (FO3, Oblivion, Skyrim, FO4) reports `pipeline_compile_ms` ≤ 0.28 and frame max ≤ 148 ms.
  - **How the stall enters the samples.** Frame 1's CPU sample (`atw_pre_t0`, `byroredux/src/app_events.rs:1074-1078`) includes its `draw_frame`. `bench_start` is armed only *after* frame 1 draws (`byroredux/src/app_frame.rs:781-783`). So a first-frame stall lands in p50/p95/max, `frame_max_over_p95` (7115×) and the per-bucket means, but not in `wall_fps`/`wall_ms`.
  - **Comment drift.** `bench_frame_max_over_p95`'s doc comment (`byroredux/src/main.rs:122-131`, #3559) attributes FNV's huge max to cell load on the render thread. Today's evidence puts it in `pipeline_compile`.
  - **Who consumes the token.** `scripts/bench-variability-envelope.sh` and `docs/smoke-tests/m-exteriors.sh` read `frame_max*`. After a shader-changing build, either of them sees a phantom hitch on its first run.
  - **Not filed.** The frame rows are advisory under the skill. If this should become a gate, exclude frame 1 from the distribution or run a throwaway warm-up capture first.
- **Debug-server client cap reached during FNV's cold load.** The FNV engine log records 6 `Debug client … refused — 8 concurrent connections already active (#3449)` lines (0 on the other four games). `capture.sh`'s 1 Hz `ping` loop (2 s client timeout) leaves server-side client threads waiting on the render-thread-drained queue while the cell loads. The capture recovered and the cross-check passed. This only matters for a load longer than the ping window; noted for `/audit-tooling`.
- **Spot lights.** FO4 still reports 5 `kind=Spot` emitters that no TSV row counts. This is Existing: #5132.

## Not run

- **Starfield** `citycydoniamainlevel`: no baseline, and memory headroom as above. Use `--sf-smoke` for coverage.
- Playable-slice, milestone and renderer-correctness gates: not part of this pass. Nothing here should be read as a pass for them.

## Cleanup

`/tmp/audit/runtime` was removed after the report was written. `pgrep -x byroredux` / `pgrep -x byro-dbg` were both empty. The worktree `../gamebyro-redux-rtaudit` was removed. `target/release/byroredux` was rebuilt at HEAD `2c36c29d8` (from the worktree path; a build from the main checkout re-fingerprints the workspace crates).

Suggested next step: `/audit-publish docs/audits/AUDIT_RUNTIME_2026-10-03.md`.
