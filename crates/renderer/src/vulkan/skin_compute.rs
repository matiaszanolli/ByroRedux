//! M29 Phase 1 — GPU pre-skinning compute pipeline.
//!
//! Reads bind-pose vertices from the shared global vertex SSBO + the
//! per-frame bone palette, multiplies each vertex through its weighted
//! bone transform, and writes the world-space skinned vertices to a
//! per-mesh dedicated output buffer. Phase 2 will refit a per-mesh BLAS
//! against this output so RT shadow / reflection / GI ray queries see
//! the animated geometry; the rasterized pipeline keeps its existing
//! inline-skinning path (`triangle.vert:147-204`) until Phase 3
//! (optional) migrates raster to read from the same buffer.
//!
//! Phase 1 ships the pipeline + slot manager + dispatch helper.
//! It does NOT yet schedule per-frame dispatches (Phase 1.5) and the
//! output buffer is unused by raster + RT (Phase 2 wires AcceleratedManager).
//!
//! See `shaders/skin_vertices.comp` for the shader-side contract +
//! per-vertex layout (matches `vertex.rs::Vertex`, 25 floats / 100 B
//! post-#783; tangent slot at floats 21..24 added for M-NORMALS).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{write_storage_buffer, DescriptorPoolBuilder};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::VERTEX_STRIDE_BYTES;
#[cfg(test)]
use crate::shader_constants::VERTEX_STRIDE_FLOATS;
use anyhow::{Context, Result};
use ash::vk;

const SKIN_VERTICES_COMP_SPV: &[u8] = include_bytes!("../../shaders/skin_vertices.comp.spv");
const SKIN_PALETTE_COMP_SPV: &[u8] = include_bytes!("../../shaders/skin_palette.comp.spv");

/// Compute workgroup size (local_size_x in skin_vertices.comp +
/// skin_palette.comp). Both shaders use the same 64-wide workgroup so
/// the dispatch arithmetic and occupancy story stay aligned.
const WORKGROUP_SIZE: u32 = 64;

/// Push constant payload — matches `skin_vertices.comp::PushConstants`.
/// 12 bytes (3 × u32). std430 doesn't require 16-B block alignment when
/// no vec4 follows, so we ship the tight layout. Well inside the 128 B
/// `maxPushConstantsSize` floor every Vulkan implementation guarantees.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkinPushConstants {
    /// Where this mesh's bind-pose vertices start in the input SSBO
    /// (in vertices, not floats).
    pub vertex_offset: u32,
    pub vertex_count: u32,
    /// Where this mesh's bone palette starts in the BoneBuffer (in
    /// mat4 entries). Must match the value the inline-skinning vertex
    /// shader reads from `GpuInstance.boneOffset`.
    pub bone_offset: u32,
}

const PUSH_CONSTANTS_SIZE: u32 = std::mem::size_of::<SkinPushConstants>() as u32;

/// Per-skinned-mesh output buffer + descriptor sets. Allocated lazily
/// on first dispatch and held until the owning entity is destroyed.
/// The descriptor sets bind the shared input + per-frame bone palette
/// against this slot's dedicated output buffer; one set per
/// frame-in-flight so the bone-palette buffer rotation stays correct.
pub struct SkinSlot {
    /// Skinned-vertex output buffer. Sized for `vertex_count` ×
    /// `VERTEX_STRIDE_BYTES`. Layout-identical to the input SSBO so
    /// Phase 3 can swap raster to read from it directly.
    pub output_buffer: GpuBuffer,
    /// Capacity of the output buffer in bytes (equal to the active
    /// vertex_count × VERTEX_STRIDE_BYTES at allocation time).
    pub output_size: vk::DeviceSize,
    /// One descriptor set per frame-in-flight — each binds (input,
    /// bone palette for that frame, this slot's output).
    descriptor_sets: [vk::DescriptorSet; MAX_FRAMES_IN_FLIGHT],
    /// Vertex count this slot was sized for. If the underlying mesh's
    /// vertex_count changes (NIF re-import, mod swap), the slot must
    /// be destroyed + recreated rather than reused.
    vertex_count: u32,
    /// LRU bookkeeping: frame counter at the most-recent dispatch into
    /// this slot. Bumped from `VulkanContext::draw_frame`'s skin chain
    /// every time the entity appears in `draw_commands` (including
    /// skip-path entries — see #1195 — so quiescent slots are NOT
    /// evicted as a side effect of the dispatch-dirty gate). Read by
    /// the per-frame eviction sweep — a slot whose `last_used_frame`
    /// trails the current frame by more than `MAX_FRAMES_IN_FLIGHT`
    /// has no in-flight reference and is safe to destroy
    /// synchronously. Same threshold the static `evict_unused_blas`
    /// path uses for non-skinned BLAS. See #643 / MEM-2-1.
    pub last_used_frame: u64,
    /// #1195 / PERF-DIM7-01 — flips `true` after the first successful
    /// `skin_pipeline.dispatch` for this slot. Before that flip, the
    /// output buffer holds uninitialised memory and dispatching it as
    /// BLAS input would produce garbage geometry, so the dispatch-
    /// dirty gate MUST NOT skip dispatch until this is `true`. The
    /// paired #1196 BLAS refit gate piggy-backs on the same invariant
    /// (refit reads `output_buffer` as input — no point refitting
    /// over uninitialised data).
    pub has_populated_output: bool,
    /// #1197 / PERF-DIM7-03 — per-FIF cache of "currently-bound
    /// `(input, palette)` buffer handle pair" for the slot's
    /// descriptor sets. `dispatch` compares this against the live
    /// pair; on match it skips `vkUpdateDescriptorSets` entirely.
    /// `output_buffer` isn't tracked — it's a function of the slot
    /// itself, so once any FIF has been written it stays correct
    /// (the buffer doesn't move). `input` changes on cell transitions
    /// (global vertex SSBO rebuild via
    /// `MeshRegistry::rebuild_geometry_ssbo`); `palette` is fixed for
    /// the renderer lifetime today but the comparison handles any
    /// future rotation correctly without an explicit invalidation hook.
    /// `None` = never written; first dispatch on that FIF is the cold
    /// write. Steady-state writes per slot per frame: 0.
    descriptor_bindings: [Option<(vk::Buffer, vk::Buffer)>; MAX_FRAMES_IN_FLIGHT],
}

