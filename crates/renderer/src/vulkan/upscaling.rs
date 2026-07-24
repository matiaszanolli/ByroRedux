//! Runtime upscaler selection and the render/output extent contract.
//!
//! This module is the single source of truth for preset sizing and the temporal
//! input contract so extent, jitter, and motion-vector conversion math cannot
//! drift from the SDK dispatch boundary.

use ash::vk;
use byroredux_fsr3_sys::{self as fsr3, QualityMode};
use std::fmt;
use std::str::FromStr;

/// FSR 3.1 upscaler quality preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsrQuality {
    NativeAa,
    Quality,
    Balanced,
    Performance,
}

impl FsrQuality {
    fn sdk_mode(self) -> QualityMode {
        match self {
            Self::NativeAa => QualityMode::NativeAa,
            Self::Quality => QualityMode::Quality,
            Self::Balanced => QualityMode::Balanced,
            Self::Performance => QualityMode::Performance,
        }
    }
}

impl fmt::Display for FsrQuality {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NativeAa => "native-aa",
            Self::Quality => "quality",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        })
    }
}

impl FromStr for FsrQuality {
    type Err = ParseRendererOptionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native-aa" => Ok(Self::NativeAa),
            "quality" => Ok(Self::Quality),
            "balanced" => Ok(Self::Balanced),
            "performance" => Ok(Self::Performance),
            _ => Err(ParseRendererOptionError::FsrQuality(value.to_owned())),
        }
    }
}

/// Temporal reconstruction path selected for the renderer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UpscalerMode {
    /// Existing native-resolution TAA path and compatibility fallback.
    #[default]
    Taa,
    /// FSR 3.1 upscaler-only path. Frame generation is not represented here.
    Fsr3(FsrQuality),
}

impl fmt::Display for UpscalerMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Taa => formatter.write_str("taa"),
            Self::Fsr3(quality) => write!(formatter, "fsr3/{quality}"),
        }
    }
}

/// Renderer options parsed once by the application and passed into Vulkan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RendererConfig {
    pub upscaler: UpscalerMode,
}

/// Scene-render and presentation extents for one swapchain generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameExtentSet {
    /// Extent used by scene/G-buffer/ReSTIR/SVGF work.
    pub render: vk::Extent2D,
    /// Extent used by the swapchain and presentation passes.
    pub output: vk::Extent2D,
}

impl FrameExtentSet {
    /// Query the canonical render dimensions for `mode` and validate them
    /// against the selected Vulkan device's 2D-image limit.
    pub fn for_output(
        output: vk::Extent2D,
        mode: UpscalerMode,
        max_image_dimension_2d: u32,
    ) -> Result<Self, FrameExtentError> {
        if output.width == 0 || output.height == 0 {
            return Err(FrameExtentError::ZeroOutput);
        }
        if max_image_dimension_2d == 0
            || output.width > max_image_dimension_2d
            || output.height > max_image_dimension_2d
        {
            return Err(FrameExtentError::DeviceLimit {
                width: output.width,
                height: output.height,
                limit: max_image_dimension_2d,
            });
        }

        let render = match mode {
            UpscalerMode::Taa => output,
            UpscalerMode::Fsr3(quality) => {
                let [width, height] =
                    fsr3::render_resolution(output.width, output.height, quality.sdk_mode())?;
                vk::Extent2D {
                    width: width.min(max_image_dimension_2d),
                    height: height.min(max_image_dimension_2d),
                }
            }
        };
        if render.width == 0 || render.height == 0 {
            return Err(FrameExtentError::ZeroRender);
        }

        Ok(Self { render, output })
    }

    /// Recommended material-texture mip bias for the selected mode.
    ///
    /// TAA keeps the historical zero bias. FSR follows AMD's
    /// `log2(render_width / output_width) - 1` recommendation.
    pub fn material_mip_bias(self, mode: UpscalerMode) -> f32 {
        match mode {
            UpscalerMode::Taa => 0.0,
            UpscalerMode::Fsr3(_) => {
                (self.render.width as f32 / self.output.width as f32).log2() - 1.0
            }
        }
    }
}

/// One SDK-authored projection-jitter sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FsrJitterSample {
    /// SDK sample in render-resolution pixel units. This exact value is passed
    /// to the FSR dispatch.
    pub pixel: [f32; 2],
    /// Matching Vulkan projection offset. ByroRedux's projection is Y-flipped,
    /// so the SDK's positive pixel Y becomes negative NDC Y.
    pub ndc: [f32; 2],
}

/// Deterministic FSR jitter and reset state.
///
/// The complete SDK sequence is queried once for the active scale ratio. The
/// index advances only through [`Self::mark_dispatch_completed`], which the
/// renderer calls after a successfully submitted FSR frame.
/// Recording failures therefore cannot silently desynchronise projection and
/// reconstruction history.
#[derive(Debug)]
pub struct FsrTemporalState {
    samples: Vec<FsrJitterSample>,
    index: usize,
    reset_pending: bool,
}

