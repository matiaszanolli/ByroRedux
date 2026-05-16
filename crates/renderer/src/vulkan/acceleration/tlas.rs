//! TLAS build and access.
//!
//! Single top-level acceleration structure rebuilt each frame from all
//! draw instances. Double-buffered per frame-in-flight. The
//! BUILD-vs-UPDATE decision lives in [`super::predicates::decide_use_update`].

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::constants::{MIN_TLAS_INSTANCE_RESERVE, UPDATABLE_AS_FLAGS};
use super::predicates::{
    column_major_to_vk_transform, decide_use_update, draw_command_eligible_for_tlas,
    is_scratch_aligned, scratch_needs_growth, shrink_scratch_if_oversized,
};
use super::types::TlasState;
use super::AccelerationManager;
use crate::vulkan::context::DrawCommand;
use anyhow::{Context, Result};
use ash::vk;

impl AccelerationManager {
    /// Build or rebuild the TLAS from draw commands for a specific frame-in-flight slot.
    ///
    /// Each frame slot has its own TLAS resources (accel structure, instance buffer,
    /// scratch buffer), so overlapping frames cannot interfere. The caller's fence
    /// wait guarantees the previous use of this slot is complete.
    ///
    /// `instance_map[i]` is `Some(ssbo_idx)` when `draw_commands[i]` is present
    /// in the compacted SSBO produced by the draw-frame builder, or `None`
    /// when the draw command was filtered out (e.g. the mesh handle no longer
    /// resolves). `instance_custom_index` is set from this map so the shader
    /// always indexes a valid SSBO entry regardless of which filter rejected
    /// a draw command. Before #419 the TLAS encoded the raw enumerate index
    /// here, which diverged from the SSBO's compacted index the moment any
    /// filter rejected anything — silent material/transform corruption on
    /// every RT hit downstream.
    ///
    /// Records commands into `cmd` — caller must ensure a memory barrier after.
    pub unsafe fn build_tlas(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        draw_commands: &[DrawCommand],
        instance_map: &[Option<u32>],
        frame_index: usize,
    ) -> Result<()> {
        // Advance the frame counter for LRU tracking.
        self.frame_counter += 1;

        debug_assert_eq!(
            instance_map.len(),
            draw_commands.len(),
            "instance_map must be 1:1 with draw_commands (see #419)"
        );

        // Build instance array. `instance_custom_index` comes from the shared
        // `instance_map` so it matches the SSBO position exactly — the TLAS
        // can still be sparse (missing BLAS drop instances, particle / UI
        // draws with `in_tlas = false`), but the shader's
        // `rayQueryGetIntersectionInstanceCustomIndexEXT` is guaranteed to
        // land on the right SSBO entry. Pre-#516 `in_tlas` was also flipped
        // off for out-of-frustum entities; now frustum culling only gates
        // rasterization (`in_raster`) and off-screen occluders stay in
        // the TLAS so on-screen fragments' shadow / reflection / GI rays
        // hit them. See #419 + #516.
        let mut instances = std::mem::take(&mut self.tlas_instances_scratch);
        instances.clear();
        instances.reserve(draw_commands.len());
        // Diagnostic counter for the warn-rate-limited log below
        // (#678 / AS-8-6). Counts ONLY draws that opted into TLAS
        // inclusion but couldn't get an instance emitted — i.e.
        // genuine RT-shadow regressions. Pre-fix this was derived
        // from `draw_commands.len() - instances.len()`, which
        // bundled in `!in_tlas` skips (particles, UI quad — by
        // design rasterized but not in TLAS). A frame with 200
        // particle draws would spam the warning every second
        // suggesting an RT regression that didn't exist.
        let mut missing_blas: usize = 0;
        // REN-D8-NEW-14 — capture the first few offenders so the
        // warn-rate-limited log below identifies which meshes /
        // entities are dropping out of the TLAS instead of just
        // reporting a count. Pre-#926 the warn fired "N lack BLAS"
        // with no hint of which N, so chasing an RT regression
        // required adding ad-hoc logs every time. Bounded sample to
        // keep the log line readable; the count above stays exact.
        const MISSING_BLAS_SAMPLE_LIMIT: usize = 5;
        let mut missing_samples: Vec<String> = Vec::new();
        for (i, draw_cmd) in draw_commands.iter().enumerate() {
            // Two-axis eligibility (#516 + #1024 / F-WAT-03):
            //  - `in_tlas == false` skips particles / UI quads / other
            //    rasterized-only draws.
            //  - `is_water == true` skips water surfaces so water rays
            //    don't hit the water plane itself. Sibling contract on
            //    `DrawCommand::is_water` (see its doc-comment).
            // See [`draw_command_eligible_for_tlas`] for the pinned
            // predicate that the unit test pins this contract against.
            if !draw_command_eligible_for_tlas(draw_cmd) {
                continue;
            }
            // M29 Phase 2 — skinned draws (`bone_offset != 0`) reference
            // a per-entity BLAS that's refit each frame against the
            // SkinComputePipeline output buffer. Look up by entity_id
            // first; rigid draws fall through to the per-mesh
            // `blas_entries` table. The skinned-BLAS path keeps the
            // same `last_used_frame` LRU bump as the static path so a
            // skinned NPC dropped from the draw list ages out
            // alongside its mesh.
            let blas_address: vk::DeviceAddress = if draw_cmd.bone_offset != 0 {
                let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) else {
                    // Skinned entity hasn't had its BLAS built yet
                    // (first sight is processed earlier in the same
                    // draw_frame; this gate is defensive — if we get
                    // here the entity will be invisible to RT this
                    // frame, but raster's inline-skinning path still
                    // renders it correctly).
                    missing_blas += 1;
                    if missing_samples.len() < MISSING_BLAS_SAMPLE_LIMIT {
                        missing_samples
                            .push(format!("skinned entity {:?} (no BLAS)", draw_cmd.entity_id));
                    }
                    continue;
                };
                entry.last_used_frame = self.frame_counter;
                entry.device_address
            } else {
                let mesh_handle = draw_cmd.mesh_handle as usize;
                let Some(Some(blas)) = self.blas_entries.get_mut(mesh_handle) else {
                    missing_blas += 1;
                    if missing_samples.len() < MISSING_BLAS_SAMPLE_LIMIT {
                        missing_samples
                            .push(format!("rigid mesh_handle={} (no BLAS)", mesh_handle));
                    }
                    continue;
                };
                blas.last_used_frame = self.frame_counter;
                blas.device_address
            };
            // Skip commands that the SSBO builder also skipped. This
            // keeps the two filters in lockstep even when `blas_entries`
            // and `mesh_registry` diverge (e.g. a BLAS briefly survives
            // its source mesh during eviction).
            let Some(ssbo_idx) = instance_map.get(i).copied().flatten() else {
                missing_blas += 1;
                if missing_samples.len() < MISSING_BLAS_SAMPLE_LIMIT {
                    missing_samples.push(format!(
                        "mesh_handle={} (no SSBO instance — evicted?)",
                        draw_cmd.mesh_handle
                    ));
                }
                continue;
            };

            // Convert column-major model_matrix [f32; 16] to
            // VkTransformMatrixKHR (3x4 row-major). See
            // `column_major_to_vk_transform` for the layout pin.
            let transform = column_major_to_vk_transform(&draw_cmd.model_matrix);

            // SAFETY: AccelerationStructureReferenceKHR is a union — device_handle field
            // is used because our BLAS is on-device (not host-built). The address was
            // obtained from get_acceleration_structure_device_address after BLAS creation.
            //
            // Gate TRIANGLE_FACING_CULL_DISABLE on `draw_cmd.two_sided` so RT
            // traversal matches what the rasterizer renders. Pre-#416 every
            // instance disabled backface culling, so shadow / GI rays hit the
            // interior backfaces of closed single-sided meshes (rooms,
            // buildings) from outside — self-shadowing on far walls, ~2× ray
            // cost on closed meshes. The `two_sided` bit already rides on
            // `DrawCommand` (set from NiTriShape's NIF properties) and the
            // rasterizer pipeline cache keys on it via `PipelineKey`; the RT
            // path now honors the same bit.
            let instance_flags = if draw_cmd.two_sided {
                vk::GeometryInstanceFlagsKHR::TRIANGLE_FACING_CULL_DISABLE.as_raw()
            } else {
                0
            };
            // #957 / REN-D8-NEW-13 — `instance_custom_index` is a 24-bit
            // field in the Vulkan AS-instance struct. `Packed24_8::new`
            // silently truncates anything ≥ 2^24 = 16 777 216, which would
            // re-route every RT hit's SSBO lookup to the wrong instance and
            // silently corrupt material / transform reads.
            //
            // Unreachable today: `MAX_INSTANCES = 0x40000` (262 144,
            // `scene_buffer/constants.rs`) is the upstream cap, enforced by
            // the `RP-1` assert at `context/draw.rs::draw_frame`. That's a
            // ~64× margin below the 24-bit ceiling. The invariant lives in
            // a different file from the truncation site though, so a future
            // `MAX_INSTANCES` bump past 2^24 (large open-world streaming
            // ambitions, M40 Phase 2+) wouldn't catch this gap. Mirror the
            // RP-1 assert here so the 24-bit invariant is documented and
            // enforced at the truncation site itself.
            debug_assert!(
                ssbo_idx < (1u32 << 24),
                "REN-D8-NEW-13: ssbo_idx {ssbo_idx} exceeds 24-bit \
                 instance_custom_index ceiling (2^24 = 16 777 216). \
                 Either MAX_INSTANCES was bumped past 2^24 without \
                 partitioning the TLAS, or the build_instance_map \
                 upstream cap drifted. See #957.",
                ssbo_idx = ssbo_idx,
            );
            instances.push(vk::AccelerationStructureInstanceKHR {
                transform,
                // #419 — SSBO-compacted index from the shared map, NOT
                // the raw enumerate index. The 24-bit field holds the
                // `instances[ssbo_idx]` position the shader reads via
                // `rayQueryGetIntersectionInstanceCustomIndexEXT`.
                //
                // Mask 0xFF: every instance is hit by every ray. The
                // 8-bit mask is AND'd against `cullMask` at
                // `rayQueryInitializeEXT` time, so per-light-type
                // segregation (e.g. shadow rays skipping transparent
                // foliage, or directional vs point shadow buckets)
                // could light up by handing instances bucket-specific
                // bit masks and the corresponding ray sites narrower
                // cullMask values. Today every shader passes 0xFF and
                // the lighting model doesn't need the segregation; the
                // extension point is the mask byte here. See
                // REN-D8-NEW-07 (audit 2026-05-09).
                instance_custom_index_and_mask: vk::Packed24_8::new(ssbo_idx, 0xFF),
                instance_shader_binding_table_record_offset_and_flags: vk::Packed24_8::new(
                    0,
                    instance_flags as u8,
                ),
                acceleration_structure_reference: vk::AccelerationStructureReferenceKHR {
                    device_handle: blas_address,
                },
            });
        }

