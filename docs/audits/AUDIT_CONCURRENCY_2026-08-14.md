# Concurrency Audit — 2026-08-14

**Scope**: `/audit-concurrency --focus 1,2` — the GPU queue / acceleration-structure
synchronisation slice, run as part of the `rt-deep` audit-suite preset
(`/audit-suite rt-deep`).

| Dimension | Area | Findings |
|---|---|---|
| 1 | Vulkan Queue & Acceleration-Structure Sync | 2 MEDIUM (1 HYPOTHESIS) |
| 2 | Compute → AS → Fragment Chains | 1 HIGH (HYPOTHESIS) |

**Repo state**: HEAD `205744ae`, branch `main`. Dedup baseline: 2813 issues
(251 OPEN) fetched 2026-08-14.

---

## ⚠ Verification status — read this before acting on anything below

**No Vulkan device, no captured validation run, and no RenderDoc capture backed
any verdict in this report.** Every conclusion is source-read only.

Two of the three findings are therefore filed as explicit **HYPOTHESIS rows**
under the skill's speculative-fix guardrail and the project's standing rule
against shipping Vulkan barrier changes whose failure modes are invisible to
`cargo test`. **Do not ship a barrier, stage-mask, or layout change on the
reasoning in this report alone.**

The cheapest way to convert these from hypothesis to fact is a release-build run
with sync-validation enabled:

```
BYRO_VALIDATION=1 cargo run --release -- …      # Khronos layer + Synchronization Validation
BYRO_VALIDATION=gpuav cargo run --release -- …  # adds GPU-Assisted Validation
```

Release, not debug — debug builds are too slow to stream into the dense cells
that fault. Each finding below names the concrete signal that would confirm it.

---

## Executive Summary

**0 CRITICAL · 1 HIGH · 2 MEDIUM · 0 LOW** — with 2 of the 3 unconfirmed by
tooling.

- **CON-D2-01 (HIGH, HYPOTHESIS)** — `build_tlas`'s failure arm skips the frame's
  only `AS_BUILD → ray-query` barrier, leaving that frame's skinned-BLAS refits
  unpublished to the volumetrics compute ray query. This is the highest-severity
  finding of the entire `rt-deep` run, and it is also the one most in need of a
  validation-layer confirmation before anyone touches it.
- **CON-D1-01 (MEDIUM)** — `shrink_tlas_to_fit` destroys the slot's acceleration
  structure while scene set-1 binding 2 still names it, re-opening the #2673
  dangling-descriptor window that #2673's own fallback assumes cannot happen.
  Binding 2 is not `PARTIALLY_BOUND`. Impact is bounded: `rt_flag` gating still
  prevents traversal, so the expected symptom is a statically-used-invalid-
  descriptor VUID, not a use-after-traverse.
- **CON-D1-02 (MEDIUM, HYPOTHESIS)** — the static one-time BLAS build paths
  (`build_blas`, `build_blas_batched`) do not self-emit the leading
  scratch-serialize barrier that both skinned paths do (#983/#1300); the reverse
  cross-submission direction rests on a trailing barrier whose dst access mask is
  `AS_READ` only.

### What came back clean — and how hard it was pushed

The negative results here carry real weight, because several were reached by
actively trying to break an invariant rather than by pattern-matching:

- **All five Dimension-1 regression guards intact**: #284 (queue-Mutex
  discipline), #1790 (`record_scratch_serialize_barrier` dst mask `WRITE|READ`),
  #507945d8/#1436 (AS build-*input* barriers using `SHADER_READ` at the
  `ACCELERATION_STRUCTURE_BUILD` stage), #1782 (deferred `pending_destroy_scratch`),
  #a476b256 (deferred `pending_destroy_blas` vs in-flight ray-query reads).
- **Last run's two fixes verified still present in code**, not merely marked
  CLOSED: #2673 and #2674.
- **Every early exit in the acquire→submit window enumerated**: six `return Err`,
  zero bare `?`, all recovering the acquire signal.
- **Swapchain-recreate ordering attacked and held**: `recreate_for_swapchain`
  deliberately precedes `create_main_framebuffers`, so the #1211 sentinel cannot
  report "complete" against stale per-image vec sizes.
- **14 Dimension-2 guards intact**, including #1105's volumetrics
  `tlas_written` latch set/reset symmetry, #931's bloom post-barrier-only
  accounting (no pre-barrier reintroduction proposed), all four per-FIF
  ping-pong indexings (SVGF, TAA, volumetrics, bone palette), and the R1
  `MaterialBuffer` upload still landing before draw recording.
- **#2403's skin→fragment barrier re-traced through the `ray_hit.glsl` include
  graph**: only `triangle.frag` and `water.frag` reach it, so the missing COMPUTE
  dst bit is genuinely not a gap — a conclusion that required following the
  include graph rather than trusting the barrier's face value.

### Two pieces of skill/issue drift found (not code defects)

1. The Dimension-1 checklist's "guard must not be held across `queue_submit`"
   text is **wrong against the shipped code**. Already OPEN as **#2690**; not
   re-reported here.
2. The Dimension-2 checklist frames water-caustic as reading "the previous
   frame's slot". **Water-caustic is cleared every frame**, so that framing does
   not match current code. `audit-concurrency/SKILL.md` should be corrected.

### Deduplication

Three Dimension-2 candidates were dropped as already-filed rather than
re-reported: SSAO's two-frame-stale AO consumed by `triangle.frag` (**#2798**),
`clear_for_skip`'s stale `parked_frames` (**#2780**), and `build_tlas`'s second
LRU pass (**#2769**).

### Suggested next step

Run the `--cornell` harness and one dense cell under `BYRO_VALIDATION=1` in
release and diff the hazard count. That single run adjudicates CON-D2-01 and
CON-D1-02 together, and it is the same channel that confirmed #507945d8 (~40
RAW hazards/frame pre-fix). Until it is run, these two stay hypotheses.

---

## Dimension 1



Audit: `/audit-concurrency` · suite `rt-deep` · repo `/mnt/data/src/gamebyro-redux` (branch `main`)
Dedup baseline: `/tmp/audit/concurrency/issues.json` (2813 issues, 251 OPEN)
Prior-report overlap scan: `docs/audits/AUDIT_CONCURRENCY_2026-08-12.md` (the most recent Dim-1 run, 2 days old), plus the 2026-08-07 / 2026-08-03 concurrency reports.

## Scope & Coverage

### No device, no validation run

**There was no Vulkan device and no captured `BYRO_VALIDATION` run available for this audit.**
Every verdict below is derived from source order, symbol-level grep, and the Vulkan spec.
Per the skill's speculative-fix guardrail, no barrier / stage / layout change is proposed on
reasoning alone: the one finding whose consequence depends on driver/layer behaviour
(`CON-D1-02`) is filed explicitly as a **HYPOTHESIS** row with the concrete signal that would
confirm it. `CON-D1-01`'s *state* is provable from source order; only its *runtime effect* is
unconfirmed, and that is stated in its Verification Path.

