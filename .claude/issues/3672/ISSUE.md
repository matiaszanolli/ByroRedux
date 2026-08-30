# #3672 — PERF-D8-2026-08-30-01: 73–81 % of the per-NIF CPU budget runs on the main thread — the streaming worker parallelises only the cheaper 15–30 %

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D8-2026-08-30-01`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,nif-parser,nif,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3672

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `byroredux/src/streaming.rs:1163-1214` (`parse_one_nif`, worker) vs `byroredux/src/cell_loader/partial.rs:67-74` (`finish_partial_import`, main thread) · `byroredux/src/streaming_helpers.rs:515-530,653-663` (the `FinishImports` drain phase)
- **Status**: NEW
- **Description**: The two-phase pre-parse architecture (#830 → #877 → #1262 → #3089) moves `parse_nif` and the three *pool-free* imports onto a dedicated rayon pool of `available_parallelism()/2` threads (`build_stream_parse_pool`, `streaming.rs:1050-1060`). But the **mesh + collision import** — `import_nif_with_collision_and_resolver` — stays on the main thread inside `finish_partial_import`, because it needs `&mut StringPool` out of the `World` (`partial.rs:68-74`) and a `&dyn MeshResolver` backed by the archive provider. Measured on real archives, that main-thread stage is the overwhelming majority of the per-unique-NIF cost. The `FinishImports` phase drains **one import per budget unit** (`streaming_helpers.rs:654-657`), strictly serialised, so a fresh cell's whole import cost is a single-threaded queue on a 32-thread machine.
- **Evidence** (release build, per unique NIF, 3,000-file stratified sample per archive; buckets are the exact call sequence the two code paths make):

  | Archive | files / bytes | worker `parse_nif` | worker lights+emitters+anim | MAIN `summarize_collision_authoring` | **MAIN `import_nif_with_collision`** |
  |---|---|---|---|---|---|
  | `Skyrim - Meshes0.bsa` | 3000 / 302.4 MiB | 84.83 ms (23.8 %) | 10.28 ms (2.9 %) | 1.68 ms (0.5 %) | **260.18 ms (72.9 %)** |
  | `Fallout - Meshes.bsa` (FNV) | 3000 / 379.6 MiB | 40.55 ms (14.6 %) | 10.01 ms (3.6 %) | 2.04 ms (0.7 %) | **225.78 ms (81.1 %)** |
  | `Oblivion - Meshes.bsa` | 3000 / 436.7 MiB | 57.23 ms (29.7 %) | 7.20 ms (3.7 %) | — | **127.96 ms (66.5 %)** |

  The results are cached identically (`NifImportRegistry` is keyed per unique model path, `canonical_model_path_key`), so both buckets run exactly once per unique NIF — the ratio is apples-to-apples.
- **Impact**: Fresh-cell streaming latency (session start, first entry to a worldspace region, door teleports into un-warmed interiors) is dominated by a serial main-thread stage; the `N/2`-thread pool #3089 built sits idle for the majority of the work it was created to absorb. Not a frame hitch — `FrameTimeBudget` yields (`streaming_helpers.rs:607`) — but it converts a parallelisable cost into wall-clock cell-load latency that scales with core count not at all. The `partial.rs:92-99` comment ("Running the full `import_nif_scene` again here just to get the node names would double the per-NIF parse cost") shows the cost was believed to be ~1× parse; it is ~3×.
- **Related**: #830, #877, #1262, #3089 (the pre-parse parallelisation chain); PERF-D8-2026-08-30-02 (same tier, gate side); the un-fixed `flame_attach_offset` deferral at `partial.rs:92-99` is a symptom of the same boundary.
- **Suggested Fix**: The only main-thread dependency in the mesh walk is `pool.intern(texture_path)` (`walk/mod.rs:1700`, threaded through `extract_*_local(…, ctx.pool)`). Move the walk onto the worker with a **worker-local `StringPool`**, and re-intern into the `World` pool during the drain — `ImportedMaterial`'s texture slots already go through the generic `MaterialTextureSet<T>::map_ref`, so a `FixedString → String → FixedString` re-intern at the boundary is a single mapping pass over ≤22 slots per mesh rather than a re-walk. Measure first with a `--bench-hold` cell-load trace; the alternative (a lock-free interner shared across the pool) is a bigger change with an ECS-resource-access story.
> **Cross-reference**: `#3540` (Starfield frame-0 single-threaded stall). Distinct root cause, same "the load frame is single-threaded on a 16-core part" hardware-contract violation.
>
> **Hardware contract**: the project note *User Hardware* records the dev machine as a Ryzen 7950X (16c/32t) and states that a CPU bottleneck on it is by definition a bug. That is why this is filed as a defect rather than a tuning note.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
