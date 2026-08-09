//! Post-geometry pass recording — water-caustic barrier, SVGF denoise,
//! caustic splat, volumetrics, TAA, SSAO, bloom, and the final composite.
//! Extracted from `draw.rs` (#1857 / TD1-001) to shrink that file; the
//! recording order and per-pass permanent-failure latches are unchanged.
//!
//! Also carries `copy_depth_to_history`, the small depth→history-image
//! copy that feeds soft-particle fade and runs immediately around the
//! post-pass sequence.

use super::super::descriptors::memory_barrier;
use super::super::frame_upscaler::{FsrFrameParameters, UpscaleDispatchInputs};
use super::super::presentation::{ImageSpaceModifierView, PresentationFrame};
use super::{SkyParams, VulkanContext};
use anyhow::Result;
use ash::vk;

/// Inputs for [`VulkanContext::record_volumetrics_pass`] (#2258 / TD1-080).
/// A named-field struct rather than positional arguments — this pass has
/// the largest parameter surface of the extracted post-passes, and several
/// fields share a type (`f32`, `[f32; 3]`) that a positional call could
/// silently transpose.
struct VolumetricsPassInputs<'a> {
    camera_pos: [f32; 3],
    render_origin: byroredux_core::math::Vec3,
    prev_view_proj: &'a [f32; 16],
    inv_vp_arr: [[f32; 4]; 4],
    previous_camera_pos: [f32; 3],
    frame_counter: u32,
    volumetric_time_seconds: f32,
    sky_params: &'a SkyParams,
    fog_color: [f32; 3],
    fog_far: f32,
    fog_extinction_per_meter: f32,
    fog_single_scatter_albedo: f32,
    fog_coverage: f32,
    fog_height_reference: f32,
    fog_volumes: &'a [super::super::volumetrics::GpuFogVolume],
}

