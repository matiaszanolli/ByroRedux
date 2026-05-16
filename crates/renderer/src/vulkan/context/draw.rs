//! Frame recording and submission — the per-frame hot path.

use super::super::descriptors::memory_barrier;
use super::super::material::GpuMaterial;
use super::super::pipeline::{gamebryo_to_vk_compare_op, PipelineKey};
use super::super::scene_buffer::{
    self, GpuInstance, GpuTerrainTile, INSTANCE_FLAG_ALPHA_BLEND, INSTANCE_FLAG_CAUSTIC_SOURCE,
    INSTANCE_FLAG_NON_UNIFORM_SCALE, INSTANCE_FLAG_TERRAIN_SPLAT, INSTANCE_RENDER_LAYER_MASK,
    INSTANCE_RENDER_LAYER_SHIFT, INSTANCE_TERRAIN_TILE_MASK, INSTANCE_TERRAIN_TILE_SHIFT,
    MATERIAL_KIND_GLASS,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::water::WaterDrawCommand;
use super::{DrawCommand, FrameTimings, SkyParams, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;

/// Halton low-discrepancy sequence value at `index` (1-indexed) for `base`.
/// Returns a value in [0, 1).
fn halton(mut index: u32, base: u32) -> f32 {
    let mut result = 0.0_f32;
    let mut f = 1.0 / base as f32;
    while index > 0 {
        result += f * (index % base) as f32;
        index /= base;
        f /= base as f32;
    }
    result
}

/// Return `true` when `cmd` represents a real refractive surface that the
/// caustic compute pass (`caustic_splat.comp`) should splat from. The CPU
/// gate produces `INSTANCE_FLAG_CAUSTIC_SOURCE` on the `GpuInstance.flags`
/// word; the compute pass burns `max_lights` TLAS ray queries per flagged
/// pixel, so the gate has to stay tight.
///
/// Accepted refractive signals:
///   * `material_kind == MATERIAL_KIND_GLASS` — engine-classified glass
///     from `render::build_render_data` (alpha-blend + low metal + low
///     roughness + not a decal). See #515 / #706.
///   * Skyrim+ `MultiLayerParallax` (kind 11) with a non-zero inner-layer
///     refraction scale — real two-layer refractive surface.
///
/// Rejected (pre-#922 false positives the old `alpha_blend &&
/// metalness < 0.3` gate caught): hair (HairTint, kind 6), foliage (kind 0
/// alpha-test cutouts), particle billboards (kind 0, emissive), decals
/// (`is_decal` excluded by the glass classifier), `BSEffectShaderProperty`
/// FX cards (kind 101 — MATERIAL_KIND_EFFECT_SHADER).
fn is_caustic_source(cmd: &DrawCommand) -> bool {
    if cmd.material_kind == MATERIAL_KIND_GLASS {
        return true;
    }
    const MATERIAL_KIND_MULTI_LAYER_PARALLAX: u32 = 11;
    if cmd.material_kind == MATERIAL_KIND_MULTI_LAYER_PARALLAX
        && cmd.multi_layer_refraction_scale > 0.0
    {
        return true;
    }
    false
}

/// A batch of instances sharing the same mesh + pipeline state.
/// Drawn with a single `cmd_draw_indexed` call.
///
/// `pub(super)` so the enclosing `VulkanContext` can hold a reusable
/// `Vec<DrawBatch>` scratch buffer as a field and amortize allocations
/// across frames. See issue #243.
pub(super) struct DrawBatch {
    pub mesh_handle: u32,
    /// Pipeline selector. `Opaque` uses the single prebuilt opaque
    /// pipeline; `Blended { src, dst }` resolves through the lazy
    /// blend pipeline cache on `VulkanContext`. See #392 / #930.
    pub pipeline_key: PipelineKey,
    /// Two-sided / cull-disabled rendering. Drives per-batch
    /// `cmd_set_cull_mode(NONE)` (was a separate pipeline pre-#930).
    /// MUST be part of the merge key so adjacent draws with different
    /// cull state don't fold into one batch.
    pub two_sided: bool,
    /// Content-class layer driving the depth-bias ladder
    /// (Architecture / Clutter / Actor / Decal). Replaces the previous
    /// `is_decal` + per-frame `needs_depth_bias` derivation from
    /// commits 0f13ff5 / ee3cb13 — `RenderLayer::Decal` subsumes both.
    /// Set per-DrawCommand at cell-load time from the REFR's base
    /// record type, with the alpha-test / NIF-decal-flag escalation
    /// rule already applied. Bias values come from
    /// `byroredux_core::ecs::components::RenderLayer::depth_bias`.
    pub render_layer: byroredux_core::ecs::components::RenderLayer,
    pub first_instance: u32,
    pub instance_count: u32,
    pub index_count: u32,
    /// Offset into the global index buffer (in indices). Used with the
    /// global geometry SSBO as `first_index` in `cmd_draw_indexed`. #294.
    pub global_index_offset: u32,
    /// Offset into the global vertex buffer (in vertices). Used with the
    /// global geometry SSBO as `vertex_offset` in `cmd_draw_indexed`. #294.
    pub global_vertex_offset: i32,
    /// `NiZBufferProperty.z_test` — fed to `vkCmdSetDepthTestEnable`
    /// before the batch (extended dynamic state, Vulkan 1.3 core).
    /// Batched into the merge key so consecutive draws sharing depth
    /// state pay zero state-change cost. See #398.
    pub z_test: bool,
    /// `NiZBufferProperty.z_write` — fed to `vkCmdSetDepthWriteEnable`.
    pub z_write: bool,
    /// `NiZBufferProperty.z_function` — fed to `vkCmdSetDepthCompareOp`
    /// (Gamebryo `TestFunction` enum mapped to `vk::CompareOp`).
    pub z_function: u8,
}

impl VulkanContext {
    /// Record and submit a frame.
    ///
    /// `view_proj`: combined view-projection matrix as column-major [f32; 16].
    /// `draw_commands`: per-object (mesh_handle, model_matrix) pairs.
    pub fn draw_frame(
        &mut self,
        clear_color: [f32; 4],
        view_proj: &[f32; 16],
        draw_commands: &[DrawCommand],
        lights: &[scene_buffer::GpuLight],
        bone_palette: &[[[f32; 4]; 4]],
        materials: &[GpuMaterial],
        camera_pos: [f32; 3],
        ambient_color: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        // `fog_clip`/`fog_power` carry the XCLL FNV+ cubic-fog curve
        // through to `CompositeParams.fog_params.z/w`. `0.0` on either
        // = no curve; composite falls back to the linear
        // `fog_near..fog_far` ramp. See #865 / FNV-D3-NEW-06.
        fog_clip: f32,
        fog_power: f32,
        ui_texture_handle: Option<u32>,
        sky_params: &SkyParams,
        timings: Option<&mut FrameTimings>,
        // `water_commands`: water-surface draws for this frame. Each
        // entry must match a `DrawCommand` with `is_water=true` that
        // supplies the corresponding `GpuInstance` SSBO slot. Built
        // by the app's per-frame render code from `WaterPlane` ECS
        // entities. Empty slice = no water rendering this frame.
        water_commands: &[WaterDrawCommand],
        // `underwater`: xyz = deep_color tint to blend the scene
        // toward; w = camera depth below the water surface in world
        // units. `[0, 0, 0, 0]` disables underwater FX. Computed by
        // the app from the camera's `SubmersionState` component.
        underwater: [f32; 4],
    ) -> Result<bool> {
        let frame = self.current_frame;
        // Use a local to avoid borrow complexity; copy out at end.
        let mut t = FrameTimings::default();

        // Reset skinned-BLAS coverage counters at frame start so a
        // frame without a skinned section (no RT, no bone buffer)
        // reads zero instead of holding the previous frame's counts.
        // Section-local increments below populate it; `fill_skin_
        // coverage_stats` snapshots it after `Scheduler::run`.
        self.last_skin_coverage_frame =
            super::super::skin_compute::SkinCoverageFrame::default();

        // Wait for this frame-in-flight slot AND the previous slot to be
        // available. SVGF's temporal pass reads the previous slot's G-buffer
        // images (mesh_id, motion, raw_indirect) — without waiting on the
        // other slot's fence, a read-after-write hazard exists when the GPU
        // hasn't finished the other slot's render pass. See #282.
        //
        // Cost: zero in practice — the GPU is rarely more than 1 frame
        // behind the CPU, so the other fence is almost always signaled.
        let fence_t0 = Instant::now();
        unsafe {
            let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
            self.device
                .wait_for_fences(
                    &[
                        self.frame_sync.in_flight[frame],
                        self.frame_sync.in_flight[prev],
                    ],
                    true,
                    u64::MAX,
                )
                .context("wait_for_fences")?;
        }
        t.fence_wait_ns = fence_t0.elapsed().as_nanos() as u64;

        // If a screenshot was captured last frame, the GPU is done — read it back.
        self.screenshot_finish_readback();

        // Acquire next swapchain image.
        let (image_index, suboptimal) = unsafe {
            match self.swapchain_state.swapchain_loader.acquire_next_image(
                self.swapchain_state.swapchain,
                u64::MAX,
                self.frame_sync.image_available[frame],
                vk::Fence::null(),
            ) {
                Ok((idx, suboptimal)) => (idx, suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                Err(e) => anyhow::bail!("acquire_next_image: {:?}", e),
            }
        };

        let img = image_index as usize;

        // From here through `queue_submit` below, `image_available[frame]`
        // is signal-pending (set by the acquire above) and any `?`-
        // propagated error would leak the signal into the next acquire,
        // tripping VUID-vkAcquireNextImageKHR-semaphore-01779. Each
        // fallible call between this point and the submit recovers via
        // `recreate_image_available_for_frame` — sibling to the
        // `in_flight` fence recovery already wired through
        // `recreate_for_swapchain` (#908). See #910 / REN-D5-NEW-01.

        // If this swapchain image is still in use by a different frame, wait.
        let image_fence = self.frame_sync.images_in_flight[img];
        if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {
            unsafe {
                if let Err(e) = self
                    .device
                    .wait_for_fences(&[image_fence], true, u64::MAX)
                    .context("wait for image fence")
                {
                    let _ = self
                        .frame_sync
                        .recreate_image_available_for_frame(&self.device, frame);
                    return Err(e);
                }
            }
        }
        self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame];

        unsafe {
            if let Err(e) = self
                .device
                .reset_fences(&[self.frame_sync.in_flight[frame]])
                .context("reset_fences")
            {
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        // Deferred-destroy tick. Runs AFTER `wait_for_fences` so every
        // resource whose countdown reaches zero this frame is
        // guaranteed unreferenced by any in-flight command buffer.
        // Pre-#418 this ran at the TOP of `draw_frame`, before the
        // fence wait — `AccelerationManager::tick_deferred_destroy`
        // (and the `mesh_registry` / `texture_registry` siblings, all
        // three destroy GPU resources) could free a BLAS / buffer /
        // image the previous frame's TLAS or blit was still reading.
        // Latent because `MAX_FRAMES_IN_FLIGHT`-conservative countdowns
        // kept the window from ever closing, but a policy change that
        // shortened the countdown would have turned this into a
        // sync2-validated use-after-free.
        //
        // `texture_registry.begin_frame` advances the internal frame
        // counter that the tick compares against — must run BEFORE the
        // tick so the counter reflects "this frame" during the
        // deferred-destroy decision.
        self.texture_registry.begin_frame(&self.device, frame);
        if let Some(ref alloc) = self.allocator {
            self.mesh_registry
                .tick_deferred_destroy(&self.device, alloc);
            self.texture_registry
                .tick_deferred_destroy(&self.device, alloc);
            if let Some(ref mut accel) = self.accel_manager {
                accel.tick_deferred_destroy(&self.device, alloc);
            }
        }

        // Record command buffer. Indexed by frame-in-flight (not swapchain
        // image) so the fence and command buffer share the same slot — #259.
        // Safe because in_flight[frame] was just waited on, guaranteeing
        // the GPU has finished with this cmd buffer's previous recording.
        let cmd = self.command_buffers[frame];
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

        // 6 color attachments + depth. Order must match the render pass:
        //   0 HDR, 1 normal, 2 motion, 3 mesh_id, 4 raw_indirect, 5 albedo, 6 depth.
        let zero_f = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color,
                },
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
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        // Main framebuffer is now per-frame-in-flight (not per-swapchain-image).
        // Each frame slot has its own HDR color image, so no read-after-write
        // hazard across overlapping frames.
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[frame])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_state.extent,
            })
            .clear_values(&clear_values);

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
        // `crates/renderer/src/vulkan/acceleration.rs::build_tlas` and
        // the SSBO builder below — both must honour this map.
        let tlas_t0 = Instant::now();
        let instance_map: Vec<Option<u32>> =
            super::super::acceleration::build_instance_map(
                draw_commands.len(),
                super::super::scene_buffer::MAX_INSTANCES,
                |i| {
                    self.mesh_registry
                        .get(draw_commands[i].mesh_handle)
                        .is_some()
                },
            );
        // M29 Phase 2: TLAS build moved to AFTER bone upload + skin
        // chain (compute dispatch + BLAS refit) so the TLAS sees this
        // frame's skinned poses with zero lag. instance_map computed
        // here stays valid through the move — it's a pure function of
        // draw_commands + mesh_registry state.

        // Upload scene data (lights + camera) BEFORE the render pass begins.
        self.scene_buffers
            .upload_lights(&self.device, frame, lights)
            .unwrap_or_else(|e| log::warn!("Failed to upload lights: {e}"));
        let rt_flag =
            if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
                1.0
            } else {
                0.0
            };

        // TAA sub-pixel jitter via Halton(2,3) sequence. Each frame shifts
        // the projection by a different sub-pixel offset in NDC so that
        // temporal blending over ~8 frames reconstructs a super-sampled
        // result. The offset is applied in the vertex shader AFTER motion
        // vector computation so reprojection is jitter-free.
        let (jx, jy) = if self.taa.is_some() {
            let idx = (self.frame_counter % 8) as u32 + 1; // 1-indexed
            let hx = halton(idx, 2);
            let hy = halton(idx, 3);
            // Map [0,1] → [-0.5, 0.5] pixels, then to NDC.
            let w = self.swapchain_state.extent.width as f32;
            let h = self.swapchain_state.extent.height as f32;
            ((hx - 0.5) * 2.0 / w, (hy - 0.5) * 2.0 / h)
        } else {
            (0.0, 0.0)
        };

        let vp = view_proj;
        let pvp = &self.prev_view_proj;
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
            // w = monotonic frame counter for temporal jitter seed in shadow rays.
            position: [
                camera_pos[0],
                camera_pos[1],
                camera_pos[2],
                self.frame_counter as f32,
            ],
            flags: [
                rt_flag,
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
            ],
            screen: [
                self.swapchain_state.extent.width as f32,
                self.swapchain_state.extent.height as f32,
                fog_near,
                fog_far,
            ],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                if fog_far > fog_near { 1.0 } else { 0.0 }, // fog enabled flag
            ],
            // jitter[2] carries the debug-bypass bitmask for the
            // fragment shader (see `parse_render_debug_flags_env` and
            // `triangle.frag`'s `floatBitsToUint(jitter.z)` branches).
            // Zero-bits → free no-op; non-zero → debug paths active.
            jitter: [jx, jy, f32::from_bits(self.render_debug_flags), 0.0],
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
        };
        self.scene_buffers
            .upload_camera(&self.device, frame, &camera)
            .unwrap_or_else(|e| log::warn!("Failed to upload camera: {e}"));
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
        // Store this frame's viewProj as next frame's "previous" for motion vectors.
        self.prev_view_proj = *vp;
        if !bone_palette.is_empty() {
            self.scene_buffers
                .upload_bones(&self.device, frame, bone_palette)
                .unwrap_or_else(|e| log::warn!("Failed to upload bone palette: {e}"));
        }
        // #921 / REN-D12-NEW-04 — schedule the staging→device copy +
        // visibility barrier on the main command buffer BEFORE any
        // shader stage reads the device-side bone palette (the M29 skin
        // compute steady-state dispatch below, and binding 3 / 12 reads
        // in the raster vertex stage). Idempotent w.r.t. the per-prime
        // copies recorded inside the first-sight loop further down.
        self.scene_buffers
            .record_bone_copy(&self.device, cmd, frame);

        // ── M29 Phase 2: GPU pre-skin + per-skinned-entity BLAS refit ─
        //
        // Runs AFTER bone palette upload (compute reads it) and BEFORE
        // TLAS build (TLAS picks up the freshly-refit BLAS, zero-lag
        // RT). For each draw with `bone_offset != 0`:
        //   - First sight: synchronous compute prime + synchronous BLAS
        //     BUILD (with `ALLOW_UPDATE`) via two one-time command
        //     buffers. Brief stall on the very first frame an NPC
        //     appears; M40 cell streaming will eventually preload.
        //   - Steady state: dispatch compute into the frame cmd buffer,
        //     barrier (COMPUTE_WRITE → AS_BUILD_INPUT_READ), then
        //     refit the per-entity BLAS (UPDATE mode, src == dst).
        //     Final AS_BUILD_WRITE → AS_BUILD_INPUT_READ barrier hands
        //     fresh BLAS to TLAS below.
        //
        // Skips entirely when `skin_compute` / `accel_manager` are None
        // (no RT) or no draws are skinned.
        let skin_t0 = Instant::now();
        if let (Some(ref skin_pipeline), Some(ref mut accel)) =
            (self.skin_compute.as_ref(), self.accel_manager.as_mut())
        {
            if let Some(ref alloc) = self.allocator {
                // Sub-block: limit borrow scope on `mesh_registry` /
                // `scene_buffers`. Skin-chain reads are immutable
                // through this block.
                let global_vert_buf = self
                    .mesh_registry
                    .global_vertex_buffer
                    .as_ref()
                    .map(|b| (b.buffer, b.size));
                let bone_buffer = self
                    .scene_buffers
                    .bone_buffers()
                    .get(frame)
                    .map(|b| b.buffer);
                let bone_buffer_size = self.scene_buffers.bone_buffer_size();

                if let (Some((input_buffer, input_size)), Some(bone_buf)) =
                    (global_vert_buf, bone_buffer)
                {
                    // Walk draw_commands once — collect unique skinned
                    // entities + their per-mesh metadata. Multiple
                    // draws of the same entity (rare; instanced rendering
                    // would hit this) coalesce on entity_id.
                    use std::collections::HashSet;
                    let mut seen: HashSet<EntityId> = HashSet::new();
                    let mut dispatches: Vec<(
                        EntityId,
                        super::super::skin_compute::SkinPushConstants,
                        vk::Buffer,
                        u32,
                        u32,
                    )> = Vec::new();
                    for dc in draw_commands.iter() {
                        if dc.bone_offset == 0 {
                            continue;
                        }
                        if !seen.insert(dc.entity_id) {
                            continue;
                        }
                        let Some(mesh) = self.mesh_registry.get(dc.mesh_handle) else {
                            continue;
                        };
                        let push = super::super::skin_compute::SkinPushConstants {
                            vertex_offset: mesh.global_vertex_offset,
                            vertex_count: mesh.vertex_count,
                            bone_offset: dc.bone_offset,
                        };
                        dispatches.push((
                            dc.entity_id,
                            push,
                            mesh.index_buffer.buffer,
                            mesh.index_count,
                            mesh.vertex_count,
                        ));
                    }
                    self.last_skin_coverage_frame.dispatches_total = dispatches.len() as u32;

                    // First-sight setup: for each entity that doesn't
                    // yet have a SkinSlot OR a skinned BLAS, create
                    // the slot (CPU-only) and queue the BLAS BUILD
                    // onto the per-frame `cmd` via the batched on-cmd
                    // builder below. The steady-state compute dispatch
                    // (further down) serves as the prime for the
                    // newly-allocated slot — it writes the current
                    // pose into the slot's output buffer before the
                    // COMPUTE→AS_BUILD barrier, so the queued BUILD
                    // reads valid vertex data.
                    //
                    // #679 / AS-8-9 — also re-enter this path for
                    // entities whose BLAS has refit too many times
                    // and degraded BVH traversal quality. Drop the
                    // stale BLAS first; the partition below then
                    // sees `needs_blas = true` and queues a fresh
                    // BUILD against the next compute output. The
                    // slot's output buffer is preserved (compute
                    // keeps streaming poses through it), so only the
                    // BLAS object itself is replaced.
                    //
                    // #911 / REN-D5-NEW-02 — Pre-fix this loop paid
                    // 2 fence-waits per first-sight entity (one-time
                    // submit for compute prime + one-time submit for
                    // sync BLAS BUILD), stalling `draw_frame` by
                    // 2 × N queue waits on multi-NPC spawn frames.
                    // The on-cmd batched builder eliminates both
                    // host waits — every first-sight BUILD now
                    // submits as part of the per-frame command
                    // buffer that already carries the steady-state
                    // compute dispatch, scratch-serialise barriers,
                    // refit loop and TLAS build. Two-pass scratch
                    // sizing inside
                    // `build_skinned_blas_batched_on_cmd` keeps the
                    // shared `blas_scratch_buffer` device address
                    // stable across every recorded build in the
                    // batch (the failure mode of the naive
                    // "record N back-to-back, each inline-resizing
                    // scratch" path).
                    let mut first_sight_builds: Vec<(
                        EntityId,
                        vk::Buffer, // slot.output_buffer.buffer (BUILD input)
                        u32,        // vertex_count
                        vk::Buffer, // idx_buffer
                        u32,        // idx_count
                    )> = Vec::new();
                    for &(entity_id, _push, idx_buffer, idx_count, vertex_count) in &dispatches {
                        let needs_slot = !self.skin_slots.contains_key(&entity_id);
                        if accel.should_rebuild_skinned_blas(entity_id) {
                            log::info!(
                                "skin_compute BLAS rebuild for entity {entity_id} — \
                                 refit chain reached {} frames, dropping for fresh BUILD (#679)",
                                accel
                                    .skinned_blas_entry(entity_id)
                                    .map(|e| e.refit_count)
                                    .unwrap_or(0),
                            );
                            accel.drop_skinned_blas(entity_id);
                        }
                        let needs_blas = accel.skinned_blas_entry(entity_id).is_none();
                        if !needs_slot && !needs_blas {
                            continue;
                        }
                        // Skip retry on entities whose previous attempt
                        // failed — `failed_skin_slots` is cleared on any
                        // LRU eviction (capacity opened), so a real change
                        // in pool occupancy un-suppresses the retry
                        // naturally. Pre-#900 the failure path re-fired
                        // `create_slot` every frame and re-logged the
                        // WARN, observed at 58 WARN / 300 frames on
                        // post-M41-EQUIP Prospector. The suppression
                        // happens *before* the attempt counter so the
                        // coverage gauge reports "real attempts made this
                        // frame" rather than "entities the loop visited."
                        if needs_slot && self.failed_skin_slots.contains(&entity_id) {
                            continue;
                        }
                        self.last_skin_coverage_frame.first_sight_attempted += 1;
                        if needs_slot {
                            match skin_pipeline.create_slot(&self.device, alloc, vertex_count) {
                                Ok(slot) => {
                                    self.skin_slots.insert(entity_id, slot);
                                }
                                Err(e) => {
                                    log::warn!(
                                        "skin_compute create_slot failed for entity {entity_id}: {e} \
                                         — skinned RT shadow disabled for this entity (raster unaffected)"
                                    );
                                    self.failed_skin_slots.insert(entity_id);
                                    continue;
                                }
                            }
                        }
                        if needs_blas {
                            let Some(slot) = self.skin_slots.get(&entity_id) else {
                                continue;
                            };
                            first_sight_builds.push((
                                entity_id,
                                slot.output_buffer.buffer,
                                vertex_count,
                                idx_buffer,
                                idx_count,
                            ));
                        } else {
                            // Slot was missing but BLAS already existed —
                            // structurally impossible today (slot+BLAS are
                            // paired on insert and slot eviction also drops
                            // the BLAS). Counted as a successful first-sight
                            // pass so the coverage gauge stays sound if a
                            // future refactor decouples the pair.
                            self.last_skin_coverage_frame.first_sight_succeeded += 1;
                        }
                    }

                    // Per-frame steady-state: dispatch compute for
                    // every registered skinned slot (refresh output
                    // buffer with current pose), then barrier, then
                    // refit BLAS.
                    if !dispatches.is_empty() {
                        unsafe {
                            for &(entity_id, push, _, _, _) in &dispatches {
                                let Some(slot) = self.skin_slots.get_mut(&entity_id) else {
                                    continue;
                                };
                                // #643 / MEM-2-1 — bump LRU before the
                                // dispatch so the eviction sweep below
                                // sees this entity as "active this
                                // frame" and won't drop it. Mirrors
                                // the BLAS-side `last_used_frame` bump
                                // in `acceleration.rs::build_tlas`.
                                slot.last_used_frame = self.frame_counter as u64;
                                skin_pipeline.dispatch(
                                    &self.device,
                                    cmd,
                                    slot,
                                    frame,
                                    input_buffer,
                                    input_size,
                                    bone_buf,
                                    bone_buffer_size,
                                    push,
                                );
                            }
                            // Compute writes (skinned vertex output
                            // buffers) → AS build input reads. Covers
                            // both the first-sight BUILD batch below
                            // and the refit loop further down — both
                            // read the freshly-written output buffers
                            // as BLAS-build vertex input.
                            // COMPUTE_SHADER → ACCELERATION_STRUCTURE_BUILD_KHR
                            memory_barrier(
                                &self.device, cmd,
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::AccessFlags::SHADER_WRITE,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                            );
                            // #911 — first-sight BLAS BUILDs piggyback
                            // on the per-frame `cmd` rather than each
                            // paying a host fence-wait. The compute
                            // dispatch above served as the prime for
                            // every newly-allocated slot in
                            // `first_sight_builds`; the
                            // COMPUTE→AS_BUILD barrier just emitted
                            // hands those writes to the build inputs.
                            // The helper queries every entity's
                            // `build_scratch_size`, grows
                            // `blas_scratch_buffer` ONCE to the max
                            // demand of the batch, then records each
                            // build with an internal scratch-serialise
                            // barrier (`AS_WRITE→AS_WRITE`) between
                            // iterations so the shared scratch is
                            // safely sequenced. The first refit
                            // iteration below emits its own
                            // scratch-serialise barrier as well
                            // (#983 / REN-D8-NEW-15), covering the
                            // BUILD-batch → first-refit transition.
                            if !first_sight_builds.is_empty() {
                                let results = accel.build_skinned_blas_batched_on_cmd(
                                    &self.device,
                                    alloc,
                                    cmd,
                                    &first_sight_builds,
                                );
                                for (entity_id, result) in results {
                                    match result {
                                        Ok(()) => {
                                            self.last_skin_coverage_frame
                                                .first_sight_succeeded += 1;
                                        }
                                        Err(e) => {
                                            log::warn!(
                                                "skin_compute first-sight BLAS build failed for entity {entity_id}: {e}"
                                            );
                                        }
                                    }
                                }
                            }
                            // Each `refit_skinned_blas` call shares
                            // `blas_scratch_buffer` with every other
                            // refit in this loop AND with any BUILD
                            // that ran earlier this frame — the
                            // first-sight batch above (same `cmd`,
                            // post-#911) and any `build_blas_batched`
                            // cell-load (separate submission). Vulkan
                            // spec on `scratchData` requires an
                            // AS_WRITE → AS_WRITE serialise barrier
                            // between every pair of AS-builds that
                            // share scratch, regardless of submission
                            // boundary (the host fence-wait is a
                            // host-side dependency only and does NOT
                            // establish device-side memory ordering
                            // for the next submission). Emitting the
                            // barrier before EVERY iteration covers
                            // both refit→refit (#642), the
                            // cross-submission BUILD→first-refit case
                            // (#644 / MEM-2-2), and the same-cmd
                            // BUILD-batch→first-refit case introduced
                            // by #911 (the batched on-cmd builder
                            // leaves an AS_WRITE in flight). The
                            // redundant first-iteration barrier is
                            // essentially free when the cmd has no
                            // prior AS-build — same-stage
                            // AS_WRITE↔AS_WRITE on a queue with no
                            // in-flight build work.
                            for &(entity_id, _, idx_buffer, idx_count, vertex_count) in &dispatches
                            {
                                let Some(slot) = self.skin_slots.get(&entity_id) else {
                                    continue;
                                };
                                // Past the slot gate → coverage counts a
                                // real refit attempt. Entities without a
                                // slot land in `slots_failed` instead.
                                self.last_skin_coverage_frame.refits_attempted += 1;
                                accel.record_scratch_serialize_barrier(&self.device, cmd);
                                match accel.refit_skinned_blas(
                                    &self.device,
                                    cmd,
                                    entity_id,
                                    slot.output_buffer.buffer,
                                    vertex_count,
                                    idx_buffer,
                                    idx_count,
                                ) {
                                    Ok(()) => {
                                        self.last_skin_coverage_frame.refits_succeeded += 1;
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "skin_compute BLAS refit failed for entity {entity_id}: {e}"
                                        );
                                        continue;
                                    }
                                }
                            }
                            // BLAS refit writes → TLAS build reads.
                            // ACCELERATION_STRUCTURE_BUILD_KHR → ACCELERATION_STRUCTURE_BUILD_KHR
                            memory_barrier(
                                &self.device, cmd,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                            );
                        }
                    }

                    // #643 / MEM-2-1 — drop SkinSlots (and the matching
                    // skinned BLAS) for entities whose `last_used_frame`
                    // trails the current draw by more than
                    // `MAX_FRAMES_IN_FLIGHT` frames. Mirrors
                    // `evict_unused_blas`'s LRU pattern: the threshold
                    // guarantees no in-flight command buffer still
                    // references the descriptor sets / output buffer /
                    // BLAS, so synchronous destroy is safe — no
                    // deferred-destroy queue needed.
                    //
                    // Pre-fix the `skin_slots` HashMap and the
                    // `skinned_blas` map only ever had entries
                    // *inserted* (draw.rs first-sight loop) or *drained
                    // wholesale on Drop* (context/mod.rs). On long
                    // sessions that streamed through several
                    // worldspaces, every NPC ever rendered stayed
                    // resident; the FREE_DESCRIPTOR_SET pool would
                    // exhaust well before the player's exterior
                    // population caught up.
                    let min_idle = MAX_FRAMES_IN_FLIGHT as u64 + 1;
                    let now = self.frame_counter as u64;
                    // #1003 — drain `pending_skin_unload_victims` populated by
                    // `cell_loader::unload_cell`. These entities have been
                    // despawned; their slots and per-skinned BLAS must be
                    // released NOW (post-fence-wait, so no in-flight
                    // command buffer still references the output buffer).
                    let mut evictees: Vec<EntityId> =
                        std::mem::take(&mut self.pending_skin_unload_victims);
                    // Continue with the regular eviction filter for entries
                    // that aged out via the idle policy (the original path
                    // that protects against entity-still-alive-but-not-
                    // drawn scenarios — camera moved off-screen, etc.).
                    evictees.extend(
                        self.skin_slots.iter().filter_map(|(&eid, slot)| {
                            super::super::skin_compute::should_evict_skin_slot(
                                slot.last_used_frame,
                                now,
                                min_idle,
                            )
                            .then_some(eid)
                        }),
                    );
                    if !evictees.is_empty() {
                        log::debug!(
                            "skin_slots eviction: dropping {} idle SkinSlot(s) and matching skinned BLAS",
                            evictees.len()
                        );
                        for eid in evictees {
                            if let Some(slot) = self.skin_slots.remove(&eid) {
                                skin_pipeline.destroy_slot(&self.device, alloc, slot);
                            }
                            accel.drop_skinned_blas(eid);
                        }
                        // Capacity opened up — un-suppress retry on every
                        // entity that previously failed. Cheap (the set
                        // caps at `skinned_count - SKIN_MAX_SLOTS`, zero
                        // on healthy scenes) and correct: each cleared
                        // entry will retry once next frame; if its
                        // retry succeeds, it allocates a slot, otherwise
                        // it re-enters the cache via the failure path.
                        // See #900.
                        self.failed_skin_slots.clear();
                    }
                }
            }
        }
        let _skin_chain_ns = skin_t0.elapsed().as_nanos() as u64;

        // ── TLAS build (relocated from top of frame) ─────────────────
        // Picks up just-refit per-skinned-entity BLAS via the
        // `bone_offset != 0` override in `build_tlas`. Static draws
        // continue using the per-mesh `blas_entries` table.
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Err(e) = accel.build_tlas(
                        &self.device,
                        alloc,
                        cmd,
                        draw_commands,
                        &instance_map,
                        frame,
                    ) {
                        log::warn!("TLAS build failed: {e}");
                    } else {
                        // Memory barrier: TLAS build → ray-query consumers
                        // (FRAGMENT_SHADER for main render pass +
                        // COMPUTE_SHADER for caustic_splat.comp). See
                        // #415 for the COMPUTE_SHADER widening.
                        // AS_BUILD_KHR → FRAGMENT_SHADER|COMPUTE_SHADER
                        memory_barrier(
                            &self.device, cmd,
                            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                            vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                            vk::PipelineStageFlags::FRAGMENT_SHADER
                                | vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                        );
                        if let Some(tlas_handle) = accel.tlas_handle(frame) {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                        }
                        accel.evict_unused_blas(&self.device, alloc);
                    }
                }
            }
        }
        t.tlas_build_ns = tlas_t0.elapsed().as_nanos() as u64;

        // ── Cluster light culling (compute dispatch) ─────────────────
        //
        // Runs after light + camera uploads, before the render pass.
        // The compute shader reads lights/camera and writes cluster SSBOs
        // that the fragment shader reads during the render pass.
        unsafe {
            if let Some(ref cc) = self.cluster_cull {
                // Barrier: host writes to light/camera SSBOs must be visible
                // to the compute shader before dispatch. Required by Vulkan
                // spec even for HOST_COHERENT memory. Instance data is NOT
                // uploaded yet — it is built and uploaded after this dispatch.
                // HOST → COMPUTE_SHADER (light/camera UBO flush)
                memory_barrier(
                    &self.device, cmd,
                    vk::PipelineStageFlags::HOST,
                    vk::AccessFlags::HOST_WRITE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ,
                );

                cc.dispatch(&self.device, cmd, frame);
                // Barrier: compute writes → fragment reads on cluster SSBOs.
                // COMPUTE_SHADER → FRAGMENT_SHADER (cluster SSBO outputs)
                memory_barrier(
                    &self.device, cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::SHADER_READ,
                );
            }
        }

        // ── Build instance SSBO + draw batches ────────────────────────
        //
        // Each DrawCommand becomes one GpuInstance in the SSBO. Consecutive
        // commands with the same (pipeline_key, render_layer, mesh_handle) are
        // merged into a single instanced draw call.
        //
        // The two working vectors are held on `self` as scratch buffers
        // (`gpu_instances_scratch`, `batches_scratch`). `mem::take` moves
        // them out so the rest of draw_frame can continue borrowing other
        // fields of `self` without fighting the borrow checker; at the
        // bottom of the function they are moved back, amortizing their
        // capacity across frames. Error-path early returns lose the
        // amortization for one frame only — acceptable since the draw
        // has already failed. See issue #243.
        let ssbo_t0 = Instant::now();
        let mut gpu_instances: Vec<GpuInstance> = std::mem::take(&mut self.gpu_instances_scratch);
        gpu_instances.clear();
        gpu_instances.reserve(draw_commands.len() + 1); // +1 for optional UI quad
        let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
        batches.clear();
        batches.reserve(draw_commands.len());

        // Sort contract for draw_commands is owned by render.rs
        // `build_render_data`. The per-field cluster order is covered
        // by the unit test `render::sort_key_clusters_by_alpha_decal_twosided`
        // (#500 D3-M2). A duplicate debug_assert here drifted out of
        // sync with the real key and was removed rather than kept in
        // lockstep across two crates.
        for draw_cmd in draw_commands {
            let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else {
                continue;
            };

            let instance_idx = gpu_instances.len() as u32;
            let m = &draw_cmd.model_matrix;

            // Detect non-uniform scale from the model matrix column lengths.
            // If the 3 column vectors of the upper-3x3 have different lengths,
            // the vertex shader must use inverse-transpose for normals.
            // Otherwise it can skip the expensive inverse (~40 ALU ops).
            // Three dot products is trivial compared to the per-vertex savings.
            let col0_sq = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
            let col1_sq = m[4] * m[4] + m[5] * m[5] + m[6] * m[6];
            let col2_sq = m[8] * m[8] + m[9] * m[9] + m[10] * m[10];
            let has_non_uniform_scale = {
                let tol = 0.001;
                (col0_sq - col1_sq).abs() > tol || (col0_sq - col2_sq).abs() > tol
            };
            // Per-instance flags — see INSTANCE_FLAG_* constants in
            // scene_buffer.rs. CPU-side assembly must stay in lockstep
            // with the fragment shader's `flags & N` checks.
            //   bit 0 = non-uniform scale
            //   bit 1 = NiAlphaProperty blend bit
            //   bit 2 = caustic source — real refractive surface
            //           (#922 / REN-D13-NEW-01). Pre-fix this fired for
            //           every alpha-blend + low-metalness draw, which
            //           over-included hair, foliage, particle quads,
            //           decals and FX cards — none refractive. The
            //           caustic compute (`caustic_splat.comp`) burns
            //           `max_lights` TLAS ray queries per flagged pixel,
            //           so a foliage-heavy exterior wasted significant
            //           ray budget. Gate now matches the upstream glass
            //           classification in `render::build_render_data`
            //           (#515 / #706): engine-classified
            //           `MATERIAL_KIND_GLASS` (alpha-blend + low metal +
            //           low roughness + not a decal) OR Skyrim+
            //           `MultiLayerParallax` (kind 11) with a non-zero
            //           inner-layer refraction scale.
            //   bit 3 = terrain splat (set in cell_loader for LAND entities, #470).
            let mut flags = if has_non_uniform_scale {
                INSTANCE_FLAG_NON_UNIFORM_SCALE
            } else {
                0u32
            };
            if draw_cmd.alpha_blend {
                flags |= INSTANCE_FLAG_ALPHA_BLEND;
            }
            if is_caustic_source(draw_cmd) {
                flags |= INSTANCE_FLAG_CAUSTIC_SOURCE;
            }
            if let Some(tile_idx) = draw_cmd.terrain_tile_index {
                flags |= INSTANCE_FLAG_TERRAIN_SPLAT;
                flags |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT;
            }
            // #renderlayer — pack the 2-bit layer discriminant into
            // bits 4..5 for the fragment shader's debug-viz branch
            // (BYROREDUX_RENDER_DEBUG=0x40 tints fragments by layer).
            // No other code reads this slot today; the field exists
            // purely for empirical validation of classification.
            flags |= (draw_cmd.render_layer as u32 & INSTANCE_RENDER_LAYER_MASK)
                << INSTANCE_RENDER_LAYER_SHIFT;

            // R1 Phase 6 — `GpuInstance` carries only per-DRAW data
            // now: model + mesh refs + bone_offset + flags +
            // material_id + caustic-source avg_albedo. Every
            // per-material field reads through `materials[material_id]`
            // in the fragment shader.
            gpu_instances.push(GpuInstance {
                model: [
                    [m[0], m[1], m[2], m[3]],
                    [m[4], m[5], m[6], m[7]],
                    [m[8], m[9], m[10], m[11]],
                    [m[12], m[13], m[14], m[15]],
                ],
                texture_index: draw_cmd.texture_handle,
                bone_offset: draw_cmd.bone_offset,
                vertex_offset: mesh.global_vertex_offset,
                index_offset: mesh.global_index_offset,
                vertex_count: mesh.vertex_count,
                flags,
                material_id: draw_cmd.material_id,
                _pad_id0: 0.0,
                avg_albedo_r: draw_cmd.avg_albedo[0],
                avg_albedo_g: draw_cmd.avg_albedo[1],
                avg_albedo_b: draw_cmd.avg_albedo[2],
                _pad_albedo: 0.0,
            });

            // Frustum-culled draws still need an SSBO entry so RT hit
            // shaders that land on their TLAS instance read the right
            // material / transform (#516). Skip batch formation — they
            // have no rasterized pixels this frame. Breaking the batch
            // chain here also avoids accidentally extending a previous
            // batch across a gap in the SSBO layout (`first_instance +
            // instance_count` would point past an off-screen draw).
            //
            // Water surfaces are also skipped here: their `GpuInstance`
            // SSBO slot is populated (so the water pipeline's vertex
            // shader can read the model matrix via `gl_InstanceIndex`),
            // but they render through the dedicated water pipeline in
            // a separate pass below — not through the triangle / blend
            // pipeline batches.
            if !draw_cmd.in_raster || draw_cmd.is_water {
                continue;
            }

            // Two-sided is NOT a key axis (#930) — both opaque and
            // blended pipelines declare CULL_MODE as dynamic state, so
            // two-sided rendering uses per-draw `cmd_set_cull_mode`
            // not a separate pipeline.
            let pipeline_key = if draw_cmd.alpha_blend {
                PipelineKey::Blended {
                    src: draw_cmd.src_blend,
                    dst: draw_cmd.dst_blend,
                }
            } else {
                PipelineKey::Opaque
            };

            // Extend the current batch if this draw shares the same
            // state AND is contiguous in the SSBO (no culled draws in
            // the gap). The contiguity check is new with #516 — before
            // the in_raster split the SSBO idx always advanced 1:1
            // with the batch-eligible iterations, so contiguity was
            // implicit. Now an off-screen draw pushes an SSBO entry
            // but skips batch formation, so the next rasterized draw
            // might land at a non-contiguous `instance_idx`.
            // #renderlayer — depth bias is selected from the per-layer
            // ladder via `DrawCommand::render_layer`. `RenderLayer::Decal`
            // subsumes both the legacy `is_decal` and `needs_depth_bias`
            // bits — alpha-tested rugs / posters / fences and true
            // NIF-flagged decals all carry `render_layer == Decal` set
            // at cell-load time.
            let render_layer = draw_cmd.render_layer;

            if let Some(batch) = batches.last_mut() {
                if batch.mesh_handle == draw_cmd.mesh_handle
                    && batch.pipeline_key == pipeline_key
                    && batch.two_sided == draw_cmd.two_sided
                    && batch.render_layer == render_layer
                    && batch.z_test == draw_cmd.z_test
                    && batch.z_write == draw_cmd.z_write
                    && batch.z_function == draw_cmd.z_function
                    && batch.first_instance + batch.instance_count == instance_idx
                {
                    batch.instance_count += 1;
                    continue;
                }
            }

            // Start a new batch.
            batches.push(DrawBatch {
                mesh_handle: draw_cmd.mesh_handle,
                pipeline_key,
                two_sided: draw_cmd.two_sided,
                render_layer,
                first_instance: instance_idx,
                instance_count: 1,
                index_count: mesh.index_count,
                global_index_offset: mesh.global_index_offset,
                global_vertex_offset: mesh.global_vertex_offset as i32,
                z_test: draw_cmd.z_test,
                z_write: draw_cmd.z_write,
                z_function: draw_cmd.z_function,
            });
        }

        // Append UI instance (if needed) BEFORE the bulk upload so it's
        // included in the single flush. Avoids the need for a separate raw
        // pointer write + flush that was missing on non-coherent memory (#189).
        let ui_instance_idx =
            if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
                let idx = gpu_instances.len() as u32;
                gpu_instances.push(GpuInstance {
                    texture_index: ui_tex,
                    ..GpuInstance::default()
                });
                Some(idx)
            } else {
                None
            };

        // #647 / RP-1 — guard against `gl_InstanceIndex` outrunning
        // the `MAX_INSTANCES` SSBO allocation. Post-#992 the mesh_id
        // G-buffer is `R32_UINT` (bit 31 = ALPHA_BLEND_NO_HISTORY,
        // bits 0..30 = id + 1, ceiling 0x7FFFFFFF), and `MAX_INSTANCES`
        // is sized at `0x40000` (262144) to absorb dense Skyrim/FO4
        // city cells (~50K REFRs) with ~5× headroom. The SSBO is
        // sized to `MAX_INSTANCES`, so writes past that index would
        // overrun the GPU-side allocation. `upload_instances` clamps to
        // MAX_INSTANCES in release; we log and continue rather than
        // panicking inside an active command-buffer recording (#956 /
        // REN-D5-NEW-05 — a debug_assert! at this site leaks the
        // in-flight cmd buffer on unwind).
        if gpu_instances.len() > super::super::scene_buffer::MAX_INSTANCES {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                log::error!(
                    "RP-1: visible instance count {} exceeds MAX_INSTANCES ({}). \
                     Instances past the cap are silently dropped. \
                     Bump MAX_INSTANCES or partition draws.",
                    gpu_instances.len(),
                    super::super::scene_buffer::MAX_INSTANCES,
                );
            });
        }
        // Upload all instance data (scene + UI) to the SSBO in one flush.
        if !gpu_instances.is_empty() {
            self.scene_buffers
                .upload_instances(&self.device, frame, &gpu_instances)
                .unwrap_or_else(|e| log::warn!("Failed to upload instances: {e}"));
        }

        // R1 Phase 4 — upload the deduplicated material table. The
        // fragment shader reads `materials[instance.materialId]` for
        // migrated fields (Phase 4: roughness; Phases 5–6: the rest).
        // Empty table means no draws → no material reads, so the
        // upload is skipped harmlessly.
        if !materials.is_empty() {
            self.scene_buffers
                .upload_materials(&self.device, frame, materials)
                .unwrap_or_else(|e| log::warn!("Failed to upload materials: {e}"));
        }

        // Zero the ray budget counter so the fragment shader starts each
        // frame with a fresh allowance of Phase-3 IOR glass rays.
        self.scene_buffers
            .reset_ray_budget(&self.device, frame)
            .unwrap_or_else(|e| log::warn!("Failed to reset ray budget: {e}"));

        // Reupload the terrain tile SSBO when cell load mutated it.
        // The slab is static until the next cell transition — #497
        // moved it to a single DEVICE_LOCAL buffer uploaded via a
        // transient staging copy, so one upload per dirty transition
        // is enough. The scratch Vec lives on self so its 32 KB
        // capacity amortizes across cell loads — `mem::take` moves it
        // out so the fill can run while `&self.scene_buffers` consumes
        // the slice. #496.
        let mut tile_scratch: Vec<GpuTerrainTile> = std::mem::take(&mut self.terrain_tile_scratch);
        if self.fill_terrain_tile_scratch_if_dirty(&mut tile_scratch) {
            let allocator = self.allocator.as_ref().expect("allocator missing");
            self.scene_buffers
                .upload_terrain_tiles(
                    &self.device,
                    allocator,
                    &self.graphics_queue,
                    self.transfer_pool,
                    &tile_scratch,
                )
                .unwrap_or_else(|e| log::warn!("Failed to upload terrain tiles: {e}"));
        }
        self.terrain_tile_scratch = tile_scratch;

        // Build + upload indirect-draw commands for this frame (#309).
        // One `VkDrawIndexedIndirectCommand` per DrawBatch, laid out in
        // the same order as `batches` so the draw loop can reference a
        // contiguous range of the buffer for each pipeline group.
        // Populated regardless of `device_caps.multi_draw_indirect_supported`
        // — the upload is ~N × 20 B for small N, and this keeps the
        // indirect path always ready when it is enabled.
        if !batches.is_empty() && self.device_caps.multi_draw_indirect_supported {
            let indirect_scratch = &mut self.indirect_draws_scratch;
            indirect_scratch.clear();
            indirect_scratch.extend(batches.iter().map(|b| vk::DrawIndexedIndirectCommand {
                index_count: b.index_count,
                instance_count: b.instance_count,
                first_index: b.global_index_offset,
                vertex_offset: b.global_vertex_offset,
                first_instance: b.first_instance,
            }));
            self.scene_buffers
                .upload_indirect_draws(&self.device, frame, indirect_scratch)
                .unwrap_or_else(|e| log::warn!("Failed to upload indirect draws: {e}"));
        }
        t.ssbo_build_ns = ssbo_t0.elapsed().as_nanos() as u64;

        // Pre-populate the blend pipeline cache for any new (src, dst)
        // combos this frame. Resolved up-front because the hot draw
        // loop only takes `&self.device` for `cmd_bind_pipeline` and
        // can't reborrow `&mut self` to lazy-create. After this loop
        // every `PipelineKey::Blended` has a corresponding cache entry.
        // See #392 / #930 (two-sided dropped from key).
        for batch in &batches {
            if let PipelineKey::Blended { src, dst } = batch.pipeline_key {
                if !self.blend_pipeline_cache.contains_key(&(src, dst)) {
                    if let Err(e) = self.get_or_create_blend_pipeline(src, dst) {
                        log::error!(
                            "Failed to create blend pipeline (src={src}, dst={dst}): {e}; \
                             draws using this combo will fall back to opaque pipeline"
                        );
                    }
                }
            }
        }

        // Upload composite params (fog + sky) up-front so the bulk host
        // barrier below covers this UBO's HOST_WRITE too (#909 /
        // REN-D1-NEW-03). All inputs are available from `draw_frame`'s
        // parameters; the composite pass itself runs much later, after
        // the render pass + SVGF / TAA / SSAO / Bloom, but the barrier
        // doesn't care when the consumer runs as long as it's been
        // emitted before the consumer.
        if let Some(ref mut composite) = self.composite {
            let composite_params = super::super::composite::CompositeParams {
                fog_color: [
                    fog_color[0],
                    fog_color[1],
                    fog_color[2],
                    if fog_far > fog_near { 1.0 } else { 0.0 },
                ],
                // #865 / FNV-D3-NEW-06 — pack XCLL cubic-fog curve
                // into z/w. Composite uses the curve formula
                // `pow(dist / fog_clip, fog_power)` when both are
                // > 0; else falls through to the linear
                // `fog_near..fog_far` ramp.
                fog_params: [fog_near, fog_far, fog_clip, fog_power],
                depth_params: [
                    if sky_params.is_exterior { 1.0 } else { 0.0 },
                    0.85, // exposure — default Bethesda-era HDR target; promote to WTHR field (#743)
                    // #1013 — host-side mirror of the volumetric-output
                    // gate. Composite reads this slot to decide whether
                    // to consume `vol.a` (transmittance) and `vol.rgb`
                    // (in-scattering). Pinned to the host const so a
                    // future flip of `VOLUMETRIC_OUTPUT_CONSUMED` is a
                    // single-line change.
                    if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                        1.0
                    } else {
                        0.0
                    },
                    0.0,
                ],
                sky_zenith: [
                    sky_params.zenith_color[0],
                    sky_params.zenith_color[1],
                    sky_params.zenith_color[2],
                    sky_params.sun_size,
                ],
                sky_horizon: [
                    sky_params.horizon_color[0],
                    sky_params.horizon_color[1],
                    sky_params.horizon_color[2],
                    0.0,
                ],
                // #541 — WTHR `SKY_LOWER` group. Pre-fix the
                // shader faked this as `sky_horizon * 0.3`,
                // dropping the authored colour entirely.
                sky_lower: [
                    sky_params.lower_color[0],
                    sky_params.lower_color[1],
                    sky_params.lower_color[2],
                    0.0,
                ],
                sun_dir: [
                    sky_params.sun_direction[0],
                    sky_params.sun_direction[1],
                    sky_params.sun_direction[2],
                    sky_params.sun_intensity,
                ],
                sun_color: [
                    sky_params.sun_color[0],
                    sky_params.sun_color[1],
                    sky_params.sun_color[2],
                    // #478 — pack the CLMT FNAM sun sprite handle
                    // into the previously-unused w slot via
                    // `from_bits`. The shader reinterprets with
                    // `floatBitsToUint`; `0` keeps the procedural
                    // disc (pre-fix behaviour).
                    f32::from_bits(sky_params.sun_texture_index),
                ],
                cloud_params: [
                    sky_params.cloud_scroll[0],
                    sky_params.cloud_scroll[1],
                    sky_params.cloud_tile_scale,
                    f32::from_bits(sky_params.cloud_texture_index),
                ],
                cloud_params_1: [
                    sky_params.cloud_scroll_1[0],
                    sky_params.cloud_scroll_1[1],
                    sky_params.cloud_tile_scale_1,
                    f32::from_bits(sky_params.cloud_texture_index_1),
                ],
                cloud_params_2: [
                    sky_params.cloud_scroll_2[0],
                    sky_params.cloud_scroll_2[1],
                    sky_params.cloud_tile_scale_2,
                    f32::from_bits(sky_params.cloud_texture_index_2),
                ],
                cloud_params_3: [
                    sky_params.cloud_scroll_3[0],
                    sky_params.cloud_scroll_3[1],
                    sky_params.cloud_tile_scale_3,
                    f32::from_bits(sky_params.cloud_texture_index_3),
                ],
                // #428 — composite-pass fog needs the camera origin to
                // compute per-pixel world-space distance from a depth
                // sample. `w` is unused padding.
                camera_pos: [camera_pos[0], camera_pos[1], camera_pos[2], 0.0],
                inv_view_proj: inv_vp_arr,
                underwater,
            };
            if let Err(e) = composite.upload_params(&self.device, frame, &composite_params) {
                log::warn!("composite upload_params failed: {e}");
            }
        }

        // SVGF temporal params UBO — uploaded BEFORE the bulk barrier
        // below so its HOST_WRITE → UNIFORM_READ at COMPUTE_SHADER fold
        // into the same execution dependency the bulk barrier already
        // emits for composite. Mirrors the composite-UBO fold from
        // #909 / REN-D1-NEW-03. See #961 / REN-D10-NEW-04. The α state
        // machine is host-side and depends on `svgf_recovery_frames`
        // (advanced at end-of-tick); it does NOT depend on anything
        // produced by the render pass below.
        if !self.svgf_failed {
            if let Some(ref mut svgf) = self.svgf {
                let (alpha_color, alpha_moments, next_frames) =
                    crate::vulkan::svgf::next_svgf_temporal_alpha(self.svgf_recovery_frames);
                self.svgf_recovery_frames = next_frames;
                if let Err(e) = unsafe {
                    svgf.upload_params(&self.device, frame, alpha_color, alpha_moments)
                } {
                    log::warn!("svgf upload_params failed: {e}");
                }
            }
        }

        // Barrier: make the instance SSBO host write (and any remaining
        // light/camera/bone host writes) visible to the vertex + fragment
        // shaders in the upcoming render pass. Also covers the composite
        // UBO host write (uploaded just above) — the composite fragment
        // shader's `UNIFORM_READ` at `FRAGMENT_SHADER` stage is already
        // in this barrier's destination mask, so the same execution
        // dependency carries through (#909 / REN-D1-NEW-03 fold). The
        // COMPUTE_SHADER stage was added to fold the SVGF UBO host
        // barrier into this same execution dependency (#961 /
        // REN-D10-NEW-04); the SVGF dispatch reads `param_buffers[frame]`
        // at COMPUTE_SHADER stage and the host write above completes
        // before this barrier so the same fold pattern applies. Other
        // post-render-pass compute consumers (TAA / caustic / volumetrics
        // / SSAO / bloom) still emit their own per-dispatch HOST→COMPUTE
        // barriers — bundling them into this barrier is the same fold
        // and is tracked as the #961 sibling sweep. Required by Vulkan
        // spec even for HOST_COHERENT memory.
        // HOST → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT (instance SSBO + UBOs)
        unsafe {
            memory_barrier(
                &self.device, cmd,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_WRITE,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::DRAW_INDIRECT,
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::UNIFORM_READ
                    | vk::AccessFlags::INDIRECT_COMMAND_READ,
            );
        }

        let cmd_t0 = Instant::now();
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &render_pass_begin, vk::SubpassContents::INLINE);

            // No unconditional pipeline bind here — the batch loop below
            // initializes `last_pipeline_key` to a sentinel Blended value
            // so the first real batch always rebinds to its own pipeline,
            // and the UI overlay rebinds `pipeline_ui` regardless. An
            // opaque bind at this point would always be discarded. #507.

            // Dynamic viewport + scissor.
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.swapchain_state.extent.width as f32,
                height: self.swapchain_state.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.device.cmd_set_viewport(cmd, 0, &viewports);

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.swapchain_state.extent,
            }];
            self.device.cmd_set_scissor(cmd, 0, &scissors);

            // Bind the bindless texture descriptor set (set 0) — once per frame.
            let texture_set = self.texture_registry.descriptor_set(frame);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[texture_set],
                &[],
            );

            // Bind the scene descriptor set (set 1) — once per frame.
            let scene_set = self.scene_buffers.descriptor_set(frame);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                1,
                &[scene_set],
                &[],
            );

            // ── Draw loop ─────────────────────────────────────────────
            //
            // Two paths depending on what the device supports:
            //
            // 1. **Multi-draw indirect** (#309) — when the device
            //    exposes `multiDrawIndirect` (universally supported on
            //    desktop Vulkan 1.0+) and the global VB/IB is bound,
            //    we group consecutive batches sharing
            //    `(pipeline_key, render_layer)` into one
            //    `cmd_draw_indexed_indirect` call reading N
            //    `VkDrawIndexedIndirectCommand` entries from the
            //    per-frame indirect buffer. Pipeline / depth-bias
            //    state transitions still split groups (necessary —
            //    dynamic state changes between draws).
            //
            // 2. **Per-batch fallback** — used when the device doesn't
            //    expose `multiDrawIndirect` or when the global VB/IB
            //    isn't bound (e.g. the spinning-cube demo before the
            //    scene SSBO is built). One `cmd_draw_indexed` per
            //    batch, same behavior as pre-#309.
            //
            // The indirect buffer has already been filled + flushed
            // above when `gpu_instances.upload_instances(...)` ran —
            // see the `indirect_draws` build-up where each batch
            // pushes one `VkDrawIndexedIndirectCommand` entry.
            let mut last_pipeline_key = PipelineKey::Blended {
                src: u8::MAX,
                dst: u8::MAX,
            };
            // `Option` so the first batch always emits an explicit
            // `cmd_set_depth_bias` rather than relying on the
            // pipeline-default-zero matching the bias of the first
            // batch's layer (brittle when the first batch is, say, a
            // decal).
            let mut last_render_layer: Option<byroredux_core::ecs::components::RenderLayer> = None;
            // #398 — extended dynamic depth state. Vulkan requires the
            // dynamic state to be set BEFORE any draw call when the
            // pipeline declares the corresponding `vk::DynamicState`.
            // Initialise with the Gamebryo runtime defaults so the
            // first batch's "did this change?" check sees a sensible
            // baseline. Sentinel `last_z_function = u8::MAX` forces an
            // explicit set on the first batch regardless of value.
            let mut last_z_test = true;
            let mut last_z_write = true;
            let mut last_z_function: u8 = u8::MAX;
            // CULL_MODE is declared dynamic on EVERY draw-loop pipeline
            // (see `pipeline.rs::dynamic_states` for both the opaque and
            // blend variants — the "must be dynamic on every pipeline"
            // invariant lives there with full justification). The
            // helper below fires `cmd_set_cull_mode` only when the
            // tracked last value disagrees with the desired one.
            //
            // `Option<…>` with `None` sentinel (#912 / REN-D5-NEW-03):
            // the first batch's `set_cull` fires unconditionally
            // (None != Some(any)), so the pre-#912 unconditional
            // `cmd_set_cull_mode(BACK)` before the draw loop is no
            // longer needed. That pre-emit was wasted whenever the
            // first batch wanted NONE (two-sided vegetation/foliage
            // on exterior cells) — it issued BACK and then the
            // per-batch helper immediately overrode it with NONE.
            let mut last_cull_mode: Option<vk::CullModeFlags> = None;
            // #664 — per-mesh-fallback VB/IB bind cache. Only consulted
            // on the `global_bound == false` path (early-startup or any
            // future failure mode). The two-sided alpha-blend split at
            // line ~1442 calls `dispatch_direct` twice for the same
            // batch, so without this cache the per-mesh fallback issued
            // two redundant binds per split batch. `u32::MAX` is the
            // never-bound sentinel — `MeshHandle` is `u32` and 0 is a
            // valid handle.
            let mut last_bound_mesh_handle: u32 = u32::MAX;

            // Pre-loop depth state initialization — only the two fields whose
            // per-batch trackers use a real sentinel (not a "force-first" value):
            //
            //   depth_test/write: `last_z_test = true`, `last_z_write = true`.
            //   When the first batch also wants true, the per-batch check skips
            //   (`true != true` is false) — without this pre-loop set, those
            //   dynamic states would never fire on a pure-opaque-first frame.
            //
            //   depth_bias and depth_compare_op are NOT pre-set here:
            //   - depth_bias: `last_render_layer = None` ⇒ the per-batch
            //     `set_cull_and_bias` helper fires unconditionally on the first
            //     batch, covering the Vulkan "must be set before first draw"
            //     requirement. The pre-set was pure waste (#955 / REN-D5-NEW-04).
            //   - depth_compare_op: `last_z_function = u8::MAX` ⇒ the first batch
            //     always fires `cmd_set_depth_compare_op` since u8::MAX matches no
            //     real Gamebryo compare op (#955). Mirrors `#912` / REN-D5-NEW-03
            //     which removed the redundant pre-set for `cmd_set_cull_mode`.
            self.device.cmd_set_depth_test_enable(cmd, true);
            self.device.cmd_set_depth_write_enable(cmd, true);
            // #912 / REN-D5-NEW-03 — pre-#912 this issued
            // `cmd_set_cull_mode(BACK)` unconditionally. The per-batch
            // `set_cull` helper now covers the "must be set before
            // first draw" Vulkan requirement: the first batch's call
            // fires (`last_cull_mode == None`) and the helper updates
            // the tracking. Removing the unconditional set saves one
            // wasted state change per frame whenever the first batch
            // wants NONE (exterior cells often start with two-sided
            // vegetation / foliage).

            // Bind the global geometry buffer once for all scene draws.
            // Each batch uses global_index_offset / global_vertex_offset
            // to index into this single buffer, eliminating per-mesh
            // vertex/index buffer rebinding (~200 rebinds/frame → 1). #294.
            let global_bound = if let (Some(gvb), Some(gib)) = (
                self.mesh_registry.global_vertex_buffer.as_ref(),
                self.mesh_registry.global_index_buffer.as_ref(),
            ) {
                self.device
                    .cmd_bind_vertex_buffers(cmd, 0, &[gvb.buffer], &[0]);
                self.device
                    .cmd_bind_index_buffer(cmd, gib.buffer, 0, vk::IndexType::UINT32);
                true
            } else {
                false
            };

            let use_indirect = global_bound && self.device_caps.multi_draw_indirect_supported;
            let indirect_buffer = self.scene_buffers.indirect_buffer(frame);
            let indirect_stride = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32;

            // Precompute indirect-buffer state for batch `i`. Returns
            // `(pipe, render_layer)` — consecutive batches sharing the
            // tuple form one indirect group. `render_layer` covers the
            // depth-bias state-change boundary that pre-#renderlayer
            // was split between `is_decal` and `needs_depth_bias` —
            // the per-layer ladder makes this a single key slot.
            let batch_state = |b: &DrawBatch| (b.pipeline_key, b.render_layer);

            let mut i = 0;
            while i < batches.len() {
                let batch = &batches[i];

                // Switch pipeline when rendering mode changes.
                // Two-sided rendering uses dynamic `cmd_set_cull_mode`
                // (issued elsewhere in the draw loop based on
                // `draw_cmd.two_sided`), not a separate pipeline (#930).
                if batch.pipeline_key != last_pipeline_key {
                    let pipe = match batch.pipeline_key {
                        PipelineKey::Opaque => self.pipeline,
                        PipelineKey::Blended { src, dst } => {
                            // Always present after the pre-population
                            // pass above. If creation failed earlier we
                            // fall back to the opaque pipeline rather
                            // than skipping the draw entirely — better
                            // a wrong-blend visible mesh than a vanished
                            // one. See #392.
                            *self
                                .blend_pipeline_cache
                                .get(&(src, dst))
                                .unwrap_or(&self.pipeline)
                        }
                    };
                    self.device
                        .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe);
                    last_pipeline_key = batch.pipeline_key;
                }

                // #renderlayer — per-layer depth bias from
                // `RenderLayer::depth_bias()`. The Vulkan formula is
                //   bias = constant_factor × r + slope_factor × |max_dz/dxy|
                // where `r` is the smallest representable depth at the
                // fragment (≈ 2⁻²⁴ ≈ 6e-8 for D32_SFLOAT around mid-
                // depth). The `Decal` anchor (-64, -2) lifts coplanar
                // overlays into the ~4e-6 normalised-depth range
                // (Bethesda D3D scale for decal polygon offset);
                // `Architecture` is zero (the surfaces other layers
                // sit on top of); `Clutter` and `Actor` are
                // intermediate. Per-layer table is the single source
                // of truth — modifying it does NOT require touching
                // this site.
                if last_render_layer != Some(batch.render_layer) {
                    let (bias_const, clamp, bias_slope) = batch.render_layer.depth_bias();
                    self.device
                        .cmd_set_depth_bias(cmd, bias_const, clamp, bias_slope);
                    last_render_layer = Some(batch.render_layer);
                }

                // #398 — extended dynamic depth state. Emit only on
                // change so consecutive batches sharing depth state pay
                // zero state-change cost. Sky domes / viewmodels / glow
                // halos that author `z_write=0` now actually skip the
                // depth write instead of z-fighting world geometry.
                if batch.z_test != last_z_test {
                    self.device.cmd_set_depth_test_enable(cmd, batch.z_test);
                    last_z_test = batch.z_test;
                }
                if batch.z_write != last_z_write {
                    self.device.cmd_set_depth_write_enable(cmd, batch.z_write);
                    last_z_write = batch.z_write;
                }
                if batch.z_function != last_z_function {
                    self.device
                        .cmd_set_depth_compare_op(cmd, gamebryo_to_vk_compare_op(batch.z_function));
                    last_z_function = batch.z_function;
                }

                // Classify the batch's cull-mode requirement.
                //
                // Every pipeline declares CULL_MODE as dynamic (so the
                // state persists across pipeline transitions — per
                // Vulkan spec a bind to a pipeline without the dynamic
                // state would invalidate prior cmd_set_cull_mode), so
                // we must emit the target cull per-batch even for
                // opaque draws. The per-batch cost is a single u32
                // host command.
                //
                // Two-sided alpha-blend batches are rendered in two
                // passes — FRONT cull first (draws back faces, which
                // write depth), then BACK cull (draws front faces,
                // which blend on top). Without the split, a single
                // CULL_NONE draw would put front and back triangles in
                // arbitrary index order; TAA subpixel jitter then
                // flips the depth winner per frame, producing
                // cross-hatch moiré on glass. See Phase 1 of Tier C
                // glass plan + `docs/issues/glass-investigation/`.
                let is_blend = matches!(batch.pipeline_key, PipelineKey::Blended { .. });
                let two_sided = batch.two_sided;
                let needs_split = is_blend && two_sided;
                // Opaque & single-sided-blend cull target — used by
                // every branch below except the split two-sided blend.
                let default_cull = if two_sided {
                    vk::CullModeFlags::NONE
                } else {
                    vk::CullModeFlags::BACK
                };

                let set_cull = |target: vk::CullModeFlags, last: &mut Option<vk::CullModeFlags>| {
                    if *last != Some(target) {
                        self.device.cmd_set_cull_mode(cmd, target);
                        *last = Some(target);
                    }
                };

                // Dispatch helper — one direct draw of `batch`. Factored
                // so we can call it twice for the two-sided alpha-blend
                // split without duplicating the global-bound / per-mesh
                // fallback paths.
                //
                // #664 — `last_bound` threads through so the per-mesh
                // fallback elides VB/IB rebinds when consecutive
                // dispatches share `mesh_handle` (the two-sided
                // alpha-blend split is the dominant case).
                let dispatch_direct = |this: &Self, last_bound: &mut u32| {
                    if global_bound {
                        this.device.cmd_draw_indexed(
                            cmd,
                            batch.index_count,
                            batch.instance_count,
                            batch.global_index_offset,
                            batch.global_vertex_offset,
                            batch.first_instance,
                        );
                    } else {
                        if batch.mesh_handle != *last_bound {
                            if let Some(mesh) = this.mesh_registry.get(batch.mesh_handle) {
                                this.device.cmd_bind_vertex_buffers(
                                    cmd,
                                    0,
                                    &[mesh.vertex_buffer.buffer],
                                    &[0],
                                );
                                this.device.cmd_bind_index_buffer(
                                    cmd,
                                    mesh.index_buffer.buffer,
                                    0,
                                    vk::IndexType::UINT32,
                                );
                                *last_bound = batch.mesh_handle;
                            }
                        }
                        this.device.cmd_draw_indexed(
                            cmd,
                            batch.index_count,
                            batch.instance_count,
                            0,
                            0,
                            batch.first_instance,
                        );
                    }
                };

                if needs_split {
                    // Two-sided alpha-blend: back faces first, then
                    // front faces. Fall out of indirect grouping —
                    // two-sided blend batches must draw each mesh
                    // back+front adjacently, which
                    // `cmd_draw_indexed_indirect` over a group can't
                    // express without interleaving meshes.
                    set_cull(vk::CullModeFlags::FRONT, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    set_cull(vk::CullModeFlags::BACK, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    i += 1;
                } else if use_indirect {
                    set_cull(default_cull, &mut last_cull_mode);
                    // Gather consecutive batches that share the current
                    // `(pipeline_key, render_layer)` tuple — each one is
                    // already represented in the indirect buffer as one
                    // VkDrawIndexedIndirectCommand. A single
                    // `cmd_draw_indexed_indirect` call dispatches all N.
                    //
                    // Two-sided blend batches are excluded above and
                    // can't reach this branch, so grouping is safe.
                    let key = batch_state(batch);
                    let mut end = i + 1;
                    while end < batches.len()
                        && batch_state(&batches[end]) == key
                        && !(matches!(batches[end].pipeline_key, PipelineKey::Blended { .. })
                            && batches[end].two_sided)
                    {
                        end += 1;
                    }
                    let group_size = (end - i) as u32;
                    let byte_offset = (i * indirect_stride as usize) as vk::DeviceSize;
                    self.device.cmd_draw_indexed_indirect(
                        cmd,
                        indirect_buffer,
                        byte_offset,
                        group_size,
                        indirect_stride,
                    );
                    i = end;
                } else {
                    // Direct-draw fallback: global VB/IB bound or
                    // per-mesh fallback inside `dispatch_direct`.
                    set_cull(default_cull, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    i += 1;
                }
            }

            // ── Water surfaces ────────────────────────────────────────
            //
            // After all opaque + alpha-blend triangle batches have
            // submitted but before the UI overlay, render every
            // `WaterPlane` ECS entity through the dedicated water
            // pipeline. Each `WaterDrawCommand` carries its own push
            // constants (material + flow + time); the bound set 0 +
            // set 1 from the triangle path stay compatible because
            // the water pipeline layout uses the same set layouts.
            //
            // State note: the last opaque/blend pipeline already left
            // depth-test on and depth-write off (blend pipelines
            // disable depth-write). We still re-issue the dynamic
            // state defensively — if a frame somehow has only opaque
            // geometry preceding the water, depth-write would be ON
            // and water would corrupt the depth buffer.
            //
            // Cull mode: water pipeline declares it STATIC (NONE), so
            // no `cmd_set_cull_mode` is needed when binding it. The
            // subsequent UI pipeline also has cull static, so the
            // mismatch doesn't leak to it.
            if !water_commands.is_empty() {
                // #1026 / F-WAT-05 — pin the no-resort contract right
                // before consuming `wc.instance_index`. The app's
                // render code records the position into `draw_commands`
                // at emit time; any future re-sort between that emit
                // and this consumer would silently desync the recorded
                // index from the actual SSBO slot. The assertion
                // compiles out in release builds (the forward-compat
                // trap is documented next to the sort site in
                // `byroredux/src/render.rs`).
                debug_assert!(
                    super::super::water::water_commands_match_draw_slots(
                        water_commands,
                        draw_commands,
                    ),
                    "WaterDrawCommand instance_index desynced from draw_commands — \
                     was draw_commands re-sorted after the water emit? See #1026 / F-WAT-05.",
                );
                if let Some(ref water) = self.water {
                    self.device.cmd_set_depth_test_enable(cmd, true);
                    self.device.cmd_set_depth_write_enable(cmd, false);
                    self.device
                        .cmd_set_depth_compare_op(cmd, vk::CompareOp::LESS_OR_EQUAL);
                    for wc in water_commands {
                        if let Some(mesh) = self.mesh_registry.get(wc.mesh_handle) {
                            self.device.cmd_bind_vertex_buffers(
                                cmd,
                                0,
                                &[mesh.vertex_buffer.buffer],
                                &[0],
                            );
                            self.device.cmd_bind_index_buffer(
                                cmd,
                                mesh.index_buffer.buffer,
                                0,
                                vk::IndexType::UINT32,
                            );
                            water.record_draw(
                                &self.device,
                                cmd,
                                &wc.push,
                                mesh.index_count,
                                wc.instance_index,
                            );
                        }
                    }
                }
            }

            // UI overlay: draw a fullscreen quad with the Ruffle-rendered texture.
            // The UI instance was appended to gpu_instances before the bulk upload,
            // so it's already in the SSBO with a proper flush.
            //
            // CONTRACT (#663). Defensive `cmd_set_*` calls below cover
            // every state in `UI_PIPELINE_DYNAMIC_STATES` so the UI
            // overlay is decoupled from whatever dynamic-state values
            // the last main-batch pipeline left set. Depth / cull /
            // depth-bias state on `pipeline_ui` is STATIC and applied
            // by the pipeline bind itself — no `cmd_set_*` is legal
            // for those (validation would reject it). If you grow
            // `UI_PIPELINE_DYNAMIC_STATES`, the const assertion below
            // fires and you must add the matching `cmd_set_*` here
            // before the draw.
            if let (Some(idx), Some(ui_quad)) = (ui_instance_idx, self.ui_quad_handle) {
                if let Some(mesh) = self.mesh_registry.get(ui_quad) {
                    use super::super::pipeline::UI_PIPELINE_DYNAMIC_STATES;
                    const _UI_OVERLAY_DEFENSIVE_STATE_INVARIANT: () = {
                        // Update the explicit cmd_set_* calls below to cover
                        // every state in this list when the count changes.
                        assert!(
                            UI_PIPELINE_DYNAMIC_STATES.len() == 2,
                            "UI overlay path covers VIEWPORT + SCISSOR only — \
                             extend it before growing UI_PIPELINE_DYNAMIC_STATES",
                        );
                    };
                    self.device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.pipeline_ui,
                    );
                    // Defensive re-set of dynamic viewport/scissor after the
                    // UI pipeline bind (#133). The opaque/blend pipelines
                    // all declare both as VK_DYNAMIC_STATE, so the state set
                    // at the start of the render pass is inherited —
                    // today. A future UI variant that rendered at a
                    // different extent (e.g. scaled Scaleform overlay on
                    // a non-native resolution) would silently use the
                    // inherited values. Cheap two-command insurance.
                    //
                    // REN-D5-NEW-04 (audit 2026-05-09) flagged this as
                    // "redundant" because the values match the
                    // inherited ones every frame today. Keeping the
                    // re-set is intentional — the alternative is to
                    // gate it on "does this UI variant change extent"
                    // which moves a one-liner of pre-bind state into
                    // a per-variant capability check, more code than
                    // the two `cmd_set_*` calls cost. The audit
                    // recommendation is acknowledged + declined.
                    let viewports = [vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: self.swapchain_state.extent.width as f32,
                        height: self.swapchain_state.extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }];
                    self.device.cmd_set_viewport(cmd, 0, &viewports);
                    let scissors = [vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.swapchain_state.extent,
                    }];
                    self.device.cmd_set_scissor(cmd, 0, &scissors);
                    self.device
                        .cmd_bind_vertex_buffers(cmd, 0, &[mesh.vertex_buffer.buffer], &[0]);
                    self.device.cmd_bind_index_buffer(
                        cmd,
                        mesh.index_buffer.buffer,
                        0,
                        vk::IndexType::UINT32,
                    );
                    self.device
                        .cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, idx);
                }
            }

            self.device.cmd_end_render_pass(cmd);

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
                if let Some(ref mut svgf) = self.svgf {
                    // #674 temporal α state machine + UBO host write
                    // both ran BEFORE the bulk pre-render barrier
                    // above (#961 / REN-D10-NEW-04 fold). This call
                    // only records the SVGF compute dispatch.
                    if let Err(e) = svgf.dispatch(&self.device, cmd, frame) {
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
                        if let Err(e) = caustic.dispatch(&self.device, cmd, frame) {
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
            // (#1022 / REN-D18-008). When the cell isn't exterior (interior
            // gate `!is_exterior`) or the sun is below the horizon
            // (`sun_intensity <= 0`), `sun_color` is zeroed so the
            // volumetric inject path produces no contribution — without
            // this, interior cells would otherwise inject daylight
            // god-rays through walls the moment #928 flips
            // VOLUMETRIC_OUTPUT_CONSUMED on. Direction is still plumbed
            // (rather than left at a hardcoded default) so the UBO
            // field is meaningful when a future debug toggle samples it.
            if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                if let Some(ref mut vol) = self.volumetrics {
                    let vol_tlas = self
                        .accel_manager
                        .as_ref()
                        .and_then(|accel| accel.tlas_handle(frame));
                    if let Some(tlas) = vol_tlas {
                        vol.write_tlas(&self.device, frame, tlas);
                        let sun_radiance =
                            if sky_params.is_exterior && sky_params.sun_intensity > 0.0 {
                                [
                                    sky_params.sun_color[0] * sky_params.sun_intensity,
                                    sky_params.sun_color[1] * sky_params.sun_intensity,
                                    sky_params.sun_color[2] * sky_params.sun_intensity,
                                    0.0,
                                ]
                            } else {
                                [0.0, 0.0, 0.0, 0.0]
                            };
                        // Zero scattering (and therefore extinction, since
                        // single-scattering-albedo = 1) for interior cells so
                        // the volumetric integration is a no-op: T_cum = exp(0)
                        // = 1, and composite `scene * vol.a + vol.rgb` becomes
                        // `scene * 1 + 0 = scene`. Without this gate, interior
                        // cells darkened by ~63% because extinction accumulated
                        // over 128 slices at 0.005/m with no inscatter to
                        // compensate (#1082 / REN-D18-002).
                        let scatter_coef = if sky_params.is_exterior {
                            super::super::volumetrics::DEFAULT_SCATTERING_COEF
                        } else {
                            0.0
                        };
                        let vol_params = super::super::volumetrics::VolumetricsParams {
                            inv_view_proj: inv_vp_arr,
                            camera_pos: [
                                camera_pos[0],
                                camera_pos[1],
                                camera_pos[2],
                                scatter_coef,
                            ],
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
                        };
                        if let Err(e) = vol.dispatch(&self.device, cmd, frame, &vol_params) {
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
                    if let Err(e) = taa.dispatch(&self.device, cmd, frame) {
                        log::error!(
                            "TAA dispatch failed — falling back to raw HDR for the rest of the session: {e}"
                        );
                        self.taa_failed = true;
                        if let Some(ref mut composite) = self.composite {
                            composite.fall_back_to_raw_hdr(&self.device);
                        }
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
                if let Err(e) =
                    ssao.dispatch(&self.device, cmd, frame, &vp_arr, &inv_vp_arr, camera_pos)
                {
                    log::warn!("SSAO dispatch failed: {e}");
                }
            }

            // Bloom pyramid (M58). Reads the un-TAA'd scene HDR
            // (composite.hdr_image_views[frame]) and writes a
            // multi-scale blurred bright-content texture. Composite
            // adds bloom to `combined` before the ACES tone-map.
            // The render pass's final_layout already moved HDR to
            // SHADER_READ_ONLY_OPTIMAL, so the input is sample-ready.
            // Bloom uses TAA-jittered input but the blur pyramid
            // suppresses sub-pixel jitter — visually equivalent to
            // bloom on TAA output but with simpler wiring.
            if let Some(ref mut bloom) = self.bloom {
                if let Some(ref composite) = self.composite {
                    let hdr_view = composite.hdr_image_views[frame];
                    if let Err(e) = bloom.dispatch(&self.device, cmd, frame, hdr_view) {
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
                composite.dispatch(&self.device, cmd, frame, img, bindless_set);
            }

            // Screenshot capture: copy swapchain image to staging buffer
            // if requested. Must happen after composite (image has content)
            // and before end_command_buffer (still recording).
            let swapchain_image = self.swapchain_state.images[img];
            self.screenshot_record_copy(cmd, swapchain_image);

            if let Err(e) = self
                .device
                .end_command_buffer(cmd)
                .context("end_command_buffer")
            {
                // Drop out of the inner `unsafe { ... }` block — we
                // can't call `&mut self` recovery while a closure-style
                // recovery is held; do it in the outer scope below.
                // The `?`-replacement here mirrors the other 5 sites:
                // see #910 / REN-D5-NEW-01 (acquire-signal leak).
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }
        t.cmd_record_ns = cmd_t0.elapsed().as_nanos() as u64;

        // Submit.
        let submit_t0 = Instant::now();
        let wait_semaphores = [self.frame_sync.image_available[frame]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        // #906 / REN-D1-NEW-02 — render_finished is per FRAME-IN-FLIGHT,
        // not per image. Pre-fix the per-image index left a MAILBOX
        // race where a discarded queued present (spec-legal under
        // MAILBOX) would leave `render_finished[image]` signaled, and
        // the next frame re-acquiring that image would re-signal it
        // (VUID-vkQueueSubmit-pSignalSemaphores-00067). Per-frame keys
        // off `in_flight[frame]` instead — the fence wait at line 149
        // already guarantees the previous submit on this slot has
        // retired, so the semaphore is unsignaled by the time we
        // reuse it. Matches the canonical Khronos / Vulkan-Tutorial
        // sample pattern.
        let signal_semaphores = [self.frame_sync.render_finished[frame]];
        let command_buffers_to_submit = [cmd];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers_to_submit)
            .signal_semaphores(&signal_semaphores);

        unsafe {
            let queue = *self
                .graphics_queue
                .lock()
                .expect("graphics queue lock poisoned");
            if let Err(e) = self
                .device
                .queue_submit(queue, &[submit_info], self.frame_sync.in_flight[frame])
                .context("queue_submit")
            {
                // Submit failed — `image_available[frame]` was never
                // consumed by the (would-be) wait, so it stays signal-
                // pending. Recover before propagating so the next
                // acquire on this slot doesn't trip
                // VUID-vkAcquireNextImageKHR-semaphore-01779.
                // #910 / REN-D5-NEW-01.
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        // #917 / REN-D10-NEW-03 — advance SVGF + TAA `frames_since_
        // creation` counters now that `queue_submit` returned success.
        // Each pipeline self-gates on its `dispatched_this_frame` flag
        // set during recording, so a skipped dispatch (svgf_failed
        // latch, missing pipeline, upload_params failure) is a no-op
        // here. Pre-fix the counters advanced at record time, meaning a
        // record-time / submit-time failure between them and submit
        // success would leave the counter advanced without the
        // corresponding GPU write — the next frame would assume valid
        // history that wasn't actually written.
        if let Some(ref mut svgf) = self.svgf {
            svgf.mark_frame_completed();
        }
        if let Some(ref mut taa) = self.taa {
            taa.mark_frame_completed();
        }

        // Present.
        let swapchains = [self.swapchain_state.swapchain];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_suboptimal = unsafe {
            let pq = self
                .present_queue
                .lock()
                .expect("present queue lock poisoned");
            match self
                .swapchain_state
                .swapchain_loader
                .queue_present(*pq, &present_info)
            {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(e) => anyhow::bail!("queue_present: {:?}", e),
            }
        };

        t.submit_present_ns = submit_t0.elapsed().as_nanos() as u64;
        if let Some(out) = timings {
            *out = t;
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Restore the scratch buffers to the context so their capacity
        // amortizes across frames (#243), then shrink them back toward
        // the working set after a past peak frame. Same policy as the
        // `tlas_instances_scratch` in #504 — scratch Vecs behave as
        // "grow fast, shrink on pressure": working-set × 2 keeps a
        // slack band against frame-to-frame variance, and the 512
        // floor avoids reallocations on common-case small scenes.
        let working_instances = gpu_instances.len();
        let working_batches = batches.len();
        self.gpu_instances_scratch = gpu_instances;
        self.batches_scratch = batches;
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.gpu_instances_scratch,
            working_instances,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.batches_scratch,
            working_batches,
            512,
        );

        // #645 / MEM-2-3 — TLAS instance buffer mirrored shrink. The
        // slot we just incremented to (`current_frame` after the line
        // above) is the one whose previous frame work signalled at
        // the start of this frame, so its instance / staging /
        // device-local buffers are GPU-idle at this point and safe to
        // tear down. The slot we just SUBMITTED on (the one before
        // the increment) stays in flight and is left alone.
        //
        // SAFETY: see precondition on
        // `AccelerationManager::shrink_tlas_to_fit` — caller must
        // ensure no in-flight command buffer references the target
        // slot. The `current_frame_after_increment` slot's fence was
        // waited on at the start of this frame's recording (the
        // standard MAX_FRAMES_IN_FLIGHT alternation), so by the time
        // we reach this line its previous use has completed by
        // construction. Same justification used by `#504` for the
        // CPU-side scratch shrink above.
        if let Some(accel) = self.accel_manager.as_mut() {
            if let Some(allocator) = self.allocator.as_ref() {
                let slot_to_shrink = self.current_frame;
                unsafe {
                    accel.shrink_tlas_to_fit(
                        slot_to_shrink,
                        working_instances as u32,
                        &self.device,
                        allocator,
                    );
                    // #682 / MEM-2-7 — TLAS build scratch shrink. Same
                    // safety justification as `shrink_tlas_to_fit`
                    // above (the slot's previous use completed before
                    // this frame's recording began). Order matters:
                    // run AFTER `shrink_tlas_to_fit` so a destroyed
                    // slot lets the scratch shrink hit its
                    // "tlas[slot] is None → drop scratch entirely"
                    // arm in one tick.
                    accel.shrink_tlas_scratch_to_fit(slot_to_shrink, &self.device, allocator);
                }
            }
        }

        Ok(suboptimal || present_suboptimal)
    }
}

#[cfg(test)]
mod is_caustic_source_tests {
    use super::*;

    /// Minimal `DrawCommand` builder for the caustic-gate unit tests.
    /// Fields irrelevant to `is_caustic_source` get zero/default values
    /// — the gate only consults `material_kind` and
    /// `multi_layer_refraction_scale`.
    fn cmd(material_kind: u32, multi_layer_refraction_scale: f32) -> DrawCommand {
        DrawCommand {
            mesh_handle: 0,
            texture_handle: 0,
            model_matrix: [0.0; 16],
            alpha_blend: true,
            src_blend: 6,
            dst_blend: 7,
            two_sided: false,
            is_decal: false,
            render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
            bone_offset: 0,
            normal_map_index: 0,
            dark_map_index: 0,
            glow_map_index: 0,
            detail_map_index: 0,
            gloss_map_index: 0,
            parallax_map_index: 0,
            parallax_height_scale: 0.0,
            parallax_max_passes: 0.0,
            env_map_index: 0,
            env_mask_index: 0,
            alpha_threshold: 0.0,
            alpha_test_func: 0,
            roughness: 0.5,
            metalness: 0.0,
            emissive_mult: 0.0,
            emissive_color: [0.0; 3],
            specular_strength: 0.0,
            specular_color: [0.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            sort_depth: 0,
            in_tlas: true,
            in_raster: true,
            avg_albedo: [0.0; 3],
            material_kind,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
            entity_id: 0,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            material_alpha: 1.0,
            skin_tint_rgba: [0.0; 4],
            hair_tint_rgb: [0.0; 3],
            multi_layer_envmap_strength: 0.0,
            eye_left_center: [0.0; 3],
            eye_cubemap_scale: 0.0,
            eye_right_center: [0.0; 3],
            multi_layer_inner_thickness: 0.0,
            multi_layer_refraction_scale,
            multi_layer_inner_scale: [0.0; 2],
            sparkle_rgba: [0.0; 4],
            effect_falloff: [0.0; 5],
            material_id: 0,
            vertex_color_emissive: false,
            effect_shader_flags: 0,
            is_water: false,
        }
    }

    #[test]
    fn glass_material_is_caustic_source() {
        // MATERIAL_KIND_GLASS = 100: engine-classified refractive surface.
        assert!(is_caustic_source(&cmd(MATERIAL_KIND_GLASS, 0.0)));
    }

    #[test]
    fn multi_layer_parallax_with_refraction_is_caustic_source() {
        // Skyrim+ BSLightingShaderProperty MultiLayerParallax variant
        // with non-zero refraction scale — real two-layer refraction.
        assert!(is_caustic_source(&cmd(11, 0.3)));
    }

    #[test]
    fn multi_layer_parallax_without_refraction_is_not_caustic() {
        // Kind 11 with zero refraction scale = parallax but no refraction.
        assert!(!is_caustic_source(&cmd(11, 0.0)));
    }

    #[test]
    fn default_lit_alpha_blend_is_not_caustic_source() {
        // material_kind=0 covers foliage alpha-test cutouts and particle
        // billboards. Pre-#922 the old `alpha_blend && metalness < 0.3`
        // gate fired here and burned `max_lights` TLAS ray queries per
        // foliage pixel on exterior cells.
        assert!(!is_caustic_source(&cmd(0, 0.0)));
    }

    #[test]
    fn hair_tint_is_not_caustic_source() {
        // material_kind=6 = HairTint (Skyrim+). Pre-#922 false positive.
        assert!(!is_caustic_source(&cmd(6, 0.0)));
    }

    #[test]
    fn effect_shader_is_not_caustic_source() {
        // MATERIAL_KIND_EFFECT_SHADER (101): BSEffectShaderProperty FX
        // cards — fire planes, magic auras, decals. Emissive add, no
        // refraction. Pre-#922 false positive on every alpha-blend FX.
        assert!(!is_caustic_source(&cmd(
            scene_buffer::MATERIAL_KIND_EFFECT_SHADER,
            0.0
        )));
    }

    #[test]
    fn skin_tint_is_not_caustic_source() {
        // material_kind=5 = SkinTint. Bethesda character skin meshes.
        // Pre-#922 false positive on the alpha-blend body slot.
        assert!(!is_caustic_source(&cmd(5, 0.0)));
    }
}
