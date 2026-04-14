//! GPU buffer abstraction backed by `gpu_allocator`.

use super::allocator::SharedAllocator;
use super::texture::with_one_time_commands;
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan;
use gpu_allocator::MemoryLocation;

/// Default upper bound on total staging-pool capacity. Chosen to comfortably
/// cover the largest single upload the engine performs today (a few MB) plus
/// breathing room for a burst of concurrent uploads, without letting a cell
/// load (500 meshes + 200 textures) accumulate hundreds of megabytes of
/// forever-pinned host memory. Callers that stream huge assets can raise it
/// via `StagingPool::with_budget`.
pub const DEFAULT_STAGING_BUDGET_BYTES: vk::DeviceSize = 64 * 1024 * 1024;

/// Pool of reusable staging buffers to avoid per-upload allocate/free cycles.
///
/// On a cell load with 500 meshes + 200 textures, this eliminates ~1200
/// staging buffer creation/destruction cycles through gpu_allocator.
///
/// The pool enforces a total-capacity budget on [`release`](Self::release):
/// when a returned buffer would push the total over the budget, the pool
/// evicts its largest entries (destroying the underlying Vulkan resources)
/// until it fits. Callers doing bulk streaming can also call
/// [`trim_to`](Self::trim_to) after a loading phase to force aggressive
/// eviction down to any target size (often `0` after a cell load). See #99.
pub struct StagingPool {
    /// Available staging buffers sorted by size (ascending).
    free_list: Vec<StagingEntry>,
    /// Maximum total capacity retained across all free entries. New
    /// `release` calls that would exceed this trigger eviction of the
    /// largest entries first.
    budget_bytes: vk::DeviceSize,
    device: ash::Device,
    allocator: SharedAllocator,
}

struct StagingEntry {
    buffer: vk::Buffer,
    allocation: vulkan::Allocation,
    capacity: vk::DeviceSize,
}

impl StagingPool {
    /// Create a pool with the default retained-capacity budget
    /// ([`DEFAULT_STAGING_BUDGET_BYTES`]).
    pub fn new(device: ash::Device, allocator: SharedAllocator) -> Self {
        Self::with_budget(device, allocator, DEFAULT_STAGING_BUDGET_BYTES)
    }

    /// Create a pool with an explicit retained-capacity budget.
    ///
    /// `budget_bytes = 0` disables retention entirely — every release
    /// destroys its buffer immediately, which is useful for debugging
    /// memory footprint.
    pub fn with_budget(
        device: ash::Device,
        allocator: SharedAllocator,
        budget_bytes: vk::DeviceSize,
    ) -> Self {
        Self {
            free_list: Vec::new(),
            budget_bytes,
            device,
            allocator,
        }
    }

    /// Total capacity currently held in the free list (sum of all
    /// retained entries).
    pub fn total_capacity(&self) -> vk::DeviceSize {
        self.free_list.iter().map(|e| e.capacity).sum()
    }

    /// Number of entries currently held.
    pub fn len(&self) -> usize {
        self.free_list.len()
    }

    /// Configured budget.
    pub fn budget_bytes(&self) -> vk::DeviceSize {
        self.budget_bytes
    }

    /// True when there are no retained entries.
    pub fn is_empty(&self) -> bool {
        self.free_list.is_empty()
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
    ///
    /// After insertion, if the total retained capacity exceeds the
    /// configured budget, the pool evicts its largest entries until it
    /// fits — see [`trim_to`](Self::trim_to). This keeps bulk loads
    /// (cells, archives) from retaining hundreds of megabytes of host
    /// memory forever.
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

        // Auto-trim: if this release pushed us over budget, evict
        // largest-first until we fit. This absorbs the bulk-load case
        // described in #99 without requiring callers to remember.
        if self.total_capacity() > self.budget_bytes {
            self.trim_to(self.budget_bytes);
        }
    }

    /// Evict retained staging buffers until total capacity ≤ `target`.
    ///
    /// Evicts the largest entries first — maximum bytes freed per
    /// destroyed Vulkan resource. Pass `0` to drop the pool back to
    /// empty (useful after a cell load or archive flush).
    pub fn trim_to(&mut self, target: vk::DeviceSize) {
        let capacities: Vec<vk::DeviceSize> = self.free_list.iter().map(|e| e.capacity).collect();
        let evict = select_evictions(&capacities, target);
        for _ in 0..evict {
            // `free_list` is sorted ascending by capacity, so the last
            // entry is always the largest — the one the policy wants
            // next. Pop + destroy, no index shifting.
            let Some(entry) = self.free_list.pop() else {
                break;
            };
            unsafe {
                // SAFETY: entry.buffer was created by this device via
                // `acquire` / `release`, not yet destroyed, and not
                // currently bound to any in-flight command buffer
                // (callers call `release` only after the upload
                // command's fence has signalled).
                self.device.destroy_buffer(entry.buffer, None);
            }
            self.allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(entry.allocation)
                .expect("Failed to free staging allocation");
        }
    }

