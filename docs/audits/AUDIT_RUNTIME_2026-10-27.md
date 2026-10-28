# Runtime Telemetry Audit — 2026-10-27

**HEAD**: `fa6be4a6b` **plus the orchestrator's uncommitted one-line EXT-D6-2026-10-27-01 fix** in `byroredux/src/cell_loader/terrain_lod.rs` (`vertex_bound(&vertices, |i| holes.get(i).copied().unwrap_or(false))` in `spawn_lod_block`). Every number below measures that tree, not bare HEAD. · **Baseline**: [AUDIT_RUNTIME_2026-10-03.md](AUDIT_RUNTIME_2026-10-03.md) (HEAD `2c36c29d8`; 1,694 first-parent commits since) · **Audited**: telemetry diff on all five baselined games, commit attribution of every out-of-band move, harness and baseline integrity, `capture.sh --self-test`, the byro-dbg harness contract, `m-exteriors` static and boundary on all five LAND-carrying games, P0 on all five fixtures, P2 on four, P5 save gates on FNV and FO3 · **Unchanged since baseline (skimmed)**: `byroredux/src/bench.rs` and `byroredux/tests/golden_frames.rs` (no commits since 2026-10-03)

The audit ran serially with no sub-agents. All five telemetry captures went through `capture.sh`. Smoke gates ran headless under `env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE xvfb-run -a` (the `SMOKE_HEADLESS` prefix). Before every launch, `pgrep -x byroredux` showed no engine and port 9876 was free.

## Headline

1. **The EXT-D6-01 fix holds live.** Nothing panicked in terrain LOD:
   - `m-exteriors` static: 5/5 PASS. LOD rings spawned 88 / 2 / 577 / 131 / 88 blocks (FNV / FO3 / Oblivion / Skyrim / FO4).
   - `m-exteriors` boundary (three crossings per game): 8 to 13 LOD-ring updates per profile, no panic.
   - P0 door transitions into all five exteriors (WastelandNV, MegatonWorld, Commonwealth, Tamriel, WhiterunWorld): all PASS.
   - Without the fix, all of these would hit the index-out-of-bounds.
2. **No gating regression came from a bug.** Three games carry `entities_total` outside ±2 %, and FO4's draw split moved. Each move traces to an intended commit that landed without the same-commit TSV refresh the baselines README requires (RT-1..RT-3). No TSV has been refreshed since 2026-10-07, while ~1,700 engine commits landed. All five baselines need one `--regen` run.
3. **Two declared gates are red on FO3:**
   - `m-exteriors fo3 boundary` fails its terrain-seam check (RT-4). It already failed on a 2026-10-12 build, and has grown since.
   - `p5-f5-f9-quicksave fo3` fails: the restored body settles 1.63 units below the quicksaved pose (RT-5).
4. **A new live ECS hazard:** a `QuestStageState` recursive read on FNV and FO4 (RT-6). It cannot be located, because the #3249 call-site capture has never worked: every warning names `lock_tracker.rs:172:30`.
5. **Confirmed live:** SAFE-2026-10-27-01, REN-D2-01 and ECS-2026-10-27-01 (details in §Confirmed live).
6. **Starfield: NOT RUN.** There is no baseline, and only ~7 GB of RAM was available.

## Per-game status table

Every capture reported:
- `mode=renderer-static`, with pose rows exact on all five games.
- `upscaler=fsr3/quality`, render 853x480, output 1280x720. This is the engine default, not a harness fault.
- An entities cross-check (`stats` vs `bench:`) with delta 0.

