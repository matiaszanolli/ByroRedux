//! GPU buffer abstraction backed by `gpu_allocator`.

use super::allocator::SharedAllocator;
use super::texture::{with_one_time_commands, with_one_time_commands_reuse_fence};
use super::GpuUploadCtx;
use anyhow::{bail, Context, Result};
use ash::vk;
use gpu_allocator::vulkan;
use gpu_allocator::MemoryLocation;

/// MEM-2-5 / #680 — startup assertion that a freshly-allocated
/// `MemoryLocation::CpuToGpu` allocation came back with a persistent
/// host mapping. `gpu-allocator` 0.27 maps `CpuToGpu` linear
/// allocations once at `allocate()` and keeps them mapped for the
/// allocation's lifetime; that contract is documented but not asserted
/// at our buffer construction. A future allocator-config flag (or a
/// backend swap) that defers mapping would silently make every
/// downstream `.mapped_slice_mut()` panic on "Buffer not mapped"
/// the first time the renderer tries to upload — instead of failing
/// loudly at startup. Calling this helper after every CpuToGpu
/// `allocate()` is the cheap belt-and-braces guard the audit asked
/// for. `debug_assert!` so release builds pay zero cycles.
#[inline]
pub(crate) fn debug_assert_cpu_to_gpu_mapped(
    allocation: &vulkan::Allocation,
    context: &'static str,
) {
    debug_assert!(
        allocation.mapped_slice().is_some(),
        "{context}: gpu-allocator returned a CpuToGpu allocation with no \
         persistent mapping — `mapped_slice_mut()` will panic on the first \
         write. See MEM-2-5 / #680."
    );
}

/// Default upper bound on total staging-pool capacity.
///
/// Sized for a typical interior → exterior transition burst: ~20 BC7
/// textures at 4K with full mips (~22 MB each), plus a handful of 2K
/// BC7 (~5 MB), plus mesh staging headroom. 128 MB absorbs that
/// without forcing mid-burst eviction (which destroys buffers the
/// next texture in the same burst could have reused — exactly the
/// pool's purpose at its worst moment). Long sessions still cap
/// retained capacity so RSS doesn't balloon — the budget only
/// bounds *retained* size, not in-flight allocations.
///
/// Pre-#511 default was 64 MB, sized for mesh streaming only
/// (average mesh ~50 KB). Bumped after texture uploads were wired
/// into the pool (#239).
///
/// Callers with atypical working sets (large cell batches, modded
/// content) can override via `StagingPool::with_budget`.
pub const DEFAULT_STAGING_BUDGET_BYTES: vk::DeviceSize = 128 * 1024 * 1024;

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

        // SAFETY: `self.device` is this pool's live logical device, which
        // outlives the call; `buffer_info` is a fully-populated, valid
        // VkBufferCreateInfo built just above.
        let buffer = unsafe {
            self.device
                .create_buffer(&buffer_info, None)
                .context("Failed to create staging buffer")?
        };

        // SAFETY: `buffer` was just created by this device above and not yet
        // destroyed; the device outlives the call.
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
        // MEM-2-5 / #680 sibling — every staging buffer surfaced by this
        // pool is written through `mapped_slice_mut`. See
        // `debug_assert_cpu_to_gpu_mapped` for the rationale.
        debug_assert_cpu_to_gpu_mapped(&allocation, "StagingPool::acquire");

        // SAFETY: `buffer` and `allocation` were both created here from this
        // device; the memory/offset come from the allocation that satisfied
        // `buffer`'s own memory requirements, and the buffer is not yet bound.
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

