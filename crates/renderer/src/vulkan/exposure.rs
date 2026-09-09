//! Shared exposure input for temporal upscaling and final composition.
//!
//! FSR consumes exposure as a 1x1 `R32_SFLOAT` texture. The renderer does not
//! have automatic exposure yet, so this resource stores the existing fixed HDR
//! exposure explicitly instead of letting the upscaler and composite pass grow
//! independent constants. A future auto-exposure pass can replace the one-time
//! clear with a storage-image write without changing the input contract.

use super::allocator::SharedAllocator;
use super::descriptors::color_subresource_single_mip;
use super::image::{GpuImage, GpuImageDesc};
use anyhow::{Context, Result};
use ash::vk;

pub const EXPOSURE_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
pub const DEFAULT_EXPOSURE: f32 = 0.85;

/// Exposure both consumers must assume when [`ExposureResource`] does not
/// exist at all — i.e. its 1×1 image allocation failed at startup (#2833).
///
/// **Not** [`DEFAULT_EXPOSURE`], and deliberately so. On the happy path one
/// resource feeds both consumers and they agree exactly: FSR's
/// `PrepareRgb(rgb, exposure, preExposure)` does `rgb /= preExposure; rgb *=
/// exposure` (`ffx_fsr2_common.h`), and the tone mapper multiplies by the same
/// texel. On the failure path FSR receives a *null* exposure resource and the
/// SDK substitutes its own default, whose accessor rewrites a zero texel to
/// `1.0` — a value the engine cannot override, because the only lever left is
/// `preExposure`, and that field means "what the input colour was already
/// multiplied by" (it also feeds the SDK's cross-frame `deltaPreExposure` /
/// `previousFramePreExposure` bookkeeping), so bending it to smuggle a grading
/// constant through would corrupt reconstruction rather than align it.
///
/// Presentation therefore matches the SDK instead of the reverse. Pre-fix the
/// two fell back independently — 0.85 for the tone mapper against the SDK's
/// 1.0 — so reconstruction normalised its luma against a ~1.18× different
/// exposure than the frame was graded at, in exactly the domain FSR uses for
/// locking and history rectification.
pub const NO_EXPOSURE_RESOURCE_FALLBACK: f32 = 1.0;

/// Persistent 1x1 exposure texture shared by FSR and final composition.
pub struct ExposureResource {
    /// #3860 — was `image` + `view` + `allocation` + the `device` /
    /// `allocator` clones needed to self-free, i.e. a hand-rolled `GpuImage`
    /// down to the `Drop`-calls-`destroy` shape. `device` and `allocator` stay
    /// because this type's `destroy()` takes no arguments, unlike every other
    /// migrated site.
    gpu: GpuImage,
    device: ash::Device,
    allocator: Option<SharedAllocator>,
    value: f32,
}

impl ExposureResource {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<Self> {
        // #3860 — was ~85 lines of create → allocate → bind → view with its
        // own three-arm cleanup; `GpuImage::create` owns that chain now.
        let gpu = GpuImage::create(
            device,
            allocator,
            &GpuImageDesc::color_2d(
                "exposure",
                1,
                1,
                EXPOSURE_FORMAT,
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            ),
        )?;
        let mut resource = Self {
            gpu,
            device: device.clone(),
            allocator: Some(allocator.clone()),
            value: DEFAULT_EXPOSURE,
        };
        if let Err(error) = resource.initialize(queue, command_pool) {
            resource.destroy();
            return Err(error);
        }
        Ok(resource)
    }

    fn initialize(
        &self,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        super::texture::with_one_time_commands(&self.device, queue, command_pool, |cmd| {
            let range = color_subresource_single_mip();
            // #2413 / TD2-116 — use the shared constructors instead of
            // hand-rolling the pair. They also set
            // src/dst_queue_family_index to QUEUE_FAMILY_IGNORED, which the
            // hand-rolled version left at the zeroed default.
            let to_transfer =
                super::descriptors::image_barrier_undef_to_transfer_dst(self.gpu.image, 1);
            unsafe {
                // SAFETY: `cmd` is recording; the image is at its declared
                // initial layout and has not been submitted before.
                // NONE, not TOP_OF_PIPE: UNDEFINED → TRANSFER_DST_OPTIMAL has
                // no prior write to expose. This file was written after the
                // rest of the family had already migrated (#949 / #1100 /
                // #1122) and reintroduced the stale idiom — #2413.
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_transfer],
                );
                self.device.cmd_clear_color_image(
                    cmd,
                    self.gpu.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [self.value, 0.0, 0.0, 0.0],
                    },
                    &[range],
                );
            }

