//! Scene-linear capture — copies composite's render-resolution scene image
//! to a staging buffer, averages a stated number of consecutive frames on
//! the CPU and writes a 32-bit-float RGB OpenEXR (M51 PTR-OUT-001, M-LIGHT
//! RLT-REF-007).
//!
//! Third sibling of [`super::screenshot`] and [`super::depth_capture`] and
//! shaped like them: a request the outside world sets, a copy recorded
//! inside the frame's command buffer, and a readback at the top of the
//! *next* frame once the all-slots fence wait proves the GPU is done.
//!
//! The copy is recorded between composite and bloom (`record_post_passes`),
//! so the image is scene-linear radiance before bloom, exposure, grading
//! and tone mapping. For the raw debug views (`direct_only`,
//! `indirect_only`, …) composite's `rawDebug` early return writes the raw
//! main-pass value into the same image, so the capture holds exactly the
//! quantity the view names. The presented PNG that `screenshot` writes is
//! untouched (PTR-OUT-005).
//!
//! A capture may instead tap TAA's resolved output ([`LinearCaptureTap::PostTaa`]):
//! post-bloom and post-reconstruction, the image the temporal gates measure
//! (M-LIGHT RLT-SHD-002, -006, -010), recorded right after the TAA dispatch.
//! It exists only when TAA ran that frame (a `final` view, no latched TAA
//! failure); otherwise the capture records nothing and its caller times out.
//!
//! Single consumer (the `--linear-capture` CLI), so like the depth capture it
//! carries no owner tag and no generation.

use super::VulkanContext;
use ash::vk;
use image::codecs::openexr::OpenExrEncoder;
use image::{ExtendedColorType, ImageEncoder};
use std::path::{Path, PathBuf};

/// Bytes per texel of the composite scene image (`HDR_FORMAT`,
/// `R16G16B16A16_SFLOAT`). The readback decode below hardcodes this layout;
/// `linear_capture_texel_layout_matches_hdr_format` pins it.
const SCENE_TEXEL_BYTES: usize = 8;

/// `OverlayResources`' linear-capture group: the capture in progress, its
/// staging buffer (owns device memory; freed by
/// `destroy_linear_capture_staging`), the extent recorded at copy time (the
/// same REG-02 / #1448 reason as the twins) and the last outcome.
#[derive(Default)]
pub(super) struct LinearCaptureState {
    job: Option<LinearCaptureJob>,
    staging: Option<(vk::Buffer, gpu_allocator::vulkan::Allocation, vk::DeviceSize)>,
    pending_readback: Option<vk::Extent2D>,
    /// The copy `linear_capture_record_copy*` just recorded into this frame's
    /// command buffer, staged until `queue_submit` returns `Ok` (#6811). A
    /// tail `Err` site discards the command buffer, so promoting at record
    /// time would arm a readback (and count a frame) for a copy the GPU never
    /// ran, averaging stale or uninitialised staging bytes into the EXR.
    pending_copy: Option<vk::Extent2D>,
    outcome: Option<Result<LinearCaptureReport, String>>,
}

/// Where a capture reads the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinearCaptureTap {
    /// Composite's output before bloom: scene-linear radiance with no
    /// temporal reconstruction (RLT-REF-007, RLT-REF-020).
    #[default]
    PreBloom,
    /// TAA's resolved output: after bloom and temporal reconstruction.
    PostTaa,
}

/// A capture in progress: the frames still owed and the running sum.
pub(super) struct LinearCaptureJob {
    path: PathBuf,
    tap: LinearCaptureTap,
    frames_target: u32,
    frames_recorded: u32,
    frames_read: u32,
    extent: Option<vk::Extent2D>,
    /// Per-pixel RGB sum over the frames read so far, in f64 so a long
    /// average does not lose the low bits of a dim pixel.
    sum: Vec<f64>,
    nonfinite: u64,
}

