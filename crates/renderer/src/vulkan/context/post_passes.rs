//! Post-geometry pass recording — water-caustic barrier, SVGF denoise,
//! caustic splat, volumetrics, TAA, SSAO, bloom, and the final composite.
//! Extracted from `draw.rs` (#1857 / TD1-001) to shrink that file; the
//! recording order and per-pass permanent-failure latches are unchanged.
//!
//! Also carries `copy_depth_to_history`, the small depth→history-image
//! copy that feeds soft-particle fade and runs immediately around the
//! post-pass sequence.

use super::super::descriptors::memory_barrier;
use super::{SkyParams, VulkanContext};
use ash::vk;

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
                width: self.swapchain_state.extent.width,
                height: self.swapchain_state.extent.height,
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
    /// denoise, caustic splat, volumetrics, TAA, SSAO, bloom, and the
    /// final composite, in that fixed order. Extracted verbatim from
    /// `draw_frame` (#1748 / TD1-001) to shrink that function; recording
    /// order and the per-pass permanent-failure latches are preserved
    /// exactly. Call after the main render pass ends and before
    /// `end_command_buffer`.
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
        inv_vp_arr: [[f32; 4]; 4],
        sky_params: &SkyParams,
        fog_far: f32,
    ) {
        // SAFETY: `cmd` is in the recording state — opened by
        // `begin_command_buffer` in `draw_frame` and not yet closed — and
        // this chain runs once per frame between the main render pass end
        // and `end_command_buffer`. Each `*.dispatch` / `cmd_*` records
        // into `cmd` in the documented order; the per-pass failure latches
        // (`svgf_failed` / `taa_failed` / `caustic_failed`) keep a failed
        // pass from re-recording. This is the same single `unsafe` scope
        // `draw_frame` wrapped this chain in before it was extracted (#1748).
        unsafe {
            if let Some(ref wca) = self.water_caustic_accum {
                wca.barrier_post_render_pass(&self.device, cmd, frame);
            }

            // SVGF temporal accumulation (Phase 3): reprojects previous
            // frame's accumulated indirect, blends with raw 1-SPP indirect
            // at α=0.2. Reads G-buffer raw_indirect/motion/mesh_id (now in
            // SHADER_READ_ONLY_OPTIMAL via render pass final_layout) +
            // history from previous frame's SVGF output slot, writes this
            // frame's accumulated indirect + moments. Composite samples
            // the output below.
            // SVGF permanent-failure latch: after the first dispatch
            // error, skip all further attempts and leave the warn-log
            // behind (escalated to `error!` so the once-per-session
            // signal stands out). Composite's `indirectTex` descriptor
            // keeps pointing at the stale denoised image until the
            // next `recreate_swapchain` resets the latch. Rebinding to
            // the raw-indirect G-buffer view would give a live (noisy)
            // picture but requires composite-side plumbing deferred
            // until a real lost-device repro. See #479.
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

            // Caustic scatter (#321): per-refractive-pixel refracted-light
            // splat. Runs after SVGF (reads the same G-buffer slots that
            // are now in SHADER_READ_ONLY_OPTIMAL) and before composite
            // (which samples the caustic accumulator). Writes binding 5
            // of the composite descriptor set.
            // Caustic permanent-failure latch — same shape as SVGF.
            // Composite's `causticTex` sampler keeps reading the
            // accumulator's last valid contents, so at worst one
            // stale caustic frame hangs around until resize. See #479.
            if !self.caustic_failed {
                if let Some(ref mut caustic) = self.caustic {
                    // Bind this frame's TLAS before dispatch — the AccelerationManager
                    // rebuilds/refits per frame but the handle is stable across frames
                    // once created, so we write it once and then again defensively.
                    // Skip the dispatch entirely when no TLAS is available
                    // for this frame (RT unsupported or scene-load not yet
                    // settled). Mirrors the shader's `sceneFlags.x < 0.5`
                    // early-out — pre-#640 the dispatch ran every frame
                    // regardless and the shader paid full ray-query cost
                    // against unwritten / stale TLAS state.
                    let tlas_handle = self
                        .accel_manager
                        .as_ref()
                        .and_then(|accel| accel.tlas_handle(frame));
                    if let Some(tlas) = tlas_handle {
                        caustic.write_tlas(&self.device, frame, tlas);
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_caustic_splat_start(&self.device, cmd, frame);
                        }
                        let caustic_result =
                            caustic.dispatch(&self.device, cmd, frame, camera_static);
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_caustic_splat_end(&self.device, cmd, frame);
                        }
                        if let Err(e) = caustic_result {
                            log::error!(
                                "Caustic dispatch failed — pass disabled for the rest of the session: {e}"
                            );
                            self.caustic_failed = true;
                        }
                    }
                }
            }

            // Volumetric lighting (M55 Phase 2c — sun-only injection
            // with HG phase + RT shadow visibility). Runs before TAA /
            // SSAO / composite so the fragment shader can sample the
            // integrated volume.
            //
            // ── Composite-output gate (#928) ────────────────────────
            // The composite shader currently multiplies the volumetric
            // result by 0.0 (composite.frag:362) because the per-
            // froxel single-shadow-ray approach produces visible
            // banding on bright surfaces (diagnosed 2026-05-09 against
            // Prospector cups and lanterns). While the output is
            // unused, dispatching the inject + integrate passes is
            // pure GPU waste — ~1.84M ray-query traces and ~28 MB of
            // memory bandwidth per frame for nothing.
            //
            // The `VOLUMETRIC_OUTPUT_CONSUMED` const in volumetrics.rs
            // is the single source of truth for whether the read is
            // active. Both that const and the `* 0.0` in composite.frag
            // get flipped together when M-LIGHT v2 (multi-tap soft
            // shadows + temporal stability) lands and removes the
            // banding. See #928.
            //
            // Gated on TLAS being available, mirroring caustic
            // (caustic.rs:627 / draw.rs:1648). When no TLAS exists
            // (RT unsupported, scene not yet built, accel_manager
            // absent) we skip BOTH the descriptor write and the
            // dispatch — composite reads the prior frame's integrated
            // volume, which retains its last valid contents (or the
            // post-`initialize_layouts` zero-init on the very first
            // frame).
            //
            // Sun direction + radiance are plumbed from
            // `SkyParams::sun_direction` / `sun_color` / `sun_intensity`
            // (#1022 / REN-D18-008). Below-horizon (`sun_intensity <= 0`)
            // still zeros `sun_color` regardless of interior/exterior — no
            // sun, no godrays, trivially correct either way.
            //
            // Interior vs exterior no longer zeroes sun/scattering outright
            // (pre-fix behavior — see git blame for the old gate). That
            // approach was too blunt: it also blocked real sun-through-
            // window godrays the moment #928 flips
            // VOLUMETRIC_OUTPUT_CONSUMED on. Instead the inject shader
            // itself distinguishes "real window" from "geometry gap" via
            // `render_origin.w` (is_exterior) — see the two-pass shadow-ray
            // note on `VolumetricsParams::render_origin` in `volumetrics.rs`
            // and the interior-godray investigation: a `--cell`-loaded
            // interior has no complete ceiling mesh (never seen from
            // inside, so Bethesda authoring omits it), so a naive single
            // opaque-mask shadow ray escaping upward would register as
            // "lit" everywhere, not just through real windows.
            if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                if let Some(ref mut vol) = self.volumetrics {
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
                    if let (Some(tlas), Some((light_buf, light_buf_size, grid_buf, index_buf))) =
                        (vol_tlas, vol_lights)
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
                        let scatter_coef = super::super::volumetrics::DEFAULT_SCATTERING_COEF;
                        let vol_params = super::super::volumetrics::VolumetricsParams {
                            inv_view_proj: inv_vp_arr,
                            camera_pos: [camera_pos[0], camera_pos[1], camera_pos[2], scatter_coef],
                            sun_dir: [
                                sky_params.sun_direction[0],
                                sky_params.sun_direction[1],
                                sky_params.sun_direction[2],
                                super::super::volumetrics::DEFAULT_PHASE_G,
                            ],
                            sun_color: sun_radiance,
                            volume_extent: [
                                super::super::volumetrics::DEFAULT_VOLUME_FAR,
                                0.0,
                                0.0,
                                0.0,
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
                        };
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_volumetrics_start(&self.device, cmd, frame);
                        }
                        let vol_result = vol.dispatch(&self.device, cmd, frame, &vol_params);
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_volumetrics_end(&self.device, cmd, frame);
                        }
                        if let Err(e) = vol_result {
                            log::warn!("Volumetrics dispatch failed: {e}");
                        }
                    }
                }
            }

            // TAA resolve: reprojects previous frame's history via motion
            // vectors, neighborhood-clamps in YCoCg, and writes the anti-
            // aliased HDR result for composite to sample. Runs after SVGF
            // (which denoises the indirect term) and before SSAO/composite.
            // TAA permanent-failure recovery: on the first dispatch
            // error the composite's binding 0 (which currently points
            // at TAA's output) gets rebound to the raw HDR render-pass
            // attachments so the screen keeps updating — without the
            // fallback the last TAA-written HDR frame would freeze on
            // screen for the rest of the session with only a `warn!`
            // log hinting at the cause. See #479.
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

            // SSAO compute pass: reads depth buffer (now in READ_ONLY layout
            // after render pass), writes AO texture for this frame's fragment
            // shader. Runs before composite so AO is current-frame (no lag).
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

            // Bloom pyramid (M58). Reads the raw pre-TAA HDR attachment
            // (`composite.hdr_image_views[frame]` — the main render pass'
            // HDR target, NOT TAA's output) and writes a multi-scale
            // blurred bright-content texture. Composite adds bloom to
            // `combined` before the ACES tone-map. The render pass's
            // final_layout already moved HDR to SHADER_READ_ONLY_OPTIMAL,
            // so the input is sample-ready.
            //
            // Why pre-TAA: TAA's resolved output is consumed by composite
            // separately (`composite.rebind_hdr_views` rewires the
            // descriptor at `context/mod.rs:1715-1717`, but the
            // `hdr_image_views` field still references the raw attachment
            // — only the descriptor was swapped). Bloom intentionally
            // shares the raw view because the blur pyramid smears out
            // sub-pixel jitter, making the bloom haloes spatially stable
            // anyway. Final image = ACES(TAA-stable base + spatial bloom).
            // #1166: the previous comment claimed bloom was post-TAA;
            // that was wrong. #1107 / REN-D19-002 is the original
            // rewire-composite-to-TAA work this commit references.
            //
            // The `if let Some(...)` guard below is dead at runtime
            // (#1276): `VulkanContext::new` at `context/mod.rs:1958-1967`
            // hard-fails with `anyhow::anyhow!(...)` if bloom init
            // returns `None` (policy from #1081 — no fallback binding
            // for bloomTex when bloom is absent), so the engine never
            // reaches `draw_frame` with `self.bloom == None`. The
            // `Option` wrapper is kept because the resize-recreate
            // path benefits from it as a temporary, but the runtime
            // `None` branch is unreachable.
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

            // Composite UBO host-write + barrier moved to the pre-render-
            // pass bulk barrier site (#909 / REN-D1-NEW-03). The dedicated
            // late HOST→FRAGMENT barrier was correct but isolated 750
            // lines from the bulk barrier; folded into it now so all
            // host writes consumed by the render pass / composite pass
            // share one execution dependency.

            // Composite pass: sample HDR + indirect + albedo, combine, ACES
            // tone map, write to swapchain. Runs in its own render pass.
            // The main render pass's outgoing subpass dependency handles
            // the layout transitions of all input attachments to
            // SHADER_READ_ONLY_OPTIMAL.
            if let Some(ref composite) = self.composite {
                let bindless_set = self.texture_registry.descriptor_set(frame);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_composite_start(&self.device, cmd, frame);
                }
                composite.dispatch(&self.device, cmd, frame, img, bindless_set);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_composite_end(&self.device, cmd, frame);
                }
            }
        }
    }
}
