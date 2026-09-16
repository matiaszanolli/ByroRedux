# #4418 — RT-2026-09-16-02: Skyrim SE `WhiterunDragonsreach` `entities_total` is +16.4 % past its band, and its "held pending bisect" status has no open tracker

**Labels**: medium,tech-debt,bug,game:skyrim

**Source**: `docs/audits/AUDIT_RUNTIME_2026-09-16.md` (RT-2)

- **Severity**: MEDIUM
- **Status**: Existing condition, **untracked** (#3553 / #3554 closed 2026-09-02 by `a2a2168f`, which kept this cell's row unchanged "pending its own bisect")
- **Dimension**: runtime telemetry / ECS body-count
- **Description**: `entities_total` 8126 → 9461 (+16.4 %) and
  `skin_pool_live` 83 → 133.

  | Sweep | entities_total | skin_pool_live |
  |-------|----------------|----------------|
  | 2026-08-30 | 9363 | 133 |
  | 2026-09-11 | 9428 | 133 |
  | today | 9461 | 133 |

  The number keeps creeping, and it has now stayed outside the gate for
  17 days. `a2a2168f` closed both issues and kept this row at its old value,
  so no open issue tracks the bisect it deferred. Until someone does it, every
  sweep re-reports the same breach.
- **Evidence**: The render-load rows are still inside their gates:
  - `bench_draws_cmds` 2342→2457 (+4.9 %, inside ×1.1)
  - `bench_draws_batches` 9→9 and `bench_draws_gpu_calls` 2→2
  - `light_count_point` 28→28
  - `skin_pool_overflow_attempts` 0
  - `mesh_cache_failed_count` 9→0 (improved)

  `bench_fps` 161.9→96.5 (−40 %) is advisory only, and FPS has been noisy
  under xvfb (#1701).
- **Suggested Fix**: File a dedicated tracker. Bisect `entities_total` on this
  cell from the 2026-08-09 regen to HEAD, using `world.owners` for a per-class
  breakdown as #4124 did for Oblivion. Once explained, `--regen` the file.
  The same regen should also pick up the `mesh_cache_failed_count` 9→0
  improvement (see RT-4).

## Completeness Checks
- [ ] **SIBLING**: fnv/fo3/fo4/oblivion regen headers already cite their creep root cause — match that here
- [ ] **TESTS**: The regen commit cites the bisected root-cause commit