| Game | Cell | Status | Δ vs baseline (gating first; advisory last) |
|------|------|--------|---------------------------------------------|
| fnv | FreesideAtomicWrangler | PASS | entities 6985→6985; tex_missing_base_color 1→1 (`textures/grey.bmp`); mesh 0→0; lights 30 pt / 0 spot / 0 dir exact; skin max/overflow 1364/0; draws 2069/**503b**/**118c** raster 1284 (baseline 500b/120c: batches +3 within ×1.1, gpu_calls improved; RT-7); raster < 3000; advisory: skin_live 106→106, fps 133.5→151.9, frame p50/p95/max 6.63/8.13/165.78 ms, all_slots 1→1 |
| fo3 | MegatonPlayerHouse | PASS | entities 3626→3626; tex/mesh 0/0; lights 11/0/0 exact; skin 1364/0; draws **1515**/**516b**/**62c** raster **1289** (baseline 1514/511b/65c raster 1285: all within ×1.1; RT-7); advisory: skin_live 3→3, fps 87.3→91.0, frame 10.98/11.64/60.05 ms, all_slots 0→0 |
| oblivion | ICMarketDistrictTheGildedCarafe | **REGRESSION (attributed, RT-2)** | **entities 928→969 (+4.4 %)**; tex/mesh 0/0; lights 8/0/0 exact; skin 1364/0; draws **336**/18b/2c raster 20 (cmds +1); advisory: skin_live 9→10, fps 172.9→167.4, frame 6.18/8.78/11.91 ms, all_slots 0→0 |
| skyrim_se | WhiterunDragonsreach | **REGRESSION (attributed, RT-3)** | **entities 9499→9809 (+3.3 %)**; tex/mesh 0/0; lights 28/0/0 exact; skin 1364/0; draws **2495**/643b/34c raster 1786 (cmds +1); advisory: skin_live 122→123, fps 177.4→172.7, frame 5.13/6.53/68.33 ms, all_slots 2→2 (the control-char `textures/\bnor` plus `ore_ebony_e.dds`) |
| fo4 | InstituteBioScience | **REGRESSION (attributed, RT-1)** | **entities 16885→14061 (−16.7 %)**; tex_missing_base_color 1→1 (`textures/temp_v1_d.dds`); mesh 0→0; lights 680/5/0 exact; skin 1364/0; draws **cmds 3971→2471 and raster 3618→2247 improved** (now **below** the 3000 `DRAW_SORT_PARALLEL_THRESHOLD`; the baseline was above it), batches 700→710 and gpu_calls 108→110 (both within ×1.1); advisory: **skin_live 132→258**, fps 44.4→65.4, frame 13.97/14.89/220.52 ms, all_slots 2→2 |
| starfield | citycydoniamainlevel | NOT RUN | no baseline; free RAM ~7 GB available (skill requires ample headroom) |

## Gate matrix

