# Runtime Telemetry Audit — 2026-08-07

**Scope**: `/audit-runtime` — drive the headless engine against each game's
representative interior cell and diff the visible-symptom telemetry (`stats`,
`tex.missing`, `mesh.cache failed`, `light.dump`, `bench:`) against the
checked-in baselines under `.claude/audit-baselines/runtime/`.

**Games run**: `fnv`, `fo3`, `oblivion`, `skyrim_se`, `fo4` — all five cells
with committed baselines, run serially per the skill's single-debug-port
contract. `starfield` was **not run**: per the skill's own candidate-cell
table, the Starfield profile in `assets/debug_profiles.toml` ships empty
`default_bsas` / `default_textures_bsas` / `sample_cells` and has no committed
runtime cell baseline — SF coverage lives in the separate `--sf-smoke`
ESM-resolve harness, out of this skill's scope.

**Build**: `cargo build --release -p byroredux -p byro-dbg` — clean, no
warnings surfaced in the audited paths.

**Method**: `xvfb-run -a --server-args="-screen 0 1280x720x24" ./target/release/byroredux --game <key> --cell <EDID> --bench-frames 240 --bench-hold`,
polled for `byro-dbg` ping, then drove `stats` / `tex.missing` /
`mesh.cache failed` / `light.dump` / `quit`. Runs were serial (one engine +
one `byro-dbg` capture at a time, default port 9876), matching the skill's
documented single-port constraint.

## Methodology notes (process, not product, findings)

Two operational issues surfaced while driving the audit; neither is a
regression in the engine itself, but both are worth recording since they
affect how future runs of this skill should be executed:

1. **`BYROREDUX_FIXED_DT=0` silently suppresses the `skin=L/M+S` telemetry
   line the skill's own Phase 3 metric table requires.** `log_stats_system`
   (`byroredux/src/systems/debug.rs`) only emits the once-per-second
   `engine::stats` line (which carries `skin_pool_live`/`_max`/
   `_overflow_attempts`) when `crosses_one_second_boundary(TotalTime, DeltaTime)`
   is true. With `FIXED_DT=0`, `DeltaTime` is 0 every frame forever, so
   `TotalTime` never advances past 0 and the boundary never fires — no matter
   how long `--bench-hold` keeps the engine open. The skill's "Determinism"
   note recommends `FIXED_DT=0` specifically "when capturing tolerance
   metrics," which is in direct tension with needing that same log line.
   **Workaround used here**: two passes per (game, cell) — a primary pass with
   `FIXED_DT=0` for `bench:`/`tex.missing`/`mesh.cache failed`/`light.dump`
   (reproduces the baseline capture conditions exactly — see below), and a
   second pass with `FIXED_DT=0.016` (still a fixed, deterministic per-frame
   delta, just nonzero) solely to harvest the `skin=` line. Cross-checked on
   FNV: the nonzero-dt pass reproduced `skin_pool_live=677` bit-for-bit
   against the frozen-dt pass, so the workaround does not compromise the
   metric. Every one of the 5 games showed the same exact-match behavior on
   `skin_pool_*` across both passes, corroborating that skin-pool occupancy is
   not sensitive to the jitter this dt difference introduces.
2. **`kill -INT $WRAPPER_PID` on the `xvfb-run` wrapper can orphan the real
   `byroredux` process.** `xvfb-run` is a shell script; its child does not
   always share the parent's PID, so signaling the wrapper PID recorded by
   `$!` sometimes leaves the actual engine process running in the background
   after the capture script believes it exited cleanly — confirmed via
   `pgrep` showing a live `byroredux` process after a "DONE" print, which then
   corrupted a subsequent run's log file (both processes had the same log
   path open, one truncated under the other mid-write). Fixed for this run by
   re-resolving the real engine PID via `pgrep -f "target/release/byroredux
   --game $GAME --cell $CELL"` before teardown and killing that PID directly,
   plus an unconditional `pkill -9 -f 'target/release/byroredux'` sweep before
   and after each capture. Confirmed clean (`pgrep` empty) after every run in
   this sweep.
