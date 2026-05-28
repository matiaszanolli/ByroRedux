//! Built-in engine resources.

use super::components::ItemInstanceId;
use super::resource::Resource;
use std::num::NonZeroU32;

/// System names stored as a resource for debug and console queries.
pub struct SystemList(pub Vec<String>);
impl Resource for SystemList {}

/// Snapshot of [`crate::ecs::scheduler::AccessReport`] captured at
/// scheduler-build time and stored as a resource so console commands
/// (`sys.accesses`) can read the per-stage declared-access map without
/// a live `Scheduler` reference. R7.
pub struct SchedulerAccessReport(pub crate::ecs::scheduler::AccessReport);
impl Resource for SchedulerAccessReport {}

/// Bridge for requesting screenshots from the renderer.
/// Atomically claim ownership via [`ScreenshotBridge::try_claim`]; the
/// renderer captures the next frame and places PNG bytes in `result`.
/// [`ScreenshotBridge::take_result_for`] consumes the bytes only when
/// the calling consumer matches the in-flight owner.
///
/// Two consumers exist by design: the CLI `--screenshot path.png`
/// deadline loop and the debug-server `DebugRequest::Screenshot`
/// handler. Pre-#1006 both could fire concurrently and race on the
/// single result slot — last drainer won the PNG, the other reported
/// "timed out" and the user's path argument was silently ignored.
/// The owner-tagged API rejects the second caller with `false` from
/// `try_claim`; the rejected consumer surfaces a "screenshot in
/// progress (claimed by ...)" error to its user.
pub struct ScreenshotBridge {
    pub requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub result: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
    /// Atomic owner tag — `SCREENSHOT_OWNER_NONE` / `_CLI` / `_DEBUG_SERVER`.
    /// CAS'd from NONE → owner by `try_claim`, reset to NONE by
    /// successful `take_result_for` or by `cancel`.
    pub owner: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

/// `ScreenshotBridge` is idle — neither CLI nor debug-server holds it.
pub const SCREENSHOT_OWNER_NONE: u8 = 0;
/// CLI `--screenshot` deadline loop owns the in-flight request.
pub const SCREENSHOT_OWNER_CLI: u8 = 1;
/// Debug-server `DebugRequest::Screenshot` owns the in-flight request.
pub const SCREENSHOT_OWNER_DEBUG_SERVER: u8 = 2;

impl ScreenshotBridge {
    /// Set `requested = true` without owner-gating. Kept for
    /// renderer-internal use (the staging-copy poll loop on the
    /// device side which doesn't care about consumer identity).
    /// **New CLI / debug-server code should use [`try_claim`]
    /// instead** so the two consumers can't collide. See #1006.
    pub fn request(&self) {
        self.requested
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Atomically claim the bridge for `owner` and set `requested = true`.
    /// Returns `true` on successful claim, `false` when another owner
    /// (or the same owner re-claiming a still-in-flight request)
    /// already holds the bridge.
    ///
    /// `owner` must be `SCREENSHOT_OWNER_CLI` or
    /// `SCREENSHOT_OWNER_DEBUG_SERVER` — passing `_NONE` is a logic
    /// error.
    pub fn try_claim(&self, owner: u8) -> bool {
        debug_assert!(
            owner == SCREENSHOT_OWNER_CLI || owner == SCREENSHOT_OWNER_DEBUG_SERVER,
            "ScreenshotBridge::try_claim must be called with CLI or DEBUG_SERVER, not NONE"
        );
        if self
            .owner
            .compare_exchange(
                SCREENSHOT_OWNER_NONE,
                owner,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return false;
        }
        self.requested
            .store(true, std::sync::atomic::Ordering::Release);
        true
    }

    /// Read the current in-flight owner (CLI / DebugServer / None).
    /// Useful for surfacing a precise rejection message when
    /// `try_claim` fails.
    pub fn current_owner(&self) -> u8 {
        self.owner.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn take_result(&self) -> Option<Vec<u8>> {
        // #1174 — recover from poison. The state inside the mutex
        // (`Option<Vec<u8>>`) is a plain value with no invariants that
        // a panicking writer could have left half-formed; treating a
        // poisoned bridge as "PNG-encode failed → result empty" is the
        // correct contract.
        self.result.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// Owner-gated result drain. Returns `Some(bytes)` only when the
    /// in-flight owner matches `owner` AND a result is available.
    /// On a successful take, atomically resets the owner to
    /// `SCREENSHOT_OWNER_NONE` so the next request can claim a fresh
    /// bridge. Mismatched owners get `None` even if bytes exist —
    /// the bytes stay queued for their rightful claimant. See #1006.
    pub fn take_result_for(&self, owner: u8) -> Option<Vec<u8>> {
        if self.owner.load(std::sync::atomic::Ordering::Acquire) != owner {
            return None;
        }
        // #1174 — see `take_result` for poison-recovery rationale.
        let bytes = self.result.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        // Release the bridge for the next consumer.
        self.owner
            .store(SCREENSHOT_OWNER_NONE, std::sync::atomic::Ordering::Release);
        Some(bytes)
    }

    /// Cancel a pending request and discard any straggler result bytes.
    ///
    /// Pre-#1011, a `DebugDrainSystem` timeout cleared
    /// `pending_screenshot = None` but left `requested = true` if the
    /// renderer hadn't yet observed it (renderer paused, swapchain
    /// recreate). The renderer would later drain the request and write
    /// a result that nobody was waiting for — the bytes sat in
    /// `result.lock()` until the *next* screenshot request claimed
    /// them, leaking a stale PNG into the wrong response.
    ///
    /// Returns `true` when state was actually mutated (request was in
    /// flight or result was buffered). The boolean is informational —
    /// callers don't need to branch on it.
    pub fn cancel(&self) -> bool {
        let had_request = self
            .requested
            .swap(false, std::sync::atomic::Ordering::AcqRel);
        // #1174 — see `take_result` for poison-recovery rationale.
        let had_result = self
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some();
        // #1006 — release ownership so the next consumer can claim.
        self.owner
            .store(SCREENSHOT_OWNER_NONE, std::sync::atomic::Ordering::Release);
        had_request || had_result
    }
}

impl Resource for ScreenshotBridge {}

/// Per-frame delta time in seconds.
pub struct DeltaTime(pub f32);
impl Resource for DeltaTime {}

/// Accumulated wall-clock time since engine start, in seconds.
pub struct TotalTime(pub f32);
impl Resource for TotalTime {}

/// Global engine configuration.
pub struct EngineConfig {
    pub vsync: bool,
    pub target_fps: Option<u32>,
    pub debug_logging: bool,
}

impl Resource for EngineConfig {}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            target_fps: None,
            debug_logging: cfg!(debug_assertions),
        }
    }
}

const FRAME_HISTORY_SIZE: usize = 128;

/// Per-frame engine diagnostics, updated by the main loop.
///
/// Contains a rolling window of frame times for FPS calculation,
/// plus counts of entities, meshes, textures, and draw calls.
pub struct DebugStats {
    /// Current instantaneous FPS (1.0 / last dt).
    pub fps: f32,
    /// Current frame time in milliseconds.
    pub frame_time_ms: f32,
    /// Rolling buffer of frame times in seconds.
    frame_times: [f32; FRAME_HISTORY_SIZE],
    /// Write cursor into frame_times.
    frame_index: usize,
    /// Number of frames recorded (saturates at FRAME_HISTORY_SIZE).
    frame_count: usize,
    /// Total entities in the world.
    pub entity_count: u32,
    /// GPU meshes in the MeshRegistry. **Registry-wide** — never
    /// drops on cell unload, so a leak that holds the last reference
    /// to a mesh keeps this counter inflated. Pair with
    /// [`Self::meshes_in_use`] to spot the leak: `mesh_count >
    /// meshes_in_use` is the count of meshes the registry is retaining
    /// for no active consumer.
    pub mesh_count: u32,
    /// GPU textures in the TextureRegistry. **Registry-wide** — same
    /// caveats as [`Self::mesh_count`]. Pair with
    /// [`Self::textures_in_use`]. See #637 / FNV-D5-02.
    pub texture_count: u32,
    /// Distinct non-zero `MeshHandle` values reachable from live ECS
    /// entities at the last `stats_system` tick. Scene-scoped: drops
    /// the moment a cell unload removes the last entity holding the
    /// handle, so a regression that keeps a mesh in the registry
    /// after unload shows up as `mesh_count > meshes_in_use`. See
    /// #637 / FNV-D5-02.
    pub meshes_in_use: u32,
    /// Distinct non-zero `TextureHandle` values reachable from live
    /// ECS entities at the last `stats_system` tick. Same scene-scoped
    /// semantics as [`Self::meshes_in_use`]. See #637 / FNV-D5-02.
    pub textures_in_use: u32,
    /// `DrawCommand` count input to the batch merger last frame
    /// (== `app.draw_commands.len()`). The pre-batch input — NOT the
    /// GPU call count. Renamed from `draw_call_count` in #1258 /
    /// PERF-D3-NEW-03 to fix a longstanding mislabel: the field was
    /// surfaced as "Draws" by the `stats` command and the bench
    /// summary, but stored the input-to-batcher count, which led
    /// every perf audit's "~N µs/draw" arithmetic to use the wrong
    /// denominator. Paired with `batch_count` + `indirect_call_count`
    /// below for full pipeline visibility.
    pub draw_command_count: u32,
    /// Post-merge `DrawBatch` count from the main raster pass last
    /// frame (== `VulkanContext::last_draw_call_stats.batch_count`).
    /// Upper bound on the actual GPU draw call count;
    /// `cmd_draw_indexed_indirect` further compresses runs of
    /// compatible batches into a single call (see
    /// `indirect_call_count`). Dedup ratio = `draw_command_count /
    /// batch_count` is what tells you whether the batcher is
    /// collapsing repeated meshes. #1258 / PERF-D3-NEW-03.
    pub batch_count: u32,
    /// Actual number of `cmd_draw_indexed` + `cmd_draw_indexed_indirect`
    /// invocations recorded into last frame's main raster pass
    /// (== `VulkanContext::last_draw_call_stats.indirect_call_count`).
    /// Includes the two-sided alpha-blend split (which emits 2 direct
    /// draws per batch) and excludes the water / sky / UI / composite
    /// passes (O(1) per frame each). Indirect grouping ratio =
    /// `batch_count / indirect_call_count`. This is the "Draws" number
    /// the user actually wants when asking "how expensive is the
    /// frame?" — the real GPU call count. #1258 / PERF-D3-NEW-03.
    pub indirect_call_count: u32,
    /// `SkinSlotPool` live-slot count last frame (== entities currently
    /// allocated a bone-palette slot, excluding the reserved slot 0).
    /// Mirrored from `App::skin_slot_pool` each frame because the pool
    /// itself lives on the App struct rather than the ECS world.
    /// #1284.
    pub skin_pool_live: u32,
    /// `SkinSlotPool` capacity ceiling
    /// (`(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1`). Pinned via
    /// `SKIN_MAX_SLOTS` to the bone-palette architectural ceiling so
    /// the descriptor pool never silently becomes the dominant cap.
    /// #1284.
    pub skin_pool_max: u32,
    /// `SkinSlotPool` cumulative spill count — how many `allocate()`
    /// calls returned `None` since engine start. `0` is the healthy
    /// state; any non-zero value means at least one entity is rendering
    /// in bind pose for lack of a slot. Drives the cap-sizing feedback
    /// loop on #1284: capture this from the `audit-runtime` baseline to
    /// know how far the next bump needs to go.
    pub skin_pool_overflow_attempts: u32,
}

impl Resource for DebugStats {}

impl Default for DebugStats {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_time_ms: 0.0,
            frame_times: [0.0; FRAME_HISTORY_SIZE],
            frame_index: 0,
            frame_count: 0,
            entity_count: 0,
            mesh_count: 0,
            texture_count: 0,
            meshes_in_use: 0,
            textures_in_use: 0,
            draw_command_count: 0,
            batch_count: 0,
            indirect_call_count: 0,
            skin_pool_live: 0,
            skin_pool_max: 0,
            skin_pool_overflow_attempts: 0,
        }
    }
}

