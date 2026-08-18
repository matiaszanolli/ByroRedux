//! Structured renderer debug views and the bounded selected-ray probe.

use std::fmt;
use std::str::FromStr;

/// Named full-frame renderer diagnostic.
///
/// The legacy `BYROREDUX_RENDER_DEBUG` bitmask remains available for narrow
/// feature ablations. This enum owns the mutually-exclusive, operator-facing
/// correctness views so compound bit patterns no longer have to be memorised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum RenderDebugMode {
    /// Preserve the launch-time legacy bitmask's categorical view selection.
    #[default]
    LegacyFlags = crate::shader_constants::RENDER_DEBUG_LEGACY_FLAGS,
    Final = crate::shader_constants::RENDER_DEBUG_FINAL,
    ShadowVisibility = crate::shader_constants::RENDER_DEBUG_SHADOW_VISIBILITY,
    SelectedLight = crate::shader_constants::RENDER_DEBUG_SELECTED_LIGHT,
    DirectOnly = crate::shader_constants::RENDER_DEBUG_DIRECT_ONLY,
    IndirectOnly = crate::shader_constants::RENDER_DEBUG_INDIRECT_ONLY,
    MaterialLobe = crate::shader_constants::RENDER_DEBUG_MATERIAL_LOBE,
    CompositeTerm = crate::shader_constants::RENDER_DEBUG_COMPOSITE_TERM,
    RtLod = crate::shader_constants::RENDER_DEBUG_RT_LOD,
    VolumetricTerm = crate::shader_constants::RENDER_DEBUG_VOLUMETRIC_TERM,
}

impl RenderDebugMode {
    pub const USER_MODES: [Self; 9] = [
        Self::Final,
        Self::ShadowVisibility,
        Self::SelectedLight,
        Self::DirectOnly,
        Self::IndirectOnly,
        Self::MaterialLobe,
        Self::CompositeTerm,
        Self::RtLod,
        Self::VolumetricTerm,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyFlags => "legacy_flags",
            Self::Final => "final",
            Self::ShadowVisibility => "shadow_visibility",
            Self::SelectedLight => "selected_light",
            Self::DirectOnly => "direct_only",
            Self::IndirectOnly => "indirect_only",
            Self::MaterialLobe => "material_lobe",
            Self::CompositeTerm => "composite_term",
            Self::RtLod => "rt_lod",
            Self::VolumetricTerm => "volumetric_term",
        }
    }

    pub const fn shader_value(self) -> u32 {
        self as u32
    }

    pub fn user_mode_names() -> String {
        Self::USER_MODES
            .iter()
            .map(|mode| mode.as_str())
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl fmt::Display for RenderDebugMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RenderDebugMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "legacy" | "legacy_flags" => Ok(Self::LegacyFlags),
            "final" => Ok(Self::Final),
            "shadow" | "visibility" | "shadow_visibility" => Ok(Self::ShadowVisibility),
            "selected" | "selected_light" => Ok(Self::SelectedLight),
            "direct" | "direct_only" => Ok(Self::DirectOnly),
            "indirect" | "indirect_only" => Ok(Self::IndirectOnly),
            "material" | "material_lobe" => Ok(Self::MaterialLobe),
            "composite" | "composite_term" => Ok(Self::CompositeTerm),
            "lod" | "rt_lod" => Ok(Self::RtLod),
            "volume" | "volumetric" | "volumetric_term" => Ok(Self::VolumetricTerm),
            _ => Err(format!(
                "unknown render debug mode '{value}' (expected {})",
                Self::user_mode_names()
            )),
        }
    }
}

/// Result of one bounded selected-light visibility-ray request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectedRayProbeResult {
    pub generation: u32,
    pub pixel: [u32; 2],
    /// False when no eligible main-pass fragment reached the probe site.
    pub fragment_captured: bool,
    /// False when the captured fragment had no selected visibility ray.
    pub ray_valid: bool,
    pub selected_light_index: Option<u32>,
    pub visibility_mask: u32,
    pub ray_origin: [f32; 3],
    pub ray_t_min: f32,
    pub ray_direction: [f32; 3],
    pub ray_t_max: f32,
    pub committed_hit_instance: Option<u32>,
    pub committed_hit_distance: Option<f32>,
    pub averaged_visibility: [f32; 3],
    /// Exact four-`vec4` GPU light record addressed by the selected index.
    pub light_position_radius: [f32; 4],
    pub light_color_type: [f32; 4],
    pub light_direction_angle: [f32; 4],
    pub light_params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectedRayProbeRequest {
    pub generation: u32,
    pub pixel: [u32; 2],
}

