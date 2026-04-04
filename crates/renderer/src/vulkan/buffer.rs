//! GPU buffer abstraction backed by `gpu_allocator`.

use super::allocator::SharedAllocator;
use super::texture::with_one_time_commands;
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
    /// Create a vertex buffer in DEVICE_LOCAL memory via staging upload.
    /// When `rt_enabled` is true, adds usage flags needed for BLAS builds.
    pub fn create_vertex_buffer<T: Copy>(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        data: &[T],
        rt_enabled: bool,
    ) -> Result<Self> {
        let size = (std::mem::size_of::<T>() * data.len()) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::VERTEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(device, allocator, queue, command_pool, size, usage, data)
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
    ) -> Result<Self> {
        let size = (std::mem::size_of::<u32>() * data.len()) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::INDEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(device, allocator, queue, command_pool, size, usage, data)
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

    /// Write data to a host-visible buffer's mapped memory.
    pub fn write_mapped<T: Copy>(&mut self, data: &[T]) -> Result<()> {
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
        };
        let alloc = self
            .allocation
            .as_mut()
            .context("Buffer has no allocation")?;
        let mapped = alloc.mapped_slice_mut().context("Buffer not mapped")?;
        let len = bytes.len().min(mapped.len());
        mapped[..len].copy_from_slice(&bytes[..len]);
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
    fn create_device_local_buffer<T: Copy>(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        data: &[T],
    ) -> Result<Self> {
        // 1. Create and fill the staging buffer (HOST_VISIBLE).
        let staging_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_info, None)
                .context("Failed to create staging buffer")?
        };

        let staging_reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };

        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "buffer_staging",
                requirements: staging_reqs,
                location: MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate staging memory")?;

        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind staging buffer")?;
        }

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
        with_one_time_commands(device, queue, command_pool, |cmd| unsafe {
            device.cmd_copy_buffer(cmd, staging_buffer, buffer, &[copy_region]);
        })?;

        // 4. Free staging resources.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .expect("Failed to free staging allocation");

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