impl FsrTemporalState {
    pub fn new(extents: FrameExtentSet) -> Result<Self, FsrTemporalError> {
        let phase_count = fsr3::jitter_phase_count(extents.render.width, extents.output.width)?;
        if phase_count <= 0 {
            return Err(FsrTemporalError::InvalidPhaseCount(phase_count));
        }
        let samples = (0..phase_count as u32)
            .map(|index| {
                let pixel = fsr3::jitter_offset(index, phase_count)?;
                Ok(FsrJitterSample {
                    pixel,
                    ndc: fsr_pixel_jitter_to_ndc(pixel, extents.render),
                })
            })
            .collect::<Result<Vec<_>, fsr3::Error>>()?;

        Ok(Self {
            samples,
            index: 0,
            reset_pending: true,
        })
    }

    pub fn current(&self) -> FsrJitterSample {
        self.samples[self.index]
    }

    pub fn phase_count(&self) -> usize {
        self.samples.len()
    }

    pub fn sequence_index(&self) -> usize {
        self.index
    }

    pub fn reset_pending(&self) -> bool {
        self.reset_pending
    }

    /// Reset both reconstruction history and the deterministic jitter phase.
    pub fn signal_reset(&mut self) {
        self.index = 0;
        self.reset_pending = true;
    }