/// What a finished capture wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearCaptureReport {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    /// Consecutive frames averaged into the image.
    pub frames: u32,
    /// Channel samples that were NaN or infinite. They are counted here and
    /// contribute zero to the average, so one bad frame cannot poison the
    /// mean; any non-zero count is a finding, never noise.
    pub nonfinite_samples: u64,
}

impl VulkanContext {
    /// Start a capture of the next `frames` consecutive frames into `path`.
    /// Replaces any capture still in progress. `frames` is clamped to ≥ 1.
    pub fn request_linear_capture(&mut self, path: impl Into<PathBuf>, frames: u32) {
        self.request_linear_capture_at(path, frames, LinearCaptureTap::PreBloom);
    }

    /// [`Self::request_linear_capture`] from a chosen tap.
    pub fn request_linear_capture_at(&mut self, path: impl Into<PathBuf>, frames: u32, tap: LinearCaptureTap) {
        self.overlay.linear_capture.outcome = None;
        self.overlay.linear_capture.job = Some(LinearCaptureJob {
            path: path.into(),
            tap,
            frames_target: frames.max(1),
            frames_recorded: 0,
            frames_read: 0,
            extent: None,
            sum: Vec::new(),
            nonfinite: 0,
        });
    }

    /// `true` while a requested capture has not finished or failed.
    pub fn linear_capture_pending(&self) -> bool {
        self.overlay.linear_capture.job.is_some()
    }

    /// The outcome of the last finished capture, once.
    pub fn take_linear_capture_outcome(&mut self) -> Option<Result<LinearCaptureReport, String>> {
        self.overlay.linear_capture.outcome.take()
    }

    /// Accumulate the copy recorded last frame and, once every requested
    /// frame is in, write the EXR. Called at the top of `draw_frame()` after
    /// the all-slots fence wait, beside the screenshot and depth readbacks.
    pub(super) fn linear_capture_finish_readback(&mut self) {
        let Some(extent) = self.overlay.linear_capture.pending_readback.take() else {
            return;
        };
        let texels = extent.width as usize * extent.height as usize;
        let expected = texels * SCENE_TEXEL_BYTES;
        // Disjoint field borrows only (staging, job, device): the failure
        // path below needs `&mut self`, so it runs after this block ends.
        let failure = {
            let Some((_, ref allocation, _)) = self.overlay.linear_capture.staging else {
                return;
            };
            let Some(job) = self.overlay.linear_capture.job.as_mut() else {
                return;
            };
            // #2740 / REN-D4-04 — the fence makes the copy complete, not
            // host-visible; `GpuToCpu` prefers `HOST_CACHED`. Same shared
            // range helper as the screenshot and depth readbacks; no-op when
            // coherent.
            if !allocation
                .memory_properties()
                .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
            {
                let (aligned_offset, aligned_size) = super::super::buffer::aligned_flush_range(
                    allocation.offset(),
                    allocation.size(),
                );
                // SAFETY: `allocation` is live and owned by
                // `self.overlay.linear_capture.staging` for this call.
                // `aligned_flush_range` widens outward to
                // `NON_COHERENT_ATOM_SIZE` inside the parent
                // `GpuAllocatorManaged` block; invalidating a superset only
                // discards possibly-stale host cache lines.
                let result = unsafe {
                    let range = vk::MappedMemoryRange::default()
                        .memory(allocation.memory())
                        .offset(aligned_offset)
                        .size(aligned_size);
                    self.device.invalidate_mapped_memory_ranges(&[range])
                };
                if let Err(e) = result {
                    log::warn!("Linear capture invalidate failed: {e} — samples may be stale");
                }
            }
            match allocation.mapped_slice() {
                None => Some("staging buffer not mapped".to_string()),
                Some(slice) if slice.len() < expected => {
                    Some(format!("staging too small: {} < {expected}", slice.len()))
                }
                Some(slice) => {
                    if job.sum.len() != texels * 3 {
                        job.sum = vec![0.0; texels * 3];
                    }
                    job.nonfinite += accumulate_rgba16f(&slice[..expected], &mut job.sum);
                    job.frames_read += 1;
                    if job.frames_read < job.frames_target {
                        return;
                    }
                    None
                }
            }
        };
        if let Some(reason) = failure {
            self.fail_linear_capture(reason);
            return;
        }

        let job = self.overlay.linear_capture.job.take().expect("checked above");
        let outcome = finish_linear_capture(job, extent);
        match &outcome {
            Ok(report) => log::info!(
                "Linear capture: {}x{}, {} frames, {} non-finite → {}",
                report.width,
                report.height,
                report.frames,
                report.nonfinite_samples,
                report.path.display()
            ),
            Err(e) => log::warn!("Linear capture failed: {e}"),
        }
        self.overlay.linear_capture.outcome = Some(outcome);
    }

