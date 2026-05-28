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
# regenerated: 2026-05-28
entities_total              5885
tex_missing_unique_paths    0
tex_missing_entity_count    0
mesh_cache_failed_count     0
light_count_directional     1
light_count_point           42
bench_fps_p50               48.7
bench_draw_calls_total      1183
```

See `.claude/commands/audit-runtime.md` §Phase 3 for the canonical metric
list and direction rules.

## What NOT to commit here

- `*.engine.log` / `*.telem.txt` — those live under `/tmp/audit/runtime/`
  and are purged at the end of each run.
- Per-developer-machine artifacts — baselines should be reproducible on
  any machine with the same game data, not pinned to one rig.