| Gate | fnv | fo3 | oblivion | skyrim_se | fo4 |
|------|-----|-----|----------|-----------|-----|
| Telemetry diff (gating rows) | PASS | PASS | REGRESSION (RT-2, attributed) | REGRESSION (RT-3, attributed) | REGRESSION (RT-1, attributed) |
| `m-exteriors` static | PASS | PASS | PASS | PASS | PASS |
| `m-exteriors` boundary | PASS (the 10-26 exterior audit had it red; green now) | **FAIL** (RT-4) | PASS | PASS | PASS (documented #3306 seam crack only) |
| `p0-door-interaction` | PASS | PASS | PASS | PASS | PASS |
| `p1-character-traversal` | NOT RUN (headless-invalid, see below) | n/a (not declared) | n/a | NOT RUN (headless-invalid) | n/a |
| `p2-melee-core` | PASS | PASS (17/17 joints live) | n/a | PASS | FAIL: the 'starts clean' wait, documented as pre-existing in the skill (NPCs fight at load). Not a regression |
| `p5-save-restart` | PASS | not run | n/a | n/a | n/a |
| `p5-f5-f9-quicksave` | PASS (graceful quit exit 0) | **FAIL** (RT-5) | n/a | n/a | n/a |

Other gates in the skill's matrix:

| Gate | Result |
|------|--------|
| Harness self-test (`capture.sh --self-test`) | PASS |
| `scripts/check-byro-dbg-harness-contracts.sh` | PASS (66 scripts) |
| Baseline schema tests (`cargo test -p byroredux bench::`) | **not run** |
| Golden frames | **not run** |
| Closed-HIGH real-data pins (`scripts/run-real-data-pins.sh`) | **not run**: ~7 GB RAM available, and the plugin pins load up to ~20 GB per master |
| `scripts/check-playable-smoke-contracts.sh` | **not run** |
| Cornell / renderer-correctness gates | **not run** |
| HUD smokes, M41 / M42 / M43 / M47 / W1 / W2 / VATS smokes | **not run** |

These are reported as not run, never as passes.

**P1 headless note.** Run headless, P1 failed on both games:
- Skyrim: "timeout waiting for door prompt recovered after walking".
- FNV: "reverse-door approach did not reach z le -3380 (last z=-3275.19)".

`docs/smoke-tests/README.md` §Headless runs says P1's frame-counted input holds overshoot under Xvfb's ~31 FPS, and that P1 must run on a display. These failures therefore say nothing about the engine. I did not open a window on the user's desktop, so P1 counts as NOT RUN.

## Attribution (bisect)

The bisect built each commit in a detached worktree (`/mnt/data/src/gamebyro-redux-rtaudit`). It used the repo's `target/` under a separate cargo profile, `rtbisect` (`--config 'profile.rtbisect.inherits="release"'`), and swapped each binary into `target/release/byroredux` for `capture.sh`. Afterwards the HEAD binary was restored, its hash verified, and HEAD rebuilt (see §Process note).

Captures per commit (index = position in `git rev-list --first-parent --reverse 2c36c29d8..HEAD`):

| idx | commit | fo4 entities / draws / skin_live | oblivion | skyrim_se |
|-----|--------|----------------------------------|----------|-----------|
| base | TSV | 16885 / 3971/700b/108c r3618 / 132 | 928 / 335 / 9 | 9499 / 2494 / 122 |
| 287 | `5a68e983e` | 17266 / 4002/719b/110c r3649 / 163 | 928 / 335 / 9 | 9505 |
| 288 | `777351ec1` **#5484** | **14237 / 2376/690b/110c r2152** / 163 | 928 | 9505 |
| 290 | `0c4d4af08` | 14237 | 928 / 335 / 9 | 9505 |
| 291 | `1b45763fa` **#5487** | 14237 | **966 / 336 / 10** | 9505 |
| 615 | `4a9341c3d` | 14776 / 2442/703b r2218 / 229 | 969 | 9508 |
| 620 | `dbc80585f` | — | — | 9508 |
| 621 | `a649b55b4` **M47.2 defaultSetStageTrig** | — | — | **9835** |
| 624/625 | `16c9bd0b2` / `eeb6793b7` | — | — | 9835 |
| 705 | `342140b65` | — | — | 9835 |
| 795/796 | `74a05df6e` / `fa1473e25` | — | — | 9847 |
| 850/851 | `0f2658642` / `e32304ec9` | — | — | 9847 |
| 922 | `c1983a81d` | 15165 / 2460/719b/111c r2236 / 247 | 969 | 9847 |
| 1229 | `011ad0c07` (= `22ff0e12e^`) | 15049 / 2471/710b/110c r2247 / 258 | 969 | 9863 |
| 1230 | `22ff0e12e` #6077 | 15057 (no material change) | 969 | 9864 |
| HEAD | `fa6be4a6b`+fix | 14061 / 2471/710b/110c r2247 / 258 | 969 / 336 / 10 | 9809 / 2495 / 123 |

(Index 660 `61ad95cf3` failed the harness's own entities cross-check, `stats=12784` vs `bench=11988`: a transient mid-M47.2 state that spawned objects over time. The capture was discarded, as the harness requires.)

FNV and FO3 are row-identical to HEAD at idx 1229 (`011ad0c07`): their small draw-row drift (RT-7) predates 2026-10-20.

## Findings

### RT-1: `entities_total` fell 16.7 % on fo4 InstituteBioScience, and the draw split improved past the parallel-sort threshold; no TSV refresh came with it
- **Severity**: MEDIUM (`entities_total` outside ±2 %; the improved draw rows also leave a loose `≤` gate)
- **Dimension**: Telemetry diff / baseline integrity
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` rows `entities_total`, `bench_draws_cmds`, `bench_draws_raster_cmds`, `bench_draws_batches`, `bench_draws_gpu_calls`, `skin_pool_live`
- **Status**: NEW
- **Game / Cell**: fo4 / InstituteBioScience
- **Baseline**: entities 16885; draws 3971/700b/108c, raster 3618; skin_live 132
- **Current**: entities 14061; draws 2471/710b/110c, raster 2247; skin_live 258
- **Description**: The dominant step is `777351ec1` (Fix #5484, "read the precombine skip set from XCRI pairs, not XPRI"), measured as a one-commit A/B:
  - entities 17266→14237
  - cmds 4002→2376
  - raster 3649→2152
  - batches 719→690

  More placed references are now absorbed by their precombine, so their duplicate draws go away. That is an intended correctness fix. Around it, many small commits moved entities in both directions:
  - 16885→17266 before idx 287
  - 14237→14776→15165 through 10-14 (M47.2)
  - 15049→15057 at #6077
  - 15057→14061 after 2026-10-20 (not bisected further)

  `skin_pool_live` doubled (132→163 by idx 287, →229 by 10-12, →258 by 10-20). It is advisory, but it is a population change nobody recorded. The raster prefix moved from above to below `DRAW_SORT_PARALLEL_THRESHOLD`, so this cell no longer exercises the parallel draw-sort branch. That was the reason the 2026-09-30 regen recorded it.
- **Evidence**: bisect table above. HEAD log: `502 REFRs skipped via FO4 PreCombined absorption`, `PreCombined: 13 hashes — 1722 entities spawned`, `21 NPCs spawned via pre-baked-FaceGen path`, `8 REFRs withheld by their XESP enable parent's state (#6077)`.
- **Impact**: The gate is red on an improvement, so the next real regression hides inside it. With the cmds/raster rows loose, the old 3971/3618 values could come back and still read green. No 3000+-raster interior baseline now exercises the parallel draw-sort branch.
- **Related**: RT-2, RT-3, RT-7; prior instance AUDIT_RUNTIME_2026-10-03 RT-1 (#5235).
- **Suggested Fix**: Run `/audit-runtime --game all --regen` in one renderer-static run. Write one `# regenerated:` block per TSV naming `777351ec1` (#5484) as the fo4 draw step and the multi-commit entity drift. Decide whether another cell should carry the >3000-raster parallel-sort coverage.

### RT-2: `entities_total` rose 4.4 % on oblivion ICMarketDistrictTheGildedCarafe
- **Severity**: MEDIUM
- **Dimension**: Telemetry diff
- **Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv` rows `entities_total`, `bench_draws_cmds` (and advisory `skin_pool_live`)
- **Status**: NEW
- **Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe
- **Baseline**: entities 928; cmds 335; skin_live 9
- **Current**: entities 969; cmds 336; skin_live 10
- **Description**: `1b45763fa` (Fix #5487, "give Oblivion its own NPC body table and race body skins") moves 928→966 entities, 335→336 cmds and 9→10 skin_live (one-commit A/B, idx 290→291). It adds the body pieces that the shared Fallout table had dropped. A further +3 landed between idx 307 and 615. This is an intended fix with no TSV refresh.
- **Evidence**: bisect rows 290/291. HEAD log: `Player body: assembled from NPC_ 00000007 (Oblivion) … meshes=20 equipment_parts=4`.
- **Impact**: The gate reads red on a fix.
- **Related**: RT-1.
- **Suggested Fix**: Covered by the RT-1 regen; name `1b45763fa` in the header block.

### RT-3: `entities_total` rose 3.3 % on skyrim_se WhiterunDragonsreach
- **Severity**: MEDIUM
- **Dimension**: Telemetry diff
- **Location**: `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` rows `entities_total`, `bench_draws_cmds`
- **Status**: NEW
- **Game / Cell**: skyrim_se / WhiterunDragonsreach
- **Baseline**: entities 9499; cmds 2494; skin_live 122
- **Current**: entities 9809; cmds 2495; skin_live 123
- **Description**: The main step is `a649b55b4` ("M47.2: defaultSetStageTrigScript recognizer; decompiled scripts keep their variable defaults"): 9508→9835 between idx 620 and 621. Smaller later steps:
  - 9835→9847 between idx 705 and 795
  - →9863 by 10-20
  - →9809 after 10-20 (not bisected)

  The cmds +1 / skin_live +1 step landed before idx 307. I did not establish which entities `a649b55b4` adds (+327). HEAD's log shows `M47.2 scripts: 12 REFRs recognized, 2 trigger volumes spawned` and three `rejected provider handler in .pex 'defaultsetStageTrigSCRIPT' … UnsupportedStatement` warnings. The recognizer is live in this cell, but a two-volume trigger does not obviously account for 327 entities.
- **Evidence**: bisect rows 620/621/624/705/795/850/1229.
- **Impact**: The gate reads red. Until someone attributes the +327 to specific entities, it is unproven that they are intended.
- **Related**: RT-1.
- **Suggested Fix**: Before the regen, diff `entities` by name or kind at `dbc80585f` vs `a649b55b4` (`byro-dbg entities`) to name the 327. Then regen with that attribution.

### RT-4: `m-exteriors fo3 boundary` hard-fails the terrain-seam gate on MegatonWorld
- **Severity**: MEDIUM (gate red; visible seam cracks along the traversal, owner `/audit-exterior`)
- **Dimension**: Gate matrix / exterior
- **Location**: gate `docs/smoke-tests/m-exteriors.sh` (fo3 profile: `--grid -1,-7 --wrld MegatonWorld`, `grid-cross`). Producer: `terrain.seams` in `byroredux/src/commands/world_info.rs`.
- **Status**: NEW. Not in the open-issue snapshot or the 2026-10-27 static reports. It is older than this window: it already fails on `a649b55b4` (2026-10-12).
- **Game / Cell**: fo3 / MegatonWorld (-1,-7) → three eastward crossings
- **Baseline**: n/a (no exterior TSV). At `a649b55b4`: `pairs_checked=3 pairs_dirty=3 height_mismatch_vertices=99`.
- **Current**: `terrain-seams: sampled=1 pairs_checked=6 pairs_dirty=5 height_mismatch_vertices=160 normal_mismatch_pairs=6 verdict=FAIL`. The boundary frame is a flat fog-green image (mean 0.143, sd 0.0063, 111 draws).
- **Description**: In the FO3 boundary run, 5 of 6 adjacent loaded cell pairs disagree in edge height. MegatonWorld carries parent flags 0xFFC5, so Use Land Data is set. The loader logs `child worldspaces: 107 LAND-less cells take their parent's landscape`, so only LAND-less cells borrow the Wasteland landscape. Cells with their own LAND keep it. If MegatonWorld's own LAND and Wasteland's disagree at shared edges, the mix produces exactly this kind of seam. That mechanism is unverified: the `terrain-seams:` line carries no pair coordinates. FNV, Oblivion and Skyrim pass the same gate. FO4 passes only through its single documented #3306 exemption.
- **Evidence**: `/tmp/audit/runtime/ext-boundary/fo3/debug.log`; HEAD summary row `fo3 FAIL 3558 111 …`; the 10-12 build re-run under the same script.
- **Impact**: Visible terrain cracks at FO3 child-worldspace seams. The FO3 boundary gate is red, so it cannot catch a new seam regression.
- **Related**: FO4-2026-10-27-01 (Use Land Data child worldspaces, FO4 leg); AUDIT_EXTERIOR_2026-10-26 EXT-D1 (FO3/FNV Use-Land-Data towns); EXT-D7-2026-10-27-01 (the seam line carries counts, not cells).
- **Suggested Fix**: Make `terrain.seams` print the dirty pair coordinates. Then compare MegatonWorld's authored LAND against Wasteland's at those edges, to decide whether Use Land Data should source all cells from the parent rather than only LAND-less ones.

### RT-5: `p5-f5-f9-quicksave fo3` red — after F9 the body settles 1.63 units below the quicksaved pose
- **Severity**: MEDIUM
- **Dimension**: Gate matrix / save-load (owner `/audit-save`)
- **Location**: gate `docs/smoke-tests/p5-f5-f9-quicksave.sh:184-186` (`pose_close … 1`); restore `byroredux/src/save_io/live_load.rs`
- **Status**: NEW
- **Game / Cell**: fo3 / MegatonMoriartysSaloon (fixture)
- **Baseline**: n/a (FNV passes the same gate, including graceful quit exit 0)
- **Current**:
  - quicksaved `poseA` body=(1287.17, **-62.38**, 43.65) grounded=true
  - restored body=(1287.17, **-64.01**, 43.65) grounded=true
  - the engine logged `save load: restored player pose at (1287.2, -62.4, 43.6)`
- **Description**: The pose restore itself is exact; the body then settles 1.63 units. Everywhere else the gate measured, the saloon floor is at y = -64.01 (pose0 at x 1110, move2 at x 991). So at save time the body stood 1.63 units up on something that is not under it after the reload. That fits a clutter or dynamic collider whose save-time position is not restored. The supporting object is unidentified.
- **Evidence**: `/tmp/byro-p5-f5-f9.cUjIU3/{poseA,pose.restored,pose0,pose.move2}.status`, `session.stderr` (`applied 528 saved deltas across 459 form-id-matched entities`).
- **Impact**: A declared FO3 gate is red. If the cause is unsaved dynamic-object placement, a quickload does not fully restore the world the player saved in.
- **Related**: #5155 (closed, P5 soak grounding).
- **Suggested Fix**: At the save pose, run a `player.status`/physics probe of the support collider, and check whether its entity carries a saved transform delta.

### RT-6: A live `QuestStageState` recursive read on FNV and FO4 cannot be located: the #3249 call-site capture always reports `lock_tracker.rs:172:30`
- **Severity**: MEDIUM. The hazard itself is HIGH-class (std `RwLock` is writer-preferring on Linux, so a recursive read deadlocks behind a parked writer). I cannot cite its site from source, so I file the locating defect and record the live hazard as evidence.
- **Dimension**: ECS runtime hazards / diagnostics (owner `/audit-ecs`)
- **Location**: `crates/core/src/ecs/lock_tracker.rs:97-182`. `Location::caller()` at :172 is called inside the `LOCKS.with(|locks| …)` closure. The closure is not `#[track_caller]`, so it returns its own line.
- **Status**: Regression of #3249. Its fix `a0b1598dc` put `Location::caller()` inside the closure from the start, so the call site was never reported. #3249 is closed.
- **Description**: Every live recursive-read warning in this audit reads `acquired at crates/core/src/ecs/lock_tracker.rs:172:30`. The hazards observed:
  - `QuestStageState` on FNV GSProspectorSaloonInterior at the first game-loop frame.
  - `QuestStageState` on FO4 InstituteBioScience right after SCEN phase transitions (`DN054DragRaceScene`, `Companion_PrestonHero_Scene`).
  - `AnimationPlayer` on Oblivion Tamriel at bootstrap quest start. This one is ECS-2026-10-27-01's `apply_pending_group_swaps` → `sequence_player` re-query, so I attribute it to that finding.

  A nested `QuestStageState` read was fixed once before (`35f1cf051`, "Story Manager walk no longer nests QuestStageState reads"). Another nesting remains, and the static ECS and concurrency audits did not find it.
- **Evidence**: `ECS recursive-read hazard: a second byroredux_scripting::quest_stages::QuestStageState read guard is live on this thread, acquired at crates/core/src/ecs/lock_tracker.rs:172:30` in `/tmp/audit/runtime/fo4-InstituteBioScience.engine.log` and the FNV quicksave session log; the same text for `AnimationPlayer` in `ext-static/oblivion` and `ext-boundary/oblivion`.
- **Impact**: There is a live deadlock-class hazard in quest-stage code under the parallel scheduler. The one diagnostic built to locate such hazards cannot locate it.
- **Related**: ECS-2026-10-27-01 (live-confirmed by the Oblivion warning); #2386; #3249.
- **Suggested Fix**: Capture `let caller = std::panic::Location::caller();` at the top of `track_read`, outside the closure, and add a test that the warning names the caller's file. Then re-run the FNV / FO4 captures to locate the `QuestStageState` nesting.

### RT-7: Small fnv / fo3 draw-row drift within tolerance, unattributed
- **Severity**: LOW (within ±5 % on tolerance metrics)
- **Dimension**: Telemetry diff
- **Location**: `.claude/audit-baselines/runtime/{fnv-FreesideAtomicWrangler,fo3-MegatonPlayerHouse}.tsv` draw rows
- **Status**: NEW
- **Baseline / Current**:
  - fnv: 2069/500b/120c → 2069/503b/118c
  - fo3: 1514/511b/65c raster 1285 → 1515/516b/62c raster 1289
- **Description**: These were already present at `011ad0c07` (2026-10-20), byte-identical to HEAD, and were not bisected further. gpu_calls improved on both, so the loose `≤` gate lets the old values return.
- **Suggested Fix**: Fold into the RT-1 regen.

### RT-8: `capture.sh` teardown orphans one `Xvfb` server per capture
- **Severity**: LOW (harness hygiene; recurring since the 2026-10-08 suite, where 7 and then 21 orphans were killed by hand)
- **Dimension**: Harness integrity
- **Location**: `.claude/commands/audit-runtime/capture.sh:425-435` (`teardown`: engine TERM, `sleep 2`, KILL, then immediately `kill -TERM "${wrapper_pid}"`)
- **Status**: NEW (never filed; only noted in AUDIT_SUITE_SUMMARY_2026-10-08 / -10-12)
- **Description**: `/usr/bin/xvfb-run` is `/bin/sh` (dash), and its only cleanup is `trap clean_up EXIT`. A TERM that reaches the wrapper while it is reaping the engine aborts the cleanup, which prints `xvfb-run: error: problem while cleaning up temporary directory` and exits 5 before `kill "$XVFBPID"`. The server then reparents to `systemd --user`. This run orphaned 4 of 5 captures (`Xvfb :99`–`:102`, killed by PID).

  A minimal reproduction (`xvfb-run -a sleep 37`, TERM the child, TERM the wrapper at once) leaves `Xvfb` alive. Waiting 1 s before the wrapper TERM leaves none. Every engine log ends `Terminated` / `xvfb-run: error: problem while cleaning up temporary directory`.
- **Impact**: Every runtime audit leaks X servers (RAM and display numbers) until someone sweeps them. The harness's survivor sweep checks only `byroredux`, so it never notices.
- **Suggested Fix**: After killing the engine, `wait` for the wrapper with a bounded timeout and TERM it only if it is still alive. Extend `sweep_survivors` to the `Xvfb` whose `-auth` names this run's `xvfb-run.*` dir, and add the case to `--self-test`.

### RT-9: SDK faction catalog rejects vanilla Oblivion's −200 XNAM edges
- **Severity**: LOW
- **Dimension**: Runtime log review (owner `/audit-tooling` for the SDK)
- **Location**: `crates/sdk/src/relationships.rs:57` (`if !(-100..=100).contains(&modifier)`); caller `byroredux/src/cell_loader/load_order.rs:277-290`
- **Status**: NEW
- **Description**: Every Oblivion boot logs four `SDK faction relationship … rejected: faction relationship modifier -200 is outside -100..=100`. A census of `Oblivion.esm` FACT XNAM finds exactly four edges with |modifier| > 100, all Shivering Isles crystal-war hostility:
  - `SE08OrderFaction` (0005282C) → `SE08GoldenSaintFaction` / `SE08DarkSeducerFaction`, −200
  - both of those → `SEOrderFaction` (000134D4), −200

  The ±100 bound has no cited source. The CS wiki documents no range, and the vanilla master authors −200. The catalog drops these four edges and marks itself `truncated`. Oblivion XNAM carries no combat reaction, so `faction_hostility` does not read the dropped modifier today. WASM mods do: the catalog is exposed to them (`crates/mod-runtime/src/runtime/host/faction_relationships.rs`).
- **Impact**: Mods see the vanilla SI relationship graph minus its strongest hostility edges, and the "truncated" flag reports a content error that does not exist.
- **Suggested Fix**: Widen the bound to what the format carries (i32), or cite a source for a clamp. Pin the four vanilla edges in a real-data test.

### RT-10: The baselines README schema block lacks `light_count_spot`, yet still says it "mirrors the committed TSVs exactly"
- **Severity**: LOW (doc-rot)
- **Dimension**: Harness and baseline integrity
- **Location**: `.claude/audit-baselines/runtime/README.md:22-46`
- **Status**: NEW (stale since `aa075ed35`, #5132, which added the row to all five TSVs, `REQUIRED_METRICS` and the skill, but not to the README)
- **Suggested Fix**: Add `light_count_spot` to the sample block.

## Confirmed live (static findings from this suite)

- **SAFE-2026-10-27-01**: every graceful quit (FNV and FO3 `p5-f5-f9` sessions) logs `ERROR … GPU allocator has 3 outstanding references — leaking allocator + device + surface + instance` plus two `GpuBuffer dropped without destroy()`. Exactly the 3-reference count the static finding predicts. The release build exits 0; the leak is real.
- **REN-D2-01**: FNV FreesideAtomicWrangler logs 88 × `moved-surface set over capacity: 203 movers, kept 64 (#6250)` within one 240-frame **renderer-static** capture (dt = 0, nothing moves). Every skinned part counts as a mover, so the cap overflows on a static scene. FO4 and Skyrim log it once each.
- **ECS-2026-10-27-01**: the `AnimationPlayer` recursive-read hazard fires at Oblivion bootstrap quest start (`Started 208 bootstrap quest(s)`), in both exterior runs.
- **EXT-D6-2026-10-27-01**: the orchestrator's fix is complete for every path these gates reach (§Headline 1). Not a confirmation of the bug: the bug is absent because of the fix.
- **Not observable with these harnesses**:
  - LC-2026-10-27-D2-01 (doors loading Open). P0 passes on all five games, because the gate checks the activation chain, not the door pose.
  - PERF-D5-01 (no submerged camera in the telemetry set).

## Observations (not findings)

- **Interior sky flag.** SkyParamsRes `is_exterior = true` in all five interior `light.dump`s is the documented #4915 state (`byroredux/src/commands/env_health_tests.rs:166-178`).
- **Exterior static gate warnings.** fo4 reports 8 unique missing textures (`diamondcorrugated01_m.dds` emissive ×42, …) and 1 failed NIF (`meshes\architecture\buildings\decokit\occlusionboxtest.nif`). Skyrim's warning is the known `textures/\bnor`; Oblivion's is `textures/plants/bwcattail02.dds` base_color. None of these are baselined.
- **Process note: shared-`target/` worktree builds can produce chimera binaries.** Building an older commit from a worktree into the repo's `target/` **release** profile reuses HEAD's crate artifacts whenever the worktree file's mtime is older than the artifact. Workspace-member fingerprints are keyed by workspace-relative path. My first attempts failed exactly this way: the 10-14 `byro-dbg` compiled against HEAD's `byroredux-debug-protocol`. They also replaced HEAD's release artifacts for several crates with older versions.

Touching only the crates whose fingerprint directories had changed was not enough. The next HEAD release build failed to compile: `byroredux_hkx::decode_animation`, `byroredux_sfmaterial::capture_settings` / `FieldSource` and `TemplateResolver::canonicalize` were all missing, because stale rlibs had been kept. I then touched every tracked file under `crates/`, `byroredux/` and `tools/` in the main checkout. Only mtimes changed; `git status` was unchanged. After the rebuild, `target/release/byroredux` matches the pre-bisect HEAD build byte for byte (sha256 `54b25de5464f0b5c…`), and so does `byro-dbg` (`16f90f08ce8c3b30…`). The `rtbisect` profile directory was deleted.

The bisect results above are uncontaminated: they ran in the separate `rtbisect` profile. Every HEAD capture and gate in this report ran before the first worktree build.

The 2026-10-03 runtime audit used the same worktree-sharing-`target/` method and could have hit the same trap. A skill or suite rule should say: build worktrees into a separate cargo profile, or never share `target/`.

## Suggested next step

`/audit-publish docs/audits/AUDIT_RUNTIME_2026-10-27.md`