impl DebugStats {
    /// Record a frame's delta time and update FPS.
    pub fn push_frame_time(&mut self, dt: f32) {
        self.frame_times[self.frame_index] = dt;
        self.frame_index = (self.frame_index + 1) % FRAME_HISTORY_SIZE;
        if self.frame_count < FRAME_HISTORY_SIZE {
            self.frame_count += 1;
        }
        self.frame_time_ms = dt * 1000.0;
        self.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    }

    /// Average FPS over the rolling window.
    pub fn avg_fps(&self) -> f32 {
        if self.frame_count == 0 {
            return 0.0;
        }
        let sum: f32 = self.frame_times[..self.frame_count].iter().sum();
        let avg_dt = sum / self.frame_count as f32;
        if avg_dt > 0.0 {
            1.0 / avg_dt
        } else {
            0.0
        }
    }

    /// Min and max frame times (seconds) over the rolling window.
    pub fn min_max_frame_time(&self) -> (f32, f32) {
        if self.frame_count == 0 {
            return (0.0, 0.0);
        }
        let slice = &self.frame_times[..self.frame_count];
        let min = slice.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = slice.iter().cloned().fold(0.0f32, f32::max);
        (min, max)
    }

    /// Current write index (useful for throttling title bar updates).
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }
}

/// One row of renderer-side scratch-buffer telemetry: name, current
/// `len`, current `capacity`, and the size in bytes of one element so
/// consumers can compute the heap footprint without knowing the
/// renderer's element types.
///
/// Used by R6 (ROADMAP) to catch unbounded `Vec` growth in the
/// renderer's per-frame scratch buffers, particularly across M40 cell
/// streaming where the high-water mark would otherwise grow silently.
#[derive(Debug, Clone, Copy)]
pub struct ScratchRow {
    pub name: &'static str,
    pub len: usize,
    pub capacity: usize,
    pub elem_size_bytes: usize,
}

impl ScratchRow {
    /// Heap footprint of the buffer at its current `capacity` (not `len`).
    pub fn bytes_used(&self) -> usize {
        self.capacity.saturating_mul(self.elem_size_bytes)
    }

    /// Bytes of headroom — `capacity - len` × element size. Sustained
    /// non-zero values across many frames mean the high-water mark
    /// drifted up at some point and never came back down.
    pub fn wasted_bytes(&self) -> usize {
        self.capacity
            .saturating_sub(self.len)
            .saturating_mul(self.elem_size_bytes)
    }
}

/// Snapshot of every renderer-side persistent `Vec` scratch's capacity.
///
/// Refreshed each frame by the engine binary (after `Scheduler::run`,
/// alongside `mesh_count` / `texture_count` on `DebugStats`). Read by
/// the `ctx.scratch` console command.
///
/// `rows` is a reused `Vec` — stabilises at the count of registered
/// scratches (5 today) after the first frame, and is the *only*
/// per-frame heap allocation in the telemetry path. Bounded by the
/// number of declared scratches at the call site
/// (`VulkanContext::fill_scratch_telemetry`), so it cannot itself
/// exhibit the unbounded-growth pattern this resource is designed
/// to catch.
#[derive(Debug, Default)]
pub struct ScratchTelemetry {
    pub rows: Vec<ScratchRow>,
    /// R1 / #780 — unique materials at end of last `build_render_data`
    /// (== `MaterialTable::len()`). Pairs with `materials_interned` to
    /// compute the dedup ratio.
    pub materials_unique: usize,
    /// R1 / #780 — total `intern()` calls during last
    /// `build_render_data` (== `MaterialTable::interned_count()`,
    /// one tick per emitted `DrawCommand`). Dedup ratio =
    /// `materials_interned / materials_unique` — how many intern calls
    /// each unique material absorbs (higher = better dedup). Displayed by
    /// the `mat.stats` console command. A drop here flags a regression
    /// (alignment hole, non-deterministic float in the producer) that
    /// breaks byte-equality dedup before VRAM pressure shows it.
    /// (#1066 / REN-D14-NEW-06 — corrected from the prior inverted formula)
    pub materials_interned: usize,
    /// Number of `intern()` calls routed to id 0 (the neutral-default
    /// fallback) because the table hit `MAX_MATERIALS`. Mirrors
    /// `MaterialTable::overflow_count()`. Zero in the common case;
    /// non-zero is the signal that the cap should be raised — see
    /// `MAX_MATERIALS` in `crates/renderer/src/vulkan/scene_buffer/
    /// constants.rs`. Surfaced by the `mem` console command.
    pub materials_overflow: usize,
}

impl Resource for ScratchTelemetry {}

impl ScratchTelemetry {
    pub fn total_bytes(&self) -> usize {
        self.rows.iter().map(ScratchRow::bytes_used).sum()
    }

    pub fn total_wasted(&self) -> usize {
        self.rows.iter().map(ScratchRow::wasted_bytes).sum()
    }
}