    /// Consume one successfully submitted FSR dispatch.
    pub fn mark_dispatch_completed(&mut self) {
        self.index = (self.index + 1) % self.samples.len();
        self.reset_pending = false;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FsrTemporalError {
    #[error("FSR jitter query failed: {0}")]
    Sdk(#[from] fsr3::Error),
    #[error("FSR returned invalid jitter phase count {0}")]
    InvalidPhaseCount(i32),
}

/// Convert the SDK's render-pixel jitter into ByroRedux's Vulkan NDC offset.
pub fn fsr_pixel_jitter_to_ndc(pixel: [f32; 2], render: vk::Extent2D) -> [f32; 2] {
    [
        2.0 * pixel[0] / render.width as f32,
        -2.0 * pixel[1] / render.height as f32,
    ]
}

/// Scale passed to FSR for the engine's `current_uv - previous_uv` motion
/// texture. FSR expects `previous_pixel - current_pixel`, hence both negative
/// render dimensions and no display-resolution/jitter-cancellation flags.
pub fn fsr_motion_vector_scale(render: vk::Extent2D) -> [f32; 2] {
    [-(render.width as f32), -(render.height as f32)]
}

/// Numerical form of the boundary conversion, used by contract tests and
/// motion debug tooling. Production dispatch passes the source texture plus
/// [`fsr_motion_vector_scale`] rather than rewriting every pixel.
pub fn engine_motion_to_fsr_pixels(motion_uv: [f32; 2], render: vk::Extent2D) -> [f32; 2] {
    let scale = fsr_motion_vector_scale(render);
    [motion_uv[0] * scale[0], motion_uv[1] * scale[1]]
}

/// Camera values required by FSR, validated at the renderer boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FsrCameraParameters {
    pub near: f32,
    pub far: f32,
    pub fov_y_radians: f32,
}

pub fn fsr_camera_parameters(
    near: f32,
    far: f32,
    fov_y_radians: f32,
) -> Option<FsrCameraParameters> {
    let values = [near, far, fov_y_radians];
    if values.into_iter().all(f32::is_finite)
        && near > 0.0
        && far > near
        && fov_y_radians > 0.0
        && fov_y_radians < std::f32::consts::PI
    {
        Some(FsrCameraParameters {
            near,
            far,
            fov_y_radians,
        })
    } else {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FrameExtentError {
    #[error("output extent must be non-zero")]
    ZeroOutput,
    #[error("render extent queried from FSR must be non-zero")]
    ZeroRender,
    #[error("output extent {width}x{height} exceeds device 2D-image limit {limit}")]
    DeviceLimit { width: u32, height: u32, limit: u32 },
    #[error("FSR render-resolution query failed: {0}")]
    Fsr(#[from] fsr3::Error),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseRendererOptionError {
    #[error("unknown upscaler '{0}'; expected 'taa' or 'fsr3'")]
    Upscaler(String),
    #[error("unknown FSR quality '{0}'; expected native-aa, quality, balanced, or performance")]
    FsrQuality(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_HD: vk::Extent2D = vk::Extent2D {
        width: 1920,
        height: 1080,
    };

    fn extents(mode: UpscalerMode) -> FrameExtentSet {
        FrameExtentSet::for_output(FULL_HD, mode, 16_384).unwrap()
    }

    #[test]
    fn taa_is_native_and_preserves_legacy_mip_bias() {
        let set = extents(UpscalerMode::Taa);
        assert_eq!(set.render, FULL_HD);
        assert_eq!(set.output, FULL_HD);
        assert_eq!(set.material_mip_bias(UpscalerMode::Taa), 0.0);
    }

    #[test]
    fn every_fsr_preset_uses_the_sdk_resolution_query() {
        let cases = [
            (FsrQuality::NativeAa, [1920, 1080]),
            (FsrQuality::Quality, [1280, 720]),
            (FsrQuality::Balanced, [1129, 635]),
            (FsrQuality::Performance, [960, 540]),
        ];
        for (quality, expected) in cases {
            let set = extents(UpscalerMode::Fsr3(quality));
            assert_eq!([set.render.width, set.render.height], expected);
            assert_eq!(set.output, FULL_HD);
        }
    }

    #[test]
    fn fsr_mip_bias_tracks_the_actual_rounded_extent() {
        let native = extents(UpscalerMode::Fsr3(FsrQuality::NativeAa));
        assert!(
            (native.material_mip_bias(UpscalerMode::Fsr3(FsrQuality::NativeAa)) + 1.0).abs() < 1e-6
        );

        let performance = extents(UpscalerMode::Fsr3(FsrQuality::Performance));
        assert!(
            (performance.material_mip_bias(UpscalerMode::Fsr3(FsrQuality::Performance)) + 2.0)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn invalid_extents_are_rejected_before_allocation() {
        assert!(matches!(
            FrameExtentSet::for_output(vk::Extent2D::default(), UpscalerMode::Taa, 16_384),
            Err(FrameExtentError::ZeroOutput)
        ));
        assert!(matches!(
            FrameExtentSet::for_output(FULL_HD, UpscalerMode::Taa, 1024),
            Err(FrameExtentError::DeviceLimit { .. })
        ));
    }

    #[test]
    fn quality_names_are_stable_cli_contracts() {
        for (name, quality) in [
            ("native-aa", FsrQuality::NativeAa),
            ("quality", FsrQuality::Quality),
            ("balanced", FsrQuality::Balanced),
            ("performance", FsrQuality::Performance),
        ] {
            assert_eq!(name.parse::<FsrQuality>().unwrap(), quality);
            assert_eq!(quality.to_string(), name);
        }
    }

    #[test]
    fn fsr_jitter_phase_count_tracks_every_scale_ratio() {
        let cases = [
            (FsrQuality::NativeAa, 8),
            (FsrQuality::Quality, 18),
            (FsrQuality::Balanced, 23),
            (FsrQuality::Performance, 32),
        ];
        for (quality, expected_phases) in cases {
            let state = FsrTemporalState::new(extents(UpscalerMode::Fsr3(quality))).unwrap();
            assert_eq!(state.phase_count(), expected_phases, "{quality}");
            assert!(state.reset_pending());
            assert_eq!(state.sequence_index(), 0);
        }
    }

    #[test]
    fn fsr_jitter_is_deterministic_bounded_and_repeats() {
        let extents = extents(UpscalerMode::Fsr3(FsrQuality::Quality));
        let mut state = FsrTemporalState::new(extents).unwrap();
        let first = state.current();
        assert!(first
            .pixel
            .into_iter()
            .all(|component| component.abs() <= 1.0));
        assert_ne!(first.pixel, [0.0, 0.0]);

        for _ in 0..state.phase_count() {
            state.mark_dispatch_completed();
        }
        assert_eq!(state.current(), first);
        assert!(!state.reset_pending());

        state.signal_reset();
        assert_eq!(state.current(), first);
        assert_eq!(state.sequence_index(), 0);
        assert!(state.reset_pending());
    }

    #[test]
    fn fsr_pixel_jitter_flips_vulkan_projection_y() {
        let render = vk::Extent2D {
            width: 1280,
            height: 720,
        };
        assert_eq!(
            fsr_pixel_jitter_to_ndc([0.5, 0.25], render),
            [1.0 / 1280.0, -0.5 / 720.0]
        );
    }

    #[test]
    fn motion_adapter_converts_current_uv_minus_previous_to_fsr_pixels() {
        let render = vk::Extent2D {
            width: 1000,
            height: 500,
        };
        assert_eq!(fsr_motion_vector_scale(render), [-1000.0, -500.0]);
        assert_eq!(
            engine_motion_to_fsr_pixels([0.010, -0.020], render),
            [-10.0, 10.0]
        );
        assert_eq!(engine_motion_to_fsr_pixels([0.0, 0.0], render), [0.0, 0.0]);
    }

    #[test]
    fn authored_camera_parameters_survive_a_large_depth_range_exactly() {
        let near = 0.1;
        let far = 300_000.0;
        let fov = 70.0_f32.to_radians();
        let params = fsr_camera_parameters(near, far, fov).unwrap();
        assert_eq!(params.near, near);
        assert_eq!(params.far, far);
        assert_eq!(params.fov_y_radians, fov);
    }
}
