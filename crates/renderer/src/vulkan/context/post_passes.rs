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
use super::super::presentation::{ImageSpaceModifier, PresentationFrame, UiOverlayDraw};
use super::{SkyParams, VulkanContext};
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
    /// Clear interiors normally author no participating medium, but a room
    /// with real local emitters still contains enough dust for their shafts
    /// to be visible.  This is deliberately an input rather than a content
    /// name heuristic: every candle, bulb, torch, and authored point/spot
    /// light follows the same physical path.
    local_emitters_present: bool,
    fog_single_scatter_albedo: f32,
    /// #3956 — authored FO4/FO76 altitude profile, or the engine default the
    /// EXAL boundary substituted.
    fog_scale_height_meters: f32,
    fog_coverage: f32,
    fog_height_reference: f32,
    wind_params: [f32; 4],
    wind_gust: [f32; 4],
    fog_volumes: &'a [super::super::volumetrics::GpuFogVolume],
}

/// Select the sun seen by the scattering volume. Interior geometry still
/// decides visibility in the inject shader; this only supplies outdoor
/// radiance without changing the interior's surface or composite lighting.
fn volumetric_sun(sky: &SkyParams, fog_far: f32) -> ([f32; 3], [f32; 4]) {
    if sky.is_exterior {
        let intensity = sky.sun_intensity.max(0.0);
        (
            sky.sun_direction,
            [
                sky.sun_color[0] * intensity,
                sky.sun_color[1] * intensity,
                sky.sun_color[2] * intensity,
                fog_far,
            ],
        )
    } else {
        (
            sky.portal_sun_direction,
            [
                sky.portal_sun_radiance[0],
                sky.portal_sun_radiance[1],
                sky.portal_sun_radiance[2],
                fog_far,
            ],
        )
    }
}