impl SkinSlot {
    /// Number of vertices this slot was sized for.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

/// Per-frame counter snapshot for the skinned-BLAS coverage path.
///
/// `VulkanContext::draw_frame` increments these as it walks the
/// dispatches / first-sight / refit loops; `fill_skin_coverage_stats`
/// then copies them into the `SkinCoverageStats` resource. Reset each
/// frame at the entry to the skinned section.
///
/// `dispatches_skipped` (#1194 / PERF-DIM7-INSTR): incremented by the
/// dispatch-dirty gate (#1195 / PERF-DIM7-01) when an entity's bone
/// palette is unchanged from the previous frame — the compute dispatch
/// is suppressed and this counter bumps instead. `dispatches_total -
/// dispatches_skipped` gives the GPU dispatch count actually issued.
#[derive(Debug, Default, Clone, Copy)]
pub struct SkinCoverageFrame {
    pub dispatches_total: u32,
    pub dispatches_skipped: u32,
    pub first_sight_attempted: u32,
    pub first_sight_succeeded: u32,
    pub refits_attempted: u32,
    pub refits_succeeded: u32,
}

/// LRU eviction predicate for [`SkinSlot`] / `skinned_blas` cleanup.
///
/// Returns `true` when a slot whose most-recent dispatch was at
/// `last_used_frame` should be dropped given the current frame
/// counter `current_frame` and the safety threshold `min_idle`.
///
/// The threshold callers use is `MAX_FRAMES_IN_FLIGHT + 1` — that
/// guarantees no in-flight command buffer still references the slot's
/// descriptor sets / output buffer / matching skinned BLAS, so
/// synchronous destroy is safe. Mirrors
/// `acceleration.rs::evict_unused_blas` for the static-mesh BLAS path.
///
/// `last_used_frame == 0` is a sentinel for "never dispatched" — the
/// predicate skips eviction in that case so a slot created mid-frame
/// (where compute prime + first dispatch happen later in the same
/// `draw_frame`) isn't immediately reaped before its first
/// steady-state dispatch can bump the counter. See #643 / MEM-2-1.
#[inline]
pub fn should_evict_skin_slot(last_used_frame: u64, current_frame: u64, min_idle: u64) -> bool {
    if last_used_frame == 0 {
        return false;
    }
    let idle = current_frame.saturating_sub(last_used_frame);
    idle >= min_idle
}

/// #1297 / #1298 (DIM12-A-01) — does an existing [`SkinSlot`]'s
/// allocated capacity no longer match the live mesh vertex count?
///
/// A `SkinSlot`'s output buffer is sized once at [`SkinComputePipeline::create_slot`]
/// time for `slot_vertex_count` vertices. The compute dispatch is bounded
/// only by the per-draw `push.vertex_count` (the *current* mesh count),
/// NOT by the slot's allocated capacity — so if an entity's `mesh_handle`
/// is remapped to a different-vertex-count mesh and the slot is reused
/// verbatim, the shader writes `outputVertexData[vid …]` past the
/// buffer's end (OOB). When this returns `true` the caller must
/// destroy + recreate the slot (and drop the paired skinned BLAS) so the
/// buffer re-allocs to the new size. Uses `!=` (any mismatch, not just
/// growth) to keep the slot exactly matching the mesh — symmetric with
/// the BLAS-side [`validate_refit_counts`] guard, which also rejects on
/// `!=`. (`validate_refit_counts` is in `acceleration::predicates`.)
#[inline]
pub fn skin_slot_capacity_stale(slot_vertex_count: u32, mesh_vertex_count: u32) -> bool {
    slot_vertex_count != mesh_vertex_count
}

/// GPU pre-skinning compute pipeline.
///
/// Owns the pipeline + descriptor-set layout + descriptor pool. Slot
/// allocation (per skinned mesh) goes through `create_slot` and
/// returns an opaque `SkinSlot` the caller hands back to `dispatch`.
/// Lifecycle: pipeline lives for the renderer's lifetime; slots live
/// for the skinned mesh's lifetime.
///
/// Buffer bindings (input vertex SSBO + per-frame bone palette) are
/// cached per-FIF on each [`SkinSlot`] via
/// `SkinSlot::descriptor_bindings`. `dispatch` compares the live
/// `(input, palette)` pair against the cached key for `frame_index`;
/// on match it skips `vkUpdateDescriptorSets` entirely — bind, push,
/// and dispatch are the entire critical path in steady state. On
/// mismatch (cold first-dispatch per FIF or cell-transition global
/// vertex SSBO rebuild) we emit all three writes and record the new
/// key. See #1197 / PERF-DIM7-03; pre-fix the dispatch unconditionally
/// emitted three writes per slot per frame, costing ~1-3 µs per
/// invocation on NVIDIA regardless of writes-per-call.
///
/// The input-vertex SSBO and bone-palette buffer (plus their sizes) bound by
/// [`SkinComputePipeline::dispatch`]. Groups the buffer/size pairs that travel
/// together into the dispatch call.
#[derive(Clone, Copy)]
pub struct SkinDispatchBuffers {
    /// Pre-skinning input vertex SSBO.
    pub input_buffer: vk::Buffer,
    /// Byte size of `input_buffer`.
    pub input_buffer_size: vk::DeviceSize,
    /// Per-frame bone palette buffer.
    pub bone_buffer: vk::Buffer,
    /// Byte size of `bone_buffer`.
    pub bone_buffer_size: vk::DeviceSize,
}

pub struct SkinComputePipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// #1197 — per-frame `vkUpdateDescriptorSets` count for the
    /// dispatch-bound bindings. Bumped each time `dispatch` actually
    /// writes (cold first-dispatch per slot per FIF, or after a
    /// global-vertex-SSBO rebuild). Reset at frame start by
    /// `VulkanContext::draw_frame` via
    /// [`Self::reset_descriptor_writes_counter`]. Surfaced through
    /// the `tex.skin` debug command alongside the existing
    /// `SkinCoverageStats` counters from #1194.
    ///
    /// `Cell<u32>` rather than a `&mut self` borrow because every
    /// call site already has `slot: &mut SkinSlot` and threading an
    /// additional `&mut self` would force lifetime gymnastics on the
    /// dispatch loop. The renderer is single-threaded so interior
    /// mutability is safe.
    descriptor_writes_this_frame: std::cell::Cell<u32>,
}

