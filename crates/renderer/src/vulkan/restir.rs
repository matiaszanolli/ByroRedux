//! ReSTIR-DI direct-shadow reservoir buffers (Bitterli et al. 2020).
//!
//! Owns the screen-sized, per-frame-in-flight reservoir SSBOs the fragment
//! shader reads (previous frame) and writes (current frame) to reuse
//! direct soft-shadow samples temporally — fixing the un-denoised
//! per-frame WRS shadow crawl. One [`Reservoir`] per screen pixel, indexed
//! `pixelY * screenWidth + pixelX` (matching `triangle.frag`).
//!
//! This holder follows the **screen-sized-resource-in-set-1 precedent**
//! already used for the SSAO texture (binding 7) and depth-history
//! (binding 15): the buffer memory lives here (with its own
//! recreate-on-resize), and is *written into* the scene descriptor set
//! (set 1, bindings 16 = curr, 17 = prev) via
//! [`super::scene_buffer::SceneBuffers::write_reservoir_buffers`]. The
//! scene descriptor set itself stays content-sized and is not recreated on
//! resize — only the reservoir descriptor writes are refreshed.
//!
//! ## Ping-pong
//!
//! With `MAX_FRAMES_IN_FLIGHT == 2`, frame N writes slot N and reads slot
//! `(N+1) % 2` as its temporal history — identical to the SVGF history
//! scheme. The per-frame fence guarantees slot `(N+1)%2` was fully written
//! two frames ago before the new read.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::Result;
use ash::vk;

/// Bytes per [`Reservoir`] — must match the `struct Reservoir` std430 layout
/// in `shaders/include/bindings.glsl` (8 × 4-byte scalars). Pinned by
/// [`tests::reservoir_stride_matches_shader`].
pub const RESERVOIR_STRIDE: vk::DeviceSize = 32;

/// Screen-sized, per-frame-in-flight reservoir storage buffers.
pub struct ReservoirBuffers {
    /// One GPU-only STORAGE buffer per frame-in-flight, sized
    /// `width * height * RESERVOIR_STRIDE`.
    buffers: Vec<GpuBuffer>,
    width: u32,
    height: u32,
}