fn volumetric_open_sky_flag(sky: &SkyParams) -> f32 {
    if sky.is_exterior || sky.interior_show_sky {
        1.0
    } else {
        0.0
    }
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
        // #2484 — the first access scope must name the depth WRITE, not
        // only reads. The data being copied here *is* the render pass's
        // depth-attachment write, and a barrier whose source scope contains
        // only reads performs no availability operation for that write.
        //
        // This was almost certainly legal already, by dependency chaining:
        // `helpers.rs::create_render_pass`'s `dependency_out` declares
        // `dst_stage = FRAGMENT_SHADER | COMPUTE_SHADER` / `dst_access =
        // SHADER_READ`, and this barrier's first scope contains
        // `FRAGMENT_SHADER` + `SHADER_READ`, so the two scopes intersect and
        // the pass's `DEPTH_STENCIL_ATTACHMENT_WRITE` availability
        // propagates through. But that makes the correctness of a depth read
        // depend on an incidental overlap with a dependency declared for an
        // unrelated consumer (SSAO / SVGF / composite): narrowing
        // `dependency_out` — a plausible future optimisation — would
        // silently break this copy, and the symptom (stale soft-particle
        // depth fade) is invisible to `cargo test`.
        //
        // Naming the write here makes the barrier self-sufficient. This is a
        // strict widening of both source scopes — adding access flags can
        // only make more memory available and adding a stage can only pull
        // more prior work into the dependency — so it cannot invalidate a
        // guarantee that held before it.
        let depth_to_src = vk::ImageMemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::SHADER_READ,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.swapchain.depth_image)
            .subresource_range(range);
        let hist_to_dst = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.swapchain.depth_history_image)
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
            .image(self.swapchain.depth_image)
            .subresource_range(range);
        let hist_to_read = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.swapchain.depth_history_image)
            .subresource_range(range);

        // SAFETY: `cmd` is recording and outside any render pass (caller contract); `depth_image` / `depth_history_image` are live, same-extent D32_SFLOAT images. The barriers correctly bracket the READ_ONLY->TRANSFER_SRC / SHADER_READ->TRANSFER_DST transitions around the copy and restore both layouts; no other access to these images is recorded between the barriers.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                // EARLY_FRAGMENT_TESTS joins LATE (#2484): depth writes are
                // produced by both fragment-test stages, so naming only LATE
                // left the early-Z write out of the source synchronization
                // scope. Same strict-widening rationale as `depth_to_src`'s
                // access mask above.
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[depth_to_src, hist_to_dst],
            );
            self.device.cmd_copy_image(
                cmd,
                self.swapchain.depth_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.swapchain.depth_history_image,
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
    ///
    /// Deliberately infallible (`()`, not `Result<()>` — #2503 /
    /// D12-2026-08-07-01): every one of the eight `record_*_pass` helpers
    /// below returns `()`, and `record_upscale_pass` in particular *must*
    /// stay that way. It runs after `svgf.dispatch`/`taa.dispatch` have
    /// latched `dispatched_this_frame`, so an error escaping from here to
    /// `draw_frame` would skip `queue_submit` *and* `mark_frame_completed`,
    /// leaving those latches set for a dispatch that never reached the GPU
    /// — stale-history / ghosting on the next frame (#2146). Keeping this
    /// signature infallible turns any future `?` added inside one of the
    /// eight helpers into a compile error at the point of introduction,
    /// rather than a silent reopening of that hazard.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_post_passes(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        img: usize,
        // #2468 — camera parked AND the scene unchanged. The caustic
        // accumulator's EMA is the only consumer down here. SVGF consumes
        // the same flag, through `next_svgf_temporal_alpha`, at its param
        // upload in `build_and_upload_instances.rs` (#4046); TAA takes no
        // static flag at all.
        caustic_history_valid: bool,
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
        local_emitters_present: bool,
        fog_single_scatter_albedo: f32,
        fog_scale_height_meters: f32,
        fog_coverage: f32,
        fog_height_reference: f32,
        wind_params: [f32; 4],
        wind_gust: [f32; 4],
        fog_volumes: &[super::super::volumetrics::GpuFogVolume],
        fsr_frame: Option<FsrFrameParameters>,
        underwater: [f32; 4],
        image_space_modifier: ImageSpaceModifier,
        ui_instance_idx: Option<u32>,
    ) {
        self.record_svgf_pass(cmd, frame);
        self.record_caustic_splat_pass(cmd, frame, caustic_history_valid);
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
                local_emitters_present,
                fog_single_scatter_albedo,
                fog_scale_height_meters,
                fog_coverage,
                fog_height_reference,
                wind_params,
                wind_gust,
                fog_volumes,
            },
        );
        self.record_ssao_pass(cmd, frame, vp, inv_vp_arr, camera_pos, render_origin);
        self.record_composite_pass(cmd, frame);
        // #2796 / REN-D16-01 — bloom now runs AFTER composite, reading and
        // writing composite's own assembled scene (sky + GI + caustics +
        // direct) instead of the pre-composite raw HDR that never
        // contained sky/GI/caustics. See `record_bloom_pass`'s doc.
        self.record_bloom_pass(cmd, frame);
        // Stage 1 — meter the post-bloom scene this frame's FSR dispatch and
        // presentation pass will both consume, so the exposure texel and the
        // reconstruction agree by construction.
        self.record_exposure_meter_pass(cmd, frame);
        // #3572 — TAA now resolves the SAME fully-composited, post-bloom
        // scene image FSR consumes, so sky / denoised indirect /
        // volumetrics / caustics / bloom are inside the resolved image on
        // both jitter phases (the raw-HDR tap resolved direct lighting
        // only, and everything composite added was never seen by the
        // filter). Composite reads the raw HDR attachment directly; TAA's
        // output feeds the upscale/presentation tap below.
        self.record_taa_pass(cmd, frame);
        self.record_upscale_pass(cmd, frame, fsr_frame);
        self.record_presentation_pass(
            cmd,
            frame,
            img,
            underwater,
            image_space_modifier,
            ui_instance_idx,
        );
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
            if let Some(ref wca) = self.post.water_caustic_accum {
                wca.barrier_post_render_pass(&self.device, cmd, frame);
            }

            if !self.svgf_failed {
                // Captured before the &mut self.post.svgf borrow: the à-trous
                // pass reads DBG_DISABLE_ATROUS out of the same render-debug
                // bitmask the fragment shader sees (env-set; console legacy
                // toggle is light-atten-only and not relevant here).
                let svgf_dbg_flags = self.render_debug_flags;
                if let Some(ref mut svgf) = self.post.svgf {
                    // #674 temporal α state machine + UBO host write
                    // both ran BEFORE the bulk pre-render barrier
                    // above (#961 / REN-D10-NEW-04 fold). This call
                    // only records the SVGF compute dispatch.
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_svgf_start(&self.device, cmd, frame);
                    }
                    // Infallible (#3981) — the fallible half of the pair is
                    // `svgf.upload_params`, which `build_and_upload_instances`
                    // runs before this and which now latches `svgf_failed` on
                    // error. This used to be wrapped in an `if let Err(…)` arm
                    // that no producer could ever populate.
                    svgf.dispatch(&self.device, cmd, frame, svgf_dbg_flags);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_svgf_end(&self.device, cmd, frame);
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
        history_valid: bool,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and the live
        // caustic/TLAS resources are indexed by the current in-flight `frame`.
        unsafe {
            let Some(ref mut caustic) = self.post.caustic else {
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
                            caustic.dispatch(&self.device, cmd, frame, history_valid);
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
            // pattern every subsequent frame. See `skip_clear_decision`.
            let (should_clear, next_latch) =
                skip_clear_decision(ran, self.caustic_cleared_on_skip[frame]);
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
    /// Gated on this frame's TLAS having built (`ray_query_tlas`, #4779)
    /// plus the cluster and geometry inputs. When any is missing (RT
    /// unsupported, scene not yet built, build failed, accel_manager
    /// absent) we skip BOTH the descriptor write and the dispatch, and the
    /// shared `skip_clear_decision` latch (#3685) records a neutral clear of
    /// the slot on the first skipped frame — composite then samples an
    /// empty medium, never a prior cell's stale fog.
    ///
    /// Exterior sun direction + radiance come from `SkyParams::sun_direction`
    /// / `sun_color` / `sun_intensity` (#1022 / REN-D18-008). Interiors use
    /// the separate `portal_sun_*` fields so their surface and composite
    /// lighting remain cell-authored. Both lanes are dark below the horizon.
    ///
    /// The inject shader distinguishes "real window" from "geometry gap" via
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
            local_emitters_present,
            fog_single_scatter_albedo,
            fog_scale_height_meters,
            fog_coverage,
            fog_height_reference,
            wind_params,
            wind_gust,
            fog_volumes,
        } = inputs;
        // #4779 — never the stale AS a failed build leaves alive: `None` takes
        // the not-ready arm below. Resolved before `vol` borrows `self.post`.
        let vol_tlas = self.ray_query_tlas(frame);
        // SAFETY: `cmd` is recording outside a render pass, and all volumetric,
        // cluster, TLAS, and timer resources belong to this live context/frame.
        unsafe {
            if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                if let Some(ref mut vol) = self.post.volumetrics {
                    // In a clear interior the legacy CELL data commonly has
                    // no fog ramp at all.  A small neutral dust coefficient
                    // lets existing, authored local emitters scatter through
                    // the froxel grid; without a medium even a perfectly
                    // shadowed candle cannot produce a visible light shaft.
                    // It is below normal gameplay fog density, so it does
                    // not turn the room into haze or alter exterior weather.
                    const INTERIOR_DUST_EXTINCTION_PER_METER: f32 = 0.006;
                    let portal_sun_active = !sky_params.is_exterior
                        && sky_params
                            .portal_sun_radiance
                            .iter()
                            .any(|channel| *channel > 0.0);
                    let effective_extinction_per_meter = if !sky_params.is_exterior
                        && (local_emitters_present || portal_sun_active)
                    {
                        fog_extinction_per_meter.max(INTERIOR_DUST_EXTINCTION_PER_METER)
                    } else {
                        fog_extinction_per_meter.max(0.0)
                    };
                    let scatter_coef = effective_extinction_per_meter
                        / super::super::volumetrics::WORLD_UNITS_PER_METER;
                    // #3685 — `ran` feeds the shared skip-clear latch
                    // (`skip_clear_decision`, same shape as the caustic
                    // pass below): true only when a real dispatch executed
                    // this frame. Composite reads the integrated froxel
                    // volume unconditionally every frame, so the first skip
                    // of a streak still needs a neutral-frame clear, but
                    // every *subsequent* skip in the same streak would
                    // otherwise re-clear an already-zero volume for as long
                    // as the gate stays off.
                    let ran = if !vol.requires_dispatch(
                        volumetric_time_seconds,
                        scatter_coef > 0.0,
                        fog_volumes,
                    ) {
                        false
                    } else {
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
                        let vol_geometry = match (
                            self.mesh_registry.global_vertex_buffer.as_ref(),
                            self.mesh_registry.global_index_buffer.as_ref(),
                        ) {
                            (Some(vertex_buffer), Some(index_buffer)) => Some((
                                self.scene_buffers.instance_buffers()[frame].buffer,
                                self.scene_buffers.instance_buffer_size(frame),
                                vertex_buffer.buffer,
                                vertex_buffer.size,
                                index_buffer.buffer,
                                index_buffer.size,
                            )),
                            _ => None,
                        };
                        if let (
                            Some(tlas),
                            Some((light_buf, light_buf_size, grid_buf, index_buf)),
                            Some((
                                instance_buf,
                                instance_buf_size,
                                vertex_buf,
                                vertex_buf_size,
                                geometry_index_buf,
                                geometry_index_buf_size,
                            )),
                        ) = (vol_tlas, vol_lights, vol_geometry)
                        {
                            vol.write_tlas(&self.device, frame, tlas);
                            vol.write_boundary_geometry(
                                &self.device,
                                frame,
                                instance_buf,
                                instance_buf_size,
                                vertex_buf,
                                vertex_buf_size,
                                geometry_index_buf,
                                geometry_index_buf_size,
                            );
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
                            // trailing barrier in `draw_frame` only
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
                            let (sun_direction, sun_radiance) = volumetric_sun(sky_params, fog_far);
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
                                    sun_direction[0],
                                    sun_direction[1],
                                    sun_direction[2],
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
                                // froxel positions for the TLAS shadow rays.
                                // w permits open-sky misses only for exteriors
                                // and interiors with the authored Show Sky bit.
                                render_origin: [
                                    render_origin.x,
                                    render_origin.y,
                                    render_origin.z,
                                    volumetric_open_sky_flag(sky_params),
                                ],
                                medium_params: [
                                    fog_single_scatter_albedo.clamp(0.0, 1.0),
                                    super::super::volumetrics::DEFAULT_BACKWARD_PHASE_G,
                                    super::super::volumetrics::DEFAULT_DUAL_LOBE_MIX,
                                    // #3956 — the same authored altitude the
                                    // composite tail uses. The froxel grid and
                                    // that tail are two models of ONE medium:
                                    // `heightFogOpticalDepth`'s caller in
                                    // `composite.frag` deliberately continues
                                    // from the grid's own boundary radiance to
                                    // make them agree at the seam, so they must
                                    // not then disagree about how fog thins
                                    // with height.
                                    fog_scale_height_meters
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
                                fog_reference: [
                                    fog_height_reference,
                                    crate::vulkan::volumetrics::DEFAULT_EMISSIVE_HISTORY_WEIGHT,
                                    self.scene_buffers
                                        .current_ray_budget(
                                            self.renderer_config.rt_test_ray_quality_tier,
                                        )
                                        .volumetric_light_cap
                                        as f32,
                                    0.0,
                                ],
                                wind_params,
                                // Lane y carries the BYRO_BFECC kill-switch
                                // into the inject shader's error correction.
                                wind_gust: [
                                    wind_gust[0],
                                    super::super::volumetrics::
                                        bfecc_error_correction_multiplier(),
                                    wind_gust[2],
                                    wind_gust[3],
                                ],
                            };
                            if let Some(ref mut timers) = self.gpu_timers {
                                timers.cmd_volumetrics_start(&self.device, cmd, frame);
                            }
                            let vol_result =
                                vol.dispatch(&self.device, cmd, frame, &vol_params, fog_volumes);
                            if let Some(ref mut timers) = self.gpu_timers {
                                timers.cmd_volumetrics_end(&self.device, cmd, frame);
                            }
                            match vol_result {
                                Ok(()) => true,
                                Err(e) => {
                                    log::warn!("Volumetrics dispatch failed: {e}");
                                    // Pre-fix behavior never cleared on a
                                    // failed dispatch either (composite
                                    // falls back to stale prior-frame
                                    // content on this exact frame) —
                                    // unchanged here. Reporting this as
                                    // "ran" resets the latch so the *next*
                                    // genuine skip streak still clears its
                                    // first frame, rather than trusting a
                                    // latch left over from before this
                                    // attempt.
                                    true
                                }
                            }
                        } else {
                            // Never let a prior cell's integrated fog hang over a
                            // frame whose TLAS/cluster inputs are not ready yet.
                            false
                        }
                    };
                    // #2507-shaped latch, shared with the caustic pass —
                    // see `skip_clear_decision`.
                    let (should_clear, next_latch) =
                        skip_clear_decision(ran, self.volumetrics_cleared_on_skip[frame]);
                    self.volumetrics_cleared_on_skip[frame] = next_latch;
                    if should_clear {
                        vol.record_neutral_frame(&self.device, cmd, frame);
                    }
                }
            }
        }
    }

    /// TAA resolve (#2258 / TD1-080, extracted from `record_post_passes`):
    /// reprojects previous frame's history via motion vectors,
    /// neighborhood-clamps in YCoCg, and writes the anti-aliased HDR
    /// result for the upscale/presentation tap to sample. #3572 moved the
    /// dispatch to run after composite and bloom, resolving the SAME
    /// fully-composited, post-bloom scene image `record_upscale_pass`
    /// feeds FSR (`composite.scene_view`, wired in at pipeline
    /// construction) instead of the raw direct-only main-pass attachment;
    /// composite reads the raw HDR attachment directly and never samples
    /// this pass's output.
    ///
    /// TAA permanent-failure policy: `latch_taa_failure` (below) is the
    /// one reachable failure path, and no descriptor rebind exists or is
    /// needed — composite's binding 0 names the raw HDR attachment
    /// unconditionally, so a latched failure simply leaves the frame tail
    /// blitting the composite scene through un-resolved. See #479 /
    /// #3572.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_taa_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // Raw-output policy (W3.16): correctness views bypass temporal
        // reconstruction entirely. Pre-move TAA ran unconditionally and its
        // output only ever reached composite's direct term; post-move it
        // filters the FINAL image, so letting it run under a raw debug view
        // would temporally smooth the very categorical values the oracle
        // gates assert on. Same gate shape as record_bloom_pass's.
        if crate::shader_constants::render_debug_requires_raw_output(
            self.render_debug_flags,
            self.render_debug_mode.shader_value(),
        ) {
            return;
        }
        // SAFETY: `cmd` is recording outside a render pass, and the TAA,
        // composite, and timer resources are live for the current `frame`.
        unsafe {
            if !self.taa_failed {
                if let Some(ref mut taa) = self.post.taa {
                    // #1194 — bracket the TAA compute dispatch.
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_taa_start(&self.device, cmd, frame);
                    }
                    // Infallible (#3981) — see `latch_taa_failure` for where
                    // the reachable TAA failure is handled.
                    taa.dispatch(&self.device, cmd, frame);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_taa_end(&self.device, cmd, frame);
                    }
                }
            }
        }
    }

    /// The TAA permanent-failure path — every action a failure must take,
    /// in one place, so they cannot drift apart.
    ///
    /// Called from `build_and_upload_instances` when `taa.upload_params`
    /// fails. That is the *only* reachable TAA failure: `upload_params`
    /// writes the host-visible param UBO through `write_mapped` (the same
    /// fallible mapped-slice class #2504 hardened for `upload_indirect_draws`),
    /// while `TaaPipeline::dispatch` only records `ash` commands and cannot
    /// fail at all. Before #3981 the reverse was assumed: the three actions
    /// below lived in a `dispatch`-error arm no producer could populate, and
    /// the upload failure — the one that actually happens — was a bare
    /// `warn!` with no latch, leaving the dispatch to resolve a jittered
    /// frame against whatever the UBO held from a previous frame, or nothing
    /// at all on a slot's first use.
    ///
    /// Ordering note: `build_and_upload_instances` runs before
    /// `record_post_passes`, so `record_taa_pass`'s `!self.taa_failed` gate
    /// picks this up in the SAME frame and skips the resolve entirely; the
    /// `#1932` un-jitter gate in `assemble_camera_and_lights` picks it up on
    /// the next.
    pub(super) fn latch_taa_failure(&mut self, error: &anyhow::Error) {
        log::error!(
            "TAA parameter upload failed — the frame tail blits the composite \
             scene through un-resolved for the rest of the session: {error}"
        );
        self.taa_failed = true;
        // #3572 — composite samples the raw HDR attachment directly (no
        // TAA-output rebind exists to undo), so the #4006 deferred-rebind
        // latch this arm used to schedule is retired with the composite-side
        // tap. #3605 (REN-2026-08-30-D13-02) — this frame's geometry pass already
        // rendered with the Halton jitter offset (chosen at the top of
        // `draw_frame`, before the upload failed), and the frame tail blits
        // that image through with nothing to resolve it. Mirrors
        // the FSR sibling at #2519: flush temporal history so the NEXT frame
        // does not reproject against a half-pixel-shifted image — later
        // frames are chosen unjittered by the `!taa_failed` gate (#1932), so
        // one frame covers it.
        self.signal_temporal_discontinuity(
            super::super::frame_upscaler::TAA_DISPATCH_FAILURE_RECOVERY_FRAMES,
        );
    }

    /// The exposure meter's permanent-failure path, mirroring
    /// `latch_taa_failure`'s discipline (#3981 / #2146): `upload_params`'s
    /// mapped write is the pass's one reachable failure, and everything the
    /// failure must do lives here so it cannot drift apart.
    ///
    /// Milder than the TAA sibling: presentation and FSR keep sampling the
    /// per-frame exposure slots (cleared to `DEFAULT_EXPOSURE` at init), so
    /// the frame stays correctly graded — exposure simply freezes at the
    /// slots' last value instead of tracking the scene or the fixed setting.
    /// No history flush is needed; the meter writes no temporal resource any
    /// other pass reads.
    pub(super) fn latch_exposure_meter_failure(&mut self, error: &anyhow::Error) {
        log::error!(
            "Exposure-meter parameter upload failed — exposure is frozen at the \
             last written value for the rest of the session: {error}"
        );
        self.exposure_meter_failed = true;
    }

    /// SSAO compute pass (#2258 / TD1-080, extracted from
    /// `record_post_passes`): reads depth buffer (now in READ_ONLY layout
    /// after render pass), writes this frame's slot of the per-FIF AO
    /// image array. `composite.frag` has no AO binding at all — the sole
    /// reader is `triangle.frag`'s main render pass, which for a given
    /// frame runs *before* this SSAO dispatch in command order. With
    /// `MAX_FRAMES_IN_FLIGHT == 2`, that main-pass read samples the AO
    /// slot this same pass wrote two frames ago, not the current frame
    /// and not the immediately-prior one — see the AO-sample site in
    /// `triangle.frag`.
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
            if let Some(ref mut ssao) = self.post.ssao {
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

    /// Exposure metering (Stage 1, RENDERING-PLAN.md): reads the SAME
    /// post-bloom composite scene image TAA and the upscaler consume,
    /// reduces it to a geometric-mean luminance, and writes this frame's
    /// exposure texel — the one both the FSR dispatch (compute, this
    /// command buffer, later) and `presentation.frag` (`exposureTex`)
    /// sample. Runs between `record_bloom_pass` and `record_taa_pass`;
    /// bloom's outgoing `apply_to_scene` barrier already covers this
    /// dispatch's compute read of the scene image, and the meter's own
    /// post-write barrier publishes the texel to FSR + presentation.
    ///
    /// Fixed mode still dispatches (the shader writes the constant without
    /// sampling), which keeps the slots authoritative for every consumer
    /// instead of splitting fixed/auto across two code paths.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from.
    fn record_exposure_meter_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        if self.exposure_meter_failed {
            return;
        }
        // #4591 — the raw-debug gate every later post pass carries. In auto
        // mode the meter would otherwise meter the debug image (false
        // colour, raw AO, …) and adapt the persistent per-slot exposure
        // toward it; presentation bypasses exposure for raw views, so the
        // damage shows as a pop + re-adaptation from the wrong starting
        // value when the view is dismissed. Fixed mode (the default) just
        // re-writes its constant, so the gate is only load-bearing in auto
        // mode — but skipping both keeps the slot untouched by debug
        // frames, matching the siblings' unconditional shape.
        if crate::shader_constants::render_debug_requires_raw_output(
            self.render_debug_flags,
            self.render_debug_mode.shader_value(),
        ) {
            return;
        }
        // SAFETY: `cmd` is recording outside a render pass; the meter
        // pipeline, this frame's exposure slot, and the composite scene view
        // are live for this frame.
        unsafe {
            let (Some(ref mut meter), Some(composite)) =
                (self.post.exposure_meter.as_mut(), self.post.composite.as_ref())
            else {
                return;
            };
            let exposure = &self.post.exposure;
            // #4618 — the bracket wraps the dispatch and the two stage-wide
            // barriers `dispatch` records around it. It sits inside the two
            // skip paths above (`exposure_meter_failed`, the raw-debug gate)
            // and the missing-resource `else`, so a frame that never ran the
            // meter reads inactive rather than ~0 ms.
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_exposure_meter_start(&self.device, cmd, frame);
            }
            meter.dispatch(
                &self.device,
                cmd,
                frame,
                composite.scene_view(frame),
                exposure.image(frame),
                exposure.view(frame),
            );
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_exposure_meter_end(&self.device, cmd, frame);
            }
        }
    }

    /// Bloom pyramid (M58) (#2258 / TD1-080, extracted from
    /// `record_post_passes`; moved to run AFTER composite under #2796 /
    /// REN-D16-01). Reads `composite.scene_view(frame)` — the
    /// FULLY ASSEMBLED scene composite.frag just wrote (sky + demodulated
    /// GI + caustics + direct + fog) — builds the down/up mip pyramid
    /// from it, then [`bloom::BloomPipeline::apply_to_scene`] adds the
    /// result back onto that same image in place, before FSR/native
    /// upscale samples it.
    ///
    /// This is a SEPARATE image from composite's own `hdrTex` input
    /// (binding 0): composite reads the raw main-pass HDR attachment
    /// there unconditionally — no TAA-output swap has existed since #3572
    /// retired the rebind mechanism. Bloom reads composite's OUTPUT
    /// (`scene_image_views`), the same post-bloom image TAA resolves and
    /// the upscale pass consumes.
    ///
    /// Pre-#2796 this read the pre-composite raw HDR attachment (whatever
    /// the main render pass alone had written), which structurally never
    /// contained sky, GI, or caustics — those only ever existed inside
    /// composite.frag's own `combined` accumulator. That meant the sun
    /// disc and bright sky could never bloom (despite #2233's rationale
    /// for making the add unconditional), and for exteriors the untouched
    /// sky texels bloom's pyramid saw were the raw HDR clear colour
    /// (`byroredux_core::types::Color::CORNFLOWER_BLUE` pre-#2466,
    /// transparent black since) rather than real sky radiance.
    ///
    /// The scene image must be `SHADER_READ_ONLY_OPTIMAL` on entry
    /// (composite's render pass `final_layout` contract) — `dispatch`
    /// only samples it (no layout change); `apply_to_scene` owns the
    /// round-trip through `GENERAL` and restores `SHADER_READ_ONLY_
    /// OPTIMAL` before returning, satisfying `frame_upscaler.rs`'s entry
    /// contract for `record_upscale_pass` right after this.
    ///
    /// The `if let Some(...)` guard below is dead at runtime (#1276):
    /// `VulkanContext::new`'s bloom-init arm hard-fails with
    /// `anyhow::anyhow!(...)` if bloom init returns `None` (policy from
    /// #1081 — no fallback binding for bloomTex when bloom is absent), so
    /// the engine never reaches `draw_frame` with `self.post.bloom == None`.
    /// The `Option` wrapper is kept because the resize-recreate path
    /// benefits from it as a temporary, but the runtime `None` branch is
    /// unreachable.
    ///
    /// # Safety
    /// `cmd` is in the recording state — opened by `begin_command_buffer`
    /// in `draw_frame` and not yet closed — and this runs once per frame
    /// between the main render pass end and `end_command_buffer`, at the
    /// fixed position `record_post_passes` calls it from, AFTER
    /// `record_composite_pass` and BEFORE `record_upscale_pass`.
    fn record_bloom_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
        // Correctness views are already final in the composite scene image.
        // Bloom is applied in-place *after* composite, so the shader's own
        // raw-output early return cannot protect them here. Without this
        // gate a flat categorical value such as 0.10 was raised to ~0.175,
        // invalidating the Cornell material oracle even though composite and
        // presentation both selected their raw paths.
        if crate::shader_constants::render_debug_requires_raw_output(
            self.render_debug_flags,
            self.render_debug_mode.shader_value(),
        ) {
            return;
        }

        // SAFETY: `cmd` is recording outside a render pass, and bloom's input
        // view, pipeline resources, and timers are live for the current frame.
        unsafe {
            if let Some(ref mut bloom) = self.post.bloom {
                if let Some(ref composite) = self.post.composite {
                    let scene_image = composite.scene_image(frame);
                    let scene_view = composite.scene_view(frame);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_bloom_start(&self.device, cmd, frame);
                    }
                    // Infallible (#3981) — bloom's params are uploaded once
                    // at construction (#2037), so unlike TAA/SVGF there is no
                    // per-frame fallible step here at all.
                    bloom.dispatch(&self.device, cmd, frame, scene_view);
                    bloom.apply_to_scene(&self.device, cmd, frame, scene_image, scene_view);
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_bloom_end(&self.device, cmd, frame);
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
            if let Some(ref composite) = self.post.composite {
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
            // #3572 — in TAA mode the blit source is TAA's resolved output
            // (history slot, GENERAL layout), not the scene image; FSR mode
            // and the raw/native-blit fallback keep the scene image. The
            // raw-output gate mirrors record_taa_pass: under a raw view the
            // resolve never ran, so its output must not be consumed.
            let taa_resolved = !self.taa_failed
                && self.post.taa.is_some()
                && !crate::shader_constants::render_debug_requires_raw_output(
                    self.render_debug_flags,
                    self.render_debug_mode.shader_value(),
                );
            let (scene_color, source_layout) = if taa_resolved {
                (
                    self.post
                        .taa
                        .as_ref()
                        .expect("taa exists per taa_resolved")
                        .output_image(frame),
                    vk::ImageLayout::GENERAL,
                )
            } else {
                (
                    self.post
                        .composite
                        .as_ref()
                        .expect("composite must exist while recording")
                        .scene_image(frame),
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                )
            };
            let (motion_vectors, reactive_mask, transparency_mask) = {
                let gbuffer = self
                    .post
                    .gbuffer
                    .as_ref()
                    .expect("G-buffer must exist while recording");
                (
                    gbuffer.motion_image(frame),
                    gbuffer.reactive_image(frame),
                    gbuffer.transparency_image(frame),
                )
            };
            let exposure_image = Some(self.post.exposure.image(frame));
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
            let force_native_debug = crate::shader_constants::render_debug_requires_raw_output(
                self.render_debug_flags,
                self.render_debug_mode.shader_value(),
            );
            self.post
                .frame_upscaler
                .as_mut()
                .expect("frame upscaler must exist while recording")
                .record(
                    &self.device,
                    cmd,
                    frame,
                    UpscaleDispatchInputs {
                        scene_color,
                        scene_color_layout: source_layout,
                        depth: self.swapchain.depth_image,
                        depth_format: self.swapchain.depth_format,
                        motion_vectors,
                        exposure: exposure_image,
                        reactive: reactive_mask,
                        transparency: transparency_mask,
                    },
                    fsr_frame,
                    force_native_debug,
                );
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_upscale_end(&self.device, cmd, frame);
            }
            // #2519 — a dispatch rejected *inside* `record` fails after this
            // frame's geometry pass already rendered with the FSR jitter
            // offset (chosen at the top of `draw_frame`, when dispatch was
            // still active), and the recovery path blits that image through
            // with nothing to resolve it. Flush temporal history so the next
            // frame does not reproject against a half-pixel-shifted image;
            // later frames are chosen unjittered, so one frame covers it.
            let fsr_dispatch_failed_this_frame = self
                .post
                .frame_upscaler
                .as_mut()
                .expect("frame upscaler must exist while recording")
                .take_new_dispatch_failure();
            if fsr_dispatch_failed_this_frame {
                self.signal_temporal_discontinuity(
                    super::super::frame_upscaler::FSR_DISPATCH_FAILURE_RECOVERY_FRAMES,
                );
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
        image_space_modifier: ImageSpaceModifier,
        ui_instance_idx: Option<u32>,
    ) {
        // SAFETY: `cmd` is recording outside a render pass, and presentation,
        // exposure, swapchain-image, and timer resources are live for this frame.
        unsafe {
            // #3426 — the Scaleform overlay composites inside the
            // presentation pass now, so its draw state is assembled here and
            // handed to `dispatch`. `None` (no UI texture this frame, no
            // registered quad, or a quad with no per-mesh buffers — the
            // #2505 global-only case) simply skips the overlay draw.
            // Assembled with combinators rather than `?` on purpose: every
            // line from the SVGF latch to `queue_submit` must stay
            // error-propagation-free, and
            // `record_post_passes_has_no_error_propagation_after_the_svgf_latch`
            // enforces that by scanning this file's text (#2146 / #917).
            let overlay = ui_instance_idx.zip(self.overlay.ui_quad_handle).and_then(
                |(instance_index, ui_quad)| {
                    self.mesh_registry.get(ui_quad).and_then(|mesh| {
                        mesh.vertex_buffer
                            .as_ref()
                            .zip(mesh.index_buffer.as_ref())
                            .map(|(vb, ib)| UiOverlayDraw {
                                texture_set: self.texture_registry.descriptor_set(frame),
                                scene_set: self.scene_buffers.descriptor_set(frame),
                                vertex_buffer: vb.buffer,
                                index_buffer: ib.buffer,
                                index_count: mesh.index_count,
                                instance_index,
                            })
                    })
                },
            );
            if overlay.is_none() && ui_instance_idx.is_some() {
                // Mirrors the warn-once the geometry pass carried for this
                // case before the overlay moved (#2505 / D12-2026-08-07-03).
                //
                // #3594 — this condition is reachable via two distinct
                // causes: `self.mesh_registry.get(ui_quad)` missing the
                // handle entirely, or the mesh being registered but
                // global-only (no per-mesh vertex/index buffer). The
                // message used to name only the second; widened to cover
                // both rather than mis-attributing the first.
                // `ui_quad_handle == None` is NOT a live third cause here —
                // `draw.rs` gates `ui_instance_idx` on it, so `zip` never
                // short-circuits on that half.
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    log::warn!(
                        "UI overlay quad is unavailable (not in the mesh \
                         registry, or global-only with no per-mesh vertex/ \
                         index buffer) — skipping UI draw. #2505"
                    );
                });
            }
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_presentation_start(&self.device, cmd, frame);
            }
            self.post
                .presentation
                .as_ref()
                .expect("presentation pipeline must exist while recording")
                .dispatch(
                    &self.device,
                    cmd,
                    frame,
                    img,
                    PresentationFrame {
                        underwater,
                        image_space: image_space_modifier,
                        render_debug_flags: self.render_debug_flags,
                        render_debug_mode: self.render_debug_mode.shader_value(),
                        // Stage 1 — display-transform selection; the exposure
                        // itself arrives via `exposureTex` (binding 2), the
                        // same texel the FSR dispatch normalized against.
                        tonemap_op: self.tonemap.shader_value(),
                    },
                    overlay,
                );
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_presentation_end(&self.device, cmd, frame);
            }
        }
    }
}

