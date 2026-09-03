//! Draw-command → `GpuInstance` translation, draw-batch formation, and the
//! material-table / terrain-tile / composite-param reuploads — extracted
//! from `draw.rs` (#3282 / TD1-2026-08-24-01) to shrink `draw_frame`. This
//! is the single largest sub-block of the pre-split function: the
//! instance-SSBO sort-key contract (`DrawBatch` merge key, indirect-draw
//! command build) and every per-frame UBO upload that must land before the
//! bulk pre-render-pass barrier. The recording order — including the final
//! HOST→VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT barrier and the water-caustic
//! pre-clear — is unchanged from the pre-split `draw_frame`.

use super::super::descriptors::memory_barrier;
use super::super::material::GpuMaterial;
use super::super::pipeline::PipelineKey;
use super::super::scene_buffer::{
    self, GpuInstance, GpuTerrainTile, INSTANCE_FLAG_ALPHA_BLEND, INSTANCE_FLAG_CAUSTIC_SOURCE,
    INSTANCE_FLAG_DIFFUSE_ALPHA, INSTANCE_FLAG_FLAT_SHADING, INSTANCE_FLAG_NON_UNIFORM_SCALE,
    INSTANCE_FLAG_TERRAIN_SPLAT, INSTANCE_RENDER_LAYER_MASK, INSTANCE_RENDER_LAYER_SHIFT,
    INSTANCE_TERRAIN_TILE_MASK, INSTANCE_TERRAIN_TILE_SHIFT,
};
use super::draw::{
    build_composite_params, is_caustic_source, is_refractive_glass, morph_gpu_fields_for_draw,
    morph_slot_backs_mesh, rebase_model_matrix, skin_slot_backs_mesh,
    skinned_vertex_address_for_draw, uses_rigid_motion_history, CompositeParamsInputs, DrawBatch,
};
use super::{DrawCommand, FrameTimings, SkyParams, VulkanContext};
use ash::vk;
use std::time::Instant;

/// Output of [`VulkanContext::build_and_upload_instances`] — the locals
/// `draw_frame`'s own tail (bookkeeping after `record_geometry_pass`) and
/// the immediately-following `record_geometry_pass` / `record_post_passes`
/// calls still need.
pub(super) struct BuildInstancesOutput {
    pub(super) gpu_instances: Vec<GpuInstance>,
    pub(super) previous_models: Vec<scene_buffer::GpuPreviousModel>,
    pub(super) current_rigid_models: rustc_hash::FxHashMap<u32, [f32; 16]>,
    pub(super) batches: Vec<DrawBatch>,
    pub(super) ui_instance_idx: Option<u32>,
    pub(super) caustic_history_valid: bool,
}

