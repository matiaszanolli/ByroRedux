//! Per-pass GPU timer (#1194 / PERF-DIM7-INSTR).
//!
//! Bracketing three skin-chain / TAA hot spots with `vkCmdWriteTimestamp`
//! so PERF-DIM7-01 / -02 / -03 (#1195 / #1196 / #1197) can be measured
//! rather than guessed. Owns one `VkQueryPool` per frame-in-flight slot,
//! 6 TIMESTAMP queries each:
//!
//! | Slot | Bracket                                |
//! |------|----------------------------------------|
//! | 0    | skin compute dispatch loop — start     |
//! | 1    | skin compute dispatch loop — end       |
//! | 2    | skinned BLAS refit loop — start        |
//! | 3    | skinned BLAS refit loop — end          |
//! | 4    | TAA compute dispatch — start           |
//! | 5    | TAA compute dispatch — end             |
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

/// One TIMESTAMP query per bracket endpoint × three brackets.
const QUERIES_PER_FRAME: u32 = 6;

const Q_SKIN_DISPATCH_START: u32 = 0;
const Q_SKIN_DISPATCH_END: u32 = 1;
const Q_BLAS_REFIT_START: u32 = 2;
const Q_BLAS_REFIT_END: u32 = 3;
const Q_TAA_START: u32 = 4;
const Q_TAA_END: u32 = 5;

/// Per-pass elapsed GPU time, milliseconds. Reads `0.0` for any
/// bracket that didn't run on the snapshot frame OR before the
/// first complete pipelined cycle.
#[derive(Debug, Default, Clone, Copy)]
pub struct GpuTimerSnapshot {
    pub skin_dispatch_ms: f32,
    pub skin_blas_refit_ms: f32,
    pub taa_ms: f32,
}

/// Per-frame-in-flight TIMESTAMP query pools.
pub struct GpuPerFrameTimers {
    pools: [vk::QueryPool; MAX_FRAMES_IN_FLIGHT],
    /// Ticks → milliseconds multiplier
    /// (`timestamp_period_ns * 1e-6`).
    ticks_to_ms: f32,
    /// Per-frame "was this bracket's pair written?" — set by the
    /// caller at `mark_bracket_active`, cleared on reset. Slot index
    /// matches the frame slot the pool reads from. Each u8 packs:
    /// bit 0 = skin dispatch, bit 1 = blas refit, bit 2 = TAA.
    active_bits: [u8; MAX_FRAMES_IN_FLIGHT],
    /// Stash for the previous frame's snapshot. Console / bench
    /// consumers read this; the writer pipeline updates it at the
    /// top of `draw_frame`.
    last_snapshot: GpuTimerSnapshot,
}

const BIT_SKIN_DISPATCH: u8 = 0x01;
const BIT_BLAS_REFIT: u8 = 0x02;
const BIT_TAA: u8 = 0x04;

impl GpuPerFrameTimers {
    /// Create one TIMESTAMP query pool per frame-in-flight slot.
    /// Returns `Ok(None)` when the driver lacks
    /// `timestampComputeAndGraphics` — caller continues without
    /// instrumentation.
    pub fn new(device: &ash::Device, caps: &super::super::vulkan::device::DeviceCapabilities) -> Result<Option<Self>> {
        if !caps.timestamp_supported {
            return Ok(None);
        }
        let mut pools = [vk::QueryPool::null(); MAX_FRAMES_IN_FLIGHT];
        for (i, slot) in pools.iter_mut().enumerate() {
            let info = vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(QUERIES_PER_FRAME);
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
        self.last_snapshot = snap;

        // Reset the slot for the upcoming frame's writes.
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
    pub fn cmd_skin_dispatch_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
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
    pub fn cmd_skin_dispatch_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
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
    pub fn cmd_blas_refit_start(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
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
    pub fn cmd_blas_refit_end(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
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

    /// Destroy every query pool. Caller must wait for queue idle
    /// before calling (matches the rest of VulkanContext's Drop
    /// ordering — query pools share the destroy-before-device
    /// invariant of every other VkObject).
    pub fn destroy(&mut self, device: &ash::Device) {
        for pool in self.pools.iter_mut() {
            if *pool != vk::QueryPool::null() {
                unsafe {
                    device.destroy_query_pool(*pool, None);
                }
                *pool = vk::QueryPool::null();
            }
        }
    }
}
