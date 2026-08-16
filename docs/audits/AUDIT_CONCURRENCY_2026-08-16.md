# Concurrency Audit — 2026-08-16

**Command**: `/audit-concurrency` (all 7 dimensions, `--depth deep`), run as part
of the `comprehensive` audit-suite sweep.

**Repo state**: HEAD `85b77371`, branch `main`. Dedup baseline: 269 OPEN issues
cached at `/tmp/audit/issues.json`, plus every prior `docs/audits/AUDIT_CONCURRENCY_*.md`
and the ECS reports of 2026-08-07 / 2026-08-10 / 2026-08-16.

## Scope

All seven dimensions were run. Beyond the standard entry points, this sweep
carried two explicit extra-scope mandates:

1. **`crates/debug-server` + `crates/debug-protocol`** — an **un-owned
   subsystem** whose only coverage anywhere in the audit suite is this
   dimension's "worker threads" bullet. Given a real pass this time: the TCP
   listener, the per-client thread lifecycle, the command queue, the
   `DebugDrainSystem` ↔ main-loop interaction, the type-erased component
   registry, and the whole Papyrus-AST evaluator surface (`evaluator.rs`, 1060
   LOC) were read end-to-end.
2. **The P2 gameplay slice** (`byroredux/src/combat.rs`, `inventory.rs`, the
   action half of `interaction.rs`) — ~2.6k LOC landed 2026-08-15/16, also
   un-owned, and never before checked for guard-lifetime or lock-order
   discipline. Traced site by site under Dimension 3.

**Coverage gaps declared**: `crates/facegen`, `crates/mod-runtime`,
`crates/hkx`, and `crates/fsr3-sys` were touched only incidentally (they carry
no threads and no ECS locks — see the thread-inventory below). The FSR3 FFI
crossing is `/audit-safety` Dim 1 / `/audit-renderer` Dim 23 territory and was
not re-audited here.

## ⚠ Verification status

**No Vulkan device, no captured validation run, and no RenderDoc capture backed
any Dimension 1 or 2 verdict.** Those are source-read only. Per the skill's
speculative-fix guardrail, no barrier / stage-mask / layout change is proposed
anywhere in this report.

Dimension 4 is the exception and the one dimension with **executed** evidence:

```
cargo test -p byroredux --bin byroredux scheduler_access
→ 10 passed; 0 failed
```

## Executive Summary

**0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW.**

| Dimension | Area | Result |
|---|---|---|
| 1 | Vulkan Queue & AS Sync | CLEAN — 12 guards intact, both 2026-08-14 findings verified FIXED |
| 2 | Compute → AS → Fragment Chains | CLEAN — 5 guards intact |
| 3 | ECS Lock Ordering & Deadlock | CLEAN for the new gameplay slice |
| 4 | Scheduler Access Declarations | CLEAN — regression guard green **by execution** |
| 5 | RwLock: Resource↔Storage & Physics | CLEAN apart from already-OPEN #2404 |
| 6 | Resource Lifecycle (GPU teardown) | CLEAN — full `Option<…>`-field ↔ Drop cross-check |
| 7 | Worker Threads (Streaming, Debug Server) | **1 MEDIUM, 2 LOW** |

This is an honest thin result for a dimension set that has now been audited
thirty-plus times, and the negative results below are the substance of the
report. Two things are worth reading even if you act on nothing:

- **The 2026-08-14 `rt-deep` findings are genuinely fixed, not just marked
  CLOSED.** CON-D1-01 (`shrink_tlas_to_fit` dangling descriptor) → `c25f61e6`
  routes the shrink through allocate-then-swap. CON-D2-01 (`build_tlas` failure
  arm skipping the frame's only AS_WRITE→AS_READ barrier) → the barrier at
  `crates/renderer/src/vulkan/context/draw.rs:2452-2460` now sits **outside**
  the `tlas_build_failed` branch, with a comment explaining that it also
  publishes `record_skinned_blas_refit`. CON-D1-02's hypothesis (static BLAS
  paths not self-emitting the leading scratch-serialize barrier) resolved by
  deletion — `999478ef` removed the dead single-shot path, and the surviving
  batched path calls `record_scratch_serialize_barrier` at
  `crates/renderer/src/vulkan/acceleration/blas_static.rs:550`.
