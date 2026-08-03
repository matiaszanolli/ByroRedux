# Runtime Telemetry Audit — 2026-08-03

Headless drive of all five baselined `(game, cell)` pairs, telemetry harvested
over `byro-dbg`, diffed against `.claude/audit-baselines/runtime/`. Repo HEAD:
`1ae86f62`. Starfield skipped (profile ships empty archives, no cell baseline
— use `--sf-smoke`). No `--regen` passed; **baselines untouched**.

This is one leg of a `comprehensive` audit-suite sweep run the same day as
`/audit-performance` (`docs/audits/AUDIT_PERFORMANCE_2026-08-03.md`, 0
CRITICAL, 1 HIGH — PERF-D7-01, exterior persistent-cell load bypassing the
new resumable/budgeted streaming architecture). **None of this audit's five
corpora are exterior cells** (all five are interior), so this sweep produced
no telemetry that exercises that code path — it can neither corroborate nor
refute PERF-D7-01. This is a standing coverage gap in this skill's candidate
matrix (Starfield's is the only other noted gap, for unrelated reasons); it
existed in every prior runtime sweep too.

## Headline

Every gate trip this sweep reproduces (exactly or in the same direction) a
finding already tracked by an OPEN issue from the 2026-07-27 sweep — **#2215**
(RT-1: `#2165`'s fix doesn't restore indirect draw grouping) and **#2216**
(RT-2: skyrim_se/fo4 `entities_total` + `skin_pool_live` benign drift) — with
two pieces of genuinely new information:

1. **fo4's `bench_draws_gpu_calls` recovered**: 40 (baseline) → 48 (07-27,
   regressed) → **13 (now, well within the `×1.1` gate)**. This lines up with
   `b5d9f181` ("feat(render): add sorting for raster-visible draws and improve
   draw command handling", 2026-08-01, touching `byroredux/src/render/mod.rs`
   + `byroredux/src/render/static_meshes.rs`), which landed after the 07-27
   sweep. **fnv and oblivion did not recover** — fnv `gpu_calls` is still 23
   (baseline 10), oblivion `bench_draws_batches` is still 31 (baseline 27) —
   so whatever `b5d9f181` fixed, it did not fix the same thing #2215's other
   two named corpora are hitting.
2. **skyrim_se now shows a fourth, previously-unmeasured symptom**:
   `bench_draws_batches` regressed baseline-3 → 8 (reproduced identically
   across two independent runs today), the same symptom class as #2215 but on
   a corpus #2215 doesn't name. See RT-1 below.

