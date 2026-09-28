//! Runtime structured debug-mode and selected-ray probe control.

use super::VulkanContext;
use crate::vulkan::render_debug::{
    RenderDebugMode, SelectedRayProbeRequest, SelectedRayProbeResult,
};

/// SVGF elevated-α window for leaving (or entering) a raw-output debug view —
/// the same 8 frames a resize or upscaler switch uses, since every temporal
/// history is equally stale in all three cases.
const RAW_OUTPUT_TOGGLE_RECOVERY_FRAMES: u32 = 8;

/// Whether switching `old` → `new` under `flags` crosses the raw-output
/// boundary. On a raw-output view TAA does not dispatch (its history slots
/// freeze while the G-buffer keeps moving) and FSR is skipped, so the first
/// frame back must reset them rather than reproject a history that is as old
/// as the debug session (#4966).
fn crosses_raw_output_boundary(flags: u32, old: RenderDebugMode, new: RenderDebugMode) -> bool {
    crate::shader_constants::render_debug_requires_raw_output(flags, old.shader_value())
        != crate::shader_constants::render_debug_requires_raw_output(flags, new.shader_value())
}

impl VulkanContext {
    pub fn set_render_debug_mode(&mut self, mode: RenderDebugMode) {
        if self.render_debug_mode != mode {
            log::info!("render debug mode: {} -> {}", self.render_debug_mode, mode);
            let crosses =
                crosses_raw_output_boundary(self.render_debug_flags, self.render_debug_mode, mode);
            self.render_debug_mode = mode;
            if crosses {
                // #4966 — resets TAA, FSR and volumetrics history together.
                self.signal_temporal_discontinuity(RAW_OUTPUT_TOGGLE_RECOVERY_FRAMES);
            }
        }
    }

    pub fn render_debug_mode(&self) -> RenderDebugMode {
        self.render_debug_mode
    }

    /// Queue one bounded selected-light visibility-ray capture.
    ///
    /// Coordinates are render-resolution pixels with the same upper-left
    /// origin used by Vulkan fragment coordinates and the screenshot path.
    pub fn request_selected_ray_probe(&mut self, pixel: [u32; 2]) -> Result<u32, String> {
        let extent = self.frame_extents.render;
        if pixel[0] >= extent.width || pixel[1] >= extent.height {
            return Err(format!(
                "probe pixel ({}, {}) is outside render extent {}x{}",
                pixel[0], pixel[1], extent.width, extent.height
            ));
        }
        let generation = self.next_selected_ray_probe_generation.max(1);
        self.next_selected_ray_probe_generation = generation.wrapping_add(1).max(1);
        self.pending_selected_ray_probe = Some(SelectedRayProbeRequest { generation, pixel });
        Ok(generation)
    }

    pub fn take_selected_ray_probe_result(&mut self) -> Option<SelectedRayProbeResult> {
        self.selected_ray_probe_result.take()
    }

    /// Resolve a ray-query custom index to the ECS entity recorded for the
    /// current TLAS gather. The shader index is an instance-SSBO slot, not an
    /// entity ID or TLAS leaf ordinal.
    pub fn selected_ray_hit_entity_id(&self, instance_ssbo_index: u32) -> Option<u32> {
        self.accel_manager
            .as_ref()?
            .tlas_entity_ids_scratch
            .get(instance_ssbo_index as usize)
            .copied()
    }
}

#[cfg(test)]
mod raw_output_toggle_tests {
    use super::*;

    #[test]
    fn leaving_and_entering_a_raw_view_crosses_the_boundary() {
        for mode in RenderDebugMode::USER_MODES {
            if mode == RenderDebugMode::Final {
                continue;
            }
            assert!(crosses_raw_output_boundary(0, mode, RenderDebugMode::Final));
            assert!(crosses_raw_output_boundary(0, RenderDebugMode::Final, mode));
        }
        assert!(!crosses_raw_output_boundary(
            0,
            RenderDebugMode::Final,
            RenderDebugMode::Final
        ));
        // Raw → raw keeps TAA/FSR skipped; nothing resumes, nothing to reset.
        assert!(!crosses_raw_output_boundary(
            0,
            RenderDebugMode::DirectOnly,
            RenderDebugMode::IndirectOnly
        ));
    }

    /// The setter needs a device, so its call is pinned at source level.
    #[test]
    fn set_render_debug_mode_signals_a_discontinuity_on_the_crossing() {
        let src = crate::source_scan::production_text(include_str!("render_debug.rs"));
        let body = &src[src.find("pub fn set_render_debug_mode(").unwrap()..];
        let body = &body[..body.find("\n    }\n").unwrap()];
        let crosses = body
            .find("crosses_raw_output_boundary(")
            .expect("the setter must compare the old and new raw-output state (#4966)");
        let assign = body.find("self.render_debug_mode = mode;").unwrap();
        let signal = body
            .find("self.signal_temporal_discontinuity(")
            .expect("leaving a raw view must reset TAA/FSR history (#4966)");
        assert!(
            crosses < assign,
            "the old mode must be read before it is overwritten"
        );
        assert!(body[crosses..signal].contains("if crosses"));
    }
}