    fn fail_linear_capture(&mut self, reason: String) {
        log::warn!("Linear capture failed: {reason}");
        self.overlay.linear_capture.job = None;
        self.overlay.linear_capture.outcome = Some(Err(reason));
    }

    /// If a capture still owes frames, record the copy from composite's
    /// scene image for `frame` into the staging buffer.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording and outside any render pass, after this
    /// frame's composite render pass and before bloom. Composite leaves the
    /// scene image in `SHADER_READ_ONLY_OPTIMAL`, with its outgoing subpass
    /// dependency making the colour writes visible to `TRANSFER` reads; this
    /// function restores that layout before returning, so bloom, the
    /// exposure meter, TAA and the native bridge see what they expect
    /// whether or not a capture ran.
    pub(super) unsafe fn linear_capture_record_copy(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        let Some((extent, staging_buffer)) = self.linear_capture_copy_target(LinearCaptureTap::PreBloom) else {
            return;
        };
        let Some(scene_image) = self.post.composite.as_ref().map(|c| c.scene_image(frame)) else {
            self.fail_linear_capture("composite pipeline absent".to_string());
            return;
        };

        let range = super::super::descriptors::color_subresource_single_mip();
        let to_src = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(scene_image)
            .subresource_range(range);
        let restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(scene_image)
            .subresource_range(range);
        let region = full_copy_region(extent);

        // SAFETY: `cmd` is recording outside any render pass (caller
        // contract). `scene_image` is this frame's live composite target in
        // `SHADER_READ_ONLY_OPTIMAL`; the barriers bracket the single copy
        // and restore that layout, and nothing else touches the image in
        // between. `staging_buffer` is `TRANSFER_DST`, at least
        // `buffer_size` bytes, and not read by the host until the next
        // frame's all-slots fence wait.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_src],
            );
            self.device.cmd_copy_image_to_buffer(
                cmd,
                scene_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[region],
            );
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[restore],
            );
        }

        self.linear_capture_copy_recorded(extent);
    }

    /// The post-TAA tap: copy this frame's resolved TAA output. Call right
    /// after a recorded TAA dispatch.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside any render pass, after this frame's
    /// TAA dispatch. TAA leaves its output in `GENERAL` behind a
    /// compute-write → shader-read barrier; the copy only reads it in that
    /// layout, so it changes nothing the upscale tap or next frame's TAA
    /// expect.
    pub(super) unsafe fn linear_capture_record_copy_post_taa(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        let Some((extent, staging_buffer)) = self.linear_capture_copy_target(LinearCaptureTap::PostTaa) else {
            return;
        };
        let Some((image, taa_extent)) = self
            .post
            .taa
            .as_ref()
            .map(|taa| (taa.output_image(frame), vk::Extent2D { width: taa.width, height: taa.height }))
        else {
            self.fail_linear_capture("TAA pipeline absent".to_string());
            return;
        };
        if taa_extent != extent {
            self.fail_linear_capture(format!(
                "TAA output {}x{} is not the render extent {}x{}",
                taa_extent.width, taa_extent.height, extent.width, extent.height
            ));
            return;
        }
        let to_transfer = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(super::super::descriptors::color_subresource_single_mip());
        // SAFETY: caller contract above. `image` is this frame's TAA history
        // slot in `GENERAL`; the barrier orders the dispatch's writes before
        // the transfer read, and the copy is read-only, so no restore is
        // needed. `staging_buffer` is as in `linear_capture_record_copy`.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );
            self.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                staging_buffer,
                &[full_copy_region(extent)],
            );
        }
        self.linear_capture_copy_recorded(extent);
    }

    /// The extent and staging buffer for this frame's copy, if a capture
    /// from `tap` still owes frames. Fails the capture on an extent change.
    fn linear_capture_copy_target(&mut self, tap: LinearCaptureTap) -> Option<(vk::Extent2D, vk::Buffer)> {
        let job = self.overlay.linear_capture.job.as_ref()?;
        if job.tap != tap || job.frames_recorded >= job.frames_target {
            return None;
        }
        let extent = self.frame_extents.render;
        if let Some(first) = job.extent {
            if first != extent {
                self.fail_linear_capture(format!(
                    "render extent changed mid-capture: {}x{} → {}x{}",
                    first.width, first.height, extent.width, extent.height
                ));
                return None;
            }
        }
        let buffer_size = extent.width as vk::DeviceSize
            * extent.height as vk::DeviceSize
            * SCENE_TEXEL_BYTES as vk::DeviceSize;
        self.ensure_linear_capture_staging(buffer_size);
        let Some((staging_buffer, _, _)) = self.overlay.linear_capture.staging.as_ref() else {
            self.fail_linear_capture("staging buffer creation failed".to_string());
            return None;
        };
        Some((extent, *staging_buffer))
    }

    /// Stage the copy just recorded; `promote_linear_capture_copy` commits it
    /// once the submit succeeded (#6811).
    fn linear_capture_copy_recorded(&mut self, extent: vk::Extent2D) {
        self.overlay.linear_capture.pending_copy = Some(extent);
    }

    /// Drop a staged copy whose command buffer never reached the queue. Called
    /// at the top of `draw_frame`, beside `discard_pending_skip_clear_latches`,
    /// so a frame that bails or whose submit failed commits nothing (#6811).
    pub(super) fn discard_pending_linear_capture_copy(&mut self) {
        self.overlay.linear_capture.pending_copy = None;
    }

    /// Commit the staged copy: count the recorded frame and arm the readback
    /// the next frame's `linear_capture_finish_readback` consumes. Runs once
    /// `queue_submit` has returned `Ok` (#6811), beside `promote_skip_clear_latches`.
    pub(super) fn promote_linear_capture_copy(&mut self) {
        let Some(extent) = self.overlay.linear_capture.pending_copy.take() else {
            return;
        };
        if let Some(job) = self.overlay.linear_capture.job.as_mut() {
            job.frames_recorded += 1;
            job.extent = Some(extent);
        }
        self.overlay.linear_capture.pending_readback = Some(extent);
    }

    fn ensure_linear_capture_staging(&mut self, required_size: vk::DeviceSize) {
        if let Some((_, _, existing)) = &self.overlay.linear_capture.staging {
            if *existing >= required_size {
                return;
            }
            self.destroy_linear_capture_staging();
        }
        self.overlay.linear_capture.staging = super::helpers::create_staging_buffer(
            &self.device,
            self.allocator.as_ref(),
            required_size,
            "linear-capture-staging",
        );
    }

    /// Free the linear-capture staging buffer under the contract on
    /// [`super::helpers::destroy_staging_buffer`], shared with the
    /// screenshot and depth-capture twins.
    pub(super) fn destroy_linear_capture_staging(&mut self) {
        let staging = self.overlay.linear_capture.staging.take();
        super::helpers::destroy_staging_buffer(&self.device, self.allocator.as_ref(), staging);
    }
}

