//! Per-pass GPU timer (#1194 / PERF-DIM7-INSTR + debug-UI Phase 6).
//!
//! Bracketing GPU hot spots with `vkCmdWriteTimestamp` so per-pass
//! cost can be measured rather than guessed. Owns one `VkQueryPool`
//! per frame-in-flight slot, 14 TIMESTAMP queries each:
//!
//! | Slot | Bracket                                |
//! |------|----------------------------------------|
//! | 0    | skin compute dispatch loop — start     |
//! | 1    | skin compute dispatch loop — end       |
//! | 2    | skinned BLAS refit loop — start        |
//! | 3    | skinned BLAS refit loop — end          |
//! | 4    | TAA compute dispatch — start           |
//! | 5    | TAA compute dispatch — end             |
//! | 6    | main render pass — start               |
//! | 7    | main render pass — end                 |
//! | 8    | TLAS build / refit — start             |
//! | 9    | TLAS build / refit — end               |
//! | 10   | cluster light culling dispatch — start |
//! | 11   | cluster light culling dispatch — end   |
//! | 12   | SVGF temporal dispatch — start         |
//! | 13   | SVGF temporal dispatch — end           |
//! | 14   | composite pass — start                 |
//! | 15   | composite pass — end                   |
//! | 16   | SSAO compute dispatch — start          |
//! | 17   | SSAO compute dispatch — end            |
//! | 18   | bloom pyramid (full) — start           |
//! | 19   | bloom pyramid (full) — end             |
//! | 20   | caustic splat compute — start          |
//! | 21   | caustic splat compute — end            |
//! | 22   | volumetrics inject+integrate — start   |
//! | 23   | volumetrics inject+integrate — end     |
//!
//! The original three brackets (skin / BLAS refit / TAA) shipped
//! with the #1194 perf-bisect work. The four added in debug-UI
//! Phase 6 (main render / TLAS / cluster cull / SVGF) and the five
//! added in Phase 7 (composite / SSAO / bloom / caustic splat /
//! volumetrics) close the remaining "438 ms unaccounted" gap that
//! Phase 6's instrumentation surfaced — `main_render` was only
//! 35 ms, so the bottleneck has to live in one of the five
//! Phase-7-bracketed passes.
//!
//! ## Lifecycle
//!
//! Created once per `VulkanContext` (one pool per frame slot). At the
//! top of `draw_frame` (after the per-frame fence wait — which proves
//! the *previous* use of this slot is complete), we read the slot's
//! results into `last_snapshot`, then `cmd_reset_query_pool` for the
//! upcoming frame. The fence ordering means results are always one
//! `MAX_FRAMES_IN_FLIGHT` cycle behind the current frame, but for a
//! perf instrumentation that's exactly the right amount of lag: the
//! console / bench summary reads a steady value, not a value that's
//! still mid-GPU.
//!
//! ## When a bracket doesn't fire
//!
//! Some frames skip the skin chain entirely (no skinned draws, no RT)
//! or skip TAA (disabled). On those frames the bracket timestamps are
//! never written; the prior frame's result stays valid (no flicker),
//! and the caller flags the bracket as inactive via the
//! `*_ran_this_frame` bits supplied alongside the snapshot. Consumers
//! should pair the elapsed-ms field with the bit.
//!
//! ## When the driver lacks timestamp support
//!
//! `DeviceCapabilities::timestamp_supported == false` skips creation
//! entirely; `last_snapshot()` returns zeroed values. Vanishingly
//! rare on desktop GPUs (the spec mandates support on any device
//! that exposes `VK_KHR_acceleration_structure`, which is our RT
//! gate) but the path stays sound.

use anyhow::{Context, Result};
use ash::vk;

use super::sync::MAX_FRAMES_IN_FLIGHT;

/// One TIMESTAMP query per bracket endpoint × twelve brackets.
const QUERIES_PER_FRAME: u32 = 24;