- **The gameplay slice is clean, and that is not the null result it looks
  like.** Every `world.get::<T>()` in the engine returns a `ComponentRef` that
  *holds the read lock* for its lifetime, and the workspace is on edition 2021,
  where `if let` scrutinee temporaries live to the end of the block. The three
  new systems dodge that trap at every site — not by luck, but by consistently
  consuming the guard through a by-value combinator (`Option::map` /
  `and_then`) so it drops inside the closure. `combat.rs:113-115` and
  `interaction.rs:745-746` both carry explicit comments about resolving Rapier
  ownership *before* acquiring `PhysicsWorld`.

### The one MEDIUM in one line

`CONC-2026-08-16-01` — the M40 streaming pre-parse worker exists to keep cell
parsing off the frame's critical path, but its parallel phase runs on rayon's
**global** pool, which is the same pool `Scheduler::run` injects the parallel
ECS batch into. There is no dedicated `rayon::ThreadPool` anywhere in the
workspace.

### Deduplication — one finding withdrawn

The sharpest candidate this sweep produced was `eval_walk_entity`
(`crates/debug-server/src/evaluator.rs:331-338`) acquiring **seven** storage
read guards plus a `StringPool` resource guard in source order, with no TypeId
sort, safe only because `DebugDrainSystem` is registered `add_exclusive` in a
*different crate* (`crates/debug-server/src/lib.rs:33`). It was written up,
then withdrawn: **#2388 (ECS-D1-06, OPEN)** already names `eval_walk_entity`
and `eval_inspect_skinned_mesh` by line number as part of the same six-pair
family. Skipped per the dedup rule. It is called out here only because it would
otherwise look like a coverage hole in the extra-scope pass — it is not; it is
already filed.

---

## Findings

### CONC-2026-08-16-01: The streaming pre-parse worker's parallel phase and the parallel ECS scheduler contend for rayon's single global thread pool

- **Severity**: MEDIUM
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `byroredux/src/streaming.rs:1233-1239` (worker side);
  `crates/core/src/ecs/scheduler.rs:499-504` (scheduler side)
- **Status**: NEW
- **Trigger Conditions**: A cell whose fresh-parse count reaches
  `PRE_PARSE_RAYON_MIN = 8` — i.e. session start, first entry into a new
  worldspace region, or any door transition into un-cached content. Steady-state
  streaming (0-6 fresh NIFs per cell, per the in-code Riverwood measurement)
  takes the serial fast path and does not trigger this.
- **Description**: `cell_pre_parse_worker` runs on its own dedicated
  `std::thread` (`streaming.rs:738`), which is the whole point of the M40
  design — cell parsing should not sit on the frame's critical path. But its
  CPU-bound Phase 2 fans out with `extracted.into_par_iter().map(parse_one_nif)
  .collect()`, and that goes to rayon's **global** pool. `Scheduler::run`
  dispatches each stage's parallel batch with
  `data.parallel.par_iter_mut().for_each(…)` into the *same* global pool.
  `grep -rn "ThreadPoolBuilder\|num_threads"` over the workspace returns zero
  hits, so neither side has a private pool and neither side bounds its share.

  The mechanism that makes this more than ordinary CPU contention is rayon's
  non-worker calling-thread path. Both the streaming worker and the main thread
  are plain OS threads, not pool members. When such a thread calls `par_iter`,
  rayon injects the root job and then **blocks the caller on a latch** — the
  calling thread does not execute the work itself. So on a stage whose parallel
  batch holds a single system (`Stage::Update`, `Stage::PostUpdate`, and
  `Stage::Physics` each have exactly one — `boot.rs:913`, `:993`, `:1189`, the
  last being `physics_sync_system`), the main thread hands off one system and
  then sleeps until a rayon worker becomes free. If every worker is mid-`parse_one_nif`,
  that wait is a full NIF-parse quantum, and it is paid once per stage that has
  a parallel batch — five of them (`Early`, `Update`, `PostUpdate`, `Physics`,
  `Late`).
