# #3694 — PERF-D9-2026-08-30-06: `ScratchTelemetry` covers zero of the seven engine-binary per-frame scratches, including `draw_commands` — the largest per-frame Vec in the process

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D9-2026-08-30-06`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3694

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: Telemetry & Origin Cost
- **Location**: `byroredux/src/main.rs:248-285` (declarations), `byroredux/src/app_frame.rs:150-160` (per-frame use), consumer `byroredux/src/commands/world_info.rs:165-171`
- **Status**: NEW
- **Description**: `ScratchTelemetry` is populated exclusively by the renderer
  (`ctx.fill_scratch_telemetry(&mut tlm.rows)`); the engine binary contributes
  only the three material counters (`app_frame.rs:172-176`). Its seven own
  per-frame scratches — `draw_commands`, `water_commands`, `gpu_lights`,
  `gpu_fog_volumes`, `light_sort_scratch`, `bone_world`, `skin_offsets` — are
  all `App` fields handed to `build_render_data` as `&mut`, cleared on entry and
  refilled every frame with capacity retained, and none appears in any row.
  `draw_commands` is the input the renderer's own tracked scratches are all
  sized *from*: `gpu_instances_scratch`, `previous_models_scratch`,
  `batches_scratch` and both rigid maps are each `reserve(draw_commands.len())`.
  So the quantity that drives five reported rows is itself unreported.
- **Evidence**:
  ```rust
  // main.rs:248-285 (declarations), :572-591 (all Vec::new() / FxHashMap::default())
  draw_commands: Vec<DrawCommand>,
  water_commands: Vec<…::WaterDrawCommand>,
  gpu_lights: Vec<…::GpuLight>,
  gpu_fog_volumes: Vec<…::GpuFogVolume>,
  light_sort_scratch: Vec<(f32, …::GpuLight)>,
  bone_world: Vec<[[f32; 4]; 4]>,
  skin_offsets: rustc_hash::FxHashMap<EntityId, u32>,
  ```
  `byroredux/src/render/mod.rs:630-632` names the same set as caller-owned
  scratch: *"All scratch buffers — `draw_commands`, `gpu_lights`,
  `gpu_fog_volumes`, `light_sort_scratch`, `bone_world`, `skin_offsets` — are
  owned by the caller and cleared on entry."*
- **Impact**: The scratch cluster with the highest per-element count in the
  frame has no capacity-vs-used or wasted-bytes visibility, and neither
  `ctx.scratch` nor its "renderer not initialized yet" message tells the reader
  the report is renderer-only. Two sibling dimensions already hit this blind
  spot from the other side: `dim_1.md:100` notes that "neither `ScratchTelemetry`
  nor the `cpu_ms:` breakdown brackets `build_render_data`'s water tail", and
  `dim_1.md:75` notes no instrument covers the animation path. LOW — this is a
  visibility gap, not a defect in the scratches themselves (all seven are
  correctly reused; none reallocates per frame the way finding 02's does).
- **Related**: `PERF-D9-2026-08-30-05` (the renderer-side half of the same
  coverage question); `PERF-D9-2026-08-30-02` (the over-reserve `draw_commands`
  drives); #780 / #1066 / #2711 (the material-counter half of this resource).
- **Suggested Fix**: Push seven `ScratchRow`s from `app_frame.rs` alongside the
  existing material-counter block, and have `ctx.scratch` label the two groups
  (`renderer:` / `engine:`) so the report's scope is legible.
---

## Prioritized Fix Order

Quick wins (scratch reuse, preallocation, gate restoration, one-line guards)
before architectural changes. Nothing here requires a Vulkan render-pass,
barrier or pipeline-state restructure; the two findings that touch GPU
scheduling (`PERF-D5-…-01`, `-02`) are gate/skip changes with existing
telemetry to verify them, not restructures.

### Tier 1 — one change, disproportionate payoff

1. **Quantise the depth tiebreaker in `draw_sort_key`** into buckets so the
   per-frame draw order is stable under camera motion. Fixes
   `PERF-D4-2026-08-30-01` (instance/previous-model/indirect upload dirty
   gates start hitting) and `PERF-D5-2026-08-30-01` (TLAS returns to
   UPDATE-mode refit in steady state) with one edit. Verify with the existing
   `tlas_build_ms` timer and the instance-hash dirty counters; add a
   build-vs-update counter, which does not exist today.
2. **Close the skinning telemetry gap** (`PERF-D9-2026-08-30-03`): move or add
   a `gpu_timers` bracket so `skin_palette.comp` and the two staging copies
   are inside one. Prerequisite for sizing item 4.
3. **Fix `between_frames_ms`'s sample point** (`PERF-D9-2026-08-30-01`) — a
   one-line move out of the post-`draw_frame` `Ok` arm — and print it on the
   `cpu_ms:` line (`-04`). Without these the engine's primary triage surface
   attributes in-engine render cost to "outside the engine".

### Tier 2 — cheap, local, measurable

4. **Gate the bone-world staging copy on `pose_dirty`**
   (`PERF-D4-2026-08-30-03`). `pose_dirty` already crosses the crate boundary
   and is consumed per entity by the BLAS refit; the bone copy is the one
   consumer ignoring it.
5. **Align `batches_scratch`'s `reserve()` with its shrink hysteresis**
   (`PERF-D9-2026-08-30-02`) — reserve against the batch high-water, not
   `draw_commands.len()`.
6. **Skip `copy_depth_to_history` when no draw carries
   `MAT_FLAG_EFFECT_SOFT`** (`PERF-D5-2026-08-30-02`); the skip is
   layout-neutral. Add its bracket at the same time.
7. **Latch the volumetrics gate-off clear** (`PERF-D5-2026-08-30-05`) — the
   caustic pass 200 lines away already has exactly that latch to copy.
8. **Early-out `reemit_water_planes` on an empty water query**
   (`PERF-D1-2026-08-30-02`, one line, `QueryRead::is_empty()` exists) and
   cache the REGN ambient resolution (`PERF-D1-2026-08-30-03`).
9. **Reuse `MorphSlot`'s existing right-sized `pending_weights` buffer**
   (`PERF-D6-2026-08-30-02`) via `clear()+extend()`, and stop marking the slot
   dirty unconditionally so `flush_pending_weights`' early-out can fire.
10. **Stagger `SKINNED_BLAS_REFIT_THRESHOLD`** (`PERF-D6-2026-08-30-01`) with
    a per-entity jitter term or a per-frame rebuild cap, in one pure
    predicate — otherwise a cell's whole NPC cohort rebuilds in lockstep
    roughly every 10 s.
11. **Move the lock tracker's `held_others` materialisation after the
    `ENABLED` check** (`PERF-D1-2026-08-30-04`) — coordinate with the
    concurrent ECS audit's `ECS-D3-01`, same lines.
12. **Convert the four residual std-hash hot-path sites** (`PERF-D1-…-01`,
    `PERF-D2-…-03`), and widen `#3061`'s source-scan guard corpus so
    `texture_registry.rs` and the animation path stop being invisible to it.

