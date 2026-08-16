# Performance Audit — 2026-08-16

**Scope**: `/audit-performance` — all 9 dimensions, `--depth deep`, run as part of
the `comprehensive` audit-suite sweep.

**Repo state**: HEAD `85b77371`, branch `main`. Dedup baseline: 269 OPEN issues
(`/tmp/audit/issues.json`) plus a 400-issue all-state fetch so CLOSED findings
could be checked for regression; `docs/audits/` scanned back through
`AUDIT_PERFORMANCE_2026-08-14.md`.

| Dim | Area | Findings |
|---|---|---|
| 0 | Bench-of-record hygiene | 1 LOW |
| 1 | CPU Per-Frame Allocations & Hot Paths | 1 MEDIUM · 2 LOW |
| 2 | Draw & Instancing | **clean** |
| 3 | GPU Memory Pressure & Eviction Thrash | **clean** |
| 4 | SSBO Sizing & Upload | **clean** |
| 5 | GPU Pipeline & Pass Efficiency | **clean** |
| 6 | Skinning & BLAS Cost | 1 LOW |
| 7 | Streaming & Cells | **clean** |
| 8 | NIF Parse | 1 LOW |
| 9 | Telemetry & Origin Cost | **clean** |

**0 CRITICAL · 0 HIGH · 1 MEDIUM · 5 LOW.**

---

## Executive Summary

