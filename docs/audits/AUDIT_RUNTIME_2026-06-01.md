# Runtime Telemetry Audit — 2026-06-01

Driven headless under Xvfb against the FNV regression-guard cell and the
FO4 candidate cell, immediately after the M49 work (FO4 precombined CSG
geometry rendering, commits `b93ad7a9` → `a30c088a`). Scope: **FNV**
(the one committed baseline) and **FO4** (the path M49 changes). The
other per-game candidate cells have no committed baseline and were not
run this pass.

## Per-game baseline comparison

| Game | Cell | Status | Δ vs baseline |
|------|------|--------|---------------|
| FNV  | FreesideAtomicWrangler | **PASS (M49)** · minor stale-baseline drift | entities 9250→9249 (−1), skin_pool_max 1365→1364 (−1); all M49-relevant + structural metrics unchanged |
| FO4  | InstituteBioScience    | **BASELINE CREATED** | first run (post-M49); 1722 precombine entities, 0 decode/CSG failures, 0 panics |

**Headline: M49 introduced no regression on the FNV guard, and renders
FO4 precombined geometry cleanly.** The audit also *surfaced* a real M49
bug mid-run (LOD z-fighting), which was fixed before the FO4 baseline was
captured — see RT-3.

## FNV FreesideAtomicWrangler — full diff

| metric | base | cur | verdict |
|---|---|---|---|
| entities_total | 9250 | 9249 | −1 (exact) — see RT-1 |
| tex_missing_unique_paths | 1 | 1 | PASS |
| mesh_cache_failed_count | 11 | 11 | PASS |
| light_count_directional | 1 | 1 | PASS |
| skin_pool_live | 686 | 686 | PASS |
| skin_pool_max | 1365 | 1364 | −1 (exact) — see RT-1 |
| skin_pool_overflow_attempts | 0 | 0 | PASS (healthy) |
| bench_fps_p50 / avg | 141.4 / 147.3 | 97–151 | unreliable — see RT-2 |
| bench_draws_cmds | 2722 | 2665–2921 | PASS (±10%) |
| bench_draws_batches | 104 | 104 | PASS |
| bench_draws_gpu_calls | 10 | 10 | PASS |

M49 is a **strict no-op for FNV**: `spawn_precombined_meshes` early-returns
on empty `precombined_mesh_hashes` *before* the new CSG-open path, and
Gamebryo cells carry none (the engine log emits no `PreCombined:` line).
So the two −1 deltas cannot originate from this work.

## Findings

### RT-1: FNV baseline drift (entities −1, skin_pool_max −1) — pre-M49
- **Severity**: LOW (stale-baseline drift, not a regression from the change under test)
- **Detail**: `entities_total` 9250→9249 and `skin_pool_max` 1365→1364,
  both **stable across re-runs** (so real, not measurement noise). The
  committed baseline is dated 2026-05-28 (`post-#1284 step-2`); several
  commits landed between then and HEAD (session-44 closeout + M49). The
  drift is attributable to one of those intervening commits, **not M49**
  (verified no-op above).
- **Suggested fix**: regenerate the FNV baseline against current HEAD
  once the −1 source is acknowledged: `/audit-runtime --game fnv --regen`.
  If the single-entity drop matters, `git bisect` between `c1951b89`
  (pre-M49) and `2026-05-28` on `entities_total` for FreesideAtomicWrangler.

### RT-2: `bench_fps_*` is unreliable under headless Xvfb — methodology
- **Severity**: LOW (measurement methodology, not an engine regression)
- **Detail**: FNV `wall_fps` measured 150.7, then 97.4 on back-to-back
  re-runs (same binary, same cell). Headless Xvfb has no real present
  timing and is sensitive to host contention, so the ≥0.9×baseline gate
  on `bench_fps_*` false-positives. The deterministic structural metrics
  (entities, tex.missing, mesh.cache, skin overflow, light, draw *counts*)
  are the reliable surface; treat fps as advisory under Xvfb.
- **Suggested fix**: either drop `bench_fps_*` from the headless contract,
  or average N runs. (Real-display capture would be needed for a stable
  fps number — out of scope for this guard.)

### RT-3: M49 precombine LOD z-fighting — FOUND AND FIXED this pass
- **Severity**: was HIGH (visible render corruption); **resolved** in `a30c088a`
- **Detail**: FO4 precombined floors rendered a z-fighting "comb" pattern.
  The decoder concatenated all three LOD levels (alternative triangulations
  of the same surface) into one index buffer. Fix: render only the finest
  LOD (highest triangle count, honoring its `tri_offset/3` start). Confirmed
  via headless screenshot — comb gone, same 1722 precombine entities. Draw
  batches dropped 2383→272 (fewer overlapping triangles).
- This is exactly the class of defect the runtime audit exists to catch —
  static code audits can't see overlapping triangulations.

## FO4 InstituteBioScience — new baseline (post-M49)

entities 9167, tex_missing 1 (`temp_v1_d.dds`, a benign vanilla
placeholder), mesh_cache_failed 0, skin 100/1364 overflow 0, draws
3800/272b/7c. 13 `_oc.nif` hashes → 1722 precombine entities, 79 REFRs
absorbed, **0 CSG-read / decode failures, 0 panics, 0 Vulkan errors**.
Baseline committed at `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`.

## Skill methodology fixes applied this run (for the next operator)

- **`BYROREDUX_FIXED_DT=0` suppresses the `skin=` line.** The
  `engine::stats` skin metrics emit on a wall-second crossing
  (`total.floor() != prev.floor()`, `systems/debug.rs:32`); a frozen dt
  never crosses one. Drop `FIXED_DT` (the skill's tolerance-metric tip
  conflicts with the skin_pool_* baseline contract) and keep
  `RUST_LOG=info` so `engine::stats` reaches the log.
- **Teardown must kill the engine, not the `xvfb-run` wrapper.**
  `xvfb-run` spawns the engine as a grandchild; a wrapper-PID kill leaves
  it holding debug port 9876, so the *next* game's `byro-dbg` harvests the
  stale engine's telemetry (this pass's first FO4 capture returned FNV
  data). Fix: `pkill -9 -f target/release/byroredux` pre-launch + teardown.

## Next

- `/audit-publish docs/audits/AUDIT_RUNTIME_2026-06-01.md` to file RT-1/RT-2
  (RT-3 is already fixed — no issue needed).