/// Per-frame skinned-mesh BLAS coverage telemetry.
///
/// Refreshed each frame by the engine binary via
/// `VulkanContext::fill_skin_coverage_stats`, alongside
/// [`ScratchTelemetry`]. Surfaced by the `skin.coverage` console
/// command.
///
/// Closes the M29.3 / "skinned BLAS coverage" observability gap: the
/// per-skinned-entity pre-skin + BLAS refit path is wired, but several
/// silent skips can drop a visible skinned entity from this frame's
/// refit — slot-pool exhaustion (`failed_skin_slots`), first-sight
/// prime/BUILD failure, missing `MeshHandle`. This resource is the
/// falsifiable signal that "every visible skinned entity refit this
/// frame": `refits_succeeded == dispatches_total` is the green-bar.
#[derive(Debug, Default)]
pub struct SkinCoverageStats {
    /// Unique skinned entities in this frame's draw_commands (those with
    /// `bone_offset > 0`). Denominator for everything below.
    pub dispatches_total: u32,
    /// Entities whose compute dispatch was elided this frame because the
    /// bone palette hadn't changed since the previous dispatch. Counter
    /// landed in #1194 / PERF-DIM7-INSTR; incremented by the dispatch-
    /// dirty gate (#1195 / PERF-DIM7-01). `dispatches_total -
    /// dispatches_skipped` gives the GPU dispatch count actually issued.
    pub dispatches_skipped: u32,
    /// Entities currently holding a `SkinSlot` (gauge — not per-frame).
    pub slots_active: u32,
    /// Slot-pool capacity (`SKIN_MAX_SLOTS`). Constant after init.
    /// `slots_active` approaching this is the pressure signal.
    pub slot_pool_capacity: u32,
    /// Entities whose `create_slot` call returned an error and are
    /// suppressed until LRU eviction frees a slot. Gauge — cleared on
    /// any eviction.
    pub slots_failed: u32,
    /// First-sight entities entering the sync prime + BUILD path this
    /// frame (slot newly created OR BLAS missing/rebuild-requested).
    pub first_sight_attempted: u32,
    /// First-sight entities for which both prime and BUILD succeeded.
    pub first_sight_succeeded: u32,
    /// Per-frame compute dispatch + refit attempts (entities with a
    /// `SkinSlot` AND with an existing BLAS — i.e. past first-sight).
    pub refits_attempted: u32,
    /// Refits that returned `Ok` this frame.
    pub refits_succeeded: u32,
    /// Sample of entity IDs currently in `failed_skin_slots`. Bounded
    /// snapshot (first ~16 entries) so the resource stays cheap to copy
    /// out of the renderer each frame; the full count is in
    /// `slots_failed`.
    pub failed_entity_ids: Vec<super::storage::EntityId>,
    /// Per-pass GPU elapsed time in milliseconds (#1194 /
    /// PERF-DIM7-INSTR). One `MAX_FRAMES_IN_FLIGHT` cycle behind the
    /// frame the stats were filled on — that's the pipeline lag
    /// between writing TIMESTAMPs and reading them after the
    /// frame's fence signals. Zero when the driver lacks
    /// `timestampComputeAndGraphics` OR before the first complete
    /// pipelined cycle OR when the corresponding bracket didn't run
    /// (skin section skipped, TAA disabled, etc.).
    pub gpu_skin_dispatch_ms: f32,
    pub gpu_skin_blas_refit_ms: f32,
    pub gpu_taa_ms: f32,
    /// Main render pass (G-buffer + per-fragment RT loop) wall-clock
    /// in milliseconds. Added in debug-UI Phase 6 to surface the
    /// 540 ms / 1 FPS Sleeping-Giant-Inn pathology that hid behind
    /// the unprofiled main pass. Names stay on `SkinCoverageStats`
    /// for historical compatibility; the resource is the canonical
    /// landing pad for every per-pass GPU timer.
    pub gpu_main_render_ms: f32,
    pub gpu_tlas_build_ms: f32,
    pub gpu_cluster_cull_ms: f32,
    pub gpu_svgf_ms: f32,
    /// Phase-7 brackets — added to close the "438 ms unaccounted"
    /// gap that Phase 6's instrumentation exposed (main_render
    /// itself reads only ~35 ms on a Skyrim interior, so the
    /// bottleneck has to live in one of these five). Naming
    /// retained on `SkinCoverageStats` for historical
    /// compatibility; the resource is the canonical landing pad
    /// for every per-pass GPU timer now, not just skin-related.
    pub gpu_composite_ms: f32,
    pub gpu_ssao_ms: f32,
    pub gpu_bloom_ms: f32,
    pub gpu_caustic_splat_ms: f32,
    pub gpu_volumetrics_ms: f32,
}

/// CPU-side per-frame wall-clock breakdown — populated by the
/// binary's main loop using the `byroredux_renderer::FrameTimings`
/// struct `draw_frame` fills.
///
/// **Why this resource exists**: the GPU TIMESTAMP brackets in
/// `SkinCoverageStats` measure only work bracketed inside the main
/// command buffer. Operations that happen OUTSIDE that bracket —
/// the fence wait at the top of `draw_frame`, the `vkQueueSubmit`
/// + `vkQueuePresentKHR` block at the end, the egui set_textures
/// transfer-queue submit-and-wait — are invisible to those
/// brackets. A pathology surfaced by Phase 7 instrumentation
/// (sum of 12 GPU brackets = 78 ms vs 389 ms wall frame time = 311 ms
/// "missing") drove the addition of this resource: if `fence_wait_ms`
/// or `submit_present_ms` is large, the bottleneck is a GPU stall
/// or a present-mode block that GPU timestamps can't see.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuFrameTimings {
    /// `vkWaitForFences` at the top of `draw_frame` — CPU stall
    /// waiting for the previous frame's GPU work to complete. If
    /// this is large, the GPU is genuinely the bottleneck even
    /// when the per-pass GPU TIMESTAMPs sum to less.
    pub fence_wait_ms: f32,
    /// CPU work for the TLAS build path (instance map gather +
    /// command record). The GPU AS build runs async; this is
    /// the host-side overhead.
    pub tlas_build_ms: f32,
    /// Instance SSBO fill + upload (memcpy + indirect draws).
    /// Dominant CPU-side work per frame on dense cells.
    pub ssbo_build_ms: f32,
    /// All command-buffer recording between begin_render_pass
    /// and end_command_buffer.
    pub cmd_record_ms: f32,
    /// `vkQueueSubmit` + `vkQueuePresentKHR` — driver overhead
    /// plus any vsync / present-mode-FIFO stall. The other place
    /// "missing" GPU work hides.
    pub submit_present_ms: f32,
    /// `vkAcquireNextImageKHR` CPU stall. Falls in the gap
    /// between `fence_wait` and `cmd_record`. With FIFO present
    /// mode + a low swapchain image count this is where the
    /// compositor / vsync block lands. Phase 9 of the debug-UI
    /// plan added the bracket after a reading showed
    /// fence_wait + submit_present both trivial yet a 390 ms
    /// per-frame gap.
    pub acquire_ms: f32,
    /// Wall time between the END of one frame and the START of
    /// the next. Captures the period winit hands the thread back
    /// to the OS — Wayland frame-callback wait, compositor
    /// throttling, the ECS scheduler tick in `about_to_wait`,
    /// any event-loop sleep. If `acquire_ms` is small but this
    /// is large, the bottleneck is *outside* the engine's render
    /// path (compositor, OS, ECS systems running between
    /// frames). Phase 9.
    pub between_frames_ms: f32,
    /// Wall time in the `about_to_wait`'s pre-scheduler phase:
    /// dt update, entity-walk handle dedup (`meshes_in_use` /
    /// `textures_in_use`), DebugStats refresh, scratch +
    /// skin-coverage fill from the renderer. The entity walk
    /// alone is O(entity_count); on dense cells this can grow.
    /// Phase 10.
    pub atw_pre_ms: f32,
    /// Wall time of `Scheduler::run` — runs every registered ECS
    /// system in stage order. The biggest single block of work
    /// in `about_to_wait`; any pathology that holds the host
    /// (kira mutex, physics island fence, audio queue flush)
    /// lands here. Phase 10.
    pub atw_scheduler_ms: f32,
    /// Wall time of the post-scheduler steps in `about_to_wait`:
    /// `step_streaming` (M40 exterior streaming drain),
    /// `step_debug_loads` (debug-UI queued NIF / cell loads),
    /// `step_cell_transition` (`door.teleport` dispatch), window
    /// title update. Phase 10.
    pub atw_post_ms: f32,
    /// `render_one_frame`'s pre-draw_frame phase: egui run,
    /// build_render_data (ECS walk producing draw_commands),
    /// material table refresh, ScratchTelemetry update, UI
    /// manager (Ruffle SWF) tick + texture upload, geometry SSBO
    /// rebuild check. Phase 15.
    pub rof_pre_draw_ms: f32,
    /// Wall time of the `draw_frame` CPU call itself. Subtract
    /// the sum of `acquire_ms + fence_wait_ms + cmd_record_ms +
    /// ssbo_build_ms + tlas_build_ms + submit_present_ms` to see
    /// how much hidden host wait the GPU brackets miss
    /// (egui set_textures' internal queue submit, implicit
    /// barriers, etc.). Phase 15.
    pub rof_draw_call_ms: f32,
    /// `render_one_frame`'s post-draw_frame phase: FrameTimings
    /// fold into `CpuFrameTimings`, bench accumulator update,
    /// swapchain recreate, `last_redraw_end` stamp. Phase 15.
    pub rof_post_draw_ms: f32,
}