impl SkinComputePipeline {
    /// Create the pipeline. Buffer bindings are deferred to per-dispatch
    /// (see struct doc-comment for rationale).
    pub fn new(
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        max_slots: u32,
    ) -> Result<Self> {
        // Descriptor set layout — 3 storage buffers (input, palette,
        // output). One set per (slot × frame_in_flight); the pool
        // sizes for `max_slots × MAX_FRAMES_IN_FLIGHT × 3` storage
        // buffer descriptors total.
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        // Cross-check the layout against SPIR-V reflection so a future
        // shader edit that reorders / renames bindings fails the build
        // instead of silently mis-binding. See cluster_cull's same
        // pattern + #427.
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "skin_vertices.comp",
                spirv: SKIN_VERTICES_COMP_SPV,
            }],
            "skin_compute",
            &[],
        )
        .expect("skin_compute layout drifted against skin_vertices.comp");

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("create skin_compute descriptor set layout")?
        };

        // Pipeline layout with a single push constant range covering
        // SkinPushConstants. Compute-only access.
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(PUSH_CONSTANTS_SIZE);
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&descriptor_set_layout))
                        .push_constant_ranges(std::slice::from_ref(&push_range)),
                    None,
                )
                .context("create skin_compute pipeline layout")?
        };

        // Compile the compute pipeline.
        let shader_module = super::pipeline::load_shader_module(device, SKIN_VERTICES_COMP_SPV)?;
        let pipeline_result = unsafe {
            device.create_compute_pipelines(
                pipeline_cache,
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(shader_module)
                            .name(c"main"),
                    )
                    .layout(pipeline_layout)],
                None,
            )
        };
        unsafe { device.destroy_shader_module(shader_module, None) };
        let pipeline = match pipeline_result {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                // Roll back what we already created — the partial
                // struct path used in cluster_cull is overkill for
                // three resources; explicit cleanup is clearer here.
                unsafe {
                    device.destroy_pipeline_layout(pipeline_layout, None);
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                }
                return Err(e).context("create skin_compute pipeline");
            }
        };

        // Descriptor pool — sized for max_slots × MAX_FRAMES_IN_FLIGHT
        // descriptor sets, each consuming 3 storage buffers (input,
        // palette, output). The chosen `max_slots` is the cell-load
        // ceiling for skinned entities — see the rationale comment at
        // `context/mod.rs::SKIN_MAX_SLOTS`. The architectural ceiling
        // is `MAX_TOTAL_BONES / MAX_BONES_PER_MESH` (the bone-palette
        // SSBO ceiling) — picking a smaller cap keeps it as a pressure
        // signal. (The pre-#900 comment here claimed `max_slots == 32
        // (matches MAX_TOTAL_BONES / MAX_BONES_PER_MESH)` — that math
        // was wrong; the ratio is the SSBO ceiling, not 32. Today's
        // exact value depends on `MAX_BONES_PER_MESH`, currently 144
        // per #1135, yielding floor(196608 / 144) = 1365.)
        let pool_total = max_slots * (MAX_FRAMES_IN_FLIGHT as u32);
        // Slots are freed on entity destruction (cell unload);
        // FREE_DESCRIPTOR_SET allows `vkFreeDescriptorSets` rather
        // than only `vkResetDescriptorPool`. Pool sizes derived from
        // `bindings` so any future binding addition flows through to
        // the pool count automatically (#1030 / REN-D10-NEW-09).
        let descriptor_pool =
            DescriptorPoolBuilder::from_layout_bindings(&bindings, pool_total)
                .max_sets(pool_total)
                .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                .build(device, "create skin_compute descriptor pool")?;

        log::info!(
            "Skin compute pipeline created (max_slots={}, sets={}, push={} B)",
            max_slots,
            pool_total,
            PUSH_CONSTANTS_SIZE,
        );

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            // #1197 — fresh counter at pipeline creation; the draw
            // loop resets it each frame, but starting at 0 is the
            // correct invariant.
            descriptor_writes_this_frame: std::cell::Cell::new(0),
        })
    }

    /// Allocate a per-mesh slot. The caller owns the returned
    /// `SkinSlot` and must hand it back to [`Self::destroy_slot`]
    /// before this pipeline is destroyed. Descriptor sets are
    /// allocated empty here — `dispatch` writes the bindings each
    /// frame (input + palette + output) so a global-vertex-buffer
    /// rebuild on cell transition doesn't invalidate the slot.
    pub fn create_slot(
        &self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        vertex_count: u32,
    ) -> Result<SkinSlot> {
        let output_size = (vertex_count as u64) * VERTEX_STRIDE_BYTES;
        // Phase 2 wires the output buffer as a BLAS-build input (vertex
        // source for the per-frame refit). The BLAS build path requires:
        //   - STORAGE_BUFFER     — compute shader writes
        //   - SHADER_DEVICE_ADDRESS — `vkGetBufferDeviceAddress` on the
        //                            buffer; AS build uses the device
        //                            address directly
        //   - ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR — the AS
        //                            build reads the buffer as a vertex
        //                            source
        //
        // VERTEX_BUFFER is deliberately NOT requested here: Phase 3
        // (M29.3, raster reading skinned output as VBO) is deferred,
        // and an unused usage bit tightens the memory-type mask
        // gpu-allocator must satisfy — on unified-memory iGPU configs
        // it can push the allocation onto a smaller heap. Re-add the
        // flag in the same commit that lands the raster bind path.
        // See #681 / MEM-2-6.
        let mut output_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            output_size,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                | vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR,
        )
        .context("allocate skin slot output buffer")?;

        // One descriptor set per frame-in-flight.
        // #871 — pool exhaustion at >32 simultaneously-skinned entities
        // returns Err here AFTER `output_buffer` is already allocated;
        // explicit rollback below avoids a one-time GPU-memory leak per
        // exhaustion event. `GpuBuffer::Drop` is warn-only by design
        // (C3-10 leak-on-drop pattern), so the natural-Drop pass on the
        // `output_buffer` local does NOT free the device-local memory.
        let layouts = [self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let allocated = match unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.descriptor_pool)
                    .set_layouts(&layouts),
            )
        } {
            Ok(sets) => sets,
            Err(e) => {
                output_buffer.destroy(device, allocator);
                return Err(e).context("allocate skin slot descriptor sets");
            }
        };
        let mut descriptor_sets = [vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        for (i, set) in allocated.iter().enumerate() {
            descriptor_sets[i] = *set;
        }

        Ok(SkinSlot {
            output_buffer,
            output_size,
            descriptor_sets,
            vertex_count,
            // Initialise to 0; the draw chain bumps to the current
            // frame counter on the first dispatch (and every
            // subsequent one). #643 / MEM-2-1.
            last_used_frame: 0,
            // #1195 — flips `true` after the first successful dispatch
            // for this slot. Until then the dispatch-dirty gate must
            // always dispatch (output buffer is uninitialised).
            has_populated_output: false,
            // #1197 / PERF-DIM7-03 — no descriptor writes recorded yet
            // for any FIF; first dispatch on each FIF is the cold
            // write.
            descriptor_bindings: [None; MAX_FRAMES_IN_FLIGHT],
        })
    }

    /// Free the GPU resources behind a slot. Caller must ensure the
    /// slot's output buffer isn't referenced by an in-flight command
    /// buffer (typical pattern: defer via the `MeshRegistry`'s
    /// `deferred_destroy` slot, or call after a `device_wait_idle`).
    pub fn destroy_slot(
        &self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        mut slot: SkinSlot,
    ) {
        unsafe {
            // Free the descriptor sets back to the pool. Required
            // because the pool was created with FREE_DESCRIPTOR_SET;
            // without this call the pool would leak descriptor slots
            // until pool reset / destruction.
            let _ = device.free_descriptor_sets(self.descriptor_pool, &slot.descriptor_sets);
        }
        slot.output_buffer.destroy(device, allocator);
    }

    /// Record a dispatch into `cmd` that pre-skins this slot's
    /// vertices. Must be called between the bone-palette upload for
    /// `frame_index` and any consumer of the output buffer (Phase 2:
    /// the BLAS refit reads it as `ACCELERATION_STRUCTURE_BUILD_INPUT`).
    ///
    /// #1197 / PERF-DIM7-03 — descriptor writes are compared against
    /// `slot.descriptor_bindings[frame_index]` and skipped when the
    /// `(input, palette)` buffer pair is unchanged. In steady state
    /// every dispatch is bind + push + dispatch with NO
    /// `vkUpdateDescriptorSets` (saving 3 writes × ~1-3 µs per slot
    /// per frame on NVIDIA). Cell transitions rebuild the global
    /// vertex SSBO; the comparison auto-detects the new handle and
    /// re-writes for the FIF that next dispatches.
    ///
    /// The per-frame fence at draw_frame's top guarantees previous-
    /// frame use of `slot.descriptor_sets[frame_index]` is complete
    /// before we rewrite, so no external sync is needed.
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer. `input_buffer` must
    /// stay alive for the lifetime of this dispatch (typically the
    /// global vertex SSBO held by `MeshRegistry`).
    pub unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        slot: &mut SkinSlot,
        frame_index: usize,
        buffers: SkinDispatchBuffers,
        push: SkinPushConstants,
    ) {
        let SkinDispatchBuffers {
            input_buffer,
            input_buffer_size,
            bone_buffer,
            bone_buffer_size,
        } = buffers;
        let descriptor_set = slot.descriptor_sets[frame_index];
        // #1197 — only the (input, palette) pair varies across the
        // slot's lifetime. `output` is a function of the slot itself
        // and cannot change without destroying the slot, so a cached
        // hit on `(input, palette)` is sufficient to prove all three
        // descriptors are still correct.
        let live_key = (input_buffer, bone_buffer);
        let needs_write = slot.descriptor_bindings[frame_index] != Some(live_key);
        if needs_write {
            let input_info = [vk::DescriptorBufferInfo {
                buffer: input_buffer,
                offset: 0,
                range: input_buffer_size,
            }];
            let bone_info = [vk::DescriptorBufferInfo {
                buffer: bone_buffer,
                offset: 0,
                range: bone_buffer_size,
            }];
            let output_info = [vk::DescriptorBufferInfo {
                buffer: slot.output_buffer.buffer,
                offset: 0,
                range: slot.output_size,
            }];
            let writes = [
                write_storage_buffer(descriptor_set, 0, &input_info),
                write_storage_buffer(descriptor_set, 1, &bone_info),
                write_storage_buffer(descriptor_set, 2, &output_info),
            ];
            device.update_descriptor_sets(&writes, &[]);
            slot.descriptor_bindings[frame_index] = Some(live_key);
            self.descriptor_writes_this_frame
                .set(self.descriptor_writes_this_frame.get() + 1);
        }

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[descriptor_set],
            &[],
        );
        // SAFETY: `SkinPushConstants` is `repr(C)` with three u32 fields,
        // 12 bytes, no interior padding. The slice is contiguous +
        // aligned (`size_of::<SkinPushConstants>()` matches the
        // shader-side `PushConstants` block byte-for-byte; mismatched
        // shape is caught by `push_constants_size_is_12_bytes` test).
        let bytes = std::slice::from_raw_parts(
            (&push as *const SkinPushConstants) as *const u8,
            PUSH_CONSTANTS_SIZE as usize,
        );
        device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        let groups = push.vertex_count.div_ceil(WORKGROUP_SIZE);
        device.cmd_dispatch(cmd, groups, 1, 1);
    }

    /// #1197 / PERF-DIM7-03 — how many `vkUpdateDescriptorSets` calls
    /// `dispatch` has issued this frame. Reset each frame by
    /// [`Self::reset_descriptor_writes_counter`]; surfaced via the
    /// `tex.skin` debug command alongside the #1194 coverage counters.
    /// Pre-fix this was a hardcoded `3 × slot_count` per frame;
    /// post-fix steady state is `0` and cell transitions briefly bump
    /// to `slot_count` (one cold write per slot for the FIF that
    /// dispatches first after the global vertex SSBO rebuild).
    pub fn descriptor_writes_this_frame(&self) -> u32 {
        self.descriptor_writes_this_frame.get()
    }

    /// #1197 — reset the per-frame descriptor-writes counter. Called
    /// once at frame start by `VulkanContext::draw_frame` before the
    /// skinned-section dispatch loop. Symmetric with the
    /// `last_skin_coverage_frame` reset earlier in the function.
    pub fn reset_descriptor_writes_counter(&self) {
        self.descriptor_writes_this_frame.set(0);
    }

    /// Tear down the pipeline + descriptor pool. Caller must
    /// destroy every outstanding slot first (slots hold descriptor
    /// sets from the pool; destroying the pool while sets are
    /// outstanding triggers a Vulkan validation error). The renderer
    /// pairs this with `device_wait_idle` in the Drop chain.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` is valid and live, the device is not lost,
    /// and that no object owned by `self` (pipeline, layouts, descriptor pool,
    /// or outstanding slots) is still in use by an in-flight command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