impl SelectedRayProbeResult {
    pub(crate) fn from_gpu(record: crate::vulkan::scene_buffer::GpuSelectedRayProbe) -> Self {
        let fragment_captured =
            record.control[1] == crate::vulkan::scene_buffer::GpuSelectedRayProbe::STATUS_READY;
        let ray_valid = fragment_captured
            && record.ids[3] & crate::vulkan::scene_buffer::GpuSelectedRayProbe::FLAG_RAY_VALID
                != 0;
        let valid_index = |value| (value != u32::MAX).then_some(value);
        Self {
            generation: record.control[0],
            pixel: [record.control[2], record.control[3]],
            fragment_captured,
            ray_valid,
            selected_light_index: valid_index(record.ids[0]),
            visibility_mask: record.ids[1],
            ray_origin: [
                record.origin_tmin[0],
                record.origin_tmin[1],
                record.origin_tmin[2],
            ],
            ray_t_min: record.origin_tmin[3],
            ray_direction: [
                record.direction_tmax[0],
                record.direction_tmax[1],
                record.direction_tmax[2],
            ],
            ray_t_max: record.direction_tmax[3],
            committed_hit_instance: valid_index(record.ids[2]),
            committed_hit_distance: (record.hit_visibility[0].is_finite())
                .then_some(record.hit_visibility[0]),
            averaged_visibility: [
                record.hit_visibility[1],
                record.hit_visibility[2],
                record.hit_visibility[3],
            ],
            light_position_radius: record.light_position_radius,
            light_color_type: record.light_color_type,
            light_direction_angle: record.light_direction_angle,
            light_params: record.light_params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_debug_mode_parser_accepts_canonical_names_and_aliases() {
        assert_eq!(
            "shadow_visibility".parse(),
            Ok(RenderDebugMode::ShadowVisibility)
        );
        assert_eq!("selected-light".parse(), Ok(RenderDebugMode::SelectedLight));
        assert_eq!("direct".parse(), Ok(RenderDebugMode::DirectOnly));
        assert_eq!("rt-lod".parse(), Ok(RenderDebugMode::RtLod));
        assert_eq!("volumetric".parse(), Ok(RenderDebugMode::VolumetricTerm));
    }

    #[test]
    fn user_mode_values_match_the_dense_shader_contract() {
        for (index, mode) in RenderDebugMode::USER_MODES.iter().enumerate() {
            assert_eq!(mode.shader_value(), index as u32);
        }
        assert_eq!(
            RenderDebugMode::LegacyFlags.shader_value(),
            crate::shader_constants::RENDER_DEBUG_LEGACY_FLAGS
        );
    }

    #[test]
    fn armed_probe_without_fragment_decodes_as_completed_no_fragment_result() {
        let record = crate::vulkan::scene_buffer::GpuSelectedRayProbe::armed(7, [12, 34]);
        let result = SelectedRayProbeResult::from_gpu(record);
        assert_eq!(result.generation, 7);
        assert_eq!(result.pixel, [12, 34]);
        assert!(!result.fragment_captured);
        assert!(!result.ray_valid);
        assert_eq!(result.selected_light_index, None);
    }

    #[test]
    fn ready_probe_decodes_indices_geometry_visibility_and_light_record() {
        let mut record = crate::vulkan::scene_buffer::GpuSelectedRayProbe::armed(9, [2, 3]);
        record.control[1] = crate::vulkan::scene_buffer::GpuSelectedRayProbe::STATUS_READY;
        record.ids = [5, 0x13, 42, 1];
        record.origin_tmin = [1.0, 2.0, 3.0, 0.0];
        record.direction_tmax = [0.0, 1.0, 0.0, 99.0];
        record.hit_visibility = [17.0, 0.2, 0.4, 0.6];
        record.light_params = [7.0, 8.0, 9.0, 10.0];
        let result = SelectedRayProbeResult::from_gpu(record);
        assert!(result.fragment_captured);
        assert!(result.ray_valid);
        assert_eq!(result.selected_light_index, Some(5));
        assert_eq!(result.visibility_mask, 0x13);
        assert_eq!(result.committed_hit_instance, Some(42));
        assert_eq!(result.committed_hit_distance, Some(17.0));
        assert_eq!(result.averaged_visibility, [0.2, 0.4, 0.6]);
        assert_eq!(result.light_params, [7.0, 8.0, 9.0, 10.0]);
    }
}