3. **Heavier cells can still be mid-warmup at the skill's suggested fixed 3 s
   post-connect sleep.** FO4 `InstituteBioScience` (~12.6k entities,
   precombine-CSG-heavy) did not print its `bench:` frame-240 summary line
   within 3 s of the `byro-dbg` ping succeeding on the first attempt —
   querying at that point captured a mid-load, not a settled, state. Fixed by
   polling the engine log for the `^bench:` line (up to 120 s) before issuing
   the telemetry capture sequence. Lighter cells (Oblivion, FNV) crossed this
   within 1–2 s; FO4 needed several more.

None of these three affected the final reported numbers below — all runs were
redone (or in the FO4 case, re-verified) under the corrected methodology
before being compared against baseline.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|----------------|
| fnv | FreesideAtomicWrangler | **PASS** | entities 9271→9403 (+1.42%, in-band); tex_missing 1→1; mesh_failed 11→11; skin 677/1364/0→677/1364/0 (exact); draws 2553/89b/25c→2553/89b/25c (**bit-exact**) |
| fo3 | MegatonPlayerHouse | **PASS (1 LOW)** | entities 3311→3380 (+2.08%, marginally past ±2% band — RT-1); tex 0→0; mesh_failed 3→3; skin 0/1364/0→0/1364/0 (exact); draws_cmds 1839→1565 (−14.9%, improvement, non-gating); batches 96→83 (improvement); gpu_calls 9→9 (exact) |
| oblivion | ICMarketDistrictTheGildedCarafe | **PASS** | entities 701→704 (+0.43%, in-band); tex 0→0; mesh_failed 0→0; skin 3/1364/0→3/1364/0 (exact); draws 324/47b/4c→324/47b/4c (**bit-exact**) |
| skyrim_se | WhiterunDragonsreach | **PASS** | entities 8126→8279 (+1.88%, in-band); tex 0→0; mesh_failed 9→9; skin 83/1364/0→83/1364/0 (exact); draws 2342/9b/2c→2342/9b/2c (**bit-exact**) |
| fo4 | InstituteBioScience | **PASS** | entities 12448→12634 (+1.49%, in-band); tex 1→1; mesh_failed 0→0; skin 124/1364/0→124/1364/0 (exact); draws_cmds 3440→3344 (−2.8%, improvement); batches 753→204 (improvement); gpu_calls 42→13 (improvement) |
| starfield | — | **NOT RUN** | No committed cell baseline; profile ships empty archives/sample_cells per skill's candidate-cell table. Use `--sf-smoke` for SF coverage. |