/// Pure decision for a "dispatch every frame, but the fallback output is
/// unconditionally sampled downstream" pass's post-dispatch latch update —
/// whether this frame's accumulator slot needs a skip-clear, and the
/// latch's value going into the next frame. Split out so the state machine
/// is unit-testable without a live Vulkan device (#2507), matching the
/// `acceleration::predicates` convention. Shared by
/// [`VulkanContext::record_caustic_splat_pass`] (`caustic_cleared_on_skip`)
/// and [`VulkanContext::record_volumetrics_pass`]
/// (`volumetrics_cleared_on_skip`, #3685) — same shape, same latch
/// semantics, only the accumulator being cleared differs.
///
/// - `ran`: did a real dispatch execute (and succeed) this frame?
/// - `already_cleared`: was this frame-slot's latch already set from a
///   prior skip earlier in the same streak?
///
/// Returns `(should_clear, next_latch)`.
fn skip_clear_decision(ran: bool, already_cleared: bool) -> (bool, bool) {
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
    use super::{skip_clear_decision, volumetric_open_sky_flag, volumetric_sun, SkyParams};

    #[test]
    fn volumetric_sun_uses_portal_lane_only_inside() {
        let interior = SkyParams {
            sun_direction: [0.0, -1.0, 0.0],
            sun_color: [10.0, 10.0, 10.0],
            sun_intensity: 5.0,
            portal_sun_direction: [0.6, 0.8, 0.0],
            portal_sun_radiance: [2.0, 1.0, 0.5],
            ..SkyParams::default()
        };
        assert_eq!(
            volumetric_sun(&interior, 4096.0),
            ([0.6, 0.8, 0.0], [2.0, 1.0, 0.5, 4096.0])
        );

        let exterior = SkyParams {
            is_exterior: true,
            ..interior
        };
        assert_eq!(
            volumetric_sun(&exterior, 4096.0),
            ([0.0, -1.0, 0.0], [50.0, 50.0, 50.0, 4096.0])
        );
    }

    #[test]
    fn only_authored_open_sky_interiors_allow_unoccluded_sun_rays() {
        let sealed = SkyParams::default();
        assert_eq!(volumetric_open_sky_flag(&sealed), 0.0);

        let show_sky = SkyParams {
            interior_show_sky: true,
            ..SkyParams::default()
        };
        assert_eq!(volumetric_open_sky_flag(&show_sky), 1.0);

        let exterior = SkyParams {
            is_exterior: true,
            ..SkyParams::default()
        };
        assert_eq!(volumetric_open_sky_flag(&exterior), 1.0);
    }

    /// Regression: #4773 / REN-D8-2026-09-23-01 — `draw_frame` passed
    /// `fog_coverage, fog_scale_height_meters` into parameters declared
    /// `fog_scale_height_meters, fog_coverage`. Both are `f32`, so it
    /// compiled, and the froxel medium collapsed to a ~1 m ground layer.
    ///
    /// `record_post_passes` takes ~30 positional arguments, most of them
    /// same-typed scalars, and the call site names nearly every local after
    /// its parameter. Pin that convention: every argument spelled as a
    /// parameter's name must sit in that parameter's position.
    #[test]
    fn record_post_passes_named_arguments_match_parameter_positions() {
        fn ident(token: &str) -> &str {
            let token = token.trim().trim_end_matches(',').trim_start_matches('&');
            token.strip_prefix("self.").unwrap_or(token)
        }

        let post = include_str!("post_passes.rs");
        let sig_at = post
            .find("fn record_post_passes(")
            .expect("record_post_passes must still exist under this name");
        let sig = &post[sig_at..];
        let sig = &sig[..sig.find(") {").expect("signature end")];
        let params: Vec<&str> = sig
            .lines()
            .skip(1)
            .filter_map(|line| line.trim().split_once(':').map(|(name, _)| name.trim()))
            .filter(|name| *name != "&mut self" && *name != "&self")
            .collect();

        let draw = include_str!("draw.rs");
        let call_at = draw
            .find("self.record_post_passes(")
            .expect("draw_frame must still call record_post_passes");
        let call = &draw[call_at..];
        let call = &call[..call.find(");").expect("call end")];
        let args: Vec<&str> = call
            .lines()
            .skip(1)
            .map(ident)
            .filter(|arg| !arg.is_empty())
            .collect();

        assert_eq!(
            args.len(),
            params.len(),
            "argument/parameter count drifted: args {args:?} params {params:?}"
        );
        let mut pinned = 0;
        for (position, arg) in args.iter().enumerate() {
            if params.contains(arg) {
                assert_eq!(
                    *arg, params[position],
                    "argument `{arg}` is passed in the `{}` slot (position {position})",
                    params[position]
                );
                pinned += 1;
            }
        }
        assert!(
            pinned >= 20,
            "only {pinned} arguments share a parameter name; pin is vacuous"
        );
        let fog_scale = params.iter().position(|p| *p == "fog_scale_height_meters");
        let fog_coverage = params.iter().position(|p| *p == "fog_coverage");
        assert!(
            fog_scale.is_some() && fog_coverage.is_some(),
            "fog params renamed"
        );
    }

    /// A real dispatch running must never trigger a clear, and must reset
    /// the latch so a later skip streak clears fresh.
    #[test]
    fn dispatch_ran_never_clears_and_resets_latch() {
        assert_eq!(skip_clear_decision(true, false), (false, false));
        assert_eq!(
            skip_clear_decision(true, true),
            (false, false),
            "a real dispatch must reset a stale latch from a prior streak"
        );
    }

    /// The first skip of a new streak (latch not yet set) must clear.
    #[test]
    fn first_skip_of_a_streak_clears() {
        assert_eq!(skip_clear_decision(false, false), (true, true));
    }

    /// A subsequent skip in the same streak (latch already set) must NOT
    /// clear again — #2507's fix must not degrade into clearing every
    /// frame while genuinely skipped.
    #[test]
    fn subsequent_skip_in_same_streak_does_not_reclear() {
        assert_eq!(skip_clear_decision(false, true), (false, true));
    }

    /// #3685 — `record_volumetrics_pass` needs a live `VulkanContext`, so
    /// (matching this crate's established convention for that situation,
    /// e.g. `build_and_upload_instances.rs`'s `batches_scratch_reserve_
    /// tests`) pin the wiring with a static source-scan instead of a live
    /// test: the function must route both former unconditional
    /// `record_neutral_frame` call sites through the shared
    /// `skip_clear_decision` latch rather than clearing every frame the
    /// gate stays off.
    ///
    /// Scoped to the source before this file's own `#[cfg(test)]` module
    /// (this module doesn't reference `record_volumetrics_pass` by name,
    /// so a whole-file search would be safe here too, but scoping matches
    /// the convention #3674/#3675 established after finding an unscoped
    /// search can self-match its own needle literal).
    #[test]
    fn record_volumetrics_pass_routes_skip_clears_through_the_shared_latch() {
        let full_src = include_str!("post_passes.rs");
        let test_mod_start = full_src
            .find("#[cfg(test)]")
            .expect("this file has at least one #[cfg(test)] module");
        let src = &full_src[..test_mod_start];

        let fn_start = src
            .find("fn record_volumetrics_pass(")
            .expect("record_volumetrics_pass must still exist");
        // #3572 — record_taa_pass moved below bloom; the next fn after
        // volumetrics is now record_taa_pass's successor in source order.
        let fn_end = src[fn_start..]
            .find("\n    fn record_taa_pass(")
            .map(|rel| fn_start + rel)
            .expect("record_taa_pass must still exist after record_volumetrics_pass in source order");
        let body = &src[fn_start..fn_end];

        assert!(
            body.contains("skip_clear_decision(ran, self.volumetrics_cleared_on_skip[frame])"),
            "record_volumetrics_pass must route its skip decision through \
             the shared latch — see #3685"
        );
        assert_eq!(
            body.matches("vol.record_neutral_frame(").count(),
            1,
            "record_neutral_frame must be called from exactly one place — \
             the latch-gated `if should_clear` — not unconditionally at \
             each skip site"
        );
    }

    /// #3605 (REN-2026-08-30-D13-02) — a TAA failure must signal a temporal
    /// discontinuity the same way the FSR sibling does at #2519: the failing
    /// frame's geometry pass already rendered with the Halton jitter offset
    /// before the failure is discovered, and without this call
    /// SVGF/volumetrics would reproject the NEXT frame against that
    /// jittered-but-unresolved image with no history flush to protect it.
    /// The path needs a live `VulkanContext`, so — matching this file's own
    /// `record_volumetrics_pass_routes_skip_clears_through_the_shared_latch`
    /// convention just above — pin the wiring with a static source-scan
    /// instead of a live test.
    ///
    /// #3981 — this used to scope on `record_taa_pass`, which was where the
    /// three failure actions lived, on a `dispatch`-error arm that no
    /// producer could populate. The scan passed on unreachable text, which is
    /// exactly the false confidence that let the finding survive. It now
    /// scopes on `latch_taa_failure` **and** asserts a live caller, so
    /// "the code exists" and "the code runs" are both checked.
    #[test]
    fn a_taa_failure_signals_a_temporal_discontinuity() {
        let full_src = include_str!("post_passes.rs");
        let test_mod_start = full_src
            .find("#[cfg(test)]")
            .expect("this file has at least one #[cfg(test)] module");
        let src = &full_src[..test_mod_start];
        let body = taa_failure_body(src);

        assert!(
            body.contains("self.taa_failed = true;"),
            "latch_taa_failure must still latch taa_failed — the needle this \
             test scopes its check around has moved or been renamed"
        );
        assert!(
            body.contains("self.signal_temporal_discontinuity(")
                && body.contains("TAA_DISPATCH_FAILURE_RECOVERY_FRAMES"),
            "a TAA failure must call signal_temporal_discontinuity, mirroring the \
             FSR dispatch-failure path (#2519) — its own doc names this exact hazard on the \
             TAA side but nothing closed it before #3605"
        );

        // The half #3981 is actually about: reachability. A latch nothing
        // calls is the state this whole finding described.
        let caller = include_str!("build_and_upload_instances.rs");
        assert!(
            caller.contains("self.latch_taa_failure(&e);"),
            "build_and_upload_instances must route the taa.upload_params error \
             into latch_taa_failure — it is the only reachable TAA failure, and \
             before #3981 it was a bare warn! that latched nothing while the \
             dispatch went on to resolve a jittered frame against stale params"
        );
    }

    /// #3981 SIBLING — SVGF has the same shape and took the same fix.
    /// `SvgfPipeline::dispatch` is infallible, so the latch hangs off
    /// `upload_params`; without it a failed param write left the denoiser
    /// running against a previous frame's alpha (or an uninitialised slot on
    /// first use) with `svgf_failed` reading false.
    #[test]
    fn an_svgf_parameter_upload_failure_latches_the_pass() {
        let caller = include_str!("build_and_upload_instances.rs");
        let start = caller
            .find("svgf.upload_params(")
            .expect("build_and_upload_instances must still upload SVGF params");
        let arm = &caller[start..start + 1400.min(caller.len() - start)];
        assert!(
            arm.contains("self.svgf_failed = true;"),
            "the svgf.upload_params error arm must latch svgf_failed — a bare \
             warn! leaves record_svgf_pass denoising against stale parameters \
             (#3981)"
        );
    }

    /// #3981 — the shape that caused the finding, pinned across every pass.
    ///
    /// `TaaPipeline::dispatch`, `SvgfPipeline::dispatch` and
    /// `BloomPipeline::dispatch` each advertised `-> Result<()>` while their
    /// bodies contained no `?`, no `return Err`, no `bail!`, no `.context(`,
    /// no `map_err` — every statement an infallible `ash` recording call —
    /// and ended in an unconditional `Ok(())`. Their callers' `Err` arms were
    /// therefore dead, and three successive audits read the recovery those
    /// arms contained as live.
    ///
    /// This does not forbid a fallible dispatch. It requires that one which
    /// *says* it can fail actually can, which is the property that makes an
    /// error arm worth reading. It walks `vulkan/*.rs` rather than naming the
    /// three known files, because a hard-coded list would have to be extended
    /// by whoever adds the next pipeline — the same "someone remembered"
    /// failure the mirror guards at #3564 removed.
    #[test]
    fn a_pass_dispatch_advertising_a_result_must_be_able_to_produce_one() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/vulkan");
        // Composed at runtime, not written as literals: this file is itself
        // scanned by `svgf.rs`'s
        // `record_post_passes_has_no_error_propagation_after_the_svgf_latch`,
        // and a bare `"?;"` here would read as error propagation in the
        // production code that scanner is actually about.
        let error_forms = [
            format!("{}{}", "?", ";"),
            format!("return {}", "Err"),
            format!("{}!", "bail"),
            format!(".{}(", "context"),
            format!("map_{}", "err"),
        ];
        let mut checked = 0usize;
        for entry in std::fs::read_dir(&dir).expect("src/vulkan must be readable") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("utf-8 file name")
                .to_string();
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(sig_start) = src.find("pub unsafe fn dispatch(") else {
                continue;
            };
            let body_start = sig_start
                + src[sig_start..]
                    .find('{')
                    .expect("a dispatch signature is followed by its body");
            if !src[sig_start..body_start].contains("-> Result") {
                checked += 1;
                continue;
            }
            let mut depth = 0usize;
            let mut end = body_start;
            for (i, c) in src[body_start..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = body_start + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &src[body_start..end];
            assert!(
                error_forms.iter().any(|n| body.contains(n.as_str())),
                "{name}'s dispatch is declared `-> Result<()>` but its body cannot \
                 produce an error, so every caller's `Err` arm is dead code that \
                 reads as a live recovery tier — the #3981 trap. Either drop the \
                 Result or make the failure real."
            );
            checked += 1;
        }
        assert!(
            checked >= 5,
            "only {checked} pass dispatches found under src/vulkan — the walk \
             stopped finding them, so this guard is no longer covering anything"
        );
    }

    /// The body of the one function that owns the TAA failure policy.
    /// Shared by the scanners below so a rename lands as one failure rather
    /// than three.
    fn taa_failure_body(src: &str) -> &str {
        let start = src
            .find("fn latch_taa_failure(")
            .expect("latch_taa_failure must still exist (#3981)");
        let end = src[start..]
            .find("\n    /// SSAO compute pass")
            .map(|rel| start + rel)
            .expect("record_ssao_pass's doc must still follow latch_taa_failure");
        &src[start..end]
    }

    /// #3572 (W3.16) — the #4006 deferred-rebind mechanism is RETIRED.
    /// Composite samples the raw HDR attachment directly, so there is no
    /// TAA-output descriptor for a TAA failure to hand back; the latch
    /// field, its sync-side consumer, and `fall_back_to_raw_hdr` are gone.
    /// This scan holds the retirement: the failure path must not grow a
    /// descriptor rewrite back, and the retired symbols must not return.
    #[test]
    fn the_retired_taa_failure_descriptor_rebind_stays_retired() {
        let full_src = include_str!("post_passes.rs");
        let test_mod_start = full_src
            .find("#[cfg(test)]")
            .expect("this file has at least one #[cfg(test)] module");
        let src = &full_src[..test_mod_start];
        let body = taa_failure_body(src);

        assert!(
            !body.contains("fall_back_to_raw_hdr")
                && !body.contains("rebind_hdr_views")
                && !body.contains("update_descriptor_sets"),
            "the TAA failure path runs during frame build — it must never \
             rewrite any pipeline's descriptor sets there (VUID-\
             vkUpdateDescriptorSets-None-03047); the raw-HDR fallback the \
             #4006 latch scheduled no longer exists because composite \
             samples the raw HDR attachment directly (#3572)"
        );
        assert!(
            !src.contains("composite_needs_raw_hdr_rebind"),
            "the #4006 latch field is retired (#3572) — composite has no \
             TAA-output binding to fall back from; do not resurrect the \
             symbol without reinstating a composite-side TAA tap"
        );
        let composite_src = include_str!("../composite.rs");
        assert!(
            !composite_src.contains("fall_back_to_raw_hdr"),
            "fall_back_to_raw_hdr is retired with the composite-side TAA tap \
             (#3572); its only caller was the retired #4006 consumer"
        );
    }

    /// The comment-free text of one `impl` method: `signature` up to the
    /// method's own closing brace (the first line that is exactly `    }`).
    /// Bounding the slice is the point (#4841) — `&src[start..]` runs to the
    /// end of the file's production text, so a later function carrying the
    /// same needle satisfied the assertion — and dropping `//` lines keeps a
    /// comment that names the needle from satisfying it either.
    fn method_body(src: &str, signature: &str) -> String {
        let start = src
            .find(signature)
            .unwrap_or_else(|| panic!("`{signature}` must still exist"));
        let end = start
            + src[start..]
                .find("\n    }\n")
                .unwrap_or_else(|| panic!("`{signature}` must close at impl indentation"));
        src[start..end]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `body` must consult the shared raw-output policy, and the `if` that does
    /// so must return before `dispatch`. The `return;` is checked inside the
    /// gate's own block: a `let … else { return; }` between the gate and the
    /// dispatch (the exposure meter has one) would otherwise stand in for it.
    fn assert_raw_output_gate_returns_before_dispatch(body: &str, dispatch: &str, pass: &str) {
        let gate = body
            .find("render_debug_requires_raw_output(")
            .unwrap_or_else(|| panic!("{pass} must consult the shared raw-output policy"));
        let dispatch_pos = body
            .find(dispatch)
            .unwrap_or_else(|| panic!("{pass} must still contain `{dispatch}`"));
        let block_end = gate
            + body[gate..]
                .find('}')
                .unwrap_or_else(|| panic!("{pass}'s raw-output gate must be an `if` block"));
        assert!(
            block_end < dispatch_pos && body[gate..block_end].contains("return;"),
            "{pass} must return from the raw-output gate before `{dispatch}` — a raw \
             correctness view must not reach the dispatch"
        );
    }

    /// #3572 (W3.16) — the TAA resolve must consume the SAME post-composite,
    /// post-bloom scene image FSR resolves: composite → bloom → TAA →
    /// upscale. Pin the record_post_passes ordering statically (matching
    /// this module's scanner convention), plus the two invariants that make
    /// the move sound: the raw-output gate on the TAA dispatch, and the
    /// upscale input switching to TAA's output only when the resolve ran.
    #[test]
    fn taa_resolves_the_post_bloom_scene_tap() {
        let full_src = include_str!("post_passes.rs");
        let test_mod_start = full_src
            .find("#[cfg(test)]")
            .expect("this file has at least one #[cfg(test)] module");
        let src = &full_src[..test_mod_start];

        let fn_start = src
            .find("pub(super) fn record_post_passes(")
            .expect("record_post_passes must exist");
        let body = &src[fn_start..];
        let composite = body.find("self.record_composite_pass(cmd, frame);").expect("composite call");
        let bloom = body.find("self.record_bloom_pass(cmd, frame);").expect("bloom call");
        let meter = body
            .find("self.record_exposure_meter_pass(cmd, frame);")
            .expect("exposure meter call");
        let taa = body.find("self.record_taa_pass(cmd, frame);").expect("taa call");
        let upscale = body.find("self.record_upscale_pass(cmd, frame, fsr_frame);").expect("upscale call");
        assert!(
            composite < bloom && bloom < meter && meter < taa && taa < upscale,
            "the frame tail must order composite -> bloom -> exposure meter -> TAA -> \
             upscale: the meter writes the exposure texel FSR normalizes against \
             (#3572 sequencing, Stage 1 exposure)"
        );
        assert!(
            !body[..taa].contains("self.record_taa_pass(cmd, frame);"),
            "TAA must appear exactly once in the tail — a pre-composite \
             dispatch would resolve the raw direct-only attachment again"
        );

        // The TAA dispatch must respect the shared raw-output policy — post-move
        // it filters the final image and would temporally smooth raw
        // correctness views otherwise.
        assert_raw_output_gate_returns_before_dispatch(
            &method_body(src, "fn record_taa_pass("),
            "taa.dispatch(",
            "record_taa_pass",
        );
        // #4591 — the exposure meter carries the same gate: in auto mode it
        // would adapt the persistent per-slot exposure toward the debug
        // image, and dismissing the view pops + re-adapts from the wrong
        // starting value.
        assert_raw_output_gate_returns_before_dispatch(
            &method_body(src, "fn record_exposure_meter_pass("),
            "meter.dispatch(",
            "record_exposure_meter_pass",
        );
        assert!(
            src.contains("scene_color_layout: source_layout,"),
            "the upscale dispatch must carry the blit source's layout — the \
             TAA output slot arrives in GENERAL and must be restored to it"
        );
    }

    /// Bloom mutates composite's scene image after the composite shader has
    /// returned, so raw correctness views need a CPU-side gate here; their
    /// shader early returns cannot prevent the later in-place add.
    #[test]
    fn record_bloom_pass_skips_raw_correctness_views_before_dispatch() {
        let full_src = include_str!("post_passes.rs");
        let test_mod_start = full_src
            .find("#[cfg(test)]")
            .expect("this file has at least one #[cfg(test)] module");
        let src = &full_src[..test_mod_start];
        let fn_start = src
            .find("fn record_bloom_pass(")
            .expect("record_bloom_pass must still exist");
        let fn_end = src[fn_start..]
            .find("\n    fn record_composite_pass(")
            .map(|rel| fn_start + rel)
            .expect("record_composite_pass must still follow record_bloom_pass");
        let body = &src[fn_start..fn_end];
        let gate = body
            .find("render_debug_requires_raw_output(")
            .expect("bloom must consult the shared raw-output policy");
        let dispatch = body
            .find("bloom.dispatch(")
            .expect("bloom dispatch must still exist");

        assert!(
            gate < dispatch && body[gate..dispatch].contains("return;"),
            "record_bloom_pass must return before dispatch for raw correctness views"
        );
    }
}

