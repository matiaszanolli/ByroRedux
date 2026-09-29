# #5125: RT-2026-09-29-03: runtime baselines' draw split (batches / gpu_calls / raster) moved on all five games — the renderer-static bench camera moved with `b9e961eeb`'s interior spawn ladder

**Labels**: medium, bug, performance

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-3`)
**Severity**: MEDIUM (skill Phase 4: count against direction; the baselines are now stale, no code regression shown)
**Dimension**: Telemetry diff (draw split) / baseline integrity

## Location
- `bench_draws_batches` / `bench_draws_gpu_calls` / `bench_draws_raster_cmds` rows in all five `.claude/audit-baselines/runtime/*.tsv`
- `byroredux/src/cell_loader/interior_spawn.rs` (added in `b9e961eeb`)

## Description
Baseline → current (batches, gpu_calls, raster): fnv 167→500, 36→120, 283→1284 · fo3 114→511, 12→65, 123→1285 · skyrim_se 13→643, 3→35, 14→1786 · fo4 196→723, 13→110, 256→3618 · oblivion 78→18, 5→2, 132→20 (decreased).

`renderer-static` holds the authored camera; for a direct interior load that camera sits at the player's eyes on the spawn pose. `b9e961eeb` replaced the old door-placement/bbox heuristic with a ladder: COCMarkerHeading first, then the partner door's XTEL. HEAD's engine logs show COCMarkerHeading for AtomicWrangler, MegatonPlayerHouse, WhiterunDragonsreach and InstituteBioScience, and a partner-XTEL arrival for GildedCarafe. A new pose means a new frustum, which moves the raster-visible prefix and with it batching and the indirect-call count. Bisect: the draw split is unchanged at `cb44d99f6` (= baseline, bit-for-bit) and moves at `b9e961eeb` with entity and light counts identical (Oblivion 335/78b/5c r132 → 335/18b/2c r20, FNV 2204/167b/36c → 2244/700b/122c, FO4 3969/196b/13c → 4142/894b/115c).

## Evidence
Bisect table in the report; `Interior spawn for '<cell>': …` and `bench camera 'static' … from (…)` lines in each `*.engine.log` under `/tmp/audit/runtime/`.

## Impact
None of the draw rows is a usable regression guard until regenerated. The old camera for several cells faced a wall (Oblivion subject distance 3.9 BU; Skyrim raster 14), so the old gate barely exercised raster batching. FO4's raster prefix is now 3618, which puts the renderer-static gate on the **parallel** sort branch (threshold 3000) for the first time. Any R6a-style "batching regression" read across `b9e961eeb` compares two different views (RT-4).

## Related
RT-4, RT-5, RT-6, RT-7, FNV-2026-09-29-D6-01 (#5067), PERF 2026-09-29 (R6a-regress-22 static attribution). Should land after the RT-2 fix so the Oblivion directional row is regenerated at the corrected value.

## Suggested Fix
Review a HEAD screenshot of each new pose, then regenerate all five TSVs in one renderer-static `--regen` run with a `# regenerated:` header naming `b9e961eeb` / `5570c221c` / `a070baaad` / `f87490826`. Consider recording `camera_pos` / `camera_forward` in the TSVs, or asserting them in `capture.sh`, so a pose change fails loudly instead of masquerading as a batching regression.

Validated at HEAD 9fcfdc3fc: all five TSVs still carry the pre-`b9e961eeb` draw rows (last baseline commit `c37714ba6` touched only the README); `interior_spawn.rs` present.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. camera pose recorded/asserted by the harness)