impl Resource for CpuFrameTimings {}

impl Resource for SkinCoverageStats {}

// ────────────────────────────────────────────────────────────────────
// M29.6 — per-entity persistent bone-palette slot pool
// ────────────────────────────────────────────────────────────────────

/// Per-entity persistent slot pool for the GPU bone-palette
/// (`bone_world` + `bind_inverses`) SSBOs.
///
/// Pre-M29.6 the `build_skinned_palettes` pass packed every skinned
/// entity into iteration-order slots — entity E could land at offset
/// 100 one frame and offset 144 the next. M29.5's GPU compute pass
/// works fine with that because both inputs (`bone_world` and
/// `bind_inverses`) get re-uploaded per frame in the same packing.
///
/// M29.6 promotes `bind_inverses` to a persistent SSBO that's only
/// written when an entity first appears. For that to work the slot ID
/// must be stable across frames for a given entity — this resource
/// owns the per-entity slot assignment.
///
/// Slot 0 is reserved for the global identity slot
/// (`build_render_data` pushes IDENTITY at `bone_world[0]` /
/// `bind_inverses[0]`); the pool's `next_slot` starts at 1 and
/// `allocate` never returns 0.
///
/// Lifecycle:
/// - First sight (entity in SkinnedMesh query but not yet in pool):
///   `allocate(entity, frame)` returns a fresh slot ID and pushes
///   `(slot, entity)` onto `pending_uploads`. The caller drains
///   `pending_uploads` after `build_skinned_palettes` and the
///   renderer schedules a one-time `bind_inverses` upload for each
///   pending slot.
/// - Steady-state: `allocate(entity, frame)` returns the existing
///   slot ID and refreshes `last_seen_frame`.
/// - Reclaim: `sweep(current_frame, min_idle)` returns slots whose
///   entities haven't been seen for `min_idle` frames; the caller can
///   then queue the slot for reuse. Slot data on the GPU is not
///   cleared — overwritten by the next `allocate`'s upload.
///
/// Capacity: `max_skinned` is set at construction (typical value is
/// `MAX_TOTAL_BONES / MAX_BONES_PER_MESH`, currently 196608 / 144 =
/// 1366 with slot 0 reserved → 1365 allocatable; see #1284); `allocate`
/// returns `None` past it. The caller is expected to warn-once and
/// fall back to bind-pose rendering for the overflowed entity. See
/// `Self::overflow_warned` (one-shot log) and `Self::overflow_attempt_count`
/// (cumulative spill telemetry surfaced via `DebugStats::skin_pool_*` and
/// the `engine::stats` `skin=L/M+S` line); see #1284 for the cap-sizing
/// feedback loop.
pub struct SkinSlotPool {
    /// Stable slot ID per entity. Values are in `1..=max_slot`; slot 0
    /// is reserved.
    entity_to_slot: std::collections::HashMap<super::storage::EntityId, u32>,
    /// Recycled slot IDs (popped LIFO so the most-recently-freed slot
    /// is reused first — cache-friendlier than FIFO on the persistent
    /// `bind_inverses` SSBO).
    free_list: Vec<u32>,
    /// Monotonic ceiling for fresh allocations. Starts at 1 (slot 0
    /// reserved). Bumped only when `free_list` is empty.
    next_slot: u32,
    /// Frame at which each entity was last seen via `mark_seen`.
    /// Drives the `sweep` reclaim.
    last_seen_frame: std::collections::HashMap<super::storage::EntityId, u64>,
    /// `(slot_id, entity)` for entities that allocated a slot this
    /// frame and still need their `bind_inverses` uploaded to the
    /// persistent SSBO. Drained by the renderer once per frame.
    pending_uploads: Vec<(u32, super::storage::EntityId)>,
    /// Capacity — the pool refuses to allocate past `next_slot >
    /// max_slot`. Set at construction; never changes.
    max_slot: u32,
    /// One-shot warn gate for the overflow path. Latched on first
    /// `allocate` failure; never reset (the warn is observability,
    /// not flow control).
    overflow_warned: bool,
    /// Per-entity hash of the last-uploaded `bone_world` slice. Set by
    /// `try_mark_pose_dirty`; absent for entities that haven't been
    /// hashed yet. #1195 / PERF-DIM7-01.
    last_pose_hash: std::collections::HashMap<super::storage::EntityId, u64>,
    /// Entities whose pose hash differs from the previous frame's
    /// (or who have never been hashed). Cleared at frame start by
    /// `clear_pose_dirty`; populated by `try_mark_pose_dirty`. Drained
    /// by the renderer to gate skin compute dispatch + skinned-BLAS
    /// refit. #1195 / PERF-DIM7-01, paired with #1196 / PERF-DIM7-02.
    pose_dirty: std::collections::HashSet<super::storage::EntityId>,
    /// Cumulative count of distinct entities that have ever attempted
    /// allocation past `max_slot`. Drives the sizing-data feedback loop
    /// on #1284: the one-shot warning tells you the cap was exceeded,
    /// but not by how much. This counter lets `audit-runtime` capture
    /// the actual demand so the next bump is sized to the data.
    overflow_attempt_count: u32,
}

impl SkinSlotPool {
    /// Construct a pool with capacity `max_skinned` (slots 1..=
    /// `max_skinned` are allocatable; slot 0 is reserved).
    pub fn new(max_skinned: u32) -> Self {
        assert!(
            max_skinned >= 1,
            "SkinSlotPool requires capacity ≥ 1; got {max_skinned}"
        );
        Self {
            entity_to_slot: std::collections::HashMap::new(),
            free_list: Vec::new(),
            next_slot: 1,
            last_seen_frame: std::collections::HashMap::new(),
            pending_uploads: Vec::new(),
            max_slot: max_skinned,
            overflow_warned: false,
            last_pose_hash: std::collections::HashMap::new(),
            pose_dirty: std::collections::HashSet::new(),
            overflow_attempt_count: 0,
        }
    }

    /// Return the slot ID assigned to `entity`, allocating a fresh
    /// slot on first sight. `Some(slot)` on success; `None` when the
    /// pool is full (logs once per session — see `overflow_warned`).
    ///
    /// Side effects:
    /// - First-sight calls push `(slot, entity)` onto `pending_uploads`.
    /// - Every call refreshes `last_seen_frame[entity] = frame`.
    pub fn allocate(
        &mut self,
        entity: super::storage::EntityId,
        frame: u64,
    ) -> Option<u32> {
        if let Some(&slot) = self.entity_to_slot.get(&entity) {
            self.last_seen_frame.insert(entity, frame);
            return Some(slot);
        }
        let slot = if let Some(reused) = self.free_list.pop() {
            reused
        } else if self.next_slot <= self.max_slot {
            let s = self.next_slot;
            self.next_slot += 1;
            s
        } else {
            self.overflow_attempt_count =
                self.overflow_attempt_count.saturating_add(1);
            if !self.overflow_warned {
                self.overflow_warned = true;
                log::warn!(
                    "SkinSlotPool exhausted at capacity {} (slot 0 reserved). \
                     Excess skinned entities silently fall back to bind pose. \
                     Bump MAX_TOTAL_BONES or implement variable-stride packing. \
                     (Subsequent spills counted silently — query \
                     `overflow_attempt_count` for total demand.)",
                    self.max_slot,
                );
            }
            return None;
        };
        self.entity_to_slot.insert(entity, slot);
        self.last_seen_frame.insert(entity, frame);
        self.pending_uploads.push((slot, entity));
        Some(slot)
    }

    /// Refresh `last_seen_frame[entity]` without changing allocation.
    /// Equivalent to `allocate` for already-resident entities; provided
    /// as a separate entry-point for paths that look up the slot
    /// without re-allocating.
    pub fn mark_seen(&mut self, entity: super::storage::EntityId, frame: u64) {
        if self.entity_to_slot.contains_key(&entity) {
            self.last_seen_frame.insert(entity, frame);
        }
    }

