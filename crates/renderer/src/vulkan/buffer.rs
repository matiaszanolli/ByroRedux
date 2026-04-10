//! GPU buffer abstraction backed by `gpu_allocator`.

use super::allocator::SharedAllocator;
use super::texture::with_one_time_commands;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan;
use gpu_allocator::MemoryLocation;

/// Pool of reusable staging buffers to avoid per-upload allocate/free cycles.
///
/// On a cell load with 500 meshes + 200 textures, this eliminates ~1200
/// staging buffer creation/destruction cycles through gpu_allocator.
pub struct StagingPool {
    /// Available staging buffers sorted by size (ascending).
    free_list: Vec<StagingEntry>,
    device: ash::Device,
    allocator: SharedAllocator,
}

struct StagingEntry {
    buffer: vk::Buffer,
    allocation: vulkan::Allocation,
    capacity: vk::DeviceSize,
}

impl StagingPool {
    pub fn new(device: ash::Device, allocator: SharedAllocator) -> Self {
        Self {
            free_list: Vec::new(),
            device,
            allocator,
        }
    }

    /// Acquire a mapped staging buffer with at least `size` bytes.
    /// Returns a reused buffer from the pool or creates a new one.
    pub fn acquire(&mut self, size: vk::DeviceSize) -> Result<(vk::Buffer, vulkan::Allocation)> {
        // Find the smallest free buffer that fits.
        if let Some(idx) = self.free_list.iter().position(|e| e.capacity >= size) {
            let entry = self.free_list.remove(idx);
            return Ok((entry.buffer, entry.allocation));
        }

        // No suitable buffer — create a new one.
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            self.device
                .create_buffer(&buffer_info, None)
                .context("Failed to create staging buffer")?
        };

        let reqs = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let allocation = self
            .allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "staging_pool",
                requirements: reqs,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate staging memory")?;

        unsafe {
            self.device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind staging buffer")?;
        }

        Ok((buffer, allocation))
    }

    /// Return a staging buffer to the pool for reuse.
    pub fn release(
        &mut self,
        buffer: vk::Buffer,
        allocation: vulkan::Allocation,
        capacity: vk::DeviceSize,
    ) {
        // Insert sorted by capacity for best-fit search.
        let pos = self.free_list.partition_point(|e| e.capacity < capacity);
        self.free_list.insert(
            pos,
            StagingEntry {
                buffer,
                allocation,
                capacity,
            },
        );
    }

    /// Destroy all pooled staging buffers. Call before device destruction.
    pub fn destroy(&mut self) {
        for entry in self.free_list.drain(..) {
            unsafe {
                // SAFETY: buffer was created by this device, not yet destroyed.
                self.device.destroy_buffer(entry.buffer, None);
            }
            self.allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(entry.allocation)
                .expect("Failed to free staging allocation");
        }
    }
}

/// RAII guard for a staging buffer. Destroys on drop if not explicitly released.
/// Used to ensure cleanup on early return from upload paths.
pub(crate) struct StagingGuard {
    pub buffer: vk::Buffer,
    pub allocation: Option<vulkan::Allocation>,
    device: ash::Device,
    allocator: SharedAllocator,
}

impl StagingGuard {
    pub fn new(
        buffer: vk::Buffer,
        allocation: vulkan::Allocation,
        device: ash::Device,
        allocator: SharedAllocator,
    ) -> Self {
        Self {
            buffer,
            allocation: Some(allocation),
            device,
            allocator,
        }
    }

    /// Consume the guard, destroying staging resources.
    pub fn destroy(mut self) {
        self.cleanup();
    }

    fn cleanup(&mut self) {
        unsafe {
            // SAFETY: buffer was created by this device and has not been destroyed yet.
            self.device.destroy_buffer(self.buffer, None);
        }
        if let Some(alloc) = self.allocation.take() {
            self.allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(alloc)
                .expect("Failed to free staging allocation");
        }
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.allocation.is_some() {
            self.cleanup();
        }
    }
}

