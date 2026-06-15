# Runtime Telemetry Baselines

Per-game-per-cell baseline scalars used by
[`.claude/commands/audit-runtime.md`](../../commands/audit-runtime.md).
Each file is `<game>-<cell>.tsv`, one tab-separated `key<TAB>value` per
line with a leading `# regenerated: YYYY-MM-DD` header.

## Regeneration

After an intentional change that legitimately moves the numbers (texture
loader fix, new lighting pass, etc.), regenerate the affected baselines:

```bash
/audit-runtime --game <name> --regen
# or for the full matrix
/audit-runtime --game all --regen
```

Commit the resulting TSV diff in the SAME commit as the engine change —
reviewer needs to see "this metric moved because of THIS change."

## Schema

```
# regenerated: 2026-05-28 (post-#1284 step-2)
entities_total              5885
tex_missing_unique_paths    0
mesh_cache_failed_count     0
light_count_directional     1
skin_pool_live              686
skin_pool_max               1365
skin_pool_overflow_attempts 0
bench_fps_p50               48.7
bench_fps_avg               49.1
bench_draws_cmds            1183
bench_draws_batches         96
bench_draws_gpu_calls       9
```

The key set above mirrors the committed TSVs exactly. Earlier revisions of
this block listed `tex_missing_entity_count`, `light_count_point`, and
`bench_draw_calls_total`, which **no** committed baseline carries and the
skill's Phase 3 contract does not diff (`light.dump` surfaces only the
directional sun — no per-point tally — and the draw count is the three-way
`bench_draws_{cmds,batches,gpu_calls}` split, not a single total). See #1622.

See `.claude/commands/audit-runtime.md` §Phase 3 for the canonical metric
list and direction rules.

## What NOT to commit here

- `*.engine.log` / `*.telem.txt` — those live under `/tmp/audit/runtime/`
  and are purged at the end of each run.
- Per-developer-machine artifacts — baselines should be reproducible on
  any machine with the same game data, not pinned to one rig.