            let to_shader_read =
                super::descriptors::image_barrier_transfer_dst_to_shader_read(self.gpu.image, 1);
            unsafe {
                // SAFETY: the transfer clear above is ordered before future
                // compute/fragment shader reads of the same subresource.
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[to_shader_read],
                );
            }
            Ok(())
        })
        .context("initialize exposure image")
    }

    pub fn image(&self) -> vk::Image {
        self.gpu.image
    }

    pub fn view(&self) -> vk::ImageView {
        self.gpu.view
    }

    pub const fn layout(&self) -> vk::ImageLayout {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    }

    pub const fn extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: 1,
            height: 1,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Idempotent, and unlike the other migrated sites it takes no arguments —
    /// this resource stashes its own `device` / `allocator`, so callers that
    /// hold neither can still tear it down.
    pub fn destroy(&mut self) {
        // #3860 — `GpuImage::destroy` is itself idempotent and frees the
        // allocation before destroying the image.
        let device = self.device.clone();
        if let Some(allocator) = self.allocator.take() {
            self.gpu.destroy(&device, &allocator);
        }
    }
}

impl Drop for ExposureResource {
    fn drop(&mut self) {
        self.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fsr_exposure_contract_is_one_r32_float_texel() {
        assert_eq!(EXPOSURE_FORMAT, vk::Format::R32_SFLOAT);
        assert_eq!(DEFAULT_EXPOSURE, 0.85);
    }

    /// #2833 — with no `ExposureResource`, FSR gets a null exposure and the
    /// SDK substitutes `1.0`. The tone mapper has to agree, or reconstruction
    /// normalises its luma against a different exposure than the frame is
    /// graded at.
    #[test]
    fn absent_resource_fallback_matches_the_sdk_substitution() {
        assert_eq!(
            NO_EXPOSURE_RESOURCE_FALLBACK, 1.0,
            "the SDK's null-exposure accessor rewrites a zero texel to 1.0; \
             this constant exists to mirror it"
        );
        assert_ne!(
            NO_EXPOSURE_RESOURCE_FALLBACK, DEFAULT_EXPOSURE,
            "these are deliberately different: DEFAULT_EXPOSURE seeds a live \
             resource, this one only applies when no resource exists at all"
        );
    }

    /// Pin the consumer, not just the constant: the presentation fallback is
    /// what actually has to move in lockstep with the SDK, and a future edit
    /// reverting it to `DEFAULT_EXPOSURE` restores the ~1.18× mismatch with
    /// nothing at runtime to catch it.
    #[test]
    fn presentation_falls_back_to_the_shared_constant() {
        let src = include_str!("context/post_passes.rs");
        assert!(
            src.contains("exposure::NO_EXPOSURE_RESOURCE_FALLBACK"),
            "presentation must fall back to the shared no-resource constant"
        );
        let fallback_site = src
            .split_once("let exposure = self")
            .expect("presentation still reads an exposure fallback")
            .1;
        // #3393 sibling — back up to a char boundary. A bare
        // `&s[..s.len().min(400)]` panics with "byte index 400 is not a char
        // boundary" the moment an em dash (or any multi-byte scalar) in the
        // scanned source straddles the cut. Backing up never widens the scope.
        let mut end = fallback_site.len().min(400);
        while end > 0 && !fallback_site.is_char_boundary(end) {
            end -= 1;
        }
        let head = &fallback_site[..end];
        assert!(
            !head.contains("DEFAULT_EXPOSURE"),
            "presentation's no-resource branch drifted back to DEFAULT_EXPOSURE (#2833)"
        );
    }
}
