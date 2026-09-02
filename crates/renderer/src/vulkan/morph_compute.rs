//! Per-entity morph-target GPU resources (#3231).
//!
//! Deliberately additive/isolated from [`super::skin_compute::SkinSlot`]
//! rather than a new field on it: `SkinSlot` is lazily created during
//! the per-frame skin-dispatch chain and carries a delicate set of
//! invariants (LRU eviction, descriptor-binding cache, first-sight
//! dispatch gating, remap-safety checks) that took several dedicated
//! bug-fix passes to get right (#1195, #1196, #1197, #2402, #2403,
//! #2743). `MorphSlot` needs none of that machinery — it holds no
//! descriptor sets at all (both buffers are read in shaders purely via
//! `buffer_reference` device addresses cached at creation time, never
//! rebound), and its data is only known at NIF-import/mesh-spawn time,
//! not at draw-dispatch time — so it is created once at spawn (see
//! `byroredux::cell_loader::spawn::mesh_instance`) rather than lazily
//! on first dispatch. Keeping it a fully separate resource means a bug
//! here cannot corrupt `SkinSlot`'s own state.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::GpuUploadCtx;
use anyhow::{Context, Result};
use ash::vk;

/// Per-entity morph-target GPU state. One per skinned entity whose mesh
/// carries `ImportedMesh.morph_targets`. Both buffers are dereferenced
/// in-shader via `buffer_reference` (`MorphDeltaRef` / `MorphWeightRef`
/// in `skin_vertices.comp` / `triangle.vert` / `include/bindings.glsl`)
/// — there are no descriptor sets to manage, unlike `SkinSlot`.
pub struct MorphSlot {
    /// Static per-target-per-vertex position deltas, uploaded ONCE at
    /// creation from the owning mesh's `ImportedMesh.morph_targets`.
    /// Layout: `target_count` groups of `vertex_count` `vec4` (xyz
    /// delta, w unused padding — matches `MorphDeltaRef`'s std430
    /// `vec4 data[]`; a tightly-packed `vec3` array would misalign
    /// against std430's 16-byte vec3 rounding, the exact bug #3231
    /// already hit once on `GpuInstance`'s padding tail — see that
    /// struct's doc for the post-mortem). Never re-uploaded — morph
    /// targets are a static property of the mesh; only the weights
    /// animate.
    delta_buffer: GpuBuffer,
    delta_address: vk::DeviceAddress,
    /// Current morph weights, one `f32` per target. Small (≤
    /// `MAX_MORPH_TARGETS_PER_MESH` × 4 B = 256 B) and host-visible.
    /// Writes occur only from [`Self::flush_pending_weights`], after
    /// `draw_frame` has waited both in-flight fences (#3244). Zero-initialized
    /// at creation so a
    /// first-sight frame (before the animation system's first write)
    /// reads "no deformation" rather than uninitialised memory.
    weight_buffer: GpuBuffer,
    weight_address: vk::DeviceAddress,
    /// CPU-side handoff populated before `draw_frame`. Staging here performs
    /// no mapped-memory access, so it is safe while an earlier submission is
    /// still reading `weight_buffer`.
    pending_weights: Vec<f32>,
    pending_weights_dirty: bool,
    target_count: u32,
    vertex_count: u32,
    /// LRU bookkeeping, same shape as `SkinSlot::last_used_frame` —
    /// bumped every frame this entity appears in `draw_commands`
    /// (including skip-path entries), read by the per-frame eviction
    /// sweep. `0` means "never touched this frame yet" (sentinel,
    /// mirrors `SkinSlot`).
    pub last_used_frame: u64,
}

impl MorphSlot {
    /// Number of vertices this slot's delta buffer was sized for. Used
    /// the same way `SkinSlot::vertex_count()` is — the draw-time
    /// publish site must verify this equals the LIVE mesh's vertex
    /// count before handing `delta_address` to the shader (a remapped
    /// entity reusing a stale slot sized for a different mesh would
    /// otherwise let the shader read/index past the buffer's real
    /// extent through a raw device address with no range check).
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub fn target_count(&self) -> u32 {
        self.target_count
    }

    pub fn delta_address(&self) -> vk::DeviceAddress {
        self.delta_address
    }

    pub fn weight_address(&self) -> vk::DeviceAddress {
        self.weight_address
    }

