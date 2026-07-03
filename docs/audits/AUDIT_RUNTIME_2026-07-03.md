# Runtime Telemetry Audit — 2026-07-03

`/audit-runtime --game all`, part of a 21-audit `comprehensive` sweep against
HEAD `8498e559`.

## Setup

| Check | Result |
|-------|--------|
| Headless X | PRESENT — `/usr/bin/xvfb-run`, `-screen 0 1280x720x24` |
| Game data dirs | PRESENT — Oblivion, Fallout 3, Fallout NV, Skyrim SE, Fallout 4, Starfield all resolve under `/mnt/data/SteamLibrary/steamapps/common` |
| Binaries | Rebuilt clean: `cargo build --release -p byroredux -p byro-dbg` (8.6 s) |
| Baselines | 5 committed TSVs (fnv, fo3, oblivion, skyrim_se, fo4) under `.claude/audit-baselines/runtime/` |
| Starfield | SKIPPED — profile ships empty archives + no `sample_cells`; no cell baseline exists (SF coverage is `--sf-smoke`, out of scope here) |
| Dedup source | `gh issue list --repo matiaszanolli/ByroRedux --limit 200` → 71 issues fetched to `/tmp/audit/issues.json` |

Six games' data resolved; five have baselines and were diffed, run **serially**
per the SKILL's port-collision warning. Starfield has no cell baseline and was
skipped, matching every prior sweep.