const Q_SKIN_DISPATCH_START: u32 = 0;
const Q_SKIN_DISPATCH_END: u32 = 1;
const Q_BLAS_REFIT_START: u32 = 2;
const Q_BLAS_REFIT_END: u32 = 3;
const Q_TAA_START: u32 = 4;
const Q_TAA_END: u32 = 5;
const Q_MAIN_RENDER_START: u32 = 6;
const Q_MAIN_RENDER_END: u32 = 7;
const Q_TLAS_BUILD_START: u32 = 8;
const Q_TLAS_BUILD_END: u32 = 9;
const Q_CLUSTER_CULL_START: u32 = 10;
const Q_CLUSTER_CULL_END: u32 = 11;
const Q_SVGF_START: u32 = 12;
const Q_SVGF_END: u32 = 13;
const Q_COMPOSITE_START: u32 = 14;
const Q_COMPOSITE_END: u32 = 15;
const Q_SSAO_START: u32 = 16;
const Q_SSAO_END: u32 = 17;
const Q_BLOOM_START: u32 = 18;
const Q_BLOOM_END: u32 = 19;
const Q_CAUSTIC_SPLAT_START: u32 = 20;
const Q_CAUSTIC_SPLAT_END: u32 = 21;
const Q_VOLUMETRICS_START: u32 = 22;
const Q_VOLUMETRICS_END: u32 = 23;

/// Per-pass elapsed GPU time, milliseconds. Reads `0.0` for any
/// bracket that didn't run on the snapshot frame OR before the
/// first complete pipelined cycle.
#[derive(Debug, Default, Clone, Copy)]
pub struct GpuTimerSnapshot {
    pub skin_dispatch_ms: f32,
    pub skin_blas_refit_ms: f32,
    pub taa_ms: f32,
    /// Wall-clock time for the main geometry render pass —
    /// G-buffer fill + per-fragment RT loop (shadow rays, GI ray,
    /// metal reflection, glass IOR). Expected dominant cost on
    /// interior cells with cluster-light count near `MAX_LIGHTS_PER_CLUSTER`.
    pub main_render_ms: f32,
    /// TLAS build / refit time. First-cell-load frames spike (full
    /// BUILD); steady-state should report an UPDATE-mode refit
    /// in the sub-millisecond range.
    pub tlas_build_ms: f32,
    /// Cluster light culling compute dispatch — frustum-vs-light
    /// intersection across the 16×9×24 cluster grid.
    pub cluster_cull_ms: f32,
    /// SVGF temporal accumulation compute dispatch — motion-vector
    /// reprojection of last frame's denoised indirect.
    pub svgf_ms: f32,
    /// Composite pass — fullscreen fragment shader combining HDR +
    /// SVGF indirect + albedo + bloom + caustic + volumetrics into
    /// the swapchain image with ACES tone-mapping. Phase-7 bracket.
    pub composite_ms: f32,
    /// SSAO compute — 16 samples per pixel, full-screen. Phase-7.
    pub ssao_ms: f32,
    /// Bloom pyramid (downsample + upsample chain combined) compute.
    /// Phase-7.
    pub bloom_ms: f32,
    /// Caustic splat compute — RT-traced per-refractive-pixel
    /// caustic accumulator. Phase-7.
    pub caustic_splat_ms: f32,
    /// Volumetrics inject + integrate combined. Reads `0.0` when
    /// `VOLUMETRIC_OUTPUT_CONSUMED` is false (the current default —
    /// composite multiplies the result by 0). Phase-7 bracket
    /// confirms the gate is actually holding.
    pub volumetrics_ms: f32,
}

/// Per-frame-in-flight TIMESTAMP query pools.
pub struct GpuPerFrameTimers {
    pools: [vk::QueryPool; MAX_FRAMES_IN_FLIGHT],
    /// Ticks → milliseconds multiplier
    /// (`timestamp_period_ns * 1e-6`).
    ticks_to_ms: f32,
    /// Per-frame "was this bracket's pair written?" — set by the
    /// END writer, cleared on reset. Slot index matches the frame
    /// slot the pool reads from. Each u16 packs `BIT_*` flags
    /// (one per bracket — currently 7). The bit-gated read in
    /// `read_and_reset` is required because WAIT-reading an
    /// unwritten query blocks forever.
    active_bits: [u16; MAX_FRAMES_IN_FLIGHT],
    /// Stash for the previous frame's snapshot. Console / bench
    /// consumers read this; the writer pipeline updates it at the
    /// top of `draw_frame`.
    last_snapshot: GpuTimerSnapshot,
}

const BIT_SKIN_DISPATCH: u16 = 0x0001;
const BIT_BLAS_REFIT: u16 = 0x0002;
const BIT_TAA: u16 = 0x0004;
const BIT_MAIN_RENDER: u16 = 0x0008;
const BIT_TLAS_BUILD: u16 = 0x0010;
const BIT_CLUSTER_CULL: u16 = 0x0020;
const BIT_SVGF: u16 = 0x0040;
const BIT_COMPOSITE: u16 = 0x0080;
const BIT_SSAO: u16 = 0x0100;
const BIT_BLOOM: u16 = 0x0200;
const BIT_CAUSTIC_SPLAT: u16 = 0x0400;
const BIT_VOLUMETRICS: u16 = 0x0800;

