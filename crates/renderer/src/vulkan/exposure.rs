//! Shared exposure input for temporal upscaling, final composition, and the
//! Stage-1 exposure-meter pass (`exposure_meter.rs` / `exposure_meter.comp`).
//!
//! FSR consumes exposure as a 1x1 `R32_SFLOAT` texture and `presentation.frag`
//! samples the same texel as `exposureTex`, so reconstruction and the tone
//! mapper can never grade a frame at different exposures (the #2833 contract,
//! now enforced by construction: one image, per frame in flight, written by a
//! single producer).
//!
//! ## Per-frame-in-flight ownership
//!
//! The resource holds one 1x1 image per frame in flight. Slots are written by
//! the metering compute pass inside the owning frame's command buffer and read
//! by that same command buffer's FSR dispatch and presentation pass, so
//! in-flight overlap between submissions never races a write against another
//! frame's read — the same slot-ownership discipline every other per-frame
//! pass follows. Slots are cleared to [`DEFAULT_EXPOSURE`] at init, so the
//! fixed mode (or a metering failure) degrades to the historical constant.
//!
//! ## Photometry
//!
//! EV100 follows the Frostbite formulation (Lagarde & de Rousiers,
//! "Moving Frostbite to Physically Based Rendering", SIGGRAPH 2014, §5.6):
//! with the reflected-light meter calibration constant K = 12.5 and sensor
//! sensitivity S = 100, `EV100 = log2(L_avg * S / K)` and
//! `exposure = 1.2 * 2^-EV100`. The pure functions below mirror the metering
//! shader and are unit-tested against hand values.

use super::allocator::SharedAllocator;
use super::descriptors::color_subresource_single_mip;
use super::image::{GpuImage, GpuImageDesc};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

pub const EXPOSURE_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
/// Fixed-mode exposure and the init clear of every metering slot. Kept at the
/// historical constant so the fixed path is bit-identical to the pre-meter
/// renderer.
pub const DEFAULT_EXPOSURE: f32 = 0.85;

/// Reflected-light meter calibration constant (ISO 2720), Frostbite §5.6.
pub const LIGHT_METER_CALIBRATION_K: f32 = 12.5;
/// Sensor sensitivity EV100 normalizes to (ISO 100).
pub const SENSOR_SENSITIVITY_S: f32 = 100.0;
/// Middle-grey exposure constant of the Frostbite formulation.
pub const EXPOSURE_CONSTANT: f32 = 1.2;
/// Exposure clamps. Wide enough for LDR-authored Bethesda content through a
/// few stops of HDR headroom; exists so a degenerate meter (e.g. a black
/// frame) cannot zero or blow up reconstruction.
pub const MIN_AUTO_EXPOSURE: f32 = 1.0 / 256.0;
pub const MAX_AUTO_EXPOSURE: f32 = 16.0;

/// EV100 for an average scene luminance — mirror of the metering shader's
/// `log2(max(L, 1e-6) * 8.0)` (S/K = 100/12.5 = 8).
pub fn ev100_from_average_luminance(average_luminance: f32) -> f32 {
    (average_luminance.max(1.0e-6) * SENSOR_SENSITIVITY_S / LIGHT_METER_CALIBRATION_K).log2()
}

/// Linear exposure for an EV100 value: `1.2 * 2^-EV100`.
pub fn exposure_from_ev100(ev100: f32) -> f32 {
    EXPOSURE_CONSTANT * (-ev100).exp2()
}

/// Target auto exposure for a metered average luminance with a compensation
/// bias in photographic stops (positive = darker), clamped to the operating
/// range. Pure decision mirror of the shader's single-thread epilogue.
pub fn auto_exposure(average_luminance: f32, compensation_stops: f32) -> f32 {
    let target = exposure_from_ev100(ev100_from_average_luminance(average_luminance));
    (target * (-compensation_stops).exp2()).clamp(MIN_AUTO_EXPOSURE, MAX_AUTO_EXPOSURE)
}

/// Per-frame exponential-adaptation blend factor for a time constant:
/// `1 - exp(-dt / tau)` clamped to [0, 1]. Computed host-side so the shader
/// needs no clock; `tau <= 0` yields 1 (snap).
pub fn adaptation_alpha(delta_seconds: f32, tau_seconds: f32) -> f32 {
    if tau_seconds <= 0.0 {
        return 1.0;
    }
    (1.0 - (-delta_seconds / tau_seconds).exp()).clamp(0.0, 1.0)
}

