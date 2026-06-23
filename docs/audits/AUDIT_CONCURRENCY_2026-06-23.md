# Concurrency & Synchronization Audit — 2026-06-23

**Scope**: Vulkan queue/AS sync, compute→AS→fragment chains, ECS lock ordering,
scheduler access declarations, physics RwLock patterns, GPU resource lifecycle,
worker threads. All 7 dimensions run inline at `--depth deep`.

**Verdict**: The synchronization machinery is exceptionally robust. Every
checklist item across all 7 dimensions was verified against current code and
held. The codebase is heavily hardened by prior audits — the dense
issue-numbered comments (#313, #507945d8/#1436, #1167, #1105, #931, #1483,
#665, #1394/#1602, #1520) are live evidence that each historical hazard has a
structural fix plus a regression guard. **No CRITICAL, HIGH, or MEDIUM findings.**
One LOW informational item (a deliberate, documented serialization choice).

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 1 |

Dedup baseline: `/tmp/audit/issues.json` (28 OPEN). No open or closed
concurrency issue overlaps the LOW item below.

---

## Findings

### CONC-D1-01: One-time command helper holds the graphics-queue Mutex across `wait_for_fences(u64::MAX)`
- **Severity**: LOW
- **Dimension**: Vulkan Queue & AS Sync / Worker Threads
- **Location**: `crates/renderer/src/vulkan/texture.rs:659-666` (`with_one_time_commands_inner`); mirrored at `crates/renderer/src/vulkan/context/draw.rs:3487-3491` (egui `dispatch`)
- **Status**: NEW
- **Description**: `with_one_time_commands_inner` locks `graphics_queue`, submits,
  then blocks on `wait_for_fences(&[fence], true, u64::MAX)` **before** dropping the
  guard. The Vulkan spec (VUID-vkQueueSubmit-queue-00893) only requires external
  synchronization of the queue for the *submit call itself*, not for the
  subsequent fence wait. Holding the Mutex across the wait serializes any other
  graphics-queue user for the full GPU execution of the one-time submit. The egui
  overlay `dispatch` path holds the same guard across `set_textures` (an internal
  egui-ash-renderer submit) for the same reason.
- **Evidence**:
  ```rust
  let q = queue.lock().expect("graphics queue lock poisoned");
  device.queue_submit(*q, &[submit_info], fence)...;
  device.wait_for_fences(&[fence], true, u64::MAX)...;   // guard still held
  drop(q);
  ```
  Contrast the main per-frame submit (`draw.rs:3590-3619`), which correctly drops
  the guard immediately after `queue_submit`, with no wait under lock.
- **Impact**: None today. The engine is single-threaded in the draw loop; these
  helpers run only at load/cell-transition frequency (terrain-tile upload, BLAS
  initial build, texture streaming) or in the debug-only egui overlay path. There
  is no concurrent graphics-queue contender, so the held guard never actually
  blocks another thread. This is a latent serialization point that would matter
  only if a future design submits to the graphics queue from a second thread
  (e.g. an async upload path).
- **Related**: The comments at both sites explicitly justify holding the guard
  across the *submit* (CONC-D2-NEW-01, audit 2026-05-16); the over-broad coverage
  of the *wait* is the incremental observation here.
- **Suggested Fix**: Copy the queue handle out (`let q = *queue.lock()...; drop`)
  for the submit, then wait on the fence after the guard is released — the fence
  + dedicated command buffer already provide the necessary completion guarantee
  without serializing the queue for the wait duration. Defer unless/until a
  second graphics-queue thread is introduced; not worth the churn now.

---

## Dimension-by-dimension verification (all PASS)

### Dimension 1 — Vulkan Queue & Acceleration-Structure Sync
- **Queue is single-Mutex, lock→copy→submit→drop.** `graphics_queue` /
  `present_queue` are `Arc<Mutex<vk::Queue>>` with `present_queue` an `Arc::clone`
  of `graphics_queue` (`context/mod.rs:1392-1476`). The main submit
  (`draw.rs:3590-3619`) and present (`3647-3660`) hold the guard *only* across the
  submit/present call and drop it immediately — correct per
  VUID-vkQueueSubmit-queue-00893. (The one-time-cmd + egui sites over-hold across
  the fence wait — see CONC-D1-01, LOW.)
- **Frame-in-flight discipline.** `in_flight[frame]` waited before cmd reuse;
  per-image `render_finished[img]` keyed off the acquire boundary (the #906
  VUID-vkQueueSubmit-pSignalSemaphores-00067 fix is intact, `draw.rs:3535-3547`);
  `reset_fences` lands immediately before submit (#952, `draw.rs:3556-3580`).
- **AS build→read barrier.** TLAS build → `AS_BUILD_WRITE → FRAGMENT|COMPUTE READ`
  (`draw.rs:1762-1770`, #415 COMPUTE widening intact). Static BLAS WRITE→READ at
  `blas_static.rs:776-782`. Skinned refit→TLAS WRITE→READ at `draw.rs:1644-1651`.
- **AS build-INPUT barrier flag (#1436/#507945d8 regression guard).** Compute
  skin-output → BLAS build uses `SHADER_READ` at `AS_BUILD` stage
  (`draw.rs:1494-1500`); instance-buffer copy → TLAS build uses
  `TRANSFER_WRITE → SHADER_READ` at `AS_BUILD` (`tlas.rs:711-732`). Neither uses
  the wrong `ACCELERATION_STRUCTURE_READ_KHR`. **Intact.**
- **Deferred AS destruction (#a476b256/#1449).** `drop_blas` routes through
  `pending_destroy_blas` (`blas_static.rs:51-68`); skinned BLAS via
  `drop_skinned_blas` → same queue (`blas_skinned.rs:637-651`). No immediate
  `destroy_acceleration_structure` at any eviction/unload site. The TLAS
  instance-buffer shrink (`memory.rs:167-180`) destroys immediately but only on
  the freshly-fence-waited `current_frame` slot (`draw.rs:3694-3732`), whose prior
  work is complete by construction — safe. Shutdown drains via
  `destroy()`→`drain_pending_destroys` (#732, `mod.rs:2876-2884`).
- **Swapchain recreate.** `recreate_swapchain` calls `device_wait_idle` first
  (`resize.rs:23`) before any destroy/rebuild.
- **One-time command buffers** run at load/transition frequency, gated by dirty
  flags (terrain tiles: `fill_terrain_tile_scratch_if_dirty`), not per-frame.

### Dimension 2 — Compute → AS → Fragment Chains
- **Skin chain (M29).** palette (`COMPUTE_WRITE→SHADER_READ`, `draw.rs:1126-1153`)
  → skin output → BLAS refit reads it → ray query. Live raster uses inline
  skinning in `triangle.vert`, so no VERTEX_INPUT barrier required (matches the
  checklist note). Chain intact.
- **Cross-frame ping-pong.** SVGF (`svgf.rs:764`) and TAA (`taa.rs:539`) both use
  `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT` — reads the *other* slot. Caustic /
  water-caustic / volumetrics accumulators are per-FIF-indexed. No slot-N reads
  slot-N's in-flight write.
- **Volumetrics gate (#1105).** `write_tlas` sets `tlas_written[frame]=true`
  (`volumetrics.rs:1040`); `dispatch` `debug_assert!`s it then resets to false
  (`volumetrics.rs:864-870`). Set/reset symmetry correct.
- **Bloom RAW chain (#931).** Down-pyramid emits per-mip
  `SHADER_WRITE→SHADER_READ` after every dispatch (`bloom.rs:563-578`); up-pyramid
  likewise, with `up_mips[0]` widened to `FRAGMENT_SHADER` for composite
  (`bloom.rs:611-626`). Final publish present. Bloom is per-FIF (frame-exclusive),
  fully recreated on resize.
- **MaterialBuffer SSBO (R1).** Uploaded before draw recording; covered by the
  frame fence. Not moved into a mid-frame compute path.

### Dimension 3 — ECS Lock Ordering & Deadlock
- **TypeId-sorted acquisition.** `query_2_mut` / `query_2_mut_mut` / `resource_2_mut`
  / `try_resource_2_mut` all branch on `id_a < id_b` and acquire (and set up
  `lock_tracker` scopes) in TypeId-ascending order regardless of generic-param
  order (`world.rs:444-474, 506-537, 652-683`). The #313 ABBA-on-`<B,A>` fix is
  intact. `assert_ne!` on same-type still present.
- **No combined storage+resource accessor exists.** Only two-storage and
  two-resource pairs are offered, both sorted; the unordered Resource↔Storage pair
  is handled by drop-discipline (Dimension 5).
- **Guard lifetime in system bodies.** Spot-checked the heaviest system
  (`animation_system_inner`): every nested re-entry drops the prior guard first
  (e.g. `drop(player_query)` at `animation.rs:404`; `drop(sq)` before
  `ensure_subtree_cache` at `animation.rs:572`, with an explicit comment that the
  SubtreeCache write must precede the AnimationStack read). `NameIndex` /
  `SubtreeCache` are touched only by the animation system (single owner, declared
  reads+writes both), so no cross-thread reverse-order pair exists.
- **Poisoning.** Every non-test ECS lock acquisition resolves `PoisonError`
  through `storage_lock_poisoned::<T>()` / `resource_lock_poisoned::<R>()` /
  `into_inner` (`world.rs:133,256,282,449,...`). No silent `unwrap()` into torn
  state.
- **`lock_tracker` coverage.** Same-thread re-entry detection always-on in debug;
  cross-thread global order graph opt-in via `BYRO_LOCK_ORDER_CHECK`
  (`lock_tracker.rs:217`). Matches the documented contract.

### Dimension 4 — Scheduler Access Declarations (regression guard, M27 closed)
- **Conflict model sound.** `AccessConflict` is `None | Unknown | Conflict`, no
  `Parallel` variant; undeclared ⇒ `Unknown` ⇒ pessimistic serialise.
- **KPIs guarded at startup.** `main.rs:974-994` `debug_assert_eq!`s
  `undeclared_parallel_count()`, `known_conflict_count()`, and `unknown_pair_count()`
  all to 0 (the #1394 + #1602 belt-and-suspenders guard). Any future `add_to()`
  (vs `add_to_with_access`) or a declared parallel conflict trips it before the
  Unknown/Conflict row reaches `sys.accesses`.
- **Exclusive phase.** `audio_system`, `spin_system`, `particle_system`,
  `event_cleanup_system`, footstep, and the papyrus-demo dispatchers remain
  `add_exclusive`. **Note on the checklist text:** `player_controller_system` is
  no longer `add_exclusive` — M27 Phase 3 (commit `05fe2bac`) moved it to
  `add_to_with_access` (`main.rs:644-658`) with a *declared union* access surface
  precisely so its Transform+PhysicsWorld WriteWrite is visible to the analyzer.
  It is the only system writing those in `Stage::Early` (peers: `weather_system`,
  `timer_tick_system`, disjoint surfaces), and `known_conflict_count()==0` is
  asserted, so it is correct-by-construction. The skill's "must stay exclusive"
  wording is stale, not a regression — recommend updating the SKILL.md checklist.

### Dimension 5 — RwLock Patterns (Resource↔Storage, Physics)
- **Resource↔Storage is the only unordered pair; release-before-acquire holds
  everywhere.** `set_linear_velocity` / `set_kinematic_translation` read
  `RapierHandles` via `.copied()` so the read guard drops at the `match`
  expression end before `resource_mut::<PhysicsWorld>()` (`sync.rs:33-41, 65-72`).
- **`physics_sync_system` 4-phase.** `collect_newcomers` returns a `Vec` (all read
  guards drop at fn return, `sync.rs:190-226`) before `register_newcomers` takes
  the writes. `register_newcomers` explicitly `drop(pw)` (`sync.rs:343`) before
  taking the `RapierHandles` write (`sync.rs:352`) — PhysicsWorld and RapierHandles
  writes never overlap. `ContactConfig` snapshotted once via `*r` (`sync.rs:243-246`),
  not re-locked per newcomer.
- **Cell-unload teardown (#1520).** `release_victim_rapier_bodies` collects
  `RapierHandles`/`Ragdoll` into scratch Vecs under read guards in a scoped block
  (`unload.rs:385-400`), drops them, then `try_resource_mut::<PhysicsWorld>()`
  (`unload.rs:404`); runs before the despawn loop (`unload.rs:187 < 191`).
- **Single-threaded placement.** `physics_sync_system` is the sole `Stage::Physics`
  system, fully access-declared (`main.rs:856-877`); a conflicting co-scheduled
  peer would trip the startup `known_conflict_count()==0` assert.

### Dimension 6 — Resource Lifecycle (GPU teardown)
- **Reverse-order Drop + allocator-last.** `device_wait_idle` first
  (`context/mod.rs:2776`), egui_pass taken/dropped, allocator-INDEPENDENT block
  (#1483 hoist of timers/skin_palette/water/fences/command-pools/framebuffers —
  runs on the allocator-`None` path too), then allocator-dependent block, then
  depth/pipelines/render-pass/swapchain, allocator freed last (`mod.rs:3017`),
  device after (`mod.rs:3056`). The #665 dangling-Arc path leaks rather than
  UAF-ing on `vkFreeMemory`.
- **No use-after-destroy on recreate.** `recreate_swapchain` rebuilds gbuffer,
  svgf, taa, caustic, water_caustic, composite, ssao, reservoir buffers, egui
  framebuffers, and fully recreates the resolution-dependent bloom pyramid
  (`resize.rs:562-585`); per-FIF history/accumulator images recreated for every
  slot. Volumetrics is fixed-froxel-size (rebind only).
- **AS cleanup on shutdown.** `accel.destroy()` drains `pending_destroy_blas` +
  `skinned_blas` + TLAS/scratch (#732/#1138). Skin output buffers freed per-slot
  before the skin pipeline (`mod.rs:2856-2890`).

### Dimension 7 — Worker Threads & Thread-Safety Bounds
- **Streaming Drop ordering (#1167).** `shutdown(&mut self)` takes the worker
  handle (short-circuit guard), takes+drops `request_tx` (closes channel) BEFORE
  `join_with_timeout` (`streaming.rs:302-326`). `Drop` delegates to `shutdown`
  with a 1 s timeout (`streaming.rs:339-343`). Ordering does not rely on
  declaration-order field drop. `join_with_timeout` is poll-based, no watcher-
  thread leak (#1169).
- **Worker ↔ main data flow.** `cell_pre_parse_worker(request_rx, payload_tx)`
  takes no `&mut World` and no `MaterialProvider` (`streaming.rs:413`). BGSM
  merge (`merge_bgsm_into_mesh`, `&mut MaterialProvider`) stays on
  `WorldStreamingState` (main thread). Off-thread BSA/BA2 extract goes through
  `Arc<TextureProvider>` whose `BsaArchive`/`Ba2Archive` serialise `File` via
  `Mutex<File>` (`archive/mod.rs:49`, `ba2.rs:113`) — concurrent extracts safe.
- **Debug server.** Per-client threads push to a bounded `Arc<Mutex<Vec<…>>>`
  command queue with a capacity check before push (`listener.rs:62-83`); all World
  mutation routes through `DebugDrainSystem` (Late-stage exclusive). No unbounded
  buffering.
- **Allocator sharing.** `SharedAllocator = Arc<Mutex<Allocator>>` dispatched
  single-threaded inside `draw_frame`; no holder keeps the Mutex across a queue
  submit (the allocator guard is scoped to each allocate/free call,
  e.g. `upload.rs:677-687, 733-737`).

---

## Notes for the next auditor
- The single LOW finding is genuinely latent — do not escalate it absent a
  second graphics-queue thread; the project's "no speculative Vulkan fixes" rule
  applies.
- SKILL.md Dimension 4 checklist text ("`player_controller_system` must stay
  exclusive") is stale post-`05fe2bac`; the system is now declared-parallel and
  guarded by the analyzer. Worth a one-line doc fix in the skill.
- All Vulkan-sync items here were confirmed against current code, not reasoning
  alone. None require a `BYRO_VALIDATION` run to escalate because none are
  hypotheses — they are confirmations that existing barriers/guards are present
  and correct.

Suggested next step:

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-06-23.md
```