    /// Return the slot ID for `entity` without touching `last_seen_frame`
    /// or `pending_uploads`. Returns `None` if the entity has never
    /// been allocated. For draws that already routed through
    /// `allocate`; useful for diagnostics.
    pub fn get(&self, entity: super::storage::EntityId) -> Option<u32> {
        self.entity_to_slot.get(&entity).copied()
    }

    /// Cumulative count of `allocate` calls that returned `None` (pool
    /// was full). Drives the #1284 cap-sizing feedback loop — capture
    /// this from `audit-runtime` baselines to know how far the
    /// next `MAX_TOTAL_BONES` bump needs to go.
    pub fn overflow_attempt_count(&self) -> u32 {
        self.overflow_attempt_count
    }

    /// Number of allocatable slots currently in use (excludes slot 0).
    pub fn live_slot_count(&self) -> u32 {
        self.entity_to_slot.len() as u32
    }

    /// Configured capacity (slots 1..=`max_slot` are allocatable).
    pub fn max_slot(&self) -> u32 {
        self.max_slot
    }

    /// Drain up to `max` pending uploads, returning the list of slots
    /// whose `bind_inverses` haven't been uploaded yet. The caller
    /// (renderer) must complete the GPU upload BEFORE the next
    /// compute dispatch reads `bind_inverses_persistent[slot × MBPM
    /// ..]`. Any entries beyond `max` STAY in `pending_uploads` and
    /// will surface on the next `drain_pending` call (M29.6 hotfix
    /// #1192 / SAFE-D7-NEW-02 — the renderer's staging buffer has a
    /// fixed `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` cap; pre-
    /// hotfix the renderer silently dropped the excess, leaving the
    /// pool's `entity_to_slot` populated but the persistent SSBO
    /// untouched at those slots).
    ///
    /// Pass `usize::MAX` to drain everything (the pre-hotfix shape).
    pub fn drain_pending(&mut self, max: usize) -> Vec<(u32, super::storage::EntityId)> {
        let n = self.pending_uploads.len().min(max);
        self.pending_uploads.drain(..n).collect()
    }

    /// Sweep entities idle for ≥ `min_idle` frames; return their slot
    /// IDs to the free-list. Returns the freed slot IDs (caller can
    /// log / telemetry if desired).
    ///
    /// `current_frame` should be the renderer's `frame_counter`;
    /// `min_idle` is typically `MAX_FRAMES_IN_FLIGHT + 1` so a slot is
    /// only reclaimed after no in-flight command buffer could
    /// reference it.
    pub fn sweep(&mut self, current_frame: u64, min_idle: u64) -> Vec<u32> {
        let mut freed = Vec::new();
        // Collect doomed entities first to avoid mutating the map
        // while iterating.
        let doomed: Vec<super::storage::EntityId> = self
            .last_seen_frame
            .iter()
            .filter_map(|(entity, &last)| {
                let idle = current_frame.saturating_sub(last);
                if idle >= min_idle {
                    Some(*entity)
                } else {
                    None
                }
            })
            .collect();
        for entity in doomed {
            if let Some(slot) = self.entity_to_slot.remove(&entity) {
                self.free_list.push(slot);
                freed.push(slot);
            }
            self.last_seen_frame.remove(&entity);
            // #1195 / PERF-DIM7-01 — evicted entities must drop their
            // stale pose hash too; otherwise a recycled slot ID under
            // a new entity could collide with the prior tenant's hash
            // map entry (the keying is by EntityId so collisions are
            // impossible in practice, but dropping is the right
            // hygiene — keeps the map bounded to live entities).
            self.last_pose_hash.remove(&entity);
            self.pose_dirty.remove(&entity);
        }
        freed
    }

    /// Number of slots currently allocated to entities. Diagnostic.
    pub fn allocated_count(&self) -> usize {
        self.entity_to_slot.len()
    }

    /// Free-list depth — slots reclaimed but not yet reused.
    /// Diagnostic.
    pub fn free_list_depth(&self) -> usize {
        self.free_list.len()
    }

    /// Highest slot ID issued (or 0 if none). The skin_palette
    /// dispatch covers slots `0..=max_used_slot` × MBPM. Diagnostic
    /// and dispatch-sizing.
    pub fn max_used_slot(&self) -> u32 {
        self.next_slot.saturating_sub(1)
    }

    /// Update the per-entity pose hash; mark dirty (and return `true`)
    /// when the new hash differs from the stored one (or the entity
    /// has never been hashed). #1195 / PERF-DIM7-01.
    ///
    /// Callers compute `new_hash` over the entity's bone-matrix slice
    /// of `bone_world_out` after [`crate::ecs::components::SkinnedMesh`]
    /// pose construction in `build_skinned_palettes`. The renderer
    /// drains [`pose_dirty`](Self::pose_dirty) once per frame and
    /// uses it to gate skin compute dispatch + skinned-BLAS refit.
    ///
    /// First-sight (no prior hash) always returns `true`. Idle bone
    /// poses converge to "not dirty" on the second consecutive frame.
    pub fn try_mark_pose_dirty(
        &mut self,
        entity: super::storage::EntityId,
        new_hash: u64,
    ) -> bool {
        let dirty = match self.last_pose_hash.get(&entity) {
            Some(&old) => old != new_hash,
            None => true,
        };
        if dirty {
            self.last_pose_hash.insert(entity, new_hash);
            self.pose_dirty.insert(entity);
        }
        dirty
    }

    /// Clear the dirty set; called at the start of each frame before
    /// `build_skinned_palettes` repopulates it. Leaves
    /// [`last_pose_hash`](Self::last_pose_hash) intact so the next
    /// frame's hash comparison still has a baseline. #1195.
    pub fn clear_pose_dirty(&mut self) {
        self.pose_dirty.clear();
    }

    /// Read-only view of the per-frame dirty entity set. The renderer
    /// uses this to gate skin compute dispatch (#1195) and skinned-
    /// BLAS refit (#1196) — entities NOT in this set whose slots
    /// already have populated output + live BLAS can skip both passes.
    pub fn pose_dirty(&self) -> &std::collections::HashSet<super::storage::EntityId> {
        &self.pose_dirty
    }
}

impl Resource for SkinSlotPool {}

#[cfg(test)]
mod skin_slot_pool_tests {
    use super::SkinSlotPool;

    #[test]
    fn allocates_monotonically_then_recycles() {
        let mut pool = SkinSlotPool::new(10);
        let s1 = pool.allocate(1, 100).unwrap();
        let s2 = pool.allocate(2, 100).unwrap();
        let s3 = pool.allocate(3, 100).unwrap();
        assert_eq!((s1, s2, s3), (1, 2, 3), "monotonic from slot 1");

        // Free entity 2 by sweeping it past idle threshold.
        // Need to also refresh 1 and 3 so they survive.
        pool.mark_seen(1, 110);
        pool.mark_seen(3, 110);
        let freed = pool.sweep(110, /*min_idle=*/ 5);
        assert_eq!(freed, vec![s2], "only entity 2 was idle past threshold");

        // Next allocation should reuse slot 2.
        let s4 = pool.allocate(4, 111).unwrap();
        assert_eq!(s4, s2, "free-list LIFO reuse");
    }

    #[test]
    fn returns_none_at_max_skinned() {
        let mut pool = SkinSlotPool::new(3);
        assert_eq!(pool.allocate(1, 0), Some(1));
        assert_eq!(pool.allocate(2, 0), Some(2));
        assert_eq!(pool.allocate(3, 0), Some(3));
        assert_eq!(
            pool.allocate(4, 0),
            None,
            "fourth allocation past capacity 3 must return None"
        );
        // Subsequent overflow calls still return None and don't panic.
        assert_eq!(pool.allocate(5, 0), None);
    }

    #[test]
    fn sweep_reclaims_unseen() {
        let mut pool = SkinSlotPool::new(10);
        let slot = pool.allocate(42, 100).unwrap();
        // Frame 100 → 104, idle = 4 < min_idle 5 → keep.
        assert_eq!(pool.sweep(104, 5), Vec::<u32>::new());
        assert_eq!(pool.get(42), Some(slot));
        // Frame 105, idle = 5 ≥ min_idle 5 → evict.
        let freed = pool.sweep(105, 5);
        assert_eq!(freed, vec![slot]);
        assert_eq!(pool.get(42), None);
    }

    #[test]
    fn first_allocation_queues_pending_upload() {
        let mut pool = SkinSlotPool::new(10);
        let _ = pool.allocate(7, 100).unwrap();
        let pending = pool.drain_pending(usize::MAX);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, 7, "entity ID round-tripped");