impl ReservoirBuffers {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                Self::byte_size(width, height),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }
        // PERF-D5-NEW-04 / #1814 — this is the largest single VRAM
        // addition of the Session 49 denoiser overhaul (~127 MB at
        // 1080p, ~236 MB at 1440p, ~531 MB at 4K across both FIF
        // slots) and had no attributing telemetry anywhere; see
        // docs/engine/memory-budget.md's "ReSTIR Reservoirs" section.
        log::info!(
            "ReSTIR reservoir buffers created: {}x{}, {:.1} MB/slot × {} slots = {:.1} MB total",
            width,
            height,
            Self::byte_size(width, height) as f64 / (1024.0 * 1024.0),
            MAX_FRAMES_IN_FLIGHT,
            (Self::byte_size(width, height) * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize) as f64
                / (1024.0 * 1024.0),
        );
        Ok(Self {
            buffers,
            width,
            height,
        })
    }

    fn byte_size(width: u32, height: u32) -> vk::DeviceSize {
        (width as vk::DeviceSize) * (height as vk::DeviceSize) * RESERVOIR_STRIDE
    }

    /// The reservoir buffer this frame writes (set-1 binding 16).
    pub fn curr_buffer(&self, frame: usize) -> vk::Buffer {
        self.buffers[frame].buffer
    }

    /// The reservoir buffer this frame reads as temporal history (binding 17):
    /// the other frame-in-flight slot, mirroring the SVGF ping-pong.
    pub fn prev_buffer(&self, frame: usize) -> vk::Buffer {
        self.buffers[(frame + 1) % MAX_FRAMES_IN_FLIGHT].buffer
    }

    /// Byte size of one reservoir buffer (for the descriptor range).
    pub fn buffer_size(&self) -> vk::DeviceSize {
        Self::byte_size(self.width, self.height)
    }

    /// Recreate at a new extent after a swapchain resize. History is
    /// meaningless across a resize; the temporal pass's first-frame reset
    /// is owned by the shader's packed surface-ID + normal check, and stale
    /// reservoir contents cannot pass both validations, so no explicit clear
    /// is needed.
    pub fn recreate_on_resize(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        width: u32,
        height: u32,
    ) -> Result<()> {
        // SAFETY: called from the fenced swapchain-resize path
        // (`recreate_swapchain` waits both frames-in-flight first), so no
        // in-flight command references these buffers.
        for mut buf in self.buffers.drain(..) {
            buf.destroy(device, allocator);
        }
        self.width = width;
        self.height = height;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.buffers.push(GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                Self::byte_size(width, height),
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }
        // PERF-D5-NEW-04 / #1814 — same telemetry as `new()`; a resize
        // is exactly when this footprint changes, so it's the other
        // point where under-tracked VRAM growth would otherwise go
        // unnoticed.
        log::info!(
            "ReSTIR reservoir buffers recreated: {}x{}, {:.1} MB/slot × {} slots = {:.1} MB total",
            width,
            height,
            Self::byte_size(width, height) as f64 / (1024.0 * 1024.0),
            MAX_FRAMES_IN_FLIGHT,
            (Self::byte_size(width, height) * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize) as f64
                / (1024.0 * 1024.0),
        );
        Ok(())
    }

    /// Destroy all reservoir buffers.
    ///
    /// # Safety
    /// Caller must ensure no in-flight command buffer references these
    /// buffers (device idle or both frames-in-flight fenced).
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for mut buf in self.buffers.drain(..) {
            buf.destroy(device, allocator);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rust-side stride must match the std430 `struct Reservoir` in
    /// `bindings.glsl` (8 scalars × 4 bytes). If the shader struct grows,
    /// this and the GLSL must move together.
    #[test]
    fn reservoir_stride_matches_shader() {
        // lightAndSurface + W + M + histLen + accumR + accumG + accumB
        // + pad0 (packed geometric normal). 8 scalars × 4 bytes.
        assert_eq!(RESERVOIR_STRIDE, 8 * 4);
    }

    #[test]
    fn temporal_reuse_validates_surface_and_does_not_seed_neighbor_radiance() {
        let src = include_str!("../../shaders/triangle.frag");
        assert!(
            src.contains("rpSurfaceId == surfaceId")
                && src.contains("dot(geomN, rpGeomN) >= TEMPORAL_NORMAL_COS"),
            "ReSTIR temporal history must validate surface identity and normal"
        );
        assert!(
            !src.contains("SPATIAL_SEED_HIST") && !src.contains("spatColSum"),
            "spatial reuse may borrow light candidates, never old surface radiance"
        );
        assert!(
            src.contains("bool cameraStatic = dofParams.w > 0.5")
                && src.contains("cameraStatic ? 64.0 : 16.0")
                && src.contains("cameraStatic ? 0.025 : 0.1"),
            "direct-light history must converge when parked and remain responsive in motion"
        );
    }

    /// Ping-pong: curr/prev are always different slots at
    /// MAX_FRAMES_IN_FLIGHT == 2 (else temporal history aliases the write).
    #[test]
    fn ping_pong_slots_differ() {
        assert!(MAX_FRAMES_IN_FLIGHT >= 2);
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            assert_ne!(f, (f + 1) % MAX_FRAMES_IN_FLIGHT);
        }
    }

    /// PERF-D5-NEW-04 / #1814 — pins the exact per-resolution byte totals
    /// documented in docs/engine/memory-budget.md's "ReSTIR Reservoirs"
    /// section against the live `RESERVOIR_STRIDE` / `MAX_FRAMES_IN_FLIGHT`
    /// constants. If either constant changes, this fails as the nudge to
    /// update the doc's table alongside the code (the whole point of the
    /// fix — the doc had silently drifted out of existence before #1814).
    #[test]
    fn byte_size_matches_documented_memory_budget_figures() {
        // (width, height, expected per-slot MB, expected 2-FIF total MB)
        // MB here is decimal (bytes / 1_000_000), matching every other
        // entry in memory-budget.md.
        let cases = [
            (1920u32, 1080u32, 66.4, 132.7),
            (2560, 1440, 118.0, 235.9),
            (3840, 2160, 265.4, 530.8),
        ];
        for (w, h, expected_per_slot_mb, expected_total_mb) in cases {
            let per_slot = ReservoirBuffers::byte_size(w, h) as f64 / 1_000_000.0;
            let total = per_slot * MAX_FRAMES_IN_FLIGHT as f64;
            assert!(
                (per_slot - expected_per_slot_mb).abs() < 0.1,
                "{w}x{h}: per-slot {per_slot:.1} MB != documented {expected_per_slot_mb} MB"
            );
            assert!(
                (total - expected_total_mb).abs() < 0.1,
                "{w}x{h}: total {total:.1} MB != documented {expected_total_mb} MB"
            );
        }
    }
}