        let instance_count = instances.len() as u32;
        if missing_blas > 0 && frame_index == 0 {
            // Log once per second (at 60fps, frame_index 0 fires 30×/s — good enough).
            static LAST_LOG: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let prev = LAST_LOG.load(std::sync::atomic::Ordering::Relaxed);
            if now != prev {
                LAST_LOG.store(now, std::sync::atomic::Ordering::Relaxed);
                let sample = if missing_samples.is_empty() {
                    String::new()
                } else {
                    format!(
                        " [first {} offender{}: {}{}]",
                        missing_samples.len(),
                        if missing_samples.len() == 1 { "" } else { "s" },
                        missing_samples.join("; "),
                        if missing_blas > missing_samples.len() {
                            "; ..."
                        } else {
                            ""
                        },
                    )
                };
                log::warn!(
                    "TLAS: {} instances from {} draw commands ({} lack BLAS — no RT shadows for those meshes){}",
                    instance_count, draw_commands.len(), missing_blas, sample
                );
            }
        }

        // Even with 0 instances, we build a valid (empty) TLAS so the
        // descriptor set binding is always valid for the shader.

        // Create/resize instance buffer if needed for this frame slot.
        let need_new_tlas = self.tlas[frame_index].is_none()
            || self.tlas[frame_index].as_ref().unwrap().max_instances < instance_count;

