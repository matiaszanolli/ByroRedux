# Runtime Telemetry Audit — 2026-09-11

Full real-engine sweep, `--game all` (fnv, fo3, oblivion, skyrim_se, fo4,
starfield). Release build (`cargo build --release -p byroredux -p byro-dbg`)
succeeded clean. No pre-existing engine instance was running before this
sweep (`pgrep -x byroredux`/`byro-dbg`, port 9876 all clear) — safe to launch.

**Method note (read before the table).** `capture.sh` FATAL-aborted on both
attempts it was given (fnv, fo3) — see RT-1. The remaining five game/cell
pairs (fnv, fo3, oblivion, skyrim_se, fo4) were captured by hand: launch under
`xvfb-run` with `--bench-frames 240 --bench-hold`, wait for the `bench:`
summary line to appear in the engine log, then pipe
`stats / tex.missing / mesh.cache failed / light.dump / quit` through
`byro-dbg` while the engine held. Same commands, same telemetry surface the
skill defines — just without the harness's fixed timing assumption. Every
engine process was launched and torn down strictly one at a time; each
teardown was verified clean (`pgrep -x byroredux`, `pgrep -x Xvfb`, port 9876)
before the next launch. Starfield's `citycydoniamainlevel` was attempted twice
and both times aborted for memory safety before producing telemetry — see
RT-5; it carries no committed baseline in this skill's scope regardless.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| fnv | FreesideAtomicWrangler | PASS | entities 7342→7407 (+0.9%), tex_missing_base_color 1→1, mesh_cache_failed 0→0, draws 2110→2197/109→109b/26→26c, skin 217/1364+0 (unchanged) |
| fo3 | MegatonPlayerHouse | PASS (2 improved) | entities 3493→3528 (+1.0%), tex_missing_base_color 0→0, **mesh_cache_failed 3→0** (improved), draws 1581→1581/100→100b/11→11c, skin 7/1364+0 (unchanged) |
| oblivion | ICMarketDistrictTheGildedCarafe | REGRESSION (MEDIUM) | **entities 705→745 (+5.7%, past ±2% band)**; draws 325→330/20→20b/2→2c (flat/exact — confirms render-load unaffected); light_count 8pt/2dir unchanged |
| skyrim_se | WhiterunDragonsreach | KNOWN GAP reconfirmed (not new) | entities 8126(stale baseline)→9428, matches the already-recorded 2026-08-30 measurement (9363/skin_pool_live=133) almost exactly — see baseline's own `#3553`/`#3554` note; draws 2342→2457/9→9b/2→2c (flat/exact); **mesh_cache_failed 9→0** (improved) |
| fo4 | InstituteBioScience | REGRESSION (LOW) | **entities 19399→18969 (−2.2%, past −2% floor)**; draws 3949→3964/296→248b/16→16c (flat/improved — confirms no render content lost); light_count 685pt unchanged |
| starfield | citycydoniamainlevel | INCONCLUSIVE / no baseline | two attempts, both memory-safety-aborted before a `bench:` line; see RT-5 |

All five captured games: `tex_missing_base_color` exact match to baseline on
every game (fnv=1, fo3=0, oblivion=0, skyrim_se=0, fo4=1) —
**zero regressions on the strict texture-resolution gate**.
`skin_pool_overflow_attempts=0` and `skin_pool_max=1364` on all five — no
pressure on the #1284 cap anywhere. `bench_draws_batches` and
`bench_draws_gpu_calls` are exact-match or improved on all five games — the
render-load contract (the thing these draw-split gates exist to protect) held
everywhere, including on the two cells whose `entities_total` moved past
tolerance.

## Findings