/// Create a fresh host-visible staging buffer and bind memory to it.
///
/// Every fallible step unwinds internally, so a failure leaks nothing:
/// an `allocate` failure destroys the just-created `VkBuffer`, and a
/// `bind_buffer_memory` failure destroys the buffer *and* frees the
/// allocation. Callers own both on success and must hand them to a
/// [`StagingGuard`] (which is what makes every *subsequent* `?` safe).
///
/// Extracted for REN-LOW L-5 / #2164: this create → allocate → bind
/// prologue was open-coded at three sites, and the window between
/// `create_buffer` and the guard was unprotected at all of them. Doing
/// the unwinding once, here, is what closes it — a guard alone can't,
/// since it cannot exist until after the allocation succeeds.
pub(crate) fn create_staging_buffer(
    device: &ash::Device,
    allocator: &SharedAllocator,
    size: vk::DeviceSize,
    name: &'static str,
) -> Result<(vk::Buffer, vulkan::Allocation)> {
    let info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {
        // SAFETY: `device` is the caller's live logical device, valid for the
        // call; `info` is a fully-populated valid VkBufferCreateInfo whose
        // borrows outlive it; the None allocation callback is always valid.
        device
            .create_buffer(&info, None)
            .with_context(|| format!("Failed to create {name} staging buffer"))?
    };

    // From here on, every early return must destroy `buffer` by hand —
    // there is no guard to own it until the allocation lands.
    let destroy = || unsafe {
        // SAFETY: `buffer` was created by `device` immediately above, has
        // never been bound or referenced by a command buffer, and is
        // destroyed exactly once on each of these unwind paths.
        device.destroy_buffer(buffer, None);
    };

    let reqs = unsafe {
        // SAFETY: pure query — `buffer` was just created by this same live
        // `device` and has not been destroyed.
        device.get_buffer_memory_requirements(buffer)
    };

    let allocation = match allocator.lock().expect("allocator lock poisoned").allocate(
        &vulkan::AllocationCreateDesc {
            name,
            requirements: reqs,
            location: MemoryLocation::CpuToGpu,
            linear: true,
            allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
        },
    ) {
        Ok(a) => a,
        Err(e) => {
            destroy();
            return Err(e).with_context(|| format!("Failed to allocate {name} staging memory"));
        }
    };
    debug_assert_cpu_to_gpu_mapped(&allocation, name);

    let bind = unsafe {
        // SAFETY: `device` is live; `buffer` was created by it above and is
        // not yet bound; `allocation`'s memory + offset come from the
        // allocator's successful allocation against this buffer's own memory
        // requirements, so the binding satisfies size/alignment.
        device.bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
    };
    if let Err(e) = bind {
        destroy();
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(allocation)
            .ok();
        return Err(e).with_context(|| format!("Failed to bind {name} staging buffer"));
    }

    Ok((buffer, allocation))
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

    /// Host-visible bytes of the guarded allocation.
    ///
    /// Exists so upload paths can fill staging *through* the guard rather
    /// than holding the raw `Allocation` alongside it — the `?` here
    /// unwinds into `Drop`, where the pre-#2164 spelling (mapping before
    /// constructing the guard) leaked both buffer and allocation.
    pub fn mapped_slice_mut(&mut self) -> Result<&mut [u8]> {
        self.allocation
            .as_mut()
            .context("StagingGuard allocation already released")?
            .mapped_slice_mut()
            .context("Staging buffer not host-mapped")
    }

    /// Publish host writes before a staged transfer when the allocator chose
    /// non-coherent memory. Most desktop upload heaps are coherent, but the
    /// batched mesh path fills one large arena directly and must not inherit
    /// that hardware assumption.
    pub fn flush_if_needed(&self, device: &ash::Device) -> Result<()> {
        let alloc = self
            .allocation
            .as_ref()
            .context("StagingGuard allocation already released")?;
        if alloc
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            return Ok(());
        }

        let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
        debug_assert_flush_range_bounded(alloc, aligned_offset, aligned_size);
        unsafe {
            // SAFETY: `alloc` is live and persistently mapped; the outward-
            // aligned range follows the same GpuAllocatorManaged contract as
            // `GpuBuffer::flush_if_needed`, and the staging allocation stays
            // alive until the transfer fence signals.
            device
                .flush_mapped_memory_ranges(&[vk::MappedMemoryRange::default()
                    .memory(alloc.memory())
                    .offset(aligned_offset)
                    .size(aligned_size)])
                .context("Failed to flush staging memory")?;
        }
        Ok(())
    }

    /// Consume the guard and return the staging buffer + allocation to
    /// a pool for reuse instead of destroying them. Pre-#239 the pool's
    /// release arm was unreachable — every upload path called
    /// [`destroy`] at the end, so acquired buffers went straight to
    /// gpu-allocator's free list rather than the pool's. This method
    /// disarms the guard (clears `allocation`) before handing the
    /// resources off, so the subsequent `Drop` is a no-op.
    ///
    /// `capacity` should be `allocation.size()` from the acquire call —
    /// the pool uses it as the entry size for best-fit searches.
    pub fn release_to(mut self, pool: &mut StagingPool, capacity: vk::DeviceSize) {
        if let Some(alloc) = self.allocation.take() {
            pool.release(self.buffer, alloc, capacity);
        }
        // `self` drops here; `allocation` is `None` so `Drop` is a no-op.
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
/// Typical: 64 bytes (NVIDIA/AMD desktop), max observed: 256 bytes
/// (Intel iGPU, some older mobile parts). Used to align flush
/// offsets when the exact device value isn't plumbed through.
///
/// Worst-case fallback: REN-D2-NEW-03 (audit 2026-05-09, INFO).
/// Replacing this with the queried
/// `VkPhysicalDeviceLimits::nonCoherentAtomSize` would cut every
/// `aligned_flush_range` call's rounded-up size by 2–4× on the
/// typical desktop case (64 vs 256 B). Skipped pending the
/// device-limits-plumbing groundwork (`PhysicalDeviceLimits` isn't
/// stashed on `VulkanContext` today; adding it touches every flush
/// site). Visible cost is bounded — each persistent host-visible
/// mapping (UBOs, instance staging, bone palette staging) sees one
/// flush per frame at most, so the wasted bytes per frame are
/// O(num_mapped_buffers × 192) on the worst case ≈ a few KB.
///
/// The 256 here is an *upper bound* on every real device, but rounding
/// to it is only valid if the actual `nonCoherentAtomSize` never exceeds
/// it — a smaller value over-aligns (safe), a larger value would
/// under-align and silently corrupt the flushed range
/// (VUID-VkMappedMemoryRange-size-01390). `device::create_logical_device`
/// holds a `debug_assert!` against this constant so an exotic future
/// device that reports a larger atom size fails loudly at init rather
/// than under-flushing. Exported `pub(crate)` so that guard pins to this
/// exact value instead of re-hardcoding 256. #1759 / TD7-002.
pub(crate) const NON_COHERENT_ATOM_SIZE: vk::DeviceSize = 256;

/// Compute a `(offset, size)` pair that fully covers `[offset, offset+size)`
/// and satisfies `VkMappedMemoryRange` alignment requirements: offset rounded
/// down and size rounded up to `NON_COHERENT_ATOM_SIZE`.
///
/// Used instead of `VK_WHOLE_SIZE` so flushes don't extend past the
/// gpu-allocator sub-allocation into unrelated memory.
pub(crate) fn aligned_flush_range(
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

/// #2683 (SAFE-D4-01) — best-effort guard against `aligned_flush_range`'s
/// outward rounding pushing a flush past the end of the memory object it's
/// flushing into. `Allocation` doesn't expose the parent gpu-allocator
/// block's size through any public API (verified against the vendored
/// `gpu-allocator 0.28.0` source — `memory_block_index` is private and
/// there is no accessor), so this can only check the one case gpu-allocator
/// DOES make checkable: `Allocation::is_dedicated()` — an allocation made
/// with an explicit `AllocationScheme::Dedicated{Buffer,Image}` gets its
/// OWN `VkDeviceMemory` object sized to exactly `alloc.size()`, so for that
/// case `alloc.offset() + alloc.size()` IS the true end of the memory
/// object and a widened flush past it is a real
/// VUID-VkMappedMemoryRange-size-01389 violation, not just an
/// over-generous flush into a sibling sub-allocation. This is exactly the
/// escalation path the SAFETY comments at the three call sites warn about:
/// a future refactor switching a host-visible buffer to a dedicated
/// allocation scheme.
///
/// Does NOT catch the OTHER escalation those comments warn about — a
/// `GpuAllocatorManaged` allocation whose requested size exceeds
/// gpu-allocator's shared-block threshold also gets a personal block sized
/// to fit exactly it, with the identical VUID exposure, but
/// `is_dedicated()` reports `false` for that case too (it reflects the
/// requested `AllocationScheme`, not whether gpu-allocator's internal
/// bin-packing happened to hand this allocation its own block) — nothing
/// in the public API distinguishes it from an ordinary shared-block
/// sub-allocation. That gap needs either an upstream gpu-allocator API
/// addition or validation-layer verification per the issue; not fixable
/// from this side of the crate boundary.
fn debug_assert_flush_range_bounded(
    alloc: &vulkan::Allocation,
    aligned_offset: vk::DeviceSize,
    aligned_size: vk::DeviceSize,
) {
    if alloc.is_dedicated() {
        debug_assert!(
            flush_range_within(aligned_offset, aligned_size, alloc.offset(), alloc.size()),
            "flush range [{aligned_offset}, {}) escapes the dedicated allocation's \
             [{}, {}) — VUID-VkMappedMemoryRange-size-01389 (#2683)",
            aligned_offset + aligned_size,
            alloc.offset(),
            alloc.offset() + alloc.size(),
        );
    }
}

/// Pure bound check `debug_assert_flush_range_bounded` relies on, split out
/// so the arithmetic is unit-testable without a live `Allocation` —
/// `gpu_allocator::vulkan::Allocation`'s fields are all private with no
/// public setters, only `Default` (all-zero), so a meaningful populated
/// instance can't be constructed outside the crate that owns it.
fn flush_range_within(
    aligned_offset: vk::DeviceSize,
    aligned_size: vk::DeviceSize,
    alloc_offset: vk::DeviceSize,
    alloc_size: vk::DeviceSize,
) -> bool {
    aligned_offset >= alloc_offset && aligned_offset + aligned_size <= alloc_offset + alloc_size
}

pub struct GpuBuffer {
    pub buffer: vk::Buffer,
    pub size: vk::DeviceSize,
    allocation: Option<vulkan::Allocation>,
    // Cached `HOST_COHERENT` flag from the allocation's memory type.
    // Fixed at bind time — querying it per flush cost ~5–15 µs/frame
    // across the ~10 per-frame mapped writes (#498).
    is_coherent: bool,
    /// Stashed at construction so `Drop` can self-free if the
    /// canonical `destroy(device, allocator)` path is missed.
    /// `ash::Device` is `Arc`-backed and `SharedAllocator` is already
    /// `Arc<Mutex<…>>`, so cloning is cheap. Sibling fix to
    /// `Texture` (#656) — same release-build leak otherwise.
    device: ash::Device,
    /// `Option` so `destroy()` can release the Arc clone immediately
    /// after freeing the underlying allocation. Pre-#927 this was a
    /// bare `SharedAllocator`; the Arc stayed alive until the
    /// GpuBuffer struct itself naturally dropped, which on
    /// `VulkanContext::Drop` happens *after* `Arc::try_unwrap` has
    /// already given up — driving the "GPU allocator has N
    /// outstanding references" leak path. Once `allocation` is `None`
    /// the allocator is no longer needed (`Drop` short-circuits the
    /// self-clean), so dropping the Arc here is safe.
    allocator: Option<SharedAllocator>,
}

impl GpuBuffer {
    /// Create several device-local buffers through one packed staging arena
    /// and one submit/fence cycle.
    ///
    /// Exterior precombined NIFs routinely contain 100-200 submeshes. The
    /// scalar upload path submits once for each vertex buffer and once for
    /// each index buffer; this helper preserves distinct destination buffers
    /// for BLAS ownership while collapsing all of their copies into one GPU
    /// transaction.
    pub fn create_device_local_buffers_batched(
        ctx: GpuUploadCtx,
        uploads: &[DeviceLocalBufferUpload<'_>],
        mut staging_pool: Option<&mut StagingPool>,
        transfer_fence: &std::sync::Mutex<vk::Fence>,
    ) -> Result<Vec<Self>> {
        if uploads.is_empty() {
            return Ok(Vec::new());
        }
        let sizes = uploads
            .iter()
            .map(|upload| upload.bytes.len() as vk::DeviceSize)
            .collect::<Vec<_>>();
        let (offsets, staging_size) = packed_copy_layout(&sizes)?;

        let GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        } = ctx;
        let (staging_buffer, staging_alloc) = if let Some(pool) = staging_pool.as_deref_mut() {
            pool.acquire(staging_size)?
        } else {
            create_staging_buffer(device, allocator, staging_size, "batched_buffer_staging")?
        };
        let mut staging = StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );
        {
            let mapped = staging.mapped_slice_mut()?;
            for (upload, &offset) in uploads.iter().zip(&offsets) {
                let start = offset as usize;
                let end = start + upload.bytes.len();
                mapped[start..end].copy_from_slice(upload.bytes);
            }
        }
        staging.flush_if_needed(device)?;

        let mut buffers = Vec::with_capacity(uploads.len());
        for (index, upload) in uploads.iter().enumerate() {
            match Self::create_device_local_uninit(
                device,
                allocator,
                sizes[index],
                upload.usage | vk::BufferUsageFlags::TRANSFER_DST,
            ) {
                Ok(buffer) => buffers.push(buffer),
                Err(error) => {
                    for mut buffer in buffers {
                        buffer.destroy(device, allocator);
                    }
                    return Err(error).context("create batched device-local destination");
                }
            }
        }

        let record_result = with_one_time_commands_reuse_fence(
            device,
            queue,
            command_pool,
            transfer_fence,
            |cmd| {
                for ((buffer, &src_offset), &size) in buffers.iter().zip(&offsets).zip(&sizes) {
                    unsafe {
                        // SAFETY: `cmd` is recording; staging and every
                        // destination are live with TRANSFER_SRC/DST usage;
                        // `packed_copy_layout` produces 4-byte-aligned,
                        // non-overlapping source ranges whose sizes match the
                        // corresponding destination buffers.
                        device.cmd_copy_buffer(
                            cmd,
                            staging.buffer,
                            buffer.buffer,
                            &[vk::BufferCopy {
                                src_offset,
                                dst_offset: 0,
                                size,
                            }],
                        );
                    }
                }
                Ok(())
            },
        );
        if let Err(error) = record_result {
            // A submit/wait failure does not tell us whether the command is
            // still executing. Conservatively leak both sides instead of
            // risking a host-side destroy racing an in-flight transfer. This
            // is a fatal-style GPU failure path; retaining the allocations is
            // safer than attempting recovery with ambiguous queue ownership.
            std::mem::forget(staging);
            std::mem::forget(buffers);
            return Err(error).context("submit batched device-local uploads");
        }

        if let Some(pool) = staging_pool {
            let capacity = staging
                .allocation
                .as_ref()
                .map(|allocation| allocation.size())
                .unwrap_or(staging_size);
            staging.release_to(pool, capacity);
        } else {
            staging.destroy();
        }
        Ok(buffers)
    }

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
        let size = std::mem::size_of_val(data) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::VERTEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(
            GpuUploadCtx {
                device,
                allocator,
                queue,
                command_pool,
            },
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
        let size = std::mem::size_of_val(data) as vk::DeviceSize;
        let mut usage = vk::BufferUsageFlags::INDEX_BUFFER;
        if rt_enabled {
            usage |= vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        }
        Self::create_device_local_buffer(
            GpuUploadCtx {
                device,
                allocator,
                queue,
                command_pool,
            },
            size,
            usage,
            data,
            staging_pool,
        )
    }

    /// Create a host-visible buffer for per-frame CPU writes (no staging needed).
    /// Used for SSBO/UBO data that changes every frame.
    ///
    /// **Audit guard (#423):** every call site of this function MUST be
    /// per-frame mutable (UBOs / per-frame SSBOs / instance buffers).
    /// Static-data buffers (vertex / index / global geometry SSBO)
    /// must instead use [`Self::create_device_local_buffer`] (or the
    /// `create_vertex_buffer` / `create_index_buffer` wrappers), which
    /// stage→copy to `MemoryLocation::GpuOnly` and avoid the small
    /// pinned-host-visible BAR heap (~256 MB on most NVIDIA parts).
    /// Routing static data through this function would exhaust the BAR
    /// heap on cells with thousands of unique meshes long before VRAM
    /// runs out. Verified survey 2026-04-18: every caller (ssao /
    /// taa / caustic / svgf / composite param_buffers, scene_buffer's
    /// light/camera/bone/instance/indirect, acceleration's TLAS
    /// instance staging) is per-frame mutable.
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

        // SAFETY: `device` is the caller's live logical device, valid for the
        // call; `buffer_info` is a fully-populated valid VkBufferCreateInfo.
        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create host-visible buffer")?
        };

        // SAFETY: `buffer` was just created by this device above and is live;
        // the device outlives the call.
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
        // MEM-2-5 / #680 — every per-frame buffer (lights, camera, bones,
        // instances, indirect, ray-budget, TLAS instance staging) reaches
        // `mapped_slice_mut` on its hot path. Catch a regression in the
        // allocator's mapping policy at construction, not on the first
        // write.
        debug_assert_cpu_to_gpu_mapped(&allocation, "create_host_visible");

        // SAFETY: `buffer` and `allocation` were both created here from this
        // device; the memory/offset come from the allocation that satisfied
        // `buffer`'s memory requirements, and the buffer is not yet bound.
        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind host-visible buffer")?;
        }

        let is_coherent = allocation
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
            is_coherent,
            device: device.clone(),
            allocator: Some(allocator.clone()),
        })
    }

    /// Create a host-visible buffer for device→host **readback**.
    ///
    /// #2752 / REN-D4-05 — the readback sibling of [`Self::create_host_visible`],
    /// which pins `MemoryLocation::CpuToGpu`. The two locations are not
    /// interchangeable: `CpuToGpu` exists for staging *uploads*, and
    /// gpu-allocator resolves it toward `HOST_VISIBLE | HOST_COHERENT` —
    /// frequently device-local BAR memory on a discrete card, i.e. uncached
    /// write-combined from the CPU's point of view. `GpuToCpu` is the
    /// readback preset and additionally prefers `HOST_CACHED`, which is what
    /// a buffer the host drains every frame on the hot path actually wants.
    ///
    /// Kept as a separate constructor rather than a `location` parameter on
    /// `create_host_visible` so that function's #423 audit guard ("every call
    /// site MUST be per-frame mutable upload") keeps its meaning — a reader
    /// checking either constructor's call sites is checking one intent, not
    /// two.
    ///
    /// **`HOST_CACHED` is exactly the case where a missing invalidate becomes
    /// observable**, so every caller must run [`Self::invalidate_if_needed`]
    /// after the fence wait and before reading through
    /// [`Self::mapped_slice_mut`]. That ordering is the whole point of this
    /// pair — see #2740 (REN-D4-04) for why a fence alone does not make
    /// device writes visible to the host.
    pub fn create_host_readback(
        device: &ash::Device,
        allocator: &SharedAllocator,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
    ) -> Result<Self> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: `device` is the caller's live logical device, valid for the
        // call; `buffer_info` is a fully-populated valid VkBufferCreateInfo.
        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create host-readback buffer")?
        };

        // SAFETY: `buffer` was just created by this device above and is live;
        // the device outlives the call.
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };

        let allocation = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&vulkan::AllocationCreateDesc {
                name: "host_readback_buffer",
                requirements,
                location: MemoryLocation::GpuToCpu,
                linear: true,
                allocation_scheme: vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate host-readback memory")?;
        // Same mapping-policy tripwire as the upload path: every reader
        // reaches `mapped_slice_mut` on its hot path, so catch a regression
        // at construction rather than on the first drain.
        debug_assert_cpu_to_gpu_mapped(&allocation, "create_host_readback");

        // SAFETY: `buffer` and `allocation` were both created here from this
        // device; the memory/offset come from the allocation that satisfied
        // `buffer`'s memory requirements, and the buffer is not yet bound.
        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind host-readback buffer")?;
        }

        let is_coherent = allocation
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT);

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
            is_coherent,
            device: device.clone(),
            allocator: Some(allocator.clone()),
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

        // SAFETY: `device` is the caller's live logical device, valid for the
        // call; `buffer_info` is a fully-populated valid VkBufferCreateInfo.
        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create device-local buffer")?
        };

        // SAFETY: `buffer` was just created by this device above and is live;
        // the device outlives the call.
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

        // SAFETY: `buffer` and `allocation` were both created here from this
        // device; the memory/offset come from the allocation that satisfied
        // `buffer`'s memory requirements, and the buffer is not yet bound.
        unsafe {
            device
                .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
                .context("Failed to bind device-local buffer")?;
        }

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
            // DEVICE_LOCAL without HOST_VISIBLE is never mapped; flush paths
            // don't run on it. Value is inert.
            is_coherent: false,
            device: device.clone(),
            allocator: Some(allocator.clone()),
        })
    }

    /// Get the mapped memory slice for direct writes (no intermediate Vec).
    /// Call `flush_if_needed()` after writing to ensure GPU visibility.
    ///
    /// #2793 (REN-D5-05) described the gap that #2752 then closed: a caller
    /// that READS GPU-written bytes back through this slice (an
    /// atomic-counter buffer the shader writes and the host later drains)
    /// needs the device→host counterpart, not `flush_if_needed()`. That
    /// counterpart now exists — [`Self::invalidate_if_needed`] — and every
    /// reader must call it after the fence wait and before reading here.
    /// The image-health buffers were the case in point: they are allocated
    /// via [`Self::create_host_readback`] (`GpuToCpu`, which prefers
    /// `HOST_CACHED`) precisely because they are drained every frame, and
    /// `HOST_CACHED` is the memory type where skipping the invalidate stops
    /// being theoretical.
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

        if !self.is_coherent {
            let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
            debug_assert_flush_range_bounded(alloc, aligned_offset, aligned_size);
            // SAFETY: alloc.memory() is valid and mapped. `aligned_flush_range`
            // rounds the offset DOWN and the size UP to `NON_COHERENT_ATOM_SIZE`,
            // so the flushed range is a strict SUPERSET of this allocation, not a
            // subset — it is bounded only by the parent gpu-allocator memory
            // block, not by `alloc`'s own `[offset, offset+size)` slice.
            // gpu-allocator 0.28 has no `nonCoherentAtomSize` awareness at all
            // (verified against the vendored source — zero occurrences of
            // `non_coherent`/`atom_size`); it sub-allocates purely on
            // `VkMemoryRequirements.alignment`, so nothing pads sub-allocations
            // to this boundary on its behalf. What actually keeps this sound
            // today: every call site here uses `AllocationScheme::
            // GpuAllocatorManaged`, so `alloc.memory()` backs a multi-MB shared
            // block and the widened range still lands inside it — see #2683 /
            // SAFE-D4-01 and `debug_assert_flush_range_bounded`'s doc for the
            // one case (a dedicated allocation) that's actually checkable.
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

    /// Invalidate mapped memory if not HOST_COHERENT. Call BEFORE reading
    /// device-written bytes back through [`Self::mapped_slice_mut`].
    ///
    /// #2752 / REN-D4-05 — the device→host counterpart to
    /// [`Self::flush_if_needed`], and the prerequisite that let the
    /// image-health buffers move to `MemoryLocation::GpuToCpu`. Waiting on a
    /// fence proves the submission *completed*; it does not make the device's
    /// writes *visible to the host*, because a fence's memory-dependency
    /// access scope covers device access only (#2740 / REN-D4-04). On a
    /// `HOST_CACHED` allocation — exactly what `GpuToCpu` prefers — the host
    /// can otherwise read stale cache lines.
    ///
    /// A no-op on coherent memory, which is what the dev card resolves to, so
    /// this is insurance against a memory-type change rather than something
    /// observable here. That is also why it cannot be falsified by
    /// `cargo test`: it needs a device that hands back a non-coherent
    /// readback type, or a validation-layer run.
    pub fn invalidate_if_needed(&mut self, device: &ash::Device) -> Result<()> {
        let alloc = self
            .allocation
            .as_ref()
            .context("Buffer has no allocation")?;

        if !self.is_coherent {
            let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset(), alloc.size());
            debug_assert_flush_range_bounded(alloc, aligned_offset, aligned_size);
            // SAFETY: identical range-widening argument to `flush_if_needed`
            // above — `aligned_flush_range` rounds the offset DOWN and the
            // size UP to `NON_COHERENT_ATOM_SIZE`, producing a strict
            // SUPERSET of this allocation that stays inside the parent
            // `GpuAllocatorManaged` block. Invalidating a superset is safe in
            // a way flushing one is not: it discards possibly-stale host
            // cache lines for a neighbouring suballocation, forcing a re-read
            // from memory, rather than publishing this allocation's bytes
            // over anything. `IMAGE_HEALTH_BUFFER_BYTES` is 8, far below a
            // typical 64-byte `nonCoherentAtomSize`, so the widening is the
            // normal case here, not the edge case.
            unsafe {
                let range = vk::MappedMemoryRange::default()
                    .memory(alloc.memory())
                    .offset(aligned_offset)
                    .size(aligned_size);
                device
                    .invalidate_mapped_memory_ranges(&[range])
                    .context("Failed to invalidate mapped memory")?;
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
        let is_coherent = self.is_coherent;
        let alloc = self
            .allocation
            .as_mut()
            .context("Buffer has no allocation")?;

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
            // Flush only the range actually written (capped at `len`), not
            // the entire allocation. The instance SSBO is 1.28 MB but a
            // typical frame writes only a few KB — flushing the full range
            // wastes bandwidth on non-coherent memory (#301).
            //
            // Vulkan spec requires both offset and size to be multiples of
            // nonCoherentAtomSize (or size == VK_WHOLE_SIZE); aligned_flush_range
            // rounds outward to satisfy that.
            let (aligned_offset, aligned_size) =
                aligned_flush_range(alloc.offset(), len as vk::DeviceSize);
            debug_assert_flush_range_bounded(alloc, aligned_offset, aligned_size);

            // SAFETY: alloc.memory() returns the VkDeviceMemory backing this
            // allocation, which is valid and mapped (verified above).
            // `aligned_size` is rounded UP from `len` (the written length) to
            // `NON_COHERENT_ATOM_SIZE` with no cap — `aligned_flush_range`
            // never clamps to `alloc.size()` — so the flushed range can extend
            // past this allocation's own end, bounded only by the parent
            // gpu-allocator memory block (see #2683 / SAFE-D4-01 and
            // `debug_assert_flush_range_bounded`'s doc). Sound today because
            // every call site uses `AllocationScheme::GpuAllocatorManaged`,
            // whose shared multi-MB block absorbs the overshoot.
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

    /// Flush a specific range of mapped memory if not HOST_COHERENT.
    ///
    /// Use this after direct writes via [`mapped_slice_mut`] when you know
    /// how many bytes were actually written — avoids flushing the entire
    /// allocation. `offset` and `size` are relative to the start of this
    /// buffer's allocation (not absolute device memory).
    pub fn flush_range(
        &mut self,
        device: &ash::Device,
        offset: vk::DeviceSize,
        size: vk::DeviceSize,
    ) -> Result<()> {
        let alloc = self
            .allocation
            .as_ref()
            .context("Buffer has no allocation")?;

        if self.is_coherent || size == 0 {
            return Ok(());
        }

        let (aligned_offset, aligned_size) = aligned_flush_range(alloc.offset() + offset, size);
        debug_assert_flush_range_bounded(alloc, aligned_offset, aligned_size);

        // SAFETY: alloc.memory() is valid and mapped. Caller is responsible
        // for `offset + size <= alloc.size()`, but the ACTUALLY flushed range
        // is `aligned_flush_range`'s outward-rounded superset of
        // `[offset, offset+size)`, not that range itself — it can extend past
        // this allocation's own end, bounded only by the parent gpu-allocator
        // memory block (see #2683 / SAFE-D4-01 and
        // `debug_assert_flush_range_bounded`'s doc). Sound today because
        // every call site uses `AllocationScheme::GpuAllocatorManaged`, whose
        // shared multi-MB block absorbs the overshoot.
        unsafe {
            let range = vk::MappedMemoryRange::default()
                .memory(alloc.memory())
                .offset(aligned_offset)
                .size(aligned_size);
            device
                .flush_mapped_memory_ranges(&[range])
                .context("Failed to flush mapped memory")?;
        }
        Ok(())
    }

    /// Destroy the buffer and free its GPU memory.
    /// Must be called before the device is destroyed.
    pub fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if let Some(allocation) = self.allocation.take() {
            // SAFETY: `self.buffer` was created by this device and has not yet
            // been destroyed (guarded by the `allocation.take()` above, which
            // runs once); the caller guarantees the device is idle before destroy.
            unsafe {
                device.destroy_buffer(self.buffer, None);
            }
            // #2487 / D5-02 — null the handle, matching the convention the
            // sibling helpers already follow (`destroy_depth_resources`,
            // `TextureRegistry::destroy`). The `allocation.take()` above
            // already prevents a double-free, and Drop already short-circuits;
            // what was undefended was a *read*. A caller that destroys through
            // a long-lived `&mut GpuBuffer` and later binds `.buffer` (a `pub`
            // field) got a destroyed handle with no way to tell — a class of
            // bug invisible to `cargo test` and visible only as a
            // validation-layer complaint or a GPU fault. `VK_NULL_HANDLE` at
            // least makes the misuse loud and its own destroy a documented
            // no-op.
            self.buffer = vk::Buffer::null();
            allocator
                .lock()
                .expect("allocator lock poisoned")
                .free(allocation)
                .expect("Failed to free GPU allocation");
        }
        // #927 — release the stored allocator Arc clone now that the
        // GPU side is freed. Without this, the struct's Arc clone
        // outlives `destroy()` and only releases on natural Drop of
        // the owning container — which on shutdown happens *after*
        // `VulkanContext::Drop`'s `Arc::try_unwrap`, triggering the
        // outstanding-refs leak path. Drop's safety-net branch
        // (only hit when destroy() was skipped) handles the None case.
        self.allocator = None;
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Create a DEVICE_LOCAL buffer and upload data via a CpuToGpu staging buffer.
    pub fn create_device_local_buffer<T: Copy>(
        ctx: GpuUploadCtx,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        data: &[T],
        mut staging_pool: Option<&mut StagingPool>,
    ) -> Result<Self> {
        let GpuUploadCtx {
            device,
            allocator,
            queue,
            command_pool,
        } = ctx;
        // 1. Acquire staging buffer — from pool (reuse) or create fresh.
        // #2164 / L-5 — the fresh path unwinds its own create/allocate/bind
        // window inside `create_staging_buffer`; the guard below covers
        // everything after.
        let (staging_buffer, staging_alloc) = if let Some(pool) = staging_pool.as_deref_mut() {
            pool.acquire(size)?
        } else {
            create_staging_buffer(device, allocator, size, "buffer_staging")?
        };

        // Wrap staging resources in RAII guard — ensures cleanup on early
        // return. Constructed BEFORE the host write (#2164 / L-5): the
        // `mapped_slice_mut` failure path used to run while both the buffer
        // and the allocation were still owned by bare locals.
        let mut staging = StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );

        // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
        // valid and aligned (from a live slice), and size_of_val gives the
        // exact byte length. The borrow is bounded by this scope.
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
        };

        staging.mapped_slice_mut()?[..bytes.len()].copy_from_slice(bytes);

        // 2. Create the device-local buffer (GPU_ONLY).
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage | vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: `device` is the caller's live logical device, valid for the
        // call; `buffer_info` is a fully-populated valid VkBufferCreateInfo.
        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .context("Failed to create device-local buffer")?
        };

        // SAFETY: `buffer` was just created by this device above and is live;
        // the device outlives the call.
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

        // SAFETY: `buffer` and `allocation` were both created here from this
        // device; the memory/offset come from the allocation that satisfied
        // `buffer`'s memory requirements, and the buffer is not yet bound.
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
            // SAFETY: `cmd` is in the recording state for the duration of the
            // `with_one_time_commands` closure; `staging.buffer` and `buffer` are
            // both live, distinct, and have matching TRANSFER_SRC/DST usage; the
            // copy region (offset 0, `size` bytes) was sized to both allocations
            // and no other access to either buffer races this command.
            unsafe {
                device.cmd_copy_buffer(cmd, staging.buffer, buffer, &[copy_region]);
            }
            Ok(())
        })?;

        // 4. Release staging resources. When a pool was provided, hand
        //    the buffer back for reuse; otherwise destroy outright.
        //    Pre-#239 this always called `destroy` — which meant even
        //    callers that passed a pool never got any reuse, because
        //    each "pooled" acquire was followed by a destroy. See the
        //    #239 investigation for the full premise verification.
        if let Some(pool) = staging_pool {
            let capacity = staging
                .allocation
                .as_ref()
                .map(|a| a.size())
                .unwrap_or(size);
            staging.release_to(pool, capacity);
        } else {
            staging.destroy();
        }

        Ok(Self {
            buffer,
            size,
            allocation: Some(allocation),
            // DEVICE_LOCAL staging target — never mapped by the owner.
            is_coherent: false,
            device: device.clone(),
            allocator: Some(allocator.clone()),
        })
    }
}

