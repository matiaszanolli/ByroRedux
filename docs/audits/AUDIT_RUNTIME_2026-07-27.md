# Runtime Telemetry Audit — 2026-07-27

Headless drive of five baselined `(game, cell)` pairs, telemetry harvested over
`byro-dbg`, diffed against `.claude/audit-baselines/runtime/`.
Repo HEAD: `db625997`. Starfield skipped (profile ships empty archives, no cell
baseline — use `--sf-smoke`). No `--regen` passed; **baselines untouched**.

## Headline

**Every gate trip in this sweep was already filed by
[AUDIT_RUNTIME_2026-07-25.md](AUDIT_RUNTIME_2026-07-25.md) (RT-1/RT-2/RT-3).
The new information is that the fix which closed them does not work.**

Commit `8e55a714` ("perf: restore particle indirect grouping", 2026-07-25) closed
**#2165** and states in its own message:

> Confirmed at runtime on three corpora (fnv gpu_calls 10->23, oblivion batches
> 27->31, fo4 gpu_calls 40->46).

I measured all three corpora **at that exact commit** and at HEAD. None of the
three was restored:

| Corpus / metric | Baseline | Pre-regression<br>`883f57cd~1` | **At the "fix"**<br>`8e55a714` | **HEAD**<br>`db625997` |
|---|---|---|---|---|
| fnv `bench_draws_gpu_calls` | 10 | **8** | **23** | **23** |
| oblivion `bench_draws_batches` | 27 | — | **31** | **31** |
| fo4 `bench_draws_gpu_calls` | 40 | — | **48** | **48** |

The pre-regression build (`883f57cd~1`, built and run in an isolated worktree)
returns FNV to `2629/103b/**8c**`, confirming both the baseline's ~10 and the
attribution of the regression to `883f57cd` (2026-07-20). But the corrective
commit changed nothing measurable on any of the three corpora it names, and
FO4 is now **48**, worse than the 46 it claims to have repaired.

This corroborates the standing caution recorded after the last sweep — that the
two-sided-blend-split predicate is dormant on tested cells and that drift
attributed to it is misattributed. `8e55a714` fixed that predicate
(`DrawBatch::order_dependent_glass`, `is_refractive_glass`); the metric did not
move. **The real mechanism inside `883f57cd` has not been found.** Locating it is
a `/audit-performance` job, not a telemetry one — this report establishes the
empirical bracket and hands it off.