    /// Trim the pool back to its configured budget. Intended for
    /// end-of-phase cleanup points (post-cell-load, post-BSA-load)
    /// where the caller wants to force eviction without raising or
    /// lowering the steady-state budget.
    pub fn trim(&mut self) {
        self.trim_to(self.budget_bytes);
    }

    /// Destroy all pooled staging buffers. Call before device destruction.
    pub fn destroy(&mut self) {
        self.trim_to(0);
    }
}

/// Compute how many entries to evict from a capacity-sorted free list
/// (ascending) so the total drops to `target` or below. Always pops
/// largest-first (from the end) because that minimizes destroyed
/// entries for a given number of freed bytes. Pure, deterministic, and
/// independent of Vulkan so it can be unit-tested without a device.
fn select_evictions(capacities_ascending: &[vk::DeviceSize], target: vk::DeviceSize) -> usize {
    let mut total: vk::DeviceSize = capacities_ascending.iter().sum();
    if total <= target {
        return 0;
    }
    let mut evict = 0usize;
    for &cap in capacities_ascending.iter().rev() {
        if total <= target {
            break;
        }
        total = total.saturating_sub(cap);
        evict += 1;
    }
    evict
}

impl Drop for StagingPool {
    fn drop(&mut self) {
        if !self.free_list.is_empty() {
            log::warn!(
                "StagingPool dropped with {} unreleased buffers — call destroy() before drop",
                self.free_list.len(),
            );
            debug_assert!(
                self.free_list.is_empty(),
                "StagingPool leaked {} staging buffers",
                self.free_list.len(),
            );
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

    // ── StagingPool eviction policy tests (#99) ─────────────────────
    //
    // These exercise `select_evictions` directly instead of spinning up
    // a real Vulkan device. The policy is pure arithmetic so it can be
    // validated in isolation; the trim_to / release integration on top
    // of it is a straight mapping (pop largest, destroy).

    #[test]
    fn select_evictions_under_budget_is_noop() {
        // Sum 30 ≤ budget 100 → nothing to evict.
        let caps = [10u64, 10, 10];
        assert_eq!(select_evictions(&caps, 100), 0);
    }

    #[test]
    fn select_evictions_exactly_at_budget_is_noop() {
        // Boundary — 30 == 30 still passes.
        let caps = [10u64, 10, 10];
        assert_eq!(select_evictions(&caps, 30), 0);
    }

    #[test]
    fn select_evictions_evicts_largest_first() {
        // Ascending: [10, 20, 30, 40]. Total 100, target 50.
        // Popping the 40 gives 60 (still > 50); then the 30 gives 30.
        // Must evict exactly 2 entries.
        let caps = [10u64, 20, 30, 40];
        assert_eq!(select_evictions(&caps, 50), 2);
    }

    #[test]
    fn select_evictions_target_zero_evicts_everything() {
        let caps = [10u64, 20, 30];
        assert_eq!(select_evictions(&caps, 0), 3);
    }

    #[test]
    fn select_evictions_handles_empty_list() {
        let caps: [u64; 0] = [];
        assert_eq!(select_evictions(&caps, 0), 0);
        assert_eq!(select_evictions(&caps, 100), 0);
    }

    #[test]
    fn select_evictions_cell_load_scenario() {
        // Simulate the cell-load case from #99: 700 small staging buffers
        // averaging 256 KiB each = 175 MiB retained. Default budget is
        // 64 MiB → must evict enough to fit under 64 MiB.
        let caps: Vec<u64> = (0..700).map(|_| 256 * 1024).collect();
        let budget = DEFAULT_STAGING_BUDGET_BYTES;
        let evict = select_evictions(&caps, budget);

        // Post-eviction total must be ≤ budget.
        let remaining: u64 = caps.iter().take(caps.len() - evict).copied().sum();
        assert!(
            remaining <= budget,
            "remaining {} bytes should fit under {} byte budget after evicting {} entries",
            remaining,
            budget,
            evict,
        );

        // And we should not over-evict: one fewer eviction must exceed
        // the budget. Otherwise the policy is wasteful.
        if evict > 0 {
            let over: u64 = caps.iter().take(caps.len() - (evict - 1)).copied().sum();
            assert!(
                over > budget,
                "evicting {} entries left {} bytes — could have evicted one fewer",
                evict - 1,
                over,
            );
        }
    }
}
