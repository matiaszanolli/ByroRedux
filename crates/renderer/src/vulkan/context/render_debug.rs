//! Runtime structured debug-mode and selected-ray probe control.

use super::VulkanContext;
use crate::vulkan::render_debug::{
    RenderDebugMode, SelectedRayProbeRequest, SelectedRayProbeResult,
};

impl VulkanContext {
    pub fn set_render_debug_mode(&mut self, mode: RenderDebugMode) {
        if self.render_debug_mode != mode {
            log::info!("render debug mode: {} -> {}", self.render_debug_mode, mode);
            self.render_debug_mode = mode;
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
}