**No HIGH findings.** Every symptom-proxy metric the skill gates at HIGH —
`tex_missing_unique_paths`, `mesh_cache_failed_count`, `skin_pool_overflow_attempts`
— is flat or improved on all five corpora. Nothing is rendering worse for want of
a texture, a mesh, or a skin slot.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | REGRESSION (MEDIUM) | entities 9250→9271 (+0.23%, in band); tex 1→1; mesh 11→11; skin 686→677 (decrease, fine); cmds 2722→2629 (decrease), batches 104→106 (+1.9%, fine); **gpu_calls 10→23 (+130%)**; fps 141.4→141.4 (exact) |
| fo3 | MegatonPlayerHouse | **PASS** | entities 3311→3311 (exact); tex 0→0; mesh 3→3; skin 0→0; cmds 1839→1583 (−13.9%), batches 96→91 (decrease), gpu_calls 9→8 (decrease); fps 93.3→89.3 (advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | REGRESSION (MEDIUM) | entities 701→701 (exact); tex 0→0; mesh 0→0; skin 3→3; cmds 324→324 (exact), gpu_calls 4→4 (exact); **batches 27→31 (+14.8%)**; fps 323.4→439.6 (advisory) |
| skyrim_se | WhiterunDragonsreach | REGRESSION (MEDIUM ×2) | **entities 6044→6391 (+5.74%)**; tex 0→0; mesh 11→9 (decrease, fine); **skin 0→25 (against `≤ baseline`)**; cmds 2614→2432 (decrease), batches 3→9, gpu_calls 5→2 (decrease); fps 321.1→250.9 (advisory) |
| fo4 | InstituteBioScience | REGRESSION (MEDIUM ×3) | **entities 11279→12448 (+10.36%)**; tex 1→1; mesh 0→0; **skin 100→124 (against `≤ baseline`)**; cmds 3800→3824 (+0.6%, fine), batches 272→279 (+2.6%, fine); **gpu_calls 40→48 (+20%)**; fps 50.0→59.4 (advisory) |

## Findings

### RT-1: `8e55a714` closed #2165 without restoring any of the three metrics it names

- **Severity**: MEDIUM
- **Game**: fnv, oblivion, fo4 (all three corpora named in the commit)
- **Cell**: FreesideAtomicWrangler / ICMarketDistrictTheGildedCarafe / InstituteBioScience
- **Status**: **NEW.** #2165 is CLOSED; no open issue tracks the still-live regression.
- **Baseline / Current**: see the table above — fnv 10→23, oblivion 27→31, fo4 40→48, unchanged from the regressed state at the fix commit itself.
- **Evidence**: three builds in an isolated worktree (`883f57cd~1`, `8e55a714`, HEAD), each run headless at `--bench-frames 240`. `883f57cd~1` yields FNV `2629/103b/8c`; `8e55a714` and HEAD both yield `2629/106b/23c`. Oblivion is `324/31b/4c` at both the fix and HEAD; FO4 is `3824/279b/48c` at both.
- **Why this matters**: the regression was root-caused to a predicate
  (`needs_two_sided_blend_split` losing its `&& b.z_write` limb), the predicate
  was rewritten to be material-driven, tests were added — and the runtime metric
  did not move a single call. The code change is defensible on its own terms; the
  *causal claim* attached to it is not. `883f57cd` is a large commit (stable
  surface ID replacing `GpuInstance` padding, a thin-glass material flag, changed
  alpha-blend blending state); the indirect-grouping loss is somewhere else in it.
- **Suggested Fix**: re-open the tracking issue. Bisect *within* `883f57cd` by
  reverting its sub-changes one at a time against the FNV corpus (8 → 23 GPU calls
  is a large, stable, 2-second signal). Add a telemetry assertion to the runtime
  harness so a `gpu_calls` regression cannot be closed without the number moving.
  Verified non-causes: `is_refractive_glass` correctly excludes
  `MATERIAL_KIND_FIRE_REFRACTION` (103), and `surface_id` does not enter the batch
  merge key or `group_state`.

### RT-2: `entities_total` beyond the ±2 % band on skyrim_se and fo4

- **Severity**: MEDIUM (per the skill's gate) — **but probably benign; recommend `--regen`**
- **Game / Cell**: skyrim_se `WhiterunDragonsreach` (6044→6391, +5.74%); fo4 `InstituteBioScience` (11279→12448, +10.36%)
- **Status**: Re-measurement of the 2026-07-25 sweep's identical finding (6395 / 12448 there). Not new; not regressed further.
- **Assessment**: this is the documented benign-creep pattern (#1705 / RT-3) —
  non-rendering bodies (collision-only colliders, ragdoll rigs, markers) drifting
  up as collision/actor work lands. The render-load contract corroborates it:
  FO4 `bench_draws_cmds` moved +0.6 % (3800→3824) against a +10.36 % entity rise,
  and Skyrim's *fell* 7 % (2614→2432). More bodies, no more rendering.
- **Suggested Fix**: regenerate both baselines (`--regen`) once the RT-1
  investigation lands, so the band stops flagging settled drift. Regenerating now
  would bake in the live RT-1 draw-split regression.

### RT-3: `skin_pool_live` rose against its `≤ baseline` direction on skyrim_se and fo4

- **Severity**: MEDIUM (per the skill's gate) — **but this looks like a fix landing, not a regression**
- **Game / Cell**: skyrim_se `WhiterunDragonsreach` (0→25); fo4 `InstituteBioScience` (100→124)
- **Status**: Re-measurement of the 2026-07-25 finding. Stable across both this sweep's methodologies.
- **Assessment**: Skyrim going **0 → 25** live skin slots is the shape of skinned
  meshes that previously failed to skin now succeeding — and `22798ecc`
  ("fix skin version gates", in this window) is the obvious candidate. Read that
  way it is progress, not cost. `skin_pool_overflow_attempts` is still `0` and
  `skin_pool_max` still `1364` on every corpus, so there is no pressure on the
  #1284 cap either way. The `≤ baseline` gate direction treats slot occupancy as
  pure cost, which mis-reads a coverage improvement as a regression.
- **Suggested Fix**: confirm the attribution to `22798ecc`, then `--regen` both
  baselines. Consider gating `skin_pool_live` as a tolerance band (like
  `entities_total`) rather than a one-sided `≤`, since it moves with skinning
  *coverage*, not just cost.

## Methodology note (affects how these numbers compare to the baselines)

The skill's determinism guidance (`BYROREDUX_FIXED_DT=0`) is **incompatible with
its own `skin_pool_*` metrics**: `log_stats_system` only emits the `skin=L/M+S`
line on `crosses_one_second_boundary(total, dt)`, and `dt = 0` freezes
`TotalTime`, so the line never fires and three of the twelve metrics are
uncapturable. The `skin=` line also needs `RUST_LOG` to admit `info` on target
`engine::stats`.

This sweep therefore ran twice per corpus: once at `BYROREDUX_FIXED_DT=0.01666`
(deterministic 60 Hz *and* an advancing clock) and once with `FIXED_DT` unset. The
control run reproduced the FNV baseline `wall_fps` **exactly** (141.4 vs 141.4),
establishing that the committed baselines were captured on wall-clock dt — so the
**unset-dt numbers are the ones reported above**. Under `FIXED_DT=0.01666`,
animation advances ~2.4× faster per frame and `wall_fps` drops accordingly
(FNV 141.4→68.2); that is a methodology artifact, not a regression, and is one
more reason `bench_fps_*` stays advisory.

Every structural metric was **identical under both methodologies** on all five
corpora, with one exception: fo3 `gpu_calls` read 10 under fixed-dt and 8 under
wall-clock (baseline 9). That metric is view/timing-dependent at small counts, so
fo3's apparent one-call trip is noise and fo3 is scored **PASS**.

Two harness bugs were hit and fixed in the driver, both worth folding into the
skill:

1. `kill $PID` kills the `xvfb-run` **wrapper**, not the engine. The engine
   survived teardown and the next launch collided on the fixed debug port 9876 —
   the exact RT-1/#1619 hazard the skill warns about, reached *without*
   parallelism. Teardown must `pkill -x byroredux` (the `-x` exact-name form; a
   `-f` pattern match would also match this audit's own shell, the exit-144
   self-kill trap).
2. Capturing telemetry on a debug-server ping races the bench window. With
   animation advancing, FNV needed ~12 s to reach frame 240 while the debug
   server answered at ~4 s, so the capture sampled a partially-settled cell and
   no `bench:` line existed yet. The driver now waits for `wall_fps=` to appear
   in the log — which is what `--bench-hold` exists to make safe — before
   attaching.

## Advisory `bench_fps_*` deltas (never gating, per RT-2/#1701)

fnv 141.4→141.4 (0.0 %) · fo3 93.3→89.3 (−4.3 %) · oblivion 323.4→439.6
(+35.9 %) · skyrim_se 321.1→250.9 (−21.9 %) · fo4 50.0→59.4 (+18.8 %, baseline
is a deliberately conservative floor). Oblivion's +36 % and Skyrim's −22 % on
cells of 701 and 6391 entities are the documented Xvfb-jitter band; the FNV
exact-reproduction is the calibration point showing the harness itself is sound.

## Baseline actions

None taken — no `--regen` was passed and no baseline file was modified. Recommended
sequence once RT-1 is resolved: fix the draw-split regression first, re-run, then
`--regen` skyrim_se and fo4 to absorb the settled `entities_total` / `skin_pool_live`
drift. Regenerating before RT-1 lands would bake the live regression into the guard.