impl GpuPerFrameTimers {
    /// Create one TIMESTAMP query pool per frame-in-flight slot.
    /// Returns `Ok(None)` when the driver lacks
    /// `timestampComputeAndGraphics` — caller continues without
    /// instrumentation.
    pub fn new(
        device: &ash::Device,
        caps: &super::super::vulkan::device::DeviceCapabilities,
    ) -> Result<Option<Self>> {
        if !caps.timestamp_supported {
            return Ok(None);
        }
        let mut pools = [vk::QueryPool::null(); MAX_FRAMES_IN_FLIGHT];
        for (i, slot) in pools.iter_mut().enumerate() {
            let info = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(QUERIES_PER_FRAME);
            // SAFETY: `info` is correctly populated; `device` is valid and
            // outlives this pool (destroyed in `Self::destroy` before device).
            *slot = unsafe {
                device
                    .create_query_pool(&info, None)
                    .with_context(|| format!("create TIMESTAMP query pool slot {i}"))?
            };
            // Spec mandates reset before first use. cmd_reset is
            // gated to draw_frame; host-side
            // `device.reset_query_pool` (VK_KHR_host_query_reset)
            // works pre-cmd and avoids needing a dedicated init
            // submit. The extension is core in Vulkan 1.2.
            // SAFETY: pool was just created; no GPU work references it yet.
            // `reset_query_pool` is host-side (no command buffer required).
            unsafe {
                device.reset_query_pool(*slot, 0, QUERIES_PER_FRAME);
            }
        }
        Ok(Some(Self {
            pools,
            ticks_to_ms: caps.timestamp_period_ns * 1.0e-6,
            active_bits: [0; MAX_FRAMES_IN_FLIGHT],
            last_snapshot: GpuTimerSnapshot::default(),
        }))
    }

    /// Read the slot's previous results, refresh `last_snapshot`,
    /// then reset the slot for the upcoming frame's writes.
    ///
    /// Must be called AFTER this slot's fence has been waited on
    /// (so the previous cycle's GPU work is complete and timestamps
    /// are available) and BEFORE recording any new commands into
    /// the per-frame command buffer.
    ///
    /// The first time a slot is read its `active_bits` are zero —
    /// nothing has been written yet — so all three ms fields stay
    /// at the default `0.0` until the second cycle. From then on
    /// the snapshot is whatever the previous cycle wrote, with
    /// inactive brackets reading `0.0`.
    pub fn read_and_reset(&mut self, device: &ash::Device, frame: usize) {
        let pool = self.pools[frame];
        let bits = self.active_bits[frame];
        // Read brackets individually. WAIT-reading the entire 6-query
        // pool when only a subset was written blocks forever on the
        // unwritten queries (Vulkan spec: VK_QUERY_RESULT_WAIT_BIT
        // blocks until ALL queried results are available; reset-but-
        // never-written queries never become available). The
        // `active_bits` gate captures which START/END pairs were
        // actually written; bracketed reads keep WAIT correct
        // because the fence preceding `read_and_reset` proves any
        // emitted timestamp has retired.
        let mut snap = GpuTimerSnapshot::default();
        if bits & BIT_SKIN_DISPATCH != 0 {
            snap.skin_dispatch_ms =
                Self::read_bracket(device, pool, Q_SKIN_DISPATCH_START, self.ticks_to_ms);
        }
        if bits & BIT_BLAS_REFIT != 0 {
            snap.skin_blas_refit_ms =
                Self::read_bracket(device, pool, Q_BLAS_REFIT_START, self.ticks_to_ms);
        }
        if bits & BIT_TAA != 0 {
            snap.taa_ms = Self::read_bracket(device, pool, Q_TAA_START, self.ticks_to_ms);
        }
        if bits & BIT_MAIN_RENDER != 0 {
            snap.main_render_ms =
                Self::read_bracket(device, pool, Q_MAIN_RENDER_START, self.ticks_to_ms);
        }
        if bits & BIT_TLAS_BUILD != 0 {
            snap.tlas_build_ms =
                Self::read_bracket(device, pool, Q_TLAS_BUILD_START, self.ticks_to_ms);
        }
        if bits & BIT_CLUSTER_CULL != 0 {
            snap.cluster_cull_ms =
                Self::read_bracket(device, pool, Q_CLUSTER_CULL_START, self.ticks_to_ms);
        }
        if bits & BIT_SVGF != 0 {
            snap.svgf_ms = Self::read_bracket(device, pool, Q_SVGF_START, self.ticks_to_ms);
        }
        if bits & BIT_COMPOSITE != 0 {
            snap.composite_ms =
                Self::read_bracket(device, pool, Q_COMPOSITE_START, self.ticks_to_ms);
        }
        if bits & BIT_SSAO != 0 {
            snap.ssao_ms = Self::read_bracket(device, pool, Q_SSAO_START, self.ticks_to_ms);
        }
        if bits & BIT_BLOOM != 0 {
            snap.bloom_ms = Self::read_bracket(device, pool, Q_BLOOM_START, self.ticks_to_ms);
        }
        if bits & BIT_CAUSTIC_SPLAT != 0 {
            snap.caustic_splat_ms =
                Self::read_bracket(device, pool, Q_CAUSTIC_SPLAT_START, self.ticks_to_ms);
        }
        if bits & BIT_VOLUMETRICS != 0 {
            snap.volumetrics_ms =
                Self::read_bracket(device, pool, Q_VOLUMETRICS_START, self.ticks_to_ms);
        }
        self.last_snapshot = snap;

        // Reset the slot for the upcoming frame's writes.
        // SAFETY: the fence preceding `read_and_reset` guarantees all GPU work for
        // this slot has retired; `reset_query_pool` is host-side (no cmd buffer).
        unsafe {
            device.reset_query_pool(pool, 0, QUERIES_PER_FRAME);
        }
        self.active_bits[frame] = 0;
    }