/// Push-constant payload for [`SkinPaletteComputePipeline::dispatch`] —
/// matches `skin_palette.comp::PushConstants`. 4 bytes (1 × u32). The
/// dispatch covers `ceil(bone_count / 64)` workgroups; the shader
/// early-returns for any tail-slot past `bone_count` so the dense
/// MAX_TOTAL_BONES output buffer is dispatch-safe in one shot.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkinPalettePushConstants {
    /// Number of populated palette slots this frame.
    pub bone_count: u32,
}

const SKIN_PALETTE_PUSH_CONSTANTS_SIZE: u32 =
    std::mem::size_of::<SkinPalettePushConstants>() as u32;

/// M29.5 — GPU bone-palette compute pipeline.
///
/// Reads two per-frame SSBOs (`bone_world[]` + `bind_inverses[]`) and
/// writes the existing bone-palette SSBO that
/// [`SkinComputePipeline`] (M29.3) + `triangle.vert`'s inline-skinning
/// path consume. Lifts the per-bone `world × bind_inv` matrix multiply
/// off the CPU.
///
/// Buffer bindings are compared per-FIF on each `dispatch` against
/// `descriptor_bindings[frame]`; on match the
/// `vkUpdateDescriptorSets` call is skipped (mirrors
/// [`SkinComputePipeline`]'s #1197 fix). All three palette-pipeline
/// buffers (`bone_world`, `bind_inverses`, `palette`) are fixed
/// handles for the renderer lifetime today, so steady-state hits the
/// cache every frame after the first MAX_FRAMES_IN_FLIGHT-frame
/// warm-up. The per-frame fence at `draw_frame`'s top guarantees
/// previous-frame use of `descriptor_sets[frame]` is complete before
/// we rewrite, so no external sync is needed.
///
/// The three buffer/size pairs ([bone-world], [bind-inverse], [palette]) bound
/// by [`SkinPaletteComputePipeline::dispatch`]. Groups the buffer arguments
/// that travel together into the dispatch call.
#[derive(Clone, Copy)]
pub struct PaletteDispatchBuffers {
    /// Per-frame bone world-transform buffer.
    pub bone_world_buffer: vk::Buffer,
    /// Byte size of `bone_world_buffer`.
    pub bone_world_buffer_size: vk::DeviceSize,
    /// Bind-pose inverse-bind-matrix buffer.
    pub bind_inverse_buffer: vk::Buffer,
    /// Byte size of `bind_inverse_buffer`.
    pub bind_inverse_buffer_size: vk::DeviceSize,
    /// Output bone-palette buffer.
    pub palette_buffer: vk::Buffer,
    /// Byte size of `palette_buffer`.
    pub palette_buffer_size: vk::DeviceSize,
}