/// #4618 — the exposure meter ran every frame in no GPU-timer bracket, the
/// third pass to (#3676, #4315) after the bracket set was thought complete.
/// This derives the set to check from `record_post_passes`'s own call list, so
/// a new pass added there without a bracket fails here instead of showing up
/// later as unexplained frame time.
#[cfg(test)]
mod post_pass_gpu_timer_bracket_tests {
    /// Passes `record_post_passes` calls that are deliberately not bracketed,
    /// each with why. Empty: all ten are bracketed.
    const UNBRACKETED: [(&str, &str); 0] = [];

    /// A method's text: from its `fn <name>(` header to the first closing
    /// brace at the method indent (nested blocks are indented deeper).
    fn method<'a>(src: &'a str, name: &str) -> &'a str {
        let header = format!("fn {name}(");
        let start = src
            .find(&header)
            .unwrap_or_else(|| panic!("`{header}` not found in post_passes.rs"));
        let body = &src[start..];
        &body[..body.find("\n    }\n").expect("method closes at its indent")]
    }

    #[test]
    fn every_post_pass_is_gpu_timer_bracketed_or_deliberately_not() {
        let src = crate::source_scan::production_text(include_str!("post_passes.rs"));
        let calls = method(src, "record_post_passes");

        let passes: Vec<&str> = calls
            .match_indices("self.record_")
            .filter_map(|(at, _)| {
                let rest = &calls[at + "self.".len()..];
                Some(&rest[..rest.find('(')?])
            })
            .filter(|name| name.ends_with("_pass"))
            .collect();

        // Liveness: a scan that stopped matching would pass vacuously.
        assert!(
            passes.len() >= 10 && passes.contains(&"record_exposure_meter_pass"),
            "the post-pass call scan found {passes:?}; fix the scan before trusting this guard"
        );

        for pass in passes {
            let excused = UNBRACKETED.iter().any(|(name, _)| *name == pass);
            let bracketed = method(src, pass).contains("timers.cmd_");
            assert!(
                bracketed || excused,
                "`{pass}` runs every frame from `record_post_passes` but writes no GPU timestamps, \
                 so its cost lands in no `gpu_timers` bracket (#3676, #4315, #4618). Bracket it \
                 (see `gpu_timers.rs`), or add it to UNBRACKETED with the reason it is not worth measuring"
            );
            assert!(
                !(bracketed && excused),
                "`{pass}` is bracketed and also in UNBRACKETED; drop the stale entry"
            );
        }
    }
}
