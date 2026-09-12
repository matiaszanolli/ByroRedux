### RT-3: FO4 `InstituteBioScience` entities_total dropped just past the −2% tolerance floor

- **Severity**: LOW
- **Dimension**: runtime telemetry / ECS body-count
- **Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`
- **Status**: NEW
- **Description**: `entities_total` 19399 → 18969 (−2.22%), a small drop just past the tolerance band's lower edge. Per the skill's own note, a drop past −2% gates because it could mean entities failing to spawn — but the magnitude here (−2.22%) sits inside the LOW ±5%-drift sub-band, and the render-load evidence argues against lost content.
- **Evidence**: `bench:` line: `entities=18969 … draws=3964/248b/16c`. Committed baseline (`.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`, `entities_total 19399`, `bench_draws_cmds 3949`, `bench_draws_batches 296`, `bench_draws_gpu_calls 16`) confirms `bench_draws_cmds` 3949→3964 (+0.4%, well inside ×1.1); `bench_draws_batches` 296→248 (fell, still a pass since the gate is only "increase past ×1.1"); `bench_draws_gpu_calls` 16→16 (exact). `tex_missing_base_color` (1→1, exact), `mesh_cache_failed_count` (0→0), and `light_count_point` (685→685, exact) are all unchanged.
- **Impact**: none observed — the draw split is flat-to-improved, which is the opposite of what a "content failed to spawn" regression would show (that would drop `bench_draws_cmds` alongside `entities_total`, not hold it flat).
- **Related**: RT-2 (same-sweep sibling finding, same tolerance-band mechanism, opposite direction).
- **Suggested Fix**: low priority given the render-load evidence; if this recurs or grows on a future sweep, bisect the same way as RT-2. No action needed now beyond noting the direction for the next sweep to compare against.

## Completeness Checks
- [ ] **TESTS**: If this recurs on a future sweep and is bisected, the baseline regen commit cites the root-cause commit
