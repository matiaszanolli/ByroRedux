//! The main geometry render pass — extracted from `draw.rs` (#1857 /
//! TD1-001) to shrink that file. Recorded between the bulk pre-render
//! barrier and `copy_depth_to_history` / `record_post_passes`; the
//! single `unsafe` scope, barrier order, and recording order are
//! unchanged from the pre-split `draw_frame`.

use super::super::pipeline::{gamebryo_to_vk_compare_op, PipelineKey};
use super::super::water::WaterDrawCommand;
use super::draw::{group_state, needs_two_sided_blend_split, DrawBatch};
use super::{DrawCommand, VulkanContext};
use ash::vk;

impl VulkanContext {
    /// Record the main geometry render pass into the open per-frame
    /// command buffer (#1748). Extracted verbatim from `draw_frame` — the
    /// single `unsafe` scope, barrier order, and recording order are
    /// unchanged. Runs between the bulk pre-render barrier and
    /// `copy_depth_to_history` / `record_post_passes`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_geometry_pass(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        render_pass_begin: &vk::RenderPassBeginInfo,
        batches: &[DrawBatch],
        draw_commands: &[DrawCommand],
        water_commands: &[WaterDrawCommand],
        ui_instance_idx: Option<u32>,
    ) {
        // SAFETY: `cmd` is recording (begin_command_buffer succeeded above) and `framebuffers[frame]` / `render_pass` / pipeline layout + descriptor sets / global VB+IB are all live for this frame. `cmd_begin_render_pass` opens the pass; viewport/scissor/cull/depth dynamic state is set before any draw; all binds use the GRAPHICS bind point with the matching `pipeline_layout`; `cmd` is recorded by this thread only and `end_command_buffer` closes it. The fence wait at frame start guarantees no in-flight frame is still using this buffer or its bound resources.
        unsafe {
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_main_render_start(&self.device, cmd, frame);
            }
            self.device
                .cmd_begin_render_pass(cmd, render_pass_begin, vk::SubpassContents::INLINE);

            // No unconditional pipeline bind here — the batch loop below
            // initializes `last_pipeline_key` to a sentinel Blended value
            // so the first real batch always rebinds to its own pipeline,
            // and the UI overlay rebinds `pipeline_ui` regardless. An
            // opaque bind at this point would always be discarded. #507.

            // Dynamic viewport + scissor.
            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.frame_extents.render.width as f32,
                height: self.frame_extents.render.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            self.device.cmd_set_viewport(cmd, 0, &viewports);

            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.frame_extents.render,
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
                wireframe: false,
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
            // #1581 / F1 — the indirect-merge key is `group_state` (module
            // fn, unit-tested): it must include EVERY dynamic state set once
            // from the group leader, or the leader's state wrongly applies to
            // the whole merged group.

            // #1258 / PERF-D3-NEW-03 — snapshot post-merge batch count.
            // Surfaced via `DebugStats::batch_count` and the `stats`
            // command so the next perf audit can distinguish "12k
            // DrawCommands" (input to the batcher) from "200 batches"
            // (actual GPU draw upper bound) from "20 indirect calls"
            // (post-grouping; bumped in the branches below).
            self.last_draw_call_stats.batch_count = batches.len() as u32;

            let mut i = 0;
            while i < batches.len() {
                let batch = &batches[i];

                // Switch pipeline when rendering mode changes.
                // Two-sided rendering uses dynamic `cmd_set_cull_mode`
                // (issued elsewhere in the draw loop based on
                // `draw_cmd.two_sided`), not a separate pipeline (#930).
                if batch.pipeline_key != last_pipeline_key {
                    let pipe = match batch.pipeline_key {
                        PipelineKey::Opaque { wireframe: false } => self.pipeline,
                        // Wireframe falls back to FILL on devices
                        // without `fillModeNonSolid`. #869.
                        PipelineKey::Opaque { wireframe: true } => {
                            self.pipeline_wireframe.unwrap_or(self.pipeline)
                        }
                        PipelineKey::Blended {
                            src,
                            dst,
                            wireframe,
                        } => {
                            // Always present after the pre-population
                            // pass above. If creation failed earlier we
                            // fall back to the opaque pipeline rather
                            // than skipping the draw entirely — better
                            // a wrong-blend visible mesh than a vanished
                            // one. See #392.
                            let wireframe =
                                wireframe && self.device_caps.fill_mode_non_solid_supported;
                            *self
                                .blend_pipeline_cache
                                .get(&(src, dst, wireframe))
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
                let two_sided = batch.two_sided;
                let needs_split = needs_two_sided_blend_split(batch);
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
                        // Per-mesh fallback (global SSBO not bound this frame).
                        // A global-only scene mesh (distant terrain LOD, #1370)
                        // carries no per-mesh buffers — skip it; it draws via
                        // the global buffer once `rebuild_geometry_ssbo` runs
                        // (≤1-frame distant pop-in, invisible).
                        let Some(mesh) = this.mesh_registry.get(batch.mesh_handle) else {
                            return;
                        };
                        let (Some(vb), Some(ib)) =
                            (mesh.vertex_buffer.as_ref(), mesh.index_buffer.as_ref())
                        else {
                            return;
                        };
                        if batch.mesh_handle != *last_bound {
                            this.device
                                .cmd_bind_vertex_buffers(cmd, 0, &[vb.buffer], &[0]);
                            this.device.cmd_bind_index_buffer(
                                cmd,
                                ib.buffer,
                                0,
                                vk::IndexType::UINT32,
                            );
                            *last_bound = batch.mesh_handle;
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
                    // #1258 — two-sided split emits 2 direct draws.
                    self.last_draw_call_stats.indirect_call_count += 2;
                    i += 1;
                } else if use_indirect {
                    set_cull(default_cull, &mut last_cull_mode);
                    // Gather consecutive batches that share the current
                    // `(pipeline_key, render_layer)` tuple — each one is
                    // already represented in the indirect buffer as one
                    // VkDrawIndexedIndirectCommand. A single
                    // `cmd_draw_indexed_indirect` call dispatches all N.
                    //
                    // Two-sided blend batches are excluded above (`needs_split`
                    // draws them directly) and can't reach this branch.
                    // `group_state` now captures two_sided + depth state, so a
                    // group is homogeneous in every leader-set dynamic state —
                    // the leader's cull/depth applies correctly to all of it.
                    let key = group_state(batch);
                    let mut end = i + 1;
                    while end < batches.len() && group_state(&batches[end]) == key {
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
                    // #1258 — one indirect call dispatches `group_size`
                    // batches; surfaced grouping ratio = batch_count /
                    // indirect_call_count.
                    self.last_draw_call_stats.indirect_call_count += 1;
                    i = end;
                } else {
                    // Direct-draw fallback: global VB/IB bound or
                    // per-mesh fallback inside `dispatch_direct`.
                    set_cull(default_cull, &mut last_cull_mode);
                    dispatch_direct(self, &mut last_bound_mesh_handle);
                    // #1258 — direct fallback emits 1 draw per batch.
                    self.last_draw_call_stats.indirect_call_count += 1;
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
            // Cull mode: water pipeline declares it DYNAMIC (#1071 /
            // F-WAT-11) — the caller is now required to emit
            // `cmd_set_cull_mode(NONE)` before the draw. Done explicitly
            // below regardless of the per-batch coalescing helper's
            // `last_cull_mode` state because water is rendered through
            // a separate, water-specific dispatch loop that doesn't
            // route through the main per-batch helper.
            // #1561 — water.frag traces RT rays (TLAS at set=1 binding=2)
            // with no `sceneFlags.x` runtime guard, so the water draw must not
            // run when RT isn't live: on a non-RT device binding 2 is absent
            // from the bound layout (`self.water` is also `None` there), and
            // even on RT hardware a frame whose TLAS wasn't written would trace
            // a stale/unwritten structure. Gate on the same
            // `ray_query_supported && tlas_written[frame]` signal that drives
            // `rt_flag`/`sceneFlags.x` everywhere else (the shader-side
            // `sceneFlags.x < 0.5` early-out — mirroring caustic_splat.comp —
            // remains a follow-up needing RenderDoc/non-RT verification).
            let rt_live =
                self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame];
            if !water_commands.is_empty() && rt_live {
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
                    // #1071 / F-WAT-11 — water pipeline declares CULL_MODE dynamic.
                    // Emit the runtime override here so the draw uses NONE (water
                    // surfaces are visible from above and below the camera plane).
                    self.device.cmd_set_cull_mode(cmd, vk::CullModeFlags::NONE);
                    for wc in water_commands {
                        if let Some(mesh) = self.mesh_registry.get(wc.mesh_handle) {
                            let vb = mesh
                                .vertex_buffer
                                .as_ref()
                                .expect("water mesh requires a per-mesh vertex buffer");
                            let ib = mesh
                                .index_buffer
                                .as_ref()
                                .expect("water mesh requires a per-mesh index buffer");
                            self.device
                                .cmd_bind_vertex_buffers(cmd, 0, &[vb.buffer], &[0]);
                            self.device.cmd_bind_index_buffer(
                                cmd,
                                ib.buffer,
                                0,
                                vk::IndexType::UINT32,
                            );
                            water.record_draw(
                                &self.device,
                                cmd,
                                &wc.push,
                                mesh.index_count,
                                wc.instance_index,
                                frame, // #1255 — selects set 2 per-FIF water-caustic descriptor
                                self.texture_registry.descriptor_set(frame), // #1258 — set 0
                                self.scene_buffers.descriptor_set(frame), // #1258 — set 1
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
                        width: self.frame_extents.render.width as f32,
                        height: self.frame_extents.render.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }];
                    self.device.cmd_set_viewport(cmd, 0, &viewports);
                    let scissors = [vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.frame_extents.render,
                    }];
                    self.device.cmd_set_scissor(cmd, 0, &scissors);
                    let vb = mesh
                        .vertex_buffer
                        .as_ref()
                        .expect("UI mesh requires a per-mesh vertex buffer");
                    let ib = mesh
                        .index_buffer
                        .as_ref()
                        .expect("UI mesh requires a per-mesh index buffer");
                    self.device
                        .cmd_bind_vertex_buffers(cmd, 0, &[vb.buffer], &[0]);
                    self.device
                        .cmd_bind_index_buffer(cmd, ib.buffer, 0, vk::IndexType::UINT32);
                    self.device
                        .cmd_draw_indexed(cmd, mesh.index_count, 1, 0, 0, idx);
                }
            }

            self.device.cmd_end_render_pass(cmd);
            if let Some(ref mut timers) = self.gpu_timers {
                timers.cmd_main_render_end(&self.device, cmd, frame);
            }
        }
    }
}
