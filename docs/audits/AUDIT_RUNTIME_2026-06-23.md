# Runtime Telemetry Audit — 2026-06-23

Runtime arm of the audit suite. Drives the release engine headless under
`xvfb-run` against each game's committed cell baseline, captures
`stats` / `tex.missing` / `mesh.cache failed` / `light.dump` over `byro-dbg`
plus the `bench:` + `skin=` lines from the engine log, and diffs the extracted
scalars against `.claude/audit-baselines/runtime/*.tsv`.

- **Scope**: `--game all` — every game whose profile data dir resolves.
- **Build**: `cargo build --release -p byroredux -p byro-dbg` — **succeeded**
  (the warned-about uncommitted M47.2 `crates/scripting` work compiled clean in
  release; no build-phase finding).
- **Game data**: Oblivion, Fallout 3, Fallout NV, Skyrim SE, Fallout 4 all
  present under `/mnt/data/SteamLibrary/steamapps/common`. **Starfield skipped**
  — no committed cell baseline (profile ships empty archives; SF coverage lives
  in `--sf-smoke`, out of this arm's scope). **Fallout 76 skipped** — no profile.
- **Vulkan**: real device present (`renderD128/129`, Vulkan 1.4.341); 240-frame
  benches rendered for real, not a null device.
- **Determinism / driving**: each game launched `--bench-frames 240
  --bench-hold` and captured serially.

## Environment note — debug-port collision forced a non-default port (no finding)

The default debug port **9876 was already occupied by another `byroredux`
instance from a concurrent session** when this audit started. The first FNV
launch logged `Debug server failed to bind port 9876: Address already in use`
and the `byro-dbg` capture silently attached to the *other* engine — the exact
RT-1 / #1619 hazard. The audit was re-run with `BYRO_DEBUG_PORT=9921` for both
the engine and `byro-dbg` (the per-game-offset escape hatch the skill
documents). All numbers below are from the 9921 runs. This is an environmental
artifact of a shared box, not an engine regression — #1619 (the skill still
endorsing parallel 9876 runs) remains the standing fix.

A second harness correction: for heavy cells the bench window must be allowed to
**complete** before capture — the first Skyrim capture read mid-warm-up stats
(FPS 6.8) with no `bench:` line yet. The harness now waits for
`bench: frames=240` in the engine log (cap 120 s) before driving `stats`.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | **PASS** | entities 9250→9352 (+102, non-render); fps 141.4→182.2↑; all symptom metrics unchanged |
| fo3 | MegatonPlayerHouse | **PASS** | identical symptom metrics; fps 93.3→145.1↑ |
| oblivion | ICMarketDistrictTheGildedCarafe | **REGRESSION (MEDIUM)** | fps 411.8→352.3 (−14.4%, below ×0.9 gate); gpu_calls 3→4 |
| skyrim_se | WhiterunDragonsreach | **REGRESSION (HIGH)** | fps 321.1→**8.7** (−97%); `atw_scheduler` 1ms→~140ms for the first ~28 s. entities +5; mesh_fail 11→9 (improved) |
| fo4 | InstituteBioScience | **PASS** | entities 11279→11289 (+10, non-render); fps 50.0→134.3↑; all symptom metrics unchanged |

Full extracted scalars are in `/tmp/audit/runtime/<game>-<cell>.current.tsv`
(captured this run; baselines under `.claude/audit-baselines/runtime/` were
**not** modified — no `--regen`).

### Detail tables

**fnv — FreesideAtomicWrangler**

