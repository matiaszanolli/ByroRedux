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
    /// is owned by the shader's reproject bounds check, and stale reservoir
    /// contents are harmless (the final visibility ray re-validates every
    /// shaded sample), so no explicit clear is needed.
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
        // lightIndex + W + M + histLen + accumR + accumG + accumB
        // + pad0 (packed geometric normal). 8 scalars × 4 bytes.
        assert_eq!(RESERVOIR_STRIDE, 8 * 4);
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
}
