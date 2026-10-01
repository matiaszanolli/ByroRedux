//! Dynamic pixels stay on the graphics queue: one persistent image per handle,
//! one retained staging arena per frame slot, and no one-shot submit/fence wait.
//! Only successful frame submission consumes a queued update. A discarded
//! recording can retry without losing the latest pixels or advancing layout.

use super::*;
use crate::vulkan::buffer::GpuBuffer;

#[derive(Default)]
pub(super) struct PendingRgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    dirty: bool,
    recorded_slot: Option<usize>,
}

#[derive(Default)]
pub(super) struct DynamicRgbaUploads {
    pub(super) updates: HashMap<TextureHandle, PendingRgba>,
    staging: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT],
    /// #4889 — a staging-side failure (arena growth under BAR pressure, a
    /// mapped-write/flush error) degrades to a skipped overlay frame, not a
    /// dead session; this flag keeps the warn to once per failure episode.
    staging_skip_logged: bool,
}

impl DynamicRgbaUploads {
    pub(super) fn queue(
        &mut self,
        handle: TextureHandle,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<()> {
        crate::vulkan::texture::validate_rgba_upload(
            vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            width,
            height,
            pixels.len(),
        )?;
        let update = self.updates.entry(handle).or_default();
        update.width = width;
        update.height = height;
        update.pixels.clear();
        update.pixels.extend_from_slice(pixels);
        update.dirty = true;
        // If a caller replaces pixels after recording, that submission only
        // contains the older version and cannot acknowledge these new bytes.
        update.recorded_slot = None;
        Ok(())
    }

    pub(super) fn submitted(&mut self, slot: usize) {
        for update in self.updates.values_mut() {
            if update.recorded_slot == Some(slot) {
                update.dirty = false;
                update.recorded_slot = None;
            }
        }
        // A completed frame means staging works again on this device —
        // re-arm the once-per-episode warn (#4889).
        self.staging_skip_logged = false;
    }

    pub(super) fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // Registry teardown already requires device idle. Release the arenas
        // before its device/allocator, including partially recorded frames.
        for staging in &mut self.staging {
            if let Some(mut buffer) = staging.take() {
                buffer.destroy(device, allocator);
            }
        }
        self.updates.clear();
    }
}

/// #4889 — record a degraded frame: warn once per failure episode and leave
/// the dirty updates queued so the next frame retries them. Takes the flag
/// directly because `record_pending_rgba_uploads` holds disjoint `&mut`
/// borrows of `staging` and `updates` through the loop — a whole-struct
/// method call on `dynamic_rgba` would conflict with both.
fn note_staging_skip(flag: &mut bool, context: &str, error: &anyhow::Error) {
    if !*flag {
        *flag = true;
        log::warn!(
            "dynamic RGBA staging failed ({context}: {error:#}) — skipping this frame's \
             overlay texture upload; the dirty pixels stay queued and retry next frame \
             (a cosmetic overlay must not kill the session, #4889)"
        );
    }
}