        if need_new_tlas {
            // Destroy old TLAS for this frame slot.
            //
            // INVARIANT — load-bearing: `draw_frame` calls
            // `wait_for_fences` on both this slot's and the previous
            // slot's `in_flight` fences BEFORE reaching this site
            // (`context/draw.rs::draw_frame` fence-wait block, ~line
            // 158). The double-fence wait guarantees no command
            // buffer still references the resources we're about to
            // destroy, so `device_wait_idle` here would only
            // duplicate work. Pre-fix this comment claimed the fence
            // wait covers it without naming the invariant; if a
            // future refactor moves the fence wait or splits the
            // pair, this resize path silently destroys live TLAS
            // resources. Add a defensive `device.device_wait_idle()`
            // here if either changes. See REN-D2-NEW-04 (audit
            // 2026-05-09).
            if let Some(mut old) = self.tlas[frame_index].take() {
                log::info!(
                    "TLAS[{frame_index}] resize: {} → {} instances",
                    old.max_instances,
                    instance_count,
                );
                self.accel_loader
                    .destroy_acceleration_structure(old.accel, None);
                old.buffer.destroy(device, allocator);
                old.instance_buffer.destroy(device, allocator);
                old.instance_buffer_device.destroy(device, allocator);
            }

            // Pre-size generously to avoid future resizes. 8192 covers
            // interior cells (~200-800) and large exterior cells (~3000-5000).
            // Growth: 2x current requirement, minimum 8192.
            //
            // The 2× + 8192-floor strategy intentionally over-allocates
            // — a 200-instance interior gets 8192-slot backing
            // (~660 KB BAR), a 3000-instance exterior gets 8192
            // (still ~660 KB), and only past 4096 does the 2× term
            // dominate. The trade-off: each TLAS resize destroys
            // both the staging + device-local instance buffers and
            // recreates them, including a fresh allocator slot lookup
            // and host→device staging — collectively ~50 µs per
            // resize. Over-allocating to amortise resizes away costs
            // a fixed ~660 KB of BAR per TLAS slot in the typical
            // case (well under 1 MB total across both FIF slots).
            // See REN-D8-NEW-10 (audit 2026-05-09).
            //
            // REN-D2-NEW-02 (audit 2026-05-09) flagged the 8192 floor
            // as wasting ~1 MB BAR on interior cells with < 100
            // instances. The waste is real (8192 slots × 88 B per
            // instance × 2 FIF = ~1.4 MB across both slots) but the
            // floor stays for the cell-streaming case: M40
            // exterior-tile loads frequently transition through low-
            // instance frames between high-instance cell pairs, and
            // a per-cell-tuned floor would resize every transition.
            // The 8192 floor is the right knob for that pattern; the
            // 1 MB BAR cost is a fraction of the per-FIF scene
            // buffer total (~10-20 MB).
            let padded_count =
                ((instance_count as usize) * 2).max(MIN_TLAS_INSTANCE_RESERVE as usize);
            let padded_size = (std::mem::size_of::<vk::AccelerationStructureInstanceKHR>()
                * padded_count) as vk::DeviceSize;

            // Host-visible staging buffer for CPU writes.
            let mut instance_buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                padded_size,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?;

            // Device-local buffer for GPU reads during AS build. On discrete
            // GPUs this avoids PCIe traversal (~10-30x faster). See #289.
            let mut instance_buffer_device = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                padded_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                    | vk::BufferUsageFlags::TRANSFER_DST,
            )?;