**Methodology note (self-caught mid-run):** on the Oblivion capture, the prior
FO3 engine process had not yet exited when Oblivion launched, so Oblivion's own
debug server logged `failed to bind port 9876: Address already in use` at
startup — exactly the RT-1/#1619 collision the SKILL warns about. The first
`byro-dbg` capture against "oblivion" was silently reading FO3's telemetry
(3311 entities, FO3's mesh-cache-failed list) even though the `bench:` line in
Oblivion's own log correctly showed `entities=701`. Caught by cross-checking
the `byro-dbg` entity count against the `bench:` line before trusting either.
FO3 was killed, the port confirmed free, and Oblivion was **relaunched clean**
and recaptured — the table below uses the clean second run.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | PASS | entities 9250→9352 (+1.1%, in ±2% band); tex 1→1; mesh 11→11; skin 686/·+0; draws 2722→2921 cmds (+7.3%, ≤×1.1); `skin_pool_max` 1365→1364 — Existing: #1833 |
| fo3 | MegatonPlayerHouse | PASS | entities 3311→3311 (exact); tex 0→0; mesh 3→3; skin 0/·+0; draws 1839→1839 (exact) |
| oblivion | ICMarketDistrictTheGildedCarafe | PASS (1 LOW) | entities 701→701 (exact); tex 0→0; mesh 0→0; skin 3/·+0; draws 324→324 cmds, batches 30→27, `gpu_calls` 3→4 (+1, nominally exceeds ×1.1 — RT-1) |
| skyrim_se | WhiterunDragonsreach | PASS (structural) / perf ANOMALY | entities 6044→6049 (+0.08%, in band); tex 0→0; mesh 11→9 (improved, confirms #1620 fix intact); skin 0/·+0; draws 2614→2442 cmds (decrease, fine); `wall_fps` 321.1→32.7 (advisory) — Existing: #1698; freefall symptom also reproduced — Existing: #1832 |
| fo4 | InstituteBioScience | PASS | entities 11279→11289 (+0.09%, in band); tex 1→1 (`textures\temp_v1_d.dds`); mesh 0→0; skin 100/·+0 (exact); draws 3800→3800/279b/40c (exact) |
| starfield | — | SKIPPED | no cell baseline / empty profile archives (unchanged from every prior sweep) |

All five diffable games are structurally clean — zero new texture/mesh-cache
regressions, zero skin-slot overflow, draw-call counts within the ×1.1 gate on
every game, `entities_total` drift within the documented ±2% tolerance band
everywhere. The two performance/physics anomalies on Skyrim and the one
recurring Oblivion `gpu_calls` drift are all either previously-filed OPEN
issues (verified still reproducing, not regressed) or a continuation of a
long-standing baseline-staleness pattern.

## Findings

### RT-1: oblivion `bench_draws_gpu_calls` reproducibly reads 4, baseline says 3
- **Severity**: LOW
- **Dimension**: runtime / baseline staleness
- **Location**: `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv:9`
- **Status**: NEW (no dedicated issue found; closest sibling is #1833, the
  analogous FNV `skin_pool_max` staleness, which *is* filed)
- **Description**: The Oblivion baseline (created 2026-06-14) records
  `bench_draws_gpu_calls 3`. Every subsequent sweep that touched this cell —
  06-23 (`AUDIT_RUNTIME_2026-06-23.md`, reported as MEDIUM regression),
  06-26 (`AUDIT_RUNTIME_2026-06-26.md`, downgraded to LOW "RT-3b"), 07-02
  (`AUDIT_RUNTIME_2026-07-02.md`, called "within run-to-run noise"), and now
  this run — has read `4`, never `3`. Four independent runs across three weeks
  landing on the same value is a *stable* number, not run-to-run jitter; the
  baseline itself is one GPU call stale, structurally identical to the
  `skin_pool_max 1365→1364` pattern already tracked at #1833.
- **Evidence**: This run's `byro-dbg` capture: `Draws: 324 cmds → 27 batches →
  4 GPU calls`; the committed baseline row reads `bench_draws_gpu_calls	3`.
  `bench_draws_cmds` (324) and `bench_draws_batches` (27, ≤ baseline 30) both
  match/improve, so this is isolated to the GPU-call count on a 4-call scene —
  crossing an integer boundary on a very small draw count inflates the
  percentage (33%) even though the absolute drift is ±1.
- **Impact**: None functionally — `gpu_calls` on a 4-draw-call interior is
  cosmetic telemetry, not a render defect. The only real effect is this LOW
  finding re-appearing in every future `--game all` sweep until the baseline
  is regenerated, adding noise to the audit trail (the same complaint that
  produced #1833 for the FNV case).
- **Suggested Fix**: `--regen` the Oblivion baseline alongside a fix for
  #1833 in the same pass — both are stale-value housekeeping, not code bugs.
- **Related**: #1833 (identical pattern, different metric/game)

### RT-2: Skyrim Dragonsreach scheduler stall reproduces at 321→32.7 fps
- **Severity**: HIGH (unchanged)
- **Dimension**: runtime / performance (physics scheduler)
- **Location**: physics/character scheduler stage; symptom on `bench:`
  `systems_ms` (`byroredux/src/systems/debug.rs`, `engine::stats` cpu_ms)
- **Status**: **Existing: #1698** ("RT-1: Skyrim Dragonsreach bench-window FPS
  collapsed 321→8.7 — ECS scheduler stalls ~140 ms/frame for ~28 s") — OPEN,
  confirmed still present, not regressed further
- **Description**: This run's `bench:` line: `wall_fps=32.7 wall_ms=30.58 …
  draw_ms=1.07 [fence=0.46 …] systems_ms=29.15` — 29.15 ms of the 30.58 ms
  frame budget (95%) is `systems_ms`, not GPU draw/present time. Structural
  render metrics (entities, textures, mesh cache, draws) are all clean; this
  is exclusively a CPU-side scheduler/physics cost, exactly matching #1698's
  description. Per the SKILL, `bench_fps_*` is advisory and does not gate a
  regression finding on its own, but the magnitude (321.1 baseline → 32.7,
  −89.8%) is large enough that this audit re-confirms rather than silently
  drops it, consistent with the two prior sweeps (06-14, 07-02) that also
  logged it against the same OPEN issue.
- **Evidence**: `bench: frames=240 wall_fps=32.7 wall_ms=30.58 brd_ms=0.70
  ui_ms=0.00 draw_ms=1.07 [fence=0.46 tlas=0.13 ssbo=0.13 cmd=0.07 submit=0.11]
  [gpu_skin_disp=0.000 gpu_blas_refit=0.000 gpu_taa=0.319] systems_ms=29.15
  ticks_per_frame=1.0 unaccounted_ms=0.00 entities=6049 meshes=651 textures=310
  draws=2442/2b/2c`
- **Impact**: Skyrim SE interiors with a nontrivial physics/ragdoll body count
  run at ~33 fps instead of the ~320 fps every other structural metric implies
  they should sustain — a real, user-visible perf cliff on one specific game,
  not a rendering defect.
- **Related**: #1698 (tracking issue), #1832 (below, likely same root cause
  family — runaway physics simulation)

### RT-3: TES-family character rig freefall reproduces on Oblivion and Skyrim
- **Severity**: MEDIUM (unchanged)
- **Dimension**: runtime / physics (character controller)
- **Location**: `byroredux/src/systems/character.rs`, `M28.5` grounding log
- **Status**: **Existing: #1832** ("RT-2: TES-family character rig never
  grounds -> infinite freefall (Oblivion, Skyrim)") — OPEN, confirmed still
  present on both games this run
- **Description**: Both Oblivion and Skyrim SE logs show the player/character
  rigid body falling indefinitely (`grounded=false` on every tick, `v=-2000.0`
  terminal-velocity clamp) for the full bench-hold window. FO3/FNV/FO4 (all
  Fallout-family) were not observed doing this in their respective logs during
  this run.
- **Evidence**: Skyrim — `M28.5 frame 1560: body Y -95069.8→-95131.9 (Δ
  -62.110), v -2000.0, grounded=false, rapier_bodies=1575` (31 occurrences in
  the captured window). Oblivion — `M28.5 frame 10800: body Y 323.2→323.2 (Δ
  0.000), v -2000.0, grounded=false, rapier_bodies=156` (body has settled
  against geometry but `grounded` still reads false).
  it is entirely plausible this shares a root cause with RT-2/#1698 above —
  1575 falling rapier bodies on Skyrim is a large simulated mass that would
  plausibly explain a scheduler stall, though this audit did not bisect that
  causal link.
- **Impact**: Same as filed at #1832 — no gameplay-visible effect yet (no
  player-controller landing logic consumes this state today) but it is a
  latent correctness gap in the TES-family grounding path that FO-family
  avoids.
- **Related**: #1698 (possible shared root cause — see note above)

### RT-4: FNV `skin_pool_max` baseline still stale (1365 vs live 1364)
- **Severity**: LOW
- **Dimension**: runtime / baseline staleness
- **Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv:11`
- **Status**: **Existing: #1833** ("RT-3: FNV runtime baseline skin_pool_max
  is stale (1365 vs live 1364)") — OPEN, confirmed still present, not fixed
  since it was filed
- **Description**: This run's FNV capture reads `skin=686/1364+0`
  (`skin_pool_max=1364`); the committed baseline still records `1365`. Every
  other baseline (fo3/oblivion/skyrim_se/fo4) already records the correct
  `1364`, so FNV's is the one stale outlier, exactly as filed at #1833.
- **Evidence**: engine log tail: `fps=146 avg=222 dt=6.83ms entities=9352
  meshes=1301 textures=425 draws=2893/101b/9c skin=686/1364+0`; baseline TSV:
  `skin_pool_max	1365`.
- **Impact**: Cosmetic-only — the pool cap is uniform `1364` across every
  game this run confirmed; no functional bone-palette regression. Purely a
  recurring audit-noise item until the baseline is regenerated.
- **Suggested Fix**: `--regen` the FNV baseline's `skin_pool_max` row to
  `1364` (same housekeeping pass suggested for RT-1 above).
- **Related**: RT-1 (identical staleness pattern, different game/metric)

## Positive confirmations (not findings)

- **Skyrim `mesh_cache_failed_count` improved 11→9**, and the 9 remaining
  paths contain none of the corrupted control-character paths called out in
  the original 2026-06-14 baseline note. The engine log for this run shows
  ~30+ `#1620 — ARMO … corrupt MODL mesh path (control bytes), treating as
  model-less` WARN lines — confirming the #1620 fix (treat corrupt MODL paths
  as model-less rather than attempting — and failing — the mesh load) is
  landed and intact, not regressed. This is a genuine improvement over the
  committed baseline, not a regression, and needs no action beyond an eventual
  `--regen` to stop it showing as "improved" drift forever.
- **FNV, FO3, FO4** all reproduce their committed baselines essentially
  exactly (entities within documented tolerance, zero new missing textures,
  zero new mesh-cache failures, exact skin-pool and draw-split counts on
  FO3/FO4). No regressions found on any of the three.

## Cleanup

- All five engine instances launched serially, `byro-dbg`-polled for `pong`,
  captured, and torn down with `SIGINT` → `SIGKILL` before the next launch.
- Final check: `pgrep -af 'byroredux|byro-dbg'` returned no engine/debug-CLI
  processes before this report was written.
- `/tmp/audit/runtime` removed; committed baselines under
  `.claude/audit-baselines/runtime/` untouched (no `--regen` was requested for
  this run).

## References

- SKILL: `.claude/commands/audit-runtime/SKILL.md`
- Shared protocol: `.claude/commands/_audit-common.md`, `.claude/commands/_audit-severity.md`
- Prior runs: `docs/audits/AUDIT_RUNTIME_2026-06-14.md`,
  `AUDIT_RUNTIME_2026-06-01.md`, `AUDIT_RUNTIME_2026-06-23.md`,
  `AUDIT_RUNTIME_2026-06-26.md`, `AUDIT_RUNTIME_2026-07-02.md`
- Tracking issues referenced: #1698, #1832, #1833