### Files read (in full or in the relevant regions)

- `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame` in full across the
  fence-wait → acquire → record → submit → present window, plus the TLAS-build block and the
  end-of-frame shrink block.
- `crates/renderer/src/vulkan/sync.rs` — complete (533 lines), including the test module.
- `crates/renderer/src/vulkan/context/mod.rs` — the `graphics_queue` / `present_queue` field
  declarations and the `Arc::clone` construction site.
- `crates/renderer/src/vulkan/acceleration/blas_static.rs` — `build_blas`, `build_blas_batched`,
  `tick_deferred_destroy`, `drain_pending_destroys`, `evict_unused_blas`.
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` — `build_skinned_blas_batched_on_cmd`,
  `refit_skinned_blas`, `record_scratch_serialize_barrier`, `drop_skinned_blas`.
- `crates/renderer/src/vulkan/acceleration/tlas.rs` — `build_tlas`, `ensure_tlas_state`,
  `tlas_handle`.
- `crates/renderer/src/vulkan/acceleration/memory.rs` — complete.
- `crates/renderer/src/vulkan/acceleration/mod.rs` — `destroy()`.
- `crates/renderer/src/vulkan/acceleration/predicates.rs` — `submit_one_time`, `ScratchUser`,
  `requires_scratch_serialize_barrier_before`, `tlas_instance_should_shrink`.
- `crates/renderer/src/vulkan/acceleration/constants.rs`, `.../acceleration/tests.rs` (barrier tests).
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — complete.
- `crates/renderer/src/vulkan/context/resize.rs` — `recreate_swapchain_core`, the phase
  orchestrator `recreate_swapchain`, and the `recreate_for_swapchain` call site.
- `crates/renderer/src/vulkan/texture.rs` — `with_one_time_commands_inner`.
- `crates/renderer/src/vulkan/egui_pass.rs` — `dispatch`.
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings` +
  the descriptor-binding-flags block), `.../scene_buffer/descriptors.rs` (`write_tlas`),
  `.../scene_buffer/upload.rs` (`patch_camera_rt_flag`), `.../scene_buffer/constants.rs`.
- `crates/renderer/src/deferred_destroy.rs` — complete.
- `crates/renderer/src/vulkan/swapchain.rs` (sharing-mode block), `.../vulkan/device.rs` (queue families).
- `crates/renderer/shaders/triangle.frag` — the `rtEnabled` gate and all three
  `rayQueryInitializeEXT` sites; `crates/renderer/src/vulkan/context/geometry_pass.rs` water gate.

### Checklist items verified INTACT (regression guards — reported as guards, not findings)