### RT-1: `capture.sh`'s fixed post-launch query window is too short for the known first-frame cell-load hitch, causing zero-telemetry FATAL aborts
- **Severity**: HIGH
- **Status**: NEW
- **Dimension**: audit infrastructure (`.claude/commands/audit-runtime/capture.sh`)
- **Description**: `capture.sh` waits for `byro-dbg`'s `ping`→`pong` (up to 90 s), then sleeps a fixed 3 s and immediately fires `stats`/`tex.missing`/`mesh.cache failed`/`light.dump`/`quit`. The debug server accepts `ping` and answers it almost immediately after bind — well before the scene has actually finished loading or the render loop has produced a frame — so `up_at` reads as fast as 1–6 s regardless of how long the cell itself takes to become responsive. Both attempts made this sweep (`--game fnv --cell FreesideAtomicWrangler`, `--game fo3 --cell MegatonPlayerHouse`) hit this: `dbg up at 6s` / `dbg up at 1s`, then all four telemetry commands returned `Error: timeout waiting for engine response`, tripping the script's own entities cross-check (`MISSING stats='' bench=''`) and FATAL-aborting with **no telemetry captured at all**.
- **Evidence**: this is the exact, already-measured, already-accepted mechanism from **#3559** ("RT-13: first-frame hitch of 29s (fnv) / 10s (fo3) blocks the render thread — cell load runs on it"), closed 2026-08-31 with instrumentation only (`ad5bbbb2` added `CellLoadPhaseTimings` + `frame_max_over_p95` telemetry) and an explicit "**NOT done**: moving cell load off the render thread or chunking it" — i.e. the hitch itself was deliberately left in place as milestone-sized work. Reproduced live this sweep: FNV's manual run recorded `frame_max_ms=11736.42` (`frame_max_over_p95=722.5`) on frame 0/1; FO3's manual run logged `dt=10521.45ms` with `rof_draw_call=10503` on frame 1 (`engine::stats`, `.engine.log`). Both fall inside `capture.sh`'s 90 s ping-wait, so `up_at` reports success, but both blow straight through the fixed 3 s settle + per-command timeout the script allocates for the actual query.
- **Impact**: `capture.sh` — the harness this skill instructs auditors to use instead of hand-rolling the launch — currently cannot capture **2 of the 6** committed baselines (fnv, fo3) on a cold engine process, which is the normal case for every invocation (each `byroredux` process starts cold; this is not a one-time driver-warmup artifact — it reproduced on FO3 as the *second* engine launch of this session, with the driver already warm from the FNV run 90 seconds earlier). Every capture this sweep had to fall back to a manual launch + wait-for-`bench:`-line + manual `byro-dbg` attach, which is exactly the workaround `capture.sh` exists to make unnecessary.
- **Related**: #3559 (the underlying hitch, closed as instrumentation-only, explicitly not the render-thread fix), #3560/RT-14 (the mis-attribution defect `capture.sh` already fixed — this is a distinct gap in the same script).
- **Suggested Fix**: don't gate the settle purely on `ping`/`pong`. Either (a) poll `stats` itself in a retry loop (treating a `timeout waiting for engine response` as "not ready yet," not fatal) until a real response or a longer deadline (60–90 s) elapses, or (b) block on the engine log itself containing a `bench:` line (which only appears once `--bench-frames` frames have actually rendered) before attempting the telemetry commands. Cross-reference `bench_frame_max_ms` in the resulting telemetry so a future re-widening of the hitch is visible without re-deriving this by hand.