            let instance_address = device.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(instance_buffer_device.buffer),
            );

            // Query TLAS sizes.
            //
            // `GeometryFlagsKHR::OPAQUE` on `INSTANCES` is redundant with
            // the `gl_RayFlagsOpaqueEXT` set at every `rayQueryInitialize`
            // call site in triangle.frag — the ray-query flag forces
            // opaque traversal regardless of geometry/instance flags. We
            // keep the flag set anyway for stylistic parity with the BLAS
            // build paths (which need it for the any-hit-elision path),
            // and so a future change that drops `gl_RayFlagsOpaqueEXT`
            // doesn't silently un-opaque the TLAS. REN-D8-NEW-01
            // (audit `2026-05-09`).
            let geometry = vk::AccelerationStructureGeometryKHR::default()
                .geometry_type(vk::GeometryTypeKHR::INSTANCES)
                .flags(vk::GeometryFlagsKHR::OPAQUE)
                .geometry(vk::AccelerationStructureGeometryDataKHR {
                    instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                        .array_of_pointers(false)
                        .data(vk::DeviceOrHostAddressConstKHR {
                            device_address: instance_address,
                        }),
                });

            // Shared `UPDATABLE_AS_FLAGS` (PREFER_FAST_TRACE | ALLOW_UPDATE):
            // REFIT (#247) handles most per-frame TLAS changes, so full
            // rebuilds are rare and the trace-time wins from a higher-
            // quality BVH pay off on every ray query (shadows, reflections,
            // GI, caustics, window portal). The UPDATE counterpart below
            // must read the same flag set per VUID-…-pInfos-03667; the
            // shared constant enforces that. See #307 / #958 /
            // REN-D8-NEW-14 + AUDIT_PERFORMANCE_2026-04-13b P1-09.
            let build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
                .flags(UPDATABLE_AS_FLAGS)
                .mode(vk::BuildAccelerationStructureModeKHR::BUILD)
                .geometries(std::slice::from_ref(&geometry));

            let mut sizes = vk::AccelerationStructureBuildSizesInfoKHR::default();
            self.accel_loader.get_acceleration_structure_build_sizes(
                vk::AccelerationStructureBuildTypeKHR::DEVICE,
                &build_info,
                &[padded_count as u32],
                &mut sizes,
            );
            // Scratch is sized for BUILD which is >= UPDATE per Vulkan
            // spec, so the same buffer serves both modes.

            // DEVICE_LOCAL: GPU-built, GPU-read during ray queries.
            let mut tlas_buffer = GpuBuffer::create_device_local_uninit(
                device,
                allocator,
                sizes.acceleration_structure_size,
                vk::BufferUsageFlags::ACCELERATION_STRUCTURE_STORAGE_KHR
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            )
            .inspect_err(|_| {
                instance_buffer.destroy(device, allocator);
                instance_buffer_device.destroy(device, allocator);
            })?;

            let accel_info = vk::AccelerationStructureCreateInfoKHR::default()
                .buffer(tlas_buffer.buffer)
                .size(sizes.acceleration_structure_size)
                .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL);

            let accel = self
                .accel_loader
                .create_acceleration_structure(&accel_info, None)
                .inspect_err(|_| {
                    tlas_buffer.destroy(device, allocator);
                    instance_buffer.destroy(device, allocator);
                    instance_buffer_device.destroy(device, allocator);
                })
                .context("Failed to create TLAS")?;

            // Record this build's scratch requirement on the slot
            // (#682 / MEM-2-7). The fresh-create path is the ONLY site
            // that re-runs `vkGetAccelerationStructureBuildSizesKHR`
            // for TLAS — refit/update reuse the existing scratch on
            // the spec guarantee `BUILD ≥ UPDATE`. So this is the
            // canonical peak for the slot's lifetime, written
            // unconditionally even if the existing scratch buffer is
            // big enough to skip realloc below: a previous-slot's
            // larger peak shouldn't permanently inflate a smaller
            // current-slot's recorded peak.
            self.tlas_scratch_peak_bytes[frame_index] = sizes.build_scratch_size;

            // Grow-only per-frame scratch buffer (#424 SIBLING) — reuse
            // the existing allocation when it still fits the new build.
            // DEVICE_LOCAL: GPU-only scratch during TLAS build. The
            // `scratch_data.device_address` alignment requirement is
            // checked at the call site below via
            // `debug_assert_scratch_aligned` (#659 / #260 R-05).
            let needs_new_scratch = scratch_needs_growth(
                self.scratch_buffers[frame_index].as_ref().map(|b| b.size),
                sizes.build_scratch_size,
            );
            if needs_new_scratch {
                if let Some(mut old_scratch) = self.scratch_buffers[frame_index].take() {
                    old_scratch.destroy(device, allocator);
                }
                let scratch_result = GpuBuffer::create_device_local_uninit(
                    device,
                    allocator,
                    sizes.build_scratch_size,
                    vk::BufferUsageFlags::STORAGE_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                );
                match scratch_result {
                    Ok(scratch) => self.scratch_buffers[frame_index] = Some(scratch),
                    Err(e) => {
                        self.accel_loader
                            .destroy_acceleration_structure(accel, None);
                        tlas_buffer.destroy(device, allocator);
                        instance_buffer.destroy(device, allocator);
                        instance_buffer_device.destroy(device, allocator);
                        return Err(e);
                    }
                }
            }

            self.tlas[frame_index] = Some(TlasState {
                accel,
                buffer: tlas_buffer,
                instance_buffer,
                instance_buffer_device,
                max_instances: padded_count as u32,
                last_blas_addresses: Vec::with_capacity(padded_count),
                // A freshly-created TLAS has no source to refit from —
                // the first frame after creation must do a full BUILD.
                needs_full_rebuild: true,
                // Sentinel that no real generation can match — forces
                // the first build_tlas after (re)creation to take the
                // gen-mismatch short-circuit, skipping the zip-compare
                // since `last_blas_addresses` is empty anyway.
                last_blas_map_gen: u64::MAX,
                // 0 signals "no prior BUILD" — the first build_tlas call
                // will always use BUILD mode (via needs_full_rebuild) and
                // set this to instance_count. (#1083)
                built_primitive_count: 0,
            });
        }

        // Capture scratch_align before the mutable borrow on
        // `self.tlas[frame_index]` so the alignment assert further down
        // doesn't try to re-borrow `&self` (#659).
        let scratch_align = self.scratch_align;

        let tlas = self.tlas[frame_index].as_mut().unwrap();

        // Decide BUILD vs UPDATE (#247). REFIT (UPDATE) is legal only
        // when the per-instance BLAS references are unchanged from the
        // last BUILD. Transforms, custom indices, SBT offsets, masks,
        // and flags can all change and still be refitted; only the
        // `acceleration_structure_reference` field is off-limits.
        //
        // Gate: if `needs_full_rebuild` is set (freshly created or
        // previous frame BUILT), or the BLAS map mutated since the
        // last build (#300 dirty flag), or the current BLAS address
        // sequence differs from the last BUILD, we BUILD. Otherwise
        // UPDATE. The dirty-flag short-circuit lets cell load /
        // unload / eviction frames skip the O(N) per-instance
        // zip-compare entirely — the gen mismatch already proves the
        // address sequence could have shifted.
        let map_gen = self.blas_map_generation;
        // Materialise the current address sequence as `&[u64]` so the
        // pure decision helper can compare it without re-reading the
        // union field — same invariant as the push loop below.
        // SAFETY: AccelerationStructureReferenceKHR is a union; our
        // BLAS entries are always device-built so `device_handle` is
        // the live variant on every push site in this manager.
        let mut current_addresses_scratch = std::mem::take(&mut self.tlas_addresses_scratch);
        current_addresses_scratch.clear();
        current_addresses_scratch.reserve(instances.len());
        for inst in &instances {
            current_addresses_scratch
                .push(unsafe { inst.acceleration_structure_reference.device_handle });
        }
        let (mut use_update, _did_zip) = decide_use_update(
            tlas.needs_full_rebuild,
            tlas.last_blas_map_gen,
            map_gen,
            &tlas.last_blas_addresses,
            &current_addresses_scratch,
        );

        // VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03708: UPDATE must
        // use the same primitiveCount as the source BUILD. When instance_count
        // grows beyond what the last BUILD declared, we must force a full BUILD
        // before the count increase becomes a VUID violation. This path fires
        // only while `instance_count ∈ (built_primitive_count, max_instances]`
        // — the TLAS already has capacity so no resize is needed, but the
        // count mismatch would corrupt the BVH on NVIDIA / trip validation on
        // debug builds. See #1083 / REN-D8-001.
        if use_update && instance_count > tlas.built_primitive_count {
            use_update = false;
        }

        // Promote this frame's addresses to be next frame's "last", and
        // recover the previous "last" Vec into the manager-level scratch
        // for next frame to refill (#660). Pre-fix this was a fresh
        // `Vec::with_capacity(N)` per frame — 64 KB heap churn at the
        // 8k-instance ceiling, 3.84 MB/s at 60 FPS, all to feed a 4-byte
        // bool. Swap is allocation-free: each TLAS slot's Vec ping-pongs
        // with the manager scratch.
        std::mem::swap(
            &mut tlas.last_blas_addresses,
            &mut current_addresses_scratch,
        );
        // #914 / REN-D8-NEW-04 — invariant guard. `last_blas_addresses`
        // is consumed by next frame's `decide_use_update` zip against
        // the freshly-rebuilt `current_addresses_scratch`, and by the
        // build-info `primitive_count` on UPDATE-mode rebuilds. Any
        // future "skip empty tail instances" / partial-instance
        // optimisation that desyncs the two would silently produce a
        // `primitiveCount`-mismatch on next frame's UPDATE call (a
        // validation-layer error in debug, garbage TLAS contents in
        // release). The cached addresses were just swapped *out* of
        // `current_addresses_scratch`, which was filled from
        // `instances.iter()` in the loop above — so length must equal
        // `instance_count`. Debug-only: zero release-build cost.
        debug_assert_eq!(
            tlas.last_blas_addresses.len(),
            instance_count as usize,
            "TLAS instance bookkeeping desync — UPDATE will fail next frame \
             (last_blas_addresses.len() != instance_count)"
        );
        self.tlas_addresses_scratch = current_addresses_scratch;
        shrink_scratch_if_oversized(&mut self.tlas_addresses_scratch, instances.len(), 512);
        // After this BUILD/UPDATE completes, the next frame can refit
        // unless something invalidates it (resize, layout change).
        tlas.needs_full_rebuild = false;
        tlas.last_blas_map_gen = map_gen;

        // Mark referenced BLAS entries as used for LRU eviction.
        // Skinned draws ride the per-entity skinned_blas table; rigid
        // draws ride the per-mesh blas_entries table. The override
        // mirror in the build loop above kept the same discriminator.
        // Filter is `draw_command_eligible_for_tlas` so water surfaces
        // (#1024) and `!in_tlas` draws stay out of the LRU-bump path
        // in lockstep with the build loop above.
        for draw_cmd in draw_commands {
            if !draw_command_eligible_for_tlas(draw_cmd) {
                continue;
            }
            if draw_cmd.bone_offset != 0 {
                if let Some(entry) = self.skinned_blas.get_mut(&draw_cmd.entity_id) {
                    entry.last_used_frame = self.frame_counter;
                }
            } else {
                let h = draw_cmd.mesh_handle as usize;
                if let Some(Some(ref mut blas)) = self.blas_entries.get_mut(h) {
                    blas.last_used_frame = self.frame_counter;
                }
            }
        }

        // Write instances to host-visible staging buffer.
        //
        // REN-D8-NEW-02 — `write_mapped` performs the
        // `vkFlushMappedMemoryRanges` internally when the allocation
        // isn't HOST_COHERENT. The host→transfer barrier emitted
        // below covers the visibility hop from host writes to the
        // transfer engine; the flush is what makes the writes
        // visible to the device in the first place. Together those
        // two operations are sufficient — no additional flush is
        // needed before the `cmd_copy_buffer` further down.
        tlas.instance_buffer.write_mapped(device, &instances)?;

        let copy_size = (instances.len()
            * std::mem::size_of::<vk::AccelerationStructureInstanceKHR>())
            as vk::DeviceSize;

        // Skip the host→device staging copy entirely when there are no
        // instances this frame (#317 / audit D1-02). Vulkan's spec
        // rejects `VkBufferCopy.size` and `VkBufferMemoryBarrier.size`
        // of 0 (VUID-VkBufferCopy-size-01988 and
        // VUID-VkBufferMemoryBarrier-size-01188), and the spec leaves
        // `size = 0` driver-defined: NVIDIA treats it as a no-op,
        // AMD / Intel historically have not — this guard keeps the
        // path portable across vendors. Pre-fix we tripped four
        // validation errors per empty-TLAS frame (two barriers + one
        // copy per TLAS slot × two frame-in-flight slots). The empty-
        // instance TLAS BUILD below is still legal —
        // `primitiveCount = 0` produces an empty AS that ray queries
        // return "no hit" against.
        if copy_size > 0 {
            // Barrier 1: make host write visible to the transfer engine.
            let host_to_transfer = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::HOST_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                .buffer(tlas.instance_buffer.buffer)
                .offset(0)
                .size(copy_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[host_to_transfer],
                &[],
            );

            // Copy staging → device-local. On discrete GPUs, the AS build
            // reads from VRAM instead of traversing PCIe. See #289.
            let copy_region = vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: copy_size,
            };
            device.cmd_copy_buffer(
                cmd,
                tlas.instance_buffer.buffer,
                tlas.instance_buffer_device.buffer,
                &[copy_region],
            );

            // Barrier 2: transfer write → AS build read on the device-local buffer.
            let transfer_to_as = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR)
                .buffer(tlas.instance_buffer_device.buffer)
                .offset(0)
                .size(copy_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                vk::DependencyFlags::empty(),
                &[],
                &[transfer_to_as],
                &[],
            );
        }

        let instance_address = device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default().buffer(tlas.instance_buffer_device.buffer),
        );

        // `GeometryFlagsKHR::OPAQUE` on `INSTANCES` is redundant — see
        // the matching note at the sizes-query site above. REN-D8-NEW-01.
        let geometry = vk::AccelerationStructureGeometryKHR::default()
            .geometry_type(vk::GeometryTypeKHR::INSTANCES)
            .flags(vk::GeometryFlagsKHR::OPAQUE)
            .geometry(vk::AccelerationStructureGeometryDataKHR {
                instances: vk::AccelerationStructureGeometryInstancesDataKHR::default()
                    .array_of_pointers(false)
                    .data(vk::DeviceOrHostAddressConstKHR {
                        device_address: instance_address,
                    }),
            });

        let scratch_address = device.get_buffer_device_address(
            &vk::BufferDeviceAddressInfo::default()
                .buffer(self.scratch_buffers[frame_index].as_ref().unwrap().buffer),
        );
        debug_assert!(
            is_scratch_aligned(scratch_address, scratch_align),
            "build_tlas: scratch device address {scratch_address:#x} is not aligned to \
             minAccelerationStructureScratchOffsetAlignment ({scratch_align}); see #659"
        );

        // Mirror the flags used at creation time so Vulkan's validation
        // layer matches source and dst flags. The shared
        // `UPDATABLE_AS_FLAGS` constant keeps this site lockstep with
        // the fresh-build path above (VUID-…-pInfos-03667 / #307 / #958).
        let mut build_info = vk::AccelerationStructureBuildGeometryInfoKHR::default()
            .ty(vk::AccelerationStructureTypeKHR::TOP_LEVEL)
            .flags(UPDATABLE_AS_FLAGS)
            .dst_acceleration_structure(tlas.accel)
            .geometries(std::slice::from_ref(&geometry))
            .scratch_data(vk::DeviceOrHostAddressKHR {
                device_address: scratch_address,
            });

        // primitiveCount for the range info:
        // - BUILD: current instance_count; record it so UPDATE can match it.
        // - UPDATE: must equal the BUILD count (VUID-…-pInfos-03708 — the guard
        //   above ensures instance_count ≤ built_primitive_count here).
        let range_primitive_count = if use_update {
            // REFIT path: reuse the existing accel as the source,
            // write the updated instance transforms into the same dst.
            build_info = build_info
                .mode(vk::BuildAccelerationStructureModeKHR::UPDATE)
                .src_acceleration_structure(tlas.accel);
            // Must match the source BUILD's primitiveCount per VUID-03708.
            tlas.built_primitive_count
        } else {
            build_info = build_info.mode(vk::BuildAccelerationStructureModeKHR::BUILD);
            // Record so future UPDATEs know the count they must match.
            tlas.built_primitive_count = instance_count;
            instance_count
        };

        let range = vk::AccelerationStructureBuildRangeInfoKHR::default()
            .primitive_count(range_primitive_count);

        self.accel_loader.cmd_build_acceleration_structures(
            cmd,
            &[build_info],
            &[std::slice::from_ref(&range)],
        );

        // Restore the scratch buffer so its capacity amortizes across
        // frames, then shrink it if a past peak (exterior open cell with
        // 10 k+ draw commands) left us holding 640 KB+ of unused
        // capacity long after the scene returned to a small interior.
        // `instance_count` is the number of entries we actually used
        // this frame; the 512 floor keeps the buffer usefully large
        // for common-case small scenes without reallocating on every
        // cell transition. See #504.
        self.tlas_instances_scratch = instances;
        shrink_scratch_if_oversized(
            &mut self.tlas_instances_scratch,
            instance_count as usize,
            512,
        );

        Ok(())
    }

    /// Get the TLAS acceleration structure handle for a frame slot (for descriptor binding).
    pub fn tlas_handle(&self, frame_index: usize) -> Option<vk::AccelerationStructureKHR> {
        self.tlas[frame_index].as_ref().map(|t| t.accel)
    }
}