    /// Read one bracket (2 consecutive queries starting at
    /// `start_query`) and return its elapsed time in milliseconds.
    /// WAIT-safe because the caller only invokes this when both
    /// queries were written (gated by `active_bits`).
    fn read_bracket(
        device: &ash::Device,
        pool: vk::QueryPool,
        start_query: u32,
        ticks_to_ms: f32,
    ) -> f32 {
        let mut ticks = [0u64; 2];
        // SAFETY: caller gates on the active bit, which was set by
        // the END writer. END can only fire after START in the same
        // command buffer (paired by construction in `cmd_*_start` /
        // `cmd_*_end`). The fence preceding `read_and_reset` ensures
        // both timestamps have retired; WAIT is therefore a no-op
        // on the host but kept for spec compliance.
        let result = unsafe {
            device.get_query_pool_results(
                pool,
                start_query,
                &mut ticks,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        };
        match result {
            Ok(()) => ticks[1].saturating_sub(ticks[0]) as f32 * ticks_to_ms,
            Err(e) => {
                log::warn!("GPU TIMESTAMP read failed (bracket @{start_query}): {e}");
                0.0
            }
        }
    }

    /// Last snapshot read by [`Self::read_and_reset`]. Zero-defaulted
    /// until the second pipelined cycle completes.
    pub fn last_snapshot(&self) -> GpuTimerSnapshot {
        self.last_snapshot
    }

    /// Write the skin-dispatch START timestamp. Caller must pair
    /// with `cmd_skin_dispatch_end` in the same command buffer.
    pub fn cmd_skin_dispatch_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_SKIN_DISPATCH_START,
            );
        }
    }

    /// Write the skin-dispatch END timestamp.
    pub fn cmd_skin_dispatch_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_SKIN_DISPATCH_END,
            );
        }
        self.active_bits[frame] |= BIT_SKIN_DISPATCH;
    }

    /// Write the BLAS-refit START timestamp.
    pub fn cmd_blas_refit_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_BLAS_REFIT_START,
            );
        }
    }

    /// Write the BLAS-refit END timestamp.
    pub fn cmd_blas_refit_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                self.pools[frame],
                Q_BLAS_REFIT_END,
            );
        }
        self.active_bits[frame] |= BIT_BLAS_REFIT;
    }

    /// Write the TAA-dispatch START timestamp.
    pub fn cmd_taa_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_TAA_START,
            );
        }
    }

    /// Write the TAA-dispatch END timestamp.
    pub fn cmd_taa_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_TAA_END,
            );
        }
        self.active_bits[frame] |= BIT_TAA;
    }

    /// Write the main-render-pass START timestamp. Caller writes
    /// this immediately before `cmd_begin_render_pass`; the END
    /// goes right after `cmd_end_render_pass`. `TOP_OF_PIPE` on
    /// start so the timestamp captures the moment work for the
    /// pass is queued, not when prior compute work finishes.
    pub fn cmd_main_render_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_MAIN_RENDER_START,
            );
        }
    }

    /// Write the main-render-pass END timestamp. `BOTTOM_OF_PIPE`
    /// on end so the timestamp waits for the last fragment shader
    /// and color-attachment write to retire (the actual cost the
    /// bracket is measuring).
    pub fn cmd_main_render_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.pools[frame],
                Q_MAIN_RENDER_END,
            );
        }
        self.active_bits[frame] |= BIT_MAIN_RENDER;
    }

    /// Write the TLAS-build / refit START timestamp.
    pub fn cmd_tlas_build_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_TLAS_BUILD_START,
            );
        }
    }

    /// Write the TLAS-build / refit END timestamp. End-stage is
    /// `ACCELERATION_STRUCTURE_BUILD_KHR` so the bracket waits
    /// for the actual AS-build pipeline stage to retire.
    pub fn cmd_tlas_build_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                self.pools[frame],
                Q_TLAS_BUILD_END,
            );
        }
        self.active_bits[frame] |= BIT_TLAS_BUILD;
    }

    /// Write the cluster-cull-dispatch START timestamp.
    pub fn cmd_cluster_cull_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_CLUSTER_CULL_START,
            );
        }
    }

    /// Write the cluster-cull-dispatch END timestamp.
    pub fn cmd_cluster_cull_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_CLUSTER_CULL_END,
            );
        }
        self.active_bits[frame] |= BIT_CLUSTER_CULL;
    }

    /// Write the SVGF-dispatch START timestamp.
    pub fn cmd_svgf_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_SVGF_START,
            );
        }
    }

    /// Write the SVGF-dispatch END timestamp.
    pub fn cmd_svgf_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_SVGF_END,
            );
        }
        self.active_bits[frame] |= BIT_SVGF;
    }

    // ── Phase-7 brackets (closing the 438ms gap) ─────────────────

    pub fn cmd_composite_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_COMPOSITE_START,
            );
        }
    }

    /// Composite END uses BOTTOM_OF_PIPE so the timestamp waits for
    /// the fragment shader + color-attachment-write to retire (the
    /// actual cost of the fullscreen pass).
    pub fn cmd_composite_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.pools[frame],
                Q_COMPOSITE_END,
            );
        }
        self.active_bits[frame] |= BIT_COMPOSITE;
    }

    pub fn cmd_ssao_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_SSAO_START,
            );
        }
    }

    pub fn cmd_ssao_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_SSAO_END,
            );
        }
        self.active_bits[frame] |= BIT_SSAO;
    }

    pub fn cmd_bloom_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_BLOOM_START,
            );
        }
    }

    pub fn cmd_bloom_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_BLOOM_END,
            );
        }
        self.active_bits[frame] |= BIT_BLOOM;
    }

    pub fn cmd_caustic_splat_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_CAUSTIC_SPLAT_START,
            );
        }
    }

    pub fn cmd_caustic_splat_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_CAUSTIC_SPLAT_END,
            );
        }
        self.active_bits[frame] |= BIT_CAUSTIC_SPLAT;
    }

    pub fn cmd_volumetrics_start(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                self.pools[frame],
                Q_VOLUMETRICS_START,
            );
        }
    }

    pub fn cmd_volumetrics_end(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        // SAFETY: `cmd` is recording; pool is live; slot is within QUERIES_PER_FRAME.
        unsafe {
            device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                self.pools[frame],
                Q_VOLUMETRICS_END,
            );
        }
        self.active_bits[frame] |= BIT_VOLUMETRICS;
    }

    /// Destroy every query pool. Caller must wait for queue idle
    /// before calling (matches the rest of VulkanContext's Drop
    /// ordering — query pools share the destroy-before-device
    /// invariant of every other VkObject).
    pub fn destroy(&mut self, device: &ash::Device) {
        for pool in self.pools.iter_mut() {
            if *pool != vk::QueryPool::null() {
                // SAFETY: caller ensures queue idle before `destroy()`; the
                // null-check above guards against double-free.
                unsafe {
                    device.destroy_query_pool(*pool, None);
                }
                *pool = vk::QueryPool::null();
            }
        }
    }
}
