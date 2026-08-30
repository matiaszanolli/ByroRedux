# RT-2: Skyrim `WhiterunDragonsreach` draw split regressed past the x1.1 gate on both batch axes — merge efficiency 260.2 -> 123.0 cmds/batch

**Issue**: #3548
**Labels**: bug, renderer, high, game:skyrim
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-2. Measured device telemetry (RTX 4070 Ti, release build at `64f64480`, `xvfb-run`, `--bench-frames 240`).

## Description

`skyrim_se` / `WhiterunDragonsreach` crossed the runtime baseline's x1.1 gate on **both** draw-batch axes.

| Metric | Baseline (2026-08-06) | Current | Ratio | Verdict |
|---|---|---|---|---|
| `bench_draws_cmds` | 2342 | 2460 | x1.050 | PASS |
| `bench_draws_batches` | 9 | **20** | **x2.22** | FAIL |
| `bench_draws_gpu_calls` | 2 | **4** | **x2.00** | FAIL |

`cmds` moving only +5% while batches more than double means the same geometry is being split into more, smaller batches — merge efficiency fell **260.2 -> 123.0 cmds/batch**.

## Evidence

`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` records `bench_draws_cmds 2342` / `bench_draws_batches 9` / `bench_draws_gpu_calls 2`, regenerated 2026-08-06 under RT-2/#2216.

## Note on premise — this invalidates a prior closure

That baseline file's header explicitly argues this cell does **not** share the #2215 `draw_sort_key` alpha-over mechanism that moved fnv / fo3 / fo4, and **#2351 was closed as non-reproducing on that basis**. That reasoning no longer holds: the cell has now moved on exactly those two axes.

## Impact

Batch-merge efficiency on the reference Skyrim interior halved. Coupled with RT-8 (this cell's `entities_total` rose +15.2% *while* `cmds` rose +5.0%, breaking the "more non-rendering bodies, not more rendering" defence that justifies benign entity creep elsewhere), this is the one cell where the entity rise is **not** independently benign.

## Suggested Fix

Re-open the #2215 question for this cell specifically. Bisect `byroredux/src/render/mod.rs` (`draw_sort_key` / `group_state`) between 2026-08-06 and HEAD. Small-N noise is a real possibility at 9->20 batches, so confirm with a second run before committing a fix. Hold the baseline stale until then so the evidence survives.

## Completeness Checks
- [ ] **SIBLING**: Same batch-split pattern checked on the other four baselined cells (fnv / fo3 / fo4 / oblivion)
- [ ] **TESTS**: A regression test pins this specific fix (the draw-sort-key regression tests live in `byroredux/src/render/*_tests.rs`)
- [ ] **BASELINE**: `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` regenerated only *after* the cause is understood, with the header updated to retract the #2351 non-reproducing rationale
