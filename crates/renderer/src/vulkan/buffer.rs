//! GPU buffer abstraction backed by `gpu_allocator`.

use super::allocator::SharedAllocator;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan;
use gpu_allocator::MemoryLocation;

/// A GPU buffer with its backing allocation.
///
/// Destruction requires the allocator, so call [`destroy`](Self::destroy)
/// explicitly before dropping. Dropping without destroy will leak.
pub struct GpuBuffer {
    pub buffer: vk::Buffer,
    pub size: vk::DeviceSize,
    allocation: Option<vulkan::Allocation>,
}

impl GpuBuffer {
    /// Create a vertex buffer with HOST_VISIBLE | HOST_COHERENT memory
    /// and immediately upload the provided data.
    pub fn create_vertex_buffer<T: Copy>(
        device: &ash::Device,
        allocator: &SharedAllocator,
        data: &[T],
    ) -> Result<Self> {
        let size = (std::mem::size_of::<T>() * data.len()) as vk::DeviceSize;
        let mut buf = Self::create_buffer(
            device,
            allocator,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
        )?;
        buf.upload(data)?;
        Ok(buf)
    }

    /// Create an index buffer with HOST_VISIBLE | HOST_COHERENT memory
    /// and immediately upload the provided data.
    pub fn create_index_buffer(
        device: &ash::Device,
        allocator: &SharedAllocator,
        data: &[u32],
    ) -> Result<Self> {
        let size = (std::mem::size_of::<u32>() * data.len()) as vk::DeviceSize;
        let mut buf = Self::create_buffer(
            device,
            allocator,
            size,
            vk::BufferUsageFlags::INDEX_BUFFER,
        )?;
        buf.upload(data)?;
        Ok(buf)
    }

    /// Write data into the buffer's mapped memory.
    pub fn upload<T: Copy>(&mut self, data: &[T]) -> Result<()> {
        let alloc = self
            .allocation
            .as_mut()
            .context("Buffer has no allocation (already destroyed?)")?;

        let mapped = alloc
            .mapped_slice_mut()
            .context("Buffer memory is not mapped")?;

        // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
        // valid and aligned (from a live slice), and size_of_val gives the
        // exact byte length. The borrow is bounded by this function scope.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
        };

        mapped[..bytes.len()].copy_from_slice(bytes);
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

    fn create_buffer(
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
                .context("Failed to create buffer")?
        };

        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "gpu_buffer",
                requirements,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate GPU memory")?;

        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind buffer memory")?;
        }

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
        })
    }
}
