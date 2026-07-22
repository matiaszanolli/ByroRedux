//! Runtime upscaler selection and the render/output extent contract.
//!
//! Phase 2 deliberately stops short of dispatching FSR. This module is the
//! single source of truth for preset sizing so every extent-dependent renderer
//! resource can be migrated without duplicating scale math.

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
    /// Existing native-resolution TAA fallback. This remains the default until
    /// every FSR validation gate has passed.
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
}
