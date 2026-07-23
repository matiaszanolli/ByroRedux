//! M29 GPU pre-skin + per-skinned-entity BLAS refit — extracted from
//! `draw.rs` (#1857 / TD1-001) to shrink that file. Recorded after the
//! bone-palette upload and before the TLAS build (which picks up the
//! freshly-refit BLAS via `self`); the internal `unsafe` scopes,
//! barriers, and recording order are unchanged from the pre-split
//! `draw_frame`.

use super::super::descriptors::memory_barrier;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::{DrawCommand, VulkanContext};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;

impl VulkanContext {
    /// Record the M29 GPU pre-skin + per-skinned-entity BLAS refit into the
    /// open per-frame command buffer (#1748). Extracted verbatim from
    /// `draw_frame` — runs after the bone-palette upload and before the
    /// TLAS build, which picks up the freshly-refit BLAS via `self`. The
    /// internal `unsafe` scopes, barriers, and recording order are unchanged.
    pub(super) fn record_skinned_blas_refit(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        draw_commands: &[DrawCommand],
        pose_dirty: &std::collections::HashSet<EntityId>,
    ) {
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
        // #661 / SY-4 / #1436 (VKC-007): AS-build INPUT reads (the skinned
        // vertex output fed to the BLAS build) use `SHADER_READ` at the
        // AS_BUILD stage — the access the Vulkan spec assigns to build inputs.
        // Reading an acceleration STRUCTURE — a BLAS during the TLAS build, or
        // the TLAS during a ray query — is the separate
        // `ACCELERATION_STRUCTURE_READ_KHR`, retained on those barriers.
        // The earlier `ACCELERATION_STRUCTURE_READ_KHR`-for-inputs form was a
        // sync1 shortcut on the assumption the two flags were aliased;
        // synchronization validation disproved it (a compute/copy→build RAW
        // hazard on the input buffer), so input barriers now carry SHADER_READ.
        //
        // Skips entirely when `skin_compute` / `accel_manager` are None
        // (no RT) or no draws are skinned.
        //
        // #1796 / D6-02 — reaching this function at all proves `draw_frame`
        // got past both early-return guards, so the CPU-side pose hash
        // commit made earlier this frame (in `build_render_data`, before
        // `draw_frame` was even called) is safe to keep. Set unconditionally,
        // before the `Some`/`Some` gate below, since the absence of RT /
        // skin_compute means there's no dispatch to protect either way.
        self.skin_dispatch_ran = true;
        let skin_t0 = Instant::now();
        if let (Some(skin_pipeline), Some(ref mut accel)) =
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
                    //
                    // #1133 / PERF-D7-NEW-01 — `mem::take` from the
                    // `skin_*_scratch` cluster on `self`, drop the
                    // amortized capacity back at the end of the
                    // skinned block (line ~911). Matches the pattern
                    // documented at `context/mod.rs::Per-frame scratch
                    // cluster`.
                    let mut seen = std::mem::take(&mut self.skin_dispatch_seen_scratch);
                    seen.clear();
                    let mut dispatches = std::mem::take(&mut self.skin_dispatches_scratch);
                    dispatches.clear();
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
                        // Only meshes uploaded RT-capable may back a BLAS: their
                        // index buffer is the AS build input, and without
                        // `SHADER_DEVICE_ADDRESS |
                        // ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` both
                        // the device-address query and the build are invalid
                        // (VUID-VkBufferDeviceAddressInfo-buffer-02601 /
                        // -geometry-03673). Skinned effect-shader proxies and
                        // decals are uploaded `for_rt = false` precisely so rays
                        // don't hit their non-physical hulls; the static BLAS
                        // path already gates on the same flag, so this keeps the
                        // two paths consistent instead of ray-tracing a proxy
                        // only when it happens to be skinned.
                        if !mesh.rt_capable {
                            continue;
                        }
                        let push = super::super::skin_compute::SkinPushConstants {
                            vertex_offset: mesh.global_vertex_offset,
                            vertex_count: mesh.vertex_count,
                            bone_offset: dc.bone_offset,
                        };
                        dispatches.push((
                            dc.entity_id,
                            push,
                            mesh.index_buffer
                                .as_ref()
                                .expect("skinned mesh requires a per-mesh index buffer")
                                .buffer,
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
                    // #1133 — sibling scratch; same lifetime as `seen` /
                    // `dispatches`. Replaced back into self at end of block.
                    let mut first_sight_builds =
                        std::mem::take(&mut self.skin_first_sight_builds_scratch);
                    first_sight_builds.clear();
                    // D6-05 / #1812 — sibling scratch tracking entities
                    // whose BLAS gets a fresh BUILD this frame, so the
                    // refit loop below can skip the redundant UPDATE.
                    let mut built_this_frame =
                        std::mem::take(&mut self.skin_built_this_frame_scratch);
                    built_this_frame.clear();
                    for &(entity_id, _push, idx_buffer, idx_count, vertex_count) in &dispatches {
                        let mut needs_slot = !self.skin_slots.contains_key(&entity_id);

                        // #1297 / #1298 (DIM12-A-01) — reconcile an existing
                        // slot's allocated capacity against the live mesh
                        // vertex_count. If the entity's mesh_handle was remapped
                        // to a different-vertex-count mesh, the slot's output
                        // buffer (sized at create_slot time) is mis-sized, and
                        // the compute dispatch — bounded only by
                        // `push.vertex_count`, not the slot capacity — would
                        // write past the buffer (OOB). Destroy + recreate the
                        // slot and drop the now-stale paired skinned BLAS so
                        // `create_slot` re-allocs to the new size. Immediate
                        // destroy is safe here: the wait-on-both-in-flight-
                        // fences at the top of `draw_frame` (line ~234) has
                        // retired every command buffer referencing this slot's
                        // buffer. Symmetric with the BLAS-side
                        // `validate_refit_counts` guard.
                        if !needs_slot {
                            let stale_vc = self
                                .skin_slots
                                .get(&entity_id)
                                .map(|s| s.vertex_count())
                                .filter(|&slot_vc| {
                                    super::super::skin_compute::skin_slot_capacity_stale(
                                        slot_vc,
                                        vertex_count,
                                    )
                                });
                            if let Some(slot_vc) = stale_vc {
                                log::info!(
                                    "skin_compute slot for entity {entity_id} sized {slot_vc} verts \
                                     but mesh now has {vertex_count} (mesh remap) — recreating slot \
                                     to avoid OOB compute write (#1298)"
                                );
                                if let Some(slot) = self.skin_slots.remove(&entity_id) {
                                    skin_pipeline.destroy_slot(&self.device, alloc, slot);
                                }
                                accel.drop_skinned_blas(entity_id);
                                needs_slot = true;
                            }
                        }

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
                    //
                    // #1195 / PERF-DIM7-01 — dispatch is gated on the
                    // per-entity pose-dirty bit. Idle skinned entities
                    // (no bone movement since the previous frame) skip
                    // the GPU dispatch entirely; the output buffer
                    // already holds last frame's pose and the BLAS
                    // already references it.
                    //
                    // Safety: the skip path is gated on
                    // `slot.has_populated_output` — first-sight slots
                    // (output buffer uninitialised) MUST dispatch
                    // unconditionally, otherwise the BLAS would refit
                    // against garbage memory. The flag is set true the
                    // first time we actually dispatch for the slot.
                    // The LRU bump happens on the skip path too so
                    // quiescent slots aren't reaped by the eviction
                    // sweep.
                    if !dispatches.is_empty() {
                        // #1194 — bracket the skin compute dispatch
                        // loop. START sits before the per-entity
                        // dispatches; END sits after the loop body
                        // (before the COMPUTE→AS_BUILD barrier so
                        // the bracket measures only the dispatches
                        // themselves, not the barrier transition cost
                        // which lands inside the BLAS refit window).
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_skin_dispatch_start(&self.device, cmd, frame);
                        }
                        // SAFETY: `cmd` is recording; `skin_pipeline`, each `slot`'s descriptors, and the global vertex / bone input buffers are live for this frame. Each `dispatch` binds the compute pipeline + slot set at the COMPUTE bind point; the loop records sequentially with no concurrent use of `cmd`.
                        unsafe {
                            for &(entity_id, push, _, _, _) in &dispatches {
                                let Some(slot) = self.skin_slots.get_mut(&entity_id) else {
                                    continue;
                                };
                                // #643 / MEM-2-1 — bump LRU first
                                // (before the skip gate) so the
                                // eviction sweep below sees this
                                // entity as "active this frame" even
                                // when the dispatch is skipped.
                                slot.last_used_frame = self.frame_counter as u64;

                                // #1195 / PERF-DIM7-01 — skip the
                                // dispatch when the entity's pose is
                                // unchanged AND the output buffer is
                                // already populated. First-sight slots
                                // always fall through to the dispatch
                                // below (their `has_populated_output`
                                // is still false).
                                let is_dirty = pose_dirty.contains(&entity_id);
                                if slot.has_populated_output && !is_dirty {
                                    self.last_skin_coverage_frame.dispatches_skipped += 1;
                                    continue;
                                }
                                skin_pipeline.dispatch(
                                    &self.device,
                                    cmd,
                                    slot,
                                    frame,
                                    super::super::skin_compute::SkinDispatchBuffers {
                                        input_buffer,
                                        input_buffer_size: input_size,
                                        bone_buffer: bone_buf,
                                        bone_buffer_size,
                                    },
                                    push,
                                );
                                // Flip the "populated" bit on the
                                // first successful dispatch so the
                                // next-frame skip gate can fire.
                                slot.has_populated_output = true;
                            }
                        }
                        // #1194 — END of skin compute dispatch bracket
                        // (before the COMPUTE→AS_BUILD barrier).
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_skin_dispatch_end(&self.device, cmd, frame);
                        }
                        // SAFETY: `cmd` is recording. The COMPUTE_SHADER_WRITE -> AS_BUILD_READ barrier sequences the skin outputs before they are read as BLAS build inputs; the first-sight builds and refits share `blas_scratch_buffer` and self-emit AS_WRITE->AS_WRITE scratch-serialize barriers between builds; the closing AS_BUILD_WRITE -> AS_BUILD_READ barrier hands refit results to the TLAS build below.
                        unsafe {
                            // Compute writes (skinned vertex output
                            // buffers) → AS build input reads. Covers
                            // both the first-sight BUILD batch below
                            // and the refit loop further down — both
                            // read the freshly-written output buffers
                            // as BLAS-build vertex input.
                            // COMPUTE_SHADER → ACCELERATION_STRUCTURE_BUILD_KHR.
                            // Skinned vertex output is a BLAS-build INPUT, so the
                            // dst access is SHADER_READ (the spec's build-input
                            // access), NOT ACCELERATION_STRUCTURE_READ. #1436.
                            memory_barrier(
                                &self.device,
                                cmd,
                                vk::PipelineStageFlags::COMPUTE_SHADER,
                                vk::AccessFlags::SHADER_WRITE,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::SHADER_READ,
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
                                            self.last_skin_coverage_frame.first_sight_succeeded +=
                                                1;
                                            // D6-05 / #1812 — mark this
                                            // entity so the refit loop
                                            // below skips it: the BUILD
                                            // just recorded already
                                            // produced a complete BLAS
                                            // from the exact same vertex
                                            // data a refit would re-read.
                                            // A failed build does NOT get
                                            // marked — it leaves no BLAS
                                            // behind, so the refit's own
                                            // `accel.has_skinned_blas`
                                            // check still governs that
                                            // entity unchanged.
                                            built_this_frame.insert(entity_id);
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
                            // #1194 — bracket the skinned-BLAS refit loop.
                            // START is just before the loop body; END
                            // is right after the AS_BUILD→AS_BUILD
                            // barrier closes the refit window.
                            if let Some(ref mut timers) = self.gpu_timers {
                                timers.cmd_blas_refit_start(&self.device, cmd, frame);
                            }
                            for &(entity_id, _, idx_buffer, idx_count, vertex_count) in &dispatches
                            {
                                let Some(slot) = self.skin_slots.get(&entity_id) else {
                                    continue;
                                };
                                // #1196 / PERF-DIM7-02 — paired refit
                                // gate. Same predicate as the dispatch
                                // skip above: if the entity's pose was
                                // unchanged this frame AND the slot
                                // already has a populated output AND a
                                // live BLAS, skip the refit. The BLAS
                                // still references the same output
                                // buffer; nothing changed underneath
                                // it. The skip uses the same
                                // `pose_dirty` set so the two decisions
                                // can't diverge — the "split decisions
                                // are the trap" warning from the audit.
                                //
                                // D6-05 / #1812 — first-sight entities
                                // are always dirty, so the predicate
                                // above alone can't catch them; they
                                // used to fall through to a full UPDATE
                                // against the exact vertex data their
                                // BUILD (above, same `cmd`) just
                                // consumed — pure wasted work, not a
                                // correctness requirement. Skip them via
                                // `built_this_frame` instead.
                                let is_dirty = pose_dirty.contains(&entity_id);
                                if built_this_frame.contains(&entity_id)
                                    || (slot.has_populated_output
                                        && !is_dirty
                                        && accel.has_skinned_blas(entity_id))
                                {
                                    // Skip path mirrors the dispatch
                                    // skip — counts via `dispatches_skipped`
                                    // is the dispatch's responsibility;
                                    // refit just falls through silently.
                                    continue;
                                }
                                // Past the slot gate → coverage counts a
                                // real refit attempt. Entities without a
                                // slot land in `slots_failed` instead.
                                self.last_skin_coverage_frame.refits_attempted += 1;
                                // Scratch-serialize barrier is now self-emitted at the
                                // top of refit_skinned_blas (blas_skinned.rs:555, #983).
                                // Removed the redundant caller-side emit (#1095 / REN-D12-002).
                                match accel.refit_skinned_blas(
                                    &self.device,
                                    cmd,
                                    entity_id,
                                    crate::vulkan::acceleration::SkinnedBlasGeometry {
                                        vertex_buffer: slot.output_buffer.buffer,
                                        vertex_count,
                                        index_buffer: idx_buffer,
                                        index_count: idx_count,
                                    },
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
                                &self.device,
                                cmd,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                                vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                                vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                            );
                        }
                        // #1194 — END of skinned-BLAS refit bracket
                        // (after the AS_BUILD→AS_BUILD barrier).
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_blas_refit_end(&self.device, cmd, frame);
                        }
                    }

                    // #1133 — return the skin-path scratches to `self`.
                    // Same shape as the gpu_instances / batches replace
                    // at the end of build_render_data → SSBO upload.
                    self.skin_dispatch_seen_scratch = seen;
                    self.skin_dispatches_scratch = dispatches;
                    self.skin_first_sight_builds_scratch = first_sight_builds;
                    self.skin_built_this_frame_scratch = built_this_frame;

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
                    evictees.extend(self.skin_slots.iter().filter_map(|(&eid, slot)| {
                        super::super::skin_compute::should_evict_skin_slot(
                            slot.last_used_frame,
                            now,
                            min_idle,
                        )
                        .then_some(eid)
                    }));
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
    }
}

// D6-05 / #1812 — first-sight entities must skip the redundant
// post-BUILD refit. The refit loop lives deep inside `draw_frame`'s
// live-Vulkan-device path, so (mirroring `skin_dispatch_ran_ordering_tests`
// above) this pins the fix at the source level rather than exercising it
// end-to-end.
#[cfg(test)]
mod skin_built_this_frame_skip_tests {
    #[test]
    fn built_entities_are_marked_only_on_successful_build_and_skip_the_refit() {
        let src = include_str!("skinned_blas_refit.rs");

        let insert_pos = src
            .find("built_this_frame.insert(entity_id);")
            .expect("draw_frame must mark successfully-built entities in built_this_frame (#1812)");
        let ok_arm_pos = src
            .find("Ok(()) => {\n                                            self.last_skin_coverage_frame.first_sight_succeeded")
            .expect("the first-sight build result match must have an Ok(()) arm");
        let err_arm_pos = src
            .find("Err(e) => {\n                                            log::warn!(\n                                                \"skin_compute first-sight BLAS build failed")
            .expect("the first-sight build result match must have an Err(e) arm");
        assert!(
            ok_arm_pos < insert_pos && insert_pos < err_arm_pos,
            "built_this_frame.insert must happen inside the Ok(()) arm only — a \
             failed build leaves no BLAS behind, so it must not be marked as \
             built (#1812)"
        );

        let refit_gate_pos = src
            .find("if built_this_frame.contains(&entity_id)")
            .expect("the skinned-BLAS refit loop must gate on built_this_frame (#1812)");
        let refits_attempted_pos = src
            .find("self.last_skin_coverage_frame.refits_attempted += 1;")
            .expect("the refit loop must count attempted refits");
        assert!(
            insert_pos < refit_gate_pos,
            "built_this_frame must be populated by the build-results loop before \
             the refit loop reads it"
        );
        assert!(
            refit_gate_pos < refits_attempted_pos,
            "the built_this_frame gate must precede the refits_attempted counter \
             so a freshly-built entity's skip doesn't inflate spawn-frame \
             telemetry (#1812)"
        );
    }
}