impl VulkanContext {
    /// Translate `draw_commands` into `GpuInstance`s + `DrawBatch`es, upload
    /// the instance/material/terrain-tile SSBOs, upload the composite/SVGF/
    /// TAA/water per-frame UBOs, and emit the bulk pre-render-pass barrier.
    /// Extracted verbatim from `draw_frame` — the recording order (including
    /// the final barrier and the water-caustic pre-clear) is unchanged.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_and_upload_instances(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        draw_commands: &[DrawCommand],
        render_origin: byroredux_core::math::Vec3,
        camera_cut: bool,
        camera_static: bool,
        pose_dirty: &rustc_hash::FxHashSet<byroredux_core::ecs::storage::EntityId>,
        lights: &[scene_buffer::GpuLight],
        instance_map: &[Option<u32>],
        ui_texture_handle: Option<u32>,
        materials: &[GpuMaterial],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        fog_extinction_per_meter: f32,
        fog_single_scatter_albedo: f32,
        fog_clip: f32,
        fog_power: f32,
        fog_height_reference: f32,
        sky_params: &SkyParams,
        camera_pos: [f32; 3],
        inv_vp_arr: [[f32; 4]; 4],
        underwater: [f32; 4],
        water_commands: &[super::super::water::WaterDrawCommand],
        armed_selected_ray_probe_generation: &mut Option<u32>,
        t: &mut FrameTimings,
    ) -> BuildInstancesOutput {
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
        let mut previous_models = std::mem::take(&mut self.previous_models_scratch);
        previous_models.clear();
        previous_models.reserve(draw_commands.len() + 1);
        let mut current_rigid_models = std::mem::take(&mut self.current_rigid_models_scratch);
        current_rigid_models.clear();
        current_rigid_models.reserve(draw_commands.len());
        let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
        batches.clear();
        // #3675 (PERF-D9-2026-08-30-02) — deliberately NOT
        // `reserve(draw_commands.len())`, unlike the three scratch
        // buffers above. `batches` holds one entry per MERGED draw
        // batch, not one per command — the repo's own baselines put
        // that count 13-19x lower than `draw_commands.len()` (e.g. FO4
        // InstituteBioScience: 296 batches vs 3949 commands). Reserving
        // to the wrong (much larger) quantity forced capacity above the
        // end-of-frame shrink target (`2 * max(working_batches, 512)`,
        // `shrink_scratch_if_oversized` below) on every dense cell,
        // so the shrink fired every frame and the next frame's reserve
        // immediately grew it back — two reallocations plus a memcpy of
        // the live batches, every frame, on the render hot path.
        // `push`'s own amortized O(1) growth (from whatever capacity
        // the shrink policy left it at) is a better fit than a reserve
        // keyed to a quantity 13-19x too large.
        // #2468 — scene-dirty accumulators for the caustic accumulator's
        // parked-camera EMA, gathered inside the loop below where the
        // matrices are already hot rather than in a second pass. Two
        // complementary signals, because they cover disjoint draws:
        //   * `rigid_instance_moved` — the rigid-history compare the loop
        //     already performs, which is free here and catches an occluder
        //     crossing between light and glass, or physics clutter settling.
        //   * `caustic_scene_key` — the placement of the caustic SOURCES
        //     themselves (a glass door swinging open). Those are
        //     alpha-blended, so `uses_rigid_motion_history` excludes them
        //     from the compare above and they need their own key.
        // Skinned actors are covered by `pose_dirty`, and the light rig is
        // folded into the key after the loop.
        let mut rigid_instance_moved = false;
        let mut caustic_scene_key = crate::vulkan::caustic::caustic_key_seed();

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
            let skip_batch = !draw_cmd.in_raster || draw_cmd.is_water;
            let current_model = rebase_model_matrix(m, render_origin);
            let uses_rigid_history =
                uses_rigid_motion_history(draw_cmd.bone_offset, draw_cmd.alpha_blend);
            let previous_source = if uses_rigid_history && !camera_cut {
                self.previous_rigid_models
                    .get(&draw_cmd.entity_id)
                    .unwrap_or(m)
            } else {
                m
            };
            previous_models.push(rebase_model_matrix(previous_source, render_origin));
            if uses_rigid_history {
                // #2468 — `previous_source` is this entity's last submitted
                // model matrix (or `m` itself on first sight / camera cut),
                // so an inequality here is exactly "this instance moved".
                rigid_instance_moved |= previous_source != m;
                current_rigid_models.insert(draw_cmd.entity_id, *m);
            }
            if is_caustic_source(draw_cmd) {
                for v in current_model.iter().flatten() {
                    caustic_scene_key =
                        crate::vulkan::caustic::fold_caustic_key_f32(caustic_scene_key, *v);
                }
            }

            // #1260 / PERF-D3-NEW-05 — flag-bit assembly is rasterizer-
            // only state. The non-uniform-scale dot products feed the
            // vertex shader's inverse-transpose path (triangle.vert
            // line 175); ALPHA_BLEND / FLAT_SHADING / TERRAIN_SPLAT /
            // RENDER_LAYER are all read only by the rasterized fragment
            // shader (`inst.flags & ...` at triangle.frag:1011 / 1074 /
            // 1119 / 1231 / 1728); CAUSTIC_SOURCE is gated by the
            // meshId G-buffer (caustic_splat.comp:170-172), which only
            // contains pixels for in-frustum rasterized geometry. The
            // RT hit paths read `hitInst.vertexOffset / indexOffset /
            // materialId / avgAlbedo* / textureIndex` (triangle.frag:
            // 438 / 543 / 2981 / 2147) but NEVER `hitInst.flags`.
            // Therefore off-frustum + water entries can ship `flags=0`
            // and skip the entire assembly block — the SSBO slot still
            // serves the RT contract (#516) via model+mesh refs +
            // material_id + avg_albedo, which are written
            // unconditionally below.
            let flags = if skip_batch {
                0u32
            } else {
                // Detect non-uniform scale from the model matrix column
                // lengths. If the 3 column vectors of the upper-3x3
                // have different lengths, the vertex shader must use
                // inverse-transpose for normals. Otherwise it can skip
                // the expensive inverse (~40 ALU ops). Three dot
                // products is trivial compared to the per-vertex savings.
                let col0_sq = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
                let col1_sq = m[4] * m[4] + m[5] * m[5] + m[6] * m[6];
                let col2_sq = m[8] * m[8] + m[9] * m[9] + m[10] * m[10];
                let has_non_uniform_scale = {
                    let tol = 0.001;
                    (col0_sq - col1_sq).abs() > tol || (col0_sq - col2_sq).abs() > tol
                };
                // Per-instance flags — see INSTANCE_FLAG_* constants in
                // scene_buffer.rs. CPU-side assembly must stay in
                // lockstep with the fragment shader's `flags & N` checks.
                //   bit 0 = non-uniform scale
                //   bit 1 = NiAlphaProperty blend bit
                //   bit 2 = caustic source — real refractive surface
                //           (#922 / REN-D13-NEW-01). Gate matches the
                //           upstream glass classification in
                //           `render::build_render_data` (#515 / #706):
                //           engine-classified `MATERIAL_KIND_GLASS`
                //           (alpha-blend + low metal + low roughness +
                //           not a decal) OR Skyrim+ `MultiLayerParallax`
                //           (kind 11) with a non-zero inner-layer
                //           refraction scale.
                //   bit 3 = terrain splat (set in cell_loader for LAND
                //           entities, #470).
                let mut f = if has_non_uniform_scale {
                    INSTANCE_FLAG_NON_UNIFORM_SCALE
                } else {
                    0u32
                };
                if draw_cmd.alpha_blend {
                    f |= INSTANCE_FLAG_ALPHA_BLEND;
                    // #1653 — tells the fragment shader the diffuse carries
                    // a GENUINE authored alpha channel. When clear (BC1 and
                    // other alpha-less formats) the shader pins texColor.a
                    // to 1.0 unless an alpha test is active, so a BC1
                    // 3-colour block's index-3 texel (a==0 in opaque
                    // regions, an RGB-fidelity encoder choice) can't leak
                    // transparency into the discard / decalWeight /
                    // finalAlpha paths on a pure-blend mesh. BC1 decodes as
                    // BC1_RGBA so its 1-bit punch-through still drives
                    // alpha-test cutouts (2aac5351). `handle_has_alpha` is
                    // false for BC1_RGBA (`format_has_alpha` excludes it)
                    // and true for BC2/BC3/BC7/RGBA, so the FNV picture/
                    // table blend keeps its authored alpha. Cheap cached
                    // lookup (same map as the gi_albedo mean below), gated
                    // on alpha_blend so the opaque majority pays nothing.
                    if self
                        .texture_registry
                        .handle_has_alpha(draw_cmd.texture_handle)
                    {
                        f |= INSTANCE_FLAG_DIFFUSE_ALPHA;
                    }
                }
                if is_caustic_source(draw_cmd) {
                    f |= INSTANCE_FLAG_CAUSTIC_SOURCE;
                }
                if let Some(tile_idx) = draw_cmd.terrain_tile_index {
                    f |= INSTANCE_FLAG_TERRAIN_SPLAT;
                    f |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT;
                }
                // #869 — NiShadeProperty.flags==0 flat-shading:
                // fragment shader replaces interpolated normal with
                // the per-face derivative when this bit is set.
                if draw_cmd.flat_shading {
                    f |= INSTANCE_FLAG_FLAT_SHADING;
                }
                // #renderlayer — pack the 2-bit layer discriminant
                // into bits 4..5 for the fragment shader's debug-viz
                // branch (BYROREDUX_RENDER_DEBUG=0x40 tints fragments
                // by layer).
                f |= (draw_cmd.render_layer as u32 & INSTANCE_RENDER_LAYER_MASK)
                    << INSTANCE_RENDER_LAYER_SHIFT;
                f
            };

            // R1 Phase 6 — `GpuInstance` carries only per-DRAW data
            // now: model + mesh refs + bone_offset + flags +
            // material_id + caustic-source avg_albedo. Every
            // per-material field reads through `materials[material_id]`
            // in the fragment shader.
            //
            // #1628 — fold the diffuse texture's texel-mean into the GI
            // bounce albedo. `draw_cmd.avg_albedo` is the material tint
            // (diffuse_color); multiplying it by the texture's average
            // texel colour gives the true surface mean a textured wall
            // bleeds into the one-bounce GI, instead of the flat tint.
            // The mean is computed once at DDS upload and cached per
            // handle, so this is a cheap lookup + multiply. Untextured /
            // normal-map / BC7 handles return `None` and keep the tint.
            let gi_albedo = match self
                .texture_registry
                .handle_avg_rgb(draw_cmd.texture_handle)
            {
                Some(mean) => [
                    draw_cmd.avg_albedo[0] * mean[0],
                    draw_cmd.avg_albedo[1] * mean[1],
                    draw_cmd.avg_albedo[2] * mean[2],
                ],
                None => draw_cmd.avg_albedo,
            };
            // REN-2026-07-28-02 / #2219 — skinned instances' secondary-ray
            // hit-normal reconstruction needs the deformed (post-skin)
            // vertex positions, not the bind-pose global vertex SSBO
            // `getHitTriNormal` otherwise reads unconditionally. Look up
            // this entity's SkinSlot (populated earlier this frame by the
            // skin-dispatch chain) and query its output buffer's GPU
            // address; `ray_hit.glsl` dereferences it via
            // GL_EXT_buffer_reference for `boneOffset != 0` instances
            // instead of the bind-pose path. Zero for rigid draws and for
            // skinned draws with no slot yet (first-sight frame, or a
            // pool-exhaustion fallback) — the shader's own `boneOffset !=
            // 0` branch means a stray zero address is never dereferenced
            // by a rigid draw, and a skinned draw with no slot yet already
            // has no primed skinned BLAS this frame either, so falling
            // back to the bind-pose hit-normal path is consistent with
            // what the rest of the RT pipeline does for that draw.
            let slot_address = (draw_cmd.bone_offset != 0)
                .then(|| self.skin_slots.get(&draw_cmd.entity_id))
                .flatten()
                // #2402 — the slot must have been sized for THIS mesh. The
                // refit path's capacity reconciliation skips non-RT-capable
                // meshes entirely, so a remap onto one leaves a stale slot
                // live for a few frames; publishing its address would have
                // the fragment shader index a raw device address with the
                // new mesh's (possibly larger) index range.
                // #2402 — this filter must stay IN FRONT of the address read: a
                // slot that no longer backs this mesh must contribute no address,
                // cached or queried.
                .filter(|slot| skin_slot_backs_mesh(slot.vertex_count(), mesh.vertex_count))
                // #3469 — a plain field read. This was a `vkGetBufferDeviceAddress`
                // call per skinned draw per frame, in the innermost
                // O(visible-instance) loop, for an address that is fixed for the
                // buffer's lifetime. `skin_pool_live = 83` on
                // `skyrim_se-WhiterunDragonsreach` and several draws per NPC put it
                // in the hundreds of driver round-trips per frame in that cell.
                .map(|slot| slot.output_address());
            let skinned_vertex_address =
                skinned_vertex_address_for_draw(draw_cmd.bone_offset, slot_address);
            // #3231 — GPU morph-target blending. Same shape as the
            // skinned_vertex_address lookup just above, including the
            // "backs this mesh" safety filter (#2402's hazard applies
            // identically here — see `morph_slot_backs_mesh`'s doc).
            // v1-scoped to skinned meshes only (`bone_offset != 0`),
            // matching MorphSlot's current spawn-time creation site.
            let morph_slot_fields = (draw_cmd.bone_offset != 0)
                .then(|| self.morph_slots.get(&draw_cmd.entity_id))
                .flatten()
                .filter(|slot| {
                    morph_slot_backs_mesh(
                        slot.vertex_count(),
                        slot.target_count(),
                        mesh.vertex_count,
                    )
                })
                .map(|slot| {
                    (
                        slot.delta_address(),
                        slot.weight_address(),
                        slot.target_count(),
                    )
                });
            let (morph_delta_address, morph_weight_address, morph_target_count) =
                morph_gpu_fields_for_draw(morph_slot_fields);
            gpu_instances.push(GpuInstance {
                // #markarth-precision — rebase the model translation by the
                // camera-relative render origin so `model * pos` stays near 0
                // in the shader (full f32 precision; large worldspace offsets
                // like MarkarthWorld's ~-176000 otherwise quantize fine detail
                // into spikes). The shader adds render_origin back for the
                // absolute world position. Columns 0-2 (rotation/scale) are
                // unchanged; only the translation column (m[12..14]) shifts.
                model: current_model,
                texture_index: draw_cmd.texture_handle,
                bone_offset: draw_cmd.bone_offset,
                vertex_offset: mesh.global_vertex_offset,
                index_offset: mesh.global_index_offset,
                vertex_count: mesh.vertex_count,
                flags,
                material_id: draw_cmd.material_id,
                // Reuse the layout's former padding lane for per-material IOR.
                // caustic_splat.comp names this offset `ior`; other shaders
                // keep treating it as padding, so the std430 ABI is unchanged.
                ior: draw_cmd.ior,
                avg_albedo_r: gi_albedo[0],
                avg_albedo_g: gi_albedo[1],
                avg_albedo_b: gi_albedo[2],
                // Stable across per-frame sort/batch changes. Zero remains
                // reserved for synthetic/default instances.
                surface_id: draw_cmd.entity_id.wrapping_add(1),
                skinned_vertex_address,
                _reserved: [0; 2],
                morph_delta_address,
                morph_weight_address,
                morph_target_count,
                _reserved2a: 0,
                _reserved2b: 0,
                _reserved2c: 0,
            });

            // Frustum-culled draws still need an SSBO entry so RT hit
            // shaders that land on their TLAS instance read the right
            // material / transform (#516).
            //
            // REN-LOW L-8 / #2164 — "transform" needs a caveat. Since the
            // render-origin rebase, `GpuInstance.model` is render-origin
            // *relative*, while the TLAS an RT hit arrives through is
            // absolute. Rotation and scale are therefore usable from a hit
            // shader; translation is NOT. The only current RT reader
            // (`raytrace.glsl::getHitTriNormal`) is translation-invariant,
            // so nothing is wrong today — but a future hit-position
            // reconstruction built on `.model[3]` would land `renderOrigin`
            // (up to ~176k units on MarkarthWorld) from the true hit. Add
            // `+ renderOrigin.xyz` if you ever need absolute position here.
            //
            // Skip batch formation — they
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
            if skip_batch {
                continue;
            }

            // Two-sided is NOT a key axis (#930) — both opaque and
            // blended pipelines declare CULL_MODE as dynamic state, so
            // two-sided rendering uses per-draw `cmd_set_cull_mode`
            // not a separate pipeline. Wireframe IS a key axis (#869)
            // because `polygon_mode` is static pipeline state — LINE
            // and FILL each need their own pipeline.
            let order_dependent_glass = is_refractive_glass(draw_cmd);
            let pipeline_key = if draw_cmd.alpha_blend {
                PipelineKey::Blended {
                    src: draw_cmd.src_blend,
                    dst: draw_cmd.dst_blend,
                    wireframe: draw_cmd.wireframe,
                    preserve_opaque_gbuffer: order_dependent_glass,
                }
            } else {
                PipelineKey::Opaque {
                    wireframe: draw_cmd.wireframe,
                }
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

            // #2165 — split-eligibility is a material property, resolved
            // once here at emit time. Part of the batch merge key: a
            // glass draw and a particle draw that happen to agree on
            // every pipeline/depth axis must not fold together, or the
            // merged batch would take one population's path for both.
            if let Some(batch) = batches.last_mut() {
                if batch.mesh_handle == draw_cmd.mesh_handle
                    && batch.pipeline_key == pipeline_key
                    && batch.two_sided == draw_cmd.two_sided
                    && batch.render_layer == render_layer
                    && batch.z_test == draw_cmd.z_test
                    && batch.z_write == draw_cmd.z_write
                    && batch.z_function == draw_cmd.z_function
                    && batch.order_dependent_glass == order_dependent_glass
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
                order_dependent_glass,
            });
        }

        // #2913 / REN-D1-01 — pin the AS↔SSBO index contract.
        //
        // `build_instance_map` (above) is documented as the single source of
        // truth that the TLAS `instance_custom_index` and the compacted SSBO
        // position must agree on, but only ONE of its two consumers actually
        // reads it: `build_tlas_instances` indexes `instance_map[i]`, while
        // the SSBO builder above re-derives the same compaction from
        // `gpu_instances.len()` behind its own copy of the predicate. They
        // agree today purely because both spell the `mesh_registry.get()`
        // reject identically, ~800 lines apart in one function. #419 removed
        // the divergence but not the fragility, and nothing `cargo test` can
        // see would catch its return.
        //
        // This is the one point where the two counts must match EXACTLY: the
        // draw loop has finished and the UI quad (which the map does not
        // cover) has not been appended yet. A mismatch means a `continue` was
        // added to the SSBO loop without a matching term in the map's
        // predicate — which silently shifts every later SSBO entry while the
        // TLAS custom indices stay put, so every RT hit reads the wrong
        // `GpuInstance` (wrong model matrix, wrong `material_id`, wrong
        // `surface_id`). That is the severity table's CRITICAL
        // "SSBO index mismatch" row, and it fails silently — garbage
        // material/transform in shadows/reflections/GI, not a crash or a
        // validation error.
        //
        // debug_assert, matching the sibling `previous_models` pin below: the
        // condition is an internal-consistency invariant that can only break
        // via a code change, never via content, so it cannot fire on a user's
        // machine mid-recording the way the content-dependent MAX_INSTANCES
        // check could (#956).
        debug_assert_eq!(
            gpu_instances.len(),
            instance_map.iter().flatten().count(),
            "AS<->SSBO index contract broken: the SSBO compaction produced {} \
             entries but build_instance_map mapped {} draw commands. A filter \
             was added to one compaction and not the other (#419 / #2913).",
            gpu_instances.len(),
            instance_map.iter().flatten().count(),
        );

        // Append UI instance (if needed) BEFORE the bulk upload so it's
        // included in the single flush. Avoids the need for a separate raw
        // pointer write + flush that was missing on non-coherent memory (#189).
        let ui_instance_idx =
            if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
                let idx = gpu_instances.len() as u32;
                let instance = GpuInstance {
                    texture_index: ui_tex,
                    ..GpuInstance::default()
                };
                previous_models.push(instance.model);
                gpu_instances.push(instance);
                Some(idx)
            } else {
                None
            };

        // #2468 — finish the caustic scene key with the light rig. Every
        // splat is a refraction of a specific light through a specific
        // surface, so a lantern being carried, a light being coloured /
        // dimmed by a weather or script change, or a light entering or
        // leaving the visible set all move the pool. `lights` is bounded
        // by the streaming-RIS visible set, so this is a short loop.
        caustic_scene_key =
            crate::vulkan::caustic::fold_caustic_key_f32(caustic_scene_key, lights.len() as f32);
        for light in lights {
            for v in light
                .position_radius
                .iter()
                .chain(light.color_type.iter())
                .chain(light.direction_angle.iter())
                .chain(light.params.iter())
            {
                caustic_scene_key =
                    crate::vulkan::caustic::fold_caustic_key_f32(caustic_scene_key, *v);
            }
        }
        // The accumulator's history is valid only when nothing that
        // determines a splat's landing point changed: the camera (the
        // pre-#2468 gate), the light rig or caustic-source placement (the
        // key), rigid instances (the compare in the loop above), or
        // skinned poses (`pose_dirty` — a walking NPC's torch shadow).
        let caustic_scene_static = !rigid_instance_moved
            && pose_dirty.is_empty()
            && caustic_scene_key == self.prev_caustic_scene_key;
        self.prev_caustic_scene_key = caustic_scene_key;
        let caustic_history_valid = camera_static && caustic_scene_static;

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
            debug_assert_eq!(gpu_instances.len(), previous_models.len());
            self.scene_buffers
                .upload_instances(&self.device, frame, &gpu_instances)
                .unwrap_or_else(|e| log::warn!("Failed to upload instances: {e}"));
            self.scene_buffers
                .upload_previous_models(&self.device, frame, &previous_models)
                .unwrap_or_else(|e| log::warn!("Failed to upload previous models: {e}"));
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

        // Feed the last retired main-pass timestamp into the hysteretic ray
        // allocator, then upload a fresh per-frame counter + loop limits.
        // Timer brackets are conservative upper bounds and cannot safely be
        // summed; use the slower controlled pass as the quality signal.
        let measured_lighting_ms = self
            .gpu_timers
            .as_ref()
            .map(|timers| {
                let snapshot = timers.last_snapshot();
                snapshot.main_render_ms.max(snapshot.volumetrics_ms)
            })
            .filter(|ms| *ms > 0.0);
        self.scene_buffers
            .reset_ray_budget(
                &self.device,
                frame,
                measured_lighting_ms,
                self.renderer_config.rt_test_ray_quality_tier,
            )
            .unwrap_or_else(|e| log::warn!("Failed to upload adaptive ray budget: {e}"));

        // Reupload the terrain tile SSBO when cell load mutated it.
        // The slab is static until the next cell transition — #497
        // moved it to a single DEVICE_LOCAL buffer. #3664 keeps only the
        // live high-water prefix, writes it into the current frame's
        // reusable staging buffer, and records the copy here so the
        // transfer is ordered before geometry without a queue-fence stall.
        // The scratch Vec lives on self so its capacity amortizes across
        // cell loads — `mem::take` moves it out so the fill can run while
        // `&mut self.scene_buffers` consumes the slice. #496.
        let mut tile_scratch: Vec<GpuTerrainTile> = std::mem::take(&mut self.terrain_tile_scratch);
        if self.fill_terrain_tile_scratch_if_dirty(&mut tile_scratch) {
            let allocator = self.allocator.as_ref().expect("allocator missing");
            self.scene_buffers
                .upload_terrain_tiles(
                    &self.device,
                    allocator,
                    cmd,
                    frame,
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
            // #2504 / D12-2026-08-07-02 — unlike the neighbouring data-SSBO
            // uploads above (stale content there only misrenders), the
            // indirect buffer's contents are fetched and executed by the
            // GPU. A failed upload must force the direct-draw fallback for
            // this frame in `record_geometry_pass` (`use_indirect` reads
            // `indirect_upload_ok`) rather than let `cmd_draw_indexed_
            // indirect` read stale or uninitialized commands.
            self.indirect_upload_ok = self
                .scene_buffers
                .upload_indirect_draws(&self.device, frame, indirect_scratch)
                .map_err(|e| {
                    log::warn!(
                        "Failed to upload indirect draws: {e} — falling back to direct draws this frame"
                    )
                })
                .is_ok();
        }
        t.ssbo_build_ns = ssbo_t0.elapsed().as_nanos() as u64;
        // #3467 — drained here rather than measured here: the rebuild runs in
        // `render_one_frame` before `draw_frame` is entered, so this is the
        // first point in the frame that owns a `FrameTimings` to put it in.
        t.geometry_rebuild_ns = self.mesh_registry.take_geometry_rebuild_ns();

        // Pre-populate the blend pipeline cache for any new (src, dst)
        // combos this frame. Resolved up-front because the hot draw
        // loop only takes `&self.device` for `cmd_bind_pipeline` and
        // can't reborrow `&mut self` to lazy-create. After this loop
        // every `PipelineKey::Blended` has a corresponding cache entry.
        // See #392 / #930 (two-sided dropped from key).
        // #1259 / PERF-D3-NEW-04 — pre-fix this loop did
        // `blend_pipeline_cache.contains_key` per batch (M = blended
        // batch count, typically 300-500 on a Skyrim exterior). After
        // the first few cell-load frames every (src, dst, wireframe)
        // combo is cached and the per-batch lookup always hits —
        // O(M) wasted work per frame in steady state.
        //
        // Two-stage swap: collect distinct keys into the persistent
        // `blend_seen_scratch` HashSet (O(M) inserts, but on a
        // typically-tiny set — the same 3-5 distinct combos repeat
        // across hundreds of batches), then walk the small set once.
        // The subset check after the walk also lets us skip the
        // creation pass entirely when every seen key is cached —
        // the common steady-state path.
        self.blend_seen_scratch.clear();
        for batch in &batches {
            if let PipelineKey::Blended {
                src,
                dst,
                wireframe,
                preserve_opaque_gbuffer,
            } = batch.pipeline_key
            {
                // Normalize cache key against the device-cap gate so a
                // disabled-wireframe device hits the same slot it would
                // for a regular opaque blend. Matches the gate in
                // `get_or_create_blend_pipeline`. #869.
                let wireframe = wireframe && self.device_caps.fill_mode_non_solid_supported;
                self.blend_seen_scratch
                    .insert((src, dst, wireframe, preserve_opaque_gbuffer));
            }
        }
        // Skip the creation pass when every seen key is already cached
        // (the steady-state fast path — after warmup, no new pipeline
        // creation needed).
        let all_cached = self
            .blend_seen_scratch
            .iter()
            .all(|key| self.blend_pipeline_cache.contains_key(key));
        if !all_cached {
            // Collect missing keys into a local Vec so we can release
            // the borrow on `blend_seen_scratch` before calling
            // `get_or_create_blend_pipeline` (which takes `&mut self`
            // and would re-borrow scratch via the cache field).
            let missing: Vec<(u8, u8, bool, bool)> = self
                .blend_seen_scratch
                .iter()
                .filter(|key| !self.blend_pipeline_cache.contains_key(key))
                .copied()
                .collect();
            for (src, dst, wireframe, preserve_opaque_gbuffer) in missing {
                if let Err(e) =
                    self.get_or_create_blend_pipeline(src, dst, wireframe, preserve_opaque_gbuffer)
                {
                    log::error!(
                        "Failed to create blend pipeline (src={src}, dst={dst}, \
                         preserve_opaque_gbuffer={preserve_opaque_gbuffer}): {e}; \
                         draws using this combo will fall back to opaque pipeline"
                    );
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
            let composite_params = build_composite_params(CompositeParamsInputs {
                fog_color,
                fog_near,
                fog_far,
                fog_extinction_per_meter,
                fog_single_scatter_albedo,
                fog_clip,
                fog_power,
                fog_height_reference,
                sky_params,
                render_debug_flags: self.render_debug_flags,
                render_debug_mode: self.render_debug_mode.shader_value(),
                frame_counter: self.frame_counter,
                volume_far_distance: self
                    .volumetrics
                    .as_ref()
                    .map_or(super::super::volumetrics::DEFAULT_VOLUME_FAR, |volume| {
                        volume.far_distance_world()
                    }),
                froxel_slice_count: self
                    .volumetrics
                    .as_ref()
                    .map_or(1.0, |volume| volume.extent().depth as f32),
                camera_pos,
                render_origin,
                inv_vp_arr,
                underwater,
                water_caustic_active: self.water_caustic_accum.is_some(),
            });
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
                // SAFETY: `svgf`'s host-visible param buffer for `frame` is live and not in use by an in-flight frame (the fence wait at frame start guarantees the prior use of this slot completed); the host write is made visible to the compute pass by the bulk HOST->COMPUTE barrier below.
                if let Err(e) = unsafe {
                    svgf.upload_params(
                        &self.device,
                        frame,
                        alpha_color,
                        alpha_moments,
                        camera_static,
                    )
                } {
                    log::warn!("svgf upload_params failed: {e}");
                }
            }
        }

        // TAA UBO — fold into the bulk barrier below (#1397 / NCPS-03).
        // upload_params writes the host-visible param_buffers[frame];
        // the HOST→COMPUTE dependency is covered by the bulk barrier's
        // dst_stage = COMPUTE_SHADER, so no per-dispatch barrier is needed.
        if !self.taa_failed {
            if let Some(ref mut taa) = self.taa {
                if let Err(e) = taa.upload_params(&self.device, frame) {
                    log::warn!("TAA upload_params failed: {e}");
                }
            }
        }

        // Water material UBO — upload before the shared HOST→FRAGMENT
        // barrier below. The per-draw push constant now carries only the
        // compact array index, so this is the sole material-data upload.
        if let Some(ref mut water) = self.water {
            if let Err(error) = water.upload_params(&self.device, frame, water_commands) {
                log::warn!("water parameter upload failed: {error}; skipping water this frame");
            }
        }

        // Bloom UBOs — #2037 / GPU-D5-01: every down/upsample param UBO
        // is a pure function of the (construction-time-fixed) mip
        // extents, so `BloomPipeline::new` writes them once and a
        // resize (which rebuilds the whole pipeline) re-enters that
        // same write. No per-frame upload needed here; only the
        // input_view descriptor update (which depends on the
        // render-pass HDR output) stays in dispatch().

        let selected_ray_probe_request = self
            .pending_selected_ray_probe
            .map(|request| (request.generation, request.pixel));
        if let Err(error) = self.scene_buffers.arm_selected_ray_probe(
            &self.device,
            frame,
            selected_ray_probe_request,
        ) {
            log::warn!("selected-ray probe arm failed: {error}");
        } else {
            *armed_selected_ray_probe_generation =
                selected_ray_probe_request.map(|(generation, _)| generation);
        }

        // Barrier: make the instance SSBO host write (and any remaining
        // light/camera/bone host writes) visible to the vertex + fragment
        // shaders in the upcoming render pass. Also covers all UBO host
        // writes uploaded above (composite, SVGF, TAA, bloom) — each
        // write completes before this barrier and the barrier's dst_stage
        // includes COMPUTE_SHADER, so every post-render-pass compute
        // consumer that had its UBO folded here needs no per-dispatch
        // HOST→COMPUTE barrier. Fold history: composite (#909 /
        // REN-D1-NEW-03), SVGF (#961 / REN-D10-NEW-04), TAA + bloom
        // (#1397 / NCPS-03). Required by Vulkan spec even for
        // HOST_COHERENT memory.
        // HOST → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT (instance SSBO + UBOs)
        // SAFETY: `cmd` is recording. This single HOST_WRITE -> VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT barrier makes every host-written buffer this frame (instance SSBO + composite/SVGF/TAA/bloom UBOs) visible to its shader consumers before the render pass; required by spec even for HOST_COHERENT memory.
        unsafe {
            memory_barrier(
                &self.device,
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_WRITE,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::DRAW_INDIRECT,
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::SHADER_WRITE
                    | vk::AccessFlags::UNIFORM_READ
                    | vk::AccessFlags::INDIRECT_COMMAND_READ,
            );
        }

        // #1255 / Phase C of #1210 — clear the water-caustic
        // accumulator BEFORE the main render pass begins. water.frag
        // (the live Phase D/E consumer) atomic-adds into it during
        // the main pass; the post-render-pass barrier below
        // sequences those writes to the composite read.
        // Skipped when the accumulator failed init (None) — graceful
        // degrade matches the rest of the renderer's optional-pipeline
        // policy.
        if let Some(ref wca) = self.water_caustic_accum {
            // SAFETY: `cmd` is recording and outside the render pass; `wca` (water-caustic accumulator) and its per-frame buffer are live. The clear is recorded before the main pass that atomic-adds into it, and the post-pass barrier sequences those writes to the composite read.
            unsafe { wca.clear_pre_render_pass(&self.device, cmd, frame) };
        }

        BuildInstancesOutput {
            gpu_instances,
            previous_models,
            current_rigid_models,
            batches,
            ui_instance_idx,
            caustic_history_valid,
        }
    }
}

