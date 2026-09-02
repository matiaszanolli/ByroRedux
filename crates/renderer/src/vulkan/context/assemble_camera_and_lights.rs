//! Per-frame camera + light assembly — extracted from `draw.rs` (#3282 /
//! TD1-2026-08-24-01) to shrink `draw_frame`. Covers draining the
//! transported-combustion light field, uploading lights, TAA/FSR jitter,
//! DOF, camera-cut detection, the `GpuCamera`/DALC UBO uploads, and the
//! frame-over-frame camera-state bookkeeping; the recording order is
//! unchanged from the pre-split `draw_frame`.

use super::super::frame_upscaler::FsrFrameParameters;
use super::super::scene_buffer;
use super::draw::{
    build_fsr_frame_parameters, camera_frame_deltas, dof_effective_view_proj, fsr_gated_dof,
    is_camera_cut, origin_corrected_prev_view_proj, taa_jitter, CameraFrameDeltas,
};
use super::{DofView, SkyParams, VulkanContext};
use anyhow::Result;

/// Pack the two normalized weather-surface channels into the structured
/// debug lane of `GpuCamera`. Keeping this as two u16 values gives the
/// terrain shader enough precision for gradual wetting/melting without
/// changing the CameraUBO size or any of its five GLSL mirrors.
pub(super) fn pack_weather_surface(wetness: f32, snow: f32) -> u32 {
    fn pack(value: f32) -> u32 {
        if value.is_finite() {
            (value.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32
        } else {
            0
        }
    }
    pack(wetness) | (pack(snow) << 16)
}

/// Output of [`VulkanContext::assemble_camera_and_lights`] — the locals
/// later phases (or `draw_frame`'s own tail, after `record_geometry_pass`)
/// still need. A struct rather than a long tuple: 9 fields of similar
/// primitive types (several `[f32; N]` / `[f32; 16]`) would be easy to
/// transpose positionally at the call site.
pub(super) struct CameraAssemblyOutput {
    /// Lights for this frame, including any transported-combustion
    /// contribution — owned so it can survive back to `draw_frame`'s tail
    /// (`self.frame_lights_scratch` amortization), which reads it well
    /// after `record_geometry_pass`. `draw_frame` re-derives the `lights`
    /// slice via `.as_slice()` wherever a later phase needs it, exactly as
    /// the pre-split code re-bound the same local.
    pub(super) frame_lights: Vec<scene_buffer::GpuLight>,
    pub(super) camera_cut: bool,
    pub(super) camera_static: bool,
    /// The jittered/DOF view-projection actually uploaded this frame
    /// (`vp` in the pre-split code was `&effective_vp`; returned owned here
    /// since a borrow can't outlive this function).
    pub(super) effective_vp: [f32; 16],
    /// Origin-corrected previous-frame view-projection (`pvp`).
    pub(super) pvp: [f32; 16],
    pub(super) inv_vp_arr: [[f32; 4]; 4],
    pub(super) render_origin: byroredux_core::math::Vec3,
    pub(super) previous_camera_position: [f32; 3],
    pub(super) fsr_frame: Option<FsrFrameParameters>,
}

impl VulkanContext {
    /// Drain combustion-field lights, upload lights + camera + DALC UBOs,
    /// resolve TAA/FSR jitter and DOF, detect camera cuts, and update the
    /// frame-over-frame camera bookkeeping. Extracted verbatim from
    /// `draw_frame` — the recording order is unchanged.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assemble_camera_and_lights(
        &mut self,
        frame: usize,
        lights: &[scene_buffer::GpuLight],
        fog_volumes: &[super::super::volumetrics::GpuFogVolume],
        view_proj: &[f32; 16],
        camera_pos: [f32; 3],
        input_render_origin: [f32; 3],
        ambient_color: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        fog_extinction_per_meter: f32,
        sky_params: &SkyParams,
        dof: DofView,
        frame_time_delta_ms: f32,
    ) -> Result<CameraAssemblyOutput> {
        // Drain the completed transported combustion field before uploading
        // scene lights. This is intentionally a renderer boundary: the app
        // submits canonical medium primitives, while only the renderer owns
        // the advected/cooled field that actually emits this delayed light.
        let mut frame_lights = std::mem::take(&mut self.frame_lights_scratch);
        frame_lights.clear();
        frame_lights.extend_from_slice(lights);
        if let Some(ref mut volumetrics) = self.volumetrics {
            if let Err(error) = volumetrics.append_combustion_surface_lights(
                &self.device,
                frame,
                fog_volumes,
                &mut frame_lights,
            ) {
                log::warn!("combustion surface-light readback failed: {error}");
            }
        }
        // The app already sorts authored local lights for the fixed-prefix GI
        // scan. Re-sort after adding field-derived lights using the canonical
        // score carried by GpuLight itself; directional lights remain pinned.
        let directional_count = frame_lights
            .iter()
            .take_while(|light| light.color_type[3] > 1.5)
            .count();
        frame_lights[directional_count..]
            .sort_unstable_by(|a, b| b.gi_priority_score().total_cmp(&a.gi_priority_score()));
        let lights = frame_lights.as_slice();

        // Upload scene data (lights + camera) BEFORE the render pass begins.
        self.scene_buffers
            .upload_lights(&self.device, frame, lights)
            .unwrap_or_else(|e| log::warn!("Failed to upload lights: {e}"));
        // `tlas_written[frame]` lags one frame per FIF slot — on the
        // first frame each slot gets a successful TLAS, this still reads
        // `false` because `write_tlas` runs later in `draw_frame` (see
        // the `patch_camera_rt_flag` site post-TLAS-build). The first-
        // frame fallback to `rt_flag = 0.0` is corrected in-place after
        // `write_tlas` flips the bit, so frame 0 still gets RT-enabled
        // shading at GPU-submit time. See #1227 / REN-D8-NEW-21.
        let rt_flag =
            if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
                1.0
            } else {
                0.0
            };

        // TAA sub-pixel jitter via Halton(2,3) sequence. Each frame shifts
        // the projection by a different sub-pixel offset in NDC so that
        // temporal blending reconstructs a super-sampled result. The offset
        // is applied in the vertex shader AFTER motion vector computation so
        // reprojection is jitter-free.
        //
        // Period 16 (#1093 / REN-D11-002) — see `taa_jitter`'s doc comment
        // for the corrected rationale (2026-08-31): the "natural period"
        // framing this comment used to repeat here was mathematically
        // false and misidentified which sample % 8 misses; do not re-quote
        // it, the real reason for 16 over 8 is not re-derived.
        // #1932 / TAA-D13-01 — gate on `!self.taa_failed` too, matching the
        // dispatch gate above and `upload_params`. Without it, a permanent
        // TAA failure would leave composite reading raw un-resolved HDR
        // (per #479's fallback) while geometry kept rendering with a
        // per-frame Halton sub-pixel offset — full-frame shimmer instead of
        // a stable pinhole fallback image.
        let (jx, jy, fsr_jitter_pixel, fsr_reset_pending) = match self.renderer_config.upscaler {
            super::super::upscaling::UpscalerMode::Taa => {
                let (jx, jy) = taa_jitter(
                    self.taa.is_some(),
                    self.taa_failed,
                    self.frame_counter,
                    self.frame_extents.render.width as f32,
                    self.frame_extents.render.height as f32,
                );
                (jx, jy, None, false)
            }
            super::super::upscaling::UpscalerMode::Fsr3(_) => {
                if !self.is_fsr_dispatch_active() {
                    (0.0, 0.0, None, false)
                } else {
                    let fsr = self
                        .fsr_temporal
                        .as_ref()
                        .expect("FSR mode must own temporal state");
                    let sample = fsr.current();
                    (
                        sample.ndc[0],
                        sample.ndc[1],
                        Some(sample.pixel),
                        fsr.reset_pending(),
                    )
                }
            }
        };

        // Camera-relative render origin (#markarth-precision). Computed
        // ONCE by `render::camera::assemble_camera` (the same un-jittered
        // camera position it used to build the RELATIVE `view_proj`) and
        // threaded in via `FrameInputs::render_origin` (#2043 / PERF-D9-04)
        // — this consumer no longer recomputes `snap_render_origin`
        // independently, so the rebased per-instance models below and the
        // uploaded matrices are structurally guaranteed to agree on the
        // origin rather than relying on both call sites happening to be
        // passed the same value. Uploaded `view_proj` / `inv_view_proj` are
        // relative; the vertex shader reconstructs the absolute world
        // position as `worldPos_rel + renderOrigin`. Passes that
        // reconstruct world from an inverse VP either add the origin back
        // where absolute space is required (cluster_cull, caustic_splat,
        // volumetrics_inject) or stay fully relative with a relative
        // camera position (ssao, composite — origin-invariant differences
        // only). See `GpuCamera::render_origin` (#1492).
        let render_origin = byroredux_core::math::Vec3::from_array(input_render_origin);
        // DOF aperture-disk jitter, or the pinhole pass-through. The bokeh
        // rationale and the #1525 degenerate-`focus_dist` guard live in
        // `dof_effective_view_proj`.
        // FSR forces the pinhole path — rationale in `fsr_gated_dof`.
        //
        // #2518 — gate on FSR actually *dispatching*, not merely on FSR
        // mode being selected. `fsr_temporal` is `Some` for the whole of
        // `UpscalerMode::Fsr3(..)`, including when the FSR context never got
        // created or `dispatch_failure` has latched. In those states the
        // frame runs completely unjittered on the native blit (the jitter
        // gate above sets `fsr_jitter_pixel = None` and `jx/jy = 0.0` on
        // exactly this predicate), so the stated rationale — that the
        // independent Halton(5,7) lens sequence would conflict with FSR's
        // own projection jitter — does not apply, yet authored DOF was
        // still being silently dropped. Both facts now come from one
        // predicate so they cannot diverge again.
        let active_dof = fsr_gated_dof(dof, self.is_fsr_dispatch_active());
        let (effective_vp, effective_cam_pos) = dof_effective_view_proj(
            &active_dof,
            self.frame_counter,
            camera_pos,
            render_origin,
            view_proj,
        );
        let vp = &effective_vp;
        let mut fsr_frame = match build_fsr_frame_parameters(
            &active_dof,
            fsr_jitter_pixel,
            fsr_reset_pending,
            frame_time_delta_ms,
        ) {
            Ok(params) => params,
            Err(e) => {
                let _ = unsafe {
                    // SAFETY: this early-return happens after the swapchain
                    // image acquire but before any batch is submitted this
                    // frame (the `return` below aborts before `queue_submit`),
                    // `frame < MAX_FRAMES_IN_FLIGHT` per the caller's frame
                    // index, and `self.device` is the same device that
                    // allocated the existing semaphore.
                    self.frame_sync
                        .recreate_image_available_for_frame(&self.device, frame)
                };
                return Err(e);
            }
        };
        // Automatic camera-cut detection catches debug teleports and scripted
        // snaps that do not flow through the cell-transition reset hooks.
        // Signal derivation + rationale in `camera_frame_deltas`.
        let previous_camera_position = self.prev_camera_position;
        let CameraFrameDeltas {
            camera_delta,
            cam_forward_dot,
            vp_max_abs_delta,
        } = camera_frame_deltas(
            camera_pos,
            self.prev_camera_position,
            active_dof.cam_forward,
            self.prev_cam_forward,
            vp,
            &self.prev_view_proj,
        );
        let camera_cut = is_camera_cut(self.frame_counter, camera_delta, cam_forward_dot);
        if camera_cut {
            self.signal_temporal_discontinuity(8);
            if let Some(ref mut frame) = fsr_frame {
                frame.reset = true;
            }
        }
        // #1489 / REN2-04 — `prev_view_proj` is relative to LAST frame's
        // render origin O₁; this frame's geometry (per-instance models, bone
        // palettes) is rebased by the CURRENT origin O₂. On a 4096-grid
        // crossing the two differ and every motion vector would be off by
        // ΔO — a one-frame full-screen TAA flash + SVGF history drop per
        // crossing. Right-multiplying by `translation(O₂ − O₁)` makes the
        // uploaded matrix consume current-origin positions exactly:
        // `M·(x − O₂) = prev_vp·(x − O₁)`. Off the jump frame ΔO = 0 and
        // the correction is the identity.
        let pvp = if camera_cut {
            // Reset history and emit zero velocity on the cut frame. Keeping
            // the old matrix here would feed extreme motion into the
            // disocclusion filters even though their history was flushed.
            *vp
        } else {
            origin_corrected_prev_view_proj(
                &self.prev_view_proj,
                self.prev_render_origin,
                [render_origin.x, render_origin.y, render_origin.z],
            )
        };
        // Precompute inverse(viewProj) once on the CPU so shaders
        // (cluster culling, SSAO) can read it directly from the UBO
        // instead of computing a ~100 ALU-op matrix inverse per invocation.
        let vp_mat = byroredux_core::math::Mat4::from_cols_array(vp);
        let inv_vp = vp_mat.inverse();
        let inv_vp_cols = inv_vp.to_cols_array();
        let inv_vp_arr = [
            [
                inv_vp_cols[0],
                inv_vp_cols[1],
                inv_vp_cols[2],
                inv_vp_cols[3],
            ],
            [
                inv_vp_cols[4],
                inv_vp_cols[5],
                inv_vp_cols[6],
                inv_vp_cols[7],
            ],
            [
                inv_vp_cols[8],
                inv_vp_cols[9],
                inv_vp_cols[10],
                inv_vp_cols[11],
            ],
            [
                inv_vp_cols[12],
                inv_vp_cols[13],
                inv_vp_cols[14],
                inv_vp_cols[15],
            ],
        ];
        // Camera-static detection for progressive temporal accumulation.
        // The view-proj here is jitter-free (TAA sub-pixel jitter is applied
        // later in the vertex shader), so a matrix unchanged frame-to-frame
        // means a parked camera. Computed BEFORE the camera UBO is built so
        // the flag can ride `dof_params.w` into triangle.frag's GI-seed
        // decorrelation, and BEFORE `prev_view_proj` is overwritten below.
        let camera_static = vp
            .iter()
            .zip(self.prev_view_proj.iter())
            .all(|(a, b)| (a - b).abs() < 1.0e-6);
        let camera = scene_buffer::GpuCamera {
            view_proj: [
                [vp[0], vp[1], vp[2], vp[3]],
                [vp[4], vp[5], vp[6], vp[7]],
                [vp[8], vp[9], vp[10], vp[11]],
                [vp[12], vp[13], vp[14], vp[15]],
            ],
            prev_view_proj: [
                [pvp[0], pvp[1], pvp[2], pvp[3]],
                [pvp[4], pvp[5], pvp[6], pvp[7]],
                [pvp[8], pvp[9], pvp[10], pvp[11]],
                [pvp[12], pvp[13], pvp[14], pvp[15]],
            ],
            inv_view_proj: inv_vp_arr,
            // w = monotonic frame counter for temporal jitter seed in
            // shadow rays. Masked to the bottom 24 bits before the
            // `u32 → f32` cast so consecutive frames remain
            // distinguishable for the full uptime of the process:
            // f32's mantissa stops resolving ±1 increments above 2^24,
            // so a raw cast at frame 16_777_217 would map to the same
            // `cameraPos.w` as frame 16_777_216 and the RT noise
            // patterns (reservoir streaming, shadow / reflection /
            // refraction jitter, GI hemisphere) would freeze. Wrap at
            // 2^24 instead — the noise pattern repeats every ~3.2 days
            // at 60 FPS (acceptable; TAA accumulation absorbs the
            // discontinuity). See #1161 / REN-D9-NEW-08.
            position: [
                effective_cam_pos[0],
                effective_cam_pos[1],
                effective_cam_pos[2],
                (self.frame_counter & 0xFFFFFF) as f32,
            ],
            flags: [
                rt_flag,
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
            ],
            screen: [
                self.frame_extents.render.width as f32,
                self.frame_extents.render.height as f32,
                fog_near,
                fog_far,
            ],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                if fog_extinction_per_meter > 0.0 {
                    1.0
                } else {
                    0.0
                }, // fog enabled flag
            ],
            // jitter[2] carries the debug-bypass bitmask for the
            // fragment shader (see `parse_render_debug_flags_env` and
            // `triangle.frag`'s `floatBitsToUint(jitter.z)` branches).
            // Zero-bits → free no-op; non-zero → debug paths active.
            //
            // jitter[3] carries the per-frame `is_exterior` flag
            // (#1125 / REN-D9-NEW-01). 1.0 = exterior cell (real TOD-
            // driven SkyParamsRes loaded), 0.0 = interior cell (or no
            // exterior load yet — `SkyParamsRes` absent so
            // `build_sky_params` returned `SkyParams::default()` with
            // clear-noon-blue zenith). The shader uses this to gate
            // `skyTint`-blended fallbacks in `traceReflection` /
            // refraction miss so sealed interiors don't bleed
            // daylight tint into glass refractions.
            jitter: [
                jx,
                jy,
                // REND-#1451 — OR the runtime legacy-attenuation toggle
                // (console-driven via LightTuning) onto the env-set
                // debug bitmask so both paths reach the shader's
                // `DBG_LEGACY_LIGHT_ATTEN` branch.
                f32::from_bits(
                    self.render_debug_flags
                        | if self.light_atten_legacy {
                            crate::shader_constants::DBG_LEGACY_LIGHT_ATTEN
                        } else {
                            0
                        },
                ),
                if sky_params.is_exterior { 1.0 } else { 0.0 },
            ],
            // #925 / REN-D15-NEW-03 — mirror the composite's
            // `sky_zenith.xyz` here so triangle.frag's window-portal
            // escape transmits a sky tint matching whatever
            // `compute_sky` paints behind the world. Same source of
            // truth → same TOD/weather cross-fade behaviour at no
            // extra upload cost.
            //
            // w = sun_angular_radius (rad). Plumbed from SkyParams so
            // PCSS-lite directional-shadow disk jitter in triangle.frag
            // is tunable per-cell / per-TOD without a shader recompile.
            // See #1023 / REN-D20-NEW-01.
            sky_tint: [
                sky_params.zenith_color[0],
                sky_params.zenith_color[1],
                sky_params.zenith_color[2],
                sky_params.sun_angular_radius,
            ],
            // #3323 — the exterior sky, carried through interior cells so
            // the window-portal escape in `triangle.frag` transmits the
            // live TOD colour instead of `SkyParams::default()`'s
            // clear-noon blue. Deliberately a separate lane from
            // `sky_tint` above: widening `zenith_color` itself on
            // interiors would also move `CompositeParams::sky_zenith`,
            // which is the interior sky leak #2226 removed.
            exterior_sky_tint: [
                sky_params.exterior_zenith_color[0],
                sky_params.exterior_zenith_color[1],
                sky_params.exterior_zenith_color[2],
                0.0,
            ],
            // #1210 — sun direction + intensity, plumbed for water.frag's
            // caustic synthesis (shadow ray to sun → refract on miss).
            // SkyParams.sun_direction is already unit-length and in
            // world space. w carries authored intensity so the caustic
            // splat scales with TOD / weather (dawn / dusk = dimmer
            // caustics, noon = peak).
            sun_direction: [
                sky_params.sun_direction[0],
                sky_params.sun_direction[1],
                sky_params.sun_direction[2],
                sky_params.sun_intensity,
            ],
            // x = aperture half-radius (0.0 → pinhole, DOF jitter skipped),
            // y = focal distance.
            // z = REND-#1451 point/spot attenuation knee fraction,
            // consumed by `pointSpotAtten` in triangle.frag (0 → shader
            // default 0.5). Live-tunable via the `light.atten` console
            // command for the controlled bench.
            // w = camera_static flag (1.0 = parked). triangle.frag reads it
            // to advance the GI noise seed every frame when parked, so the
            // dark indirect-lit floor converges ~4× faster (TARGET 1).
            dof_params: [
                active_dof.aperture,
                active_dof.focus_dist,
                self.light_atten_knee,
                if camera_static { 1.0 } else { 0.0 },
            ],
            // #markarth-precision — camera-relative render origin in xyz.
            // Vertex/deferred shaders add this back to recover the absolute
            // world position from the relative `view_proj` space.
            //
            // `w` is NOT padding (REN-LOW L-10 / #2164): it carries the
            // FSR one-frame-reset flag, read by `triangle.frag`'s FSR-reset
            // debug view. Any shader that treats this as a free slot will
            // fight that consumer — same trap as #1928's
            // `VolumetricsParams.render_origin.w`.
            render_origin: [
                render_origin.x,
                render_origin.y,
                render_origin.z,
                if fsr_reset_pending { 1.0 } else { 0.0 },
            ],
            render_debug: [
                self.render_debug_mode.shader_value(),
                self.renderer_config.rt_test_lod_scale_bits.unwrap_or(0),
                u32::from(self.renderer_config.rt_test_lod_telemetry),
                // Low 16 bits = rain wetness; high 16 bits = snow coverage.
                // The terrain shader decodes this only for exterior LAND
                // surfaces, so the structured debug mode/telemetry lanes
                // remain unchanged.
                pack_weather_surface(
                    sky_params.weather.surface_wetness,
                    sky_params.weather.surface_snow,
                ),
            ],
        };
        self.rt_flag_last_frame =
            match self
                .scene_buffers
                .upload_camera(&self.device, frame, &camera)
            {
                Ok(()) => rt_flag > 0.5,
                Err(error) => {
                    log::warn!("Failed to upload camera: {error}");
                    false
                }
            };
        // #993 — upload the per-TOD-lerped 6-axis directional ambient
        // cube (Skyrim WTHR.DALC). When the cell carries no DALC
        // (FNV / FO3 / Oblivion), `sky_params.dalc_cube` is `None`;
        // we upload a disabled cube so the fragment shader stays on
        // its AMBIENT_AO_FLOOR fallback path. The `flags.x` field is
        // the runtime gate the shader reads.
        let dalc_gpu = if let Some(cube) = sky_params.dalc_cube {
            super::super::scene_buffer::GpuDalcCube {
                pos_x: [cube.pos_x[0], cube.pos_x[1], cube.pos_x[2], 0.0],
                neg_x: [cube.neg_x[0], cube.neg_x[1], cube.neg_x[2], 0.0],
                pos_y: [cube.pos_y[0], cube.pos_y[1], cube.pos_y[2], 0.0],
                neg_y: [cube.neg_y[0], cube.neg_y[1], cube.neg_y[2], 0.0],
                pos_z: [cube.pos_z[0], cube.pos_z[1], cube.pos_z[2], 0.0],
                neg_z: [cube.neg_z[0], cube.neg_z[1], cube.neg_z[2], 0.0],
                specular_fresnel: [
                    cube.specular[0],
                    cube.specular[1],
                    cube.specular[2],
                    cube.fresnel_power,
                ],
                flags: [1.0, 0.0, 0.0, 0.0],
            }
        } else {
            super::super::scene_buffer::GpuDalcCube::default()
        };
        self.scene_buffers
            .upload_dalc(&self.device, frame, &dalc_gpu)
            .unwrap_or_else(|e| log::warn!("Failed to upload DALC cube: {e}"));
        // `camera_static` was computed above (before the camera UBO was
        // built) so the flag could ride `dof_params.w` into triangle.frag's
        // GI-seed decorrelation; it is reused here for the SVGF / TAA /
        // caustic param uploads. Store this frame's viewProj as next frame's
        // "previous" for motion vectors — together with the origin it was
        // built against, so next frame's upload can origin-correct it
        // (#1489 / REN2-04).
        // #2171 — capture the origin delta BEFORE `prev_render_origin` is
        // overwritten on the next line. The trace below used to subtract
        // the field after the assignment, so it printed exactly zero every
        // frame — actively arguing "no origin crossing happened" on
        // precisely the frames the ghosting investigation was looking at.
        let origin_delta = [
            render_origin.x - self.prev_render_origin[0],
            render_origin.y - self.prev_render_origin[1],
            render_origin.z - self.prev_render_origin[2],
        ];

        self.prev_view_proj = *vp;
        self.prev_camera_position = camera_pos;
        self.prev_render_origin = [render_origin.x, render_origin.y, render_origin.z];
        self.prev_cam_forward = active_dof.cam_forward;

        // #1874 diagnostic — ghosted diagonal double-image investigation.
        // Cheap, stateless (uses only locals already computed above) trace
        // of the exact values Dim 10 reasoned about statically: the
        // render-origin/view-proj delta this frame carries and whether a
        // discontinuity-recovery window is active. Enable via
        // `RUST_LOG=byroredux_renderer::vulkan::context::draw=trace` to
        // correlate a live repro's cell-transition frame against these
        // numbers instead of guessing from static analysis alone. Safe to
        // leave in — trace level, zero new state, filtered out by default.
        log::trace!(
            "camera frame={} static={} svgf_recovery_frames={} render_origin_delta=({:.3},{:.3},{:.3}) vp_max_abs_delta={:.6}",
            self.frame_counter,
            camera_static,
            self.svgf_recovery_frames,
            origin_delta[0],
            origin_delta[1],
            origin_delta[2],
            vp_max_abs_delta,
        );

        Ok(CameraAssemblyOutput {
            frame_lights,
            camera_cut,
            camera_static,
            effective_vp,
            pvp,
            inv_vp_arr,
            render_origin,
            previous_camera_position,
            fsr_frame,
        })
    }
}

#[cfg(test)]
mod weather_surface_pack_tests {
    use super::pack_weather_surface;

    #[test]
    fn packs_wetness_low_and_snow_high() {
        let packed = pack_weather_surface(0.5, 1.0);
        assert_eq!(packed & 0xFFFF, 32_768);
        assert_eq!(packed >> 16, 65_535);
    }

    #[test]
    fn clamps_invalid_surface_inputs_to_safe_range() {
        assert_eq!(pack_weather_surface(-1.0, f32::NAN), 0);
        assert_eq!(pack_weather_surface(2.0, f32::INFINITY), 0x0000_FFFF);
    }
}
