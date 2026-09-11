//! Release and deferred destroy.
//!
//! A texture's GPU memory cannot be freed the moment its last reference
//! drops — a command buffer still in flight may reference the bindless
//! slot. Everything here feeds the `pending_destroy` queue that
//! `tick_deferred_destroy` retires `MAX_FRAMES_IN_FLIGHT` frames later.
//! Slots are never recycled: a reused index would silently corrupt any
//! dangling `GpuInstance.texture_index` (#372).

use super::*;

impl TextureRegistry {
    /// Drop a texture. Its GPU resources move into the deferred-destroy
    /// ring, the bindless descriptor slot is redirected to the fallback
    /// checkerboard (so any stale draw call degrades gracefully instead
    /// of sampling a freed `VkImageView`), and the path-cache entry is
    /// purged so a re-upload of the same path produces a fresh handle.
    ///
    /// Handles stay stable: the dropped slot retains its index in the
    /// bindless array forever — reuse would produce silent material
    /// corruption on any dangling `GpuInstance.texture_index` reference.
    /// See #372. No-op on an unknown or already-dropped handle.
    pub fn drop_texture(&mut self, device: &ash::Device, handle: TextureHandle) {
        if !self.release_ref(handle) {
            return;
        }
        self.drop_released_texture(device, handle);
    }

    /// Drop a holder-counted texture batch and purge dead path mappings once.
    ///
    /// Duplicates are significant because every material holder contributes a
    /// refcount decrement. Descriptor fallback writes remain per freed handle;
    /// only the O(path-cache) purge is coalesced.
    pub fn drop_textures(&mut self, device: &ash::Device, handles: &[TextureHandle]) -> usize {
        let freed = self.release_refs_batch(handles);
        for &handle in &freed {
            self.drop_released_texture(device, handle);
        }
        freed.len()
    }

    fn drop_released_texture(&mut self, device: &ash::Device, handle: TextureHandle) {
        let Some(entry) = self.textures.get_mut(handle as usize) else {
            return;
        };
        let Some(old) = entry.texture.take() else {
            return;
        };
        let old_view_kind = old.view_kind;
        entry
            .pending_destroy
            .push_back((self.current_frame_id, old));

        // Redirect the bindless slot to the fallback texture so any
        // GpuInstance still referencing this handle reads the
        // checkerboard instead of a freed image view. Current slot is
        // written immediately; other slots queued until their fence
        // signals (#92).
        let fallback_idx = match old_view_kind {
            TextureViewKind::D2 => Some(self.fallback_handle),
            TextureViewKind::Cube => self.cube_fallback_handle,
        }
        .map(|h| h as usize);
        if let Some(fallback_idx) = fallback_idx.filter(|&idx| idx < self.textures.len()) {
            if let Some(fallback) = self.textures[fallback_idx].texture.as_ref() {
                let image_view = fallback.image_view;
                let sampler = fallback.sampler;
                self.apply_descriptor_write(
                    device,
                    handle,
                    old_view_kind.descriptor_binding(),
                    image_view,
                    sampler,
                );
            }
        }
    }

    /// Decrement the refcount for `handle` and purge the `path_map`
    /// entry when the last holder releases. Returns `true` iff the
    /// caller should proceed with a GPU-side drop (handle was live and
    /// this release took it to zero). The GPU-side work needs an
    /// `ash::Device`, so it's split out into [`drop_texture`](Self::drop_texture);
    /// the Vulkan-free half lives here so tests can exercise refcount
    /// invariants without a real device. See #524.
    pub(super) fn release_ref(&mut self, handle: TextureHandle) -> bool {
        if !self.decrement_ref(handle) {
            return false;
        }
        self.path_map.retain(|_, &mut h| h != handle);
        true
    }

    pub(super) fn release_refs_batch(&mut self, handles: &[TextureHandle]) -> Vec<TextureHandle> {
        let mut freed = Vec::new();
        for &handle in handles {
            if self.decrement_ref(handle) {
                freed.push(handle);
            }
        }
        if !freed.is_empty() {
            let freed_set = freed.iter().copied().collect::<HashSet<_>>();
            self.path_map.retain(|_, h| !freed_set.contains(&*h));
        }
        freed
    }

    fn decrement_ref(&mut self, handle: TextureHandle) -> bool {
        let Some(entry) = self.textures.get_mut(handle as usize) else {
            return false;
        };
        if entry.ref_count == 0 {
            log::warn!(
                "drop_texture({}) on already-released handle (ref_count was 0)",
                handle,
            );
            return false;
        }
        entry.ref_count -= 1;
        if entry.ref_count > 0 {
            return false;
        }
        true
    }

    /// Drain deferred-destroy queues across all entries, destroying
    /// textures whose age is now `>= MAX_FRAMES_IN_FLIGHT`. Called once
    /// per frame alongside the mesh/BLAS deferred-destroy ticks. The
    /// `update_rgba` path also drains inline; this pass catches entries
    /// queued by [`drop_texture`] where no subsequent update call fires.
    pub fn tick_deferred_destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        let current_frame_id = self.current_frame_id;
        for entry in &mut self.textures {
            while let Some(&(queued, _)) = entry.pending_destroy.front() {
                if !should_destroy_pending(current_frame_id, queued) {
                    break;
                }
                if let Some((_, mut old)) = entry.pending_destroy.pop_front() {
                    old.destroy(device, allocator);
                }
            }
        }
    }

    /// Drain every entry's `pending_destroy` queue synchronously,
    /// regardless of `current_frame_id` aging. Counterpart of
    /// [`Self::tick_deferred_destroy`] for the shutdown path — caller
    /// must have already called `device_wait_idle` so queued textures
    /// can't be in-flight on any command buffer. See #732 / LIFE-H2.
    pub fn drain_pending_destroys(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for entry in &mut self.textures {
            for (_, mut pending) in entry.pending_destroy.drain(..) {
                pending.destroy(device, allocator);
            }
        }
    }

    /// Total number of textures still waiting across every entry's
    /// `pending_destroy` queue. Surfaced for the
    /// [`drain_pending_destroys`] regression test and shutdown
    /// telemetry. See #732.
    pub fn pending_destroy_count(&self) -> usize {
        self.textures.iter().map(|e| e.pending_destroy.len()).sum()
    }
}