**0 CRITICAL, 0 HIGH found. 1 NEW finding (MEDIUM). Two existing-issue
corroborations (#2215, #2216), one of which (skyrim_se `entities_total`) has
escalated materially since it was last measured** — see RT-3.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | REGRESSION — Existing #2215 | entities 9250→9271 (+0.23%, in band); tex 1→1; mesh 11→11; skin 686→677 (dec, fine); cmds 2722→2554 (dec); batches 104→106 (+1.9%, fine); **gpu_calls 10→23 (+130%, unchanged from 07-27)**; fps 141.4→134.4 (−4.9%, advisory) |
| fo3 | MegatonPlayerHouse | **PASS** | entities 3311→3311 (exact); tex 0→0; mesh 3→3; skin 0→0; cmds 1839→1564 (dec); batches 96→91 (dec); gpu_calls 9→8 (dec); fps 93.3→108.1 (+15.9%, advisory) |
| oblivion | ICMarketDistrictTheGildedCarafe | REGRESSION — Existing #2215 | entities 701→701 (exact); tex 0→0; mesh 0→0; skin 3→3 (exact); cmds 324→324 (exact); **batches 27→31 (+14.8%, unchanged from 07-27)**; gpu_calls 4→4 (exact); fps 323.4→386.7 (+19.6%, advisory) |
| skyrim_se | WhiterunDragonsreach | REGRESSION (×3) — 2 Existing #2216, 1 NEW | **entities 6044→8068 (+33.5%, escalated from +5.74% on 07-27)** [#2216]; **skin 0→25 (against `≤ baseline`, unchanged from 07-27)** [#2216]; tex 0→0; mesh 11→9 (dec, fine); cmds 2614→2304 (dec); **batches 3→8 (+167%) — NEW, not covered by #2215 or #2216**; gpu_calls 5→2 (dec); fps 321.1→180.2 (−43.9%, advisory) |
| fo4 | InstituteBioScience | REGRESSION (×2) — Existing #2216, gpu_calls recovered | **entities 11279→12448 (+10.36%, unchanged from 07-27)** [#2216]; **skin 100→124 (against `≤ baseline`, unchanged from 07-27)** [#2216]; tex 1→1; mesh 0→0; cmds 3800→3353 (dec); batches 272→279 (+2.6%, fine); **gpu_calls 40→13 — RECOVERED from 07-27's 48**, likely `b5d9f181`; fps 50.0→42.6 (−14.8%, advisory) |

All `skin_pool_overflow_attempts` and `light_count_directional` values are `0`
and `1` respectively across all five corpora — the two HIGH-minimum gates
(overflow spill, missing sun) are clean everywhere.

## Findings

### RT-1: `bench_draws_batches` regressed on skyrim_se (baseline 3 → 8/9), the same symptom class as #2215 but on an untracked fourth corpus
- **Severity**: MEDIUM
- **Dimension**: Runtime Telemetry — draw batching
- **Location**: `byroredux/src/render/mod.rs`, `byroredux/src/render/static_meshes.rs` (draw-batch assembly touched most recently by `b5d9f181`); merge/indirect-grouping consumers in `crates/renderer/src/vulkan/context/draw.rs` and `crates/renderer/src/vulkan/context/geometry_pass.rs`
- **Status**: NEW. Confirmed via `gh issue list` — #2215 ("RT-1: #2165's fix does not restore indirect grouping — fnv gpu_calls still 23, oblivion 31, fo4 48 at HEAD") names only fnv/oblivion/fo4; #2216 covers `entities_total`/`skin_pool_live` only. Neither names skyrim_se `bench_draws_batches`.
- **Description**: skyrim_se `WhiterunDragonsreach`'s baseline `bench_draws_batches` is 3. The 07-27 sweep measured 9 (not called out as its own finding there, folded into the general Δ list). Today's sweep measured 8, reproduced **identically** across two independent back-to-back runs (`draws=2304/8b/2c` both times) — this is a stable, real behavior change, not small-count sampling noise (contrast with fo3's `gpu_calls` 9↔8↔10 wobble, which the 07-27 report correctly dismissed as noise at that scale).
- **Evidence**: baseline TSV `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` line 16 (`bench_draws_batches 3`); this sweep's two captures both show `draws=2304/8b/2c` in the `bench:` line.
- **Impact**: Same class of impact as #2215 — the post-merge batch count growing while cmds count falls (2614→2304 here) means draws that should combine into one indirect batch are not merging, adding avoidable per-frame CPU (sort/group) and GPU (extra indirect submits) overhead. Small absolute magnitude on this cell (3→8) bounds the blast radius today, but if it shares #2215's root cause in `883f57cd`, it will scale with scene complexity like the other three corpora do.
- **Related**: #2215 (same symptom class, different corpora); #2216 (this same cell also carries the tracked `entities_total`/`skin_pool_live` drift — see RT-3).
- **Suggested Fix**: fold this corpus into #2215's bisection work (`883f57cd` sub-change isolation) rather than filing a disjoint issue — track whether the same reverted sub-change also restores skyrim_se to ~3 batches. If it does not move together with fnv/oblivion under that bisection, that is itself informative (rules out a single shared cause).

### RT-2: Existing #2215 — fnv/oblivion regression persists unchanged at HEAD; fo4 has recovered
- **Severity**: n/a (status update on an existing OPEN issue, not a new finding)
- **Status**: Existing: #2215
- **Description**: Re-measured all three corpora #2215 names, at repo HEAD `1ae86f62` (one week after the 07-27 measurement that filed it). fnv `bench_draws_gpu_calls` is unchanged at 23 (baseline 10, gate is ≤11). oblivion `bench_draws_batches` is unchanged at 31 via the `bench:` line (baseline 27, gate is ≤29.7) — a `byro-dbg stats` query taken moments later showed 30, a harmless query-time variance already documented for this metric. **fo4 `bench_draws_gpu_calls` recovered**: 48 (07-27) → 13 (now), comfortably inside the ≤44 gate. This correlates in time with `b5d9f181` ("feat(render): add sorting for raster-visible draws and improve draw command handling", 2026-08-01), the only draw-path commit landed between the two sweeps, which touched `byroredux/src/render/mod.rs` and `byroredux/src/render/static_meshes.rs`.
- **Evidence**: fnv `.engine.log`/`byro-dbg` capture this run: `draws=2554/106b/23c`. oblivion: `draws=324/31b/4c` (bench) / "30 batches" (stats query). fo4: `draws=3353/279b/13c` — batches (279) essentially unchanged from 07-27's 279, only the post-merge GPU-call count fell, meaning whatever changed affects final indirect-call assembly, not the earlier per-draw batch grouping.
- **Impact**: The recovery is corpus-specific, not a general fix — #2215 should stay open. The fact that batches (279) stayed flat on fo4 while gpu_calls dropped 48→13 narrows where in the pipeline the fix (or the original break) lives: after batch grouping, at indirect-call submission.
- **Suggested Fix**: use `b5d9f181` as a new bisection candidate for #2215 — check whether reverting it re-regresses fo4's gpu_calls back to 48, and whether cherry-picking its logic onto the fnv/oblivion code paths (if they differ) restores those two as well.

### RT-3: Existing #2216 — skyrim_se `entities_total` drift has escalated materially since last measured; fo4/skin_pool_live otherwise stable
- **Severity**: n/a (status update on an existing OPEN issue) — but flagging the escalation for the issue owner to re-triage
- **Status**: Existing: #2216
- **Description**: #2216 characterizes skyrim_se/fo4 `entities_total`/`skin_pool_live` movement as benign non-rendering-body drift. fo4 is unchanged since 07-27 (`entities_total` 12448, `skin_pool_live` 124 — identical both sweeps). skyrim_se's `skin_pool_live` is also unchanged (25, both sweeps). **skyrim_se's `entities_total`, however, has grown again**: 6044 (baseline) → 6391 (07-27, +5.74%) → **8068 (now, +33.5% vs baseline, +26.2% vs the 07-27 reading alone)**. Reproduced identically across two independent runs today. `bench_draws_cmds` (the exact render-load contract) kept *falling* across all three measurements (2614→2432→2304), which is the same signature #2216 uses to argue the growth is non-rendering bodies, not scene content — so the mechanism classification likely still holds. What's new is the *rate*: this corpus gained more non-render entities in the six days since 07-27 than in the prior five weeks combined.
- **Evidence**: baseline TSV line 6 (`entities_total 6044`); this sweep's two runs both report `entities=8068` in the `bench:` line and via `byro-dbg stats`.
- **Correlated commits** (landed in the measurement window, all touch scripting/actor-spawn systems that could add non-render entities on a populated settlement cell like Whiterun): `9bf4c493` (resumable NPC assembly, 07-27), `6df3bad8` ("Implement ECS runtime for SCEN records with scene management and playback", 07-31), `022cf421` ("Implement PACK execution for scene package actions", 08-01), `0ff8612b` ("Implement cinematic effects for MQ101 quest... FragmentExecutionQueue...", 08-01). None of these are confirmed causes — offered as bisection candidates, not a root-cause claim.
- **Impact**: Still classified non-rendering per the `bench_draws_cmds`-falling signature, so likely still benign — but a +26% jump in six days on one corpus while fo4 stayed flat suggests skyrim_se specifically is accumulating faster than the "settled creep" characterization in #2216 assumed. If SCEN/PACK entities are markers/state objects that should be cleaned up post-execution and aren't, that would be a real (if non-visual) leak rather than settled drift.
- **Suggested Fix**: before the next `--regen`, have #2216's owner spot-check what's actually driving skyrim_se's entity count with `entities` / `sys.accesses`-style introspection against `WhiterunDragonsreach` — confirm the extra ~1677 entities are inert (colliders/markers/timers) and not, e.g., a per-tick SCEN/PACK spawn that never despawns.

## Advisory `bench_fps_*` deltas (never gating, per RT-2/#1701)

fnv 141.4→134.4 (−4.9%) · fo3 93.3→108.1 (+15.9%) · oblivion 323.4→386.7
(+19.6%) · skyrim_se 321.1→180.2 (−43.9%) · fo4 50.0→42.6 (−14.8%). All five
are single-sample headless `wall_fps` under `xvfb-run`; per the standing
caution these are not used to gate any finding. skyrim_se's −43.9% is large
enough to be worth a follow-up if a real fps investigation is ever opened
(re-run 3× and average, per the skill's own guidance) but is not reported as
a finding here.

## Methodology notes

- **`BYROREDUX_FIXED_DT=0` is incompatible with `skin_pool_*` capture** (as
  the 2026-07-27 report also found): `log_stats_system` only emits the
  `skin=L/M+S` line on a `TotalTime` second-boundary crossing, and fixing dt
  to `0` freezes `TotalTime`, so the line never fires. The first FNV attempt
  this session used `BYROREDUX_FIXED_DT=0` per the skill's determinism
  guidance and got zero `skin=` lines; all five captures reported here were
  re-run with `FIXED_DT` unset (real wall-clock dt), matching how the
  committed baselines were captured.
- **A stale engine process outlived `kill $PID`** on the first FNV attempt:
  `$!` after `xvfb-run -a ... ./target/release/byroredux &` captures the
  `xvfb-run` wrapper's PID, not the engine binary's. `kill -INT`/`-9` on that
  PID left the actual `byroredux` process running, which then collided with
  the next launch on debug port 9876 and briefly interleaved two processes'
  output into one log file. Fixed for the remaining four corpora (and FNV's
  re-run) by resolving the real child PID via `pgrep -af "byroredux --game
  <key>" | grep -v xvfb-run` immediately after launch and killing that PID
  instead. This is the same harness bug the 07-27 report already called out
  and recommended fixing in the skill — it recurred here because this sweep
  started from the skill text, not from that report's fix.
- skyrim_se's two key deltas (`entities_total`, `bench_draws_batches`) were
  each independently reproduced via a second, fully independent engine launch
  before being reported, per this project's audit-hygiene norm of verifying a
  finding's premise before writing it up.

## Baseline actions

None taken — no `--regen` was passed and no baseline file was modified.
Regenerating skyrim_se/fo4 now would bake in both the still-open #2215/#2216
drift and the newly-escalated skyrim_se `entities_total` growth (RT-3) before
its cause is understood; recommended sequence is unchanged from 07-27: fix
(or at least explain) the tracked regressions first, then `--regen`.
