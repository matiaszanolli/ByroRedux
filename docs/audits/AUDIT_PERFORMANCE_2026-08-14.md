# Performance Audit — 2026-08-14

**Scope**: `/audit-performance --focus 1,3` — the CPU hot-path and GPU-memory-pressure
slice, run as part of the `rt-deep` audit-suite preset (`/audit-suite rt-deep`).

| Dimension | Area | Findings |
|---|---|---|
| 1 | CPU Per-Frame Allocations & Hot Paths | 2 LOW |
| 3 | GPU Memory Pressure & Eviction Thrash | 2 MEDIUM · 2 LOW |

**Repo state**: HEAD `205744ae`, branch `main`. Dedup baseline: 2813 issues
(251 OPEN) fetched 2026-08-14.

---

## Bench-of-record note (supersedes the skill text)

`audit-performance/SKILL.md` still describes the Bench-of-record as
`R6a-stale-*`, "flagged stale and not gating". **That is out of date as of
today.** The bench was re-taken at HEAD `34074b93` under #2835, clearing
R6a-stale-19 after four consecutive folds and 186 commits of drift — it is the
first matrix reproducible on the stepped-camera harness and the first in which
all five scenes actually render. Live matrix: ROADMAP.md §"Bench-of-record
(LIVE) — stepped-camera refresh (2026-08-14, HEAD `34074b93`)". Raw rows:
`docs/audits/BENCH_stepped-camera_34074b93.tsv`.

**No bench was run for this audit, deliberately.** All six findings are static or
structural — cold paths, error paths, resize paths, and lock/hash-choice
questions — none of which is bench-differentiable at the resolution the harness
offers. Per-frame magnitudes are anchored on the checked-in
`.claude/audit-baselines/runtime/*.tsv` entity/draw counts rather than on
manufactured FPS deltas, and no absolute FPS figure appears in this report.

The skill's own trap-list applies to anyone extending this work: the live matrix
**must not** be diffed against the superseded tables above it. Three variables
moved at once (parked → stepped orbit camera, the `76373774` orbit-radius fix,
the #2834 FSR `viewSpaceToMetersFactor` correction). Prospector and FO4 Dugout
Inn were benchmarking an *empty view* before `76373774`; their apparent
regression is those scenes rendering for the first time on this harness.

---

## Executive Summary

**0 CRITICAL · 0 HIGH · 2 MEDIUM · 4 LOW.**

Dimension 1 came back essentially clean. Every Session-46 guard is intact with no
erosion — #1371 (`drain_dirty_into`, and zero production `take_dirty` callers
remain), #1372/#1725 (`AnimScratch`), #1374 (`last_cam` billboard gate), #1376
(debug-UI visibility gate — the prior audit's `format!` on the hidden path is
gone), #1379 (`next_slot` contraction), #1794 (`bone_world` no-clear), #1803
(dead probe removed), plus #2677/#2680/#1802/#2682/#1795. The two findings that
survived are both LOW and both structural rather than allocational.