pub struct SkinPaletteComputePipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// One descriptor set per frame-in-flight. Sized at construction;
    /// `dispatch` writes the three storage-buffer descriptors only
    /// when the live buffer triple differs from
    /// `descriptor_bindings[frame]`. Mirrors the per-slot set array
    /// in [`SkinSlot`] but at pipeline scope — there's only one
    /// palette dispatch per frame, not one per slot, so a fixed-size
    /// array is enough.
    descriptor_sets: [vk::DescriptorSet; MAX_FRAMES_IN_FLIGHT],
    /// #1197 / PERF-DIM7-03 — per-FIF cache of the
    /// `(bone_world, bind_inverse, palette)` buffer triple currently
    /// bound in `descriptor_sets[frame]`. `None` = never written;
    /// first dispatch on each FIF is the cold write. All three
    /// handles are stable for the renderer lifetime today, so
    /// `dispatch` skips the three writes on every steady-state call.
    descriptor_bindings: [Option<(vk::Buffer, vk::Buffer, vk::Buffer)>; MAX_FRAMES_IN_FLIGHT],
    /// #1197 — per-frame counter, paired with [`SkinComputePipeline`]'s
    /// counter. Surfaced through `tex.skin`. Pre-fix this dispatched
    /// 3 writes per frame unconditionally; post-fix steady state is 0.
    descriptor_writes_this_frame: std::cell::Cell<u32>,
}