/// One whole-image, tightly packed colour copy of mip 0.
fn full_copy_region(extent: vk::Extent2D) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
}

/// Add one RGBA16F frame's RGB to `sum` (three f64 per texel, alpha
/// dropped). Returns the number of non-finite channel samples, which add 0.
fn accumulate_rgba16f(bytes: &[u8], sum: &mut [f64]) -> u64 {
    let mut nonfinite = 0u64;
    for (texel, out) in bytes.chunks_exact(SCENE_TEXEL_BYTES).zip(sum.chunks_exact_mut(3)) {
        for (channel, slot) in out.iter_mut().enumerate() {
            let bits = u16::from_le_bytes([texel[channel * 2], texel[channel * 2 + 1]]);
            let value = half::f16::from_bits(bits).to_f32();
            if value.is_finite() {
                *slot += f64::from(value);
            } else {
                nonfinite += 1;
            }
        }
    }
    nonfinite
}

fn finish_linear_capture(
    job: LinearCaptureJob,
    extent: vk::Extent2D,
) -> Result<LinearCaptureReport, String> {
    let scale = 1.0 / f64::from(job.frames_read.max(1));
    let mean: Vec<f32> = job.sum.iter().map(|v| (v * scale) as f32).collect();
    write_linear_exr(&job.path, extent.width, extent.height, &mean)?;
    Ok(LinearCaptureReport {
        path: job.path,
        width: extent.width,
        height: extent.height,
        frames: job.frames_read,
        nonfinite_samples: job.nonfinite,
    })
}