/// Persistent per-frame-in-flight 1x1 exposure textures shared by the
/// metering pass, FSR, and final composition.
pub struct ExposureResource {
    /// #3860 — `GpuImage` owns image + view + allocation. One slot per frame
    /// in flight; see the module docs for the ownership discipline.
    gpus: Vec<GpuImage>,
    device: ash::Device,
    allocator: Option<SharedAllocator>,
}

impl ExposureResource {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<Self> {
        let mut gpus = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        // Build every slot first so a partial failure cannot escape `new`.
        for frame in 0..MAX_FRAMES_IN_FLIGHT {
            let gpu = GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_2d(
                    &format!("exposure_f{frame}"),
                    1,
                    1,
                    EXPOSURE_FORMAT,
                    // STORAGE: the metering pass writes the slot each frame
                    // (in GENERAL layout); SAMPLED/TRANSFER_DST unchanged from
                    // the historic usage so the FSR wrapper and the init
                    // clear keep working.
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::STORAGE,
                ),
            )?;
            gpus.push(gpu);
        }
        let mut resource = Self {
            gpus,
            device: device.clone(),
            allocator: Some(allocator.clone()),
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
            let to_transfer = self
                .gpus
                .iter()
                .map(|gpu| {
                    super::descriptors::image_barrier_undef_to_transfer_dst(gpu.image, 1)
                })
                .collect::<Vec<_>>();
            let to_shader_read = self
                .gpus
                .iter()
                .map(|gpu| {
                    super::descriptors::image_barrier_transfer_dst_to_shader_read(gpu.image, 1)
                })
                .collect::<Vec<_>>();
            unsafe {
                // SAFETY: `cmd` is recording; every image is at its declared
                // initial layout and has not been submitted before. NONE as
                // srcStage: UNDEFINED → TRANSFER_DST has no prior write to
                // expose (the #949 family idiom).
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_transfer,
                );
                for gpu in &self.gpus {
                    self.device.cmd_clear_color_image(
                        cmd,
                        gpu.image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: [DEFAULT_EXPOSURE, 0.0, 0.0, 0.0],
                        },
                        &[range],
                    );
                }
                // SAFETY: the transfer clears above are ordered before future
                // compute/fragment shader reads of the same subresources.
                self.device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER
                        | vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_shader_read,
                );
            }
            Ok(())
        })
        .context("initialize exposure images")
    }

    /// This frame-in-flight slot's image — what the meter writes and the FSR
    /// dispatch wraps.
    pub fn image(&self, frame: usize) -> vk::Image {
        self.gpus[frame].image
    }

    /// This frame-in-flight slot's sampled view — `presentation.frag`'s
    /// `exposureTex` binding.
    pub fn view(&self, frame: usize) -> vk::ImageView {
        self.gpus[frame].view
    }

    pub fn views(&self) -> Vec<vk::ImageView> {
        self.gpus.iter().map(|gpu| gpu.view).collect()
    }

    pub const fn layout(&self) -> vk::ImageLayout {
        // Steady state between the meter's post-write barrier and the next
        // frame's pre-write transition; also the state FSR's wrapper declares
        // and the init clear leaves the slots in.
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    }

    pub const fn extent(&self) -> vk::Extent2D {
        vk::Extent2D {
            width: 1,
            height: 1,
        }
    }

    /// Idempotent, and unlike the other migrated sites it takes no arguments —
    /// this resource stashes its own `device` / `allocator`, so callers that
    /// hold neither can still tear it down.
    pub fn destroy(&mut self) {
        // #3860 — `GpuImage::destroy` is itself idempotent and frees the
        // allocation before destroying the image.
        let device = self.device.clone();
        if let Some(allocator) = self.allocator.take() {
            for gpu in &mut self.gpus {
                gpu.destroy(&device, &allocator);
            }
        }
        self.gpus.clear();
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
    fn fsr_exposure_contract_is_one_r32_float_texel_per_frame_in_flight() {
        assert_eq!(EXPOSURE_FORMAT, vk::Format::R32_SFLOAT);
        assert_eq!(DEFAULT_EXPOSURE, 0.85);
    }

    /// The Frostbite reduction: EV100 = log2(L·8), exposure = 1.2·2^-EV100,
    /// so the whole chain collapses to 0.15/L̄ and a middle-grey average of
    /// 0.15 cd/m² meters to unit exposure. This pins both constants and the
    /// algebra against a hand value.
    #[test]
    fn ev100_chain_collapses_to_frostbite_ratio() {
        assert!((ev100_from_average_luminance(0.15) - (0.15_f32 * 8.0).log2()).abs() < 1.0e-6);
        assert!((exposure_from_ev100(0.0) - 1.2).abs() < 1.0e-6);
        assert!((auto_exposure(0.15, 0.0) - 1.0).abs() < 1.0e-5);
        // One stop darker is exactly half the exposure.
        assert!((auto_exposure(0.15, 1.0) - 0.5).abs() < 1.0e-4);
        // Each doubling of luminance is one stop: exposure halves.
        assert!((auto_exposure(0.30, 0.0) - 0.5).abs() < 1.0e-4);
    }

    /// Degenerate metering inputs must stay finite and inside the clamp
    /// range, never NaN (log of zero) and never unbounded.
    #[test]
    fn degenerate_luminance_stays_finite_and_clamped() {
        for luminance in [0.0, 1.0e-8, 1.0e6, f32::MAX] {
            let exposure = auto_exposure(luminance, 0.0);
            assert!(
                exposure.is_finite(),
                "auto_exposure({luminance}) must be finite, got {exposure}"
            );
            assert!(
                (MIN_AUTO_EXPOSURE..=MAX_AUTO_EXPOSURE).contains(&exposure),
                "auto_exposure({luminance}) = {exposure} escaped the clamp range"
            );
        }
        // A black frame is the darkest possible scene: the 1e-6 guard keeps
        // it finite and the clamp pins it at the long-exposure end.
        assert_eq!(auto_exposure(0.0, 0.0), MAX_AUTO_EXPOSURE);
    }

    /// #4590 — simulate the shader's actual update pattern: N per-FIF
    /// slots, each reading back ITS OWN texel N frames later with the
    /// host alpha computed over the slot interval (N x frame dt). The
    /// effective time constant must equal tau (not N x tau): after k
    /// slot-updates the remaining gap to target is exp(-k*N*dt/tau),
    /// identical to a single chain stepping at dt.
    #[test]
    fn per_slot_adaptation_runs_at_the_authored_tau() {
        const N: usize = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT;
        let dt = 1.0 / 60.0;
        let tau = 0.2_f32;
        // The host's alpha after #4590.
        let alpha = adaptation_alpha(dt * N as f32, tau);
        // Pre-#4590 alpha (one frame's dt) — shown in the assert message.
        let old_alpha = adaptation_alpha(dt, tau);

        let mut slots = [0.0_f32; 8];
        let target = 1.0_f32;
        let mut frame = 0usize;
        while frame < 240 {
            let slot = frame % N;
            let previous = slots[slot];
            slots[slot] = previous + (target - previous) * alpha;
            frame += 1;
        }
        let gap = target - slots[(frame - 1) % N];
        let expected_gap = (-((frame / N) as f32 * (dt * N as f32)) / tau).exp();
        assert!(
            (gap - expected_gap).abs() < 5.0e-3,
            "effective tau drifted: gap {gap:.4} vs single-chain {expected_gap:.4} \
             (alpha {alpha:.4}; the pre-#4590 one-frame alpha was {old_alpha:.4})"
        );
        // And the old behaviour demonstrably ran at N x tau — pin the
        // distinction so the multiplier cannot be silently dropped.
        let mut slow = [0.0_f32; 8];
        for f in 0..240usize {
            let slot = f % N;
            let previous = slow[slot];
            slow[slot] = previous + (target - previous) * old_alpha;
        }
        let slow_gap = target - slow[239 % N];
        assert!(
            slow_gap > gap * 1.5,
            "the pre-#4590 alpha must converge markedly slower ({slow_gap:.4} vs \
             {gap:.4}) — if not, this test lost its discriminating power"
        );
    }

    /// Adaptation: alpha(0) = 0 (a paused frame holds the previous exposure),
    /// alpha grows monotonically with dt, saturates at 1, and a non-positive
    /// time constant snaps.
    #[test]
    fn adaptation_alpha_is_a_saturating_ramp() {
        assert_eq!(adaptation_alpha(0.0, 0.2), 0.0);
        assert_eq!(adaptation_alpha(0.016, 0.0), 1.0);
        let a1 = adaptation_alpha(0.033, 0.2);
        let a2 = adaptation_alpha(0.100, 0.2);
        let a3 = adaptation_alpha(1.0, 0.2);
        assert!(a1 < a2 && a2 < a3, "alpha must grow with dt: {a1} {a2} {a3}");
        assert!(a3 < 1.0);
        assert_eq!(adaptation_alpha(60.0, 0.2), 1.0);
    }
}