- **Evidence**:
  ```rust
  // byroredux/src/streaming.rs:1234-1239 — on the dedicated worker thread
  const PRE_PARSE_RAYON_MIN: usize = 8;
  let results: Vec<(String, Option<PartialNifImport>)> = if extracted.len() < PRE_PARSE_RAYON_MIN {
      extracted.into_iter().map(parse_one_nif).collect()
  } else {
      extracted.into_par_iter().map(parse_one_nif).collect()   // ← global pool
  };
  ```
  ```rust
  // crates/core/src/ecs/scheduler.rs:501-504 — on the main thread, same pool
  data.parallel
      .par_iter_mut()
      .for_each(|entry| entry.run_tracked(world, dt, timings.as_ref()));
  ```
  The 2026-05-24 performance audit already observed the interaction in passing
  ("a contended pool (other rayon workers running renderer-side jobs)") while
  arguing for the `PRE_PARSE_RAYON_MIN` threshold, but it was never separated
  out as a finding of its own.
- **Impact**: Frame-pacing, not correctness — no deadlock, no data race, no
  starvation (rayon's injected-job handling is not FIFO-starving: each side
  injects one root job that then splits into a worker's local deque, so both
  workloads make progress). The cost is added scheduling latency on exactly the
  frames the streaming worker was built to protect: cell-crossing and
  region-entry bursts, which is the same window as the user-visible streaming
  hitch. Order of magnitude is one NIF-parse quantum per parallel stage, so
  low single-digit milliseconds per frame for the duration of the burst — but
  **this is a reasoned bound, not a measurement**, and it should be sized before
  anyone spends effort on it. Relevant context: this project treats a CPU
  bottleneck on a 7950X as a bug.
- **Related**: #877 / NIF-PERF-13 (the two-phase split that created the
  parallel phase), #1262 / NIF-D5-NEW-02 (`PRE_PARSE_RAYON_MIN`), #1167
  (worker Drop ordering, verified intact). Not a duplicate of any of them —
  all three are about the worker's internal shape, none about pool ownership.
- **Suggested Fix**: Size it first — run a fresh-region entry under
  `BYRO_PROFILE=1` and compare `SchedulerSystemTimings` for the five
  parallel-batch stages against a warm-cache run of the same route. If the
  delta is real, give the streaming worker a private
  `rayon::ThreadPoolBuilder::new().num_threads(n).build()` (sized well below
  the core count) and `install()` the Phase-2 fan-out into it, so the frame's
  parallel batch always has workers available.

---

### CONC-2026-08-16-02: A cancelled screenshot makes `DebugDrainSystem` skip that frame's entire command drain

- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug)
- **Location**: `crates/debug-server/src/system.rs:72-78`
- **Status**: NEW
- **Trigger Conditions**: A `byro-dbg` screenshot request whose client-side 5 s
  `recv_timeout` fires before the engine's 10-frame ceiling — i.e. a paused or
  GPU-stalled engine, which is precisely the state #1007 was written for — with
  at least one other command already queued behind it.
- **Description**: The `#1007` abandonment handler cancels the in-flight GPU
  capture, clears `pending_screenshot`, and then `return`s from
  `System::run`. That `return` exits the whole system, not just the
  screenshot block, so the command drain at `system.rs:136-142` never runs on
  that frame. Every other pending command is deferred a frame. The two sibling
  arms in the same block (`:110`, `:124`, `:131`) all set
  `self.pending_screenshot = None` and fall through to the drain — this is the
  only one that does not.