/// The project's one OpenEXR writer (M51 PTR-OUT-001): 32-bit-float RGB,
/// scanline, in the units the caller hands it. `rgb` is row-major from the
/// top row, three floats per pixel. The encoder's output is deterministic,
/// so two identical images hash identically.
pub fn write_linear_exr(path: &Path, width: u32, height: u32, rgb: &[f32]) -> Result<(), String> {
    let expected = width as usize * height as usize * 3;
    if rgb.len() != expected {
        return Err(format!("EXR payload is {} floats, expected {expected}", rgb.len()));
    }
    let bytes: Vec<u8> = rgb.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut encoded = Vec::new();
    OpenExrEncoder::new(std::io::Cursor::new(&mut encoded))
        .write_image(&bytes, width, height, ExtendedColorType::Rgb32F)
        .map_err(|e| format!("EXR encode failed: {e}"))?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, &encoded).map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulkan::composite::HDR_FORMAT;

    #[test]
    fn linear_capture_texel_layout_matches_hdr_format() {
        assert_eq!(HDR_FORMAT, vk::Format::R16G16B16A16_SFLOAT);
        assert_eq!(SCENE_TEXEL_BYTES, 4 * 2);
    }

    #[test]
    fn accumulate_decodes_rgb_drops_alpha_and_counts_nonfinite() {
        let texel = |r: f32, g: f32, b: f32, a: f32| -> Vec<u8> {
            [r, g, b, a]
                .iter()
                .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
                .collect()
        };
        let mut bytes = texel(0.5, 2.0, 1024.0, 0.25);
        bytes.extend(texel(f32::NAN, 1.0, f32::INFINITY, 1.0));
        let mut sum = vec![0.0; 6];
        let nonfinite = accumulate_rgba16f(&bytes, &mut sum);
        assert_eq!(nonfinite, 2);
        assert_eq!(sum, vec![0.5, 2.0, 1024.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn exr_writer_round_trips_bit_exact_and_is_deterministic() {
        let dir = std::env::temp_dir().join(format!("byro-exr-{}", std::process::id()));
        let path = dir.join("probe.exr");
        let rgb: Vec<f32> = (0..4 * 3 * 3).map(|i| i as f32 * 0.37 + 1e-4).collect();
        write_linear_exr(&path, 4, 3, &rgb).unwrap();
        let first = std::fs::read(&path).unwrap();
        write_linear_exr(&path, 4, 3, &rgb).unwrap();
        assert_eq!(first, std::fs::read(&path).unwrap(), "EXR output must be deterministic");
        let back = image::load_from_memory(&first).unwrap().into_rgb32f();
        assert_eq!((back.width(), back.height()), (4, 3));
        assert!(back
            .as_raw()
            .iter()
            .zip(&rgb)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(write_linear_exr(&path, 4, 3, &rgb[1..]).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
