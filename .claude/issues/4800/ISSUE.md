# #4800: PERF-D2-2026-09-23b-02: The bench-of-record harness drops `bench_draws_raster_cmds`, so no capture shows which sort branch the only >3000-command scene takes

**Severity**: LOW
**Labels**: low, performance, tech-debt, bug
**Source**: docs/audits/AUDIT_PERFORMANCE_2026-09-23b.md (PERF-D2-2026-09-23b-02)

- **Severity**: LOW
- **Dimension**: Draw & Instancing (threshold verification)
- **Location**: `scripts/fsr-bench-matrix.sh:265`; emitter `byroredux/src/app_events.rs:1154`
- **Status**: NEW
- **Description**:
  - The runtime baselines carry the column but peak at 283.
  - MedTek (13,545 cmds, 1,281 batches) is the only checked-in scene above 3,000. The stepped-camera TSVs record only `draws=N/Mb/Kc`, so its raster count lies somewhere in [1,281, 13,545], on either side of `DRAW_SORT_PARALLEL_THRESHOLD`.
  - The difference at stake is ~0.15 ms at N=5k and ~0.56 ms at N=10k (the in-code table).
- **Suggested Fix**: At the next deliberate harness bump, capture `bench_draws_raster_cmds` into a column, then re-bench both sides.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