### Tier 3 — ledger and guard hygiene (cheap, and these are the premises the *next* audit will trust)

13. Correct the three drifted `memory-budget.md` rows (`PERF-D5-…-03`,
    `PERF-D3-…-03`) and the four rotted comments (`PERF-D3-…-04`,
    `PERF-D5-…-04`, `-06`, `-07`, `PERF-D6-…-03`, `PERF-D2-…-02`).
14. Add the `bench_draws_raster_cmds` metric (`PERF-D2-2026-08-30-01`) so the
    parallel-sort gate becomes falsifiable; regenerate the five baselines.
15. Add `CameraUBO`'s field-order lockstep test (`PERF-D4-…-04`) — the only
    hand-duplicated GPU struct pinned by size alone.
16. Register the untracked scratch in `fill_scratch_telemetry`
    (`PERF-D9-…-05`, `-06`).
17. Re-sample `log_memory_usage` (`PERF-D3-2026-08-30-02`) from somewhere that
    executes after cells load — today the 80 % DEVICE_LOCAL warning has one
    caller, at engine init, and can never fire.

### Tier 4 — structural, schedule deliberately

18. **Interior cell load still spawns every REFR + NPC in one frame**
    (`PERF-D7-2026-08-30-02`). This supersedes `#1798`, which was closed
    measurement-only on the rationale that a resumable cursor was too large a
    change — that premise is now false: `ReferenceLoadJob` exists and two
    exterior job types already drive it under `STREAMING_APPLY_BUDGET`. The
    interior path calls the same function through a `FrameTimeBudget::
    unlimited()` wrapper.
19. **Batch-local NIF memo in the streaming worker**
    (`PERF-D7-2026-08-30-01`): a model shared by K cells in one crossing is
    extracted and parsed K times, and `finish_partial_import`'s `#864`
    early-out then discards every duplicate unread.
20. **Move `import_nif_with_collision_and_resolver` off the main thread**
    (`PERF-D8-2026-08-30-01`). Measured on three real archives, this is
    **73–81 % of per-NIF CPU** sitting on one core of a 16-core part, which
    by this project's own hardware rule is a bug. The sole main-thread
    dependency is `pool.intern(texture_path)`; a worker-local `StringPool`
    with ≤22 re-interned `MaterialTextureSet` slots at the drain removes it.
21. **Extend the dhat gate to the import tier**
    (`PERF-D8-2026-08-30-02`): a third bound file
    *crates/nif/tests/heap_allocation_bounds_import.rs*, registered in the
    existing `nif-heap-allocation-bounds` CI job. Measured peak live heap
    after parse+import is 2.0–2.3× parse-only. Add the skinning path's two
    unreserved growth sites at the same time (`PERF-D8-2026-08-30-03`).
22. **Cap and de-duplicate `MorphSlot::delta_buffer`**
    (`PERF-D3-2026-08-30-01`): mesh-static data currently allocated per
    placed entity with no residency cap, no telemetry and no ledger row,
    while the mesh itself is correctly deduped by `MeshRegistry::
    acquire_cached`.
23. **In-place retain in `PackedStorage::remove_entities_erased`**
    (`PERF-D7-2026-08-30-04`) and hoist the cinematic-retention set out of
    `unload_cells`' per-victim loop (`-05`).

---

## Appendix — dimensions that produced no findings

**None.** All nine dimensions produced at least one finding. The thinnest
were Dimension 2 (Draw & Instancing, 3) and Dimensions 6 and 8 (3 each);
Dimension 5 (GPU Pipeline) produced the most at 7. Dimension 2's own verdict
is worth recording as a positive result: sort-key ordering, per-draw dynamic
state (`cmd_set_depth_bias`, depth test/write/compare-op, `cmd_set_cull_mode`
are all `!=`-gated), descriptor-set and vertex/index binding (once per frame)
and push-constant churn (none in the batch loop) were all checked and are
clean — its three findings are about *measurability* and residual hashing,
not about batching waste.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