impl TextureRegistry {
    /// Record pending RGBA replacements before any consumer in this frame.
    ///
    /// #4889 — error contract: only the fence-idle `ensure!` below still
    /// returns `Err` (it indicates a sequencing bug in the frame loop and
    /// must stay fatal). Every staging-side failure — arena growth under
    /// BAR pressure, a mapped write/flush error, a released or resized
    /// target — degrades to a skipped overlay upload: the pixels stay
    /// queued (or are dropped when they can never succeed), a warning is
    /// logged once per episode, and the frame records without them. The
    /// HUD/Scaleform drivers were written for that contract; making these
    /// fatal (`e2f99ad55`) let a cosmetic overlay kill the session.
    ///
    /// # Safety
    /// `cmd` must be recording outside a render pass, on the same graphics
    /// queue as every texture consumer. `begin_frame(frame)` must follow that
    /// slot's completed fence wait. Call once per recording, and acknowledge
    /// only a successful queue submission via `note_frame_submitted(frame)`.
    pub(crate) unsafe fn record_pending_rgba_uploads(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) -> Result<()> {
        anyhow::ensure!(
            frame < MAX_FRAMES_IN_FLIGHT && self.fence_confirmed_idle_slot == Some(frame),
            "RGBA staging slot is not fence-idle"
        );
        for update in self.dynamic_rgba.updates.values_mut() {
            update.recorded_slot = None;
        }
        let bytes = self
            .dynamic_rgba
            .updates
            .values()
            .filter(|u| u.dirty)
            .try_fold(0usize, |sum, update| sum.checked_add(update.pixels.len()))
            .context("RGBA staging size overflow")?;
        if bytes == 0 {
            return Ok(());
        }

        let slot = &mut self.dynamic_rgba.staging[frame];
        if slot
            .as_ref()
            .is_none_or(|buffer| buffer.size < bytes as u64)
        {
            // #4889 — before e2f99ad55 this failure was a logged, skipped
            // HUD frame; the staging rewrite made it fatal to draw_frame
            // (the app exits on any draw error), so a transient host-
            // visible / BAR allocation failure killed the whole session
            // over a cosmetic overlay. Degrade to the old contract: skip
            // this frame's upload (everything stays dirty and retries),
            // keep the existing arena, and keep recording the frame.
            let replacement = match GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
            ) {
                Ok(replacement) => replacement,
                Err(e) => {
                    note_staging_skip(
                        &mut self.dynamic_rgba.staging_skip_logged,
                        "staging arena growth",
                        &e,
                    );
                    return Ok(());
                }
            };
            // The fence for THIS arena was waited on before recording. No
            // pending transfer can still read the buffer being replaced.
            if let Some(mut old) = slot.replace(replacement) {
                old.destroy(device, allocator);
            }
        }
        let staging = slot.as_mut().expect("staging arena allocated above");
        let mut offset = 0usize;
        for (&handle, update) in self
            .dynamic_rgba
            .updates
            .iter_mut()
            .filter(|(_, u)| u.dirty)
        {
            // #4889 — a released texture or a changed extent can never
            // succeed on a later frame: drop just these pixels (they are a
            // cosmetic overlay's) and keep recording, instead of failing
            // the whole frame.
            let Some(texture) = self
                .textures
                .get(handle as usize)
                .and_then(|entry| entry.texture.as_ref())
            else {
                log::warn!(
                    "dynamic RGBA update for released texture {handle} dropped — \
                     the overlay keeps its last uploaded frame (#4889)"
                );
                update.dirty = false;
                update.pixels.clear();
                continue;
            };
            if !texture.can_update_rgba(update.width, update.height) {
                log::warn!(
                    "dynamic RGBA update for {handle} dropped: extent/format changed \
                     before recording (#4889)"
                );
                update.dirty = false;
                update.pixels.clear();
                continue;
            }
            // write_mapped_at flushes non-coherent allocations. Queue-submit
            // host-write ordering publishes these bytes before the transfer.
            // #4889 — a mapped-write/flush failure is degraded like the
            // arena growth above: skip this frame's remaining uploads (they
            // stay dirty and retry), never fail the recording.
            if let Err(e) = staging.write_mapped_at(device, offset, &update.pixels) {
                note_staging_skip(
                    &mut self.dynamic_rgba.staging_skip_logged,
                    "staging mapped write",
                    &e,
                );
                return Ok(());
            }
            let range = crate::vulkan::descriptors::color_subresource_single_mip();
            let to_copy = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(texture.image)
                .subresource_range(range);
            let to_sample = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(texture.image)
                .subresource_range(range);
            let region = vk::BufferImageCopy::default()
                .buffer_offset(offset as u64)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width: update.width,
                    height: update.height,
                    depth: 1,
                });
            // SAFETY: caller establishes recording and graphics-queue order.
            // The first barrier waits for prior submissions' texture readers
            // before overwriting the same image. The second publishes pixels
            // to all shader stages (the ground-cover atlas also uses this API).
            // Every copy ends in the starting layout, so abandoning an entire
            // recording leaves the previously submitted layout valid for retry.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_copy],
                );
                device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.buffer,
                    texture.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_sample],
                );
            }
            offset += update.pixels.len(); // RGBA byte counts keep offsets 4-byte aligned.
            update.recorded_slot = Some(frame);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_updates_use_the_frame_command_buffer_and_keep_descriptors() {
        let production = crate::source_scan::production_text(include_str!("dynamic_rgba.rs"));
        assert!(production.contains("staging.write_mapped_at(device, offset, &update.pixels)"));
        assert!(production.contains("device.cmd_copy_buffer_to_image("));
        assert!(!production.contains("queue_submit("));
        assert!(!production.contains("wait_for_fences("));
        assert!(!production.contains("with_one_time_commands("));
        assert!(production.contains(".old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)"));
        assert!(
            production.contains("vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE")
        );
        assert!(production.contains(".dst_access_mask(vk::AccessFlags::SHADER_READ)"));
        let registry = include_str!("mod.rs").split("#[cfg(test)]").next().unwrap();
        let update = registry
            .split("pub fn update_rgba(")
            .nth(1)
            .unwrap()
            .split("pub fn write_rgba_inplace(")
            .next()
            .unwrap();
        assert!(
            update.find("return self.dynamic_rgba.queue(").unwrap()
                < update.find("Texture::from_rgba(").unwrap()
        );
        let begin = include_str!("../vulkan/context/begin_frame_recording.rs");
        assert!(
            begin.find(".begin_command_buffer(").unwrap()
                < begin.find(".record_pending_rgba_uploads(").unwrap()
        );
        let release = include_str!("release.rs");
        assert!(release.contains("self.dynamic_rgba.updates.remove(&handle);"));
        assert!(registry.contains("self.dynamic_rgba.destroy(device, allocator);"));
    }

    /// #4889 — a staging-side failure must skip the overlay frame, never
    /// fail `draw_frame` (the app exits on any draw error). Pre-#4889 the
    /// arena-growth and mapped-write `?`s made a transient BAR-pressure
    /// failure fatal to the whole session; now only the fence-idle
    /// sequencing check and the size-overflow fold still propagate, and
    /// both failure arms return Ok with the pixels left queued.
    #[test]
    fn staging_failures_skip_the_overlay_frame_instead_of_failing_it() {
        let production = crate::source_scan::production_text(include_str!("dynamic_rgba.rs"));
        let record = production
            .split("pub(crate) unsafe fn record_pending_rgba_uploads(")
            .nth(1)
            .expect("record_pending_rgba_uploads must stay in this file")
            .split("\n    }")
            .next()
            .expect("function body terminates");
        assert_eq!(
            record.matches("note_staging_skip(").count(),
            2,
            "both staging failure arms must route through the once-per-episode warn"
        );
        assert!(record.contains("\"staging arena growth\""));
        assert!(record.contains("\"staging mapped write\""));
        assert_eq!(
            record.matches("return Ok(());").count(),
            3,
            "bytes == 0 plus the two degraded arms — each returns Ok"
        );
        // The only propagating errors left in the recording are the
        // fence-idle sequencing ensure (fatal by design) and the
        // size-overflow fold.
        assert!(record.contains("RGBA staging slot is not fence-idle"));
        assert_eq!(
            record.matches(")?").count(),
            1,
            "only the size-overflow fold may still propagate `?` (#4889)"
        );
        // Released / resized targets drop just their own pixels.
        assert!(record.contains("the overlay keeps its last uploaded frame (#4889)"));
        assert!(record.contains("extent/format changed"));
    }

    #[test]
    fn queued_updates_coalesce_and_reuse_cpu_storage() {
        let mut uploads = DynamicRgbaUploads::default();
        uploads.queue(7, 2, 1, &[1; 8]).unwrap();
        let pointer = uploads.updates[&7].pixels.as_ptr();
        uploads.queue(7, 2, 1, &[2; 8]).unwrap();
        assert_eq!(uploads.updates.len(), 1);
        assert_eq!(uploads.updates[&7].pixels, [2; 8]);
        assert_eq!(uploads.updates[&7].pixels.as_ptr(), pointer);
        assert!(uploads.queue(7, 2, 1, &[3; 4]).is_err());
        assert_eq!(uploads.updates[&7].pixels, [2; 8]);
        assert!(uploads.queue(7, u32::MAX, u32::MAX, &[]).is_err());
    }

    #[test]
    fn only_the_submitted_recording_consumes_its_pixels() {
        let mut uploads = DynamicRgbaUploads::default();
        uploads.queue(7, 1, 1, &[1; 4]).unwrap();
        uploads.submitted(0); // no recording / an aborted frame
        assert!(uploads.updates[&7].dirty);
        uploads.updates.get_mut(&7).unwrap().recorded_slot = Some(0);
        uploads.submitted(1);
        assert!(uploads.updates[&7].dirty);
        uploads.queue(7, 1, 1, &[2; 4]).unwrap(); // supersedes recorded data
        uploads.submitted(0);
        assert!(uploads.updates[&7].dirty);
        uploads.updates.get_mut(&7).unwrap().recorded_slot = Some(1);
        uploads.submitted(1);
        assert!(!uploads.updates[&7].dirty);
        assert_eq!(uploads.updates[&7].pixels, [2; 4]);
    }
}