#[cfg(test)]
mod batches_scratch_reserve_tests {
    /// #3675 (PERF-D9-2026-08-30-02) — `batches` (the merged-draw-batch
    /// scratch) must NOT be reserved to `draw_commands.len()`, the
    /// quantity `gpu_instances`/`previous_models`/`current_rigid_models`
    /// correctly use for their own reserves just above it. `batches`'
    /// working set is one entry per MERGED batch — the repo's own
    /// baselines put that 13-19x lower than the command count (e.g. FO4
    /// InstituteBioScience: 296 batches vs 3949 commands). Reserving to
    /// the command count forced capacity above the end-of-frame shrink
    /// target (`2 * max(working_batches, 512)`) on every dense cell, so
    /// the shrink fired every frame and the next frame's reserve grew it
    /// right back — two reallocations plus a memcpy of the live batches,
    /// every frame, on the render hot path. `push`'s own amortized O(1)
    /// growth from whatever capacity the shrink policy left is a better
    /// fit; this pins that the reserve call stays gone. A live test is
    /// impractical here — `build_and_upload_instances` needs a real
    /// `VulkanContext`, matching this crate's own established convention
    /// (`pose_dirty_crosses_the_crate_boundary_without_siphash` in
    /// `context/mod.rs`) for source-scanning that class of function.
    #[test]
    fn batches_scratch_is_not_reserved_to_draw_command_count() {
        // Scoped to the PRODUCTION portion of the file, ending at this
        // test module's own opening line. An unscoped `include_str!`
        // search would match this test's own `.contains("...")` argument
        // string — the needle below is byte-identical to that argument,
        // so an unscoped search would ALWAYS find a match, even with the
        // production reserve call deleted outright (verified: an earlier
        // draft of this test did exactly that and passed regardless).
        let full_src = include_str!("build_and_upload_instances.rs");
        let module_start = full_src
            .find("mod batches_scratch_reserve_tests")
            .expect("this test module must still exist under its own name");
        let src = &full_src[..module_start];

        assert!(
            src.contains("let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);"),
            "batches must still be taken from batches_scratch via mem::take (#243) — \
             the needle this test scopes its check around has moved or been renamed"
        );
        assert!(
            !src.contains("batches.reserve(draw_commands.len())"),
            "batches (the merged-draw-batch scratch) must not be reserved to \
             draw_commands.len() — that quantity is 13-19x too large for its \
             actual working set (one entry per merged batch), which fights the \
             end-of-frame shrink policy every frame on dense cells (#3675)"
        );
    }
}