        // Steady-state allocate for the same entity must NOT push to
        // pending_uploads (would re-upload the bind_inverses every
        // frame, defeating M29.6's purpose).
        let _ = pool.allocate(7, 101).unwrap();
        assert!(
            pool.drain_pending(usize::MAX).is_empty(),
            "steady-state allocate must not re-queue pending upload"
        );
    }

    /// M29.6 hotfix #1192 / SAFE-D7-NEW-02 — `drain_pending(max)` must
    /// return at most `max` entries and KEEP the remainder in
    /// `pending_uploads` for the next drain. Pre-hotfix `drain_pending`
    /// took everything; the renderer's staging buffer cap silently
    /// dropped the tail, leaving `entity_to_slot` populated but the
    /// persistent SSBO untouched at those slots.
    #[test]
    fn drain_pending_respects_max_cap() {
        let mut pool = SkinSlotPool::new(50);
        for i in 0..20u32 {
            let _ = pool.allocate(i, 100).unwrap();
        }
        let first = pool.drain_pending(16);
        assert_eq!(first.len(), 16, "first drain takes at most max=16");
        let second = pool.drain_pending(16);
        assert_eq!(
            second.len(),
            4,
            "second drain takes the remaining tail (20 - 16 = 4)"
        );
        assert!(
            pool.drain_pending(16).is_empty(),
            "third drain finds the queue empty"
        );
    }

    /// M29.6 hotfix #1192 — entities whose pending upload was capped
    /// must NOT be lost: their slot still maps via `entity_to_slot`,
    /// and the queue tail surfaces on the next drain. The pool's
    /// invariant is "every allocated entity surfaces in pending
    /// exactly once over its lifetime"; the renderer's cap mustn't
    /// turn that into "≤ MAX_PENDING entities over the lifetime".
    #[test]
    fn drain_pending_does_not_lose_capped_entities() {
        let mut pool = SkinSlotPool::new(50);
        let mut all_seen_slots = std::collections::HashSet::new();
        for i in 0..20u32 {
            let slot = pool.allocate(i, 100).unwrap();
            all_seen_slots.insert((slot, i));
        }
        // Drain with cap = 16; the tail (4 entries) stays in pool.
        let first_round: std::collections::HashSet<_> =
            pool.drain_pending(16).into_iter().collect();
        let second_round: std::collections::HashSet<_> =
            pool.drain_pending(usize::MAX).into_iter().collect();
        let combined: std::collections::HashSet<_> =
            first_round.union(&second_round).copied().collect();
        assert_eq!(
            combined, all_seen_slots,
            "every allocated entity must surface in some drain (#1192)"
        );
    }

    #[test]
    fn never_returns_slot_zero() {
        let mut pool = SkinSlotPool::new(5);
        for entity_id in 1..=5u32 {
            let slot = pool.allocate(entity_id, 0).unwrap();
            assert_ne!(
                slot, 0,
                "slot 0 is reserved for global identity; pool must not allocate it"
            );
        }
    }

    #[test]
    fn underflow_safe_when_last_used_in_future() {
        // Defensive: if a caller bumps last_seen_frame to a value
        // larger than current_frame (frame counter wrap / reset),
        // sweep must not flip eviction true via wrap-around.
        let mut pool = SkinSlotPool::new(5);
        let _slot = pool.allocate(99, 200).unwrap();
        // current=100, last=200, saturating_sub → idle=0 → keep.
        assert_eq!(pool.sweep(100, 5), Vec::<u32>::new());
        assert!(pool.get(99).is_some());
    }

    // ── #1195 / PERF-DIM7-01 — pose-dirty gate ────────────────────

    #[test]
    fn first_sight_pose_is_always_dirty() {
        let mut pool = SkinSlotPool::new(5);
        // Entity 1 has never been hashed → first call must return
        // dirty so the renderer dispatches at least once. The
        // "skip dispatch when unchanged" optimisation must never
        // bypass first-sight; otherwise the output buffer is never
        // populated and the BLAS holds garbage.
        let dirty = pool.try_mark_pose_dirty(1, 0x1234);
        assert!(dirty, "first hash for an entity must report dirty");
        assert!(
            pool.pose_dirty().contains(&1),
            "first-sight entity must land in the dirty set"
        );
    }

    #[test]
    fn unchanged_pose_is_not_dirty_on_second_call() {
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();

        // Same hash → not dirty; idle skinned entity skips dispatch.
        let dirty = pool.try_mark_pose_dirty(1, 0xCAFE);
        assert!(!dirty, "same hash must not report dirty");
        assert!(
            !pool.pose_dirty().contains(&1),
            "stable-pose entity must not re-enter the dirty set"
        );
    }

    #[test]
    fn changed_pose_re_dirties() {
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();

        // Hash changes (bone moved) → dirty again.
        let dirty = pool.try_mark_pose_dirty(1, 0xDEAD);
        assert!(dirty, "hash mismatch must report dirty");
        assert!(pool.pose_dirty().contains(&1));
    }

    #[test]
    fn clear_pose_dirty_preserves_baseline_hash() {
        // `clear_pose_dirty` runs at the top of every frame to empty
        // the per-frame dirty set; it must NOT wipe `last_pose_hash`,
        // because the next-frame comparison needs that baseline. If
        // the baseline were wiped, every frame would re-mark every
        // entity dirty — defeating the optimisation entirely.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();
        // Same hash now → must still report not-dirty (baseline survived).
        assert!(!pool.try_mark_pose_dirty(1, 0xCAFE));
    }

    #[test]
    fn sweep_drops_stale_pose_hash_with_slot() {
        // When an entity's slot is reclaimed by `sweep`, its pose hash
        // should be evicted too — otherwise the map grows without
        // bound across long-running sessions (NPC churn from cell
        // streaming, particle skinned-mesh churn).
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.allocate(7, 100);
        let _ = pool.try_mark_pose_dirty(7, 0xCAFE);
        // Sweep with idle threshold low enough to evict.
        let freed = pool.sweep(110, /*min_idle=*/ 5);
        assert_eq!(freed.len(), 1, "entity 7 idle past threshold → evicted");
        // After eviction, re-allocating entity 7 must hit the
        // first-sight dirty branch again — proving the stale hash
        // was dropped.
        pool.clear_pose_dirty();
        assert!(
            pool.try_mark_pose_dirty(7, 0xCAFE),
            "re-allocated entity must hit first-sight dirty after sweep"
        );
    }
}

impl SkinCoverageStats {
    /// True when every visible skinned entity got a refit this frame.
    /// Reads correct only after `fill_skin_coverage_stats` populates
    /// the snapshot; before that all counters are 0 and this returns
    /// `true` trivially.
    pub fn fully_covered(&self) -> bool {
        self.refits_succeeded == self.dispatches_total
            && self.slots_failed == 0
    }
}

/// The currently-selected entity reference for debugger operations.
///
/// Bethesda-console heritage — `prid <FormID>` picks a reference, then
/// follow-up commands (`getpos`, `inspect`, `cam.tp` w/o args, …)
/// operate on the picked ref by default. byro-dbg uses `EntityId`
/// rather than `FormID` because the renderer-side debugger talks
/// directly to the ECS; an M47-era in-game console would resolve
/// FormID → EntityId through `byroredux_plugin` and set this same
/// resource.
///
/// World-scoped state (not per-TCP-client). Single-developer-at-a-time
/// is the dev-tool reality; two clients would fight, but the simpler
/// state model is worth the tradeoff for now.
#[derive(Debug, Default)]
pub struct SelectedRef(pub Option<super::storage::EntityId>);

impl Resource for SelectedRef {}

/// Per-stack divergent state.
///
/// Allocated only when an [`ItemStack`](super::components::ItemStack)
/// can't be represented by `(base_form_id, count)` alone — modlists,
/// condition deltas, charge state, named items. Most inventory rows
/// don't need one; the stack-only common case keeps `instance` at
/// `None`.
///
/// Fields are open-ended: future equip mechanics (FO4 OMOD, weapon
/// charge, food spoilage) extend this struct rather than parallel
/// inventory types. Phase A of #896 ships it minimal; Phase B/C and
/// M45 fill in real fields.
#[derive(Debug, Default, Clone)]
pub struct ItemInstance {
    /// Reserved for now. Real fields land alongside the consuming
    /// gameplay system (M45 save round-trip; FO4 OMOD wiring).
    _reserved: (),
}

/// Sparse arena for [`ItemInstance`]s with a free-list for slot reuse.
///
/// The free-list is what prevents Bethesda's pickup-drop-pickup save-
/// bloat tail. When an `ItemInstance` is released, its slot returns
/// to `free` and the next allocation reuses it. Save format dumps
/// `instances` + `free` verbatim — bounded, not log-shaped.
///
/// Slot 0 is reserved as a sentinel so [`ItemInstanceId`] (which
/// wraps `NonZeroU32`) can encode "no instance" as the absence of
/// the option without burning a u32 niche.
#[derive(Debug)]
pub struct ItemInstancePool {
    instances: Vec<Option<ItemInstance>>,
    free: Vec<u32>,
}