### RT-2: Oblivion `ICMarketDistrictTheGildedCarafe` entities_total moved past the ±2% tolerance band for the first time since its 2026-08-26 regen
- **Severity**: MEDIUM
- **Status**: NEW
- **Dimension**: runtime telemetry / ECS body-count
- **Description**: `entities_total` 705 → 745 (+5.67%), exceeding the ±2% tolerance band the skill defines for this metric (#1705/RT-3). This is the cleanest, smallest baselined cell (zero missing textures, zero mesh-cache failures both before and after), so it is an unusually legible signal.
- **Evidence**: `bench:` line: `entities=745 … draws=330/20b/2c bench_draws_raster_cmds=22 lights=10`. The render-load contract that exists specifically to distinguish "more bodies" from "more visible geometry" is untouched: `bench_draws_cmds` 325→330 (+1.5%, well inside ×1.1), `bench_draws_batches` 20→20 (exact), `bench_draws_gpu_calls` 2→2 (exact). `light_count_point`/`light_count_directional` (8/2) are also exact matches.
- **Impact**: none observed on rendering — every documented instance of this exact pattern on the other four baselines (RT-3/#1705, RT-8/#3554) turned out to be benign non-rendering body creep (collision/ragdoll/marker entities), and the draw-split evidence here points the same way. Flagged per protocol because the tolerance band is crossed, not because there is independent evidence of a real defect.
- **Suggested Fix**: `git bisect` the entity count specifically on this cell between the 2026-08-26 regen commit and HEAD, the same way the FNV/FO4/Skyrim baseline headers already did for their own creep events, to confirm which subsystem added ~40 non-rendering entities to a 705-entity interior. If confirmed benign, `--regen` this one baseline row (or the whole file) once bisected — don't fold it into the general "known creep" bucket without a citation, since this is this cell's first breach.

### RT-3: FO4 `InstituteBioScience` entities_total dropped just past the −2% tolerance floor
- **Severity**: LOW
- **Status**: NEW
- **Dimension**: runtime telemetry / ECS body-count
- **Description**: `entities_total` 19399 → 18969 (−2.22%), a small drop just past the tolerance band's lower edge. Per the skill's own note, a drop past −2% gates because it could mean entities failing to spawn — but the magnitude here (−2.22%) sits inside the LOW ±5%-drift sub-band, and the render-load evidence argues against lost content.
- **Evidence**: `bench:` line: `entities=18969 … draws=3964/248b/16c`. Baseline `bench_draws_cmds` 3949→3964 (+0.4%, well inside ×1.1); `bench_draws_batches` 296→248 (fell, still a pass since the gate is only "increase past ×1.1"); `bench_draws_gpu_calls` 16→16 (exact). `tex_missing_base_color` (1→1, exact), `mesh_cache_failed_count` (0→0), and `light_count_point` (685→685, exact) are all unchanged.
- **Impact**: none observed — the draw split is flat-to-improved, which is the opposite of what a "content failed to spawn" regression would show (that would drop `bench_draws_cmds` alongside `entities_total`, not hold it flat).
- **Suggested Fix**: low priority given the render-load evidence; if this recurs or grows on a future sweep, bisect the same way as RT-2. No action needed now beyond noting the direction for the next sweep to compare against.

### RT-4: Skyrim SE `WhiterunDragonsreach` entities_total / skin_pool_live reconfirm the already-open, deliberately-unresolved #3553/#3554 gap — not a new regression
- **Severity**: MEDIUM
- **Status**: Existing (documented in `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`'s own header; tracked at `#3553`/`#3554`, RT-7/RT-8)
- **Dimension**: runtime telemetry / ECS body-count
- **Description**: this sweep measured `entities_total=9428`, `skin_pool_live=133`. The committed baseline row (`8126`/`83`) is *deliberately* stale — its own header states the 2026-08-30 sweep already measured `entities_total=9363`/`skin_pool_live=133` and explicitly declined to regen "until that entity rise itself is bisected." Today's numbers (9428/133) match that 2026-08-30 measurement almost exactly (entities +0.7%, skin_pool_live byte-identical), i.e. this is the same standing condition holding steady, not fresh drift.
- **Evidence**: render-load contract exact-match against the committed baseline: `bench_draws_cmds` 2342→2457 (+4.9%, inside ×1.1), `bench_draws_batches` 9→9 (exact), `bench_draws_gpu_calls` 2→2 (exact). `light_count_point` 28→28 (exact), `light_count_directional` 0→0 (exact). `mesh_cache_failed_count` improved 9→0. `tex_missing_all_slots` improved 10→2, and the 2 remaining paths are the same pre-existing corrupted control-character paths already on record (`textures/\bnor` — see `AUDIT_RUNTIME_2026-06-14` RT-3) — no new missing-texture defect.
- **Impact**: no new impact; the underlying #3553/#3554 bisection remains open exactly as it was.
- **Suggested Fix**: same as the open issues already say — bisect the entities_total rise (8126/9363-range → today's 9428) before regenerating this baseline row; this sweep's contribution is confirming the value has held steady rather than continued climbing since 2026-08-30.

### RT-5: `audit-runtime` SKILL.md's Starfield guidance is stale — the CRITICAL stall it cites (#3540) was fixed and closed same-day; re-verification was inconclusive due to unrelated system memory pressure
- **Severity**: MEDIUM
- **Status**: Existing text is stale; underlying engine issue is Fixed (0c45e779, closed 2026-08-30); this sweep's re-verification attempt is Inconclusive
- **Dimension**: audit infrastructure / tech-debt (doc rot)
- **Description**: `.claude/commands/audit-runtime/SKILL.md`'s Candidate Cells table describes Starfield's `citycydoniamainlevel` stall as "unresolved as of this sync," citing the 2026-08-30 CRITICAL RT-1 finding (10-minute single-threaded stall, RSS 12→20.6 GB). That finding was filed as **#3540** and **closed the same day** (2026-08-30) with a real fix, `0c45e779` ("bound the per-frame static-BLAS recovery pass" — caps the per-frame BLAS-restore pass at 256/frame and declines it outright when the visible set projects past the residency budget). Confirmed present in this session's HEAD via `git merge-base --is-ancestor 0c45e779 HEAD`. The closing comment explicitly flags a **known, deliberately-unfixed residual**: "the load-time build/evict waste that creates the misses (95 k BLAS built, most evicted before the first frame) — that is a throughput problem," left for separate follow-up.
- **Evidence**: two attempts this sweep. Attempt 1 was aborted after 17 s (RSS 7.7 GB and climbing) purely out of caution, before checking #3540's status — that abort was premature and is not evidence of anything. Attempt 2, made after confirming the fix is present, monitored RSS/available-memory every 5 s with a safety-abort threshold: the engine reached `M28.5 static collider AABB … (94667 fixed colliders); rapier_bodies=95223` (matching #3540's original ~95k figure almost exactly) and logged `frame 0`, then RSS climbed **monotonically** 3.98 GB → 15.85 GB over 45 s without oscillating back down (unlike the old bug's documented 12↔20.6 GB oscillation) and without reaching a `bench:` line. The run was safety-aborted when system-wide available memory — already reduced to ~11 GB free / 59% swap used by this machine's *pre-existing* desktop load, independent of this test — dropped to 609 MB, which is an unacceptable risk to the user's live session to continue past.
- **Impact**: (a) the SKILL.md text will mislead a future auditor into either skipping a now-viable cell or re-litigating an already-closed CRITICAL as if it were still open; (b) this sweep cannot state with confidence whether the fix fully resolves convergence to a running frame loop on Cydonia, or whether the documented-and-accepted residual load-time throughput cost (95k BLAS built/mostly-evicted before frame 1) is simply large enough that it needs materially more time and/or memory headroom than this run could safely provide — the monotonic (non-oscillating) growth pattern observed is at least consistent with the latter, accepted explanation rather than a regression of the fixed livelock, but 45 s of observation cannot rule either way conclusively. No baseline exists for this cell in this skill's scope, so nothing here is a regression against a committed guard.
- **Suggested Fix**: update SKILL.md's Starfield row to cite #3540 as fixed (0c45e779) rather than "unresolved," and note the residual load-time-throughput caveat from the closing comment so a future sweep doesn't need to re-discover it. Re-attempt this cell's runtime capture on a machine/session with materially more free memory headroom (or after closing other memory-heavy applications) to get a conclusive convergence/non-convergence answer and, if it converges, create the first Starfield runtime baseline.

## Cleanup

All `byroredux`/`byro-dbg`/`Xvfb` processes and port 9876 verified clear after
every capture and at the end of the sweep. `/tmp/audit/runtime` retained for
this session's `.engine.log`/`.telem.txt`/`.tsv` artifacts (not deleted, in
case follow-up on RT-1 through RT-5 needs the raw evidence); baselines under
`.claude/audit-baselines/runtime/` were **not** modified (no `--regen` was
used — every finding above is reported against the existing committed
baselines).
