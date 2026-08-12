# AUDIT — Concurrency (`/audit-concurrency`)

**Date**: 2026-08-12 · **Repo**: `/mnt/data/src/gamebyro-redux` @ `efc089ba` · **Depth**: deep
**Suite**: `renderer-deep` (focused run) · **Dedup baseline**: `/tmp/audit/issues.json`
**Dimension scratch inputs**: `/tmp/audit/concurrency/dim_1_2.md`, `/tmp/audit/concurrency/dim_3.md`

---

## 1. Executive Summary

### SCOPE — read this before acting on the report

This was the **`renderer-deep` suite's focused run of `/audit-concurrency`: Dimensions 1, 2 and 3 only.**

| Dim | Subject | Run? |
|---|---|---|
| 1 | Vulkan queue & acceleration-structure sync | **YES** |
| 2 | Compute → AS → fragment chains | **YES** |
| 3 | ECS lock ordering & deadlock | **YES** |
| 4 | Scheduler access declarations | **NOT RUN** |
| 5 | RwLock `Resource` ↔ `Storage` patterns / physics | **NOT RUN** |
| 6 | Resource lifecycle | **NOT RUN** |
| 7 | Worker threads | **NOT RUN** |

Dimensions 4, 5, 6 and 7 were **not executed and are not covered by this report**. A clean
result here is *not* a clean result for the concurrency domain as a whole. In particular the
`Resource`↔`Storage` unordered-pair class (Dim 5) and off-scheduler worker threads (Dim 7) were
touched only incidentally, as edge cross-checks from inside Dim 3, and were not swept.

**No engine was launched.** There was no Vulkan device and no game data in scope for this run.
Consequently **every finding below is either provable from source order alone, or is explicitly
filed as a hypothesis** in section 3 with the named runtime signal that would confirm it. No
finding rests on "this barrier looks wrong". Per the binding speculative-Vulkan-fix guardrail,
no barrier / stage / layout change is proposed on reasoning alone.

### Findings by severity

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 1 | CONC-D1-NEW-01 |
| HIGH | 1 | CONC-D1-NEW-02 |
| MEDIUM | 2 | CONC-D3-NEW-01, CONC-D3-NEW-02 |
| LOW | 2 | CONC-D1-NEW-03, CONC-D3-NEW-03 |
| **Total** | **6** | |
| Hypotheses (unconfirmed) | 1 | CONC-D1-H1 |

### The headline — CONC-D1-NEW-01 (CRITICAL), independently confirmed twice over

`ensure_tlas_state` in [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs) **destroys the
old `VkAccelerationStructureKHR` and its three buffers before running its two fallible
replacement allocations** (the `?;` sites). This is destroy-before-fallible-allocate, confirmed
by direct inspection of the source order.

The failure does not self-limit, because `scene_buffers.tlas_written` is a genuine **one-way
latch**: it has **exactly one write site** —
[descriptors.rs](crates/renderer/src/vulkan/scene_buffer/descriptors.rs) line 175, setting
`true` — and **no reset site anywhere**, confirmed by grep across the renderer crate.
*(Accuracy note: [volumetrics.rs](crates/renderer/src/vulkan/volumetrics.rs) carries a
same-named `tlas_written` latch that **does** reset correctly at line 1492 — see guard #26. Do
not conflate the two; only the `SceneBuffers` one is defective.)*

Consequence: on a failed TLAS resize, scene descriptor **binding 2 keeps naming a destroyed
acceleration structure while `rt_flag` stays `1.0`**, so `triangle.frag` ray-queries a dead
acceleration structure on that frame-in-flight slot **every frame**, for as long as the
allocation keeps failing.

**Two independent agents converged on this one defect from two different directions.** This
concurrency audit reached it through Dimension 1 (object lifetime across the queue/AS sync
window) and filed it CRITICAL. The concurrent `/audit-renderer` run reached the same underlying
defect through its Dimension 5 and filed it as **REN-D5-02 at HIGH**. Independent convergence
from different analytical entry points is the strongest evidence this suite produced; treat the
defect as confirmed. **CONC-D1-NEW-01 stays at CRITICAL as its author set it** — this report
does not re-severity findings, and the higher of the two ratings is the safe one to act on.

### Second-most important — a correction to the audit tooling itself

**CONC-D1-NEW-03 is a defect in the `/audit-concurrency` skill text and in the instruction
relayed to the auditing agent, not in the engine.** The Dimension 1 checklist tells auditors to
confirm the queue `Mutex` guard is **not** held across `queue_submit`. The shipped code
**deliberately does hold it**, per `VUID-vkQueueSubmit-queue-00893`, refined by #1713, with a
live regression test pinning the ordering. **Acting on the skill text as written would re-open
two correct, closed fixes and re-introduce a real external-synchronisation data race on the
queue.** See section 5; this must be fixed in
[SKILL.md](.claude/commands/audit-concurrency/SKILL.md) before the next run.

### Dim 3 in one line

No reachable ECS deadlock exists today, and the reason is *not* the runtime detector — it is the
static zero-conflict scheduler invariant (CONC-D3-NEW-03). The runtime detector meanwhile has a
coverage hole that a complete 3-cycle **already sits in** (CONC-D3-NEW-01), with CI green on it.

### Calibration evidence — what was *disproved* rather than reported

Recorded deliberately, as evidence the run was calibrated rather than credulous:

- **Disproved candidate (Dim 3, guard #4)**: the resource accessors defuse the tracker scope
  *before* `ResourceRead::new` / `ResourceWrite::new`, which superficially looks like the #2149
  bug. It is **not** — those constructors are infallible (the fallible `downcast_ref().expect(…)`
  lives in `Deref`, not `new`), so there is no panic window that could orphan a tracker row. Not
  reported.
- **Disproved candidate (Dim 3, guard #13)**: streaming's `into_par_iter()` shares rayon's global
  pool with `Scheduler::run`'s `par_iter_mut()`. This **cannot** nest an ECS lock — the calling
  thread is a plain `std::thread`, not a pool worker, so it blocks rather than work-steals, and
  the injected NIF-parse jobs take no ECS locks at all. Not reported.
- **Confirmed fixed**: the 2026-07-25 CI `BYRO_LOCK_ORDER_CHECK` gap (CONC-D4-NEW-01) is closed
  by **#2137**, with the in-file comment citing it. Not re-reported as a finding.
- **9 open issues re-confirmed open and deliberately not re-reported** across the two dimensions
  (Dim 1/2: #2465, #2403, #2402, #2401, #2484, #2485 — Dim 3 additionally re-confirmed #2389,
  #2393, #2391, #2388, #2387, #2386, #2385, #2384, #2547, #2404, #2400, #2399, #2398, #2270,
  #2660). See section 6's pre-existing list.

**48 guards were verified intact** (33 from Dim 1/2, 15 from Dim 3, no cross-dimension
duplicates) — section 4.

---

## 2. Findings

Every finding is reproduced with its full Base Per-Finding Format block plus this audit's two
required extra fields — **Trigger Conditions** and **Verification Path**. Finding IDs are
preserved exactly. Nothing has been re-severitied, merged away, or dropped.

---

### CRITICAL

#### CONC-D1-NEW-01: TLAS resize destroys the old acceleration structure before allocating its replacement — on failure the scene descriptor's binding 2 dangles while `rt_flag` stays 1.0

- **Severity**: CRITICAL
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)`:695-790`
  (`AccelerationManager::ensure_tlas_state`); consumers
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:2190-2224` (`draw_frame`),
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:1628-1633` (`rt_flag`),
  [descriptors.rs](crates/renderer/src/vulkan/scene_buffer/descriptors.rs)`:169-185`
  (`SceneBuffers::write_tlas`)
- **Status**: NEW (no match in the dedup baseline; nearest neighbours #2481 CLOSED — *BLAS* slot
  overwrite — and #2297 OPEN — TLAS eligibility filter — are different defects)
- **Description**: `ensure_tlas_state` takes the destroy-then-allocate order. It `take()`s the
  slot, calls `destroy_acceleration_structure(old.accel)` plus three `GpuBuffer::destroy`s, and
  only afterwards runs the two fallible allocations (`GpuBuffer::create_host_visible(...)?`,
  `GpuBuffer::create_device_local_uninit(...)?`) and the AS creation. Any `?` in that window
  leaves `self.tlas[frame_index] == None` and propagates `Err` out of `build_tlas`.

  `draw_frame`'s call site treats that as non-fatal —
  `if let Err(e) = accel.build_tlas(..) { log::warn!("TLAS build failed: {e}"); }` — and the
  `else` arm that carries the AS-write→read barrier, `scene_buffers.write_tlas`, and
  `patch_camera_rt_flag` is skipped. Scene descriptor set binding 2 therefore keeps naming the
  `VkAccelerationStructureKHR` that was just destroyed.

  The failure is not self-limiting for the frame that hits it, because `rt_flag` is derived from
  `SceneBuffers::tlas_written[frame]`, and `tlas_written` is a **one-way latch**: `write_tlas`
  sets it `true` (`descriptors.rs:175`) and nothing ever clears it. Once a slot has ever had a
  TLAS, `rt_flag = 1.0` on every later frame, so `triangle.frag` (and `water.frag`) initialise
  ray queries against the dangling binding-2 handle. The render pass still executes; only
  `caustic` / `volumetrics` self-gate on `tlas_handle(frame) == None`.
- **Evidence**:
  ```rust
  // tlas.rs:719-732 — destroy first …
  if let Some(mut old) = self.tlas[frame_index].take() {
      let _ = device.device_wait_idle();
      self.accel_loader.destroy_acceleration_structure(old.accel, None);
      old.buffer.destroy(device, allocator);
      old.instance_buffer.destroy(device, allocator);
      old.instance_buffer_device.destroy(device, allocator);
  }
  // tlas.rs:768-784 — … allocate second, with two `?` exits
  let mut instance_buffer = GpuBuffer::create_host_visible(device, allocator, padded_size, ..)?;
  let mut instance_buffer_device = GpuBuffer::create_device_local_uninit(device, allocator, padded_size, ..)?;
  ```
  ```rust
  // draw.rs:2190-2199 — warn-only, barrier + write_tlas live in the else arm
  if let Err(e) = accel.build_tlas(&self.device, alloc, cmd, draw_commands, &instance_map, frame) {
      log::warn!("TLAS build failed: {e}");
  } else { /* memory_barrier(...); write_tlas(...); patch_camera_rt_flag(...) */ }
  ```
  ```rust
  // descriptors.rs:175 — one-way latch, never reset
  self.tlas_written[frame_index] = true;
  ```
  Independently re-verified for this merge: `grep -rn "tlas_written" crates/renderer/src/`
  returns exactly one assignment on the `SceneBuffers` latch (`descriptors.rs:175`, `= true`)
  and **no** reset. The `volumetrics.rs` latch of the same name is a separate field that *does*
  reset (`volumetrics.rs:1492`) — see guard #26.
- **Impact**: Use-after-free of a `VkAccelerationStructureKHR` read by every RT-shading path
  (shadows, reflections, GI, water refraction) for as long as the allocation keeps failing on
  that frame-in-flight slot. Realistic outcome is a GPU page fault → TDR →
  `VK_ERROR_DEVICE_LOST`; the benign outcome is garbage BVH traversal. The trigger condition
  (device-local allocation failure at TLAS grow time) is precisely the VRAM-pressure regime the
  BLAS budget + LRU eviction machinery exists to survive, so **this is the failure mode that
  fires exactly when the engine is trying to degrade gracefully.**
- **Trigger Conditions**: A frame whose `instance_count` exceeds the slot's `max_instances`
  (`need_new_tlas == true` — i.e. a cell load crossing the 8192 `MIN_TLAS_INSTANCE_RESERVE`, or
  the very first grow) **and** a device-local / host-visible allocation failure for
  `padded_size` (VRAM exhaustion, BAR exhaustion, `OUT_OF_HOST_MEMORY`). Reproducible
  deterministically by fault-injecting a failure return from either `GpuBuffer::create_*` inside
  `ensure_tlas_state`, in the same style as the existing `BYRO_FSR_FORCE_DISPATCH_FAIL` hook.
- **Verification Path**: Not observable in `cargo test` (no headless device). **Validation
  layer**: a `BYRO_VALIDATION=1` release run with the fault injected reports the
  destroyed-object-in-descriptor message at draw time
  (`VUID-vkCmdDrawIndexedIndirect-None-08114` family — "Descriptor in binding #2 index 0 is
  using acceleration structure … that is invalid or has been destroyed"). Without fault
  injection, natural repro needs a VRAM-starved exterior stream; **RenderDoc** would show
  binding 2 as an unresolvable AS handle.
- **Related**: **REN-D5-02 (HIGH) from the concurrent `/audit-renderer` run — the same
  underlying defect, reached independently from a different dimension. Two independent agents
  converging on one defect from different directions is the strongest evidence this suite
  produced.** Also #2481 (CLOSED, BLAS-side sibling of the same "replace without
  destroying/ordering" class); CONC-D1-NEW-02 below shares the "commit before the operation
  succeeds" root cause.
- **Suggested Fix**: Allocate the replacement buffers + AS into locals first and only destroy
  the old `TlasState` once every fallible step has succeeded (allocate-then-swap). As defence in
  depth, make `tlas_written[frame]` two-way: clear it (and re-upload `rt_flag = 0.0` via the
  existing `patch_camera_rt_flag`) whenever `build_tlas` returns `Err`, so the frame degrades to
  non-RT shading instead of reading a dead handle.

---

### HIGH

#### CONC-D1-NEW-02: `build_tlas` commits its BUILD-vs-UPDATE bookkeeping before the build is actually recorded, so a failed frame can let the next frame pick UPDATE with changed BLAS references

- **Severity**: HIGH
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)`:138-165` (commit) vs
  `tlas.rs:200` (`instance_buffer.write_mapped(..)?`) and `tlas.rs:320-363`
  (`built_primitive_count` + `cmd_build_acceleration_structures`), all in
  `AccelerationManager::build_tlas`
- **Status**: NEW
- **Description**: The three pieces of state that `decide_use_update` consults next frame are
  all committed **before** the build is recorded and before the one fallible call in between:
  1. `std::mem::swap(&mut tlas.last_blas_addresses, &mut current_addresses_scratch)` (line 138) —
     promotes this frame's BLAS address list to "the addresses the last BUILD used";
  2. `tlas.needs_full_rebuild = false` (line 164);
  3. `tlas.last_blas_map_gen = map_gen` (line 165).

  `tlas.instance_buffer.write_mapped(device, &instances)?` at line 200 can return `Err`, and
  `built_primitive_count` is only assigned at line 352, inside the BUILD arm that follows. So a
  failure at line 200 leaves the manager asserting "a BUILD landed at generation `map_gen` with
  address list X" when no build was recorded at all.

  Next frame `decide_use_update` sees `needs_full_rebuild == false`,
  `tlas_last_gen == current_gen`, and a zip-compare against the *promoted* cache that now
  matches — so it returns `use_update = true`. The single remaining guard is
  `if use_update && instance_count != tlas.built_primitive_count { use_update = false; }`
  (line 127), which only catches a **count** change. A frame in which the BLAS map generation
  bumped (cell load / eviction / skinned-BLAS rebuild — all four `blas_map_generation` bump
  sites) while the instance *count* stayed constant slips straight through, and the UPDATE is
  submitted with `acceleration_structure_reference` values that differ from those of the last
  real BUILD.
- **Evidence**:
  ```rust
  // tlas.rs:138-141 — cache promoted …
  std::mem::swap(&mut tlas.last_blas_addresses, &mut current_addresses_scratch);
  // tlas.rs:164-165 — … dirty flags cleared …
  tlas.needs_full_rebuild = false;
  tlas.last_blas_map_gen = map_gen;
  // tlas.rs:200 — … and only now the first fallible step
  tlas.instance_buffer.write_mapped(device, &instances)?;
  // tlas.rs:352 / 359 — count recorded and build actually emitted, far later
  tlas.built_primitive_count = instance_count;
  self.accel_loader.cmd_build_acceleration_structures(cmd, &[build_info], &[..]);
  ```
  The function's own doc (`tlas.rs:78-82`) states the invariant this breaks: "only the
  `acceleration_structure_reference` field is off-limits" across an UPDATE.
- **Impact**: A spec-violating TLAS UPDATE
  (`VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03707` class). The refit BVH keeps device
  addresses of BLAS entries that were replaced or evicted; evicted entries are freed
  `DEFAULT_COUNTDOWN` frames later by `tick_deferred_destroy`, at which point every shadow /
  reflection / GI ray traverses freed device memory. This is the "AS built at wrong address"
  severity row. Rated HIGH rather than CRITICAL only because reaching it requires the line-200
  host write to fail first (OOM / flush failure / near-device-lost); the *consequence* is
  CRITICAL-class.
- **Trigger Conditions**: Frame N takes the BUILD path because `blas_map_generation` changed
  (cell load, `evict_unused_blas`, `drop_blas`, `drop_skinned_blas`, or a skinned-BLAS rebuild)
  but `instances.len()` is unchanged from the previous successful BUILD; `write_mapped`'s flush
  fails on that frame. Frame N+1 then selects UPDATE. Deterministically reproducible by
  fault-injecting the `write_mapped` return.
- **Verification Path**: The ordering itself is checkable **in `cargo test`** with the repo's
  existing source-position pinning idiom (cf. `skin_built_this_frame_skip_tests` in
  [skinned_blas_refit.rs](crates/renderer/src/vulkan/context/skinned_blas_refit.rs)) — assert
  the three commit sites appear *after* `cmd_build_acceleration_structures`. The runtime
  consequence is **validation-layer-only** (`VUID-…-pInfos-03707` on the UPDATE call), not
  visible to `cargo test`.
- **Related**: CONC-D1-NEW-01 (same commit-before-success root cause); the #917 /
  REN-D10-NEW-03 fix in `draw.rs:3198-3216` (SVGF / TAA / volumetrics history counters advanced
  only after `queue_submit` returns `Ok`) is the established house pattern this site does not
  follow.
- **Suggested Fix**: Move the `mem::swap`, `needs_full_rebuild = false` and
  `last_blas_map_gen = map_gen` commits to immediately after `cmd_build_acceleration_structures`
  returns, alongside the existing `built_primitive_count` assignment — mirroring the post-submit
  history advance in `draw_frame`.

---

### MEDIUM

#### CONC-D3-NEW-01: The cross-thread ABBA detector closes only length-2 cycles — a 3-lock cycle already present in the live edge graph is invisible to it

> **Emphasised.** The detector's own documentation claims general cycle detection. It does not
> deliver it — and a **complete 3-cycle already exists in the live edge graph today**, with
> `BYRO_LOCK_ORDER_CHECK=1` CI staying **GREEN** on it. It is unreachable only because the three
> producers happen to sit in three different scheduler stages. **Restaging any one of them
> springs the trap while the detector stays silent.**

- **Severity**: MEDIUM
- **Dimension**: 3 — ECS Lock Ordering & Deadlock
- **Location**: [lock_tracker.rs](crates/core/src/ecs/lock_tracker.rs) —
  `global_order::record_and_check` (both the read fast path and the write slow path), plus the
  module doc block above `mod global_order` and the file-header doc item 2.
- **Status**: NEW
- **Description**: `record_and_check` panics only when the *direct* reverse edge already exists:
  for each currently-held `held_id`, it tests `GRAPH[new_id].contains(held_id)` — i.e. "was
  `held_id` ever acquired while `new_id` was held?". There is **no reachability search** over
  `GRAPH`, so a cycle of length ≥ 3 (`A → B`, `B → C`, `C → A`, each edge observed on a
  different thread and each individually legal) is recorded happily and never reported. The
  documentation claims more than the code delivers: the module doc says "If … the graph has a
  cycle … the second observation panics" and "the graph generalizes the guarantee to any N-lock
  hold pattern across the scheduler". The generalization is real for *how many locks are held at
  once* (every held lock contributes an edge) but not for *cycle length*, which is capped at 2.

  This is not hypothetical: the current schedule already writes a complete 3-cycle into `GRAPH`
  on any character-mode frame.
- **Evidence**: The detector, at both check sites:
  ```rust
  if let Some(new_edges) = graph.get(&new_id) {
      for (held_id, held_name) in held_others {
          if new_edges.contains(held_id) {   // ← depth-1 only, no DFS
              panic!("ECS cross-thread deadlock risk (ABBA) …");
  ```
  The three edges that close the triangle, each confirmed by reading the guard lifetimes (not
  inferred from declarations):

  | Edge | Producer | Held-across evidence |
  |---|---|---|
  | `Transform → GlobalTransform` | `make_transform_propagation_system` ([systems.rs](crates/core/src/ecs/systems.rs)) | `query_mut::<Transform>()` is bound first and still live when `query_mut::<GlobalTransform>()` is taken 5 lines later (same fn body, no intervening drop) |
  | `GlobalTransform → CharacterController` | `camera_follow_system` ([character.rs](byroredux/src/systems/character.rs)) | inside the `let (body_pos, eye_height, prev_cam_y) = { … }` block: `gq = world.query::<GlobalTransform>()` then `cq = world.query::<CharacterController>()`; `gq` is read again (`gq.get(cam_entity)`) after `cq` is bound, so it is provably still held |
  | `CharacterController → Transform` | `character_controller_system` (same file) | inside the `let (controller, current_pos, …) = { … }` block: `cq = world.query::<CharacterController>()` then the nested `let pos = { let tq = world.query::<Transform>() … }` |

  The three producers live in `Stage::Early` (via `player_controller_system`),
  `Stage::PostUpdate`, and `Stage::Late` respectively — all three are `add_to_with_access`
  parallel-batch members ([boot.rs](byroredux/src/boot.rs)), so all three edges are recorded
  from rayon worker threads under `BYRO_LOCK_ORDER_CHECK=1`, and the CI `vulkan-validation` job
  stays green.
- **Impact**: The only automated cross-thread deadlock guard in the project reports "clean" for
  an entire class of real deadlocks. Today the triangle is *unreachable* because the three edges
  are produced in three different stages, and `Scheduler::run` runs stages strictly sequentially
  — so this is a detector-coverage defect, not a live hang. The blast radius is what happens
  next: any stage merge, any promotion of one of these three systems into a sibling's stage, or
  any new parallel system that reproduces one of these edges in a stage where another already
  exists, produces a hard hang (three rayon workers blocked forever, no panic, no log) that the
  `lock-order-check` and `vulkan-validation` jobs will both certify as passing.
  `camera_follow_system` and `transform_propagation` are both on the renderer-feeding path, so
  the hang would present as a frozen render loop.
- **Trigger Conditions**: Detection gap: **always** — no timing window needed; run CI today with
  `BYRO_LOCK_ORDER_CHECK=1` on a character-mode cell and all three edges land in `GRAPH` with
  zero diagnostics. Actual deadlock requires the three edge-producers to be co-scheduled: e.g.
  move `camera_follow_system` from `Stage::Late` to `Stage::PostUpdate` (it already carries an
  ordering comment tying it to `physics_sync_system`, so a future Physics/PostUpdate merge is
  plausible) and the `GlobalTransform → CharacterController` and `Transform → GlobalTransform`
  holds overlap; add `character_controller_system`'s `CharacterController → Transform` hold on a
  third worker and the cycle closes with no participant able to proceed.
- **Verification Path**: **Pure-CPU, `cargo test`-observable** — no Vulkan or RenderDoc needed.
  Add a unit test in `lock_tracker.rs`'s existing `global_graph_detector_end_to_end` style: with
  `set_enabled_for_tests(true)`, drive `A→B`, then `B→C`, then `C→A` on three sequential (or
  spawned) scopes and assert the third acquisition panics. It currently does not. Contrast with
  the existing scenario 1 in that test, which only exercises the 2-cycle the code does handle —
  which is precisely why the gap survived.
- **Related**: #2385 (GRAPH poison handling), #2386 (recursive same-type reads invisible to the
  graph), #2387 (no cross-worker test coverage), #2547 (detector documented as debug-only, omits
  default-off), #2388 (six inverted pairs among exclusives). None of these covers cycle length —
  they are all about *whether* an edge is recorded or *whether* the detector runs, not about the
  cycle-closure predicate.
- **Suggested Fix**: Replace the `GRAPH[new_id].contains(held_id)` test with a reachability
  probe — before inserting `held_id → new_id`, DFS/BFS from `new_id` over `GRAPH` and panic if
  `held_id` is reachable. The graph is tiny (one node per locked type) and the probe only runs on
  the novel-edge slow path, so the steady-state cost is unchanged. Until then, at minimum correct
  the module doc to say the detector closes *direct* two-lock cycles only, so a green
  `lock-order-check` is not read as proof of acyclicity.

---

#### CONC-D3-NEW-02: `camera_follow_system` reads `PlayerMode` undeclared — a third Late-parallel system with the #1787/#2389 defect, and the only non-telemetry one

- **Severity**: MEDIUM
- **Dimension**: 3 — ECS Lock Ordering & Deadlock (declaration-backed deadlock guard)
- **Location**: declaration at [boot.rs](byroredux/src/boot.rs)
  (`scheduler.add_to_with_access(Stage::Late, crate::systems::camera_follow_system, …)`); body at
  [character.rs](byroredux/src/systems/character.rs) — `camera_follow_system`, first statement.
- **Status**: NEW (explicitly *not* covered by #2389, whose body names only `log_stats_system`
  and `metrics_sample_system`)
- **Description**: `camera_follow_system`'s very first statement acquires a read guard on the
  `PlayerMode` resource as an early-out gate. Its `Access` declaration lists `PlayerEntity`,
  `ActiveCamera`, `InputState` (resources) and `CharacterController`, `GlobalTransform`,
  `Transform` (components) — `PlayerMode` is absent. `Stage::Late` is the largest parallel batch
  in the engine (4 systems, 6 analyzed pairs), and the analyzer therefore reports
  `AccessConflict::None` for pairings it has not proved disjoint on this resource.

  This matters more here than in #2389's two cases: those are telemetry systems whose entire
  effect is writing snapshot resources. `camera_follow_system` writes `Transform` and
  `GlobalTransform` on the active camera — the pose the renderer, the audio listener, and
  `submersion_system` all consume later in the frame.
- **Evidence**:
  ```rust
  // byroredux/src/systems/character.rs — camera_follow_system, first statement
  let mode = world
      .try_resource::<PlayerMode>()      // ← undeclared
      .map(|r| *r)
      .unwrap_or_default();
  if mode != PlayerMode::Character { return; }
  ```
  versus the registration in `boot.rs`, whose `Access::new()` chain runs
  `reads_resource::<PlayerEntity>` / `ActiveCamera` / `InputState` and then goes straight to the
  component list — no `PlayerMode` entry. Note the Early batch's `player_controller_system`
  *does* declare `reads_resource::<PlayerMode>()`, so the omission is asymmetric within the same
  file.
- **Impact**: No live race today, and the reason was confirmed rather than assumed: the only
  writer of `PlayerMode` is `toggle_player_mode`, whose signature is `(&mut World)`
  ([main.rs](byroredux/src/main.rs) key handler) — a `&mut World` cannot coexist with the
  `&World` the scheduler hands systems, so the write is structurally excluded from the parallel
  window. The defect is that the `known_conflict_count() == 0` invariant asserted in
  `install_runtime_registries` — the thing that makes cross-thread ABBA structurally unreachable
  among parallel systems (see CONC-D3-NEW-03) — is computed from an incomplete declaration. The
  moment `PlayerMode` acquires a system-level writer in `Stage::Late` (a mode-switch script
  effect, a save-load apply, a debug command promoted out of the exclusive drain), the analyzer
  will not see the pair, `sys.accesses` will keep printing 0 conflicts, and the resulting
  read/write overlap gets no diagnostic.
- **Trigger Conditions**: Detection gap: present on every frame the schedule is built. Realized
  race requires a second `Stage::Late` *parallel* system that writes `PlayerMode` while
  `camera_follow_system` holds its read guard — i.e. the two co-scheduled on different rayon
  workers inside the same `data.parallel.par_iter_mut()` batch. Not reachable with today's
  registration set.
- **Verification Path**: **`cargo test`-observable.** Run the `byro-dbg` `sys.accesses` command
  (or `byroredux/src/scheduler_access_tests.rs`) — the Late-stage report shows
  `camera_follow_system ↔ *` as `None` on `PlayerMode`. Direct proof: add
  `.reads_resource::<PlayerMode>()` to the declaration and confirm `known_conflict_count()` stays
  0 (no real conflict exists today), which is the same shape of fix #1787 applied to
  `physics_sync_system`'s `ContactConfig`.
- **Related**: #2389 (same class, the other two Late-parallel systems), #1787 / CONC-D4-01
  (closed, `physics_sync_system`'s `ContactConfig`), #2393 (zero-conflict invariant
  near-vacuous).
- **Suggested Fix**: Add `.reads_resource::<crate::systems::PlayerMode>()` to
  `camera_follow_system`'s `Access` in `boot.rs`. Fix alongside #2389 so all four Late-parallel
  declarations are complete in one pass.

---

### LOW

#### CONC-D1-NEW-03: Dimension-1 checklist instructs auditors to confirm the queue Mutex guard is *not* held across `queue_submit` — the opposite of the shipped, deliberately-fixed behaviour

> Also treated as the lead item of section 5 (Documentation & Skill-Text Drift), because acting
> on the skill text as written would **re-open two correct fixes**.

- **Severity**: LOW (documentation)
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [SKILL.md](.claude/commands/audit-concurrency/SKILL.md) (Dimension 1, "Queue
  submission is single-Mutex" bullet); contradicted code at
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:3158-3196` + `:3243-3258` and
  [texture.rs](crates/renderer/src/vulkan/texture.rs)`:787-802`
- **Status**: NEW
- **Description**: The checklist states that because `vk::Queue` is `Copy`, "the canonical
  pattern is lock → copy the handle out → drop the guard → `queue_submit`: confirm the guard is
  **not** held across `queue_submit`/`queue_present`". The live code deliberately does the
  opposite, and says so in-line: `draw.rs:3160-3165` binds the `MutexGuard` specifically so it
  spans the call, citing `VUID-vkQueueSubmit-queue-00893` and the audit finding CONC-D2-NEW-01
  (2026-05-16) that introduced it; `texture.rs:793-798` repeats the reasoning and refines it
  under CONC-D1-01 (#1713) — guard held across the *submit*, released before the *fence wait*.
  There is a live regression test pinning this: `one_time_lock_scope_tests::…` in
  `texture.rs:846-880` asserts the lock → submit → wait ordering. Following the checklist
  literally would re-open two closed, correct fixes and re-introduce a genuine
  external-synchronisation violation.
- **Evidence**: `draw.rs:3166-3169` binds `let queue = self.graphics_queue.lock()…;` then calls
  `queue_submit(*queue, …)` and only `drop(queue)` afterwards (lines 3181 / 3195).
  `texture.rs:799-802` scopes `let q = queue.lock()…; device.queue_submit(*q, …)` to the submit
  only, with the fence wait outside.
- **Impact**: Audit-process defect. Not a runtime bug; it manufactures false findings and, if
  acted on, would produce a real CRITICAL-class data race on the queue.
- **Trigger Conditions**: n/a (documentation).
- **Verification Path**: `cargo test -p byroredux-renderer one_time_lock_scope_tests` already
  pins the correct behaviour.
- **Related**: CONC-D2-NEW-01 (audit 2026-05-16), CONC-D1-01 / #1713.
- **Suggested Fix**: Reword the bullet to "confirm the guard **is** held across `queue_submit` /
  `queue_present` (`VUID-vkQueueSubmit-queue-00893`) and released before any subsequent
  `wait_for_fences`".

---

#### CONC-D3-NEW-03: The zero-conflict scheduler invariant is what actually prevents cross-thread ABBA, and that is documented nowhere

- **Severity**: LOW
- **Dimension**: 3 — ECS Lock Ordering & Deadlock
- **Location**: [boot.rs](byroredux/src/boot.rs) — `install_runtime_registries`
  (`debug_assert_eq!(report_snapshot.known_conflict_count(), 0, …)`);
  [access.rs](crates/core/src/ecs/access.rs) — `analyze_pair`.
- **Status**: NEW
- **Description**: `analyze_pair` treats `WriteRead`, `ReadWrite` *and* `WriteWrite` overlaps as
  conflicts. Any cross-thread ABBA between two parallel systems needs, on each of the two shared
  locks, at least one side holding or requesting it in a blocking (write) mode — which is exactly
  an `analyze_pair` conflict. Therefore `known_conflict_count() == 0` over a stage's parallel
  batch is a *proof* that no ABBA exists between any two of its members, and it is the
  load-bearing reason this dimension has no reachable finding today. Nothing says so: the
  assert's own message frames it as a throughput/correctness nag ("make one side exclusive or
  split the access (see sys.accesses)"), `lock_tracker`'s module doc presents the runtime graph
  as the cross-thread guard without mentioning the static one, and
  [contributing.md](docs/contributing.md)'s `lock-order-check` row likewise.
- **Evidence**: `analyze_pair` runs six `collect_overlap` calls — write×read, read×write,
  write×write for components and again for resources — and returns `AccessConflict::Conflict` if
  any pair is non-empty. Paired with `install_runtime_registries`'s three `debug_assert_eq!`s
  (`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count` all 0), the parallel
  batches are provably lock-disjoint. Confirmed by enumerating the two multi-member batches:
  `Stage::Early` = {`player_controller_system`, `weather_system`, `timer_tick_system`} and
  `Stage::Late` = {`camera_follow_system`, `reverb_zone_system`, `log_stats_system`,
  `metrics_sample_system`}; the only lock any two share is `TotalTime`, read-only on both sides.
- **Impact**: The invariant is easy to weaken by accident because nobody knows it is a deadlock
  guarantee. Two concrete ways it silently degrades: (1) the guard is `debug_assert_eq!`, so a
  release-only schedule divergence ships unchecked; (2) it is only as strong as the declarations,
  which is what CONC-D3-NEW-02 and #2389 erode. A reviewer told "this is just the parallelism
  report" will accept an incomplete declaration; a reviewer told "this is the deadlock proof"
  will not.
- **Trigger Conditions**: N/A — documentation/robustness gap, no timing window.
- **Verification Path**: `cargo test` / code review only. Nothing to observe at runtime; the
  check is a build-time property of the registration list.
- **Related**: #2393 (invariant near-vacuous — only 9 of ~53 systems ever paired; the two
  findings compound: a vacuous proof that nobody knows is a proof), #2391
  (`add_exclusive_with_access` has zero call sites, so the 43 exclusives get no such proof at
  all — they rely on `Scheduler::run`'s parallel-then-exclusive sequencing instead).
- **Suggested Fix**: One comment block at the `known_conflict_count` assert in `boot.rs` naming
  the property ("zero declared conflicts among a stage's parallel batch ⇒ the batch is
  lock-disjoint ⇒ no cross-thread ABBA is possible between its members; this, not the runtime
  graph, is the primary guard") and a cross-reference from `lock_tracker`'s module doc. Consider
  promoting the assert from `debug_assert_eq!` to a plain `assert!` — it runs once at
  construction, so the release cost is a single comparison.

---

## 3. Hypotheses (need validation-layer / RenderDoc)

**These are NOT confirmed bugs.** No engine was launched during this audit, so the claims below
could not be settled from source alone. Each carries the named signal that would confirm or
refute it. **Do not change this code before that signal is observed** — that is the binding
speculative-Vulkan-fix guardrail, and these entries exist precisely so the guardrail is not
quietly stepped over.

### CONC-D1-H1: `recreate_image_available_for_frame` destroys a binary semaphore that may still carry an outstanding `vkAcquireNextImageKHR` signal

- **Severity if confirmed**: HIGH (Vulkan spec violation) — **HYPOTHESIS, not a fix**
- **Dimension**: 1 — Vulkan Queue & AS Sync
- **Location**: [sync.rs](crates/renderer/src/vulkan/sync.rs)`:272-288`
  (`FrameSync::recreate_image_available_for_frame`); call sites
  [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:1424`, `:1519`, `:1535`, `:1731`,
  `:3101`, `:3153`, `:3184`
- **Status**: NEW (**unconfirmed**)
- **Description**: The #910 recovery path exists because a successful `acquire_next_image` leaves
  `image_available[frame]` signal-pending when a later `?` aborts the frame before `queue_submit`
  consumes it. The recovery destroys the old semaphore and installs a fresh one *immediately*,
  with no wait of any kind. `vkAcquireNextImageKHR`'s signal is performed by the presentation
  engine, is not a queue batch, and is therefore **not** covered by `device_wait_idle` — and no
  wait is attempted here anyway. Destroying a semaphore with a pending signal operation is the
  object-lifetime case the validation layer's semaphore tracker flags. The claim that cannot be
  settled from source alone is whether VVL treats an acquire-pending binary semaphore as "in use"
  at `vkDestroySemaphore` on this driver/loader combination; the Vulkan spec text is about
  submitted batches, and the acquire signal sits outside that wording.
- **Evidence**: `sync.rs:277-282` — `create_semaphore` → `mem::replace` → `destroy_semaphore(old)`,
  no fence, no idle. The doc comment's own safety contract (lines 263-269) only claims "no
  command buffer that waits on this semaphore is currently submitted", which is true but does not
  address the pending *signal*.
- **Impact if confirmed**: Undefined behaviour on an already-failing frame, plausibly a
  driver-side crash or a permanently wedged acquire slot — i.e. the recovery path turns a
  recoverable error into an unrecoverable one.
- **Trigger Conditions**: Any `?`-propagated failure in `draw_frame` between a successful
  `acquire_next_image` and `queue_submit` (`reset_command_buffer` / `begin_command_buffer` /
  `end_command_buffer` / `reset_fences` failure, or a failed submit), while the acquired image's
  presentation-engine signal has not yet completed — i.e. under FIFO with the compositor still
  holding the image.
- **Verification Path — CONFIRMING SIGNAL**: **Validation layer only.** Run a `BYRO_VALIDATION=1`
  release build with fault injection on one of those error arms (e.g. fault-inject
  `begin_command_buffer`); confirm or refute a **`VUID-vkDestroySemaphore-semaphore-05149`** /
  "cannot be called on `VkSemaphore` … that is currently in use" report at the recreate.
  Invisible to `cargo test`; **RenderDoc will not show it either.** Do **not** change this code
  before that run.
- **Suggested direction if confirmed**: Rather than destroy-and-replace, drain the pending signal
  first — submit an empty batch that waits on `image_available[frame]` with a throwaway fence and
  wait that fence, then recreate. (Explicitly *not* proposed as a fix today.)

---

## 4. Guards Verified Intact

Consolidated PASS list — **48 guards**: 33 from Dimensions 1 & 2, 15 from Dimension 3. No
cross-dimension duplicates were found (the two sets cover disjoint subsystems), so no dedup
collapse was needed. This is the evidence of thoroughness: the dimensions that produced few
findings produced few findings because the guards hold, not because they were skimmed.

### Dimensions 1 & 2 — Vulkan queue / AS sync and compute→AS→fragment chains (33)

| # | Guard | Site | Status |
|---|---|---|---|
| 1 | Queue Mutex held across `queue_submit` (VUID-…-queue-00893), released before the fence wait — CONC-D2-NEW-01 / CONC-D1-01 #1713 | `draw.rs:3166-3195`, `texture.rs:793-802`, test `one_time_lock_scope_tests` | PASS |
| 2 | Same discipline at the `queue_present` site | `draw.rs:3244-3258` | PASS |
| 3 | `present_queue` is an `Arc::clone` of `graphics_queue` on matching families (#284) | `context/mod.rs:1668-1675` | PASS |
| 4 | `in_flight[frame]` **and** `in_flight[prev]` both waited before cmd-buffer / per-frame-resource reuse (#282); `MAX_FRAMES_IN_FLIGHT == 2` makes that device-idle-for-prior-frames, pinned by a `const _: () = assert!` | `draw.rs:1346-1360`, `sync.rs:6,33-35` | PASS |
| 5 | `reset_fences` immediately before `queue_submit` (#952 / REN-D1-NEW-04), with fence + semaphore recreation on both failure arms | `draw.rs:3132-3196`, `sync.rs:290-340` | PASS |
| 6 | Every `?` exit between `acquire_next_image` and `queue_submit` calls `recreate_image_available_for_frame` (#910) — 6/6 sites | `draw.rs:1399-3156` | PASS |
| 7 | `render_finished` is **per swapchain image**, not per FIF slot (VUID-vkQueueSubmit-pSignalSemaphores-00067) | `draw.rs:3111-3123`, `sync.rs:56-99` | PASS |
| 8 | `images_in_flight[img]` waited before image reuse; entries nulled when the owning fence is recreated (#1188) | `draw.rs:1412-1429`, `sync.rs:322-331` | PASS |
| 9 | AS build → ray-query read barrier: `AS_BUILD/AS_WRITE → FRAGMENT\|COMPUTE / AS_READ` after `build_tlas` (dst stages match the only two shader stages that declare `accelerationStructureEXT`) | `draw.rs:2203-2216` | PASS |
| 10 | Skinned-BLAS refit → TLAS-build read barrier `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` | `skinned_blas_refit.rs:563-572` | PASS |
| 11 | Static-BLAS build → compaction-query read barrier | `blas_static.rs:868-882` | PASS |
| 12 | **#1790** — `record_scratch_serialize_barrier` dst mask is `AS_WRITE \| AS_READ`, not WRITE-only (covers same-cmd BUILD-then-UPDATE adjacency) | `blas_skinned.rs:682-686` | PASS |
| 13 | **#507945d8 / #1436** — AS-build **input** barriers use `SHADER_READ` at `ACCELERATION_STRUCTURE_BUILD`, not `AS_READ_KHR`: instance-copy→TLAS-build and skin-compute→BLAS-build | `tlas.rs:251-272`, `skinned_blas_refit.rs:394-405` | PASS |
| 14 | **#1782** — `blas_scratch_buffer` retirement routes through `pending_destroy_scratch` on grow and shrink | `blas_static.rs:326`, `:788`, `memory.rs:79`, `:92` | PASS |
| 15 | `build_skinned_blas_batched_on_cmd`'s own grow-destroy is immediate **by design** (post-fence-wait) — not flagged, per brief | `blas_skinned.rs:229-230` | PASS (whitelisted) |
| 16 | **#a476b256 / #1449** — BLAS eviction + `drop_blas` + `drop_skinned_blas` route through `pending_destroy_blas` (`DEFAULT_COUNTDOWN`); shutdown drains via `drain_pending_destroy` | `blas_static.rs:61`, `:1315`, `blas_skinned.rs:711-719`, `blas_static.rs:102-145` | PASS |
| 17 | Deferred-destroy tick runs **after** the fence wait, and after `texture_registry.begin_frame` (#418) | `draw.rs:1443-1469` | PASS |
| 18 | `recreate_swapchain` pays `device_wait_idle` before destroying any swapchain-dependent resource; `set_upscaler_mode` likewise | `resize.rs:32-38`, `:1125-1130`, `:1168` | PASS |
| 19 | Blocking one-time submits (`build_blas_batched`, texture flush) sit in the streaming / cell-load path, **not** inside `draw_frame`'s recording window | `resources.rs:113-234`, `spawn.rs:663`, `exterior.rs:1090` | PASS |
| 20 | TLAS BUILD-vs-UPDATE gated on `blas_map_generation` + per-instance address zip + `built_primitive_count` equality (VUID-…-03708) | `predicates.rs:188-229`, `tlas.rs:110-129` | PASS (but see CONC-D1-NEW-02 for the commit-ordering hole in the *same* machinery) |
| 21 | Skin chain M29: palette dispatch → `COMPUTE_WRITE → SHADER_READ (COMPUTE\|VERTEX)` → per-entity skin dispatch → `COMPUTE_WRITE → AS_BUILD/SHADER_READ` → first-sight BUILD batch + refit loop → `AS_WRITE → AS_READ` → TLAS build → ray query | `draw.rs:2150-2175`, `skinned_blas_refit.rs:394-572` | PASS |
| 22 | Raster path uses inline skinning in `triangle.vert`, so no `VERTEX_INPUT` barrier is required; no raster-from-skinned-SSBO path was added | `draw.rs:2107-2112` | PASS |
| 23 | Bone-world staging→device copy and M29.6 bind-inverse staging→persistent copy each emit `TRANSFER_WRITE → SHADER_READ` at `COMPUTE_SHADER` before the palette dispatch reads them | `scene_buffer/upload.rs:265-311`, `:371-445` | PASS |
| 24 | SVGF / TAA cross-frame ping-pong reads `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT`, with `const _: () = assert!(MAX_FRAMES_IN_FLIGHT >= 2)` compile-time gates (#918) | `svgf.rs:70-81`, `:792`, `taa.rs:47-57`, `:535` | PASS |
| 25 | Volumetrics ping-pong: `previous = (frame + MAX-1) % MAX`, with `pre_inject` (prev READ → curr WRITE) and `history_ready` (prev WRITE → curr READ) image barriers; `inj_to_int`, `pre_int_write`, and `post_int` (COMPUTE→FRAGMENT) complete the chain | `volumetrics.rs:918`, `:1565-1665` | PASS |
| 26 | **#1105** volumetrics latch: `write_tlas` sets `tlas_written[frame]`, `dispatch` `debug_assert!`s then resets; `write_lights_and_clusters` / `lights_written` mirror it; both writers and `dispatch` live in the *same* `(Some(tlas), Some(lights))` arm, so set/reset is symmetric and the neutral-frame arm never asserts | `volumetrics.rs:1486-1500`, `:1774-1790`, `post_passes.rs:484-635` | PASS |
| 27 | **#931** bloom RAW chain: per-mip `SHADER_WRITE → SHADER_READ` post-barrier on both pyramids; the final `up_mips[0]` publishes with dst stage `FRAGMENT_SHADER` for composite | `bloom.rs:563-644` | PASS |
| 28 | Caustic accumulator CLEAR → COMPUTE → FRAGMENT, plus the #2507 skip-clear path (`TRANSFER → FRAGMENT` directly) | `caustic.rs:809-920`, `:946-1000` | PASS |
| 29 | Water-caustic per-FIF accumulator: `clear_pre_render_pass` before `vkCmdBeginRenderPass`, `barrier_post_render_pass` before the composite read | `draw.rs:2985-2988`, `post_passes.rs:266-269`, `water_caustic.rs:282-387` | PASS |
| 30 | ReSTIR reservoir ping-pong: `prev_buffer(frame) = buffers[(frame + 1) % MAX]`, bindings 16/17 rewritten at init + resize | `restir.rs:86-94`, `descriptors.rs:137-166` | PASS |
| 31 | MaterialBuffer (R1) upload is host-side and lands before the bulk `HOST_WRITE → VERTEX\|FRAGMENT\|COMPUTE\|DRAW_INDIRECT` barrier that precedes the render pass — has not moved into a compute path | `draw.rs:2710`, `:2960-2976` | PASS |
| 32 | **#2494** skin-slot LRU sweep + `pending_skin_unload_victims` drain sit outside the `(global_vert_buf, bone_buffer)` guard; victims are ECS-despawned before `build_render_data`, so no evicted slot can be referenced by an already-recorded dispatch in the same cmd | `skinned_blas_refit.rs:590-660` + its two source-position tests | PASS |
| 33 | Main render pass declares both incoming and outgoing `SUBPASS_EXTERNAL` dependencies (#947 / #573), so post-pass compute reads of the HDR/G-buffer attachments are ordered | `context/helpers.rs:248-295` | PASS |

### Dimension 3 — ECS lock ordering & deadlock (15)

1. **TypeId-sorted acquisition (#313)** — all four multi-lock accessors in
   [world.rs](crates/core/src/ecs/world.rs) (`query_2_mut`, `query_2_mut_mut`, `resource_2_mut`,
   `try_resource_2_mut`) branch on `if id_a < id_b`, and **both** the real lock acquisition and
   the `lock_tracker` scope construction are emitted in TypeId-ascending order inside each arm.
   No caller-order leakage. `try_resource_2_mut` delegates to `resource_2_mut` after a lock-free
   `contains_key` pre-check (#465), so it inherits the ordering rather than re-implementing it.
   There is no fifth/newer multi-lock accessor — every `pub fn query_*` / `resource_*` in
   `world.rs` was enumerated. **PASS**
2. **`assert_ne!` same-type panic** — present on all four accessors and covered by four
   `#[should_panic]` tests (`query_2_mut_same_type_panics`, `query_2_mut_mut_same_type_panics`,
   `resource_2_mut_same_type_panics`, `try_resource_2_mut_same_type_panics`, `world_tests.rs`).
   **PASS**
3. **#313 order pin exists implicitly** — `world_tests.rs` exercises the same storage pair in
   *both* generic orders (`query_2_mut::<Position, Velocity>` and
   `query_2_mut::<Velocity, Position>`) and the same resource pair in both orders
   (`resource_2_mut::<ResA, ResB>` / `<ResB, ResA>`) within one test binary. Under the
   `lock-order-check` CI job a regression of the TypeId sort would record opposing edges and
   panic. (Implicit, not asserted — worth a comment, not a finding.) **PASS**
4. **#2149 defuse-after-wrapper ordering** — `query`, `query_mut`, `query_2_mut`,
   `query_2_mut_mut`, and `World::get` all construct the `QueryRead`/`QueryWrite`/`ComponentRef`
   wrapper *before* calling `scope.defuse()`, and the paired accessors defuse both scopes only
   after both wrappers exist. Pinned by
   `defuse_follows_wrapper_construction_at_every_query_site`.
   *Disproved candidate finding*: the resource accessors defuse *before*
   `ResourceRead::new`/`ResourceWrite::new`, which looks like the same bug — it is not, because
   those constructors are infallible (the fallible `downcast_ref().expect(...)` lives in `Deref`,
   not `new`), so there is no panic window to orphan a tracker row. **PASS**
5. **Poison handling** — every storage and resource acquisition in `world.rs` resolves
   `PoisonError` through `storage_lock_poisoned::<T>()`,
   `storage_lock_poisoned_erased(type_name)`, or `resource_lock_poisoned::<R>()`. Includes the
   type-erased `&mut self` paths (`despawn`, `despawn_batch`, `clear_entities`,
   `shrink_storages`) which route the `TypeId` through the #466 `type_names` side-table. No
   `.read().unwrap()` / `.write().unwrap()` on a storage or resource lock anywhere in
   `crates/core/src/ecs/` outside tests. Poison-unwind tracker cleanliness is pinned across nine
   methods in `world_tests.rs`. **PASS**
6. **No `World::insert` during system execution** — structurally impossible: `System::run` takes
   `&World` and `insert`/`spawn`/`despawn`/`remove`/`insert_resource`/`remove_resource` all take
   `&mut self`. Verified the same for the one mode-mutating helper,
   `toggle_player_mode(&mut World)`, which is called from the winit key handler, not from a
   system. **PASS**
7. **CI `lock-order-check` coverage** — [ci.yml](.github/workflows/ci.yml) has both the dedicated
   `lock-order-check` job (`BYRO_LOCK_ORDER_CHECK: 1` + `cargo test --workspace`) *and*
   `BYRO_LOCK_ORDER_CHECK: 1` on the `vulkan-validation` job, the only job that boots the real
   engine and dispatches the parallel batch across rayon workers. **The 2026-07-25
   CONC-D4-NEW-01 gap is closed (#2137**, comment cites it in-file). `docs/contributing.md`
   documents both. **PASS — no finding.**
8. **`lock_tracker` coverage asymmetry is correctly implemented** — `track_read`/`track_write`'s
   conflict panics carry no `cfg` gate (always-on, debug and release); only the `held_others` Vec
   collection + `record_and_check` call sit behind `#[cfg(debug_assertions)]` (#823), and
   `global_order` is further gated on `BYRO_LOCK_ORDER_CHECK` via a `LazyLock<AtomicBool>`. The
   `is_new` transition gate correctly avoids self-edges on re-entrant reads. (Doc wording of this
   asymmetry is #2547, open.) **PASS**
9. **#2134 fix holds** — `follow_system_inner`, `escort`, `travel`, `guard`, `patrol`, `wander`
   all snapshot component reads into a scratch `Vec` and drop those guards *before*
   `try_resource::<PhysicsWorld>()`, with in-file comments citing #2134 and per-system
   `#2134 regression guard` tests that install a real `PhysicsWorld`. No
   `GlobalTransform`-under-`PhysicsWorld` inversion survives in these six. **PASS**
10. **#2392 seat-reservation prune ordering** — `prune_seat_reservations`
    ([cell_loader/references/mod.rs](byroredux/src/cell_loader/references/mod.rs)) snapshots
    `Furniture` + `Seated` into owned collections before `try_resource_mut::<SeatReservations>()`,
    matching `sandbox_seat_system_inner`'s component-before-resource order. The in-file comment
    states the reason. Recent code got this right. **PASS**
11. **Renderer-feeding exclusive systems' guard lifetimes** — `billboard` (single
    `GlobalTransform` write query, #829 collapsed the read/write cycle), `bounds` (scoped
    dirty-drain write, then one fixed read set), `light_anim` (single
    `query_2_mut::<LightFlicker, LightSource>`), `particle` (single
    `query_2_mut::<GlobalTransform, ParticleEmitter>`), `water::submersion_system` (explicit
    `drop(gq)` / `drop(wq)` / `drop(vq)` before the `SubmersionState` write), `camera`
    (`RapierHandles` probe is a temporary, dropped before the `Transform` write) — all clean, no
    guard held across a re-entry into the same storage. **PASS**
12. **`weather_system` internal resource order is self-consistent** — `WeatherTransitionRes`(W)
    is dropped before `WeatherDataRes`(R) is taken; the only held pair is
    `WeatherDataRes → WeatherTransitionRes`, and `promote_weather_transition_target` drops its
    `WeatherTransitionRes` read before taking `WeatherDataRes` write, so no reverse edge exists.
    The `if world.try_resource::<…>().is_some()` gate is a `match`-scrutinee-free `if` condition,
    so its temporary guard drops before the block body calls back into the same resource. **PASS**
13. **No off-scheduler thread takes ECS locks** — the streaming worker
    ([streaming.rs](byroredux/src/streaming.rs)) parses NIFs off-thread and returns payloads over
    an mpsc channel without touching `World`; the debug server queues commands drained by an
    exclusive `Stage::Late` system. *Disproved candidate finding*: streaming's `into_par_iter()`
    shares rayon's global pool with `Scheduler::run`'s `par_iter_mut()`, but the calling thread is
    a plain `std::thread` (not a pool worker), so it blocks rather than work-stealing, and the
    injected NIF-parse jobs take no ECS locks — a pool worker mid-system cannot nest one. **PASS**
14. **Exclusive systems never overlap anything** — `Scheduler::run` joins
    `data.parallel.par_iter_mut().for_each(…)` before the `for entry in &mut data.exclusive` loop,
    per stage. 43 exclusives (interaction, script dispatchers, `spin`, `animate_lights`,
    `footstep`, `particle`, `billboard`, `bounds`, `submersion`, the seven AI procedures,
    `ragdoll_writeback`, `audio`, `event_cleanup`, the debug drain) therefore have no co-scheduled
    peer, which is why their several non-TypeId-sorted acquisition orders (#2388, #2399, #2400,
    #2404, #2270) are latent rather than live. **PASS**
15. **`byroredux/src/render/`** — `build_render_data` and its per-pass modules take ECS locks, but
    only from the main thread after `scheduler.run()` returns; they cannot overlap a system. Guard
    hygiene is nonetheless clean (`drop(active)` before the `Transform` query in `camera.rs`, the
    explicit "two resource locks are never held simultaneously" discipline in `lights.rs`).
    `mod.rs`'s `cell_lit` resource guard held across the static-mesh query set is a
    resource↔storage unordered pair of the #2404 class, latent for the same main-thread-only
    reason. **PASS**

---

## 5. Documentation & Skill-Text Drift

### 5.1 — LEAD ITEM: CONC-D1-NEW-03 — the `/audit-concurrency` skill text is wrong, and acting on it would break working code

This is a correction to **the audit skill itself and to the instruction the orchestrator relayed
to the auditing agent**. It is recorded here prominently so it is not lost between runs.

**What the skill says.** The Dimension 1 checklist in
[SKILL.md](.claude/commands/audit-concurrency/SKILL.md) instructs auditors: because `vk::Queue`
is `Copy`, "the canonical pattern is lock → copy the handle out → drop the guard →
`queue_submit`: confirm the guard is **not** held across `queue_submit`/`queue_present`". The
orchestrator relayed this same instruction verbatim to the auditing agent for this run.

**What the code does.** The shipped code **deliberately DOES hold the guard across the submit**,
and documents why in-line:

- [draw.rs](crates/renderer/src/vulkan/context/draw.rs)`:3160-3165` binds the `MutexGuard`
  specifically so it spans the call, citing **`VUID-vkQueueSubmit-queue-00893`** and the audit
  finding **CONC-D2-NEW-01 (2026-05-16)** that introduced it.
- [texture.rs](crates/renderer/src/vulkan/texture.rs)`:793-798` repeats the reasoning and
  **refines it under CONC-D1-01 / #1713** — guard held across the *submit*, released before the
  *fence wait*.
- A **live regression test** pins the ordering: `one_time_lock_scope_tests::…` at
  `texture.rs:846-880` asserts lock → submit → wait.

**Why this is urgent.** `vkQueueSubmit` requires external synchronisation of the `VkQueue`
parameter. Copying the `Copy` handle out and dropping the guard defeats that entirely: two
threads could then be inside `vkQueueSubmit` on the same queue simultaneously. **Following the
checklist literally would re-open two closed, correct fixes (CONC-D2-NEW-01 and CONC-D1-01 /
#1713) and re-introduce a genuine CRITICAL-class data race.**

**Required edit (must-fix).** In
[SKILL.md](.claude/commands/audit-concurrency/SKILL.md), Dimension 1, "Queue submission is
single-Mutex" bullet — replace the current text with:

> confirm the guard **is** held across `queue_submit` / `queue_present`
> (`VUID-vkQueueSubmit-queue-00893`) and released before any subsequent `wait_for_fences`.
> Regression-pinned by `one_time_lock_scope_tests` in `crates/renderer/src/vulkan/texture.rs`.
> Do **not** report guard-held-across-submit as a finding; it is the correct pattern here, fixed
> deliberately under CONC-D2-NEW-01 (2026-05-16) and refined under CONC-D1-01 / #1713.

Until this edit lands, every future `/audit-concurrency` run will manufacture the same false
finding, and the orchestrator's relayed instruction will keep propagating it.

### 5.2 — CONC-D3-NEW-01 (doc half): `lock_tracker`'s module doc overclaims

The `global_order` module doc states "If … the graph has a cycle … the second observation panics"
and that "the graph generalizes the guarantee to any N-lock hold pattern across the scheduler".
The generalization holds for *how many locks are held at once*, **not for cycle length**, which
is hard-capped at 2 by the `GRAPH[new_id].contains(held_id)` predicate. Even if the reachability
fix is deferred, the doc must be corrected to say the detector closes *direct two-lock cycles
only* — otherwise a green `lock-order-check` job is read as proof of acyclicity, which it is not.
The file-header doc item 2 needs the same correction.

### 5.3 — CONC-D3-NEW-03: the real deadlock proof is undocumented

The load-bearing cross-thread ABBA guarantee is the **static** `known_conflict_count() == 0`
assert in [boot.rs](byroredux/src/boot.rs)'s `install_runtime_registries`, not the runtime graph.
Nothing says so: the assert's message frames it as a throughput nag,
[lock_tracker.rs](crates/core/src/ecs/lock_tracker.rs)'s module doc presents the runtime graph as
*the* cross-thread guard, and [contributing.md](docs/contributing.md)'s `lock-order-check` row
does the same. Needs one comment block at the assert plus a cross-reference from the
`lock_tracker` module doc.

### 5.4 — In-code doc contradicted by its own function (CONC-D1-NEW-02, doc half)

`build_tlas`'s doc at [tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs)`:78-82` states
the invariant "only the `acceleration_structure_reference` field is off-limits" across an UPDATE
— which is exactly the invariant the function's own commit ordering can break. The doc is
correct; the code does not honour it. Fixing the code (CONC-D1-NEW-02) resolves this; no separate
doc edit needed.

### 5.5 — Safety-contract comment that does not cover the case (CONC-D1-H1, doc half)

`recreate_image_available_for_frame`'s doc comment
([sync.rs](crates/renderer/src/vulkan/sync.rs)`:263-269`) claims only that "no command buffer that
waits on this semaphore is currently submitted". That is true but does not address the pending
**signal** from `vkAcquireNextImageKHR`, which is the actual object-lifetime question. Whether or
not CONC-D1-H1 is confirmed, the comment should state which side of the semaphore it is making a
claim about.

---

## 6. Prioritized Fix Order

| # | ID | Sev | Action | Effort | Gate |
|---|---|---|---|---|---|
| **1** | **CONC-D1-NEW-01** | **CRITICAL** | Restructure `ensure_tlas_state` to allocate-then-swap: build the replacement buffers + AS into locals, destroy the old `TlasState` only after every fallible step returns `Ok`. Then make `tlas_written[frame]` two-way — clear it and re-upload `rt_flag = 0.0` via `patch_camera_rt_flag` on any `build_tlas` `Err`. | M | Add the `GpuBuffer::create_*` fault-injection hook (mirroring `BYRO_FSR_FORCE_DISPATCH_FAIL`) and verify under `BYRO_VALIDATION=1` that the `VUID-vkCmdDrawIndexedIndirect-None-08114` family no longer fires. |
| **2** | **CONC-D1-NEW-03** | LOW (doc) | **Edit [SKILL.md](.claude/commands/audit-concurrency/SKILL.md) Dimension 1 per §5.1 before the next `/audit-concurrency` run.** Ranked second despite LOW severity purely because it is a 5-minute edit that prevents a future run from re-opening two correct fixes and shipping a real queue race. | XS | `cargo test -p byroredux-renderer one_time_lock_scope_tests` still green (it already pins the correct behaviour). |
| **3** | CONC-D1-NEW-02 | HIGH | Move the `mem::swap`, `needs_full_rebuild = false` and `last_blas_map_gen = map_gen` commits to immediately after `cmd_build_acceleration_structures`, next to the existing `built_primitive_count` assignment. Mirrors the #917 post-submit history-advance pattern. | S | New source-position test in the `skin_built_this_frame_skip_tests` idiom asserting the three commits follow the build call — **`cargo test`-observable**. |
| **4** | CONC-D3-NEW-01 | MEDIUM | Replace the depth-1 `GRAPH[new_id].contains(held_id)` predicate with a reachability probe (DFS/BFS from `new_id`, panic if `held_id` is reachable) on the novel-edge slow path. Correct the module doc + file-header item 2 regardless. | S | New `lock_tracker` unit test driving `A→B`, `B→C`, `C→A` and asserting the third acquisition panics — **pure-CPU `cargo test`**. Expect the live `Transform`/`GlobalTransform`/`CharacterController` triangle to surface once the probe lands; that surfacing is the point, and the fix for it is a lock-order convention, not suppressing the probe. |
| **5** | CONC-D3-NEW-02 | MEDIUM | Add `.reads_resource::<PlayerMode>()` to `camera_follow_system`'s `Access` in [boot.rs](byroredux/src/boot.rs). **Fix in the same pass as #2389** so all four `Stage::Late` parallel declarations are complete together. | XS | `sys.accesses` / `scheduler_access_tests.rs` — confirm `known_conflict_count()` stays 0. |
| **6** | CONC-D3-NEW-03 | LOW | Comment block at the `known_conflict_count` assert naming the property as the primary deadlock guard, plus a cross-reference from `lock_tracker`'s module doc. Consider promoting `debug_assert_eq!` → `assert!` (runs once at construction). | XS | Review only. |
| **7** | CONC-D1-H1 | HYPOTHESIS | **Do not patch.** Run the confirming experiment first: `BYRO_VALIDATION=1` release build, fault-inject `begin_command_buffer`, look for **`VUID-vkDestroySemaphore-semaphore-05149`**. Promote to a finding only if it fires. Fix §5.5's misleading safety comment either way. | — | Validation layer only; invisible to both `cargo test` and RenderDoc. |

**Batching note.** Items 1 and 3 are both in
[tlas.rs](crates/renderer/src/vulkan/acceleration/tlas.rs) and share the "commit before the
operation succeeds" root cause — do them in one pass, with one shared `GpuBuffer::create_*` /
`write_mapped` fault-injection hook serving both verifications. Items 5 and 6 are both in
[boot.rs](byroredux/src/boot.rs) and land together with #2389.

---

## 7. Coverage & Pre-Existing Issues

### Coverage

**Dim 1**: `context/draw.rs` (`draw_frame` fence/acquire/submit/present window, deferred-destroy
tick, TLAS build site), `vulkan/sync.rs` (`FrameSync` + all three recreate paths),
`vulkan/texture.rs` (`with_one_time_commands_inner`), `context/resize.rs`
(`recreate_swapchain_core` / `recreate_swapchain` / `set_upscaler_mode`),
`acceleration/{mod,types,constants,predicates,blas_static,blas_skinned,tlas,memory}.rs`,
`context/{mod,resources,skinned_blas_refit}.rs`, plus the `build_blas_batched` call sites in
`byroredux/src/{scene/nif_loader,cell_loader/{spawn,exterior},cornell}.rs`.

**Dim 2**: `skin_compute.rs` chain via `context/skinned_blas_refit.rs`,
`scene_buffer/{upload,descriptors,buffers}.rs`, `svgf.rs`, `taa.rs`, `caustic.rs`,
`water_caustic.rs`, `volumetrics.rs`, `bloom.rs`, `material.rs`, `restir.rs`,
`context/{post_passes,geometry_pass,helpers}.rs`, and a stage-usage sweep of all 21 GLSL sources
for `rayQueryEXT` / `accelerationStructureEXT` consumers.

**Dim 3**: every system in `byroredux/src/systems/` (`animation`, `audio`, `billboard`, `bounds`,
`camera`, `character`, `cinematic`, `debug`, `escort`, `follow`, `guard`, `light_anim`,
`locomotion`, `metrics`, `particle`, `patrol`, `sandbox`, `travel`, `wander`, `water`,
`weather`), `byroredux/src/render/{mod,camera,lights,skinned,static_meshes,particles,sky,water}.rs`,
`byroredux/src/boot.rs` (full schedule enumeration, all 5 stages),
`crates/core/src/ecs/{world,query,resource,lock_tracker,scheduler,access,systems}.rs` +
`world_tests.rs`, `crates/scripting/src/timer.rs`, `crates/physics/src/sync.rs` (edge cross-check
only), `byroredux/src/cell_loader/references/mod.rs` (`prune_seat_reservations` only),
`.github/workflows/ci.yml`, `docs/contributing.md`.

**NOT covered** — flagged for honesty, in addition to the un-run Dimensions 4/5/6/7 named in the
scope line: `frame_upscaler.rs` / `presentation.rs` / `fsr3-sys` FFI internals, `egui_pass.rs`,
`screenshot.rs`, worker-thread streaming beyond the Dim 3 edge cross-check, the console-command
bodies under `byroredux/src/commands/`, the debug-server `evaluator.rs` (both #2388's subject),
the scripting-crate systems beyond `timer.rs`, and `crates/save/`.

### Pre-existing issues re-confirmed, deliberately not re-reported

**Dim 1/2 adjacent**: #2465 (swapchain UNDEFINED→COLOR_ATTACHMENT_OPTIMAL transition not provably
ordered after the acquire semaphore), #2403 (CHAIN2-D2-01: skinned-vertex fragment read rides the
cluster-cull pass's trailing barrier), #2402 (`skinnedVertexAddress` from a stale `SkinSlot` when
a mesh becomes non-RT-capable), #2401 (caustic parked-camera EMA counts global frames while each
FIF slot accumulates every other frame), #2484 / #2485 (`copy_depth_to_history` source access
scope; `record_upscale_pass` depth sharing under `MAX_FRAMES_IN_FLIGHT == 2`).

**Dim 3**: #2389 (Late-parallel telemetry systems' undeclared resource reads — verified still
present verbatim), #2393, #2391, #2388, #2387, #2386, #2385, #2384, #2547, #2404, #2400, #2399,
#2398, #2270, #2660.

**Confirmed FIXED, not carried forward**: CONC-D4-NEW-01 (2026-07-25 CI `BYRO_LOCK_ORDER_CHECK`
gap) — closed by **#2137**.