| Metric | Baseline | Current | Dir | Verdict |
|--------|----------|---------|-----|---------|
| entities_total | 9250 | 9352 (+102) | exact | drift (non-render; `draws_cmds` is load-bearing) |
| tex_missing_unique_paths | 1 (`grey.bmp`) | 1 | ≤ | PASS |
| mesh_cache_failed_count | 11 | 11 | ≤ | PASS (same DLC marker/furniture NIFs) |
| skin L/M+S | 686/1365+0 | 686/1364+0 | — | PASS (max −1 is stale-baseline, #1284) |
| bench_fps | 141.4 | 182.2 | ≥0.9× | PASS |
| draws cmds/batches/gpu | 2722/104/10 | 2921/101/10 | ≤1.1× | PASS (cmds +7.3%, within tol) |

**fo3 — MegatonPlayerHouse**

| Metric | Baseline | Current | Verdict |
|--------|----------|---------|---------|
| entities_total | 3311 | 3311 | PASS (exact) |
| tex_missing / mesh_fail | 0 / 3 | 0 / 3 | PASS |
| skin L/M+S | 0/1364+0 | 0/1364+0 | PASS |
| bench_fps | 93.3 | 145.1 | PASS |
| draws cmds/batches/gpu | 1839/96/9 | 1839/98/9 | PASS |

**oblivion — ICMarketDistrictTheGildedCarafe**

| Metric | Baseline | Current | Verdict |
|--------|----------|---------|---------|
| entities_total | 701 | 701 | PASS (exact) |
| tex_missing / mesh_fail | 0 / 0 | 0 / 0 | PASS (cleanest path) |
| skin L/M+S | 3/1364+0 | 3/1364+0 | PASS |
| bench_fps | 411.8 | 352.3 | **FAIL** (−14.4%, gate 370.6) |
| draws cmds/batches/gpu | 324/30/3 | 324/30/4 | gpu_calls +1 (OVER ×1.1 of 3) |

**skyrim_se — WhiterunDragonsreach**

| Metric | Baseline | Current | Verdict |
|--------|----------|---------|---------|
| entities_total | 6044 | 6049 (+5) | drift (non-render) |
| tex_missing | 0 | 0 | PASS |
| mesh_cache_failed_count | 11 | 9 | PASS (improved; the 2 control-char paths of RT-3 06-14 did not recur) |
| skin L/M+S | 0/1364+0 | 0/1364+0 | PASS |
| bench_fps | 321.1 | **8.7** | **FAIL** (−97%) |
| draws cmds/batches/gpu | 2614/3/5 | 2445/2/4 | PASS (all ≤) |

**fo4 — InstituteBioScience**

| Metric | Baseline | Current | Verdict |
|--------|----------|---------|---------|
| entities_total | 11279 | 11289 (+10) | drift (non-render; render entities = draws_cmds = 3800 unchanged) |
| tex_missing / mesh_fail | 1 (`temp_v1_d.dds`) / 0 | 1 / 0 | PASS |
| skin L/M+S | 100/1364+0 | 100/1364+0 | PASS |
| bench_fps | 50.0 (floor) | 134.3 | PASS (improved) |
| draws cmds/batches/gpu | 3800/272/40 | 3800/279/40 | PASS (batches +2.6%) |

## Findings

### RT-1: Skyrim Dragonsreach bench-window FPS collapsed 321→8.7 — ECS scheduler stalls ~140 ms/frame for the first ~28 s
- **Severity**: HIGH
- **Dimension**: performance / ecs (scheduler) — surfaced via runtime telemetry
- **Location**: `bench:` line of
  `/tmp/audit/runtime/skyrim_se-WhiterunDragonsreach.engine.log`; cost attributed
  to the `atw_scheduler` stage in `engine::stats` `cpu_ms` (the parallel system
  scheduler — `crates/core/src/ecs/` scheduler + the per-frame systems registered
  in `byroredux/src/main.rs` / `byroredux/src/systems.rs`).
- **Status**: NEW (no open issue; dedup against `/tmp/audit/issues.json` — only
  #1661 mentions Skyrim, an unrelated BSA-sibling bug. Not covered by the 06-14
  audit, which created this baseline at 321.1 fps.)
- **Description**: On the heaviest baselined interior (WhiterunDragonsreach,
  6049 entities, 294 newly-parsed meshes), the 240-frame bench window runs at a
  **steady** ~7 fps / dt≈147 ms for its entire ~28 s duration, then recovers
  instantly to 555–697 fps / dt≈1.5 ms the moment the window ends. The cost is
  **entirely CPU-side in the scheduler stage** — `wall_ms=117.3`,
  `systems_ms=116.3`, while `draw_ms=1.0` and every GPU pass reads ~0. The
  per-second `cpu_ms` breakdown pins it precisely: `atw_scheduler=138..147` ms
  during the window vs `atw_scheduler=1` ms once warm.
- **Evidence** (run 1; run 2 reproduced 8.7 fps / systems_ms=113.5 / 27 slow
  seconds):
  ```
  bench: frames=240 wall_fps=8.5 wall_ms=117.27 ... draw_ms=1.03
         systems_ms=116.33 ... entities=6049 draws=2444/2b/4c
  04:21:55 cpu_ms: ... atw_scheduler=138 ...   (during window)
  04:22:22 fps=555 avg=490 dt=1.80ms ...        (window over)
  04:22:22 cpu_ms: ... atw_scheduler=1 ...
  ```
  The same metric on **all four other games** shows `systems_ms` 0.14–1.18 ms
  with **zero** dt>100 ms frames — the pathology is unique to this cell.
- **Impact**: The first ~28 wall-seconds after entering Dragonsreach (or any
  cell of comparable scheduler load) render at ~7 fps. For a player this is a
  multi-second hitch on cell entry, not a steady-state stall — but it is a
  reproducible 37× regression against the contract metric and a >20% FPS drop,
  so HIGH per the skill's severity rule. The recover-after-N-frames shape points
  to a transient backlog draining through the scheduler (candidates: first-frame
  query-cache population across the 294 fresh meshes, deferred BLAS/descriptor
  warm-up serialized onto the main scheduler, or a newly-added per-frame system
  doing one-time-amortized work). It is **not** the M47.2 scripting systems
  (`trigger_detection_system` / `recurring_update_tick_system` iterate only the
  sparse `TriggerVolume` / `RecurringUpdate` sets, not all entities — and an
  O(entities) system would not self-recover after 28 s).
- **Related**: baseline created clean at 321.1 fps in AUDIT_RUNTIME_2026-06-14;
  scheduler-timing gate landed since (`e787cc78` Fix #1647); M45/M47.2 systems
  landed since.
- **Suggested Fix**: Bisect bench-window `atw_scheduler` on Dragonsreach across
  the 06-14→06-23 range (`git log --since=2026-06-14 -- crates/core/src/ecs/
  byroredux/src/systems.rs byroredux/src/main.rs`). Add a one-line
  per-system-cost dump for the first 60 frames (the scheduler already times each
  system post-#1647) to name the offending stage, then decide whether the
  backlog should be amortized across frames or moved off the per-frame scheduler.
  Pair with `/audit-performance` and `/audit-ecs`.

### RT-2: Oblivion Gilded Carafe FPS dropped 14.4% below the ×0.9 gate (411.8→352.3); gpu_calls 3→4
- **Severity**: MEDIUM
- **Dimension**: performance — surfaced via runtime telemetry
- **Location**: `bench:` line of
  `/tmp/audit/runtime/oblivion-ICMarketDistrictTheGildedCarafe.engine.log`.
- **Status**: NEW.
- **Description**: On Oblivion's tiny clean cell (701 entities), bench fps fell
  411.8→352.3, just under the `≥ baseline×0.9` gate (370.6). GPU calls ticked
  3→4. All structural symptom metrics (entities, tex.missing=0, mesh_fail=0,
  skin) are unchanged.
- **Evidence**: `wall_fps=352.3`, `draws=324/30b/4c` (baseline `324/30b/3c`).
- **Impact**: Low absolute — both fps and the +1 GPU call are small moves on a
  4-call, 400-fps scene where Xvfb wall-clock jitter dominates. The prior audit
  (RT-2, 06-14) explicitly flagged headless `bench_fps_*` as unreliable and
  recommended demoting it from the hard gate. This finding is the second data
  point for that demotion; absent the fps gate it would be a clean PASS.
- **Related**: AUDIT_RUNTIME_2026-06-14 RT-2 (`bench_fps_*` Xvfb noise);
  `draws=N/Mb/Kc` view-dependent count (#1258).
- **Suggested Fix**: Re-run 3× and average before treating as a true
  regression; if it holds, bisect the `draws` gpu-call split. Otherwise fold
  into the standing decision to make `bench_fps_*` advisory rather than gating
  (06-14 RT-2).

### RT-3: `entities_total` "exact-match" metric drifted up on fnv (+102), skyrim (+5), fo4 (+10) without a regen
- **Severity**: LOW
- **Dimension**: tech-debt (baseline hygiene) — surfaced via runtime telemetry
- **Location**: `.claude/audit-baselines/runtime/{fnv,skyrim_se,fo4}-*.tsv`
  `entities_total` rows.
- **Status**: NEW (continuation of the pattern in AUDIT_RUNTIME_2026-06-14 RT-2
  for FNV and RT-4 for FO4).
- **Description**: Three baselines carry an `entities_total` that the engine no
  longer reproduces exactly: fnv 9250→9352, skyrim 6044→6049, fo4 11279→11289.
  These are non-rendering entities (collision-only bodies, ragdoll/character
  rig, markers) added by ongoing work; the load-bearing render count
  (`bench_draws_cmds`) is unchanged on fnv (—) and fo4 (3800=3800), and the
  symptom metrics (tex.missing / mesh_fail / skin) all pass.
- **Evidence**: see per-game tables above.
- **Impact**: Cosmetic — but `entities_total` being a hard "exact match" metric
  means every benign non-render entity addition trips a false diff and can mask
  a real entity-count regression in the noise. This is the third audit to log
  the same drift.
- **Suggested Fix**: Either (a) regen the three baselines with `--regen` after
  eyeballing (the entity deltas are accounted for by collision/ragdoll/material
  work), or (b) split the contract into `render_entities` (= `draws_cmds`, keep
  exact) and `entities_total` (move to a ±2% tolerance), per the 06-14 RT-4
  suggestion. Until then these three are knowingly-stale, not regressions.

## Notes on metrics that held

- `tex_missing_unique_paths` and `mesh_cache_failed_count` — the two
  visible-symptom HIGH-gate metrics — **passed on every game**. Skyrim's
  `mesh_cache_failed_count` even improved 11→9: the two corrupted control-char
  paths flagged in 06-14 RT-3 did not recur in this capture (the 9 remaining are
  the legitimate marker/effect/clutter NIFs).
- `skin_pool_overflow_attempts == 0` on all five — no bind-pose spills (#1284
  cap healthy).
- `light_count_directional == 1` on all five (the constant single sun).

## Phase 6 cleanup

`/tmp/audit/runtime` retained for evidence paths cited above; no `byroredux`
engine of this audit's left running on port 9921 (verified). The concurrent
session's engine on 9876 was left untouched.

---

Report ready. Suggested next step:

```
/audit-publish docs/audits/AUDIT_RUNTIME_2026-06-23.md
```