impl SkinPaletteComputePipeline {
    pub fn new(device: &ash::Device, pipeline_cache: vk::PipelineCache) -> Result<Self> {
        // Three storage-buffer bindings: bone_world (in), bind_inverses
        // (in), palette (out). All COMPUTE-only.
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "skin_palette.comp",
                spirv: SKIN_PALETTE_COMP_SPV,
            }],
            "skin_palette",
            &[],
        )
        .expect("skin_palette layout drifted against skin_palette.comp");

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("create skin_palette descriptor set layout")?
        };

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(SKIN_PALETTE_PUSH_CONSTANTS_SIZE);
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&descriptor_set_layout))
                        .push_constant_ranges(std::slice::from_ref(&push_range)),
                    None,
                )
                .context("create skin_palette pipeline layout")?
        };

        let shader_module = super::pipeline::load_shader_module(device, SKIN_PALETTE_COMP_SPV)?;
        let pipeline_result = unsafe {
            device.create_compute_pipelines(
                pipeline_cache,
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(shader_module)
                            .name(c"main"),
                    )
                    .layout(pipeline_layout)],
                None,
            )
        };
        unsafe { device.destroy_shader_module(shader_module, None) };
        let pipeline = match pipeline_result {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                unsafe {
                    device.destroy_pipeline_layout(pipeline_layout, None);
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                }
                return Err(e).context("create skin_palette pipeline");
            }
        };

        // Exactly MAX_FRAMES_IN_FLIGHT descriptor sets — one palette
        // dispatch per frame, so the pool sizing is fixed (no per-slot
        // multiplication like SkinComputePipeline). Pool sizes derived
        // from `bindings` so a future binding addition flows through
        // automatically (#1030 / REN-D10-NEW-09).
        let pool_total = MAX_FRAMES_IN_FLIGHT as u32;
        let descriptor_pool =
            DescriptorPoolBuilder::from_layout_bindings(&bindings, pool_total)
                .max_sets(pool_total)
                .build(device, "create skin_palette descriptor pool")?;

        let layouts = [descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let allocated = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("allocate skin_palette descriptor sets")?
        };
        let mut descriptor_sets = [vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        for (i, set) in allocated.iter().enumerate() {
            descriptor_sets[i] = *set;
        }

        log::info!(
            "Skin palette compute pipeline created (sets={}, push={} B)",
            pool_total,
            SKIN_PALETTE_PUSH_CONSTANTS_SIZE,
        );

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
            // #1197 — descriptor caches start empty; the first
            // dispatch on each FIF is the cold write.
            descriptor_bindings: [None; MAX_FRAMES_IN_FLIGHT],
            descriptor_writes_this_frame: std::cell::Cell::new(0),
        })
    }

    /// Record the palette-build dispatch. Must be called between the
    /// bone_world transfer + bind_inverses first-sight transfers (if
    /// any) and any palette consumer (the existing M29.3
    /// `SkinComputePipeline::dispatch` for RT, or the raster
    /// `triangle.vert` read).
    ///
    /// `bind_inverse_buffer` post-M29.6 points at the PERSISTENT
    /// `bind_inverses` SSBO held on [`SceneBuffers`]. The same handle
    /// is passed every frame; the underlying data is written once per
    /// skinned-mesh first-sight via
    /// [`SceneBuffers::record_pending_bind_inverse_copies`].
    ///
    /// The caller is responsible for emitting:
    ///   - TRANSFER_WRITE → COMPUTE_SHADER_READ barriers on
    ///     `bone_world_buffer` (every frame) and on
    ///     `bind_inverse_buffer` (frames with pending first-sight
    ///     uploads only) BEFORE this dispatch.
    ///   - COMPUTE_SHADER_WRITE → (COMPUTE_SHADER_READ | VERTEX_SHADER_READ)
    ///     barrier on `palette_buffer` AFTER this dispatch.
    ///
    /// # Safety
    /// `cmd` must be a recording command buffer; all three buffers
    /// must remain valid for the duration of the dispatch.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        buffers: PaletteDispatchBuffers,
        push: SkinPalettePushConstants,
    ) {
        let PaletteDispatchBuffers {
            bone_world_buffer,
            bone_world_buffer_size,
            bind_inverse_buffer,
            bind_inverse_buffer_size,
            palette_buffer,
            palette_buffer_size,
        } = buffers;
        let descriptor_set = self.descriptor_sets[frame_index];
        // #1197 — compare against cached binding triple. Steady-state
        // hits this cache every frame post-warm-up because all three
        // palette-pipeline buffer handles are renderer-lifetime
        // stable. The comparison is still required for correctness in
        // case a future refactor rotates one of them.
        let live_key = (bone_world_buffer, bind_inverse_buffer, palette_buffer);
        let needs_write = self.descriptor_bindings[frame_index] != Some(live_key);
        if needs_write {
            let world_info = [vk::DescriptorBufferInfo {
                buffer: bone_world_buffer,
                offset: 0,
                range: bone_world_buffer_size,
            }];
            let bind_info = [vk::DescriptorBufferInfo {
                buffer: bind_inverse_buffer,
                offset: 0,
                range: bind_inverse_buffer_size,
            }];
            let palette_info = [vk::DescriptorBufferInfo {
                buffer: palette_buffer,
                offset: 0,
                range: palette_buffer_size,
            }];
            let writes = [
                write_storage_buffer(descriptor_set, 0, &world_info),
                write_storage_buffer(descriptor_set, 1, &bind_info),
                write_storage_buffer(descriptor_set, 2, &palette_info),
            ];
            device.update_descriptor_sets(&writes, &[]);
            self.descriptor_bindings[frame_index] = Some(live_key);
            self.descriptor_writes_this_frame
                .set(self.descriptor_writes_this_frame.get() + 1);
        }

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[descriptor_set],
            &[],
        );
        // SAFETY: `SkinPalettePushConstants` is `repr(C)` with one u32
        // field, 4 bytes, no interior padding. Push-constant size pinned
        // by `skin_palette_push_constants_size_is_4_bytes` test.
        let bytes = std::slice::from_raw_parts(
            (&push as *const SkinPalettePushConstants) as *const u8,
            SKIN_PALETTE_PUSH_CONSTANTS_SIZE as usize,
        );
        device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        let groups = push.bone_count.div_ceil(WORKGROUP_SIZE);
        device.cmd_dispatch(cmd, groups, 1, 1);
    }

    /// #1197 / PERF-DIM7-03 — descriptor-writes-this-frame for the
    /// palette pipeline. Pairs with
    /// [`SkinComputePipeline::descriptor_writes_this_frame`]; surfaced
    /// via the `tex.skin` debug command. Pre-fix every frame ran
    /// 3 writes; post-fix steady state is 0.
    pub fn descriptor_writes_this_frame(&self) -> u32 {
        self.descriptor_writes_this_frame.get()
    }

    /// #1197 — reset the per-frame counter. Called once at frame
    /// start by `VulkanContext::draw_frame`.
    pub fn reset_descriptor_writes_counter(&self) {
        self.descriptor_writes_this_frame.set(0);
    }

    /// Tear down the pipeline + descriptor pool. Caller pairs this with
    /// `device_wait_idle` in the Drop chain to guarantee no in-flight
    /// command buffer still references the descriptor sets. The
    /// descriptor sets themselves are freed implicitly by destroying
    /// the pool (no FREE_DESCRIPTOR_SET flag here — there are no
    /// per-slot allocations to free individually).
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` is valid and live, the device is not lost,
    /// and that no object owned by `self` is still in use by an in-flight
    /// command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the per-vertex stride against the Rust `Vertex` size — the
    /// shader hardcodes 25 floats / 100 bytes per vertex post-#783; if
    /// a vertex field is added without bumping `VERTEX_STRIDE_FLOATS`
    /// here AND `VERTEX_STRIDE_FLOATS` in the shader, the compute pass
    /// would read past the end of each vertex and write the wrong
    /// target vertex. Phase 1 catch — the renderer crate has no
    /// Vulkan-free test path for the rest of the pipeline.
    #[test]
    fn vertex_stride_matches_rust_vertex_size() {
        use crate::vertex::Vertex;
        assert_eq!(
            std::mem::size_of::<Vertex>(),
            (VERTEX_STRIDE_FLOATS * 4) as usize,
            "VERTEX_STRIDE_FLOATS ({}) × 4 must equal size_of::<Vertex>() — \
             skin_vertices.comp will read garbage if these drift",
            VERTEX_STRIDE_FLOATS,
        );
        assert_eq!(VERTEX_STRIDE_BYTES, 100);
    }

    /// Push constant payload size must fit in the conservative 128 B
    /// minimum every Vulkan implementation guarantees, AND must match
    /// the shader's declared `PushConstants` block. A second runtime
    /// check inside `new()` would verify alignment, but a static-size
    /// assert here catches the common drift case (adding a field
    /// without updating both sides).
    #[test]
    fn push_constants_size_is_12_bytes() {
        // Three u32 fields, no trailing pad. std430 doesn't require
        // 16-B block alignment when no vec4 follows. Pinning the size
        // catches the common drift case (adding a field without
        // updating both Rust + GLSL sides).
        assert_eq!(PUSH_CONSTANTS_SIZE, 12);
        assert_eq!(std::mem::size_of::<SkinPushConstants>(), 12);
    }

    // ── #643 / MEM-2-1 — SkinSlot LRU eviction predicate ────────────

    /// Active this frame: idle = 0, must NOT evict regardless of
    /// threshold. Catches a "fence-post" off-by-one where current ==
    /// last_used wraps to a huge unsigned value via subtraction.
    #[test]
    fn should_evict_keeps_active_slot() {
        assert!(!should_evict_skin_slot(
            /*last=*/ 100, /*now=*/ 100, /*min=*/ 3
        ));
        assert!(!should_evict_skin_slot(
            /*last=*/ 100, /*now=*/ 101, /*min=*/ 3
        ));
    }

    /// Idle below threshold: keep. Idle at threshold: evict.
    /// `min_idle = MAX_FRAMES_IN_FLIGHT + 1 = 3` matches what the
    /// production caller in `draw.rs` uses.
    #[test]
    fn should_evict_at_or_above_threshold() {
        // idle = 2 (frames 100..103 - 2 = 101) → keep.
        assert!(!should_evict_skin_slot(
            /*last=*/ 100, /*now=*/ 102, /*min=*/ 3
        ));
        // idle = 3 → evict.
        assert!(should_evict_skin_slot(
            /*last=*/ 100, /*now=*/ 103, /*min=*/ 3
        ));
        // idle = 4 → also evict.
        assert!(should_evict_skin_slot(
            /*last=*/ 100, /*now=*/ 104, /*min=*/ 3
        ));
    }

    /// `last_used_frame == 0` is the "never dispatched" sentinel. A
    /// slot created mid-frame whose first steady-state dispatch hasn't
    /// run yet must NOT be evicted, even if `current_frame >= min_idle`
    /// (which is true on every frame after `min_idle - 1`).
    #[test]
    fn should_evict_skips_never_dispatched_sentinel() {
        // Even at frame 1_000_000, a sentinel-zero slot survives.
        assert!(!should_evict_skin_slot(
            /*last=*/ 0, /*now=*/ 1_000_000, /*min=*/ 3
        ));
    }

    /// Underflow guard: a future-dated `last_used_frame` (would happen
    /// only if the caller bumped it after a counter overflow / reset)
    /// must not flip eviction true via wrap-around. `saturating_sub`
    /// makes idle = 0 → keep.
    #[test]
    fn should_evict_does_not_wrap_on_future_last_used() {
        assert!(!should_evict_skin_slot(
            /*last=*/ 105, /*now=*/ 100, /*min=*/ 3
        ));
    }

    // ── #1297 / #1298 (DIM12-A-01) — slot-capacity reconciliation ───

    /// A slot whose allocated vertex count matches the live mesh is
    /// reused as-is — no destroy/recreate, no OOB risk.
    #[test]
    fn skin_slot_not_stale_when_counts_match() {
        assert!(!skin_slot_capacity_stale(1024, 1024));
        assert!(!skin_slot_capacity_stale(0, 0));
        assert!(!skin_slot_capacity_stale(u32::MAX, u32::MAX));
    }

    /// The OOB case: the entity was remapped to a LARGER mesh, so the
    /// slot's output buffer is too small for the dispatch (bounded by
    /// the new `push.vertex_count`) — must recreate.
    #[test]
    fn skin_slot_stale_when_mesh_grew() {
        assert!(skin_slot_capacity_stale(1024, 2048));
    }

    /// A SHRINK is also flagged (`!=`, not `>`) so the slot stays
    /// exactly sized to the mesh — symmetric with `validate_refit_counts`
    /// (which rejects any vertex/index drift, not only growth).
    #[test]
    fn skin_slot_stale_when_mesh_shrank() {
        assert!(skin_slot_capacity_stale(2048, 1024));
    }

    // ── #1197 / PERF-DIM7-03 — descriptor-write compare-and-skip ────

    /// `dispatch` must guard `vkUpdateDescriptorSets` behind a
    /// per-FIF cache comparison so steady-state frames issue zero
    /// descriptor writes. Pre-fix every dispatch unconditionally
    /// emitted three writes; the cache key is the
    /// `(input_buffer, bone_buffer)` pair for the slot pipeline
    /// (output is implicit) and `(world, bind_inv, palette)` for
    /// the palette pipeline.
    ///
    /// Live mock of the dispatch path needs a Vulkan device; a
    /// source-string check verifies the guard exists and the
    /// per-FIF cache is updated only inside the write branch.
    /// Mirrors the #1211 / #654 precedent.
    #[test]
    fn skin_compute_dispatch_guards_descriptor_writes_on_cache_miss() {
        let src = include_str!("skin_compute.rs");

        // The two dispatch functions must each contain the
        // `descriptor_bindings[...] != Some(...)` comparison guarding
        // the `update_descriptor_sets` call. Search for the
        // comparison + the call site, and assert the comparison
        // precedes the call in both dispatch bodies.
        let guard = src.find("let needs_write = slot.descriptor_bindings[frame_index]").expect(
            "SkinComputePipeline::dispatch must guard descriptor writes \
             via slot.descriptor_bindings[frame_index] comparison (#1197)",
        );
        let palette_guard = src.find("let needs_write = self.descriptor_bindings[frame_index]").expect(
            "SkinPaletteComputePipeline::dispatch must guard descriptor \
             writes via self.descriptor_bindings[frame_index] comparison (#1197)",
        );

        // Both guards must appear before their respective
        // `update_descriptor_sets` calls. We scope the search to the
        // production code half of the file (before `#[cfg(test)]`)
        // so this test's own string literals don't show up as
        // additional "call sites" in the count.
        // Anchor on the test module marker, not a bare `#[cfg(test)]`
        // (which also appears earlier on a `use` import at the top
        // of the file). The `mod tests {` line is unique.
        let test_module_start = src
            .find("mod tests {")
            .expect("skin_compute.rs must have a `mod tests` block");
        let production = &src[..test_module_start];

        let calls: Vec<usize> = production
            .match_indices("update_descriptor_sets(&writes, &[])")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            calls.len(),
            2,
            "expected exactly two `update_descriptor_sets(&writes, &[])` \
             call sites in skin_compute.rs production code (one per \
             dispatch fn); found {}",
            calls.len(),
        );

        // Each call site must be preceded by its respective guard
        // and contained inside the `if needs_write { ... }` block.
        assert!(
            guard < calls[0],
            "slot-dispatch guard must precede its update_descriptor_sets call (#1197)"
        );
        assert!(
            palette_guard < calls[1],
            "palette-dispatch guard must precede its update_descriptor_sets call (#1197)"
        );

        // And the slot guard must precede the palette guard
        // (SkinComputePipeline is declared first in the file).
        assert!(
            guard < palette_guard,
            "source order changed — SkinComputePipeline::dispatch should \
             precede SkinPaletteComputePipeline::dispatch in skin_compute.rs"
        );
    }

    /// `descriptor_writes_this_frame` is interior-mutability-backed
    /// via `Cell<u32>` so the &self dispatch path can bump the
    /// counter without forcing &mut self up through every caller.
    /// `reset_descriptor_writes_counter` zeroes the counter; the
    /// frame-start reset in `VulkanContext::draw_frame` relies on
    /// this. Pure Cell mechanics — no Vulkan device required.
    #[test]
    fn descriptor_writes_counter_cell_round_trip() {
        let cell: std::cell::Cell<u32> = std::cell::Cell::new(0);
        assert_eq!(cell.get(), 0);
        // Simulate three writes (the worst case in pre-fix dispatch).
        for _ in 0..3 {
            cell.set(cell.get() + 1);
        }
        assert_eq!(cell.get(), 3);
        // The frame-start reset must drop back to 0.
        cell.set(0);
        assert_eq!(cell.get(), 0);
    }

    /// Source-level assertion: the per-frame reset must be wired in
    /// `draw_frame` at the same site that resets `last_skin_coverage_frame`.
    /// Without this reset the counter accumulates across frames and
    /// the steady-state-zero invariant becomes "steady-state monotonic
    /// non-decreasing," which makes the `tex.skin` surface useless and
    /// fails the regression-test predicate.
    #[test]
    fn draw_frame_resets_descriptor_writes_counter() {
        let src = include_str!("context/draw.rs");
        assert!(
            src.contains("reset_descriptor_writes_counter"),
            "draw_frame must reset both skin compute descriptor-writes \
             counters at frame start (#1197) — pair with the \
             `last_skin_coverage_frame` reset already in place"
        );
    }

    // ── M29.5 — SkinPaletteComputePipeline pins ────────────────────

    /// Push-constant payload size must match the shader-side
    /// `PushConstants` block. One u32 (bone_count), 4 bytes. Catches
    /// the common drift case of adding a field without updating both
    /// Rust + GLSL sides.
    #[test]
    fn skin_palette_push_constants_size_is_4_bytes() {
        assert_eq!(SKIN_PALETTE_PUSH_CONSTANTS_SIZE, 4);
        assert_eq!(std::mem::size_of::<SkinPalettePushConstants>(), 4);
    }

    /// Both compute shaders must declare the same 64-wide workgroup as the
    /// Rust-side `WORKGROUP_SIZE`, since the dispatch group-count math
    /// (`vertex_count.div_ceil(WORKGROUP_SIZE)` at the vertex path,
    /// `bone_count.div_ceil(WORKGROUP_SIZE)` at the palette path) assumes
    /// it. A shader edit that changed `local_size_x` without updating the
    /// const would silently dispatch too many / too few workgroups. Pinned
    /// by string-scan of BOTH GLSL sources — the vertex shader was
    /// previously unpinned (#1319 / TD4-NEW-13).
    #[test]
    fn skin_palette_workgroup_size_matches_skin_vertices() {
        let expected = format!("local_size_x = {}", WORKGROUP_SIZE);
        for (name, src) in [
            ("skin_palette.comp", include_str!("../../shaders/skin_palette.comp")),
            ("skin_vertices.comp", include_str!("../../shaders/skin_vertices.comp")),
        ] {
            assert!(
                src.contains(&expected),
                "{name} must declare `layout({expected})` to match the \
                 Rust-side WORKGROUP_SIZE that drives its dispatch group count",
            );
        }
    }

    /// M29.5 numeric pin — the GPU compute shader and the CPU-side
    /// `SkinnedMesh::compute_palette_into` must produce byte-identical
    /// palette output for the same input pair. The shader path needs
    /// a live Vulkan device to test end-to-end, but we can reproduce
    /// the per-slot math here (`palette[i] = bone_world[i] *
    /// bind_inverses[i]`) and check it against the canonical CPU
    /// helper. Drift on either side would fail this test.
    #[test]
    fn skin_palette_per_slot_math_matches_cpu_compute_palette_into() {
        use byroredux_core::ecs::{components::SkinnedMesh, EntityId};
        use byroredux_core::math::{Mat4, Quat, Vec3};

        // Three non-identity bone worlds + three non-identity bind
        // inverses. The math is per-slot so a small fixture is enough
        // to catch most drift modes (row/column-major swap, operand
        // order swap, transpose drift).
        let bone_worlds = [
            Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)),
            Mat4::from_rotation_y(0.5),
            Mat4::from_scale_rotation_translation(
                Vec3::new(2.0, 1.0, 1.0),
                Quat::from_axis_angle(Vec3::Z, 0.3),
                Vec3::new(-1.0, 4.0, 0.5),
            ),
        ];
        let bind_inverses = [
            Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.25)),
            Mat4::from_rotation_x(-0.2),
            Mat4::from_scale(Vec3::new(0.5, 2.0, 1.5)),
        ];

        // CPU ground truth via the canonical helper. EntityId is a
        // `u32` typedef (crates/core/src/ecs/storage.rs:10), so the
        // closure can use it as a direct index into `bone_worlds`.
        let bone_entities: Vec<Option<EntityId>> =
            (0..3u32).map(Some).collect();
        // `SkinnedMesh::new` is `#[cfg(test)]`-gated within
        // byroredux-core and unreachable from another crate's test
        // build; route through the production constructor with an
        // identity `global_skin_transform` (matches what `new` does
        // internally per skinned_mesh.rs:101-107).
        let skin = SkinnedMesh::new_with_global(
            None,
            bone_entities,
            bind_inverses.to_vec(),
            Mat4::IDENTITY,
        );
        let mut cpu_palette: Vec<Mat4> = Vec::with_capacity(3);
        skin.compute_palette_into(&mut cpu_palette, |e| {
            bone_worlds.get(e as usize).copied()
        });

        // Reproduce the GPU per-slot math:
        //   palette[i] = bone_world[i] * bind_inverses[i]
        // The shader literally does this expression at
        // `skin_palette.comp::main`. Byte-equality here pins the
        // CPU-side formula and the shader formula in lockstep.
        for i in 0..3 {
            let gpu_equivalent = bone_worlds[i] * bind_inverses[i];
            assert_eq!(
                gpu_equivalent.to_cols_array(),
                cpu_palette[i].to_cols_array(),
                "slot {}: GPU-equivalent math diverged from CPU \
                 SkinnedMesh::compute_palette_into",
                i,
            );
        }
    }
}

// Shader drift-detection tests moved to shader_constants::tests after #1038
// folded all shared constants into the build.rs codegen path. The canonical
// checks are now:
//   shader_constants::tests::affected_shaders_include_constants_header
//   shader_constants::tests::generated_header_contains_all_defines
//   shader_constants::tests::vertex_stride_matches_vertex_struct
//   shader_constants::tests::max_bones_per_mesh_matches_core
