# #3660 — PERF-D2-2026-08-30-01: the parallel-sort gate reads `raster_len`, but no checked-in metric measures it — the "fo4 crosses the gate" claim is unfalsifiable

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D2-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,renderer,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3660

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:548-573` (`sort_draw_commands`), `byroredux/src/render/mod.rs:818-825` (the #2691 note), `byroredux/src/bench.rs:345` + `byroredux/src/bench.rs:530-532`, `.claude/audit-baselines/runtime/*.tsv`
- **Status**: NEW
- **Description**: `sort_draw_commands` partitions RT-only occluders to the tail and then applies the
  3000-element gate to **`raster_draws = &mut draw_commands[..raster_len]`** — the in-frustum prefix.
  The only draw-volume figure any baseline records is `bench_draws_cmds`, which is
  `draw_commands.len()` — the whole array, RT-only tail included. `raster_len` is computed, returned,
  and then used for nothing but a `BYRO_PROFILE`-gated log string (`render/mod.rs:840,868`); it is
  never written into `DebugStats` (`app_frame.rs:238` sets `draw_command_count` from
  `self.draw_commands.len()`) and is not in `REQUIRED_METRICS` (`bench.rs:519-533`). Consequently the
  in-source claim that "the FO4 baseline is *above* this gate and **takes the parallel path**"
  (`render/mod.rs:823-825`), and the prior sweep's "verified INTACT" restatement of it
  (`docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`, Dimension 2 bullet 2), are inferences from a metric
  that measures a different quantity. On an interior cell with meaningful frustum culling the raster
  prefix can sit well below 3949 — nothing in the repo says whether the parallel branch has ever
  actually executed.
- **Evidence**:
  ```rust
  // byroredux/src/render/mod.rs:564-570
  const DRAW_SORT_PARALLEL_THRESHOLD: usize = 3000;
  let raster_draws = &mut draw_commands[..raster_len];
  if raster_draws.len() >= DRAW_SORT_PARALLEL_THRESHOLD {
      raster_draws.par_sort_unstable_by_key(draw_sort_key);
  ```
  vs. the producer of the recorded metric:
  ```rust
  // byroredux/src/bench.rs:345
  draws: draw_commands.len() as u32,
  ```
  The value **is** already computed — `render/mod.rs:868` prints
  `"... ({n_draws} draws, {raster_draws} raster) ..."` — but only under `BYRO_PROFILE=1`, which
  `/audit-runtime` does not set, so it never reaches a TSV.
  The in-tree calibration harness `manual_bench_draw_sort_serial_vs_parallel`
  (`byroredux/src/render/draw_sort_key_tests.rs:494-552`) also sorts a full `Vec<DrawCommand>` of
  size N with no in-raster partition, so its N axis is the same "all commands" quantity, not the
  quantity the gate reads.
- **Impact**: No runtime defect. The consequence is that a live tuning constant on the per-frame
  render path cannot be validated or invalidated from the repo's own telemetry, and two written
  records (production source and the previous audit's guard section) assert a branch selection that
  no measurement supports. The next person to touch `DRAW_SORT_PARALLEL_THRESHOLD` reasons from a
  column that is an upper bound of unknown tightness — on a heavily-culled interior it could be 2×
  the gated quantity.
- **Related**: #2691 / PERF-D2-03 (the prose this note replaced), #934 / PERF-DC-01, #2173, `883f57cd`; #516 (the in-raster/TLAS split that introduced the divergence); `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` Dimension 2.
- **Suggested Fix**: Add one row — `bench_draws_raster_cmds` — to the bench summary line and to
  `REQUIRED_METRICS` in `bench.rs`, sourced from `sort_draw_commands`'s existing return value (thread
  it into `DebugStats` alongside `draw_command_count`), then regenerate the five baselines and
  restate the note in `render/mod.rs` against that column. Until that row exists, the note should say
  the gated quantity is unmeasured rather than assert which branch fo4 takes.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
