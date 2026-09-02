# Runtime Telemetry Baselines

Per-game-per-cell baseline scalars used by
[`.claude/commands/audit-runtime/SKILL.md`](../../commands/audit-runtime/SKILL.md).
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
# regenerated: 2026-09-02 (#3550/#3556 — RT-4/RT-10 metric-contract fixes)
entities_total	5885
tex_missing_base_color	0
tex_missing_all_slots	0
mesh_cache_failed_count	0
light_count_point	12
light_count_directional	1
skin_pool_live	686
skin_pool_max	1365
skin_pool_overflow_attempts	0
bench_fps_p50	48.7
bench_fps_avg	49.1
bench_draws_cmds	1183
bench_draws_batches	96
bench_draws_gpu_calls	9
```

The key set above mirrors the committed TSVs exactly.

- **`tex_missing_base_color` / `tex_missing_all_slots`** (#3550, RT-4) — split
  from the single old `tex_missing_unique_paths` row. `#3349` widened
  `tex.missing` from a base-color-only walk to the full 26-role
  `MaterialTextureHandles` walk; comparing the new 26-slot total against a
  1-slot baseline produced false regressions on every game. `_base_color` is
  the strict, baseline-comparable gate; `_all_slots` is informational only
  (no comparable pre-#3349 baseline exists for it yet). See
  `.claude/commands/audit-runtime/SKILL.md` §Phase 3.
- **`light_count_point`** (#3556, RT-10) — IS now diffed, contrary to what an
  earlier revision of this doc said (it used to claim `light.dump` surfaces
  only the directional sun with no per-point tally, and that no baseline
  carries this row — both stale as of the `5f970bae` per-emitter dump and
  the RT-10 regen). `light_count_directional` was **also** stale in a
  different way pre-RT-10: it used to be derived from the mere presence of a
  `CellLightingRes` block, so it always read `1` and could never fail
  (#3424). Both rows are now the real measured counts from the per-emitter
  dump.
- `bench_draw_calls_total` (a single draw-call total) never existed as a
  real row — the draw count has always been the three-way
  `bench_draws_{cmds,batches,gpu_calls}` split. See #1622.
- **`skin_pool_live`** (#3553, RT-7) is advisory, not a hard gate — it
  creeps for the same benign reason as `entities_total`. The pair that
  actually signals a `SkinSlotPool` (#1284) problem is
  `skin_pool_overflow_attempts` (`== 0`) and `skin_pool_max` (exact).

`bench_fps_p50` / `bench_fps_avg` are still stored (for the visibility Δ) but
are **advisory, not gating** as of #1701 (RT-2): the headless `wall_fps` is an
`xvfb-run` wall-clock number whose jitter dominates on small fast cells, so a
move there is reported, never raised as a regression. Only the structural
metrics gate. See `.claude/commands/audit-runtime/SKILL.md` §Phase 3 (the
advisory note) + §Phase 4.

See `.claude/commands/audit-runtime.md` §Phase 3 for the canonical metric
list and direction rules.

## What NOT to commit here

- `*.engine.log` / `*.telem.txt` — those live under `/tmp/audit/runtime/`
  and are purged at the end of each run.
- Per-developer-machine artifacts — baselines should be reproducible on
  any machine with the same game data, not pinned to one rig.