impl VulkanContext {
    /// Copy the live depth buffer into the sampleable depth-history image
    /// for next frame's soft-particle fade. Called once per frame right
    /// after the main render pass ends, while the depth image sits in
    /// `DEPTH_STENCIL_READ_ONLY_OPTIMAL` (the render pass's final layout).
    ///
    /// Layout dance:
    ///   depth:   READ_ONLY → TRANSFER_SRC → (copy) → READ_ONLY (restored
    ///            so SSAO / SVGF / composite read it exactly as before).
    ///   history: SHADER_READ_ONLY → TRANSFER_DST → (copy) → SHADER_READ_ONLY.
    ///
    /// # Safety
    /// `cmd` is the current frame's primary command buffer, recording and
    /// outside any render pass. `depth_image` / `depth_history_image` are
    /// live, same-extent, same-format (`D32_SFLOAT`) depth images.
    pub(super) fn copy_depth_to_history(&self, cmd: vk::CommandBuffer) {
        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        let depth_to_src = vk::ImageMemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags::SHADER_READ,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_image)
            .subresource_range(range);
        let hist_to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_history_image)
            .subresource_range(range);

        let layers = vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        };
        let copy = vk::ImageCopy::default()
            .src_subresource(layers)
            .dst_subresource(layers)
            .extent(vk::Extent3D {
                width: self.frame_extents.render.width,
                height: self.frame_extents.render.height,
                depth: 1,
            });

        let depth_restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_image)
            .subresource_range(range);
        let hist_to_read = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_history_image)
            .subresource_range(range);

        // SAFETY: `cmd` is recording and outside any render pass (caller contract); `depth_image` / `depth_history_image` are live, same-extent D32_SFLOAT images. The barriers correctly bracket the READ_ONLY->TRANSFER_SRC / SHADER_READ->TRANSFER_DST transitions around the copy and restore both layouts; no other access to these images is recorded between the barriers.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[depth_to_src, hist_to_dst],
            );
            self.device.cmd_copy_image(
                cmd,
                self.depth_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.depth_history_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[copy],
            );
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[depth_restore, hist_to_read],
            );
        }
    }

    /// Record the post-geometry passes: water-caustic barrier, SVGF
    /// denoise, caustic splat, volumetrics, TAA, SSAO, bloom, scene
    /// composition, reconstruction, and presentation, in that fixed order.
    /// Extracted from `draw_frame` (#1748 / TD1-001) to shrink that
    /// function; recording order and the per-pass permanent-failure
    /// latches are preserved exactly. Call after the main render pass
    /// ends and before `end_command_buffer`.
    ///
    /// #2258 (TD1-080) split the single 556-LOC body into one
    /// `record_<pass>_pass` helper per self-contained pass, called here in
    /// the same fixed sequence — a call-order-preserving decomposition
    /// only, no barrier or pass reordering. Each helper carries its own
    /// `unsafe` scope and `# Safety` doc comment instead of one covering
    /// the whole function, since that's the granularity the underlying
    /// `*.dispatch()` / `*.record()` calls are actually unsafe at.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_post_passes(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        img: usize,
        camera_static: bool,
        camera_pos: [f32; 3],
        render_origin: byroredux_core::math::Vec3,
        vp: &[f32; 16],
        prev_view_proj: &[f32; 16],
        inv_vp_arr: [[f32; 4]; 4],
        previous_camera_pos: [f32; 3],
        frame_counter: u32,
        volumetric_time_seconds: f32,
        sky_params: &SkyParams,
        fog_color: [f32; 3],
        fog_far: f32,
        fog_extinction_per_meter: f32,
        fog_single_scatter_albedo: f32,
        fog_coverage: f32,
        fog_height_reference: f32,
        fog_volumes: &[super::super::volumetrics::GpuFogVolume],
        fsr_frame: Option<FsrFrameParameters>,
        underwater: [f32; 4],
        image_space_modifier: ImageSpaceModifierView,
    ) -> Result<()> {
        self.record_svgf_pass(cmd, frame);
        self.record_caustic_splat_pass(cmd, frame, camera_static);
        self.record_volumetrics_pass(
            cmd,
            frame,
            VolumetricsPassInputs {
                camera_pos,
                render_origin,
                prev_view_proj,
                inv_vp_arr,
                previous_camera_pos,
                frame_counter,
                volumetric_time_seconds,
                sky_params,
                fog_color,
                fog_far,
                fog_extinction_per_meter,
                fog_single_scatter_albedo,
                fog_coverage,
                fog_height_reference,
                fog_volumes,
            },
        );
        self.record_taa_pass(cmd, frame);
        self.record_ssao_pass(cmd, frame, vp, inv_vp_arr, camera_pos, render_origin);
        self.record_bloom_pass(cmd, frame);
        self.record_composite_pass(cmd, frame);
        self.record_upscale_pass(cmd, frame, fsr_frame);
        self.record_presentation_pass(cmd, frame, img, underwater, image_space_modifier);
        Ok(())
    }

    /// Water-caustic barrier + SVGF temporal accumulation (#2258 /
    /// TD1-080, extracted from `record_post_passes`). The water barrier
    /// is a two-line prerequisite for the SVGF dispatch immediately
    /// following it, not a self-contained pass of its own, so it's
    /// bundled here rather than split out separately.
    ///
    /// SVGF reprojects previous frame's accumulated indirect, blends with
    /// raw 1-SPP indirect at α=0.2. Reads G-buffer raw_indirect/motion/
    /// mesh_id (now in SHADER_READ_ONLY_OPTIMAL via render pass
    /// final_layout) + history from previous frame's SVGF output slot,
    /// writes this frame's accumulated indirect + moments. Composite
    /// samples the output later in the sequence.
    ///
    /// SVGF permanent-failure latch: after the first dispatch error, skip
    /// all further attempts and leave the warn-log behind (escalated to
    /// `error!` so the once-per-session signal stands out). Composite's
    /// `indirectTex` descriptor keeps pointing at the stale denoised
    /// image until the next `recreate_swapchain` resets the latch.
    /// Rebinding to the raw-indirect G-buffer view would give a live
    /// (noisy) picture but requires composite-side plumbing deferred
    /// until a real lost-device repro. See #479.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_svgf_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording outside a render pass, and every optional
        // pipeline/resource used here is owned by this live context for `frame`.
        unsafe {
            if let Some(ref wca) = self.water_caustic_accum {
                wca.barrier_post_render_pass(&self.device, cmd, frame);
            }

            if !self.svgf_failed {
                // Captured before the &mut self.svgf borrow: the à-trous
                // pass reads DBG_DISABLE_ATROUS out of the same render-debug
                // bitmask the fragment shader sees (env-set; console legacy
                // toggle is light-atten-only and not relevant here).
                let svgf_dbg_flags = self.render_debug_flags;
                if let Some(ref mut svgf) = self.svgf {
                    // #674 temporal α state machine + UBO host write
                    // both ran BEFORE the bulk pre-render barrier
                    // above (#961 / REN-D10-NEW-04 fold). This call
                    // only records the SVGF compute dispatch.
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_svgf_start(&self.device, cmd, frame);
                    }
                    let svgf_result = svgf.dispatch(&self.device, cmd, frame, svgf_dbg_flags);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_svgf_end(&self.device, cmd, frame);
                    }
                    if let Err(e) = svgf_result {
                        log::error!(
                            "SVGF dispatch failed — pass disabled for the rest of the session: {e}"
                        );
                        self.svgf_failed = true;
                    }
                }
            }
        }
    }

    /// Caustic scatter (#321): per-refractive-pixel refracted-light splat
    /// (#2258 / TD1-080, extracted from `record_post_passes`). Runs after
    /// SVGF (reads the same G-buffer slots that are now in
    /// SHADER_READ_ONLY_OPTIMAL) and before composite (which samples the
    /// caustic accumulator). Writes binding 5 of the composite descriptor
    /// set.
    ///
    /// Caustic permanent-failure latch — same shape as SVGF, with one
    /// difference: composite's `causticTex` sampler has no validity gate at
    /// all, and the accumulator is screen-space (doesn't track camera
    /// motion), so a skipped frame (this latch, or no TLAS yet) explicitly
    /// clears the slot instead of leaving stale content to be re-composited
    /// every subsequent frame — see [`super::VulkanContext::caustic_cleared_on_skip`]
    /// and #2507. See #479 SIBLING.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_caustic_splat_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        camera_static: bool,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and the live
        // caustic/TLAS resources are indexed by the current in-flight `frame`.
        unsafe {
            let Some(ref mut caustic) = self.caustic else {
                return;
            };
            // Bind this frame's TLAS before dispatch — the AccelerationManager
            // rebuilds/refits per frame but the handle is stable across frames
            // once created, so we write it once and then again defensively.
            // Skip the dispatch entirely when no TLAS is available for this
            // frame (RT unsupported or scene-load not yet settled). Mirrors
            // the shader's `sceneFlags.x < 0.5` early-out — pre-#640 the
            // dispatch ran every frame regardless and the shader paid full
            // ray-query cost against unwritten / stale TLAS state.
            let ran = if !self.caustic_failed {
                let tlas_handle = self
                    .accel_manager
                    .as_ref()
                    .and_then(|accel| accel.tlas_handle(frame));
                match tlas_handle {
                    Some(tlas) => {
                        caustic.write_tlas(&self.device, frame, tlas);
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_caustic_splat_start(&self.device, cmd, frame);
                        }
                        let caustic_result =
                            caustic.dispatch(&self.device, cmd, frame, camera_static);
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_caustic_splat_end(&self.device, cmd, frame);
                        }
                        match caustic_result {
                            Ok(()) => true,
                            Err(e) => {
                                log::error!(
                                    "Caustic dispatch failed — pass disabled for the rest of the session: {e}"
                                );
                                self.caustic_failed = true;
                                false
                            }
                        }
                    }
                    None => false,
                }
            } else {
                false
            };
            // #2507 — skipped this frame (permanent failure latch, or no
            // TLAS yet)? Clear once per frame-slot so composite's
            // unconditional `causticTex` sample degrades to black instead
            // of re-compositing a frozen, camera-motion-blind accumulator
            // pattern every subsequent frame. See `caustic_skip_clear_decision`.
            let (should_clear, next_latch) =
                caustic_skip_clear_decision(ran, self.caustic_cleared_on_skip[frame]);
            self.caustic_cleared_on_skip[frame] = next_latch;
            if should_clear {
                caustic.clear_for_skip(&self.device, cmd, frame);
            }
        }
    }

    /// Volumetric lighting (M55 Phase 2c — sun-only injection with HG
    /// phase + RT shadow visibility) (#2258 / TD1-080, extracted from
    /// `record_post_passes`). Runs before TAA / SSAO / composite so the
    /// fragment shader can sample the integrated volume.
    ///
    /// **Composite-output gate (#928, flipped live by 977eb95a).**
    ///
    /// `VOLUMETRIC_OUTPUT_CONSUMED` (volumetrics.rs) is now `true`: the
    /// per-froxel single-shadow-ray banding that justified the original
    /// `* 0.0` discard (diagnosed 2026-05-09 against Prospector cups and
    /// lanterns) was addressed, and `composite.frag` consumes the integrated
    /// volume every frame (`combined = combined * vol.a + vol.rgb`). The inject
    /// and integrate dispatches are live GPU work, not dead weight; do not
    /// "optimize" them away as unused work. See #928 / 977eb95a.
    ///
    /// Gated on TLAS being available, mirroring caustic (caustic.rs:627 /
    /// draw.rs:1648). When no TLAS exists (RT unsupported, scene not yet
    /// built, accel_manager absent) we skip BOTH the descriptor write and
    /// the dispatch — composite reads the prior frame's integrated
    /// volume, which retains its last valid contents (or the
    /// post-`initialize_layouts` zero-init on the very first frame).
    ///
    /// Sun direction + radiance are plumbed from `SkyParams::sun_direction`
    /// / `sun_color` / `sun_intensity` (#1022 / REN-D18-008). Below-horizon
    /// (`sun_intensity <= 0`) still zeros `sun_color` regardless of
    /// interior/exterior — no sun, no godrays, trivially correct either
    /// way.
    ///
    /// Interior vs exterior no longer zeroes sun/scattering outright
    /// (pre-fix behavior — see git blame for the old gate). That approach
    /// was too blunt: it also blocked real sun-through-window godrays the
    /// moment #928 flips VOLUMETRIC_OUTPUT_CONSUMED on. Instead the inject
    /// shader itself distinguishes "real window" from "geometry gap" via
    /// `render_origin.w` (is_exterior) — see the two-pass shadow-ray note
    /// on `VolumetricsParams::render_origin` in `volumetrics.rs` and the
    /// interior-godray investigation: a `--cell`-loaded interior has no
    /// complete ceiling mesh (never seen from inside, so Bethesda
    /// authoring omits it), so a naive single opaque-mask shadow ray
    /// escaping upward would register as "lit" everywhere, not just
    /// through real windows.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_volumetrics_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        inputs: VolumetricsPassInputs<'_>,
    ) {
        let VolumetricsPassInputs {
            camera_pos,
            render_origin,
            prev_view_proj,
            inv_vp_arr,
            previous_camera_pos,
            frame_counter,
            volumetric_time_seconds,
            sky_params,
            fog_color,
            fog_far,
            fog_extinction_per_meter,
            fog_single_scatter_albedo,
            fog_coverage,
            fog_height_reference,
            fog_volumes,
        } = inputs;
        // SAFETY: `cmd` is recording outside a render pass, and all volumetric,
        // cluster, TLAS, and timer resources belong to this live context/frame.
        unsafe {
            if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                if let Some(ref mut vol) = self.volumetrics {
                    let scatter_coef = fog_extinction_per_meter.max(0.0)
                        / super::super::volumetrics::WORLD_UNITS_PER_METER;
                    if scatter_coef <= 0.0 && fog_volumes.is_empty() {
                        vol.record_neutral_frame(&self.device, cmd, frame);
                    } else {
                        let vol_tlas = self
                            .accel_manager
                            .as_ref()
                            .and_then(|accel| accel.tlas_handle(frame));
                        // Phase 2b point/spot light injection needs the SAME
                        // per-frame cluster grid / light-index buffers the
                        // fragment shader reads — reused rather than building a
                        // separate froxel-space light-culling structure. No
                        // cluster_cull pipeline (RT unsupported / not yet built)
                        // means no lights this frame; skip injection entirely
                        // rather than binding stale/undefined buffers.
                        let vol_lights = self.cluster_cull.as_ref().map(|cc| {
                            (
                                self.scene_buffers.light_buffers()[frame].buffer,
                                self.scene_buffers.light_buffer_size(),
                                cc.scene_cluster_grid_buffers[frame],
                                cc.scene_light_index_buffers[frame],
                            )
                        });
                        if let (
                            Some(tlas),
                            Some((light_buf, light_buf_size, grid_buf, index_buf)),
                        ) = (vol_tlas, vol_lights)
                        {
                            vol.write_tlas(&self.device, frame, tlas);
                            // Cluster grid / light-index buffer sizes mirror the
                            // formulas in `ClusterCullPipeline::new`
                            // (`compute.rs`): grid entries are `{offset:u32,
                            // count:u32}` = 8 B each; the index list is one u32
                            // per (cluster, light-slot) pair.
                            const CLUSTER_ENTRY_SIZE: vk::DeviceSize = 8;
                            let grid_size = CLUSTER_ENTRY_SIZE
                                * crate::shader_constants::TOTAL_CLUSTERS as vk::DeviceSize;
                            let index_size = std::mem::size_of::<u32>() as vk::DeviceSize
                                * crate::shader_constants::TOTAL_CLUSTERS as vk::DeviceSize
                                * crate::shader_constants::MAX_LIGHTS_PER_CLUSTER as vk::DeviceSize;
                            // Compute→compute visibility: cluster_cull's own
                            // trailing barrier (draw_frame, ~line 2960) only
                            // targets FRAGMENT_SHADER (the rasterizer's read).
                            // This dispatch reads the same buffers from a LATER
                            // COMPUTE_SHADER stage, which that barrier does not
                            // cover — a separate barrier is required by the
                            // Vulkan spec even though both writes happened
                            // earlier in the same command buffer.
                            memory_barrier(
                                &self.device,
                                cmd,
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::AccessFlags::SHADER_WRITE,
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::AccessFlags::SHADER_READ,
                            );
                            vol.write_lights_and_clusters(
                                &self.device,
                                frame,
                                light_buf,
                                light_buf_size,
                                grid_buf,
                                grid_size,
                                index_buf,
                                index_size,
                            );
                            let sun_radiance = if sky_params.sun_intensity > 0.0 {
                                [
                                    sky_params.sun_color[0] * sky_params.sun_intensity,
                                    sky_params.sun_color[1] * sky_params.sun_intensity,
                                    sky_params.sun_color[2] * sky_params.sun_intensity,
                                    fog_far,
                                ]
                            } else {
                                [0.0, 0.0, 0.0, fog_far]
                            };
                            let vol_params = super::super::volumetrics::VolumetricsParams {
                                inv_view_proj: inv_vp_arr,
                                prev_view_proj: [
                                    [
                                        prev_view_proj[0],
                                        prev_view_proj[1],
                                        prev_view_proj[2],
                                        prev_view_proj[3],
                                    ],
                                    [
                                        prev_view_proj[4],
                                        prev_view_proj[5],
                                        prev_view_proj[6],
                                        prev_view_proj[7],
                                    ],
                                    [
                                        prev_view_proj[8],
                                        prev_view_proj[9],
                                        prev_view_proj[10],
                                        prev_view_proj[11],
                                    ],
                                    [
                                        prev_view_proj[12],
                                        prev_view_proj[13],
                                        prev_view_proj[14],
                                        prev_view_proj[15],
                                    ],
                                ],
                                camera_pos: [
                                    camera_pos[0],
                                    camera_pos[1],
                                    camera_pos[2],
                                    scatter_coef,
                                ],
                                prev_camera_pos: [
                                    previous_camera_pos[0],
                                    previous_camera_pos[1],
                                    previous_camera_pos[2],
                                    0.0,
                                ],
                                sun_dir: [
                                    sky_params.sun_direction[0],
                                    sky_params.sun_direction[1],
                                    sky_params.sun_direction[2],
                                    super::super::volumetrics::DEFAULT_PHASE_G,
                                ],
                                sun_color: sun_radiance,
                                volume_params: [
                                    vol.far_distance_world(),
                                    super::super::volumetrics::LINEAR_DEPTH,
                                    super::super::volumetrics::LINEAR_SLICE_FRACTION,
                                    (frame_counter & 0x00ff_ffff) as f32,
                                ],
                                // #markarth-precision — inv_view_proj is relative;
                                // the inject shader adds this to recover absolute
                                // froxel positions for the TLAS shadow rays. w =
                                // is_exterior — see doc comment on the struct field.
                                render_origin: [
                                    render_origin.x,
                                    render_origin.y,
                                    render_origin.z,
                                    if sky_params.is_exterior { 1.0 } else { 0.0 },
                                ],
                                medium_params: [
                                    fog_single_scatter_albedo.clamp(0.0, 1.0),
                                    super::super::volumetrics::DEFAULT_BACKWARD_PHASE_G,
                                    super::super::volumetrics::DEFAULT_DUAL_LOBE_MIX,
                                    super::super::volumetrics::DEFAULT_SCALE_HEIGHT_METERS
                                        * super::super::volumetrics::WORLD_UNITS_PER_METER,
                                ],
                                fog_tint: super::super::volumetrics::pack_fog_tint(fog_color),
                                temporal_params: [
                                    super::super::volumetrics::DEFAULT_TEMPORAL_HISTORY_WEIGHT,
                                    super::super::volumetrics::DEFAULT_DENSITY_REJECTION,
                                    volumetric_time_seconds,
                                    fog_coverage.clamp(0.01, 1.0),
                                ],
                                // Filled by `dispatch` after it builds the
                                // camera-centered local-volume cluster grid.
                                local_volume_grid: [0.0; 4],
                                fog_reference: [fog_height_reference, 0.0, 0.0, 0.0],
                            };
                            if let Some(ref mut timers) = self.gpu_timers {
                                timers.cmd_volumetrics_start(&self.device, cmd, frame);
                            }
                            let vol_result =
                                vol.dispatch(&self.device, cmd, frame, &vol_params, fog_volumes);
                            if let Some(ref mut timers) = self.gpu_timers {
                                timers.cmd_volumetrics_end(&self.device, cmd, frame);
                            }
                            if let Err(e) = vol_result {
                                log::warn!("Volumetrics dispatch failed: {e}");
                            }
                        } else {
                            // Never let a prior cell's integrated fog hang over a
                            // frame whose TLAS/cluster inputs are not ready yet.
                            vol.record_neutral_frame(&self.device, cmd, frame);
                        }
                    }
                }
            }
        }
    }

    /// TAA resolve (#2258 / TD1-080, extracted from `record_post_passes`):
    /// reprojects previous frame's history via motion vectors,
    /// neighborhood-clamps in YCoCg, and writes the anti-aliased HDR
    /// result for composite to sample. Runs after SVGF (which denoises
    /// the indirect term) and before SSAO/composite.
    ///
    /// TAA permanent-failure recovery: on the first dispatch error the
    /// composite's binding 0 (which currently points at TAA's output)
    /// gets rebound to the raw HDR render-pass attachments so the screen
    /// keeps updating — without the fallback the last TAA-written HDR
    /// frame would freeze on screen for the rest of the session with only
    /// a `warn!` log hinting at the cause. See #479.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_taa_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording outside a render pass, and the TAA,
        // composite, and timer resources are live for the current `frame`.
        unsafe {
            if !self.taa_failed {
                if let Some(ref mut taa) = self.taa {
                    // #1194 — bracket the TAA compute dispatch.
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_taa_start(&self.device, cmd, frame);
                    }
                    if let Err(e) = taa.dispatch(&self.device, cmd, frame) {
                        log::error!(
                            "TAA dispatch failed — falling back to raw HDR for the rest of the session: {e}"
                        );
                        self.taa_failed = true;
                        if let Some(ref mut composite) = self.composite {
                            composite.fall_back_to_raw_hdr(&self.device);
                        }
                    }
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_taa_end(&self.device, cmd, frame);
                    }
                }
            }
        }
    }

    /// SSAO compute pass (#2258 / TD1-080, extracted from
    /// `record_post_passes`): reads depth buffer (now in READ_ONLY layout
    /// after render pass), writes AO texture for this frame's fragment
    /// shader. Runs before composite so AO is current-frame (no lag).
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_ssao_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        vp: &[f32; 16],
        inv_vp_arr: [[f32; 4]; 4],
        camera_pos: [f32; 3],
        render_origin: byroredux_core::math::Vec3,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and the SSAO
        // pipeline, descriptors, and timers are live for the current `frame`.
        unsafe {
            if let Some(ref mut ssao) = self.ssao {
                let vp_arr = [
                    [vp[0], vp[1], vp[2], vp[3]],
                    [vp[4], vp[5], vp[6], vp[7]],
                    [vp[8], vp[9], vp[10], vp[11]],
                    [vp[12], vp[13], vp[14], vp[15]],
                ];
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_ssao_start(&self.device, cmd, frame);
                }
                // #markarth-precision — SSAO reconstructs world from the
                // RELATIVE inv_view_proj and uses it only in differences
                // (worldPos - cameraPos, sample - worldPos), which are
                // origin-invariant, so feed the camera in the same relative
                // space. The AO result is unchanged.
                let ssao_cam_rel = [
                    camera_pos[0] - render_origin.x,
                    camera_pos[1] - render_origin.y,
                    camera_pos[2] - render_origin.z,
                ];
                let ssao_result =
                    ssao.dispatch(&self.device, cmd, frame, &vp_arr, &inv_vp_arr, ssao_cam_rel);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_ssao_end(&self.device, cmd, frame);
                }
                if let Err(e) = ssao_result {
                    log::warn!("SSAO dispatch failed: {e}");
                }
            }
        }
    }

    /// Bloom pyramid (M58) (#2258 / TD1-080, extracted from
    /// `record_post_passes`). Reads the raw pre-TAA HDR attachment
    /// (`composite.hdr_image_views[frame]` — the main render pass' HDR
    /// target, NOT TAA's output) and writes a multi-scale blurred
    /// bright-content texture. Composite adds bloom to `combined` before
    /// the ACES tone-map. The render pass's final_layout already moved
    /// HDR to SHADER_READ_ONLY_OPTIMAL, so the input is sample-ready.
    ///
    /// Why pre-TAA: TAA's resolved output is consumed by composite
    /// separately (`composite.rebind_hdr_views` rewires the descriptor at
    /// `context/mod.rs:1715-1717`, but the `hdr_image_views` field still
    /// references the raw attachment — only the descriptor was swapped).
    /// Bloom intentionally shares the raw view because the blur pyramid
    /// smears out sub-pixel jitter, making the bloom haloes spatially
    /// stable anyway. Final image = ACES(TAA-stable base + spatial
    /// bloom). #1166: the previous comment claimed bloom was post-TAA;
    /// that was wrong. #1107 / REN-D19-002 is the original
    /// rewire-composite-to-TAA work this commit references.
    ///
    /// The `if let Some(...)` guard below is dead at runtime (#1276):
    /// `VulkanContext::new` at `context/mod.rs:1958-1967` hard-fails with
    /// `anyhow::anyhow!(...)` if bloom init returns `None` (policy from
    /// #1081 — no fallback binding for bloomTex when bloom is absent), so
    /// the engine never reaches `draw_frame` with `self.bloom == None`.
    /// The `Option` wrapper is kept because the resize-recreate path
    /// benefits from it as a temporary, but the runtime `None` branch is
    /// unreachable.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_bloom_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording outside a render pass, and bloom's input
        // view, pipeline resources, and timers are live for the current frame.
        unsafe {
            if let Some(ref mut bloom) = self.bloom {
                if let Some(ref composite) = self.composite {
                    let hdr_view = composite.hdr_image_views[frame];
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_bloom_start(&self.device, cmd, frame);
                    }
                    let bloom_result = bloom.dispatch(&self.device, cmd, frame, hdr_view);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_bloom_end(&self.device, cmd, frame);
                    }
                    if let Err(e) = bloom_result {
                        log::warn!("Bloom dispatch failed: {e}");
                    }
                }
            }
        }
    }

    /// Compose the complete render-resolution linear-HDR scene (#2258 /
    /// TD1-080, extracted from `record_post_passes`). The upscale
    /// boundary and display mapping are explicit later passes. The main
    /// render pass's outgoing subpass dependency handles the layout
    /// transitions of all input attachments to SHADER_READ_ONLY_OPTIMAL.
    ///
    /// Composite UBO host-write + barrier moved to the pre-render-pass
    /// bulk barrier site (#909 / REN-D1-NEW-03). The dedicated late
    /// HOST→FRAGMENT barrier was correct but isolated 750 lines from the
    /// bulk barrier; folded into it now so all host writes consumed by
    /// the render pass / composite pass share one execution dependency —
    /// that's why no barrier code appears in this function.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_composite_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // SAFETY: `cmd` is recording outside a render pass, and the composite
        // pipeline plus per-frame bindless descriptors remain live here.
        unsafe {
            if let Some(ref composite) = self.composite {
                let bindless_set = self.texture_registry.descriptor_set(frame);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_composite_start(&self.device, cmd, frame);
                }
                composite.dispatch(&self.device, cmd, frame, bindless_set);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_composite_end(&self.device, cmd, frame);
                }
            }
        }
    }

    /// FSR upscale / reconstruction (#2258 / TD1-080, extracted from
    /// `record_post_passes`).
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_upscale_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        fsr_frame: Option<FsrFrameParameters>,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and every image,
        // descriptor, upscaler resource, and timer is live for `frame`.
        unsafe {
            let scene_color = self
                .composite
                .as_ref()
                .expect("composite must exist while recording")
                .scene_image(frame);
            let (motion_vectors, reactive_mask, transparency_mask) = {
                let gbuffer = self
                    .gbuffer
                    .as_ref()
                    .expect("G-buffer must exist while recording");
                (
                    gbuffer.motion_image(frame),
                    gbuffer.reactive_image(frame),
                    gbuffer.transparency_image(frame),
                )
            };
            let exposure_image = self.exposure.as_ref().map(|exposure| exposure.image());
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_upscale_start(&self.device, cmd, frame);
            }
            // #2146 — `record` is infallible on purpose. It runs after
            // `svgf.dispatch`/`taa.dispatch` have latched
            // `dispatched_this_frame`, so an error escaping to `draw_frame`
            // would skip `queue_submit` *and* `mark_frame_completed`, leaving
            // those latches set for a dispatch that never reached the GPU.
            // See `FrameUpscaler::record`'s doc comment before adding any
            // fallible call between here and the submit.
            self.frame_upscaler
                .as_mut()
                .expect("frame upscaler must exist while recording")
                .record(
                    &self.device,
                    cmd,
                    frame,
                    UpscaleDispatchInputs {
                        scene_color,
                        depth: self.depth_image,
                        depth_format: self.depth_format,
                        motion_vectors,
                        exposure: exposure_image,
                        reactive: reactive_mask,
                        transparency: transparency_mask,
                    },
                    fsr_frame,
                );
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_upscale_end(&self.device, cmd, frame);
            }
        }
    }

    /// Final presentation pass (#2258 / TD1-080, extracted from
    /// `record_post_passes`).
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_presentation_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        img: usize,
        underwater: [f32; 4],
        image_space_modifier: ImageSpaceModifierView,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and presentation,
        // exposure, swapchain-image, and timer resources are live for this frame.
        unsafe {
            let exposure = self
                .exposure
                .as_ref()
                .map_or(super::super::exposure::DEFAULT_EXPOSURE, |value| {
                    value.value()
                });
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_presentation_start(&self.device, cmd, frame);
            }
            self.presentation
                .as_ref()
                .expect("presentation pipeline must exist while recording")
                .dispatch(
                    &self.device,
                    cmd,
                    frame,
                    img,
                    PresentationFrame {
                        exposure,
                        underwater,
                        image_space: image_space_modifier,
                    },
                );
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_presentation_end(&self.device, cmd, frame);
            }
        }
    }
}