- **Evidence**:
  ```rust
  // crates/debug-server/src/system.rs:72-78
  if pending.cancel.load(Ordering::Acquire) {
      if let Some(bridge) = world.try_resource::<ScreenshotBridge>() {
          bridge.cancel();
      }
      self.pending_screenshot = None;
      return;                      // ← skips the drain at :136
  }
  ```
- **Impact**: One frame of extra latency on unrelated queued commands, in a
  situation where the engine is already stalled. No leak (the queue is bounded
  at `MAX_QUEUED_COMMANDS = 64` and the commands are still there next frame),
  no lost response, no state corruption. Reported because the asymmetry against
  the three sibling arms reads as unintentional rather than designed, and the
  early `return` is the kind of line a later refactor inherits without
  re-deriving why it is there.
- **Related**: #1007 (the abandonment signal this arm implements), #1011,
  #1010 (the queue cap that bounds the consequence).
- **Suggested Fix**: Replace the `return` with the same fall-through the other
  three arms use — the screenshot bookkeeping is already fully cleared by that
  point, so there is nothing left for the rest of `run` to trip over.

---

### CONC-2026-08-16-03: `pre_parse_cell`'s doc comment was cut in half by the #1262 extraction and now documents the wrong function

- **Severity**: LOW
- **Dimension**: Worker Threads (Streaming, Debug) — documentation of the
  worker/main-thread contract
