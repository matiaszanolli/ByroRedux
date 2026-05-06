# Concurrency and Synchronization Audit — 2026-05-06

**Scope (focused)**: Dimension 2 (Vulkan Sync) and Dimension 3 (Resource Lifecycle) ONLY.
Dimensions 1 (ECS Locking), 4 (Thread Safety), 5 (Compute → AS → Fragment Chains), and
6 (Worker Threads) were NOT re-audited in this pass — see the most recent coverage in:
- `AUDIT_CONCURRENCY_2026-05-05.md` (dims 4, 6)
- `AUDIT_CONCURRENCY_2026-04-12.md` (full sweep across dims 1–4)
- `AUDIT_CONCURRENCY_2026-04-10.md` (early-engine sweep)

**Trigger**: User-invoked `/audit-concurrency 2 3`. Dims 2 and 3 hadn't been re-audited
since the 2026-04-12 sweep, and the renderer has had several material changes since then
(M29 skin compute → BLAS refit chain, M37.5 TAA, R1 MaterialBuffer SSBO, caustic accumulator
pass, #654 / LIFE-M1 swapchain handoff, #665 / LIFE-L1 allocator UAF avoidance).

**Methodology**: Static read of every cited path; re-checked each finding's premise against
the current code before retaining it. ~5 of 30 findings in the 2026-04 sweep had stale
premises by the time they were filed; this pass cross-checked the open issue list and the
recent renderer audits before adding anything to the NEW pile.

---

## Executive Summary

| Severity | Count | Dimension |
|----------|-------|-----------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 1 | Resource Lifecycle |
| LOW      | 2 | Vulkan Sync (1), Resource Lifecycle (1) |
| INFO     | 8 | Vulkan Sync (4), Resource Lifecycle (4) |

**3 actionable findings, 8 informational invariant pins. No CRITICAL or HIGH bugs.**

The renderer's synchronization and resource-lifecycle layers are in much better shape
than prior sweeps suggested. Several findings from `AUDIT_CONCURRENCY_2026-04-12.md` have
quietly been fixed:

- `C2-02` (SVGF reads previous frame-slot G-buffer without fence sync) — fixed.
  `draw_frame` now waits on both in-flight fences at frame start (`draw.rs:108-120`).
- `C2-01` (composite param UBO missing host barrier) — fixed via the unified
  `HOST → VS|FS|DRAW_INDIRECT` barrier at `draw.rs:1118-1144`.
- `C3-08` (rebuild_geometry_ssbo destroying buffers without device_wait_idle) —
  superseded by the deferred-destroy ticks at `draw.rs:177-185` (`mesh_registry`,
  `texture_registry`, `accel_manager` all participate).
- `#654` (LIFE-M1 swapchain image-view destruction order) — fixed and pinned by a
  static source-order test (`resize.rs:480-538`).
- `#665` (LIFE-L1 allocator outstanding-Arc UAF) — fixed via intentional leak of
  device + surface + instance when `Arc::try_unwrap` fails (`mod.rs:1676-1714`).

One existing issue appears to have been silently fixed without being closed:

- **#677 (DEN-9)** — SVGF / TAA `recreate_on_resize` doesn't re-issue UNDEFINED→GENERAL.
  Current code DOES re-call `initialize_layouts` after every resize for SVGF, TAA, and
  Caustic (`resize.rs:312-316, 358-362, 413-417`). **Recommend closing #677 with a
  reference to those three call sites.**

The single MEDIUM finding (`L1` / #655 / LIFE-M2) is a defensive gap, not an active bug:
`SwapchainState::destroy` takes `&self` and leaves `image_views` populated after the
destroy loop. No current call path triggers a double-free; promoting to `&mut self` and
clearing the vec closes the gap at zero runtime cost.

### Dedup notes

Existing concurrency-related issues confirmed open or closed and NOT re-reported:

- **#655 (LIFE-M2)** — OPEN, re-verified present at `swapchain.rs:202-208`. Re-stated as
  L1 below to keep dim 3 self-contained.
- **#677 (DEN-9)** — OPEN, but appears FIXED — see analysis above. Close-or-verify.
- **#661 (SY-4)** — OPEN, but the "legacy ACCELERATION_STRUCTURE_READ_KHR" complaint is
  spec-correct under sync1; the issue may be tracking a sync2 upgrade. Clarify scope.
- **#856 (C6-NEW-03)** — OPEN, dim 6, out of scope for this pass.
- **#46, #92, #267, #823, #826, #829** — covered in earlier audits; out of scope.

---

## Findings

### V1: TLAS instance-buffer host-write → AS-build-read implicit-barrier reliance
- **Severity**: LOW — safe in practice, spec-fragile
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/acceleration.rs::build_tlas` (called from
  `crates/renderer/src/vulkan/context/draw.rs:729-736`)
- **Status**: NEW (informational; no current failure)
- **Trigger Conditions**: Host writes to `tlas_state[frame].instance_buffer` happen
  inside `build_tlas` immediately before the AS build that consumes them. The build is
  recorded into the per-frame command buffer.
- **Description**: The instance buffer is HOST_VISIBLE | HOST_COHERENT, so writes are
  visible without `vkFlush`. Per Vulkan spec, host writes still require a
  `HOST → ACCELERATION_STRUCTURE_BUILD` barrier (or the device-side queue-submit
  boundary, which DOES insert an implicit `HOST → ALL_STAGES` barrier). Today every
  `build_tlas` call comes from `draw_frame`, so the implicit submit barrier covers the
  case. The latent hazard would only surface if `build_tlas` were called outside a
  fresh-submit boundary (e.g. inside a one-time command buffer that wraps both upload
  and build).
- **Evidence**: No current call site violates this. Documenting as an architectural pin so
  future "synchronous build_tlas" callers (e.g. an offline-render mode) don't lose the
  implicit safety.
- **Impact**: None today.
- **Suggested Fix**: Add a one-line debug-assert inside `build_tlas` that documents the
  invariant ("caller must have a queue-submit boundary between the most recent host write
  to instance_buffer and this build, OR add an explicit HOST → AS_BUILD barrier"), or
  move the explicit barrier into `build_tlas` itself so future callers can't omit it.

---

### L1: SwapchainState::destroy leaves image_views populated (#655 LIFE-M2)
- **Severity**: MEDIUM (defensive-gap; actively no double-free path today)
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/swapchain.rs:202-208`
- **Status**: Existing: #655
- **Trigger Conditions**: A future code path that calls `SwapchainState::destroy` twice on
  the same instance — e.g. a partial recreate-failure rollback that destroys the half-built
  swapchain and then drops the original on the way out.
- **Description**: `destroy` takes `&self` (not `&mut self`), so the iterator runs over the
  vec without clearing it. After this call returns, `self.image_views` still contains the
  now-stale `vk::ImageView` handles. Today this is safe because (a) the Drop path calls
  `destroy` exactly once, and (b) the resize path uses `mem::take` on the views Vec before
  rebuilding (`resize.rs:78`), bypassing `SwapchainState::destroy` entirely for the views.
- **Evidence**:
```rust
// crates/renderer/src/vulkan/swapchain.rs:202-208
pub unsafe fn destroy(&self, device: &ash::Device) {
    for &view in &self.image_views {
        device.destroy_image_view(view, None);
    }
    self.swapchain_loader
        .destroy_swapchain(self.swapchain, None);
    // self.image_views still populated with stale handles
}
```
- **Impact**: Today: none. After any refactor that adds a second `destroy` call:
  double-free of every image view → driver-level UAF.
- **Suggested Fix**:
```rust
pub unsafe fn destroy(&mut self, device: &ash::Device) {
    for &view in &self.image_views {
        device.destroy_image_view(view, None);
    }
    self.image_views.clear();
    self.swapchain_loader
        .destroy_swapchain(self.swapchain, None);
    self.swapchain = vk::SwapchainKHR::null();
}
```
  The Drop path needs no change — `&mut self` is available in `mod.rs:1670`. The resize
  path bypasses `destroy` and is unaffected.

---

### L2: SkinSlot output_buffer leaked if `allocate_descriptor_sets` fails after `create_device_local_uninit` succeeds
- **Severity**: LOW
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:316-340`
- **Status**: NEW
- **Trigger Conditions**: An NPC enters view AND `descriptor_pool` is exhausted AND
  `GpuBuffer::create_device_local_uninit` succeeds before `allocate_descriptor_sets` fails.
  Pool exhaustion requires > 32 unique skinned entities visible at once
  (`skin_compute.rs:151` caps `max_slots` at 32).
- **Description**: `create_slot` allocates `output_buffer` first (line 316-324) and only
  THEN allocates `descriptor_sets` (line 328-336). If the descriptor-set allocation fails,
  the `?` operator returns early without destroying `output_buffer`. The 6-12 KB GPU
  allocation leaks until the engine exits.
- **Evidence**:
```rust
// skin_compute.rs:316-340 (abbreviated)
let output_buffer = GpuBuffer::create_device_local_uninit(...)
    .context("allocate skin slot output buffer")?;
let layouts = [self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
let allocated = unsafe {
    device
        .allocate_descriptor_sets(...)
        .context("allocate skin slot descriptor sets")?  // ← leaks output_buffer on err
};
```
- **Impact**: One-time leak per descriptor-pool-exhaustion event. Realistically
  unreachable on the current `max_slots = 32` allocation, which is sized to cover every
  realistic interior cell.
- **Suggested Fix**: Wrap the descriptor-set allocation in a guard that destroys
  `output_buffer` on err. The `partial.destroy(...)` rollback pattern at `caustic.rs:195`
  and `caustic.rs:351` is the local precedent.

---

## Verifications (passed — INFO records for invariant pinning)

### Vulkan Sync (Dim 2)

#### V-INFO-1: Both frame-in-flight fences are waited on at frame start
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:108-120`
- **What**: `wait_for_fences(&[in_flight[frame], in_flight[prev]], ...)` — fixes the prior
  `C2-02` (SVGF reads previous frame-slot G-buffer without fence sync).
- **Cost**: Zero in practice — the GPU is rarely > 1 frame behind the CPU.

#### V-INFO-2: Host → consuming-stage barriers are emitted before every consumer
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1118-1144`
- **What**: One unified `HOST → VERTEX_SHADER | FRAGMENT_SHADER | DRAW_INDIRECT` barrier
  covers: instance SSBO, MaterialBuffer SSBO (R1), camera UBO, light UBO, bone UBO,
  indirect-draw buffer. Cluster-cull's `HOST → COMPUTE_SHADER` at `draw.rs:778-789` covers
  the same upload visibility for the cluster pass. Caustic UBO has its own
  `HOST → COMPUTE_SHADER` at `caustic.rs:725-736`. SVGF and SSAO param UBOs follow the
  same pattern.
- **Why this matters**: Fixes the prior `C2-01` (composite param UBO missing host
  barrier). The unified barrier shape is the right pattern — one barrier covers all
  per-frame uploads.

#### V-INFO-3: Skin chain three-barrier sequence is intact
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:574-668`
- **Chain**:
  1. Compute writes (skin output buffer): `cmd_dispatch` (line 587-597, per slot).
  2. `COMPUTE_SHADER → ACCELERATION_STRUCTURE_BUILD_KHR` barrier
     (`SHADER_WRITE → ACCELERATION_STRUCTURE_READ_KHR`) at line 601-612 covers the
     compute-output → BLAS-refit hand-off.
  3. Per-refit `record_scratch_serialize_barrier` (line 640) covers cross-build scratch
     reuse (BUILD → first-refit AND refit → next-refit, including the cross-submission
     BUILD case from #644 / MEM-2-2).
  4. `ACCELERATION_STRUCTURE_BUILD_KHR → ACCELERATION_STRUCTURE_BUILD_KHR` barrier
     (`AS_WRITE → AS_READ`) at line 657-668 covers BLAS-refit → TLAS-build.
  5. TLAS-build → fragment+compute ray-query barrier at line 743-755 covers the final
     hand-off into rasterization (fragment shader's `rayQueryEXT`) and caustic compute
     (also a `rayQueryEXT` consumer).
- **Why this matters**: Anyone tightening the BLAS scratch reuse logic must keep the
  cross-submission BUILD-then-refit case intact. The comment at `draw.rs:614-634` spells
  out why the redundant first-iteration barrier is essential.

#### V-INFO-4: Per-frame-in-flight history slot ping-pong is correct for SVGF / TAA
- **Location**: `crates/renderer/src/vulkan/svgf.rs::dispatch`, `taa.rs::dispatch`
- **What**: Each pass binds slot `prev = (frame + 1) % MAX_FRAMES_IN_FLIGHT` for the
  reprojection read and slot `frame` for the write. The fence wait on `in_flight[prev]`
  at `draw.rs:113` guarantees the prev slot's previous use is complete before the read.
- **Why this matters**: The original `C2-02` design intent (RAW hazard prevention) is
  preserved.

### Resource Lifecycle (Dim 3)

#### L-INFO-1: VulkanContext::Drop reverse-order destruction is correct
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1556-1723`
- **Order**: `device_wait_idle` → screenshot staging → frame_sync → transfer_fence →
  command pools → main framebuffers → texture_registry → scene_buffers → **skin_slots →
  accel_manager (incl. skinned_blas drain) → cluster_cull → skin_compute** → ssao →
  composite → caustic → svgf → taa → gbuffer → depth → rasterization pipelines →
  pipeline_layout → mesh_registry → pipeline_cache → render_pass → swapchain →
  allocator → device → surface → debug_messenger → instance.
- **Why this matters**: The skin_slots → accel ordering at lines 1593-1609 is critical —
  slots own descriptor sets allocated from skin_compute's pool, AND skinned BLAS
  reference the slot's output buffer (via `vkGetBufferDeviceAddress`).

#### L-INFO-2: Swapchain recreate destruction order is pinned by a static source-order test
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:480-538`
- **What**: Unit test parses resize.rs and asserts byte-offset ordering of four
  landmarks. Catches any future refactor that breaks the #654 / LIFE-M1 fix.
- **Why this matters**: Static-text tests are the right pattern for tightly-ordered
  Vulkan destruction sequences with no runtime test path. Don't remove this test without
  replacing it with a Vulkan-validation-layer integration test.

#### L-INFO-3: Per-pass `recreate_on_resize` paired with post-recreate `initialize_layouts`
- **Locations**:
  - SVGF: `resize.rs:298-317`
  - TAA: `resize.rs:401-417`
  - Caustic: `resize.rs:340-363`
  - GBuffer: `resize.rs:257-273`
  - SSAO: `resize.rs:213-253`
  - Composite: `resize.rs:374-389`
- **Why this matters**: Every per-frame-in-flight history image is rebuilt with
  `initial_layout = UNDEFINED` and immediately transitioned to GENERAL via a one-time
  command buffer. **This is what closes #677 (DEN-9) — verify and close that issue.**
  The `C3-01/C3-02/C3-03` partial-allocation-leak findings from
  `AUDIT_CONCURRENCY_2026-04-12.md` are still standing as LOW issues but are not active
  bugs (allocation failure during resize is itself rare).

#### L-INFO-4: SkinSlot LRU eviction predicate is well-tested
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:96-120, 477-560`
- **What**: `should_evict_skin_slot` is unit-tested for the active-this-frame, at-or-above-
  threshold, never-dispatched-sentinel, and underflow-on-future-last-used cases.
- **Why this matters**: The eviction predicate is the cleanup hand-off between draw.rs
  (decides who to evict) and the skin_compute / acceleration manager destroy paths.
  Reuses the LRU pattern from `evict_unused_blas` for non-skinned BLAS.

---

## Cross-cuts

- **Dim 5 (Compute → AS → Fragment Chains)** was not in this pass's scope but the chain
  barriers documented in V-INFO-3 are the load-bearing piece. Dim 5 should focus on
  per-frame skinned-mesh count scaling, not barrier correctness.
- **Dim 4 (Thread Safety)** covered in `AUDIT_CONCURRENCY_2026-05-05.md`. The deferred-
  destroy queues at `texture_registry` / `mesh_registry` / `acceleration_manager` (all
  `tick_deferred_destroy` callers at `draw.rs:177-185`) are the production safe-destroy
  path; they're correctly scheduled AFTER `wait_for_fences`.

## Priority Fix Order

1. **#677** — verify and close. The `initialize_layouts` re-call at `resize.rs:312-316,
   358-362, 413-417` covers the original concern. Optional follow-up: add a test that
   would have caught the original bug (a static source-order test like #654's would
   work).
2. **#655 / L1** (MEDIUM): convert `SwapchainState::destroy` to take `&mut self` and
   clear the views vec after destruction. Five-line change, no semantic risk.
3. **L2** (LOW): rollback guard around the `output_buffer` allocation in
   `SkinComputePipeline::create_slot`. Twelve-line change.
4. **V1** (LOW): debug-assert in `build_tlas` documenting the implicit submit-boundary
   host-write barrier, or move the explicit barrier into `build_tlas`.
5. **#661** — clarify whether this tracks a sync2 upgrade (LOW priority) or claims a
   real sync1 bug (it doesn't — sync1 is correct as written).

No CRITICAL or HIGH findings in either dimension. Sync and lifecycle layers are in good
shape post the M29 / M37.5 / R1 / #654 / #665 work.

---

*Suggested next step:* `/audit-publish docs/audits/AUDIT_CONCURRENCY_2026-05-06.md`