/// Pure decision for [`VulkanContext::record_caustic_splat_pass`]'s
/// post-dispatch latch update — whether this frame's caustic accumulator
/// slot needs a skip-clear, and the latch's value going into the next
/// frame. Split out so the state machine is unit-testable without a live
/// Vulkan device (#2507), matching the `acceleration::predicates`
/// convention.
///
/// - `ran`: did a real caustic dispatch execute (and succeed) this frame?
/// - `already_cleared`: was this frame-slot's latch already set from a
///   prior skip earlier in the same streak?
///
/// Returns `(should_clear, next_latch)`.
fn caustic_skip_clear_decision(ran: bool, already_cleared: bool) -> (bool, bool) {
    if ran {
        // Fresh content just landed — the next skip (if any) must clear
        // again rather than trust a latch left over from before this
        // dispatch ran.
        (false, false)
    } else if already_cleared {
        // Already skip-cleared earlier in this streak; clearing an
        // already-zero slot every frame is correct but wasteful.
        (false, true)
    } else {
        // First skip of a new streak — clear once.
        (true, true)
    }
}

#[cfg(test)]
mod tests {
    use super::caustic_skip_clear_decision;

    /// A real dispatch running must never trigger a clear, and must reset
    /// the latch so a later skip streak clears fresh.
    #[test]
    fn dispatch_ran_never_clears_and_resets_latch() {
        assert_eq!(caustic_skip_clear_decision(true, false), (false, false));
        assert_eq!(
            caustic_skip_clear_decision(true, true),
            (false, false),
            "a real dispatch must reset a stale latch from a prior streak"
        );
    }

    /// The first skip of a new streak (latch not yet set) must clear.
    #[test]
    fn first_skip_of_a_streak_clears() {
        assert_eq!(caustic_skip_clear_decision(false, false), (true, true));
    }

    /// A subsequent skip in the same streak (latch already set) must NOT
    /// clear again — #2507's fix must not degrade into clearing every
    /// frame while genuinely skipped.
    #[test]
    fn subsequent_skip_in_same_streak_does_not_reclear() {
        assert_eq!(caustic_skip_clear_decision(false, true), (false, true));
    }
}
