# #3670 — PERF-D7-2026-08-30-01: one dispatch batch parses the same NIF once per cell that references it, and the main thread then throws every duplicate away

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D7-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,nif-parser,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3670

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/app_step.rs:178-182`, `byroredux/src/scene/world_setup.rs:833-836`, `byroredux/src/streaming.rs:877-908` (`queue_loads`), `byroredux/src/streaming.rs:1310-1322` (`pre_parse_cell`'s cache filter), `byroredux/src/cell_loader/partial.rs:44-50`
- **Status**: NEW (residual of #862, which is CLOSED and whose guard is intact — this is the gap the #862 design leaves open by construction, not a regression of it)
- **Description**: `cached_keys` is a **single snapshot of `NifImportRegistry`
  taken once, before the whole batch is queued**, and it is `Arc`-cloned into
  every `LoadCellRequest` (`streaming.rs:899`). The worker is one thread
  draining requests serially (`cell_pre_parse_worker`, `streaming.rs:1086`) and
  keeps **no memo of what it already parsed in this batch**: `pre_parse_cell`
  dedups `model_paths` within a cell (`HashSet`, `streaming.rs:1321`) and
  filters against the snapshot (`:1317`), but nothing filters against the
  cells earlier in the same batch. A model shared by K cells queued in one
  dispatch is therefore BSA-extracted, parsed and imported K times.

  The duplicate work is then **provably discarded**: `finish_partial_import`
  opens with #864's already-cached early-out (`partial.rs:44-50`), so every
  duplicate `PartialNifImport` the worker produced past the first is dropped
  unread. All that survives is the wall-clock and the peak RSS.
- **Evidence**: The bootstrap call site documents the worst case in its own
  comment (`world_setup.rs:830-832`): *"On initial-radius dispatch the cache is
  normally empty, so this typically returns an empty set and the worker parses
  everything"*. `stream_initial_radius` queues the whole initial radius in one
  `queue_loads` — 49 cells at `--radius 3`, 225 at the documented `--radius 7`
  ceiling — against that empty snapshot. Adjacent exterior tiles in a
  worldspace share the overwhelming majority of their statics (the #862 issue
  title itself measured ">95% cache hits on shared statics" on WastelandNV once
  the cache was warm), so K is the number of cells in the batch that reference
  a given rock/road/fence, not 1.

  The payload channel is `mpsc::channel()` — **unbounded**
  (`streaming.rs:772`) — and the main thread only blocks until the *centre*
  cell arrives (`bootstrap_waiting`, `world_setup.rs:843`). The remaining
  payloads accumulate, each holding a full `PartialNifImport` (parsed
  `NifScene` + imported meshes + collisions + embedded clip) for every model
  of its cell, K copies of each shared model resident at once.
- **Impact**: Off-main-thread CPU, but it lands on two things the engine
  measures: `StreamingTelemetry::worker_parse` (fed from
  `payload.timings.worker`) and `settle_full_detail`'s
  *"Exterior full detail settled around (x, y) in N ms"* line — the duplicate
  parses sit in front of the last cell's payload, so they directly extend
  time-to-settle on every fresh-content dispatch. Peak RSS during bootstrap
  scales with the duplication factor. Steady-state crossings are much milder
  (the snapshot covers everything the 42 resident cells already parsed, so only
  content genuinely new to the incoming column duplicates, K ≤ 7 at radius 3),
  which is why this has never surfaced as a frame spike — it is a latency and
  memory cost, not a hitch.
- **Related**: #862 (the snapshot that this is the residual of, CLOSED, guard
  intact); #864 (`finish_partial_import`'s early-out — the proof the work is
  discarded); #3038 (the shared `canonical_model_path_key` both sides use, so a
  batch-level memo can key off the same string).
- **Suggested Fix**: Give the worker a batch-local `HashSet<String>` of keys it
  has already produced this drain, consulted immediately after the
  `cached_keys.contains(&key)` check in `pre_parse_cell` and cleared when
  `request_rx.recv()` blocks (i.e. the queue is empty). Alternatively have the
  worker mutate a shared `Arc<Mutex<HashSet<String>>>` snapshot rather than
  taking a frozen clone. Before/after is measurable with the existing
  `worker_parse` summary plus a duplicate-skip counter alongside
  `skipped_cached` (`streaming.rs:1319`).
> **Cross-reference**: `#3540` (Starfield frame-0 stall) is a different mechanism (static-collider AABB build + BLAS construction) but lands on the same fresh-content dispatch window; peak-RSS symptoms may overlap.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