impl Resource for ItemInstancePool {}

impl Default for ItemInstancePool {
    fn default() -> Self {
        // Pre-fill slot 0 as the reserved sentinel.
        Self {
            instances: vec![None],
            free: Vec::new(),
        }
    }
}

impl ItemInstancePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate an instance, reusing a freed slot if available.
    pub fn allocate(&mut self, inst: ItemInstance) -> ItemInstanceId {
        if let Some(slot) = self.free.pop() {
            self.instances[slot as usize] = Some(inst);
            ItemInstanceId(NonZeroU32::new(slot).expect("free-list never holds slot 0"))
        } else {
            let slot = self.instances.len();
            // `usize::MAX` would wrap; in practice the cap is u32::MAX
            // entries, far above any realistic save's instance count.
            assert!(slot <= u32::MAX as usize, "ItemInstancePool overflow");
            self.instances.push(Some(inst));
            ItemInstanceId(
                NonZeroU32::new(slot as u32).expect("slot >= 1 since slot 0 is reserved"),
            )
        }
    }

    /// Release a slot back to the free-list. Returns the freed
    /// instance if it was live, `None` if the slot was already free
    /// or out of bounds (defensive — duplicate-free is a logic bug
    /// elsewhere but we don't want to corrupt the arena over it).
    pub fn release(&mut self, id: ItemInstanceId) -> Option<ItemInstance> {
        let slot = id.0.get();
        let cell = self.instances.get_mut(slot as usize)?;
        let taken = cell.take()?;
        // Avoid double-pushing onto `free` if release is called twice
        // for the same id (the `cell.take` above guards the live state
        // but the free-list contract still needs deduping).
        if !self.free.contains(&slot) {
            self.free.push(slot);
        }
        Some(taken)
    }

    pub fn get(&self, id: ItemInstanceId) -> Option<&ItemInstance> {
        self.instances
            .get(id.0.get() as usize)
            .and_then(|cell| cell.as_ref())
    }

    /// Number of live instances (excludes free slots and the sentinel).
    pub fn live_count(&self) -> usize {
        self.instances
            .iter()
            .skip(1) // skip the sentinel
            .filter(|c| c.is_some())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_stats_default_is_zero() {
        let stats = DebugStats::default();
        assert_eq!(stats.fps, 0.0);
        assert_eq!(stats.avg_fps(), 0.0);
        assert_eq!(stats.min_max_frame_time(), (0.0, 0.0));
    }

    #[test]
    fn push_frame_time_updates_fps() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(1.0 / 60.0); // 60 FPS
        assert!((stats.fps - 60.0).abs() < 0.5);
        assert!((stats.frame_time_ms - 16.67).abs() < 0.1);
    }

    #[test]
    fn avg_fps_over_multiple_frames() {
        let mut stats = DebugStats::default();
        for _ in 0..10 {
            stats.push_frame_time(1.0 / 30.0); // 30 FPS
        }
        assert!((stats.avg_fps() - 30.0).abs() < 0.5);
    }

    #[test]
    fn min_max_frame_time_correct() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(0.010); // 10ms
        stats.push_frame_time(0.020); // 20ms
        stats.push_frame_time(0.005); // 5ms
        let (min, max) = stats.min_max_frame_time();
        assert!((min - 0.005).abs() < 1e-6);
        assert!((max - 0.020).abs() < 1e-6);
    }

    #[test]
    fn circular_buffer_wraps() {
        let mut stats = DebugStats::default();
        // Fill more than 128 frames
        for i in 0..200 {
            stats.push_frame_time(0.016);
            assert!(stats.frame_count <= FRAME_HISTORY_SIZE);
            if i >= FRAME_HISTORY_SIZE {
                assert_eq!(stats.frame_count, FRAME_HISTORY_SIZE);
            }
        }
        assert_eq!(stats.frame_index, 200 % FRAME_HISTORY_SIZE);
        assert!((stats.avg_fps() - 62.5).abs() < 1.0);
    }

    #[test]
    fn scratch_row_bytes_used_is_capacity_times_elem_size() {
        let row = ScratchRow {
            name: "x",
            len: 10,
            capacity: 100,
            elem_size_bytes: 32,
        };
        assert_eq!(row.bytes_used(), 100 * 32);
        assert_eq!(row.wasted_bytes(), (100 - 10) * 32);
    }

    #[test]
    fn scratch_row_wasted_is_zero_when_full() {
        let row = ScratchRow {
            name: "full",
            len: 50,
            capacity: 50,
            elem_size_bytes: 8,
        };
        assert_eq!(row.wasted_bytes(), 0);
    }

    #[test]
    fn skin_coverage_default_is_trivially_covered() {
        // Default reports zero dispatches → no work missed; this is
        // the no-skinned-content-this-frame baseline that `skin.
        // coverage` should print as "n/a".
        let cov = SkinCoverageStats::default();
        assert!(cov.fully_covered());
        assert_eq!(cov.dispatches_total, 0);
        assert_eq!(cov.refits_succeeded, 0);
    }

    #[test]
    fn skin_coverage_full_when_refits_match_dispatches() {
        let cov = SkinCoverageStats {
            dispatches_total: 23,
            refits_succeeded: 23,
            ..Default::default()
        };
        assert!(cov.fully_covered());
    }

    #[test]
    fn skin_coverage_partial_when_refits_lag() {
        // 23 visible skinned entities but only 22 refit — one was
        // dropped somewhere between dispatches collection and the refit
        // loop. This is exactly the regression mode the instrumentation
        // exists to surface.
        let cov = SkinCoverageStats {
            dispatches_total: 23,
            refits_succeeded: 22,
            ..Default::default()
        };
        assert!(!cov.fully_covered());
    }

    /// #1194 / PERF-DIM7-INSTR — `dispatches_skipped` + GPU timer
    /// fields. Pin: they default to zero and don't affect
    /// `fully_covered` (the green-bar only reads dispatch/refit
    /// counters). PERF-DIM7-01 / -02 / -03 (#1195 / #1196 / #1197)
    /// landed the consumers that increment `dispatches_skipped` and
    /// populate the GPU timer values; this test guards the fields
    /// against accidental removal from the struct.
    #[test]
    fn skin_coverage_dim7_instr_fields_default_to_zero_and_dont_break_green_bar() {
        let cov = SkinCoverageStats {
            dispatches_total: 10,
            refits_succeeded: 10,
            dispatches_skipped: 4, // some entities elided this frame
            gpu_skin_dispatch_ms: 1.234,
            gpu_skin_blas_refit_ms: 2.345,
            gpu_taa_ms: 0.456,
            ..Default::default()
        };
        assert!(
            cov.fully_covered(),
            "instrumentation fields must not gate the green-bar — \
             dispatches_skipped is a positive signal (work elided), \
             not a regression",
        );
        assert_eq!(cov.dispatches_skipped, 4);
        assert!((cov.gpu_skin_dispatch_ms - 1.234).abs() < 1e-6);
        assert!((cov.gpu_skin_blas_refit_ms - 2.345).abs() < 1e-6);
        assert!((cov.gpu_taa_ms - 0.456).abs() < 1e-6);
    }

    #[test]
    fn skin_coverage_partial_when_slots_failed_nonzero() {
        // Even if refits == dispatches arithmetically, a non-zero
        // failed-slot count means visible entities were silently
        // skipped from the dispatches stream (they never reached
        // first-sight). The green-bar must fail.
        let cov = SkinCoverageStats {
            dispatches_total: 10,
            refits_succeeded: 10,
            slots_failed: 2,
            ..Default::default()
        };
        assert!(!cov.fully_covered());
    }

    #[test]
    fn item_instance_pool_allocate_starts_at_one() {
        let mut pool = ItemInstancePool::new();
        let id = pool.allocate(ItemInstance::default());
        // Slot 0 is reserved; first allocation must land at 1.
        assert_eq!(id.0.get(), 1);
        assert_eq!(pool.live_count(), 1);
    }

    #[test]
    fn item_instance_pool_release_reclaims_slot() {
        let mut pool = ItemInstancePool::new();
        let a = pool.allocate(ItemInstance::default());
        let b = pool.allocate(ItemInstance::default());
        assert_eq!(pool.live_count(), 2);
        let released = pool.release(a);
        assert!(released.is_some());
        assert_eq!(pool.live_count(), 1);
        // Next allocation reuses `a`'s slot (LIFO from free-list).
        let c = pool.allocate(ItemInstance::default());
        assert_eq!(c, a);
        assert_eq!(pool.live_count(), 2);
        // `b` stays valid throughout.
        assert!(pool.get(b).is_some());
    }

    #[test]
    fn item_instance_pool_double_release_does_not_corrupt_free_list() {
        let mut pool = ItemInstancePool::new();
        let a = pool.allocate(ItemInstance::default());
        let _ = pool.allocate(ItemInstance::default());
        assert!(pool.release(a).is_some());
        // Second release of the same id is a no-op rather than a
        // duplicate free-list entry.
        assert!(pool.release(a).is_none());
        let c = pool.allocate(ItemInstance::default());
        // Should reuse `a`'s slot exactly once.
        assert_eq!(c, a);
        let d = pool.allocate(ItemInstance::default());
        // `d` lands at a fresh slot — would be slot 1's reuse if the
        // free-list were corrupted with a duplicate entry.
        assert_ne!(d, a);
    }

    #[test]
    fn scratch_telemetry_aggregates_rows() {
        let mut tlm = ScratchTelemetry::default();
        tlm.rows.push(ScratchRow {
            name: "a",
            len: 1,
            capacity: 10,
            elem_size_bytes: 4,
        });
        tlm.rows.push(ScratchRow {
            name: "b",
            len: 5,
            capacity: 5,
            elem_size_bytes: 16,
        });
        assert_eq!(tlm.total_bytes(), 10 * 4 + 5 * 16);
        assert_eq!(tlm.total_wasted(), (10 - 1) * 4 + 0);
    }

    /// Test fixture — fresh idle bridge.
    fn idle_bridge() -> ScreenshotBridge {
        ScreenshotBridge {
            requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            result: std::sync::Arc::new(std::sync::Mutex::new(None)),
            owner: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(SCREENSHOT_OWNER_NONE)),
        }
    }

    /// #1011 — `cancel()` must clear both the AtomicBool `requested`
    /// flag AND any buffered result bytes. Either alone would leak
    /// state into the next request.
    #[test]
    fn screenshot_bridge_cancel_clears_request_and_result() {
        use std::sync::atomic::Ordering;

        let bridge = idle_bridge();

        // Simulate a renderer that has finished a capture (result
        // present) but a still-pending follow-up request (flag set).
        bridge.request();
        *bridge.result.lock().unwrap() = Some(vec![0xDE, 0xAD]);

        assert!(bridge.cancel(), "cancel reports state mutated");
        assert!(!bridge.requested.load(Ordering::Acquire), "flag cleared");
        assert!(
            bridge.result.lock().unwrap().is_none(),
            "buffered result discarded"
        );
    }

    #[test]
    fn screenshot_bridge_cancel_is_idempotent_on_clean_state() {
        let bridge = idle_bridge();
        assert!(
            !bridge.cancel(),
            "cancel on clean state reports no mutation"
        );
    }

    #[test]
    fn screenshot_bridge_cancel_handles_request_only() {
        let bridge = idle_bridge();
        bridge.request();
        assert!(bridge.cancel());
        assert!(
            !bridge
                .requested
                .load(std::sync::atomic::Ordering::Acquire)
        );
    }

    #[test]
    fn screenshot_bridge_cancel_handles_result_only() {
        let bridge = idle_bridge();
        *bridge.result.lock().unwrap() = Some(vec![0xCA, 0xFE]);
        assert!(bridge.cancel());
        assert!(bridge.result.lock().unwrap().is_none());
    }

    /// #1006 — `try_claim` succeeds on an idle bridge and atomically
    /// sets both owner + requested.
    #[test]
    fn screenshot_bridge_try_claim_succeeds_when_idle() {
        let bridge = idle_bridge();
        assert!(bridge.try_claim(SCREENSHOT_OWNER_CLI));
        assert_eq!(bridge.current_owner(), SCREENSHOT_OWNER_CLI);
        assert!(bridge
            .requested
            .load(std::sync::atomic::Ordering::Acquire));
    }

    /// #1006 — second `try_claim` rejects when another owner holds
    /// the bridge; no mutation occurs.
    #[test]
    fn screenshot_bridge_try_claim_rejects_when_owned() {
        let bridge = idle_bridge();
        assert!(bridge.try_claim(SCREENSHOT_OWNER_CLI));
        // Different owner — must reject.
        assert!(!bridge.try_claim(SCREENSHOT_OWNER_DEBUG_SERVER));
        // Same owner — also rejects (a still-in-flight request must
        // complete its result drain before re-claiming).
        assert!(!bridge.try_claim(SCREENSHOT_OWNER_CLI));
        // Owner unchanged.
        assert_eq!(bridge.current_owner(), SCREENSHOT_OWNER_CLI);
    }

    /// #1006 — `take_result_for` only returns bytes when owner matches.
    /// On a successful take, owner resets to NONE so the next consumer
    /// can claim.
    #[test]
    fn screenshot_bridge_take_result_for_owner_gated() {
        let bridge = idle_bridge();
        bridge.try_claim(SCREENSHOT_OWNER_DEBUG_SERVER);
        *bridge.result.lock().unwrap() = Some(vec![0xBE, 0xEF]);

        // Wrong owner — bytes stay queued.
        assert!(bridge.take_result_for(SCREENSHOT_OWNER_CLI).is_none());
        assert!(
            bridge.result.lock().unwrap().is_some(),
            "bytes not consumed by wrong owner"
        );
        assert_eq!(bridge.current_owner(), SCREENSHOT_OWNER_DEBUG_SERVER);

        // Correct owner — bytes drain AND owner resets.
        let bytes = bridge.take_result_for(SCREENSHOT_OWNER_DEBUG_SERVER);
        assert_eq!(bytes, Some(vec![0xBE, 0xEF]));
        assert_eq!(
            bridge.current_owner(),
            SCREENSHOT_OWNER_NONE,
            "successful take releases ownership"
        );
    }

    /// #1006 — after a successful drain, the bridge is idle and the
    /// other consumer can claim it.
    #[test]
    fn screenshot_bridge_handoff_cli_to_debug_server() {
        let bridge = idle_bridge();
        bridge.try_claim(SCREENSHOT_OWNER_CLI);
        *bridge.result.lock().unwrap() = Some(vec![0x01]);
        assert!(bridge.take_result_for(SCREENSHOT_OWNER_CLI).is_some());
        // Debug-server can now claim.
        assert!(bridge.try_claim(SCREENSHOT_OWNER_DEBUG_SERVER));
    }

    /// #1006 — `cancel()` also resets ownership so the bridge is fully
    /// reusable after a timeout cleanup.
    #[test]
    fn screenshot_bridge_cancel_resets_owner() {
        let bridge = idle_bridge();
        bridge.try_claim(SCREENSHOT_OWNER_DEBUG_SERVER);
        bridge.cancel();
        assert_eq!(bridge.current_owner(), SCREENSHOT_OWNER_NONE);
        // Either consumer can claim a fresh bridge.
        assert!(bridge.try_claim(SCREENSHOT_OWNER_CLI));
    }

    /// #1174 — a panic in the encode path (or any other writer) leaves
    /// the `result` mutex poisoned. Public API must recover transparently
    /// so the bridge isn't a one-shot process-killer.
    #[test]
    fn screenshot_bridge_recovers_from_poisoned_mutex() {
        use std::sync::Arc;

        let bridge = Arc::new(idle_bridge());
        // Seed a result so `take_result` has something to find.
        *bridge.result.lock().unwrap() = Some(vec![0x42]);

        // Poison the mutex by panicking inside a lock guard on a
        // helper thread, then joining the resulting Err.
        let poisoner = Arc::clone(&bridge);
        let res = std::thread::spawn(move || {
            let _guard = poisoner.result.lock().unwrap();
            panic!("synthetic encode failure");
        })
        .join();
        assert!(res.is_err(), "helper thread should have panicked");
        assert!(
            bridge.result.is_poisoned(),
            "mutex should be poisoned after the panic"
        );

        // Every public accessor must succeed despite the poison.
        let bytes = bridge.take_result();
        assert_eq!(bytes, Some(vec![0x42]), "take_result recovers state");

        // `cancel()` and `take_result_for` also recover; exercise both.
        *bridge.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(vec![0x99]);
        bridge.try_claim(SCREENSHOT_OWNER_CLI);
        let claimed = bridge.take_result_for(SCREENSHOT_OWNER_CLI);
        assert_eq!(claimed, Some(vec![0x99]));

        *bridge.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(vec![0xAA]);
        bridge.request();
        assert!(bridge.cancel(), "cancel recovers + reports mutated state");
    }
}