**All five gating comparisons pass.** Three of five (fnv, oblivion,
skyrim_se) reproduced their `bench_draws_cmds`/`batches`/`gpu_calls` triple
**bit-for-bit** against the 2026-08-06 baseline regen, and `skin_pool_live`
matched exactly on all five (a stricter check than the skill's own `≤
baseline` direction rule). No structural metric regressed on any game. `Δ
bench_fps_*` and `Δ bench_frame_*_ms` are reported per-game below for
visibility only — advisory per the skill (RT-2/#1701), never gating.

### Advisory: bench_fps_* / frame-time deltas (not gating)

| Game | Cell | fps baseline → current | frame_p50/p95/max_ms (current) |
|------|------|------------------------|----------------------------------|
| fnv | FreesideAtomicWrangler | 166.1 → 181.2 | 4.71 / 6.17 / 125.91 |
| fo3 | MegatonPlayerHouse | 93.3 → 111.4 | 8.46 / 9.41 / 22.77 |
| oblivion | ICMarketDistrictTheGildedCarafe | 613.2 → 501.8 | 1.68 / 2.86 / 11.74 |
| skyrim_se | WhiterunDragonsreach | 161.9 → 165.8 | 5.43 / 6.82 / 32.55 |
| fo4 | InstituteBioScience | 68.3 → 41.1 | 24.34 / 25.85 / 35.48 |

FO4's fps drop (68.3→41.1, −40%) is the largest of the sweep, but per the
skill's RT-2 note this is headless `xvfb-run` wall-clock jitter, not a
gating signal — the structural render-load metric (`bench_draws_cmds`)
*fell* 3440→3344 on the same run, the opposite direction a real perf
regression would move. No `frame_max_ms` value approached the 2000 ms
slow-frame watchdog on any run. Not investigated further per the skill's
explicit instruction to never raise this as a regression finding; flagged
here only because the delta is large enough to be worth a human glance if
FO4 perf work is ever the session's actual focus.

## Findings

### RT-1: entities_total drift on fo3 MegatonPlayerHouse marginally exceeds the ±2% tolerance band
- **Severity**: LOW
- **Game**: fo3
- **Cell**: MegatonPlayerHouse
- **Status**: NEW
- **Baseline**: `entities_total` = 3311 (captured 2026-06-14, never regenerated since)
- **Current**: 3380 (+69, **+2.08%** — 0.08 points past the skill's ±2%
  tolerance band for this metric)
- **Description**: Per the skill's RT-3/#1705 note, `entities_total` counts
  *all* ECS entities including non-rendering bodies (collision colliders,
  ragdoll/character rig, markers), and is expected to drift benignly as
  collision/physics work lands without changing what actually renders. The
  fo3 baseline is the only one of the five that has never been regenerated
  since its initial 2026-06-14 creation (the other four were all refreshed
  2026-08-06 alongside the RT-1/#2215 and RT-2/#2216 sort-mechanism and
  skin-gate fixes), so it is the most likely of the five to show accumulated
  drift simply from being the oldest snapshot, not from anything specific to
  this cell or game.
- **Evidence**: `bench_draws_cmds` — the metric the skill designates as the
  exact render-load contract — **fell** 1839→1565 (−14.9%) over the same
  window, the opposite direction a real "more stuff got spawned and is
  rendering" regression would move. `tex_missing_unique_paths`,
  `mesh_cache_failed_count`, and all three `skin_pool_*` fields matched the
  baseline exactly. This is the same "more non-rendering bodies, not more
  rendering" signature the skyrim_se and fo4 baseline headers document as
  benign after their own investigations.
- **Impact**: None observed — this is a tolerance-metric drift note, not a
  functional regression. Flagged per the skill's explicit LOW-severity rule
  ("count drift within ±5% on a tolerance metric") since 2.08% is technically
  past the ±2% line, even though every corroborating signal points the same
  direction as the other four games' already-accepted benign creep.
- **Related**: Same mechanism as the skyrim_se baseline's documented
  `entities_total` creep (RT-2/#2216) and the general RT-3/#1705 tolerance
  design.
- **Suggested Fix**: Regenerate the fo3 baseline with `--regen` next time
  fo3-touching work lands (it is the one baseline of the five that is now
  ~2 months stale relative to the others), the same way skyrim_se/fnv/
  oblivion/fo4 were refreshed 2026-08-06. Not urgent on its own — bundle
  with the next fo3-relevant session rather than a standalone regen.

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 (RT-1) |

**No regressions in the wrong direction.** All five games with committed
runtime baselines pass every gating metric (`tex_missing_unique_paths`,
`mesh_cache_failed_count`, `light_count_directional`, all three
`skin_pool_*` fields, and the `bench_draws_*` triple). Three of five
(fnv, oblivion, skyrim_se) reproduced their entire `draws=N/Mb/Kc` triple
bit-for-bit against the 2026-08-06 baseline. `skin_pool_overflow_attempts`
is `0` on every game — no pressure on the #1284 `SkinSlotPool` cap anywhere
in the sweep. The lone LOW finding (RT-1) is a tolerance-metric drift on the
one baseline that predates the others' 2026-08-06 refresh, corroborated as
benign by every other signal in the same run, and is a bookkeeping
suggestion (regen the stale baseline) rather than a code fix.

Starfield has no runtime cell baseline to compare against (by design, per
the skill's candidate-cell table) and was not run.

---
*Generated via `/audit-runtime`. Suggest: `/audit-publish docs/audits/AUDIT_RUNTIME_2026-08-07.md`.*
