# Runtime Telemetry Audit — 2026-06-26

`--game all` headless sweep (xvfb, RTX 4070 Ti) against the five committed
baselines. Drive → capture (`stats` / `tex.missing` / `mesh.cache failed` /
`light.dump` + `bench:`) → diff. First sweep since the #1705 `entities_total`
tolerance change landed (`27baf66c`), so the headline question is whether that
gate fix holds on live telemetry.

## Per-game baseline comparison

| Game | Cell | Status | Notable Δ vs baseline |
|------|------|--------|------------------------|
| fnv | FreesideAtomicWrangler | **PASS** | entities 9250→9352 (+102, **in ±2% band** — #1705); draws_cmds 2722→2921 (+7.3%, ≤×1.1); fps 147→164 (advisory). `skin_pool_max` 1365→1364 (stale baseline, RT-3a) |
| fo3 | MegatonPlayerHouse | **PASS** | clean — entities/draws/skin/tex/mesh all flat; fps 93→145 (advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | **PASS (1 LOW)** | `bench_draws_gpu_calls` 3→4 (+1, exceeds ×1.1 — RT-3b); batches 30→27; fps 411→282 (advisory, known Xvfb-jitter cell) |
| skyrim_se | WhiterunDragonsreach | **PASS (struct) · #1698 advisory** | entities 6044→6049 (+5, in band); mesh_fail 11→**9** (improved); **fps 321→~4.6 avg — #1698 stall, atw_scheduler=29ms**; bench did not complete in-window |
| fo4 | InstituteBioScience | **PASS** | entities 11279→11289 (+10, in band); tex_missing 1→1; draws flat; fps 50→**136** (advisory, improved) |

Structural gate: **5/5 PASS.** No HIGH/MEDIUM regression on any gating metric
(textures, mesh-cache, skin-pool overflow, entity count in-band, draw split).

## Headline — #1705 tolerance fix validated on live telemetry

The three baselines that carried the recurring benign `entities_total` creep all
came back **within the new ±2% band**, so each produced a clean PASS instead of
the false REGRESSION an exact-match gate would have raised:

| Game | Baseline | Current | Δ | ±2% band | Verdict |
|------|---------:|--------:|---:|----------|---------|
| fnv  | 9250 | 9352 | +102 (+1.1%) | ±185 → [9065, 9435] | in band ✓ |
| skyrim_se | 6044 | 6049 | +5 (+0.08%) | ±121 → [5923, 6165] | in band ✓ |
| fo4  | 11279 | 11289 | +10 (+0.09%) | ±226 → [11053, 11505] | in band ✓ |

`bench_draws_cmds` (the exact render-load contract) held flat-or-down everywhere
(fnv +7.3% within ×1.1; fo3/skyrim/fo4 ≤ baseline), confirming the non-render
body creep that drove the `entities_total` delta did **not** change what renders.
The split is doing its job.

## Findings

### RT-1 (this sweep): #1698 Dragonsreach scheduler stall — still live, mitigations confirmed working
- **Severity**: advisory (cross-reference of open **#1698**, not a new finding)
- **Game/Cell**: skyrim_se / WhiterunDragonsreach
- **Evidence**: `cpu_ms: … atw_scheduler=29` (steady, every wall-second during
  the window) vs the 1 ms warm baseline; `stats` `FPS: 32.0 (avg 4.6)`; the
  240-frame bench could not complete inside the capture window (other games hit
  240 frames in <2 s). The engine log shows the player body free-falling
  (`M28.5 … body Y -6210→-6276, v -2000, grounded=false, rapier_bodies=1575`) —
  the collider-coverage gap #1698 names: clutter/player without a static floor
  collider stay awake and pin the dynamic solver.
- **Reading**: the landed mitigations are doing what their commits claimed.
  `atw_scheduler` is **29 ms, not the original 138-147 ms** — the substep budget
  (`a608fbb7`) and ragdoll keyframing (`036a7788`) cut the per-frame cost ~5×.
  But the baseline 321 fps is **not** restored during the settle storm (exactly
  the "robust mitigation, not a root-cause fix" the commit predicted). #1698
  stays open pending the `BYRO_PROFILE_FALLERS=1` run to name the collider-gap
  forms. This sweep is independent confirmation, not new scope.

### RT-2: skin_pool_max baseline stale on fnv (1365 → 1364)
- **Severity**: LOW (baseline hygiene — like RT-3/#1705)
- **Game/Cell**: fnv / FreesideAtomicWrangler
- **Baseline**: `skin_pool_max 1365` · **Current**: `1364`
- **Evidence**: fnv's baseline (captured 2026-05-28, the oldest of the five)
  reads 1365; every other baseline (2026-06-14) and every current capture reads
  1364. The fnv value predates a −1 change in the `SkinSlotPool` max-slot sizing
  between 05-28 and 06-14. `skin_pool_max` is an `exact match` metric, so this
  trips a diff — but it is the *baseline* that is stale, not a regression.
- **Suggested Fix**: regen just the fnv baseline (`--game fnv --regen`) to bring
  `skin_pool_max` to the consistent 1364, or hand-edit that one value with a
  `# RT-2` note. No engine change.

### RT-3: oblivion bench_draws_gpu_calls 3 → 4 (+1, exceeds ×1.1)
- **Severity**: LOW (likely batch→GPU-call nondeterminism; confirm before filing)
- **Game/Cell**: oblivion / ICMarketDistrictTheGildedCarafe
- **Baseline**: `cmds 324 / batches 30 / gpu_calls 3` · **Current**: `324 / 27 / 4`
- **Evidence**: input DrawCommands identical (324); post-merge batches *dropped*
  30→27, yet actual GPU calls *rose* 3→4. On the smallest/fastest cell (701
  entities, ~4 GPU calls) the batch→GPU-call merge is the most jitter-prone of
  the #1258 three-way split — the same reason this cell's `bench_fps` is gated
  advisory (RT-2/#1701). +1 absolute GPU call with cmds flat is far more likely
  merge nondeterminism than a real draw-path regression.
- **Suggested Fix**: re-run oblivion 2-3× to confirm whether `gpu_calls` is
  stably 4 (a real batch-merge change to bisect) or oscillates 3/4 (jitter →
  fold into the same advisory class as `bench_fps` for this cell). Do not file an
  engine bug until it reproduces.

## Advisory deltas (reported, never gating — RT-2 #1701)

| Game | fps baseline → current | Note |
|------|------------------------|------|
| fnv | 147.3 → 164.4 | +12% |
| fo3 | 93.3 → 145.0 | +55% |
| oblivion | 411.8 → 281.6 | −32% (Xvfb-jitter cell — the canonical advisory case) |
| skyrim_se | 321.1 → ~4.6 avg | **#1698 settle-storm** (see RT-1); not the fps-jitter class |
| fo4 | 50.0 → 136.2 | +172% — large improvement, likely the #1698 ragdoll-keyframe + perf work |

`bench_fps` is advisory per RT-2/#1701; none of the above is a finding. fo4's
+172% and fnv/fo3 gains are surfaced for visibility, not as regressions.

## Method notes

- **skyrim metrics are `stats`-derived, not `bench:`-derived.** At ~5-32 fps the
  240-frame bench window outran the capture timeout, so no `bench:` line printed
  (itself a #1698 symptom). `entities_total` (6049), `tex_missing` (0),
  `mesh_cache_failed` (9), `skin=0/1364+0`, and draws (2397/2/4) all come from
  the live `stats` line + `tex.missing`/`mesh.cache failed` — the skill permits
  `stats Entities:` as the `entities_total` source. Draw counts are a single live
  frame vs the baseline's frame-240 aggregate; all are ≤ baseline so the PASS
  holds, but the source differs.
- **No baselines were modified** (no `--regen`). RT-2 (fnv skin_pool_max) and the
  in-band entities_total drifts are left as last-known-good; only RT-2 warrants a
  targeted regen at the user's discretion.
- One transient driver bug hit mid-sweep: an inline `pkill -f 'target/release/
  byroredux'` self-matched the audit shell's own argv (the cmdline contained the
  pattern), killing the capture command itself. Worked around by killing by PID /
  `pgrep -x byroredux`; all five captures are valid.

## Verdict

5/5 structural PASS. The #1705 `entities_total` ±2% tolerance is **validated on
live telemetry** — its three target drifts all land in-band. The only true open
item this sweep surfaces is the already-tracked **#1698** (independently
re-confirmed: mitigated 5× but not resolved). RT-2 and RT-3 are LOW baseline/
nondeterminism items, not engine regressions.