| # | Checklist item | Verdict |
|---|---|---|
| 1 | **Queue submission is single-Mutex** (#284). `present_queue` is `Arc::clone(&graphics_queue)` when families match (`context/mod.rs`), an independent `Arc<Mutex<_>>` otherwise; the swapchain uses `SharingMode::CONCURRENT` in the split-family case (`swapchain.rs`). | INTACT |
| 1b | **Guard lifetime at the submit sites.** `draw_frame` binds the `MutexGuard` and holds it *across* `queue_submit` / `queue_present` — deliberately, per `VUID-vkQueueSubmit-queue-00893`, refined by #1713. `with_one_time_commands_inner` scopes the guard to the submit and drops it **before** `wait_for_fences`. Both are correct; the Dim-1 checklist text that says the guard "must NOT be held across `queue_submit`" is the already-filed doc defect — **OPEN #2690 (CONC-DOC)**, not re-reported here. | INTACT |
| 1c | **Single-threaded GPU access.** No `thread::spawn` in `crates/renderer` or `byroredux` touches the queue; the only worker (`byroredux/src/streaming.rs`) is a CPU-side ESM pre-parser. The queue `Mutex` is therefore uncontended today — a defensive invariant, not a live race. | INTACT |
| 2 | **Frame-in-flight discipline.** Both-slots `wait_for_fences` at the top of `draw_frame` (#282), `reset_fences` immediately before `queue_submit` (#952), `images_in_flight` aliasing guard + the `invalidate_images_in_flight_for_fence` cross-reference wipe (#1188). | INTACT |
| 2b | **Acquire → submit error-recovery window.** I enumerated every early exit between `acquire_next_image` and `queue_submit`: six `return Err(e)` sites, **zero bare `?`**. All six recover via `recreate_image_available_for_frame` (#910); the submit-failure arm additionally calls `recreate_in_flight_for_frame` (#952). | INTACT |
| 3 | **Acquire → render → present semaphore chain.** `render_finished` is per **swapchain image** (`render_finished[img]`), `image_available` per frame-in-flight; pinned by `render_finished_is_sized_and_indexed_per_swapchain_image` and `swapchain_image_count_is_not_the_frames_in_flight_count` (#2783). | INTACT |
| 4 | **AS build → read barrier.** Static: `blas_static.rs` `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` before the compaction query. Skinned: trailing `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` in `record_skinned_blas_refit`. TLAS: `AS_BUILD/AS_WRITE → FRAGMENT|COMPUTE/AS_READ` in `draw_frame`. | INTACT |
| 4b | **#1790 guard** — `record_scratch_serialize_barrier`'s dst mask is `ACCELERATION_STRUCTURE_WRITE_KHR \| ACCELERATION_STRUCTURE_READ_KHR`, and `build_skinned_blas_batched_on_cmd` self-emits it before `i == 0` (#1300) as well as between iterations. | INTACT |
| 5 | **#1782 deferred scratch destroy.** Both `build_blas`/`build_blas_batched` grow paths and both `shrink_blas_scratch_to_fit` arms route the retired buffer through `pending_destroy_scratch`. `build_skinned_blas_batched_on_cmd`'s own immediate grow-destroy left alone as instructed. | INTACT |
| 6 | **#507945d8 / #1436 AS build INPUT access flag.** `tlas.rs`'s transfer→build barrier uses `dst_access_mask(SHADER_READ)` at `ACCELERATION_STRUCTURE_BUILD_KHR`; `record_skinned_blas_refit`'s compute→build barrier likewise (`SHADER_READ`, dst stage widened to `AS_BUILD \| FRAGMENT` under #2403). Neither uses `ACCELERATION_STRUCTURE_READ_KHR` for inputs. | INTACT |
| 7 | **#a476b256 deferred AS destruction.** `drop_blas`, `drop_skinned_blas` and `evict_unused_blas` all route through `pending_destroy_blas` with `DEFAULT_COUNTDOWN`; no immediate `destroy_acceleration_structure` survives at an eviction site. `AccelerationManager::destroy` calls `drain_pending_destroys` first and then drains `blas_entries` / `tlas` / `skinned_blas` / scratch. `DeferredDestroyQueue::tick` decrements-then-frees, i.e. one frame *more* conservative than its own doc claims. | INTACT |
| 8 | **Swapchain recreate sync.** `recreate_swapchain_core` opens with `device_wait_idle`; `recreate_for_swapchain` runs **before** `create_main_framebuffers`, so the #1211 `framebuffers.is_empty()` sentinel cannot report "complete" while `render_finished`/`images_in_flight` still carry the old image count. (I specifically tried to construct an index-out-of-bounds on a grown swapchain here and could not — the ordering is deliberate and commented.) | INTACT |
| 9 | **#2673 / #2674 (last run's CRITICAL + HIGH) still fixed.** `ensure_tlas_state` is allocate-then-swap with a documented commit point and per-step `inspect_err` unwinds; `build_tlas` commits `mem::swap(last_blas_addresses)`, `needs_full_rebuild = false` and `last_blas_map_gen` *after* `cmd_build_acceleration_structures`. The `build_tlas` failure arm in `draw_frame` now re-points binding 2 to any surviving handle, clears `tlas_written[frame]`, and patches `rt_flag` to 0.0. | INTACT (both CLOSED as #2673 / #2674) |
| 10 | **One-time blocking submits in the frame hot path.** `upload_terrain_tiles` and `EguiPass::dispatch`'s `set_textures` do submit+fence-wait inside `draw_frame`, but both are change-gated (terrain-dirty transition; non-empty `textures_delta.set`), and the per-frame egui case is the already-closed #2719. Not re-reported. | ACCEPTED |

### Explicitly not re-reported (sibling coverage / existing issues)

- `REN-D1-01` and `REN-D1-03` from the renderer half of this suite (shared instance map;
  `shrink_tlas_scratch_to_fit` live-slot arm). `CON-D1-01` below touches the *adjacent*
  `shrink_tlas_to_fit` (the AS + instance-buffer shrink), not the scratch shrink — distinct symbol,
  distinct failure.
- OPEN **#2690** — the Dim-1 checklist's queue-Mutex text.
- OPEN **#2774**, **#2773**, **#2769** — existing acceleration-path issues.

---

## Findings

### CON-D1-01: `shrink_tlas_to_fit` destroys the slot's acceleration structure while scene set-1 binding 2 still names it, re-opening the #2673 dangling-descriptor window that the fix's own fallback assumes cannot happen

- **Severity**: MEDIUM
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` (`AccelerationManager::shrink_tlas_to_fit`); call site `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the end-of-frame shrink block); consumers `crates/renderer/src/vulkan/acceleration/tlas.rs` (`AccelerationManager::tlas_handle`), `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`SceneBuffers::write_tlas`), `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings` + the binding-flags map)
- **Status**: NEW (nearest neighbours: #2673 CLOSED — the `ensure_tlas_state` half of the same class, fixed; #2774 OPEN — the *scratch* shrink, a different symbol)
- **Description**: `shrink_tlas_to_fit` unconditionally takes the slot and calls
  `destroy_acceleration_structure(old.accel, None)` plus three `GpuBuffer::destroy`s. It does **not**
  clear `SceneBuffers::tlas_written[slot]` and does not re-point scene descriptor set-1 binding 2,
  which keeps naming the just-destroyed `VkAccelerationStructureKHR` until the next successful
  `write_tlas` on that slot.

  In the happy path this is harmless: the next frame on that slot re-enters `ensure_tlas_state`
  (which sees `tlas[slot].is_none()`), creates a replacement, and `write_tlas` re-points binding 2
  *before* the render pass begins. The problem is the failure path. The #2673 fix's fallback arm in
  `draw_frame` is written against the premise that a failed `build_tlas` still leaves an AS alive:

  > "Re-point the binding at whatever AS the manager still owns (post-#2673 a failed resize keeps
  > the previous one alive)"

  `shrink_tlas_to_fit` is the one path that falsifies that premise. After a shrink, the slot owns
  nothing, so on the next frame `accel.tlas_handle(frame)` returns `None`, the
  `if let Some(stale_handle)` guard does not fire, and **binding 2 is left naming a destroyed
  acceleration structure for the whole geometry pass.**

  Binding 2 is **not** `PARTIALLY_BOUND` — `build_scene_descriptor_bindings`'s flag map applies
  `DescriptorBindingFlags::PARTIALLY_BOUND` only to `b.binding >= 5`. `triangle.frag` declares and
  statically uses `topLevelAS`, so the "descriptors must be valid if statically used" rule applies
  in full; the `rtEnabled`/`sceneFlags.x` runtime gate does not downgrade static use to dynamic use.
- **Evidence**:
  ```rust
  // memory.rs — shrink_tlas_to_fit: destroys, does not touch tlas_written or binding 2
  if let Some(mut old) = self.tlas[slot_index].take() {
      self.accel_loader.destroy_acceleration_structure(old.accel, None);
      old.buffer.destroy(device, allocator);
      old.instance_buffer.destroy(device, allocator);
      old.instance_buffer_device.destroy(device, allocator);
  }
  ```
  ```rust
  // draw.rs — the #2673 fallback, whose Some(..) arm cannot fire after a shrink
  if let Some(stale_handle) = accel.tlas_handle(frame) {
      self.scene_buffers.write_tlas(&self.device, frame, stale_handle);
  }
  self.scene_buffers.tlas_written[frame] = false;
  ```
  ```rust
  // scene_buffer/buffers.rs — binding 2 is NOT partially bound
  if b.binding >= 5 { vk::DescriptorBindingFlags::PARTIALLY_BOUND } else { vk::DescriptorBindingFlags::empty() }
  ```
  Reachability of the shrink itself, from `tlas_instance_should_shrink` +
  `WORKING_SET_FLOOR == MIN_TLAS_INSTANCE_RESERVE == 8192` + `TLAS_REBUILD_SLACK_BYTES == 1 MiB`:
  it needs `max_instances > 16384`, which `ensure_tlas_state`'s `max(2 × instance_count, 8192)`
  padding reaches at >8192 live TLAS instances. `MAX_INSTANCES == 0x40000`, so this is an ordinary
  large-exterior→interior walk, not a synthetic case.
- **Impact**: Not a use-after-traverse — the #2673 defence in depth still runs unconditionally
  (`tlas_written[frame] = false` + `patch_camera_rt_flag(.., 0.0)`), and I verified every
  `rayQueryInitializeEXT` in `triangle.frag` is behind `rtEnabled` / `directShadowRayEnabled` /
  `giRayEnabled` / `reflectionGlassRayEnabled`, with the water pass host-gated on the same
  `tlas_written[frame]` signal in `geometry_pass.rs`. So no ray actually traverses the dead handle.
  What remains is a bound-and-statically-used-but-invalid descriptor for one or more frames —
  `VUID-vkCmdDraw*-None-08114` class. Blast radius widens to a real traversal only if the
  `patch_camera_rt_flag(0.0)` call *also* fails (it is `log::warn!`-only), in which case
  `rt_flag` stays 1.0 over a destroyed AS.
- **Trigger Conditions**: (a) a frame whose TLAS instance count leaves the other frame-in-flight
  slot's `max_instances` more than 2× the working floor with >1 MiB of slack — i.e. walking from a
  >8192-instance exterior into a small interior, which fires `shrink_tlas_to_fit(other_slot)` at the
  end of `draw_frame`; **and** (b) the *next* frame on that slot failing `build_tlas` — in practice a
  host-visible or device-local allocation failure inside `ensure_tlas_state` (VRAM/BAR exhaustion),
  which is the same VRAM-pressure regime the BLAS budget + LRU machinery exists to survive.
  Deterministically reproducible by fault-injecting a `GpuBuffer::create_*` failure inside
  `ensure_tlas_state` on the frame after a shrink, in the style of `BYRO_FSR_FORCE_DISPATCH_FAIL`.
- **Verification Path**: Not observable in `cargo test` (no headless device). The **state** is
  provable from source order and needs no device. The **runtime consequence** needs the
  validation layer: a `BYRO_VALIDATION=1` release run with the fault injected should report the
  destroyed-object-in-descriptor message at draw time
  (`VUID-vkCmdDrawIndexedIndirect-None-08114` family, "Descriptor in binding #2 index 0 is using
  acceleration structure … that is invalid or has been destroyed"). RenderDoc would show set 1
  binding 2 as an unresolvable AS handle on the affected frame.
- **Related**: #2673 (CLOSED — the `ensure_tlas_state` half; this is the residual its fallback
  cannot cover), #2774 (OPEN — `shrink_tlas_scratch_to_fit`, the sibling shrink), #2141 (CLOSED —
  the identical "recreate failure leaves scene binding N pointing at a destroyed view" shape on the
  SSAO binding), `REN-D1-03` from the renderer half of this suite.
- **Suggested Fix**: Make the shrink symmetric with the fix it undermines. Cheapest correct option:
  have `shrink_tlas_to_fit` clear `tlas_written[slot]` for the slot it retires (it already knows the
  slot index; the flag lives on `SceneBuffers`, so either return the retired-slot index to the
  `draw_frame` call site or thread the flag through). Structurally better: add
  `PARTIALLY_BOUND` to binding 2's flags so an unwritten/retired TLAS binding is a spec-legal
  "descriptor not dynamically used" — that also covers the pre-first-build case symmetrically with
  bindings 5+. Either way, **do not ship without a `BYRO_VALIDATION` run** confirming the descriptor
  message before and after.

---

### CON-D1-02 (HYPOTHESIS): the static one-time BLAS build paths do not self-emit the leading scratch-serialize barrier that both skinned paths do — the reverse cross-submission direction rests on a trailing barrier whose dst access mask is `AS_READ` only

- **Severity**: MEDIUM — **HYPOTHESIS row, not a fix.** Do not ship a barrier change on this
  reasoning alone.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`AccelerationManager::build_blas`, `AccelerationManager::build_blas_batched` — the
  `submit_one_time` closures, whose build loops emit `record_scratch_serialize_barrier` only for
  `i > 0`); counterpart `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`, `record_scratch_serialize_barrier`);
  the trailing barrier in `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`
  (`record_skinned_blas_refit`); rule model in
  `crates/renderer/src/vulkan/acceleration/predicates.rs` (`ScratchUser`,
  `requires_scratch_serialize_barrier_before`)
- **Status**: NEW (mirror direction of #1300 CLOSED and #983 / #644; no OPEN match)
- **Description**: `blas_scratch_buffer` is a **single shared allocation** used by four writers —
  `build_blas`, `build_blas_batched` (both on `submit_one_time` one-off command buffers), and
  `build_skinned_blas_batched_on_cmd` / `refit_skinned_blas` (both on the per-frame `cmd`). That
  sharing is established fact in this codebase (#2460 was filed precisely because
  `shrink_blas_scratch_to_fit` walked only the static half of it).

  The house rule, codified by `requires_scratch_serialize_barrier_before` and its `ScratchUser`
  enum, is that *any* prior writer to the shared scratch — including across a submission boundary —
  requires an `AS_WRITE → AS_WRITE` dependency before the next build reuses it, and that a host
  fence-wait does **not** substitute for it. Both skinned paths self-emit that leading barrier
  (`refit_skinned_blas` under #983, `build_skinned_blas_batched_on_cmd`'s `i == 0` under #1300).
  **Neither static path does.** `build_blas_batched`'s loop is `if i > 0 { record_scratch_serialize_barrier(..) }`
  with no pre-loop emit, and `build_blas` records a single build with none at all.

  The direction that leaves unguarded is the mirror of the one the enum models. `ScratchUser`
  enumerates only `CrossSubmissionBuildWithFenceWait` — "a one-time BUILD ran earlier this frame and
  the host has since fence-waited it". The reverse is: the **previously-submitted per-frame command
  buffer's** skinned builds/refits are still executing on the GPU, writing this scratch, when
  `step_streaming` (in `about_to_wait`, before the next `draw_frame`'s fence wait — the exact window
  #1782's own comment names) submits a static `build_blas_batched`. Nothing has fence-waited
  anything in that direction.

  **The reason this is a HYPOTHESIS and not a defect claim**: the hazard is not entirely
  unguarded. `record_skinned_blas_refit` closes its skinned block with an
  `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ` `memory_barrier`, and a `vkCmdPipelineBarrier`'s second
  synchronization scope includes commands *later in submission order on the same queue* — i.e. the
  subsequent one-time submission. I checked the gating: that trailing barrier and the scratch writes
  are **co-gated** on the same `if !dispatches.is_empty()` block, so whenever the frame command
  buffer writes `blas_scratch_buffer` the trailing barrier is emitted too. So an execution
  dependency does exist. The open question is narrowly whether an `AS_READ`-only **dst access mask**
  is sufficient for the write-after-write on the scratch region, or whether `AS_WRITE` must appear
  there — the exact symmetric question #1790 answered in the other direction when it added `AS_READ`
  to a `WRITE`-only dst mask on `record_scratch_serialize_barrier`.
- **Evidence**:
  ```rust
  // blas_static.rs — build_blas_batched Phase 4: no pre-loop barrier
  let build_result = submit_one_time(device, queue, command_pool, transfer_fence, |cmd| {
      for (i, p) in prepared.iter().enumerate() {
          if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
          ...
  ```
  ```rust
  // blas_skinned.rs — the symmetric path DOES pre-emit (#1300 / D12B-1)
  if !prepared.is_empty() { self.record_scratch_serialize_barrier(device, cmd); }
  for (i, p) in prepared.iter().enumerate() {
      if i > 0 { self.record_scratch_serialize_barrier(device, cmd); }
  ```
  ```rust
  // skinned_blas_refit.rs — the only thing standing between the two directions
  memory_barrier(&self.device, cmd,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
      vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
      vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR);  // dst access: READ only
  ```
  The window is named by the codebase itself, in `build_blas_batched`'s own #1782 comment: *"This is
  the M40 streaming hot path (called from `step_streaming` in `about_to_wait`), the exact window
  where the previously-submitted frame's skinned-BLAS refit/first-sight command buffer may still be
  executing on the GPU."* #1782 fixed the *destroy* of the old buffer in that window; the *reuse* of
  the same buffer when no grow is needed was not part of that fix. I confirmed no `device_wait_idle`
  or fence wait exists anywhere on the `byroredux` streaming → cell-load → `build_blas_batched` path.
- **Impact**: If the `AS_READ`-only dst mask is insufficient, a cell-load BLAS build overlapping an
  in-flight skinned-BLAS refit corrupts the shared scratch region for one or both builds — a
  malformed BVH, which surfaces as wrong or missing shadows / reflections / GI on the affected
  meshes, or a GPU fault in the worst case. That matches the "BLAS/TLAS build with wrong geometry"
  severity row. It is intermittent and driver-scheduling-dependent, which is exactly why this must
  be confirmed before being fixed rather than fixed on reasoning.
- **Trigger Conditions**: A frame that (a) records at least one skinned-BLAS build or refit into the
  per-frame `cmd` (any visible NPC with a live skin slot), followed by (b) `step_streaming` in the
  same `about_to_wait` reaching `build_blas_batched` or `build_blas` with `need_new_scratch == false`
  (the existing scratch is already big enough — the steady-state case), while (c) the GPU has not yet
  retired the frame submission. Highest-probability repro: walking across an exterior cell boundary
  with NPCs on screen, at a frame rate where the GPU is ~1 frame behind.
- **Verification Path**: Not observable in `cargo test` — no headless device, and the existing
  `scratch_barrier_*` tests only pin the *predicate*, never the emitted masks on the static path.
  **Validation layer is the check**: a `BYRO_VALIDATION=1` **release** run streaming an exterior grid
  with NPCs, watching for Synchronization Validation `WRITE_AFTER_WRITE` on the
  `blas_scratch_buffer` allocation at `vkCmdBuildAccelerationStructuresKHR`. Caveat worth stating:
  `predicates.rs`'s own comment asserts *"validation layers do NOT catch it because they reason
  per-submission"* — that was written in the #983 era; current Synchronization Validation does
  maintain per-queue submission history and does report cross-submission hazards, so the run is
  worth doing, but a clean run is weaker evidence here than usual. If the layer stays silent,
  RenderDoc's resource-usage view on `blas_scratch_buffer` across the two submissions is the
  fallback. **Absent one of those signals, do not change the barrier** — the cheap, spec-safe move
  (a pre-loop `record_scratch_serialize_barrier` in both static paths, exactly mirroring #1300) is
  still a barrier change and falls under the guardrail.
- **Related**: #1300 (CLOSED — the identical gap on the skinned batched builder's `i == 0`, fixed by
  a pre-loop self-emit), #983 / #644 (CLOSED — the `refit_skinned_blas` self-emit and the original
  cross-submission scratch bug), #1782 (CLOSED — the *destroy* half of this same window),
  #1790 (CLOSED — the symmetric dst-access-mask question, answered in the other direction),
  #1797 (CLOSED — the throughput cost of the shared scratch, and the standing decision not to
  sub-allocate it).
- **Suggested Fix** (only after the signal above): mirror #1300 — emit
  `record_scratch_serialize_barrier` once before the first build in both `build_blas` and
  `build_blas_batched`'s `submit_one_time` closures, and extend `ScratchUser` with the reverse
  direction (e.g. a `CrossSubmissionRefitStillInFlight` variant) so
  `requires_scratch_serialize_barrier_before` and its unit tests pin both directions rather than one.
  The barrier is idempotent and same-stage, so the cost on a queue with no in-flight AS work is
  negligible.

---

## Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 2 | CON-D1-01, CON-D1-02 |
| LOW | 0 | — |
| **Total** | **2** | of which **1 is a HYPOTHESIS row** (CON-D1-02) |

No CRITICAL or HIGH survived. Every named regression guard in the Dimension-1 checklist
(#284, #1790, #507945d8/#1436, #1782, #a476b256) is intact, as are last run's two fixes
(#2673, #2674). The acquire→submit error-recovery window and the swapchain-recreate ordering
were both attacked directly and held.

---

## Dimension 2



Run: 2026-08-14 · preset `rt-deep` · depth `deep`

## Scope & Coverage

**No Vulkan device, no captured validation run, no RenderDoc capture backed any
verdict below.** Every conclusion is source-derived. Per the skill's
speculative-fix guardrail, the single finding is filed as a **HYPOTHESIS row**
with a named confirming signal, not as a shippable barrier change.

### Files read (in full or in the relevant span)
- `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame` master ordering
  (skin upload → palette dispatch → `record_skinned_blas_refit` → `build_tlas` →
  cluster cull → instance/material upload → bulk HOST barrier →
  `record_geometry_pass` → `record_post_passes` → submit), plus the pure
  predicates `next_clean_skin_frames` / `should_skip_skin_gpu_refresh`.
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` —
  `record_skinned_blas_refit` end to end, including all four source-position
  regression modules.
- `crates/renderer/src/vulkan/context/post_passes.rs` — `record_post_passes`
  and all eight `record_*_pass` helpers + `copy_depth_to_history` +
  `caustic_skip_clear_decision`.
- `crates/renderer/src/vulkan/skin_compute.rs` — `SkinComputePipeline::dispatch`,
  `create_slot`, the `#2743` `descriptor_bindings` cache key.
- `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` —
  `refit_skinned_blas` prologue (`record_scratch_serialize_barrier`,
  `validate_refit_flags`, `validate_refit_counts`).
- `crates/renderer/src/vulkan/acceleration/tlas.rs` — `build_tlas` +
  `ensure_tlas_state` failure paths.
- `crates/renderer/src/vulkan/svgf.rs` — `dispatch`, `write_atrous_descriptor_sets`,
  `indirect_view`, `advance_completed_frames`, `should_force_history_reset`.
- `crates/renderer/src/vulkan/taa.rs` — `dispatch`, `upload_params`,
  `mark_frame_completed`, the `frames_since_creation` scalar rationale.
- `crates/renderer/src/vulkan/caustic.rs` — `dispatch`, `clear_for_skip`,
  `initialize_layouts`, `advance_parked_visits`.
- `crates/renderer/src/vulkan/water_caustic.rs` — `clear_pre_render_pass`,
  `barrier_post_render_pass`, `initialize_layouts`.
- `crates/renderer/src/vulkan/volumetrics.rs` — `dispatch` (Stages A–F),
  `write_tlas`, `write_lights_and_clusters`, `record_neutral_frame`,
  `mark_frame_completed`, `signal_history_reset`.
- `crates/renderer/src/vulkan/bloom.rs` — `dispatch` (down + up chains),
  `output_view`.
- `crates/renderer/src/vulkan/ssao.rs` — `dispatch` barrier pair.
- `crates/renderer/src/vulkan/composite.rs` — composite render-pass
  in/out subpass dependencies.
- `crates/renderer/src/vulkan/scene_buffer/upload.rs` — `record_bone_world_copy`;
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs` — the binding-3 / binding-12
  bone-palette ring wiring.
- `byroredux/src/render/skinned.rs` — `pose_hash`, the `try_mark_pose_dirty` feed;
  `crates/core/src/ecs/resources/skin_slot_pool.rs` — `try_mark_pose_dirty`,
  `clear_pose_dirty`, `sweep`.
- Shaders: `crates/renderer/shaders/triangle.vert`, `triangle.frag`,
  `water.frag`, `caustic_splat.comp`, `volumetrics_inject.comp`,
  `include/ray_hit.glsl`, `include/shadow_common.glsl`, `include/raytrace.glsl`,
  `include/bindings.glsl` (include-graph trace for `skinnedVertexAddress`).

### Checklist items verified INTACT (regression guards, reported as such)
1. **Skin chain (M29), palette half.** `draw_frame` emits the
   `SHADER_WRITE → SHADER_READ` buffer barrier on the palette buffer with
   `dst_stage = COMPUTE_SHADER | VERTEX_SHADER` immediately after
   `SkinPaletteComputePipeline::dispatch`; `record_bone_world_copy` carries its
   own `TRANSFER_WRITE → SHADER_READ` / `TRANSFER → COMPUTE_SHADER` barrier.
2. **Skin chain, skin→AS→fragment half (#2403).** `record_skinned_blas_refit`'s
   post-dispatch `memory_barrier` still carries
   `dst = ACCELERATION_STRUCTURE_BUILD_KHR | FRAGMENT_SHADER` with
   `SHADER_READ`. I re-traced the include graph: `include/ray_hit.glsl` (the
   only `skinnedVertexAddress` dereference, via `SkinnedVertexRef`) is reached
   **only** from `triangle.frag` (through `include/raytrace.glsl`) and
   `water.frag` — both FRAGMENT. `caustic_splat.comp` and
   `volumetrics_inject.comp` include `include/shadow_common.glsl`, which touches
   no geometry buffer, so the missing `COMPUTE_SHADER` dst bit on that barrier
   is **not** a gap today.
3. **No raster-from-skinned-SSBO path.** `triangle.vert` inline-skins from the
   palette SSBO (set 1 binding 3) and annotates `skinnedVertexAddress` as
   "unused here"; `water.vert` and `ui.vert` do the same. No `VERTEX_INPUT`
   barrier is required, matching the checklist's standing note.
4. **Bone-palette cross-frame ping-pong.** `buffers.rs` binds binding 12
   (`bones_prev`) to `bone_device_buffers[(i + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT]`
   — the *other* ring slot, never slot `i`.
5. **#1811 skin-refresh skip cannot desync the palette.** `should_skip_skin_gpu_refresh`
   only fires after `MAX_FRAMES_IN_FLIGHT + 1` clean frames, and every path that
   changes palette *content or layout* forces a dirty frame: a new/re-admitted
   entity is a `last_pose_hash` miss in `try_mark_pose_dirty` (because
   `SkinSlotPool::sweep` removes the entry on eviction) and also queues a
   first-sight `bind_inverses` upload. I specifically tried to construct a
   "slot re-assigned, matrices unchanged, therefore not dirty" case and could
   not: `sweep` drops `last_pose_hash` in the same loop that returns the slot to
   `free_list`.
6. **SVGF ping-pong.** `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT` for prev mesh_id /
   prev normal / `indirect_history` / `moments_history`; the `MAX_FRAMES_IN_FLIGHT >= 2`
   `const _: () = assert!` gate is still present. The temporal post-barrier's
   `dst = FRAGMENT_SHADER | COMPUTE_SHADER` still covers both the à-trous seed
   and next frame's history read; the à-trous chain's per-iteration barrier
   widens to `FRAGMENT_SHADER` only on the final iteration, which is the slot
   `indirect_view` returns.
7. **TAA ping-pong.** Same `(f + 1) % MAX_FRAMES_IN_FLIGHT` prev wiring and the
   same compile-time gate. `frames_since_creation` remaining a scalar `u32` is
   still justified by the three all-slot reset entry points (`new_inner`,
   `signal_history_reset`, `recreate_on_resize`) — no per-slot reset path has
   been added.
8. **Volumetrics gate (#1105) set/reset symmetry.** `write_tlas` and
   `write_lights_and_clusters` each latch their slot true; `dispatch`
   `debug_assert!`s then resets both *before* the first fallible statement, so
   an early `Err` cannot strand a latch. `record_neutral_frame` sets neither
   latch and additionally clears `history_valid` + `dispatched_this_frame`, so a
   neutral frame cannot leave the next injection reading a two-frame-old
   `lighting_volumes[previous]` while claiming valid history. Both `record_neutral_frame`
   call sites in `record_volumetrics_pass` are on paths that never call `write_tlas`.
9. **Volumetrics ping-pong.** Injection binding 6 reads
   `lighting_volumes[(f + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT]`;
   Stages B/D/F barrier `lighting_volumes[frame]`, `integrated_volumes[frame]`,
   and publish the integration write to `FRAGMENT_SHADER` for composite.
10. **Bloom #931 accounting.** Post-barrier-only, on the just-written mip, in
    both chains. Every read is covered: up-iteration `i` reads `up_mips[i+1]`
    (published by iteration `i+1`'s post-barrier), `down_mips[i]` (published by
    the down chain), and the seed `down_mips[BLOOM_MIP_COUNT-1]`. The final
    `up_mips[0]` post-barrier switches `dst_stage` to `FRAGMENT_SHADER`, and
    `bloomTex` (composite.frag set 0 binding 7) is its only consumer — I
    grepped for a compute reader of `output_view` / `output_views` and found
    none, so dropping `COMPUTE_SHADER` there is correct. **No pre-barrier
    reintroduction is proposed.**
11. **Caustic CLEAR → COMPUTE → FRAGMENT.** Both arms of `CausticPipeline::dispatch`
    (parked EMA decay-then-splat, and moving-camera clear-then-splat) end in the
    `COMPUTE_SHADER → FRAGMENT_SHADER` publish; `clear_for_skip` leaves the slot
    in the same GENERAL/`SHADER_READ` state, and `caustic_skip_clear_decision` is
    unit-pinned. `advance_parked_visits` is per-FIF-slot (`parked[frame]`), which
    is what the per-slot accumulator requires.
12. **Water-caustic per-FIF `R32_UINT`.** `clear_pre_render_pass` (pre-main-pass
    clear + `TRANSFER → FRAGMENT_SHADER` publish) is unconditional in `draw_frame`;
    `barrier_post_render_pass` (`FRAGMENT → FRAGMENT`) runs at the head of
    `record_svgf_pass`, before composite. The accumulator is cleared every frame,
    so there is no cross-frame read at all — the checklist's "reads the previous
    frame's slot" framing does not describe the current code.
13. **MaterialBuffer SSBO (R1).** `upload_materials` still runs in `draw_frame`'s
    host-upload block, before the bulk
    `HOST_WRITE → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT` barrier and before
    `record_geometry_pass`. It has **not** moved into a mid-frame compute path.
14. **Adjacent chain links spot-checked and clean:** SSAO's
    `FRAGMENT_SHADER → COMPUTE_SHADER` WAR barrier on `ao_images[frame]` plus its
    `COMPUTE → FRAGMENT` restore; `copy_depth_to_history`'s depth
    READ_ONLY→TRANSFER_SRC→READ_ONLY dance (restore dst includes `COMPUTE_SHADER`
    for SVGF/SSAO); composite's `composite_dep_out`
    (`COLOR_ATTACHMENT_OUTPUT → COMPUTE_SHADER | TRANSFER`) covering the FSR
    compute read of `scene_image(frame)`.

### Items NOT verifiable without a device (stated, not guessed)
- Whether any of the "defensive / over-specified" barriers (the volumetrics
  Stage-B `history_ready` and `pre_inject`, the SVGF pre-dispatch `FRAGMENT_SHADER`
  src bit deferred by #962, the TAA pre-barrier) are genuinely redundant or
  load-bearing. Their source scopes name work from a *previous submission*, which
  a `vkCmdPipelineBarrier` in this command buffer cannot reach; whether the
  both-slots fence wait is sufficient in the driver's view is a Synchronization
  Validation question, not a source question. I did **not** propose narrowing any
  of them, consistent with #962's deferral.
- Cross-submission memory availability for AS build inputs written by an earlier
  one-time submit (the `#644` / `#983` class of question) — already litigated in
  this repo, out of scope here, and untestable without SyncVal.
- Whether the failure window in CON-D2-01 is actually reachable on the dev GPU
  (12 GB RTX 4070 Ti); it requires a VRAM-pressure `build_tlas` failure and there
  is no fault-injection env var for it (unlike `BYRO_FSR_FORCE_DISPATCH_FAIL`).

### Deduplication
Checked all 2813 rows in `/tmp/audit/concurrency/issues.json` (251 OPEN) plus
`docs/audits/AUDIT_CONCURRENCY_*.md`. Three candidates were **dropped as
already-filed**:
- SSAO's AO texture is consumed by `triangle.frag` (set 1 binding 7) during the
  *main* render pass while `record_ssao_pass` dispatches afterwards, so the
  sampled AO is two frames stale and the helper's doc claiming "current-frame
  (no lag)" is wrong → **Existing: #2798** (`REN-D8-NEW-02`).
- `clear_for_skip` leaving `parked_frames` stale across a skip streak →
  **Existing: #2780**.
- `build_tlas`'s second LRU-stamp pass over `draw_commands` → **Existing: #2769**.

Nothing was found for the finding below (searched `tlas`, `barrier`, `refit`,
`blas`, `build_tlas fail`, `warn-only`; #2673 / #2674 are the adjacent CLOSED
`build_tlas` failure-path fixes and neither covers this).

---

## Findings

### CON-D2-01: `build_tlas` failure arm skips the frame's only AS_BUILD→ray-query barrier, leaving that frame's skinned-BLAS refits unpublished to the volumetrics compute ray query
- **Severity**: HIGH — *filed as a **HYPOTHESIS** row per the speculative-fix guardrail; do not ship a barrier change on this reasoning alone.*
- **Dimension**: Compute → AS → Fragment Chains
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` — `draw_frame`, the `if let Err(e) = accel.build_tlas(...)` arm and its `else` sibling; paired with `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — `record_skinned_blas_refit`, and `crates/renderer/src/vulkan/context/post_passes.rs` — `record_volumetrics_pass`
- **Status**: NEW
- **Description**:
  `record_skinned_blas_refit` runs immediately before the TLAS build in the same
  command buffer. Its terminal barrier is
  `ACCELERATION_STRUCTURE_BUILD_KHR / ACCELERATION_STRUCTURE_WRITE_KHR →
  ACCELERATION_STRUCTURE_BUILD_KHR / ACCELERATION_STRUCTURE_READ_KHR` — scoped
  deliberately to hand the refit results to the TLAS build, nothing further. The
  *only* barrier in `draw_frame` that publishes acceleration-structure writes to
  the ray-query consumers (`memory_barrier(..., ACCELERATION_STRUCTURE_BUILD_KHR,
  ACCELERATION_STRUCTURE_WRITE_KHR, FRAGMENT_SHADER | COMPUTE_SHADER,
  ACCELERATION_STRUCTURE_READ_KHR)`, the `#415` COMPUTE widening) lives inside
  the **success** branch of `build_tlas`.

  On the failure branch, `draw_frame` writes the stale handle via
  `scene_buffers.write_tlas`, clears `scene_buffers.tlas_written[frame]`, and
  calls `patch_camera_rt_flag(.., 0.0)` — but emits **no** AS barrier at all.
  This frame's per-entity skinned-BLAS refits (`refit_skinned_blas`, `src == dst`,
  in-place) and any same-`cmd` first-sight builds
  (`build_skinned_blas_batched_on_cmd`) are therefore never made available to
  `FRAGMENT_SHADER` / `COMPUTE_SHADER` acceleration-structure reads.

  `rt_flag = 0.0` mostly saves this: `triangle.frag`'s `rtEnabled = sceneFlags.x > 0.5`
  gates every one of its ray queries, and `caustic_splat.comp` early-outs on
  `if (sceneFlags.x < 0.5) return;`. **`volumetrics_inject.comp` has no such
  gate.** It declares `topLevelAS` at set 0 binding 2 and reaches
  `rayQueryInitializeEXT` through `traceShadowBinary`
  (`crates/renderer/shaders/include/shadow_common.glsl`), whose only guard is
  `mask == 0u || tMax <= tMin`. `record_volumetrics_pass` gates its dispatch on
  `accel.tlas_handle(frame)` being `Some` — and after a `build_tlas` failure it
  *is* `Some`, because `ensure_tlas_state`'s `#2673` allocate-then-swap commit
  point leaves `self.tlas[frame_index]` untouched on every early return. That
  stale TLAS still contains instances pointing at the same per-entity BLAS
  device addresses that were refit in-place earlier in this very command buffer.

  Net: on a `build_tlas`-failure frame with fog active, the volumetrics injection
  compute pass ray-queries acceleration structures whose writes from the same
  command buffer carry no memory dependency to `COMPUTE_SHADER` /
  `ACCELERATION_STRUCTURE_READ_KHR`.
- **Evidence**:
  - `crates/renderer/src/vulkan/acceleration/tlas.rs` — `build_tlas` has exactly
    two fallible statements, `ensure_tlas_state(...)?` and
    `tlas.instance_buffer.write_mapped(device, &instances)?`, both strictly
    *before* `self.accel_loader.cmd_build_acceleration_structures(...)`. So the
    failure path records no AS write of its own — which is precisely why the
    missing barrier is easy to read as harmless, and why the *earlier* refits are
    the exposed party.
  - `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` — the closing
    `memory_barrier` in `record_skinned_blas_refit` is
    `AS_BUILD/AS_WRITE → AS_BUILD/AS_READ`; its comment states the intent
    plainly ("BLAS refit writes → TLAS build reads").
  - `crates/renderer/shaders/include/shadow_common.glsl` — `traceShadowBinary`
    guards only on `mask`/`tMax`, no scene/RT flag.
  - `crates/renderer/shaders/volumetrics_inject.comp` — declares `topLevelAS`
    and includes `include/shadow_common.glsl`; `VolumetricsParams` carries no
    RT-enable field.
  - `crates/renderer/src/vulkan/context/post_passes.rs` —
    `record_volumetrics_pass` dispatches whenever
    `(accel.tlas_handle(frame), cluster_cull-derived lights)` are both `Some`,
    with no consultation of `scene_buffers.tlas_written` or the camera `rt_flag`.
  - No barrier between the failed `build_tlas` and `record_volumetrics_pass`
    covers this: cluster cull's trailing barrier is
    `COMPUTE/SHADER_WRITE → FRAGMENT/SHADER_READ` (wrong src access scope for an
    AS write), and the bulk pre-render-pass barrier is `HOST_WRITE`-sourced.
- **Impact**:
  Read-after-write on acceleration-structure memory from a compute ray query.
  Practical blast radius on a failure frame: garbage volumetric shadow
  visibility (fog flicker / black froxels) at best; on drivers that fault on
  partially-written BVH traversal, a device-lost. Bounded to frames where
  `build_tlas` fails — i.e. VRAM exhaustion during a dense-cell TLAS grow — but
  those are exactly the frames already under stress, and the `#2673` /`#2674`
  work established that this warn-only failure path is a real, reachable state
  worth hardening rather than a theoretical one. The gap also widens silently
  the moment any RT consumer stops honouring `rt_flag`.
- **Trigger Conditions**:
  A single frame in which **all** of: (a) at least one skinned entity is drawn
  and its BLAS is refit or first-sight-built into the per-frame `cmd`;
  (b) `accel.build_tlas` returns `Err` (either `ensure_tlas_state` failing to
  allocate the TLAS buffer / AS / scratch, or `instance_buffer.write_mapped`
  failing); (c) `self.tlas[frame]` from a prior frame is still live, so
  `tlas_handle(frame)` is `Some` — guaranteed by `#2673`'s commit-point
  discipline; (d) volumetrics is not on the neutral-frame path, i.e.
  `fog_extinction_per_meter > 0` or `fog_volumes` non-empty, and `cluster_cull`
  is present. Requires no thread interleaving — it is a single-command-buffer
  GPU-stage reordering window.
- **Verification Path**:
  **Not observable in `cargo test`** (no headless device assertion reaches a
  barrier scope) and not visible in a normal validation run, because the trigger
  needs an allocation failure. Confirming signal, in order of cheapness:
  1. Add a temporary fault injection mirroring `BYRO_FSR_FORCE_DISPATCH_FAIL`
     (e.g. force `build_tlas` to return `Err` on one frame), run a fogged
     exterior with a skinned actor under `BYRO_VALIDATION=1` (release build,
     Synchronization Validation on via `instance.rs::validation_enabled`), and
     look for a `SYNC-HAZARD-READ-AFTER-WRITE` on the skinned BLAS backing
     buffer / acceleration structure at the `vkCmdDispatch` of the volumetrics
     injection pass. That message is the confirmation; its absence over a
     forced-failure run is the disproof.
  2. RenderDoc: capture a forced-failure frame and compare the resource state of
     a per-entity skinned BLAS between the `refit_skinned_blas` build command and
     the volumetrics inject dispatch — the absence of any intervening barrier
     touching AS memory is directly visible in the command list.
  3. Visible artifact class (weakest): fog/godray flicker or black froxel columns
     around skinned actors on the single frame after a TLAS-allocation warning
     (`"TLAS build failed: …"`) in the log.
- **Related**: #2673 (CONC-D1-NEW-01 — introduced the stale-handle + `rt_flag`
  clearing on this same failure arm, without an AS barrier), #2674
  (CONC-D1-NEW-02 — moved `build_tlas`'s bookkeeping commit past the recorded
  build for the same failure arm), #415 (the `COMPUTE_SHADER` dst widening on the
  success-arm barrier this finding says is unreachable on failure), #2403
  (CHAIN2-D2-01 — the sibling case where a chain relied on another pass's
  incidental trailing barrier), #1105. Adjacent to but distinct from the
  Dimension 1 AS-build/read-barrier sweep: the barrier in question is the
  *terminal link of the M29 skin chain*, and the exposure is a compute consumer
  in the post-pass sequence.
- **Suggested Fix** *(direction only — do not land without the confirmation
  above)*: hoist the `AS_BUILD/AS_WRITE → FRAGMENT|COMPUTE / AS_READ`
  `memory_barrier` out of `build_tlas`'s `else` arm so it is emitted on both
  arms whenever `record_skinned_blas_refit` recorded any AS write this frame
  (`skin_dispatch_ran` plus a non-empty refit/build set is the existing signal).
  A strictly-additive alternative that needs no barrier reasoning: make
  `record_volumetrics_pass` skip its dispatch (falling through to
  `record_neutral_frame`) when `scene_buffers.tlas_written[frame]` is `false`,
  which is exactly the "this frame's TLAS never landed" latch the failure arm
  already clears — that closes the only currently-reachable consumer without
  touching a stage mask.

---

**Summary**: 1 finding (1 HIGH, filed as HYPOTHESIS). 14 checklist guards
verified intact. 3 candidates dropped as already-filed (#2798, #2780, #2769).

---