Equally important, **four Dimension-1 candidates were investigated and dropped**
rather than filed — decorate-sort-undecorate (already measured and disproven
under #2681), the general per-entity lock volume in `animation_system_inner`
(that *is* the landed shape of closed #53/#271/#287), a `next_entity_id`-driven
full-scene BFS during streaming (disproved: nothing spawns entities per frame),
and per-particle `material_hash` (the same accepted per-draw cost as #1368).
They are listed in the Dimension 1 coverage block so a future audit does not
re-derive them.

The two MEDIUMs are both in Dimension 3, and both are **latent rather than
active**:

- **PERF-D3-01** is the one that can fire on the dev machine today. A swapchain
  recreate zeroes `VulkanContext::frame_counter`, which is also what the
  SkinSlot / skinned-BLAS LRU sweep measures idleness against. After a window
  resize, `should_evict_skin_slot`'s `saturating_sub` yields idle=0 permanently
  for every slot stamped before the reset — those slots are pinned against
  eviction for the process lifetime. The counter reset was added for TAA jitter;
  the LRU is a second, unnoticed reader of the same value.
- **PERF-D3-02** leaks compacted acceleration structures on both early-exit paths
  of `alloc_compact`, where the in-code comment cites an "outer cleanup loop"
  over `compact_accels` that does not exist. This is a residual gap left by
  closed #316, not a regression of it.

### Reachability caveat — read before prioritising

**PERF-D3-02 and PERF-D3-03 are unreachable on the 12 GB dev card.** Both sit
behind `static_blas_bytes + pending_bytes > blas_budget_bytes` or allocator OOM;
the dynamic budget is ~4 GB against a ~300 MB typical cell. They are defects
reasoned from source against the project's stated **6 GB RT minimum**, not
observed behaviour. **PERF-D3-04 is likewise invisible on single-heap NVIDIA
parts** — it only bites on a multi-heap DEVICE_LOCAL topology.

This is the recurring shape of GPU-memory findings on this project: the
machinery cannot be exercised on the development hardware, so it is verified
through its predicates and unit tests. Treat these as "correct by inspection,
unobserved under pressure".

### Suggested fix order

1. **PERF-D3-01** — reachable today on any resize; the fix is to give the LRU its
   own monotonic counter, or to stamp-rebase on reset, rather than sharing the
   TAA jitter counter.
2. **PERF-D3-02** — small, self-contained cleanup-loop fix; matters at the 6 GB
   target even though it cannot be hit at 12 GB.
3. **PERF-D3-04** — one-line query swap to `smallest_device_local_heap_bytes`
   (which already exists for exactly this purpose and is documented as "the one
   that fails first"). Note this is a case where the **code** is the wrong side,
   not the doc.
4. **PERF-D3-03** — extend the `pending_bytes` ledger through the compaction
   phase.
5. **PERF-D1-01 / PERF-D1-02** — hygiene; neither is a measured regression.

---

## Dimension 1



Audit: `/audit-performance` (rt-deep suite), 2026-08-14
Repo: `/mnt/data/src/gamebyro-redux`, branch `main`

## Scope & Coverage

### Files read in full (production halves)
- `byroredux/src/systems/animation.rs` — `animation_system_inner`, `make_animation_system`, `AnimScratch`, `apply_float_channels`, `apply_color_channels`, `apply_bool_channels`, `ensure_subtree_cache`, `write_root_motion`
- `byroredux/src/systems/bounds.rs` — `make_world_bound_propagation_system`, `skin_pose_dirty`, `skinned_world_bound`
- `byroredux/src/systems/billboard.rs` — `make_billboard_system`, `compute_billboard_rotation`
- `byroredux/src/systems/particle.rs` — `apply_emitter_params`, `apply_emitter_overlays`, `convert_force_fields_zup_to_yup`, `particle_system`, `integrate_force_fields`
- `byroredux/src/render/particles.rs` — `emit_particles`, `quantize_fade`
- `byroredux/src/render/mod.rs` — `build_render_data`, `draw_sort_key`, `sort_draw_commands`, `fog_height_reference`, `apply_fog_overrides`
- `byroredux/src/render/skinned.rs` — `build_skinned_palettes`, `pose_hash`
- `crates/core/src/ecs/packed.rs` — `drain_dirty_into`, `take_dirty`, `mark_dirty`
- `crates/core/src/ecs/resources/skin_slot_pool.rs` — `SkinSlotPool` (full impl)
- `crates/core/src/ecs/systems.rs` — `make_transform_propagation_system`

### Files read in part (call-site / allocation verification only)
- `byroredux/src/render/static_meshes.rs` (`collect_static_mesh_draws` main loop),
  `byroredux/src/render/lights.rs` (`collect_lights` decorate sort),
  `byroredux/src/render/fog_volumes.rs` (`collect_fog_volumes` sort),
  `byroredux/src/app_frame.rs` (`App::render_one_frame`),
  `byroredux/src/main.rs` (`build_debug_ui_snapshot`, `build_interaction_prompt`, `App` scratch fields),
  `byroredux/src/boot.rs` (scheduler registration of all five systems above),
  `crates/core/src/ecs/world.rs` + `crates/core/src/ecs/lock_tracker.rs` (query/resource acquisition cost),
  `crates/core/src/ecs/components/{transform,global_transform,hierarchy,local_bound}.rs` (`TRACK_CHANGES` opt-ins),
  `crates/core/src/animation/{stack,text_events}.rs`,
  `crates/renderer/src/vulkan/context/{mod,draw,skinned_blas_refit}.rs` (`FrameInputs.pose_dirty` consumers),
  `crates/renderer/src/vulkan/material.rs` (`MaterialTable::intern_by_hash`),
  `byroredux/src/cell_loader/work_budget.rs` (`FrameTimeBudget`, for the streaming-spawn hypothesis below).

### Bench posture
Read the **live** ROADMAP.md "Bench-of-record (LIVE) — stepped-camera refresh
(2026-08-14, HEAD `34074b93`)" block and `docs/audits/BENCH_stepped-camera_34074b93.tsv`.
**No bench was run for this dimension** and **no observed-vs-live delta is reported** —
both findings below are static/structural, so there is nothing to attribute to a
frame-time movement, and inventing one would be exactly the noise the refresh's three
traps warn against. Per-frame magnitudes below are anchored on the repo's own
checked-in `.claude/audit-baselines/runtime/*.tsv` entity/draw/skin counts, cited not
transcribed as tuning targets.

### Session 46 / follow-on guards verified INTACT (not re-proposed)
| Guard | Verified at | State |
|---|---|---|
| #1371 — `PackedStorage::drain_dirty_into` drains into a caller buffer and preserves `self.dirty` capacity | `crates/core/src/ecs/packed.rs` (`out.clear()` then `out.append`); test `drain_dirty_into_preserves_storage_capacity` present | **PASS** |
| #1371 — no `take_dirty` on a per-frame path | The only two drain call sites in the workspace are `systems/bounds.rs` and `crates/core/src/ecs/systems.rs`; **both** use `drain_dirty_into`. `take_dirty` has zero production callers. | **PASS** |
| #1372 / #1725 — `make_animation_system` persistent scratch | `AnimScratch` closure-captured; all seven buffers (`entities`, `playback`, `player_events`, `stack_events`, `seen_labels`, `channel_names`, `updates`) `clear()`+`extend`; text-event emit uses `.clone()`, not `mem::take` | **PASS** |
| #1374 — `make_billboard_system` `last_cam` gate | `last_cam: Option<(Vec3, Vec3)>` captured; early `return` before the `Billboard` query when the camera pose is bit-identical | **PASS** |
| #1376 — `build_debug_ui_snapshot` visibility gate | `app_frame.rs`: deep-clone path runs only under `self.debug_ui.as_ref().is_some_and(\|ui\| ui.visible)`; the hidden arm builds only `interaction_prompt`, which is now `Option<&'static str>` (the prior audit's PERF-D1-02(c) `format!` is gone) | **PASS** |
| #1379 — `SkinSlotPool` `next_slot` contraction | `sweep` sorts `free_list` and tail-pops while `top == next_slot - 1`, gated on `!freed.is_empty()`; `max_used_slot()` therefore shrinks after a cell unload | **PASS** |
| #1794 — `bone_world` steady-state reuse | `build_render_data` does **not** `.clear()` `bone_world` (only re-stamps slot 0); `build_skinned_palettes` uses an unconditional `resize` that truncates on shrink and identity-fills only a genuinely new tail. Tests `padding_tail_beyond_bone_count_is_left_untouched_across_frames` + `resize_grows_then_shrinks_to_the_exact_required_length` present | **PASS** |
| #1803 — dead `GlobalTransform` probe removed from `emit_particles` | `emit_particles` acquires only `ParticleEmitter` + `TextureHandle`; positions come from `em.particles.positions` | **PASS** |
| #2677 / prior PERF-D1-01 — skinned-bound refold dirty gate | `skin_pose_dirty` binary-searches the sorted+deduped `g_dirty`; test `clean_skin_is_not_refolded_when_an_unrelated_entity_moves` present | **PASS** |
| #2172 / #2680 / prior PERF-D1-02 — light + fog decorate sorts | `collect_lights` uses caller-owned `sort_scratch` + `sort_unstable_by`; `collect_fog_volumes` uses `out.sort_unstable_by` | **PASS** |
| #1802 — env-var caching on the render hot path | `apply_fog_overrides`, the `BYRO_PROFILE` probe in `build_render_data`, and the `BYRO_NO_CULL` probe in `collect_static_mesh_draws` are all `OnceLock`-cached | **PASS** |
| #2682 — `sort_draw_commands` partition self-swap | `if raster_len != index` guard present before `draw_commands.swap` | **PASS** |
| #1795 — particle fade quantization | `quantize_fade` / `COLOR_FADE_STEPS` present and applied to the colour LERP only | **PASS** |

### Checked and deliberately NOT reported
- **Decorate-sort-undecorate for `sort_draw_commands`.** Already prototyped, measured
  and **disproven** under #2681 (commit `96124f3c`); the falsification lives in-tree as
  `manual_bench_draw_sort_serial_vs_parallel`. Not re-proposed.
- **`DRAW_SORT_PARALLEL_THRESHOLD` = 3000.** Placed by the in-file crossover table
  (#2173) and re-confirmed against the baselines by #2691. Untouched.
- **Per-entity RwLock acquisition volume in `animation_system_inner` generally.** This is
  the *landed* shape of CLOSED #53 ("reduces lock acquisitions from N+NM to N+4 per
  frame" — per-entity acquisition was explicitly accepted), CLOSED #271 and CLOSED #287.
  The fixes are still in the code. Skipped per the dedup rule. Only the one
  *within-a-single-iteration duplicate* that post-dates those fixes is reported
  (PERF-D1-02).
- **Unbounded `PackedStorage::dirty` growth.** Only `Transform` and `GlobalTransform` are
  `TRACK_CHANGES` *and* `PackedStorage`; both have exactly one per-frame drainer
  registered unconditionally in `boot.rs`. `Parent` / `Children` / `LocalBound` are
  `SparseSetStorage`, where `TRACK_CHANGES` only bumps `structural_generation` and
  allocates nothing. No leak.
- **`make_transform_propagation_system`'s roots key including `world.next_entity_id()`,
  vs. `make_world_bound_propagation_system` deliberately excluding it.** Investigated as
  a candidate "full O(entities) BFS on every streaming frame" finding and **disproved**:
  (a) nothing spawns entities per frame in steady state — every production `World::spawn`
  site is cell-load / streaming / NPC-spawn, and the transient script markers are
  *components*, not entities; and (b) on the frames that *do* spawn (resumable cell apply
  under `FrameTimeBudget`), the `Parent`/`Children` structural generations in the same
  key would fire regardless, so the `next_entity_id` limb adds nothing, and those frames
  are already dominated by NIF import + GPU upload. Not a survivor.
- **Per-particle `material_hash()` in `emit_particles`.** Same per-draw material-hash
  cost the renderer already standardised on FxHash under CLOSED #1368; particles are just
  more draws, not a distinct pattern.
- **`SkinSlotPool::sweep` / `drain_pending` return `Vec`s every frame.** Verified
  allocation-free in steady state: `Vec::new()` with no pushes never allocates, and both
  `collect()` sites have a `0` lower size-hint when the doomed/pending sets are empty.

### Could not verify
- **Actual per-frame cost of either finding.** As the skill states, **dhat is a process
  singleton and only covers the NIF parse path** (`crates/nif/tests/heap_allocation_bounds.rs`);
  there is **no quantitative guard for any per-frame render/ECS site**. Neither finding
  below is an allocation regression — both are per-frame *probe-cost* items — and neither
  carries an estimated number beyond the probe counts derivable from the checked-in
  baselines.
- **Live runtime confirmation of `!accum_root_animated` frequency** in PERF-D1-02. It
  rests on the code's own comment plus FNV clip authoring, not on a measured trace; a
  headless run was not started (the user's own engine instance takes precedence for
  `byro-dbg` attachment).

---

## Findings

### PERF-D1-01: The per-frame skinning path is the residue of the #1368 / #2174 SipHash sweep — five `SkinSlotPool` maps, `skin_offsets`, and the `pose_dirty` set the renderer is contractually forced to keep SipHashed

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs` — `SkinSlotPool` fields `entity_to_slot`, `last_seen_frame`, `last_pose_hash`, `pose_dirty`, `rollback_pose_hash`; `byroredux/src/main.rs` — the `App::skin_offsets` field; `byroredux/src/render/mod.rs` — the `skin_offsets` parameter of `build_render_data`; `byroredux/src/render/static_meshes.rs` — the `skin_offsets.get(&entity)` probe in `collect_static_mesh_draws`; `crates/renderer/src/vulkan/context/draw.rs` — the `pose_dirty` field of `FrameInputs`
- **Status**: NEW
- **Description**: The repo has twice decided that std's SipHash-1-3 default is the wrong
  hasher for maps probed once per draw or once per entity per frame, and twice fixed it —
  #1368 (per-draw `material_hash` + the descriptor dirty-gates, now `rustc_hash::FxHasher`
  / `FxHashMap`) and #2174 (`previous_rigid_models` / `current_rigid_models_scratch`, moved
  to `FxHashMap` with the guard test `rigid_motion_history_maps_are_not_siphash`). Both
  sweeps stopped at the `crates/renderer` boundary. The skinning path — which crosses that
  boundary — was never revisited, and every one of its per-frame maps is still a std
  `HashMap` / `HashSet`:

  Per frame, with `S` = live skinned entities and `D` = mesh entities surviving to the
  draw-emit block:
  - `skin_offsets.get(&entity)` in `collect_static_mesh_draws` fires for **every** mesh
    entity, not just skinned ones — `D` probes, of which `D − S` are misses.
  - `build_skinned_palettes` adds `S` inserts (Pass 1) + `S` probes (Pass 3) on the same map.
  - `SkinSlotPool::allocate` probes `entity_to_slot` and inserts into `last_seen_frame`
    once per skinned entity (`2S`).
  - `try_mark_pose_dirty` probes `last_pose_hash` once per skinned entity, plus an `entry`
    on `rollback_pose_hash` and an insert into `pose_dirty` for each dirty one.
  - `sweep` iterates the whole `last_seen_frame` map every frame.
  - On the renderer side, `record_skinned_blas_refit` calls `pose_dirty.contains(&entity_id)`
    per skinned BLAS entry — inside the crate that already imports `FxHashMap`, but forced
    back onto SipHash because `SkinSlotPool::pose_dirty()` returns
    `&std::collections::HashSet<EntityId>` and `FrameInputs.pose_dirty` pins that exact type
    in its public signature.

  Against the repo's own checked-in baselines that is roughly `D + 5S` SipHash probes per
  frame on this path alone: `2553 + 5×677` on `fnv-FreesideAtomicWrangler.tsv` and
  `3440 + 5×124` on `fo4-InstituteBioScience.tsv`. #2174 was landed on a materially
  smaller argument shape ("probed twice per rigid draw per frame") and rated the same LOW.
- **Evidence**:
  ```rust
  // crates/core/src/ecs/resources/skin_slot_pool.rs — all five, std default hasher
  entity_to_slot:     std::collections::HashMap<EntityId, u32>,
  last_seen_frame:    std::collections::HashMap<EntityId, u64>,
  last_pose_hash:     std::collections::HashMap<EntityId, u64>,
  pose_dirty:         std::collections::HashSet<EntityId>,
  rollback_pose_hash: std::collections::HashMap<EntityId, Option<u64>>,

  // and the accessor that pins the type across the crate boundary:
  pub fn pose_dirty(&self) -> &std::collections::HashSet<EntityId> { &self.pose_dirty }
  ```
  ```rust
  // byroredux/src/render/static_meshes.rs — inside collect_static_mesh_draws' per-entity loop
  let bone_offset = skin_offsets.get(&entity).copied().unwrap_or(0);
  ```
  ```rust
  // crates/renderer/src/vulkan/context/draw.rs — FrameInputs
  pub pose_dirty: &'a std::collections::HashSet<EntityId>,
  ```
  The precedent and the remedy are both already in tree:
  `crates/renderer/src/vulkan/context/mod.rs` declares
  `previous_rigid_models: FxHashMap<u32, [f32; 16]>` and pins it with
  `rigid_motion_history_maps_are_not_siphash`; `rustc-hash` is already a workspace
  dependency (`Cargo.toml`).
- **Impact**: Bounded and small — a constant-factor probe cost on the skinning and
  static-draw loops, no allocation change and no correctness effect. It matters because it
  is the last cluster of the pattern two closed issues removed everywhere else, and because
  the existing guard test only pins the two renderer maps, so nothing would flag a *third*
  std map being added here. **No quantitative guard exists for this site** — dhat covers
  only the NIF parse path, so this cannot be bounded by a test today, only counted.
- **Related**: #1368 (CLOSED, FxHash on the render hot path), #2174 (CLOSED, the identical
  fix for the rigid motion-history maps, with its guard test), #1195 / #1196 (the
  `pose_dirty` gate these maps implement), #1379 (the `next_slot` contraction in the same
  `sweep`).
- **Suggested Fix**: Move all five `SkinSlotPool` collections and `App::skin_offsets` to
  `rustc_hash::FxHashMap` / `FxHashSet` (adding the existing workspace `rustc-hash` dep to
  `crates/core` and `byroredux`), change `SkinSlotPool::pose_dirty()`'s return type so
  `FrameInputs.pose_dirty` can follow, and extend
  `rigid_motion_history_maps_are_not_siphash` — or add a sibling in
  `skin_slot_pool.rs` — to cover the new fields so the pattern stays pinned.

---

### PERF-D1-02: `animation_system_inner` takes the `Transform` write query twice per animated entity per frame — the second acquire post-dates #53's per-entity batching and is separated from the first only by a usually-no-op call

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/animation.rs` — `animation_system_inner`, the Phase-2 per-playback-entity body: the transform-channel block, `write_root_motion`, and the accum-root grounding block that follows it
- **Status**: NEW
- **Description**: CLOSED #53 restructured this system specifically so that per-entity
  component queries are *acquired once and held for the whole batch* ("float/color/visibility
  channel queries now held for entire channel batch per entity instead of re-acquired per
  channel"). The accum-root grounding block — added later, for the Gamebryo accum/non-accum
  model — re-acquires `world.query_mut::<Transform>()` **after** the batch guard has already
  been dropped, so the common path now takes the `Transform` write lock twice per animated
  entity per frame. The only statement between the two acquisitions is
  `write_root_motion(world, entity, root_motion)`, which itself early-returns on
  `motion == Vec3::ZERO` — the ordinary case for a non-locomoting actor — and so is usually
  a no-op function call separating two acquisitions of the same lock.

  The second acquire fires on the **common** branch, not a rare one: the block is gated on
  `!accum_root_animated`, and the code's own comment states the reason it exists is that
  "most idle clips animate only `Bip01 NonAccum` … and leave the accum root untouched".

  Each ECS query acquisition is not free even in release builds: `World::query_mut` does a
  `TypeId` probe into `storages`, and `lock_tracker::track_write` / `untrack_write` do two
  more `TypeId` probes each into a thread-local `HashMap` (all std default hasher), around
  the real `RwLock`. So this is ~5 `TypeId` hash probes plus a write-lock round-trip per
  animated entity per frame, purchased for a single `Vec3::ZERO` store.

  A secondary instance of the same shape sits immediately above it: `ensure_subtree_cache`
  acquires and drops `SubtreeCache` via `try_resource`, and the very next statement acquires
  `SubtreeCache` again for the `scoped_map` lookup — two resource acquisitions per entity per
  frame where the first could hand its guard forward.
- **Evidence**: `animation_system_inner`, Phase 2, per playback entity:
  ```rust
  let mut transform_query = world.query_mut::<Transform>().unwrap();   // acquire #1
  for (channel_name, channel) in &clip.channels { /* … */ }
  drop(transform_query);

  write_root_motion(world, entity, root_motion);   // early-returns when motion == Vec3::ZERO

  if !accum_root_animated {                        // the common branch, per the code's comment
      if let Some(accum_entity) = clip.accum_root_name.as_ref().and_then(&resolve_entity) {
          if let Some(mut tq) = world.query_mut::<Transform>() {       // acquire #2
              if let Some(t) = tq.get_mut(accum_entity) { t.translation = Vec3::ZERO; }
  ```
  and, a few lines earlier in the same iteration:
  ```rust
  ensure_subtree_cache(world, root);                       // acquires+drops SubtreeCache
  let subtree_ref = world.try_resource::<SubtreeCache>();  // acquires SubtreeCache again
  ```
- **Impact**: One extra `RwLock` write acquisition plus ~5 `TypeId` hash probes per animated
  entity per frame, scaling with actor count (`skin_pool_live` is 677 on the checked-in
  `fnv-FreesideAtomicWrangler.tsv` baseline, 124 on `fo4-InstituteBioScience.tsv`). No
  correctness effect and no allocation. Reported at LOW because the magnitude is small; it is
  reported at all because it is a measurable *erosion of a landed invariant* — #53's whole
  point was one guard per entity per component — rather than a new proposal, and because
  taking the same write lock twice inside one loop iteration is also a wider surface for the
  ABBA hazards the surrounding comments (#313 / #827 / #1410) go to some length to avoid.
  **No quantitative guard exists for this site.**
- **Related**: #53 (CLOSED — the per-entity batching this erodes; note its landed shape
  *does* accept one acquisition per entity, which is why only the duplicate is reported),
  #271 / #287 (CLOSED — the same consolidation for the `AnimationStack` path),
  #2400 (OPEN — `animation_system_inner` holding `AnimationClipRegistry` + `NameIndex` read
  guards across every component acquisition; same function, concurrency angle, distinct
  issue), #1372 / #1725 (the `AnimScratch` guards, intact).
- **Suggested Fix**: Move the accum-root grounding into the still-live `transform_query`
  guard and run `write_root_motion` after that block closes — a pure reordering that
  introduces no new held-while-acquiring lock edge, since `RootMotionDelta` would then be
  taken with nothing else held. Separately, have `ensure_subtree_cache` return the
  `SubtreeCache` read guard (or fold the "build if missing" step into the existing read) so
  the resource is acquired once per entity rather than twice.

---

## Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | PERF-D1-01, PERF-D1-02 |

Every Session-46 guard and every follow-on guard listed in the Dimension-1 checklist is
intact; no erosion was found in any of them. Both findings are constant-factor probe-cost
items on per-entity/per-draw loops, not allocation regressions — and, as the skill's own
posture requires stating, **no quantitative guard exists for any per-frame render/ECS
allocation or probe site**, so neither carries an estimated number beyond probe counts
derived from the repo's checked-in runtime baselines.

---

## Dimension 3



Audit: `/audit-performance` (rt-deep suite) · Dimension 3 · 2026-08-14
Repo HEAD context: `main`, post-`205744ae` working tree.

## Scope & Coverage

### Files read in full
- `crates/renderer/src/vulkan/acceleration/predicates.rs`
- `crates/renderer/src/vulkan/acceleration/constants.rs`
- `crates/renderer/src/vulkan/acceleration/blas_static.rs`
- `crates/renderer/src/vulkan/acceleration/memory.rs`
- `crates/renderer/src/vulkan/acceleration/mod.rs`
- `crates/renderer/src/deferred_destroy.rs`
- `byroredux/src/cell_loader/nif_import_registry.rs`
- `docs/engine/memory-budget.md` (authoritative ceilings — cited, not transcribed)

### Files read in part (grep-targeted)
- `crates/renderer/src/texture_registry.rs` (`check_slot_available`, `live_slot_count`, `dead_slot_count`, `staging_pool`, `pending_destroy`, `tick_deferred_destroy`)
- `crates/renderer/src/mesh.rs` (`check_pool_growth`, `accumulate_global_geometry`, `VERTEX_POOL_*`/`INDEX_POOL_*`, `MAX_MESH_SLOTS`)
- `crates/renderer/src/vulkan/buffer.rs` (`StagingPool`, `DEFAULT_STAGING_BUDGET_BYTES`, `release_to`, `impl Drop for GpuBuffer`)
- `byroredux/src/asset_provider/material.rs` (`resolve_bgsm`, `resolve_bgem`, `MAX_BGEM_CACHE_ENTRIES`, `MAX_FAILED_PATHS`, `bgem_cache_order`, `failed_paths_order`)
- `crates/renderer/src/vulkan/context/draw.rs` (eviction + shrink call sites, `tick_deferred_destroy` site, end-of-frame block)
- `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`, `recreate_swapchain_core`)
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (SkinSlot LRU sweep, `pending_skin_unload_victims` drain)
- `crates/renderer/src/vulkan/skin_compute.rs` (`SkinSlot`, `should_evict_skin_slot`)
- `crates/renderer/src/vulkan/device.rs` (`total_device_local_bytes`, `smallest_device_local_heap_bytes`)
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` (`drop_skinned_blas`, `has_skinned_blas`)
- `crates/renderer/src/vulkan/acceleration/tests.rs` (predicate guard coverage)
- `ROADMAP.md` Bench-of-record (LIVE) block; `docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`, `AUDIT_PERFORMANCE_2026-08-07.md` skimmed for overlap.

### Checklist items verified intact (regression guards — NOT re-proposed)
| Guard | Status |
|---|---|
| **Dynamic BLAS budget** — `compute_blas_budget` = `total_device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES` (256 MB). No static "1 GB" figure anywhere in the live code. | Intact (but see PERF-D3-04 on *which* heap figure) |
| **#1792 mid-batch routing** — `evict_unused_blas`'s early-return gate *and* its per-candidate loop break both call `blas_over_budget(static_blas_bytes, pending_bytes, blas_budget_bytes)`; `build_blas_batched` passes the real accumulated `pending_bytes` at the `BATCH_EVICTION_CHECK_INTERVAL` site and `0` at the three no-batch-context sites (`build_blas`, the pre-batch call, `draw.rs`). Pinned by `blas_over_budget_accounts_for_pending_bytes`. | Intact, no erosion |
| `should_evict_mid_batch` 90% early-warning line, integer-math (`×10 >= ×9`); `BATCH_EVICTION_CHECK_INTERVAL` = 64. Pinned by `should_evict_mid_batch_fires_at_ninety_percent`, `evict_predicate_uses_static_bytes_not_total_post_920`. | Intact |
| **LRU victim = smallest `last_used_frame`** — `candidates.sort_unstable_by_key(|&(_, frame, _)| frame)`; `MIN_IDLE_FRAMES = MAX_FRAMES_IN_FLIGHT + 1` is LRU policy only (#1449 made the deferred queue the safety mechanism). | Intact |
| **#1430 BGSM/BGEM half-eviction** — `resolve_bgem` / `resolve_bgsm` drain `MAX_BGEM_CACHE_ENTRIES / 2` and `MAX_FAILED_PATHS / 2` from the insertion-order `bgem_cache_order` / `failed_paths_order` `VecDeque`s. No full-flush path exists; map↔deque stay 1:1 (the only bypassing insert, `insert_bgem_for_test`, is `#[cfg(test)]`). | Intact |
| **Deferred-destroy countdown** — `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`, derived not literal; `tick` decrements-then-destroys-at-zero; ticked in `draw_frame` after the fence wait for `MeshRegistry`, `TextureRegistry` and `AccelerationManager`. Pinned by `default_countdown_survives_max_frames_in_flight_ticks`. Three production users (`pending_destroy_blas`, `pending_destroy_scratch`, mesh) all on the countdown. | Intact |
| **Scratch shrink floors** — `scratch_should_shrink` (2× + `BLAS_REBUILD_SLACK_BYTES` 16 MB), `tlas_scratch_should_shrink` (2× + `TLAS_SCRATCH_SLACK_BYTES` 256 KB), `tlas_instance_should_shrink` (2× + `TLAS_REBUILD_SLACK_BYTES` 1 MB) with `WORKING_SET_FLOOR` = `MIN_TLAS_INSTANCE_RESERVE` = 8192 applied via `working_set.max(WORKING_SET_FLOOR)`. `shrink_blas_scratch_to_fit` walks the **union** peak (`shared_blas_scratch_peak`, #2460). | Intact |
| **`MeshRegistry` caps** — `check_pool_growth` is called from `accumulate_global_geometry`, the single funnel for both `upload_scene_mesh` and `upload_scene_mesh_global_only`; soft caps `warn!` once (`Once`), hard caps `bail!`. `VERTEX_POOL_*`/`INDEX_POOL_*` match `memory-budget.md` exactly. | Intact |
| **`NifImportRegistry`** 2048-entry LRU, `BYRO_NIF_CACHE_MAX` override, `=0` → `warn!`; eviction is `#[must_use]`-returning freed clip handles (#863). | Intact |
| **`TextureRegistry`** staging cap `DEFAULT_STAGING_BUDGET_BYTES` = 128 MB with `trim_to` auto-trim; all three `release_to` call sites pass allocation capacity, not upload size (#1921/#1954 fix present); `check_slot_available` 90% one-shot warning with `live`/`dead` split (#2030). | Intact |

### Known-and-documented, deliberately NOT re-reported
- **#1793** (CLOSED, documented-not-fixed): no rebuild path for an evicted static BLAS, and the shared `frame_counter` bump in `build_blas_batched` ageing not-yet-drawn entries during a synchronous multi-cell `--grid` burst. Both comment blocks are still present verbatim at the `frame_counter.wrapping_add(1)` site. Both gated behind `static_blas_bytes > budget`.
- **REN-D1-03** (sibling agent, renderer Dim 1): `shrink_tlas_scratch_to_fit`'s live-slot arm reallocates at bare `peak` with no `scratch_alignment_padding`, and destroys before allocating. Confirmed present at the same symbol; referenced only, not re-reported. Interacts with OPEN #2774.

### Could NOT verify — and why
- **Everything gated on `static_blas_bytes + pending_bytes > blas_budget_bytes` is unreachable on the 12 GB dev card.** `compute_blas_budget` yields ~4 GB there against a typical-cell BLAS footprint of ~300 MB (`memory-budget.md` § VRAM Rough Budget). Every eviction-behaviour claim below is therefore reasoned from source, not observed: PERF-D3-02 and PERF-D3-03 both live on allocation-failure / at-budget paths that the dev hardware does not reach. They matter on the documented 6 GB RT-minimum target (budget 2 GB), not on the RTX 4070 Ti.
- **No bench was run.** This dimension has no bench-differentiable claim (all findings are on cold/error/resize paths, none on the steady-state frame), and the "no parallel engine launch" rule applies. The LIVE `34074b93` stepped-camera Bench-of-record block in `ROADMAP.md` was read for posture; no absolute FPS is cited and no diff against the superseded tables was attempted.
- **`TextureRegistry` slot exhaustion (#2030)** is a session-length effect requiring repeated cell revisits with a live device; verified as *documented and instrumented*, not as *observed*.

---

### PERF-D3-01: Swapchain recreate zeroes the frame counter the SkinSlot / skinned-BLAS LRU sweep measures idleness against
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`, the `self.frame_counter = 0;` reset), `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit` — the `let now = self.frame_counter as u64;` sweep and the `slot.last_used_frame = self.frame_counter as u64;` stamp), `crates/renderer/src/vulkan/skin_compute.rs` (`should_evict_skin_slot`)
- **Status**: NEW
- **Description**: `VulkanContext::frame_counter` is a single `u32` serving two unrelated roles. It drives the Halton TAA jitter index, and #913 therefore resets it to `0` on every swapchain recreate so the first post-resize frame's jitter aligns with the freshly-allocated TAA history. It is *also* the clock the M29 `SkinSlot` / skinned-BLAS LRU sweep uses: `SkinSlot.last_used_frame` is stamped from it, and the sweep computes idleness as `current_frame.saturating_sub(last_used_frame)`. After a reset, every resident slot carries a `last_used_frame` from the pre-reset epoch, so the subtraction saturates to `0` and `should_evict_skin_slot` returns `false` for all of them. Nothing in `recreate_swapchain_core` / `recreate_screen_passes` re-bases `SkinSlot.last_used_frame`. Slots whose entity is still drawn re-stamp themselves on the next frame and self-heal; slots whose entity stops being drawn around the resize are never re-stamped and become **un-evictable until `frame_counter` climbs back past their stale stamp** — i.e. for as many frames as had already elapsed in the session.
- **Evidence**:
  - `resize.rs`, inside `recreate_screen_passes`: `self.frame_counter = 0;` with a comment scoped entirely to Halton jitter + TAA history (#913 / REN-D7-NEW-07). No other consumer is mentioned.
  - `skinned_blas_refit.rs`: `let min_idle = MAX_FRAMES_IN_FLIGHT as u64 + 1; let now = self.frame_counter as u64;` then `should_evict_skin_slot(slot.last_used_frame, now, min_idle)`.
  - `skin_compute.rs`: `let idle = current_frame.saturating_sub(last_used_frame); idle >= min_idle` — saturating, so a stamp in the future yields `0`, never a large value.
  - `draw.rs` is the sole bump site (`self.frame_counter = self.frame_counter.wrapping_add(1);`, once per `draw_frame`), so the counter is genuinely a frame clock everywhere else.
  - Contrast: `AccelerationManager::frame_counter` (the static-BLAS LRU clock) is a *separate* field and is **not** reset by resize — static BLAS eviction is unaffected, which is why this is a skinned-path-only defect.
  - Secondary, self-healing: a slot dispatched on the very first post-reset frame gets `last_used_frame == 0`, which `should_evict_skin_slot` treats as the "never dispatched" sentinel and skips.
- **Impact**: GPU-resource retention proportional to "skinned actors alive but off-screen at resize time" × session length. Each stranded entry is a `SkinSlot` (output buffer at `SKIN_OUTPUT_STRIDE_BYTES` × vertex count, plus `MAX_FRAMES_IN_FLIGHT` descriptor sets from the `FREE_DESCRIPTOR_SET` pool) plus its per-entity skinned BLAS. This does not compound per frame and is bounded by the actor population, so it is not a HIGH-severity leak — but the pressure it applies lands on the two ceilings that already have observed failure modes: `SKIN_MAX_SLOTS` (#1284 — exhaustion drops actors to bind-pose with no RT shadows) and the skin descriptor pool (#900). Note also that `pending_skin_unload_victims` (#1003) still drains correctly, so *despawned* entities are unaffected; only the idle-policy arm stalls. Resizes are not rare on this path — window resize, and `set_upscaler_mode` (FSR preset change) both reach the reset.
- **Related**: #913 (introduced the reset, for TAA only), #643 / MEM-2-1 (introduced the sweep), #2494 (hoisted the sweep out of the vertex-buffer guard), #1003 (`pending_skin_unload_victims`), #1284 / #900 (the ceilings this pressures). No existing OPEN or CLOSED issue covers the interaction.
- **Suggested Fix**: Give the LRU sweep its own monotonic counter that `recreate_*` never touches (mirroring `AccelerationManager::frame_counter`), or — cheaper — re-base every `SkinSlot.last_used_frame` to `0`… `min_idle` at the reset site so the sweep sees them as immediately-agable rather than future-stamped. Whichever is chosen, add a note at the `self.frame_counter = 0;` line naming the second consumer, since the current comment actively implies TAA is the only one.

---

### PERF-D3-02: BLAS compaction's allocation loop leaks every already-created compacted acceleration structure on either early exit
- **Severity**: MEDIUM
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` — the `alloc_compact` closure inside `build_blas_batched` (Phases 5+6)
- **Status**: NEW (residual gap left by CLOSED #316; the closure that #316 added is present and still runs — this is not a regression of it)
- **Description**: `alloc_compact` builds a local `Vec<CompactedBlas>` (`compact_accels`), pushing one `(mesh_handle, vk::AccelerationStructureKHR, GpuBuffer, …)` tuple per mesh. It has two failure exits, and neither destroys the tuples already pushed:
  1. `let compact_buffer = GpuBuffer::create_device_local_uninit(…)?;` — the `?` unwinds the closure with `compact_accels` holding `i` entries.
  2. The `create_acceleration_structure` `Err(e)` arm destroys only *this* iteration's `compact_buffer` and then `anyhow::bail!`s, again with `i` entries already pushed.
  In both cases `compact_accels` is simply dropped. `GpuBuffer` has a `Drop` safety net (#656) that reclaims the backing buffer, but `vk::AccelerationStructureKHR` is a raw handle with **no `Drop` impl at all** — the same reasoning #2481 / AS-D1-NEW-02 spells out for `BlasEntry` in `build_blas`. Every one of those `i` acceleration structures leaks for the process lifetime. The outer error handler (which does destroy `prepared` and the `query_pool`) cannot help: `compact_accels` never escapes the closure on the error path.
- **Evidence**: the in-loop comment on exit (2) states *"Buffer was created in this iteration but not yet pushed into `compact_accels`, so the outer cleanup loop won't see it — destroy it locally before bubbling so the OOM path is leak-free."* There is no outer cleanup loop over `compact_accels`; the closure's return type is `Result<(Vec<CompactedBlas>, u64, u64)>` and the `Err` arm at its call site only iterates `prepared`. The comment describes a cleanup that does not exist, which is why the gap survived #316. No test in `crates/renderer/src/vulkan/acceleration/tests.rs` touches the compaction rollback (grep for `compact` there returns only flag-drift and instance-map tests).
- **Impact**: A leak on the exact error path that memory pressure produces — an allocator failure during compaction leaves the pool *more* exhausted than before, so a retry on the next cell load fails earlier and leaks again. Positive feedback under sustained pressure. Blast radius is bounded by batch size (one leaked AS per already-compacted mesh in the failing batch), and `total_blas_bytes` / `static_blas_bytes` never see these bytes, so the leak is invisible to `blas_budget_bytes`, to the eviction predicate, and to `ctx.scratch` / `tex.stats` telemetry. Secondary consequence on the same path: the `GpuBuffer::Drop` safety net carries `debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first")`, so a debug build panics once per stranded buffer while unwinding an OOM it was meant to recover from.
  **Reachability**: `create_device_local_uninit` only fails on allocator OOM. Unreachable on the 12 GB dev card (BLAS budget ~4 GB vs. a ~300 MB typical cell); this is a 6 GB-RT-minimum-target defect, which is precisely the population `compute_blas_budget`'s floor exists to serve.
- **Related**: #316 (D2-02 — the closure-based rollback this is the residual of), #2481 / AS-D1-NEW-02 (the "a raw `vk::AccelerationStructureKHR` has no `Drop`" precedent in the same file), #1097 / REN-D8-003 (the equivalent, and complete, Phase-1 rollback over `prepared`).
- **Suggested Fix**: Hoist `compact_accels` out of the closure (or return it in the `Err` payload) so the existing outer rollback can walk it, destroying each `compact_accel` via `accel_loader.destroy_acceleration_structure` and each buffer via `GpuBuffer::destroy` — mirroring the `copy_result` failure arm below it, which already does exactly this correctly. Add a source-level test in the file's `tests.rs` pinning that both early exits are preceded by a `compact_accels` cleanup, as `#1812`/`#2494` do for their own ordering invariants.

---

### PERF-D3-03: Mid-batch eviction's `pending_bytes` ledger stops at Phase 1, so the batch's real peak — originals plus compaction destinations, both live at once — is never tested against the budget
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` (`build_blas_batched` — the `pending_bytes` accumulator and the `alloc_compact` closure), `crates/renderer/src/vulkan/acceleration/predicates.rs` (`should_evict_mid_batch`, `blas_over_budget`)
- **Status**: NEW
- **Description**: `pending_bytes` accumulates `sizes.acceleration_structure_size` for the *uncompacted* Phase-1 result buffers only, and is the last value the budget ever sees for this batch. Phase 5+6 (`alloc_compact`) then allocates a **second** full set of buffers — one compacted destination per mesh — while every Phase-1 original is still live; the originals are not destroyed until Phase 7, after the compaction copy submission retires. Real peak static-BLAS residency during a batch is therefore `static_blas_bytes + total_before + total_after`, but the guard only ever tests `static_blas_bytes + total_before`. There is also no `should_evict_mid_batch` / `evict_unused_blas` call anywhere inside `alloc_compact`, so the interval-based check that exists for Phase 1 has no counterpart during the phase that actually pushes residency to its maximum.
- **Evidence**: `pending_bytes = pending_bytes.saturating_add(sizes.acceleration_structure_size);` sits in the Phase-1 loop and goes out of scope before `alloc_compact` is defined. `alloc_compact` computes `total_before` (sum of `prepared[i].buffer.size`) and `total_after` (sum of `compacted_sizes`) purely for the closing `log::info!("Batched BLAS build: … compacted {:.1} KB → {:.1} KB ({:.0}% savings)")` — neither is compared against `blas_budget_bytes`. Phase 7 destroys the originals *after* the compaction copy's `submit_one_time` has returned, confirming both sets are simultaneously resident. `static_blas_bytes` is incremented in Phase 7 with the *compacted* size, so the committed ledger is correct; only the in-flight peak is under-counted.
- **Impact**: The budget under-states the transient peak by roughly `total_after` (empirically ~50–60% of `total_before`, per the savings figure the same function logs). Bounded by one batch, so on a well-behaved cell this is tens of MB. It matters only when a single batch is large relative to the budget — the same "OOM-on-first-huge-cell" scenario #1792 closed the *other* half of. **Unreachable on the 12 GB dev card** (~4 GB budget); relevant to the 6 GB RT-minimum target. Reported as LOW rather than dropped because it is the one remaining structural blind spot in an accounting path that has already been wrong once (#1792) in a way that made a whole mechanism a no-op.
- **Related**: #1792 (PERF-D3-NEW-01 — the Phase-1 half of this accounting, fixed), #510 (mid-batch eviction), #316 / PERF-D3-02 above (same closure).
- **Suggested Fix**: Carry `pending_bytes` into `alloc_compact` and add `compact_size` to it as each destination is allocated, checking `should_evict_mid_batch` on the same `BATCH_EVICTION_CHECK_INTERVAL` cadence; or, cheaper and sufficient, note in the `pending_bytes` doc comment that it deliberately excludes the compaction destinations and that the true peak is ~1.5× the tracked value, so a future budget tune is made against the right number.

---

### PERF-D3-04: `compute_blas_budget` sums every DEVICE_LOCAL heap while its own doc, the struct-field doc, and `memory-budget.md` all say "the DEVICE_LOCAL heap" — and the tighter query that exists for exactly this purpose goes unused
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` (`compute_blas_budget`), `crates/renderer/src/vulkan/device.rs` (`total_device_local_bytes`, `smallest_device_local_heap_bytes`), `crates/renderer/src/vulkan/acceleration/mod.rs` (the `blas_budget_bytes` field doc), `docs/engine/memory-budget.md` (§ Reserve floors, `MIN_BLAS_BUDGET_BYTES` row)
- **Status**: NEW
- **Description**: `compute_blas_budget` calls `total_device_local_bytes`, which **sums** the sizes of every heap carrying `MemoryHeapFlags::DEVICE_LOCAL`, then divides by 3. Three separate pieces of prose describe it as a single heap: `compute_blas_budget`'s own doc says "`VRAM / 3`"; the `blas_budget_bytes` field doc says "Derived at construction time from DEVICE_LOCAL heap size (VRAM / 3)"; `memory-budget.md`'s `MIN_BLAS_BUDGET_BYTES` row says "device_local_heap / 3, capped below". The codebase already owns the correct query for a residency ceiling — `smallest_device_local_heap_bytes`, whose own doc states the rationale outright: *"this is the tighter of the two — running an allocator to that heap's limit fails first"* — and the allocator's 80%-of-heap pressure warning (`allocator.rs`) uses it. The BLAS budget, whose entire stated purpose is "so smaller-VRAM GPUs evict before hitting an out-of-memory condition" (#387), uses the looser sum instead. The two subsystems therefore disagree about how much VRAM exists.
- **Evidence**: `total_device_local_bytes` — `.filter(|heap| heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL)).map(|heap| heap.size).sum()`. `smallest_device_local_heap_bytes` — identical filter, `.min()`. `compute_blas_budget` — `(device_local_bytes / 3).max(MIN_BLAS_BUDGET_BYTES)` over the `sum` variant. `grep -rn smallest_device_local_heap_bytes` returns only `allocator.rs` call sites; the acceleration module never references it.
- **Impact**: On any device exposing more than one DEVICE_LOCAL heap — the common AMD / hybrid layouts where a small `DEVICE_LOCAL | HOST_VISIBLE` BAR window is reported alongside the main VRAM heap, and the two are not disjoint physical memory — the budget over-estimates available VRAM and the eviction line sits above where an allocation actually starts failing. Single-heap NVIDIA desktop parts (including the RTX 4070 Ti dev card) are unaffected, so this is invisible on the target hardware and cannot be observed here. Practical magnitude on real multi-heap parts is small (a 256 MB over-count moves the budget by ~85 MB), which is why this is LOW and not a correctness finding — the value of fixing it is that the two VRAM-ceiling policies stop disagreeing, in a subsystem where a previously-wrong budget figure has already burned an audit (#387, "Roadmap claims 1 GB BLAS budget but code is 4 GB").
- **Which side is wrong**: the **code**, not the docs. All three prose sites describe the safer, intended semantics ("the DEVICE_LOCAL heap", singular); the implementation is the outlier. Changing the docs to say "sum of all DEVICE_LOCAL heaps" would document a weaker guarantee than #387 asked for.
- **Related**: #387 (FNV-D4-01 — established the dynamic budget and its OOM-avoidance purpose), #1572 (REN-D5-DOC-01 — the sibling case where `memory-budget.md` and the allocator warning were reconciled onto `smallest_device_local_heap_bytes`, which is the precedent this path did not follow).
- **Suggested Fix**: Switch `compute_blas_budget` to `smallest_device_local_heap_bytes` and keep the `MIN_BLAS_BUDGET_BYTES` floor (which already protects the degenerate zero/tiny-heap case), then leave all three doc sites as they are — they become accurate. A one-line unit test pinning "budget derives from the smallest heap, not the sum" would keep it from drifting back.

---