/// One destination buffer in a packed upload transaction.
pub struct DeviceLocalBufferUpload<'a> {
    pub bytes: &'a [u8],
    pub usage: vk::BufferUsageFlags,
}

/// Four-byte-align packed copy regions and return `(offsets, total_size)`.
/// `vkCmdCopyBuffer` requires source/destination offsets and sizes to be
/// multiples of four; mesh vertex/index payloads naturally satisfy the size
/// half of that contract.
fn packed_copy_layout(sizes: &[vk::DeviceSize]) -> Result<(Vec<vk::DeviceSize>, vk::DeviceSize)> {
    let mut offsets = Vec::with_capacity(sizes.len());
    let mut cursor = 0u64;
    for &size in sizes {
        if size == 0 || size % 4 != 0 {
            bail!("batched buffer upload size must be non-zero and 4-byte aligned (got {size})");
        }
        cursor = cursor
            .checked_add(3)
            .context("batched buffer upload offset overflow")?
            & !3;
        offsets.push(cursor);
        cursor = cursor
            .checked_add(size)
            .context("batched buffer upload size overflow")?;
    }
    Ok((offsets, cursor))
}

impl Drop for GpuBuffer {
    /// Safety net mirroring `Texture::Drop` (#656). When the canonical
    /// path called `destroy(device, allocator)`, `allocation` is None
    /// and Drop is a no-op. When something escapes the canonical
    /// destroy chain (panic, ad-hoc construction, future code path)
    /// Drop self-cleans using the stashed handles. Pre-fix release
    /// builds dropped the `Allocation` on the floor and leaked the
    /// VkBuffer plus the gpu_allocator slab.
    fn drop(&mut self) {
        if self.allocation.is_none() {
            return;
        }
        log::warn!(
            "GpuBuffer dropped without destroy() — running cleanup from Drop (#656 safety net)",
        );
        // Skip the assert during unwind so a panic mid-construction (e.g.
        // allocator lock poisoned partway through a multi-buffer init) doesn't
        // compound into N nested debug_assert panics that clobber the original
        // panic's stack. See #1128 / REN-D4-NEW-01.
        if !std::thread::panicking() {
            debug_assert!(false, "GpuBuffer leaked into Drop: call destroy() first");
        }
        // SAFETY: `self.buffer` was created by this device and has not been
        // destroyed (this Drop arm only runs when `allocation` is still
        // `Some`, i.e. `destroy()` was never called); reached only on the
        // leak-safety-net path, where the device is still alive.
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
        }
        // #2487 / D5-02 — same nulling as `destroy()`. Redundant on the Drop
        // path (nothing can read the field afterwards), kept so the two
        // teardown arms stay literally identical and neither can drift into
        // being the one that leaves a live-looking handle behind.
        self.buffer = vk::Buffer::null();
        if let Some(alloc) = self.allocation.take() {
            // Invariant: if `allocation` was `Some`, `allocator` is
            // also `Some` — `destroy()` clears them together. Hitting
            // None here would mean the buffer escaped destroy() AND
            // had its allocator cleared independently, which is not
            // a path the rest of the code takes.
            let Some(allocator) = self.allocator.as_ref() else {
                log::error!(
                    "GpuBuffer::Drop has live allocation but no allocator — \
                     slab leaks (was destroy() partially invoked?)",
                );
                return;
            };
            match allocator.lock() {
                Ok(mut a) => {
                    if let Err(e) = a.free(alloc) {
                        log::error!("GpuBuffer::Drop failed to free allocation: {e}");
                    }
                }
                Err(_) => {
                    log::error!(
                        "GpuBuffer::Drop saw a poisoned allocator mutex — slab leaks deliberately to avoid double-panic",
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod staging_guard_coverage_tests {
    //! REN-LOW L-5 / #2164 — every staging upload path must funnel its
    //! buffer through [`create_staging_buffer`] (which unwinds the
    //! pre-guard create/allocate/bind window) and then a
    //! [`StagingGuard`] (which covers every `?` after it).
    //!
    //! Static source checks: the real failure needs an allocator OOM or a
    //! `vkBindBufferMemory` failure against a live device, neither of
    //! which is reachable from `cargo test`. Pinning the *shape* is what
    //! stops the pattern being open-coded back in — the leak existed for
    //! as long as it did precisely because nothing flagged the shape.

    const UPLOAD_SRC: &str = include_str!("scene_buffer/upload.rs");
    const TEXTURE_SRC: &str = include_str!("texture.rs");

    /// The two consumer sites. `buffer.rs` itself is excluded: it *hosts*
    /// `create_staging_buffer` and `StagingPool`, the two legitimate
    /// creators of a `TRANSFER_SRC` buffer in this crate.
    fn consumer_sites() -> [(&'static str, &'static str); 2] {
        [
            ("scene_buffer/upload.rs", UPLOAD_SRC),
            ("texture.rs", TEXTURE_SRC),
        ]
    }

    /// No consumer may open-code the staging prologue. Creating a
    /// `TRANSFER_SRC` buffer outside `create_staging_buffer` is the exact
    /// shape that leaks: the `VkBuffer` exists, but nothing owns it until
    /// the allocation lands several fallible steps later.
    #[test]
    fn no_consumer_open_codes_the_staging_prologue() {
        for (name, src) in consumer_sites() {
            assert!(
                !src.contains("TRANSFER_SRC"),
                "{name}: a staging buffer is created outside `create_staging_buffer` — \
                 the create→allocate→bind window leaks the VkBuffer on any failure \
                 (REN-LOW L-5 / #2164). Route it through `create_staging_buffer`."
            );
        }
    }

    /// `upload_terrain_tiles` — the site the audit named — must tear down
    /// through the guard, not a hand-rolled destroy/free pair.
    #[test]
    fn upload_terrain_tiles_tears_down_through_the_guard() {
        let fn_start = UPLOAD_SRC
            .find("pub fn upload_terrain_tiles")
            .expect("upload_terrain_tiles not found");
        let body = &UPLOAD_SRC[fn_start..];
        let fn_end = body
            .find("\n    /// Get the light buffers")
            .unwrap_or(body.len());
        let body = &body[..fn_end];

        assert!(
            body.contains("StagingGuard::new"),
            "upload_terrain_tiles must wrap staging in a StagingGuard immediately \
             after acquiring it — the three fallible steps that follow used to leak \
             it (REN-LOW L-5 / #2164)"
        );
        assert!(
            body.contains("staging.destroy()"),
            "upload_terrain_tiles must release staging via `StagingGuard::destroy`"
        );
        assert!(
            !body.contains("destroy_buffer"),
            "upload_terrain_tiles must not hand-roll `destroy_buffer` — that spelling \
             is only reached on the success path and is what left the failure paths \
             leaking (REN-LOW L-5 / #2164)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// #927 — pins the `Option<SharedAllocator>` mechanism that
    /// `GpuBuffer::destroy()` and `Texture::destroy()` rely on. When
    /// `destroy()` sets the Option to `None`, the wrapped `Arc` must
    /// drop immediately, releasing the strong-count slot. Pre-fix the
    /// field was a bare `SharedAllocator`, so the Arc stayed alive
    /// until the GpuBuffer struct itself naturally dropped — which on
    /// shutdown happens *after* `VulkanContext::Drop`'s
    /// `Arc::try_unwrap`, contributing to the "GPU allocator has N
    /// outstanding references" leak path. This test pins the language
    /// semantics; the real regression check is the absence of the
    /// "outstanding references" error log on engine shutdown.
    #[test]
    fn option_arc_dropped_when_set_to_none() {
        let shared: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
        let mut held: Option<Arc<Mutex<()>>> = Some(Arc::clone(&shared));
        assert!(held.is_some(), "fixture: Option holds the cloned Arc");
        assert_eq!(
            Arc::strong_count(&shared),
            2,
            "test fixture: original + Option's clone"
        );
        held = None;
        assert!(held.is_none(), "destroy() sets the Option to None");
        assert_eq!(
            Arc::strong_count(&shared),
            1,
            "setting Option<Arc<_>> to None must drop the wrapped Arc immediately — \
             the contract `GpuBuffer::destroy()` and `Texture::destroy()` rely on"
        );
    }

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

    #[test]
    fn packed_copy_layout_aligns_and_preserves_order() {
        let (offsets, total) = packed_copy_layout(&[104, 12, 208, 24]).unwrap();
        assert_eq!(offsets, vec![0, 104, 116, 324]);
        assert_eq!(total, 348);
        assert!(offsets.iter().all(|offset| offset % 4 == 0));
    }

    #[test]
    fn packed_copy_layout_rejects_invalid_copy_sizes() {
        assert!(packed_copy_layout(&[0]).is_err());
        assert!(packed_copy_layout(&[6]).is_err());
    }

    /// Regression for #2683 (SAFE-D4-01) — `aligned_flush_range`'s outward
    /// rounding produces a range that is a strict SUPERSET of the input
    /// `[offset, offset+size)`, never a subset. The pre-fix SAFETY comments
    /// claimed the opposite ("contained within this allocation's slice").
    /// Pin the actual relationship directly against the function under
    /// test, independent of any `Allocation` fixture.
    #[test]
    fn aligned_flush_range_widens_outward_never_inward() {
        for (offset, size) in [(100u64, 200u64), (50, 50), (0, 48), (37, 999)] {
            let (aligned_offset, aligned_size) = aligned_flush_range(offset, size);
            assert!(
                aligned_offset <= offset,
                "aligned_offset must round DOWN, never up past the real start"
            );
            assert!(
                aligned_offset + aligned_size >= offset + size,
                "aligned end must round UP, never down past the real end"
            );
        }
    }

    /// #2752 (REN-D4-05) — `IMAGE_HEALTH_BUFFER_BYTES` is 8, far below the
    /// 256-byte `NON_COHERENT_ATOM_SIZE`, so for the readback buffer this fix
    /// is about, the widening is the *normal* case rather than an edge case:
    /// an 8-byte invalidate always covers a whole atom. That is sound for an
    /// invalidate (it only discards possibly-stale host cache lines, forcing
    /// a re-read) in a way it would not be for a flush, and it is why the
    /// invalidate path can share `aligned_flush_range` unchanged.
    #[test]
    fn eight_byte_readback_range_widens_to_a_whole_atom() {
        let (aligned_offset, aligned_size) = aligned_flush_range(0, 8);
        assert_eq!(aligned_offset, 0);
        assert_eq!(
            aligned_size, NON_COHERENT_ATOM_SIZE,
            "an 8-byte counter buffer invalidates a full atom, not 8 bytes"
        );

        // Mid-block placement: gpu-allocator suballocates on
        // `VkMemoryRequirements.alignment`, so the image-health buffer will
        // not generally start atom-aligned.
        let (aligned_offset, aligned_size) = aligned_flush_range(NON_COHERENT_ATOM_SIZE + 8, 8);
        assert_eq!(aligned_offset, NON_COHERENT_ATOM_SIZE);
        assert!(
            aligned_offset + aligned_size >= NON_COHERENT_ATOM_SIZE + 16,
            "the widened range must still cover the real 8 bytes"
        );
    }

    /// #2752 — the readback pair has to stay wired the way the fix left it,
    /// and none of it is reachable from a unit test: the allocation location
    /// needs a device, and on the dev card gpu-allocator resolves both
    /// presets to coherent memory, so a missing invalidate is invisible
    /// anyway (that's what #2740 means by "needs a device to falsify").
    /// Source-scan is the honest pin — it catches the realistic regression,
    /// which is someone copying `create_host_visible` into a new readback
    /// site or dropping the invalidate as redundant.
    #[test]
    fn readback_constructor_uses_the_readback_memory_location() {
        let src = include_str!("buffer.rs");
        let start = src
            .find("pub fn create_host_readback(")
            .expect("create_host_readback must still exist (#2752)");
        let body = &src[start..];
        let end = body
            .find("pub fn create_device_local_uninit(")
            .expect("create_host_readback must still be followed by its sibling");
        let body = &body[..end];
        assert!(
            body.contains("MemoryLocation::GpuToCpu"),
            "the readback constructor must allocate GpuToCpu — CpuToGpu is \
             the upload preset and steers toward uncached write-combined BAR"
        );
        assert!(
            !body.contains("MemoryLocation::CpuToGpu"),
            "no CpuToGpu allocation may survive inside the readback constructor"
        );

        // The invalidate primitive is the prerequisite that made the location
        // switch safe; it must not be deleted as an apparent no-op.
        assert!(
            src.contains("pub fn invalidate_if_needed("),
            "GpuBuffer::invalidate_if_needed is what makes a non-coherent \
             readback allocation sound (#2740 / REN-D4-04)"
        );
        assert!(
            src.contains("invalidate_mapped_memory_ranges"),
            "invalidate_if_needed must actually issue vkInvalidateMappedMemoryRanges"
        );
    }

    /// Regression for #2683 (SAFE-D4-01) — `flush_range_within` (the pure
    /// arithmetic `debug_assert_flush_range_bounded` gates its assert on)
    /// must accept a range that stays inside `[alloc_offset,
    /// alloc_offset+alloc_size)` and reject one that overshoots either end.
    #[test]
    fn flush_range_within_accepts_contained_and_rejects_overshoot() {
        // Exact match — the tightest possible "contained" case.
        assert!(flush_range_within(0, 256, 0, 256));
        // Strictly inside.
        assert!(flush_range_within(64, 128, 0, 256));
        // Overshoots the end — exactly the bug `aligned_flush_range`'s
        // outward-rounding size can produce against a dedicated allocation.
        assert!(!flush_range_within(0, 512, 0, 256));
        // Undershoots the start — defensive; not reachable via
        // `aligned_flush_range` today (offset is unsigned and
        // `alloc_offset` is always 0 for a dedicated allocation, per the
        // block-sizing argument in `debug_assert_flush_range_bounded`'s
        // doc), but the helper must still catch it if that invariant ever
        // changes upstream.
        assert!(!flush_range_within(0, 128, 64, 256));
    }

    /// #1759 / TD7-002 — `aligned_flush_range` rounds with the bitmask trick
    /// `offset & !(N-1)` / `(size + N-1) & !(N-1)`, which is only a correct
    /// round-down / round-up when `N` is a power of two. Pin that invariant
    /// on the constant so a future "round number" tweak (e.g. 192) that
    /// breaks the masking can't ship silently. The companion device-create
    /// `debug_assert!` (device.rs) pins the other half: every real device's
    /// reported `nonCoherentAtomSize` stays `<=` this value.
    #[test]
    fn non_coherent_atom_size_is_power_of_two() {
        assert!(
            NON_COHERENT_ATOM_SIZE.is_power_of_two(),
            "aligned_flush_range's `& !(N-1)` masking requires a power-of-two \
             NON_COHERENT_ATOM_SIZE; got {NON_COHERENT_ATOM_SIZE}",
        );
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
        // 128 MiB (#511, was 64 MiB pre-#239) → must evict enough to
        // fit under the budget.
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

#[cfg(test)]
mod destroyed_handle_nulling_tests {
    //! #2487 / D5-02 — after `destroy()`, a `GpuBuffer`'s `pub buffer` field
    //! (and a `Texture`'s `pub image` / `image_view`) must read as
    //! `VK_NULL_HANDLE`, not as the destroyed handle. Constructing either
    //! type needs a live `ash::Device` and allocator, so this pins the shape
    //! at the source level — the same approach `staging_guard_coverage_tests`
    //! above takes for the leak window it guards.

    /// Everything before the first test module, so this file's own
    /// assertion needles cannot match themselves.
    fn production_src(src: &'static str) -> &'static str {
        src.split_once("\n#[cfg(test)]")
            .expect("source lost its test modules")
            .0
    }

    /// Both teardown arms of each type: the explicit `destroy()` and the
    /// `Drop` safety net. Counting occurrences (rather than just checking
    /// for one) is what keeps the two arms in lockstep — the defect was
    /// exactly one arm having the nulling the other lacked.
    #[test]
    fn every_teardown_arm_nulls_its_handles() {
        let buffer_src = production_src(include_str!("buffer.rs"));
        let texture_src = production_src(include_str!("texture.rs"));
        for (name, src, needle, expected) in [
            (
                "buffer.rs",
                buffer_src,
                "self.buffer = vk::Buffer::null();",
                2,
            ),
            (
                "texture.rs",
                texture_src,
                "self.image = vk::Image::null();",
                2,
            ),
            (
                "texture.rs",
                texture_src,
                "self.image_view = vk::ImageView::null();",
                2,
            ),
        ] {
            assert_eq!(
                src.matches(needle).count(),
                expected,
                "{name}: expected {expected} `{needle}` (one per teardown arm: \
                 destroy() and the Drop safety net). A destroyed handle left in \
                 a pub field is invisible to cargo test and surfaces only as a \
                 validation-layer complaint or a GPU fault (#2487)."
            );
        }
    }
}