- **Location**: `byroredux/src/streaming.rs:1040-1060` and `:1103-1105`
- **Status**: NEW
- **Trigger Conditions**: None — static.
- **Description**: When `parse_one_nif` was extracted out of `pre_parse_cell`
  (#1262 / NIF-D5-NEW-02), it was inserted *between* `pre_parse_cell`'s doc
  comment and `pre_parse_cell` itself. The result is that the block at
  `:1040-1050` — which documents the `cached_keys` main-thread-snapshot
  contract, the one piece of worker↔main-thread protocol a reader most needs —
  is now attached to `parse_one_nif`, and it ends mid-sentence: "Returns a
  populated [`LoadCellPayload`] (which may have an empty". The completing
  clause sits orphaned 50 lines later at `:1103-1105`, immediately above
  `#[tracing::instrument]` and `fn pre_parse_cell`, where rustdoc attaches it to
  the right function but with no beginning.
- **Evidence**:
  ```
  1040  /// Per-cell pre-parse: walk references, resolve unique model paths, …
  1044  /// `cached_keys` is the main-thread snapshot of
  1045  /// [`crate::cell_loader::NifImportRegistry`] at request-build time; …
  1050  /// Returns a populated [`LoadCellPayload`] (which may have an empty
  1051  /// Parse + import a single (path, Option<bytes>) pair. Shared between   ← parse_one_nif's doc starts here
  1061  fn parse_one_nif(…)
  ...
  1103  /// `parsed` map if the cell doesn't exist, has no references, or          ← pre_parse_cell's doc tail
  1105  /// applies the empty payload so the pending entry is cleared).
  1106  #[tracing::instrument(name = "pre_parse_cell", …)]
  1111  fn pre_parse_cell(
  ```
- **Impact**: Documentation only. It matters more than the average doc-rot
  finding because these two functions are the worker thread's entire body, and
  the sentence that got orphaned is the one stating that `cached_keys` is a
  read-only snapshot rather than shared mutable state — the invariant a future
  reader would need before touching the worker.
- **Related**: #1262 / NIF-D5-NEW-02 (the extraction), #862 (the cache-skip
  contract the split sentence describes).
- **Suggested Fix**: Move the four orphaned lines at `:1103-1105` back up to
  join `:1050`, and give `parse_one_nif` a doc comment that starts at its own
  first line.

---

## Guards verified intact

Reported as guards rather than findings — each was actively checked at this
HEAD, not assumed from a prior report.

### Dimension 1 — Vulkan queue & acceleration-structure sync (12)

1. **Queue-Mutex discipline (#284 / CONC-D2-NEW-01).** `draw.rs:3516-3545`
   binds the `MutexGuard` and derefs inside `queue_submit` so the guard spans
   the call (VUID-vkQueueSubmit-queue-00893); same shape at the present site
   `:3620-3633`. The graphics guard is dropped at `:3545` before the present
   guard is taken at `:3620`, so the aliased-`Arc` case (`present_queue` is an
   `Arc::clone` of `graphics_queue` when the families match) cannot
   self-deadlock.
2. **One-time command helper (#1713).** `vulkan/texture.rs:814-838` scopes the
   queue guard to the submit only and releases before `wait_for_fences`; pinned
   by the source test `queue_guard_released_before_one_time_fence_wait`.
3. **Command-buffer error paths (#1861 / #2157).** All eight
   `free_command_buffers(pool, &[cmd])` sites present, pinned by a counting
   test.
4. **Frame-in-flight discipline.** `draw.rs:1477-1491` waits **both**
   `in_flight` slots. `prev = (frame + 1) % MAX_FRAMES_IN_FLIGHT` is only "the
   previous slot" at MAX == 2, and `vulkan/sync.rs:46` carries the guard
   pinning `MAX_FRAMES_IN_FLIGHT == 2` with the #870 safety contract.
5. **`reset_fences` immediately before `queue_submit` (#952).** Both failure
   arms recover `image_available[frame]`, and the submit arm additionally
   recreates `in_flight[frame]` as SIGNALED.
6. **Per-image `render_finished[img]`** signal semaphore, with the
   VUID-vkQueueSubmit-pSignalSemaphores-00067 rationale at `draw.rs:3461-3473`.
7. **AS build INPUT barrier access flag (#1436 / #507945d8).**
   `acceleration/tlas.rs:241-255` uses `TRANSFER_WRITE → SHADER_READ` at
   `ACCELERATION_STRUCTURE_BUILD_KHR`, not `ACCELERATION_STRUCTURE_READ_KHR`.
   The `copy_size > 0` guard (#317) still elides the zero-size copy + barriers.
8. **`record_scratch_serialize_barrier` dst mask (#1790).**
   `acceleration/blas_skinned.rs:684-703` still `AS_WRITE → AS_WRITE | AS_READ`.
9. **AS build → ray-query barrier on both `build_tlas` arms (#2931).**
   `draw.rs:2452-2460`.
10. **#2915 defused panic.** `tlas.rs:287-292` — the scratch lookup is
    `ok_or_else(…)?`, not `.unwrap()`, so a missing scratch degrades through
    the `tlas_build_failed` arm instead of aborting inside an open recording.
11. **Deferred destruction.** `pending_destroy_blas` + `pending_destroy_scratch`
    are both `DeferredDestroyQueue` (`acceleration/mod.rs:204,221`), drained in
    `destroy()` before the `blas_entries` / `skinned_blas` drains (#639 /
    #1138); the deferred tick runs **after** the fence wait
    (`draw.rs:1587-1598`, #418).
12. **Swapchain recreate.** `context/resize.rs:32-37` — `device_wait_idle` is
    the first statement of `recreate_swapchain_core`; `set_upscaler_mode`
    (`:1214-1227`) pays its own.

### Dimension 2 — compute → AS → fragment chains (5)

1. **Master frame ordering** in `draw.rs`: skin-palette dispatch + barrier
   (`:2301-2363`) → `record_skinned_blas_refit` (`:2365`) → `build_tlas`
   (`:2379`) → AS barrier (`:2452`) → `record_geometry_pass` (`:3343`) →
   `record_post_passes` (`:3375`) → `screenshot_record_copy` (`:3437`) →
   submit (`:3520`). The volumetrics COMPUTE ray query is inside
   `record_post_passes`, i.e. after the barrier, on both build arms.
2. **Skin-palette publish** `draw.rs:2343-2360`: `SHADER_WRITE → SHADER_READ`,
   dst stage `COMPUTE_SHADER | VERTEX_SHADER`, covering `skin_vertices.comp`
   and `triangle.vert`'s inline skinning.
3. **Volumetrics latch symmetry (#1105).** `volumetrics.rs:1485-1499` —
   `dispatch` asserts and resets both `tlas_written[frame]` and
   `lights_written[frame]`; `write_tlas` (`:1773-1789`) sets the former.
4. **Bloom within-frame RAW chain (#931).** `bloom.rs:563-644` — one per-mip
   post-barrier after each down and up dispatch, with `dst_stage` widened to
   `FRAGMENT_SHADER` on the final up-mip. The upsample seed read
   (`down_mips[N-1]`) is covered by the down chain's last post-barrier; per-frame
   mip ownership removes the cross-frame WAR. No missing publish.
5. **Host-readback discipline (#2740).** `vulkan/compute.rs:381-406`
   (`collect_telemetry`, extended by `9c805cd7`) calls
   `buffer.invalidate_if_needed` before `mapped_slice_mut`, and is called from
   `draw.rs:1504` after the both-slot fence wait — same shape as
   `collect_image_health` (`:1502`) and `screenshot_finish_readback` (`:1520`).

### Dimension 3 — ECS lock ordering (5)

1. **TypeId-sorted acquisition** — `world.rs:501-600` (`query_2_mut`,
   `query_2_mut_mut`), `:738`, `:862`; each still opens with the same-type
   `assert_ne!` and branches on `if id_a < id_b`.
2. **`lock_tracker` gating unchanged** — same-thread reentrancy panic is
   ungated (release included); only the `global_order` cross-thread graph is
   `cfg(debug_assertions)` + `BYRO_LOCK_ORDER_CHECK`
   (`lock_tracker.rs:213,239`). The CI guard
   `vulkan_validation_job_enables_the_lock_order_detector` passes.
3. **Per-resource `RwLock`** — `world.rs:61` is `HashMap<TypeId, RwLock<…>>`,
   not one lock over the whole map. Checked explicitly, because the
   `CommandRegistry`-held-across-`execute` pattern at
   `crates/debug-server/src/evaluator.rs:415-420` and `byroredux/src/main.rs:793`
   would otherwise be a live self-deadlock rather than the documented,
   contract-bounded pattern it is.
4. **`ConsoleCommand::execute` lock contract** (`crates/core/src/console.rs:43-55`)
   honoured: `resource_mut::<CommandRegistry>()` appears only in console.rs's own
   panic test.
5. **Gameplay slice guard lifetimes** — `combat.rs:116-129`, `:151-171`,
   `:196-254`; `interaction.rs:741-784`, `:803-815`, `:872-893`;
   `inventory.rs:262-266`, `:331-355`; `ragdoll.rs:281-351`. Every one either
   scopes the guard to a block or moves it into a by-value combinator. No
   `query_mut` guard is held across a re-entry into the same storage, and no
   `&mut World` structural mutation happens inside a system body.

### Dimension 5 — physics / Resource↔Storage (4)

1. `physics_sync_system` still the 4-phase shape; `collect_newcomers`
   (`crates/physics/src/sync.rs:686-757`) returns a `Vec` from read guards, and
   `register_newcomers` `drop(pw)`s at `:883` before
   `query_mut::<RapierHandles>()` at `:899`.
2. `ContactConfig` snapshotted once per batch, not re-locked per newcomer.
3. `release_victim_rapier_bodies` (#1520) unchanged.
4. **New cross-check**: `combat_damage_system` introduces a second
   `PhysicsWorld` write site (via `activate_ragdoll`) in `Stage::Update`.
   `activate_ragdoll` drops every storage read guard before
   `resource_mut::<PhysicsWorld>()`, and stages run sequentially
   (`scheduler.rs:497`), so it cannot overlap the `Stage::Physics` batch.

### Dimension 6 — GPU teardown (23 subsystem fields cross-checked)

Every `Option<…>` subsystem field on `VulkanContext` was enumerated and matched
to a destroy call in the Drop chain: `egui_pass` (`context/mod.rs:3822`),
`presentation` (`:3825`), `gpu_timers` (`:3849`), `skin_palette` (`:3860`),
`water` (`:3866`), `frame_upscaler` (`:3879` + `:3767`), `texture_registry` /
`scene_buffers` (`:3673-3674`), `skin_compute` + `skin_slots` (`:3691-3694`,
`:3724` — slots before the pipeline), `accel_manager` (`:3719`), `cluster_cull`
(`:3721`), `ssao` (`:3734`), `placeholder_ao` / `placeholder_caustic_sink`
(`:3747`, `:3750`), `exposure` (`:3760`), `composite` (`:3770`), `caustic`
(`:3773`), `volumetrics` (`:3776`), `bloom` (`:3779`), `water_caustic_accum`
(`:3792`), `svgf` (`:3794`), `reservoir_buffers` (`:3799`), `taa` (`:3801`),
`gbuffer` (`:3804`), then depth resources (`:3914-3932`), pipelines / layout /
cache / render pass / swapchain (`:3948-3979`), and the allocator last
(`:3985`, `Arc::try_unwrap` + the #665 leak guard). Two fields carry no GPU
handles and correctly need none: `fsr_temporal: Option<FsrTemporalState>`
(`vulkan/upscaling.rs:236-240` — a `Vec<FsrJitterSample>` and two scalars) and
`dalc_cube: Option<SkyDalcCube>` (`vulkan/context/mod.rs:659-673` — seven
`[f32; 3]` and one `f32`). The #1483 hoist still holds: allocator-independent
destroys run outside the `Some(allocator)` scope and nothing that needs the
allocator was moved after it. `buffer.rs:1217-1226` releases the allocator
Mutex end-of-statement, well before the one-time submit at `:1243` — no holder
keeps it locked across a queue submit.

### Dimension 7 — debug server & streaming (12)

**Streaming worker**: #1167 Drop ordering (`streaming.rs:870-894` takes `worker`
then `request_tx`, then joins; `Drop` delegates); poll-based
`join_with_timeout` with no watcher thread (#1169); the compile-time
`assert_send` at `:571`; `cached_keys` as an `Arc<HashSet<String>>` main-thread
snapshot (#862) with no write-back; `TextureProvider` confirmed stateless
(`asset_provider/texture.rs:7-10`) over `Mutex<File>`-guarded archives
(`crates/bsa/src/archive/mod.rs:49`, `crates/bsa/src/ba2.rs:115`); per-NIF and
per-cell `catch_unwind` (#854). **`merge_external_material` confirmed
unreachable from the worker** — the whole transitive set out of `parse_one_nif`
is `parse_nif` / `extract_bsx_flags` / `extract_root_flags` /
`import_nif_lights` / `import_nif_particle_emitters` /
`import_embedded_animations`; `MaterialProvider` and its four caches are never
touched.

**Debug server**: bounded queue (`MAX_QUEUED_COMMANDS = 64`,
`listener.rs:44`) with an atomic check-and-push (#1010) and three unit tests;
per-client threads never touch the World (they decode, enqueue, and block on
`recv_timeout`); the queue Mutex is held only across a `std::mem::take`
(`system.rs:136-142`), never across evaluation; shutdown shuts down every live
socket before joining the listener (#1009) with the post-accept check folded
into the registry critical section (#1172) and a `Weak` registry pruned on each
accept; wire framing bounded at 16 MB with the length checked before allocation
(`debug-protocol/src/wire.rs:9,28-38`) on a loopback-only bind (#857); the
screenshot claim/cancel/generation protocol (#1006 / #1007 / #1011 / #1603) is
entirely main-thread — `DebugDrainSystem` and `draw_frame` both run there, so
the atomics are for cross-*object* sharing, not cross-thread, and there is no
race with present.

**Thread inventory (whole workspace).** `grep` for `thread::spawn` /
`thread::Builder` across `byroredux/src`, `crates/renderer`, `crates/ui`,
`crates/save`, `crates/audio` returns exactly **two** engine-owned spawn sites:
`streaming.rs:738` (the cell worker) and `listener.rs:157` + `:232` (the debug
listener and its per-client threads). Plus rayon's global pool and kira's
internal audio thread. This is the concrete confirmation of the skill's
"Ruffle/wgpu device is `Send` but not `Sync` — confirm it stays on one thread"
item: `crates/ui` spawns nothing.

---

## Candidates considered and NOT reported

Recorded so a later sweep does not re-derive them.

1. **`eval_walk_entity` / `eval_inspect_skinned_mesh` unsorted multi-lock
   acquisition** — real, but **Existing: #2388** (ECS-D1-06, OPEN), which names
   both functions by line. Withdrawn.
2. **`push_kinematic` / `pull_dynamic` hold storage reads across a
   `PhysicsWorld` guard** — verified still present at this HEAD, but
   **Existing: #2404** (OPEN). Skipped.
3. **`with_one_time_commands` takes a bare `vk::CommandPool` with no Mutex**,
   while `vkAllocateCommandBuffers` / `vkFreeCommandBuffers` require external
   synchronisation of the pool. Disproved as a live bug: every caller is on the
   main thread today, and the in-code comment at `texture.rs:806-807` shows the
   author was reasoning about a hypothetical "second graphics-queue thread"
   rather than an existing one. Worth remembering the day a second thread is
   introduced.
4. **`build_debug_ui_snapshot` (`byroredux/src/main.rs:643-654`) holds a `Name`
   storage read and a `StringPool` resource read simultaneously** — an
   unordered Storage↔Resource pair. Disproved: both are reads, it runs on the
   main thread outside the scheduler, and `StringPool` has no runtime writer at
   that point.
5. **`rebuild_geometry_ssbo` calls `device_wait_idle` from inside
   `render_one_frame`** (`crates/renderer/src/mesh.rs:929`, reached from
   `app_frame.rs:172-182`). Disproved as a finding: it is gated behind
   `geometry_rebuild_needs_idle`, logged as a warning, documented as the
   deliberate low-headroom fallback (#2374), and the surrounding comment at
   `:905-918` correctly explains why re-pointing RT bindings 8/9 here would be
   the *unsafe* alternative.
6. **`wire::decode` allocates `vec![0u8; len]` up to 16 MB per client, × 64
   in-flight commands.** Not reported: loopback-only, operator-controlled,
   already bounded twice over.
7. **`try_enqueue_command`'s `queue.lock().unwrap()` panics on poison.**
   Disproved: the only other holder is `DebugDrainSystem::run`, which holds the
   lock across nothing more than `is_empty()` and `mem::take` — neither can
   panic. The evaluator runs entirely outside the lock.
8. **Rayon workers interleaving ECS-system jobs and NIF-parse jobs could
   contaminate `lock_tracker`'s thread-local state.** Disproved: NIF parsing
   never touches the World, and the only contamination path (a system panicking
   mid-lock) is already fail-fast by design (#1412) and already filed as
   CONC-D3-2026-08-07-03.
9. **`DebugDrainSystem` is appended to `Stage::Late`'s exclusive list *after*
   `event_cleanup_system`, so debug queries always see transient markers
   already reaped.** Real, but a UX/observability property of the debug
   surface, not a concurrency defect — out of dimension.

## Skill-text drift (not code defects, not re-filed)

The `/audit-concurrency` SKILL.md Dimension-1 bullet still instructs the reader
to "confirm the guard is **not** held across `queue_submit`". The code
deliberately does the opposite, and correctly so —
VUID-vkQueueSubmit-queue-00893 requires the queue to be externally synchronised
*for the submit call*, which is exactly what holding the guard achieves. This
was already filed as skill-text drift by the 2026-08-12 report (§5.1) and is
noted here only so the next reader of that bullet does not "fix" working code.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-08-16.md
```
