//! Command-buffer open + per-frame clear-value / instance-map setup —
//! extracted from `draw.rs` (#3282 / TD1-2026-08-24-01) to shrink
//! `draw_frame`. Covers the command buffer reset + begin, the HDR/G-buffer
//! clear-value assembly, and the `draw_idx → ssbo_idx` instance map that the
//! later TLAS build and instance-SSBO builder both consume; the recording
//! order is unchanged from the pre-split `draw_frame`.

use super::{DrawCommand, SkyParams, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use std::time::Instant;

/// Output of [`VulkanContext::begin_frame_recording`] — the command buffer
/// handle plus the two small per-frame values downstream phases need
/// (`clear_values` cannot be returned already wrapped in a
/// `vk::RenderPassBeginInfo` because that type borrows the array by
/// reference; the caller rebuilds the `RenderPassBeginInfo` from the
/// returned array instead, exactly where the pre-split `draw_frame` did).
pub(super) struct BeginFrameOutput {
    pub(super) cmd: vk::CommandBuffer,
    pub(super) clear_values: [vk::ClearValue; 9],
    pub(super) instance_map: Vec<Option<u32>>,
    /// Start instant for the "TLAS build" timing bucket. Named oddly early
    /// (before camera assembly / skin dispatch even run) because the
    /// pre-split `draw_frame` started this clock here and only read it back
    /// after the real `build_tlas` call much later — the elapsed span
    /// genuinely covers everything in between. Preserved verbatim.
    pub(super) tlas_t0: Instant,
}

impl VulkanContext {
    /// Reset + begin this frame's command buffer, assemble the render
    /// pass's clear values, and precompute the `draw_idx → ssbo_idx`
    /// instance map. Extracted verbatim from `draw_frame` — the recording
    /// order is unchanged.
    pub(super) fn begin_frame_recording(
        &mut self,
        frame: usize,
        draw_commands: &[DrawCommand],
        clear_color: [f32; 4],
        sky_params: &SkyParams,
    ) -> Result<BeginFrameOutput> {
        // Record command buffer. Indexed by frame-in-flight (not swapchain
        // image) so the fence and command buffer share the same slot — #259.
        // Safe because in_flight[frame] was just waited on, guaranteeing
        // the GPU has finished with this cmd buffer's previous recording.
        let cmd = self.command_buffers[frame];
        // SAFETY: `cmd` is `command_buffers[frame]`, whose fence `in_flight[frame]` was just waited on above, so the GPU has finished its previous recording and the buffer is safe to reset. On error we clear the pending acquire signal before propagating.
        unsafe {
            if let Err(e) = self
                .device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .context("reset_command_buffer")
            {
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: `cmd` was just reset (above) and is in the initial state; it is recorded by this thread only, so beginning recording with ONE_TIME_SUBMIT is valid. On error we clear the pending acquire signal before propagating.
        unsafe {
            if let Err(e) = self
                .device
                .begin_command_buffer(cmd, &begin_info)
                .context("begin_command_buffer")
            {
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        // 8 color attachments + depth. Order must match the render pass:
        //   0 HDR, 1 normal, 2 motion, 3 mesh_id, 4 raw_indirect, 5 albedo,
        //   6 fsr_reactive, 7 fsr_transparency, 8 depth.
        let zero_f = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        // #2466 / REN-D8-N01 — when `composite.frag`'s sky branch owns the
        // background, the HDR attachment is a pure accumulator for whatever
        // transparent geometry draws over the sky: composite now adds
        // `direct` on top of `compute_sky(dir)` weighted by the alpha
        // coverage lane (`pipeline::coverage_alpha_factors`), instead of
        // discarding it. An opaque placeholder clear there would tint every
        // such pixel (and feed the bloom pyramid a flat wash), so clear to
        // transparent black — zero colour, zero coverage — leaving the
        // caller's `clear_color` for frames composite actually shows it on
        // (interiors and the loose-NIF demo). The gate is
        // `sky_params.is_exterior`, the *same* value that becomes
        // `depth_params.x` above, so host and shader cannot disagree about
        // who owns the background.
        let hdr_clear = if sky_params.is_exterior {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            clear_color
        };
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue { float32: hdr_clear },
            },
            zero_f, // normal
            zero_f, // motion
            vk::ClearValue {
                // Mesh ID: 0 reserved for background (shader writes id + 1).
                color: vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
            },
            zero_f, // raw_indirect (background: no light)
            zero_f, // albedo (background: no color)
            // Both FSR masks clear to zero — "fully described by depth and
            // motion" — and only transparent draws MAX-blend a value in.
            zero_f, // fsr_reactive
            zero_f, // fsr_transparency
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        // Pre-compute the shared `draw_idx → ssbo_idx` map once so the
        // TLAS `instance_custom_index` values stay in lockstep with the
        // compacted SSBO positions regardless of which filter rejects a
        // draw_cmd. Before #419 the TLAS path used the raw enumerate
        // index while the SSBO builder used `gpu_instances.len()` —
        // identical only while `mesh_registry.get()` never returned None
        // for a submitted command. A single evicted mesh would shift
        // every subsequent SSBO entry by one while TLAS custom indices
        // stayed put, producing silently-wrong material/transform reads
        // on every RT hit downstream (shadows / reflections / GI /
        // caustics / primary-hit fallback in `triangle.frag`). See
        // `AccelerationManager::build_tlas` (`vulkan::acceleration::tlas`) and
        // the SSBO builder below — both must honour this map. (#2692 — the
        // path anchor here predated the `acceleration.rs` → `acceleration/`
        // split; symbols survive refactors, file paths do not.)
        let tlas_t0 = Instant::now();
        let instance_map: Vec<Option<u32>> = super::super::acceleration::build_instance_map(
            draw_commands.len(),
            super::super::scene_buffer::MAX_INSTANCES,
            |i| {
                self.mesh_registry
                    .get(draw_commands[i].mesh_handle)
                    .is_some()
            },
        );

        Ok(BeginFrameOutput {
            cmd,
            clear_values,
            instance_map,
            tlas_t0,
        })
    }
}