/// A GPU buffer with its backing allocation.
///
/// Destruction requires the allocator, so call [`destroy`](Self::destroy)
/// explicitly before dropping. Dropping without destroy will leak.
/// Conservative upper bound for nonCoherentAtomSize across all GPUs.
/// Typical: 64 bytes (NVIDIA/AMD), max observed: 256 bytes.
/// Used to align flush offsets when the exact device value isn't plumbed through.
const NON_COHERENT_ATOM_SIZE: vk::DeviceSize = 256;

/// Compute a `(offset, size)` pair that fully covers `[offset, offset+size)`
/// and satisfies `VkMappedMemoryRange` alignment requirements: offset rounded
/// down and size rounded up to `NON_COHERENT_ATOM_SIZE`.
///
/// Used instead of `VK_WHOLE_SIZE` so flushes don't extend past the
/// gpu-allocator sub-allocation into unrelated memory.
fn aligned_flush_range(
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
) -> (vk::DeviceSize, vk::DeviceSize) {
    let aligned_offset = offset & !(NON_COHERENT_ATOM_SIZE - 1);
    let extra = offset - aligned_offset;
    let unaligned_size = extra + size;
    let aligned_size =
        (unaligned_size + NON_COHERENT_ATOM_SIZE - 1) & !(NON_COHERENT_ATOM_SIZE - 1);
    (aligned_offset, aligned_size)
}

pub struct GpuBuffer {
    pub buffer: vk::Buffer,
    pub size: vk::DeviceSize,
    allocation: Option<vulkan::Allocation>,
}

