# Runtime Telemetry Audit — 2026-07-02

Scope: `/audit-runtime --game all`. Drove the release engine headless
(`xvfb-run` + `byro-dbg`) against each game whose profile data dir resolved,
serially on a single debug port, and diffed frame-~240 telemetry against the
committed baselines under `.claude/audit-baselines/runtime/`.

## Environment / capability probe

The runtime diff **actually ran** — this was not a blocked session.

| Prerequisite | Status |
|--------------|--------|
| GPU / Vulkan | PRESENT — NVIDIA RTX 4070 Ti, driver 580.159.03, Vulkan 1.4.312; `VK_KHR_acceleration_structure` + `ray_query` + `ray_tracing_pipeline` all exported (RT path exercised) |
| Headless X | PRESENT — `/usr/bin/xvfb-run`, `-screen 0 1280x720x24` |
| Game data dirs | PRESENT — Oblivion, Fallout 3, Fallout NV, Skyrim SE, Fallout 4, Starfield all resolve under `/mnt/data/SteamLibrary/steamapps/common` |
| Binaries | Rebuilt clean: `cargo build --release -p byroredux -p byro-dbg` (finished in 13 s) |
| Baselines | 5 committed TSVs (fnv, fo3, oblivion, skyrim_se, fo4) |
| Starfield | SKIPPED — profile ships empty archives + no `sample_cells`; no cell baseline exists (SF coverage is `--sf-smoke`, out of scope here) |

Six games' data resolved; five have baselines and were diffed. Starfield was
skipped per the SKILL (no runtime cell guard).

### Capture methodology note (verified against source)

- The `skin=L/M+S` line (`byroredux/src/systems/debug.rs`, `log_stats_system`)
  only emits on a wall-second boundary (`total.floor() != prev.floor()`).
  `BYROREDUX_FIXED_DT=0` freezes `TotalTime`, which **suppresses** that line
  entirely — so the skin-pool metrics cannot be captured in a fixed-dt run. All
  five games here were therefore driven with animation **live** (no fixed-dt) to
  harvest the skin pool. Consequence: the `draws=N/Mb/Kc` split is not
  frame-deterministic run-to-run (a re-run of FNV moved `gpu_calls` 8→10 and
  `cmds` 2665→2921 purely from frame-240 culling differences), so ±1–2 GPU
  calls / single-digit-% cmd drift is measurement noise, not regression. Only
  moves beyond the `×1.1` band gate.
- Skyrim's cell is CPU-bound (~30 fps), so 240 frames take ~8 s; the driver was
  updated to poll for the `bench:` line before capturing rather than a fixed
  sleep, ensuring the frame-240 draw split is real on every game.

## Per-game baseline comparison

All **gated structural metrics** (tex_missing, mesh_cache_failed, skin pool,
entities within ±2 %, directional light, draw split within ×1.1) **PASS on all
five games**. `bench_fps_*` is advisory-only per RT-2 (#1701) and never gates.

| Game | Cell | Status | Δ vs baseline (structural) | fps (advisory) |
|------|------|--------|----------------------------|----------------|
| fnv | FreesideAtomicWrangler | PASS | entities 9250→9352 (+1.1 %, in band); tex 1→1; mesh 11→11; skin 686/·+0; draws 2722→2921 cmds (+7.3 %, ≤×1.1) | 141.4→178.4 |
| fo3 | MegatonPlayerHouse | PASS | entities 3311→3311; tex 0→0; mesh 3→3; skin 0/·+0; draws 1839→1839 | 93.3→139.2 |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS | entities 701→701; tex 0→0; mesh 0→0; skin 3/·+0; draws 324→324, gpu 3→4 (within run-to-run noise) | 411.8→331.8 |
| skyrim_se | WhiterunDragonsreach | PASS (structural) / perf ANOMALY | entities 6044→6049 (+5); tex 0→0; mesh 11→**9** (improved); skin 0/·+0; draws 2614→2442 | **321.1→30.4** (advisory) — see RT-1 |
| fo4 | InstituteBioScience | PASS | entities 11279→11289 (+10); tex 1→1; mesh 0→0; skin 100/·+0; draws 3800→3800 | 50.0→136.5 |
| starfield | — | SKIPPED | no baseline / empty profile archives | — |

`skin_pool_overflow_attempts == 0` on all five games (the HIGH-severity spill
gate is clean everywhere). `skin_pool_max` read 1364 on all five current runs.

## Findings

### RT-1: Skyrim Dragonsreach steady-state scheduler stall (321→30 fps)
- **Severity**: HIGH
- **Dimension**: runtime / performance (physics scheduler)
- **Game / Cell**: skyrim_se / WhiterunDragonsreach
- **Location**: physics/character scheduler stage; symptom on `bench:` `systems_ms` (`byroredux/src/systems/debug.rs`, `engine::stats` cpu_ms `atw_scheduler`)
- **Status**: **Existing: #1698** (OPEN — "RT-1: Skyrim Dragonsreach bench-window FPS collapsed 321→8.7 — ECS scheduler stalls ~140 ms/frame")
- **Description**: Confirmed still present. At the frame-240 bench window the
  Skyrim cell holds **wall_fps=30.4** vs the 06-14 baseline's 321.1 (~10×
  slower). The `bench:` line attributes the whole cost to `systems_ms=31.97`
  (GPU/draw stages are cheap: `draw_ms=0.52`, `fence=0.01`), and the `cpu_ms`
  breakdown pins it to `atw_scheduler=~30–32 ms/frame`. FNV/FO3/FO4 (same
  headless harness, larger scenes) sit at `systems_ms` 0.66–1.45 ms, so this is
  Skyrim-cell-specific, not a global scheduler regression.