**Every regression guard this skill names is intact.** All 26 checked guards
(#1371, #1372, #1374, #1376, #1377, #1379, #1430, #1489, #1492–96, #1792, #1794,
#1799, #1802, #1803, #1804/#2165, #1195, #1196, #1197, #1791/#1796, #831, #832,
#833, #877, #1262, #2682, #2923) verified present at their cited symbol. All
six findings from the 2026-08-14 audit (#2923–#2928) are CLOSED and none has
regressed — `compute_blas_budget` now sources `smallest_device_local_heap_bytes`,
the skin-LRU resize rebase landed in `7d1c4f51`, and the `alloc_compact` leak
is gone.

**The findings are concentrated where the code is new.** ~2.6k LOC of gameplay
slice landed 2026-08-15/16 (`combat.rs`, `inventory.rs`, `settings_io.rs`, the
action half of `interaction.rs`) with three new `Stage::Update` exclusives and
no owner audit skill. That is where the one MEDIUM and two of the LOWs live.
The renderer, ESM, and NIF paths — which have absorbed six prior perf sweeps —
came back essentially clean.

**No bench was run, deliberately.** All six findings are static or structural
(a per-frame allocation shape, a hasher choice, a dead memory pre-fill, a
tracker that was never re-armed). None is differentiable at the resolution the
75-run matrix offers, and the honest thing to report about the bench is that it
is now *out of gate* — which is finding PERF-D0-01, not a number to publish.
No absolute FPS figure appears anywhere in this report.

### Observed-vs-ROADMAP delta

There is none to report, and that is itself the Dimension-0 finding. The LIVE
Bench-of-record (ROADMAP.md §"Bench-of-record (LIVE) — stepped-camera refresh
(2026-08-14, HEAD `34074b93`)", raw rows
`docs/audits/BENCH_stepped-camera_34074b93.tsv`) sits **34 commits** behind
HEAD, past ROADMAP's own 30-commit gate, with several of those commits editing
shading code. Per-frame magnitudes in this report are therefore anchored on the
checked-in entity/draw/skin-pool counts in `.claude/audit-baselines/runtime/*.tsv`
rather than on manufactured FPS deltas.

### Hot-path cost table (sourced, not estimated)

| Signal | Source | fnv-FreesideAtomicWrangler | skyrim_se-WhiterunDragonsreach | fo4-InstituteBioScience |
|---|---|---:|---:|---:|
| `entities_total` | `.claude/audit-baselines/runtime/*.tsv` | — | 8126 | 12448 |
| `bench_draws_cmds` | same | 2553 | 2342 | 3440 |
| `bench_draws_batches` | same | 89 | 9 | 753 |
| `bench_draws_gpu_calls` | same | 25 | 2 | 42 |
| `skin_pool_live` | same | — | 83 | 124 |

Sort-gate check: `DRAW_SORT_PARALLEL_THRESHOLD = 3000`
(`byroredux/src/render/mod.rs:561`) is applied to the raster prefix. Only the FO4
baseline (3440) crosses it, which matches the in-file crossover table (serial
wins to ~2750, parallel pulls away by 5000). The threshold is correctly placed —
no change proposed.

Per-pass GPU cost was **not** sampled: `gpu_timers.rs` / `ScratchTelemetry` are
runtime-only and require a live Vulkan device plus on-disk game data, and per
`feedback_no_parallel_engine_launch.md` no engine instance was spawned for this
audit. No finding in this report claims a pass is "expensive".

---

## Findings

### PERF-D1-01: `target_has_line_of_sight` rebuilds an O(all-rigid-bodies) `Vec` and linear-scans it, every frame the crosshair is on an activator

- **Severity**: MEDIUM
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:750-782`
- **Status**: NEW
- **Description**: `interaction_system` is an unconditional `Stage::Update`
  exclusive (`byroredux/src/boot.rs:776`) and calls `select_interaction_target`
  every frame. Its final step is
  `targets.into_iter().find(|t| target_has_line_of_sight(...))`, and that
  function materialises the **entire** `RapierHandles` storage into a fresh
  `Vec<(EntityId, RigidBodyHandle)>` and then linear-scans it to map the ray
  hit's rigid-body handle back to an ECS entity. There is no reverse index
  anywhere: `crates/physics/src/world.rs` never writes rapier's `user_data`
  (zero occurrences in `crates/physics/src/`), and `PhysicsRayHit` carries only
  the opaque handle.
- **Evidence**:
  ```rust
  // interaction.rs:750-760
  let (excluded_body, owners) = match world.query::<byroredux_physics::RapierHandles>() {
      Some(handles) => {
          let excluded = player.and_then(|entity| handles.get(entity).map(|h| h.body));
          let owners = handles
              .iter()
              .map(|(entity, handles)| (entity, handles.body))
              .collect::<Vec<_>>();
          (excluded, owners)
      }
      None => (None, Vec::new()),
  };
  // ... interaction.rs:777-779
  let Some(hit_owner) = owners
      .iter()
      .find_map(|(entity, body)| (*body == hit_body).then_some(*entity))
  ```
  The `Vec` exists specifically to release the component guard before acquiring
  `PhysicsWorld` (the lock-order comment at :745-746 says so) — it is not
  incidental, so it cannot be removed by rearranging the borrows.
- **Impact**: `INTERACTION_REACH_BU = 192.0` (~2.7 m), so this fires whenever
  the player's crosshair is on any door / `TwoStateActivator` /
  `RumbleOnActivate` / `QuestAdvanceOnActivate` / `MG07LabyrinthianDoor` within
  reach — the ordinary case walking a corridor. FNV and FO4 architecture lacks
  authored `bhk` collision and synthesises a static trimesh collider per REFR
  (ROADMAP R6a-stale-13 measured entity counts rising +37 % / +42 % from exactly
  that), so `owners` is thousands of entries on those games: tens of KB of
  allocation plus a full linear scan, per frame, on a machine where a CPU
  bottleneck is by policy a bug. Blast radius is the interaction path only —
  nothing is *incorrect*, it is pure waste. **No dhat guard exists for this
  site** (the profiler is a process singleton; per-frame render/ECS paths have
  no allocation-bound coverage).
- **Related**: the identical pattern at `byroredux/src/combat.rs:116-136` is
  **not** a finding — it is gated behind an attack edge plus
  `MELEE_COOLDOWN_SECONDS` (0.45 s), so it runs ~2/s, not 60+/s. #2864
  (`PHYS-D2-03`, OPEN) independently confirms the collider population is large
  enough to matter per frame.
- **Suggested Fix**: write the `EntityId` into the rapier `RigidBody`'s
  `user_data: u128` at insertion in `crates/physics/src/sync.rs`, and have
  `PhysicsRayHit` carry the resolved entity. That makes the lookup O(1),
  allocation-free, and removes the lock-order workaround entirely. A persistent
  `FxHashMap<RigidBodyHandle, EntityId>` on `PhysicsWorld` is the smaller
  alternative if `user_data` is wanted for something else.

---

### PERF-D0-01: The bench-of-record is 34 commits past its own 30-commit gate with no successor staleness tracker filed

- **Severity**: LOW
- **Dimension**: Bench-of-record hygiene
- **Location**: `ROADMAP.md:137-175` (LIVE block) and `ROADMAP.md:1090`
  (`R6a-stale-19`, RESOLVED)
- **Status**: NEW
- **Description**: `R6a-stale-19` was closed 2026-08-14 at HEAD `34074b93`
  (#2835) and, unlike every previous entry in that ladder, no `R6a-stale-20`
  was opened behind it. `git rev-list --count 34074b93..HEAD` is **34**, so
  ROADMAP's own 30-commit limb is already crossed, and the change-content limb
  is met independently: the intervening range edits shading and
  acceleration-structure code (`7d1c4f51` glass refraction tint + sky-arm
  indirect term, `9bf7d024` glass IOR + ray-budget telemetry, `c25f61e6` TLAS
  shrink allocate-then-swap + AS publish on both `build_tlas` arms, `9c805cd7`
  cluster-cull telemetry, `77b540d0` XCLL directional reclassification).
- **Evidence**: `grep -n "R6a-stale-19\|R6a-stale-20" ROADMAP.md` returns only
  `R6a-stale-19` occurrences; no successor exists in ROADMAP or in the 400-issue
  all-state issue fetch.
- **Impact**: This is the machinery that makes every *other* performance claim
  in the project falsifiable. The previous gap in this ladder ran four
  consecutive folds and 186 commits, and ROADMAP itself records the outcome:
  "no new perf claim should be published until `scripts/fsr-bench-matrix.sh 3 300`
  is re-run". The tracker is what prevents that recurring; without it the
  deferral is silent rather than recorded. Note this is not a code defect — it
  is the gate on this audit's own evidence, which is why it is reported rather
  than assumed.
- **Related**: `R6a-stale-19` (ROADMAP), #2835, #2367 (OPEN — the unbisected
  FO4/FNV regression from the last full matrix).
- **Suggested Fix**: file `R6a-stale-20` in ROADMAP's staleness ladder at HEAD
  `85b77371` with the 34-commit count and the shading-touching commit list, or
  run `scripts/fsr-bench-matrix.sh 3 300` and advance the record.

---

### PERF-D1-02: `collect_candidates` allocates a fresh SipHash `HashMap` plus a `Vec` every frame

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:817-869`
- **Status**: NEW
- **Description**: `collect_candidates` opens with
  `HashMap::<EntityId, InteractionKind>::new()`, fills it from five component
  queries, and closes with `candidates.into_iter().collect()` into a `Vec`.
  It runs unconditionally from `select_interaction_target` every frame,
  regardless of input, so both containers are allocated and dropped 60+ times a
  second.
- **Evidence**:
  ```rust
  fn collect_candidates(world: &World) -> Vec<(EntityId, InteractionKind)> {
      let mut candidates = HashMap::<EntityId, InteractionKind>::new();
      ...
      candidates.into_iter().collect()
  }
  ```
- **Impact**: Candidate cardinality is small (doors + scripted activators in the
  loaded cell), so this is allocator churn rather than volume. It is, however,
  exactly the shape #1372 (`AnimScratch`) and #1374 (billboard `last_cam`)
  removed elsewhere on the frame tick — reintroduced in a system that postdates
  both fixes and has no owner audit.
- **Related**: PERF-D1-01 (same function's caller), #2680 (CLOSED — "stop the
  last per-frame allocations", which predates this module).
- **Suggested Fix**: hang a persistent scratch `FxHashMap` + `Vec` off the
  existing `InteractionState` resource and clear+refill them, matching the
  `AnimScratch` pattern.

---

### PERF-D1-03: `refresh_action_state` clones two `HashSet`s per frame to work around a resource-guard overlap

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:681-709`
- **Status**: NEW
- **Description**: `refresh_action_state` (called from
  `player_controller_system` in `Stage::Early`, `byroredux/src/systems/character.rs:79`)
  deep-clones `InputState.keys_held` and `InputState.mouse_buttons_held` before
  dropping the `InputState` guard, purely so it can then acquire
  `ActionBindings`. Both are `std::collections::HashSet`
  (`byroredux/src/components.rs:1114-1115`), so this is two heap allocations per
  frame for data that is never mutated.
- **Evidence**:
  ```rust
  let keys_held = input.keys_held.clone();
  let mouse_buttons_held = input.mouse_buttons_held.clone();
  drop(input);
  ...
  let mut next_held = bindings.held_mask(&keys_held, &mouse_buttons_held);
  ```
  `held_mask` takes both by reference — the clone buys nothing but lock ordering.
- **Impact**: Two small allocations/frame on the input tick. Trivial in
  isolation; listed because it is a *newly introduced* per-frame allocation on a
  tick that had been driven to zero, and because the fix is free.
- **Suggested Fix**: acquire `ActionBindings` **first**, then `InputState`, and
  call `bindings.held_mask(&input.keys_held, &input.mouse_buttons_held)` in
  place — no clone, no guard overlap in the other direction.

---

### PERF-D6-01: #2923's Fx-hashing conversion covered 1 of ~9 per-frame per-entity probes on the same call path; the siblings are still SipHash-1-3

- **Severity**: LOW
- **Dimension**: Skinning & BLAS
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1166, 1195, 1308, 1327, 1347, 1568`;
  `crates/renderer/src/vulkan/acceleration/mod.rs:243`;
  probe sites in `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` and
  `crates/renderer/src/vulkan/context/draw.rs:2774, 3162-3178`
- **Status**: NEW (residual of #2923, which is CLOSED and **not** regressed)
- **Description**: #2923's guard test
  `pose_dirty_crosses_the_crate_boundary_without_siphash`
  (`context/mod.rs:4347`) pins exactly one probe — `pose_dirty.contains()` — and
  its own comment states the motivation as "SipHash-1-3 on a per-frame
  per-entity keyspace". Every sibling collection on that same loop body was left
  as a std collection:

  | Field | Declared at | Probed |
  |---|---|---|
  | `skin_slots: HashMap<EntityId, SkinSlot>` | `context/mod.rs:1308` | `contains_key` :200, `get` :300/:549, `get_mut` :356, plus `draw.rs:2774` per skinned draw |
  | `failed_skin_slots: HashSet<EntityId>` | `context/mod.rs:1327` | `contains` :268 |
  | `failed_skin_blas: HashSet<EntityId>` | `context/mod.rs:1347` | `contains` :280 |
  | `skin_dispatch_seen_scratch: HashSet<EntityId>` | `context/mod.rs:1166` | `insert` :110, once per skinned **draw command** |
  | `skin_built_this_frame_scratch: HashSet<EntityId>` | `context/mod.rs:1195` | per first-sight entity |
  | `skinned_blas: HashMap<EntityId, BlasEntry>` | `acceleration/mod.rs:243` | `has_skinned_blas` :578, :599 |
  | `blend_seen_scratch: HashSet<(u8,u8,bool,bool)>` | `context/mod.rs:1568` | `insert` per **batch**, `draw.rs:3162-3178` |

  (line numbers in the "Probed" column are `skinned_blas_refit.rs` unless noted)
- **Evidence**: `crates/renderer/src/vulkan/context/mod.rs:4338-4345` — "#2923 /
  PERF-D1-01 — the skinning path's half of the same pattern. `#1368` and `#2174`
  both stopped at the `crates/renderer` boundary; the skinning path crosses it".
  The fix crossed the boundary but did not sweep the near side.
- **Impact**: Small in absolute terms — `skin_pool_live` is 124 (FO4 baseline) /
  83 (Skyrim baseline), so the skin probes are ~1000 SipHash rounds per frame;
  `blend_seen_scratch` adds one insert per batch (753 on the FO4 baseline) into
  a set whose own cardinality is 3-5. The real cost is epistemic: the guard test
  makes this path *read* as Fx-hashed end-to-end when ~8 of 9 probes are not, so
  the next reader trusts a property that does not hold.
- **Related**: #1368, #2174, #2923 (all CLOSED). Explicitly **out of scope**:
  parser, ESM, cell-loader, and `byroredux/src/npc_spawn/resumable.rs` maps —
  those are load-time and correctly std per `_audit-common.md`'s hot-path rule.
- **Suggested Fix**: `rustc-hash` is already a `crates/renderer` dependency —
  substitute `FxHashMap`/`FxHashSet` on the seven fields above and widen
  `pose_dirty_crosses_the_crate_boundary_without_siphash` to assert the whole
  set, so the guard means what it says.

---

### PERF-D8-01: `read_pod_vec`'s zero-init pre-fill is dead work, and `NiPoint3` is the one instantiation that misses std's `IsZero` fast path

- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:449`; instantiation list at `:62-64`;
  affected call sites `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:342, 369`
  and `crates/nif/src/blocks/controller/morph.rs:261`
- **Status**: NEW
- **Description**: `read_pod_vec` pre-sizes with `vec![T::default(); count]` and
  then `read_exact`s over the whole buffer, so the fill is overwritten in full —
  dead work in every instantiation. What differs is its *cost*: `vec![elem; n]`
  dispatches through std's private `SpecFromElem`, which for `T: IsZero` lowers
  to `RawVec::with_capacity_zeroed_in` (i.e. `alloc_zeroed`). `IsZero` is
  implemented in std for the integer/float primitives and for `[T; N] where T:
  IsZero`, so **all** of `u8/i8/u16/i16/u32/i32/u64/i64/f32/f64`, `[u8;4]`,
  `[u8;32]`, `[u16;3]`, `[f32;2]`, `[f32;3]`, `[f32;4]` take the fast path.
  `NiPoint3` (`crates/nif/src/types.rs:15-21`) is a user `#[repr(C)]` struct and
  `IsZero` is sealed to std, so its `vec![...]` falls back to
  `Vec::extend_with` — an element-by-element clone loop over the full array.
- **Evidence**:
  ```rust
  // stream.rs:449
  let mut out: Vec<T> = vec![T::default(); count];
  ```
  ```rust
  // stream.rs:569-571 — the byte-identical sibling that DOES take the fast path
  pub fn read_f32_triple_array(&mut self, count: usize) -> io::Result<Vec<[f32; 3]>> {
      self.read_pod_vec::<[f32; 3]>(count)
  }
  ```
  Its own doc comment (stream.rs:565-568) states `[f32; 3]` "is POD with the
  same `#[repr(C)]` layout as `NiPoint3`" — the two are byte-identical, and only
  one of them is on the slow path.
- **Impact**: `read_ni_point3_array` backs the two largest arrays in classic
  (Oblivion / FO3 / FNV) geometry — **vertices** (`ni_tri_shape.rs:342`) and
  **normals** (`:369`) — plus morph vertex deltas. The win is confined to large
  arrays (small allocations get memset by the allocator either way), so this is
  memory bandwidth on the cell-load path, not a hitch. **Explicitly not
  measured**: no bench was run, and this is invisible to the existing
  `crates/nif/tests/heap_allocation_bounds*.rs` dhat guards because the
  allocation *count* is unchanged. Reported as a correctness-of-intent gap in a
  helper whose stated purpose (#833) is to collapse redundant passes.
- **Related**: #833, #831, #2525 (CLOSED — the sibling per-element-decode finding).
- **Suggested Fix**: keep `read_pod_vec`'s existing unsafe block and replace the
  pre-fill with `Vec::with_capacity(count)` + read into the spare capacity +
  `set_len(count)` (the `AnyBitPattern` bound already licenses it, and the
  SAFETY comment at :450-465 already covers every other premise). That removes
  the dead pass for *all* instantiations, not just `NiPoint3`.

---

## Prioritized Fix Order

1. **PERF-D1-01** — the only per-frame O(N) waste found; the `user_data` fix is
   small, removes an allocation *and* a lock-order workaround, and lands in the
   subsystem the project is actively building on.
2. **PERF-D0-01** — file `R6a-stale-20`. Cheap, and it is the gate on every
   future perf claim; the last time it lapsed it ran 186 commits.
3. **PERF-D1-02 / PERF-D1-03** — mechanical scratch-reuse and borrow reordering
   in the same file as (1); do them in the same change.
4. **PERF-D6-01** — type substitution plus a widened guard assertion. Do it as
   one sweep so the assertion and the fields cannot drift again.
5. **PERF-D8-01** — one helper, one unsafe block already present.

---

## Guards verified intact (do NOT re-propose)

Dimension 1: #1371 `drain_dirty_into` (`crates/core/src/ecs/packed.rs:73`; sole
production caller `byroredux/src/systems/bounds.rs:122`; **zero** production
`take_dirty` callers) · #1372 `AnimScratch`
(`byroredux/src/systems/animation.rs:373`) · #1374 billboard `last_cam`
(`byroredux/src/systems/billboard.rs:23,56,59`) · #1376 debug-UI snapshot gate
(`byroredux/src/app_frame.rs:63-92`) · #1379 `next_slot` contraction
(`crates/core/src/ecs/resources/skin_slot_pool.rs:318-335`) · #1794 `bone_world`
no-clear (`byroredux/src/render/mod.rs:696-700`) · #1803 dead `GlobalTransform`
probe absent from `byroredux/src/render/particles.rs` · #1802 `BYRO_PROFILE`
`OnceLock` (`render/mod.rs:708-712`) and `BYRO_NO_CULL`
(`render/static_meshes.rs:148-152`), both pinned by
`byroredux/src/render/env_var_cache_tests.rs`.

Dimension 2: #1377/#1805 GT-presence hoist (`render/static_meshes.rs:163`) ·
#1804/#2165 `needs_two_sided_blend_split` = `is_blend && two_sided &&
order_dependent_glass` with no `z_write` limb
(`crates/renderer/src/vulkan/context/draw.rs:1207-1209`) and
`order_dependent_glass` in the batch merge key (`draw.rs:2907`) · #2682 self-swap
guard (`render/mod.rs:554`) · full per-draw dynamic-state change-gating in
`crates/renderer/src/vulkan/context/geometry_pass.rs` (pipeline :247, depth bias
:293, depth test/write/compare :305/:309/:313, cull :348-353, VB/IB rebind
elision :388) · indirect grouping :437-455.

Dimension 3: dynamic BLAS budget `(heap/3).max(MIN_BLAS_BUDGET_BYTES)`
(`acceleration/predicates.rs:659`, `constants.rs:61`) now sourced from
`smallest_device_local_heap_bytes` (predicates.rs:696) · #1792 `blas_over_budget`
folding `pending_bytes` (predicates.rs:470, applied `blas_static.rs:1010,1055`),
`BATCH_EVICTION_CHECK_INTERVAL = 64` (`constants.rs:74`) · shrink floors
`MIN_TLAS_INSTANCE_RESERVE`/`WORKING_SET_FLOOR` = 8192 (`constants.rs:47-54`,
applied `memory.rs:199`) · `MeshRegistry` soft/hard caps + `check_pool_growth`
(`crates/renderer/src/mesh.rs:29-34, 456-467`) · #1430 half-eviction on all three
`byroredux/src/asset_provider/material.rs` caches (:651, :685, :751, :767) ·
`BYRO_NIF_CACHE_MAX` 2048 LRU
(`byroredux/src/cell_loader/nif_import_registry.rs:195-214`) · deferred-destroy
countdown = `MAX_FRAMES_IN_FLIGHT` (`crates/renderer/src/deferred_destroy.rs:46`).

Dimension 4: `MAX_INSTANCES`/`MAX_INDIRECT_DRAWS`/`MAX_MATERIALS`/`MAX_LIGHTS`
(`scene_buffer/constants.rs:139/162/191/15`) · `upload_instances` O(live) with
content-hash gate and ranged flush (`scene_buffer/upload.rs:542-590`) · all 51
`gpu_instance_layout_tests` pass, incl.
`gpu_instance_is_128_bytes_std430_compatible` and
`gpu_instance_does_not_re_expand_with_per_material_fields` · `MaterialTable.index`
is `FxHashMap` with capacity-preserving `clear`
(`crates/renderer/src/vulkan/material.rs:1077, 1133`) · `Material::resolve_pbr`
called only from `byroredux/src/material_translate.rs:230, 308`; zero per-draw
`classify_pbr_keyword`.

Dimension 5: #1799 `ENABLE_LEGACY_WRS = 0` in both
`crates/renderer/src/shader_constants_data.rs:710` and the generated
`crates/renderer/shaders/include/shader_constants.glsl:168`, with
`NUM_RESERVOIRS = 16` inside `#if ENABLE_LEGACY_WRS`
(`crates/renderer/shaders/triangle.frag:2575, 2585, 2613`); the shipped
`triangle.frag.spv` is newer than both its source and the generator ·
`invViewProj` is CPU-side UBO data (`cluster_cull.comp:60`, `ssao.comp:24`), the
only shader `inverse()` calls being the flag-gated non-uniform-scale normal path
· `froxel_extent` is resolution-derived
(`crates/renderer/src/vulkan/volumetrics.rs:472-482`) · GPU-timer readback drops
`WAIT` and gates on `active_bits`
(`crates/renderer/src/vulkan/gpu_timers.rs:383-425`).

Dimension 6: #1195 `pose_dirty: FxHashSet<EntityId>`
(`skin_slot_pool.rs:109`) · #1196 `SKINNED_BLAS_REFIT_THRESHOLD = 600` and
`SKINNED_BLAS_FLAGS = PREFER_FAST_BUILD | ALLOW_UPDATE`, **not** FAST_TRACE
(`acceleration/constants.rs:68, 112-116`) · #1197 descriptor-rewrite skip
(`crates/renderer/src/vulkan/skin_compute.rs:589`) · #1791/#1796 rollback check
plus its ordering guard `skin_dispatch_ran_rollback_scope_tests`
(`byroredux/src/app_frame.rs:434, 542`).

Dimension 7: #877 two-phase `pre_parse_cell` with `PRE_PARSE_RAYON_MIN = 8`
(`byroredux/src/streaming.rs:1233-1238`) · `STREAMING_APPLY_BUDGET = 4 ms`
seeding `FrameTimeBudget::until` (`byroredux/src/app_step.rs:26, 171-172`) ·
batched exterior teardown through `cell_loader::unload_cells` +
`World::despawn_batch`.

Dimension 8: #833 `read_pod_vec` as sole bulk reader with the big-endian
compile-error gate (`crates/nif/src/stream.rs`) · #831 `#[must_use]` on
`allocate_vec`, `allocate_vec_sized`, `allocate_vec_min_bytes`, `read_pod_vec`
and every `read_*_array` wrapper · #832 zero `or_insert(name.to_string())`
occurrences in `crates/nif/src` · `extract_emitter_params`/`extract_emitter_rate`
import-only (`crates/nif/src/import/walk/mod.rs:605-606, 1411-1412`) ·
`crates/nif/tests/heap_allocation_bounds.rs` + `heap_allocation_bounds_geometry.rs`.

Dimension 9: `log_stats_system` `cpu_ms` split
(`byroredux/src/systems/debug.rs:98-111`) · `ScratchTelemetry`
(`crates/core/src/ecs/resources/mod.rs:429+`, refreshed `app_frame.rs:147-152`) ·
#1492–96 one `snap_render_origin` + one `look_at_rh` per frame
(`byroredux/src/render/camera.rs:182-185`), per-instance rebase inside the
existing loop (`draw.rs:2607, 2617`), #1489 `origin_corrected_prev_view_proj`
(`draw.rs:1954`, defined :3753).

---

## Existing OPEN issues touched (deduplicated, not re-reported)

#779 (`triangle.frag` early_fragment_tests) · #2782 (water.frag same) · #2766
(`indirect_call_count` overcount) · #2821 (`GpuTimerSnapshot._active` ignored by
four readers) · #2367 (unbisected FO4/FNV regression from the last full matrix) ·
#2864 (`PHYS-D2-03`, per-streaming-frame QBVH rebuilds) · #2865, #2775, #2774,
#2769, #2686, #2689.

## Known-open, deliberately not re-reported

- The **interior** cell path still calls `load_references` with
  `FrameTimeBudget::unlimited()`
  (`byroredux/src/cell_loader/references/mod.rs:217`), so interior NPC spawn has
  no per-frame budget. `audit-performance/SKILL.md` Dimension 7 already states
  this as open-for-interiors; no GitHub issue exists (checked against the
  400-issue all-state fetch). Recorded here so a future sweep knows it was
  checked, not missed.
- #1793 (permanently-missing rigid BLAS has no recovery; synchronous multi-cell
  burst can false-evict) and #1797 (shared `blas_scratch_buffer` serializes N
  dirty skinned entities) are documented-not-fixed and unreachable on the 12 GB
  dev card.
- `TextureRegistry` slots are strictly grow-only with no count eviction — a
  documented design constraint (`docs/engine/memory-budget.md:332-353`), not a leak.
- The RT-1/#2215 depth-primary alpha-over batch-count rise is the accepted cost
  of a correctness fix (`byroredux/src/render/mod.rs:477-500`), and the two-sided
  blend split is structurally dormant for engine-classified glass (#2691) — do
  not attribute batch-count movement to it.

## Candidates investigated and dropped (so a later sweep does not re-derive them)

- **`combat.rs`'s `owners` Vec** — same shape as PERF-D1-01 but gated behind an
  attack edge + a 0.45 s cooldown. Not a per-frame cost.
- **`combat_damage_system`'s `.collect()`** — on the common empty-`HitEvent`
  frame this is `Vec::new()`, which does not allocate.
- **`SkinSlotPool::sweep` returning `Vec<u32>`** — `Vec::new()` until the first
  push; no allocation on a no-eviction frame.
- **`setting_bool` × 2 per frame on the hidden-overlay path**
  (`byroredux/src/app_frame.rs:80-89`) — a `BTreeMap<String>` probe plus a
  `SettingValue::Bool` clone. No allocation; the #1376 gate is not eroded.
- **`fog_height_reference`'s per-frame downward raycast**
  (`byroredux/src/render/mod.rs:40-68`) — an O(log n) query-pipeline probe that
  exists by design (#2225/#2859).
- **`metrics_sample_system`'s ~29 `to_string()` calls**
  (`byroredux/src/systems/metrics.rs:132-243`) — the system is self-throttled to
  ~2 Hz (`byroredux/src/boot.rs:1318-1324`).
- **`trigger_detection_system`'s `entered: Vec::new()`**
  (`crates/scripting/src/trigger.rs:117`) — no allocation on the common
  no-edge frame.
- **`current_rigid_models.reserve(draw_commands.len())`**
  (`crates/renderer/src/vulkan/context/draw.rs:2573`) — over-reserves relative to
  the rigid subset, but `HashMap::reserve` after `clear()` is a no-op once the
  capacity is reached, so the cost is a one-time few-hundred-KB allocation, not
  per-frame.
- **`geometry_pass.rs`'s repeated `group_state(&batches[end])`**
  (`crates/renderer/src/vulkan/context/geometry_pass.rs:439`) — evaluated once
  per batch in total across all groups, i.e. O(batches), not O(batches²).
- **The always-on egui pass** — `show_crosshair`/`show_prompts` now default true,
  so the overlay always has content. That is deliberate P2 HUD design, and the
  content is a two-triangle quad.

## Scope note

`crates/mod-runtime`, `crates/facegen`, `crates/hkx`, `crates/debug-server` and
`crates/debug-protocol` were **not** examined — none has a per-frame path and
none is in this skill's dimension list. The gameplay slice
(`byroredux/src/{combat,inventory,settings_io}.rs` + the action half of
`byroredux/src/interaction.rs`) **was** treated as in-scope despite having no
owner audit skill, per `_audit-common.md`'s coverage-gap table; it produced three
of the six findings.