impl GpuBuffer {
    /// Create a vertex buffer in DEVICE_LOCAL memory via staging upload.
    /// When `rt_enabled` is true, adds usage flags needed for BLAS builds.
    pub fn create_vertex_buffer<T: Copy>(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        data: &[T],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        let size = (std::mem::size_of::<T>() * data.len()) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::VERTEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            size,
            usage,
            data,
            staging_pool,
        )
    }

    /// Create an index buffer in DEVICE_LOCAL memory via staging upload.
    /// When `rt_enabled` is true, adds usage flags needed for BLAS builds.
    pub fn create_index_buffer(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        data: &[u32],
        rt_enabled: bool,
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        let size = (std::mem::size_of::<u32>() * data.len()) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::INDEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(
            device,
            allocator,
            queue,
            command_pool,
            size,
            usage,
            data,
            staging_pool,
        )
    }

    /// Create a host-visible buffer for per-frame CPU writes (no staging needed).
    /// Used for SSBO/UBO data that changes every frame.
    pub fn create_host_visible(
        device: &ash::Device,
        allocator: &SharedAllocator,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create host-visible buffer")?
        };

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "host_visible_buffer",
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate host-visible memory")?;

        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind host-visible buffer")?;
        }

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
        })
    }

    /// Create a DEVICE_LOCAL buffer without initial data.
    ///
    /// Used for GPU-only buffers that are populated by device commands
    /// (e.g. acceleration structure result/scratch buffers). Not
    /// host-visible — cannot be mapped or written from CPU.
    pub fn create_device_local_uninit(
        device: &ash::Device,
        allocator: &SharedAllocator,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create device-local buffer")?
        };

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "device_local_buffer",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate device-local memory")?;

        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind device-local buffer")?;
        }

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
        })
    }

    /// Get the mapped memory slice for direct writes (no intermediate Vec).
    /// Call `flush_if_needed()` after writing to ensure GPU visibility.
    pub fn mapped_slice_mut(&mut self) -> Result<&mut [u8]> {
        let alloc = self
            .allocation
            .as_mut()
            .context("Buffer has no allocation")?;
        alloc.mapped_slice_mut().context("Buffer not mapped")
    }

    /// Flush mapped memory if not HOST_COHERENT. Call after direct writes
    /// via `mapped_slice_mut()` to ensure GPU visibility.
    pub fn flush_if_needed(&mut self, device: &ash::Device) -> Result<()> {
        let alloc = self
            .allocation
            .as_ref()
            .context("Buffer has no allocation")?;

        let is_coherent = alloc
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

        if !is_coherent {
            let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
            // SAFETY: alloc.memory() is valid and mapped. The range is contained
            // within this allocation's slice of the parent VkDeviceMemory: offset
            // is rounded down and size rounded up to nonCoherentAtomSize, which
            // gpu-allocator already pads sub-allocations to.
            unsafe {
                let range = vk::MappedMemoryRange::default()
                    .memory(alloc.memory())
                    .offset(aligned_offset)
                    .size(aligned_size);
                device
                    .flush_mapped_memory_ranges(&[range])
                    .context("Failed to flush mapped memory")?;
            }
        }
        Ok(())
    }

    /// Write data to a host-visible buffer's mapped memory.
    ///
    /// If the allocation is not HOST_COHERENT, an explicit flush is
    /// performed to make the write visible to the GPU.
    pub fn write_mapped<T: Copy>(&mut self, device: &ash::Device, data: &[T]) -> Result<()> {
        // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
        // valid and aligned (from a live slice), and size_of_val gives the
        // exact byte length.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
        };
        let alloc = self
            .allocation
            .as_mut()
            .context("Buffer has no allocation")?;

        let is_coherent = alloc
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

        let mapped = alloc.mapped_slice_mut().context("Buffer not mapped")?;
        if bytes.len() > mapped.len() {
            log::warn!(
                "write_mapped: data ({} bytes) exceeds buffer capacity ({} bytes) — truncating",
                bytes.len(),
                mapped.len()
            );
        }
        let len = bytes.len().min(mapped.len());
        mapped[..len].copy_from_slice(&bytes[..len]);

        // Flush explicitly if the memory is not HOST_COHERENT.
        if !is_coherent {
            // Vulkan spec requires both offset and size to be multiples of
            // nonCoherentAtomSize (or size == VK_WHOLE_SIZE). Using
            // VK_WHOLE_SIZE here would over-flush past this sub-allocation
            // into unrelated memory; instead bound the range to this
            // allocation only, rounding outward to atom size.
            let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());

            // SAFETY: alloc.memory() returns the VkDeviceMemory backing this
            // allocation, which is valid and mapped (verified above). The
            // range is contained within this allocation's region of the parent
            // memory object — gpu-allocator pads sub-allocations to atom size.
            unsafe {
                let range = vk::MappedMemoryRange::default()
                    .memory(alloc.memory())
                    .offset(aligned_offset)
                    .size(aligned_size);
                device
                    .flush_mapped_memory_ranges(&[range])
                    .context("Failed to flush mapped memory")?;
            }
        }

        Ok(())
    }

    /// Destroy the buffer and free its GPU memory.
    /// Must be called before the device is destroyed.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if let Some(allocation) = self.allocation.take() {
            unsafe {
                device.destroy_buffer(self.buffer, None);
            }
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .expect("Failed to free GPU allocation");
        }
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Create a DEVICE_LOCAL buffer and upload data via a CpuToGpu staging buffer.
    pub fn create_device_local_buffer<T: Copy>(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        data: &[T],
        staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        // 1. Acquire staging buffer — from pool (reuse) or create fresh.
        let (staging_buffer, mut staging_alloc) = if let Some(pool) = staging_pool {
            pool.acquire(size)?
        } else {
            let staging_info = vk::BufferCreateInfo::default()
                .size(size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let buf = unsafe {
                device
                    .create_buffer(&staging_info, None)
                    .context("Failed to create staging buffer")?
            };

            let reqs = unsafe { device.get_buffer_memory_requirements(buf) };

            let alloc = allocator
                .lock()
                .expect("allocator lock poisoned")
                .allocate(&vulkan::AllocationCreateDesc {
                    name: "buffer_staging",
                    requirements: reqs,
                    location: MemoryLocation::CpuToGpu,
                    linear: true,
                    allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
                })
                .context("Failed to allocate staging memory")?;

            unsafe {
                device
                    .bind_buffer_memory(buf, alloc.memory(), alloc.offset())
                    .context("Failed to bind staging buffer")?;
            }
            (buf, alloc)
        };

        // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
        // valid and aligned (from a live slice), and size_of_val gives the
        // exact byte length. The borrow is bounded by this scope.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
        };

        staging_alloc
            .mapped_slice_mut()
            .context("Staging buffer not mapped")?[..bytes.len()]
            .copy_from_slice(bytes);

        // Wrap staging resources in RAII guard — ensures cleanup on early return.
        let staging = StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );

        // 2. Create the device-local buffer (GPU_ONLY).
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create device-local buffer")?
        };

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "gpu_buffer",
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate device-local memory")?;

        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind device-local buffer")?;
        }

        // 3. Copy staging → device-local via one-time command buffer.
        let copy_region = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size,
        };
        with_one_time_commands(device, queue, command_pool, |cmd| {
            unsafe {
                device.cmd_copy_buffer(cmd, staging.buffer, buffer, &[copy_region]);
            }
            Ok(())
        })?;

        // 4. Free staging resources (guard ensures cleanup even on error above).
        staging.destroy();

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
        })
    }
}