    /// Create a new slot: uploads `deltas` once (target-major flat
    /// array, `target_count * vertex_count` entries — caller's
    /// responsibility to have already converted `ImportedMorphTarget`
    /// data into this shape), allocates a zero-initialized weight
    /// buffer sized for `target_count` floats, and queries both
    /// buffers' device addresses (cached — never re-queried).
    ///
    /// `deltas.len()` MUST equal `target_count as usize * vertex_count
    /// as usize` — a debug assertion catches a caller mismatch rather
    /// than silently uploading a short/long buffer.
    pub fn create(
        ctx: GpuUploadCtx,
        deltas: &[[f32; 4]],
        target_count: u32,
        vertex_count: u32,
    ) -> Result<Self> {
        debug_assert_eq!(
            deltas.len(),
            target_count as usize * vertex_count as usize,
            "MorphSlot::create: deltas length must be target_count * vertex_count"
        );
        let device = ctx.device;
        let allocator = ctx.allocator;

        let delta_buffer = GpuBuffer::create_device_local_buffer(
            GpuUploadCtx {
                device,
                allocator,
                queue: ctx.queue,
                command_pool: ctx.command_pool,
            },
            std::mem::size_of_val(deltas) as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            deltas,
            None,
        )
        .context("create morph delta buffer")?;

        let weight_bytes = (target_count as vk::DeviceSize) * 4;
        let mut weight_buffer = GpuBuffer::create_host_visible(
            device,
            allocator,
            weight_bytes,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )
        .context("create morph weight buffer")?;
        // Zero-init so a first-sight frame (before the animation
        // system's first write) reads "no deformation" instead of
        // uninitialised host-visible memory. Mirrors the
        // image_health_buffers zero-init pattern in init.rs.
        if let Ok(bytes) = weight_buffer.mapped_slice_mut() {
            bytes.fill(0);
        }
        weight_buffer
            .flush_if_needed(device)
            .context("flush morph weight buffer zero-init")?;

        // SAFETY: the buffer was just created with SHADER_DEVICE_ADDRESS.
        let delta_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(delta_buffer.buffer),
            )
        };
        // SAFETY: the buffer was just created with SHADER_DEVICE_ADDRESS.
        let weight_address = unsafe {
            device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(weight_buffer.buffer),
            )
        };

        Ok(Self {
            delta_buffer,
            delta_address,
            weight_buffer,
            weight_address,
            pending_weights: vec![0.0; target_count as usize],
            pending_weights_dirty: false,
            target_count,
            vertex_count,
            last_used_frame: 0,
        })
    }

    /// Stage current-frame weights without touching GPU-visible memory.
    /// `draw_frame` flushes this handoff only after its dual-fence wait.
    pub fn stage_weights(&mut self, weights: Vec<f32>) {
        debug_assert_eq!(
            weights.len(),
            self.target_count as usize,
            "MorphSlot::stage_weights: weights length must equal target_count"
        );
        self.pending_weights = weights;
        self.pending_weights_dirty = true;
    }

    /// Copy staged weights into mapped GPU memory. This must only be called
    /// after all submissions that could reference `weight_buffer` have
    /// completed; `VulkanContext::draw_frame` owns that fence proof (#3244).
    pub(crate) fn flush_pending_weights(&mut self, device: &ash::Device) -> Result<()> {
        if !self.pending_weights_dirty {
            return Ok(());
        }
        let weights = &self.pending_weights;
        // SAFETY: `weights` is a live contiguous f32 slice, and the byte view
        // is used only for this copy without outliving the slice.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                weights.as_ptr() as *const u8,
                std::mem::size_of_val(weights),
            )
        };
        self.weight_buffer.mapped_slice_mut()?[..bytes.len()].copy_from_slice(bytes);
        self.weight_buffer.flush_if_needed(device)?;
        self.pending_weights_dirty = false;
        Ok(())
    }

    /// Destroy both buffers. Caller must have waited for the device to
    /// go idle, or otherwise guaranteed no in-flight command buffer
    /// still references either buffer's device address — same
    /// precondition `SkinSlot`'s teardown carries.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        self.delta_buffer.destroy(device, allocator);
        self.weight_buffer.destroy(device, allocator);
    }
}

#[cfg(test)]
mod tests {
    /// #3244 — mapped morph weights may only be written after the dual-fence
    /// wait that proves both frame-in-flight readers have completed.
    #[test]
    fn draw_flushes_pending_morph_weights_after_waiting_both_fences() {
        // #3282 / TD1-2026-08-24-01 — both the fence wait and the morph
        // flush moved from `draw.rs` into `sync_and_acquire_frame.rs`.
        let draw = include_str!("context/sync_and_acquire_frame.rs");
        let wait = draw
            .find(".wait_for_fences(")
            .expect("draw_frame must wait its in-flight fences");
        let flush = draw
            .find("self.flush_pending_morph_weights()")
            .expect("draw_frame must flush staged morph weights");
        assert!(wait < flush, "morph host write must follow the fence wait");
    }
}