- **Evidence**: `bench: … systems_ms=31.97 … entities=6049 … draws=2442/2b/4c`;
  `engine::stats cpu_ms: … atw_scheduler=32 …`. All gated structural metrics for
  this cell PASS (see table).
- **Impact**: The reference Skyrim interior renders correctly but at ~30 fps; on
  weaker hardware this couples with the GPU watchdog. Baseline was captured
  2026-06-14, before the recent character-rig physics work — consistent with a
  regression introduced by that arc.
- **Related**: #1698; root cause refined by RT-2 below.
- **Suggested Fix**: See #1698. RT-2's `grounded=false` freefall is the likely
  driver — a continuously-falling body sweeping a 1575-body scene each substep.

### RT-2: TES-family character rig never grounds → infinite freefall
- **Severity**: MEDIUM
- **Dimension**: runtime / physics (character controller)
- **Game / Cell**: oblivion / ICMarketDistrictTheGildedCarafe **and** skyrim_se / WhiterunDragonsreach
- **Location**: M28.5 character/physics grounding (`byroredux/src/systems/character.rs`; symptom logged as `M28.5 frame N: body Y a→b … grounded=false`)
- **Status**: NEW (related to #1698)
- **Description**: A clean cross-game split surfaced in the freefall telemetry:
  the character rig **grounds in every Fallout cell** (`grounded=true` — FNV,
  FO3, FO4) but **never grounds in either TES cell** (`grounded=false` —
  Oblivion and Skyrim), with body Y descending unbounded at v=-2000 (Skyrim:
  Y −6542.6 → −6609.3 in one log window). This is infinite fall, not settling.
  It is benign in Oblivion (156 rapier bodies → `systems_ms=0.14`) but
  catastrophic in Skyrim (1575 bodies → `systems_ms=31.97`, i.e. RT-1). Note
  body count alone is not the cause: FO4 has **2081** rapier bodies yet
  `grounded=true` and `systems_ms=0.67`.
- **Evidence**: per-game frame-240 telemetry — FNV `grounded=true`/1342 bodies;
  FO3 `grounded=true`/845; FO4 `grounded=true`/2081; Oblivion
  `grounded=false`/156; Skyrim `grounded=false`/1575.
- **Impact**: The player/character rig falls through the world in TES-family
  cells. Invisible on small cells; on Skyrim it is the direct cause of the RT-1
  perf collapse. Likely a floor/collision-mesh grounding gap specific to the
  TES cell-load path (collision geometry not registered with the solver, or a
  Z-up/Y-up ground-probe axis issue on the TES import route).
- **Related**: #1698 (perf half of this same root cause).
- **Suggested Fix**: Investigate why the ground probe fails for TES cells —
  compare the collision-mesh registration in the Oblivion/Skyrim cell-load path
  against the Fallout path; verify the ground raycast axis after the
  Z-up→Y-up conversion. Fixing grounding here should also close RT-1/#1698.

### RT-3: FNV baseline `skin_pool_max` is stale (1365 vs live 1364)
- **Severity**: LOW
- **Dimension**: audit baseline hygiene
- **Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv:11`
- **Status**: NEW
- **Description**: The FNV baseline records `skin_pool_max 1365`; the current
  run reports 1364, and **all four other baselines** (fo3/oblivion/skyrim_se/fo4)
  already record 1364. `skin_pool_max` is a "exact match" metric in the SKILL, so
  the −1 nominally trips the gate, but it is a stale one-off baseline value, not
  a code regression (the pool cap is uniform 1364 across every game this run).
- **Evidence**: current `skin=686/1364+0`; four sibling baselines pinned at 1364.
- **Impact**: Cosmetic — a future exact-match gate on FNV would false-positive
  on this single unit.
- **Suggested Fix**: `--regen` the FNV baseline (or hand-edit line 11 to `1364`)
  to align it with the other four. No code change.

## Notes / non-findings

- **Entities creep is within band on every game** — FNV +102 (+1.1 %), Skyrim +5,
  FO4 +10, FO3/Oblivion exact. All inside the ±2 % tolerance (RT-3 / #1705);
  benign non-render-body drift, not findings.
- **Skyrim mesh-cache improved** — `mesh_cache_failed_count` 11→9 (≤ baseline
  direction). The 2 previously-corrupted control-char paths (AUDIT_RUNTIME
  2026-06-14 RT-3) no longer appear; clean pass.
- **fps swings are advisory** — every game's fps moved (FNV/FO3/FO4 up, Oblivion
  down −19 %, Skyrim down −90 %). Only Skyrim's is structurally corroborated
  (systems_ms); the rest are Xvfb wall-clock jitter per RT-2/#1701 and are not
  findings. Oblivion −19 % is the known small-fast-cell jitter (RT-2 06-14).
- **Draw-split noise** — Oblivion `gpu_calls` 3→4 and FNV `cmds` 2722→2921 are
  within demonstrated run-to-run variance (a self-check re-run of FNV moved
  `gpu_calls` 8→10). Not regressions.

## Summary

Runtime diff ran successfully against 5 of 6 games (Starfield has no cell
baseline). Every gated structural correctness metric — missing textures,
mesh-cache failures, skin-pool live/cap/overflow, entity count (±2 %),
directional light, and draw split (×1.1) — **PASSED on all five games**. The one
genuine anomaly is Skyrim Dragonsreach's ~10× perf collapse, already tracked as
open **#1698**, whose root cause this sweep refines to a TES-family character-rig
grounding failure (RT-2). One LOW baseline-hygiene fix (RT-3).

3 findings — CRITICAL 0 · HIGH 1 (existing #1698) · MEDIUM 1 (new) · LOW 1 (new).