impl Drop for GpuBuffer {
    fn drop(&mut self) {
        if self.allocation.is_some() {
            log::warn!("GpuBuffer dropped without destroy() — VkBuffer and GPU allocation leaked");
            debug_assert!(false, "GpuBuffer leaked: call destroy() before dropping");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_flush_range_already_aligned() {
        // Offset and size both already on atom boundary.
        let (off, sz) = aligned_flush_range(512, 1024);
        assert_eq!(off, 512);
        assert_eq!(sz, 1024);
        assert!(sz % NON_COHERENT_ATOM_SIZE == 0);
    }

    #[test]
    fn aligned_flush_range_unaligned_offset() {
        // Offset 100 → rounds down to 0; size grows to cover original end.
        let (off, sz) = aligned_flush_range(100, 200);
        assert_eq!(off, 0);
        // Original range: [100, 300). Aligned: [0, ?), size must be >= 300.
        assert!(off + sz >= 300);
        assert!(sz % NON_COHERENT_ATOM_SIZE == 0);
    }

    #[test]
    fn aligned_flush_range_unaligned_size() {
        // Offset aligned, size 300 → rounds up to 512.
        let (off, sz) = aligned_flush_range(0, 300);
        assert_eq!(off, 0);
        assert_eq!(sz, 512);
    }

    #[test]
    fn aligned_flush_range_small_allocation() {
        // 48-byte allocation at offset 0 → rounds size up to 256.
        let (off, sz) = aligned_flush_range(0, 48);
        assert_eq!(off, 0);
        assert_eq!(sz, NON_COHERENT_ATOM_SIZE);
    }

    #[test]
    fn aligned_flush_range_offset_inside_atom() {
        // Offset 50, size 50 → covers [50, 100). Aligned: [0, 256).
        let (off, sz) = aligned_flush_range(50, 50);
        assert_eq!(off, 0);
        assert_eq!(sz, NON_COHERENT_ATOM_SIZE);
        assert!(off + sz >= 100);
    }

    #[test]
    fn aligned_flush_range_does_not_use_whole_size() {
        // Whatever the input, the result must be a finite size — never WHOLE_SIZE.
        let (_, sz) = aligned_flush_range(0, 1);
        assert_ne!(sz, vk::WHOLE_SIZE);
        let (_, sz) = aligned_flush_range(1024 * 1024, 4096);
        assert_ne!(sz, vk::WHOLE_SIZE);
    }
}
