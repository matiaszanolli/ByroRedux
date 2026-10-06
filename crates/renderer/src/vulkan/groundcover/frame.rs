//! Per-frame half of [`GroundCoverPipeline`] (#5089, split out of
//! `groundcover.rs`): `prepare` (upload + descriptor rebind), the readback
//! harvest, and the scatter / interaction / draw recording. Types,
//! constants and the struct live in `groundcover.rs`; construction lives
//! in `construct.rs`.
use super::*;

impl GroundCoverPipeline {
    /// Upload the frame's chunk / cell / species records and rebind the
    /// descriptors. Returns `false` when there is nothing to scatter, which is
    /// every interior frame and every exterior frame before the first terrain
    /// mesh has uploaded.
    ///
    /// Must be called after slot `frame`'s fence has been waited — it writes
    /// that slot's host-visible buffers and rewrites its descriptor sets.
    pub fn prepare(
        &mut self,
        device: &ash::Device,
        frame: usize,
        vertex_buffer: vk::Buffer,
        input: &GroundCoverFrame<'_>,
    ) -> bool {
        // Harvest the readback this slot took one pipelined cycle ago, before
        // the buffers it describes are overwritten.
        self.harvest(device, frame);

        // #4338 — the host has already cut its farthest chunks to the cap;
        // anything still over it is clamped just below, and both count.
        self.frame_chunks_truncated = input.chunks_truncated.saturating_add(
            input
                .chunks
                .len()
                .saturating_sub(GROUNDCOVER_MAX_CHUNKS as usize) as u32,
        );
        if input.chunks.is_empty() || input.cells.is_empty() || vertex_buffer == vk::Buffer::null()
        {
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }
        let chunks = &input.chunks[..input.chunks.len().min(GROUNDCOVER_MAX_CHUNKS as usize)];
        let cells = &input.cells[..input.cells.len().min(MAX_GROUNDCOVER_CELLS)];
        let species = &input.species[..input.species.len().min(MAX_GROUNDCOVER_SPECIES)];
        if species.is_empty() {
            // `GroundCoverPalette::resolve` guarantees a non-empty palette, so
            // reaching here means the host published one it had not resolved.
            // Drawing anyway would index `gcSpecies[0]` out of bounds.
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }

        // §12.4 — advance the field's header before the descriptor write, so
        // this slot's state buffer and its descriptor go up together.
        let disturbers = &input.disturbers[..input
            .disturbers
            .len()
            .min(GROUNDCOVER_INTERACTION_MAX_DISTURBERS as usize)];
        let field_state =
            self.advance_field_state(input.camera_pos, input.delta_seconds, disturbers.len());

        // §7's selection table, padded to its fixed size with species 0 so
        // every entry the scatter's 8 hash bits can reach is initialised.
        let mut species_table = [0u32; GROUNDCOVER_SPECIES_TABLE_SIZE as usize];
        let table_len = input.species_table.len().min(species_table.len());
        species_table[..table_len].copy_from_slice(&input.species_table[..table_len]);

        let uploads = self.chunk_buffers[frame]
            .write_mapped(device, chunks)
            .and_then(|()| self.cell_buffers[frame].write_mapped(device, cells))
            .and_then(|()| self.species_buffers[frame].write_mapped(device, species))
            .and_then(|()| self.species_table_buffers[frame].write_mapped(device, &species_table))
            .and_then(|()| self.field_state_buffers[frame].write_mapped(device, &[field_state]))
            .and_then(|()| {
                if disturbers.is_empty() {
                    Ok(())
                } else {
                    self.disturber_buffers[frame].write_mapped(device, disturbers)
                }
            });
        if let Err(error) = uploads {
            log::warn!("ground cover: record upload failed: {error}");
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }
        self.frame_field_state = field_state;
        self.frame_disturber_count = disturbers.len() as u32;
        self.write_descriptor_sets(device, frame, vertex_buffer);
        self.bound_vertex_buffer = vertex_buffer;

        self.frame_chunk_count = chunks.len() as u32;
        self.frame_active_chunk_count =
            chunks.iter().filter(|chunk| chunk.active != 0).count() as u32;
        self.frame_debug_points = input.debug_points;
        // `size_range[1]` is each species' height ceiling. Non-finite entries
        // cannot reach here (`GroundCoverSpecies::is_well_formed` rejects
        // them at palette resolve), and the palette is non-empty.
        self.frame_cover_reach = species
            .iter()
            .map(|s| s.size_range[1])
            .fold(0.0_f32, f32::max);
        self.frame_push = BladePush {
            camera_pixels: [
                input.camera_pos[0],
                input.camera_pos[1],
                input.camera_pos[2],
                input.pixels_per_unit_at_unit_depth,
            ],
            origin_time: [
                input.render_origin[0],
                input.render_origin[1],
                input.render_origin[2],
                input.time_seconds,
            ],
            wind: input.wind,
            gust_and_timing: [
                input.gust_frequency,
                input.time_seconds - input.delta_seconds.max(0.0),
                species.len() as f32,
                // Packed `(frame serial << 2) | lod tier`. It keeps the push
                // block compact and makes the blue-noise transition
                // repeat under a replayed simulation clock. The shift is two
                // bits, not one, because `GROUNDCOVER_INDIRECT_STREAMS` is 3:
                // a one-bit tier field cannot encode the clump-card tier, and
                // its value carried into the serial instead.
                //
                // The serial is masked to `GROUNDCOVER_FRAME_SERIAL_BITS`
                // before the ×stride so the word stays f32-exact for the life
                // of the process: past 2^24 the f32 spacing widens to 2 and
                // `record_draw`'s odd-tier `+ 1.0` rounds away (#4498). The
                // serial only seeds a wrapping tile rotation, so the 22-bit
                // period costs nothing.
                (((input.time_seconds.max(0.0) * 60.0).floor() as u64)
                    & GROUNDCOVER_FRAME_SERIAL_MASK) as f32
                    * GROUNDCOVER_LOD_TIER_STRIDE as f32,
            ],
        };
        true
    }

    /// Advance §12.4's field header for this frame and flip the ping-pong.
    ///
    /// **The origin is snapped to the texel grid**, which is what makes the
    /// compute pass's reprojection a copy rather than a filtered fetch. A
    /// fractional offset would need bilinear resampling, and a filter applied
    /// every frame to its own output is a low-pass running at frame rate: a
    /// crisp channel smears into nothing within a second or two.
    ///
    /// The previous half is only declared usable once one update has actually
    /// run, so the first frame starts from an empty field rather than from
    /// whatever `create_device_local_uninit` left in the allocation.
    fn advance_field_state(
        &mut self,
        camera_pos: [f32; 3],
        delta_seconds: f32,
        disturbers: usize,
    ) -> GpuGroundCoverFieldState {
        advance_field_state(
            &mut self.field_previous,
            &mut self.field_write_half,
            camera_pos,
            delta_seconds,
            disturbers,
        )
    }

    fn harvest(&mut self, device: &ash::Device, frame: usize) {
        let truncated = std::mem::take(&mut self.pending_truncated[frame]);
        let dispatched = std::mem::take(&mut self.pending_chunks[frame]);
        let slots = std::mem::take(&mut self.pending_chunk_slots[frame]);
        if slots == 0 {
            return;
        }
        let buffer = &mut self.counter_readback[frame];
        if buffer.invalidate_if_needed(device).is_err() {
            return;
        }
        let Ok(bytes) = buffer.mapped_slice_mut() else {
            return;
        };
        let counters: Vec<u32> = bytes
            .as_chunks::<4>().0
            .iter()
            .take(COUNTER_SLOTS)
            .map(|w| u32::from_ne_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        if counters.len() < COUNTER_SLOTS {
            return;
        }
        let mut stats = GroundCoverStats {
            chunks_dispatched: dispatched,
            chunks_truncated: truncated,
            ..Default::default()
        };
        // Clamped, not summed raw: the append cursor deliberately runs past
        // the cap (§4's saturating overflow), so the raw value is "candidates
        // that tried", not "blades that exist".
        stats.blades_accepted = counters[..slots as usize]
            .iter()
            .map(|c| c.min(&GROUNDCOVER_MAX_BLADES_PER_CHUNK))
            .sum();
        stats.blades_overflowed = counters[COUNTER_OVERFLOW];
        stats.blades_covered = counters[COUNTER_COVERED];
        stats
            .histogram
            .copy_from_slice(&counters[COUNTER_HIST_BASE..COUNTER_OVERFLOW]);
        // A frame where the field was never evaluated leaves the `min` slots
        // at their seed; reporting `u32::MAX / 1e6` there would read as a
        // density of 4295, so an untouched pair collapses to zero.
        let untouched = counters[COUNTER_EXTREMA_BASE] == EXTREMA_MIN_SEED;
        if !untouched {
            stats.d_ground_min = counters[COUNTER_EXTREMA_BASE] as f32 / 1.0e6;
            stats.d_ground_max = counters[COUNTER_EXTREMA_BASE + 1] as f32 / 1.0e6;
            stats.view_dist_min = counters[COUNTER_EXTREMA_BASE + 2] as f32;
            stats.view_dist_max = counters[COUNTER_EXTREMA_BASE + 3] as f32;
            for (i, slot) in stats.factor_max.iter_mut().enumerate() {
                *slot = counters[COUNTER_FACTOR_BASE + i] as f32 / 1.0e6;
            }
        }
        self.stats = stats;
    }

    fn write_descriptor_sets(&self, device: &ash::Device, frame: usize, vertex_buffer: vk::Buffer) {
        let blade = self.blade_buffer.as_ref().expect("created in new()");
        let indirect = self.indirect_buffer.as_ref().expect("created in new()");
        let counters = self.counter_buffer.as_ref().expect("created in new()");
        let info = |buffer: vk::Buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .range(vk::WHOLE_SIZE)]
        };
        let chunk_info = info(self.chunk_buffers[frame].buffer);
        let cell_info = info(self.cell_buffers[frame].buffer);
        let vertex_info = info(vertex_buffer);
        let blade_info = info(blade.buffer);
        let indirect_info = info(indirect.buffer);
        let counter_info = info(counters.buffer);
        let species_info = info(self.species_buffers[frame].buffer);
        let species_table_info = info(self.species_table_buffers[frame].buffer);
        let field = self.field_buffer.as_ref().expect("created in new()");
        let field_info = info(field.buffer);
        let field_state_info = info(self.field_state_buffers[frame].buffer);
        let disturber_info = info(self.disturber_buffers[frame].buffer);

        fn write<'a>(
            set: vk::DescriptorSet,
            binding: u32,
            info: &'a [vk::DescriptorBufferInfo],
        ) -> vk::WriteDescriptorSet<'a> {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(info)
        }
        let scatter = self.scatter_sets[frame];
        let draw = self.draw_sets[frame];
        let writes = [
            write(scatter, 0, &chunk_info),
            write(scatter, 1, &cell_info),
            write(scatter, 2, &vertex_info),
            write(scatter, 3, &blade_info),
            write(scatter, 4, &indirect_info),
            write(scatter, 5, &counter_info),
            write(scatter, 7, &species_table_info),
            write(draw, 0, &chunk_info),
            write(draw, 1, &cell_info),
            write(draw, 2, &vertex_info),
            write(draw, 3, &blade_info),
            write(draw, 6, &species_info),
            write(draw, 7, &field_info),
            write(draw, 8, &field_state_info),
            write(self.interaction_sets[frame], 0, &field_info),
            write(self.interaction_sets[frame], 1, &field_state_info),
            write(self.interaction_sets[frame], 2, &disturber_info),
        ];
        // SAFETY: every `*_info` slice outlives the call, and only slot
        // `frame`'s sets are touched — the caller has waited that slot's
        // fence, so nothing in flight reads them.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Record the scatter dispatch. Must be OUTSIDE a render pass and before
    /// the main geometry pass that draws the result.
    /// `timers` brackets the whole compute half — interaction dispatch,
    /// counter clears, scatter dispatch and the trailing publish barrier
    /// (#4315). Passed in rather than bracketed at the call site because both
    /// early returns below are real skips: an interior frame has no chunks,
    /// and a frame without a TLAS handle abandons the dispatch. Bracketing
    /// outside would mark those frames active with a ~0 ms reading, which is
    /// exactly the "ran and took no time" / "did not run" confusion the
    /// `_active` flags exist to prevent.
    pub fn record_scatter(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        tlas: Option<vk::AccelerationStructureKHR>,
        mut timers: Option<&mut crate::vulkan::gpu_timers::GpuPerFrameTimers>,
    ) {
        if self.frame_chunk_count == 0 {
            return;
        }
        // The scatter statically uses binding 6, so it cannot run against an
        // unwritten one. `ray_query_tlas(frame)` returns `None` both when the
        // acceleration manager is gone and when this frame's TLAS build did
        // not succeed. Skip the frame and clear its active chunk counts rather
        // than draw last frame's indirect list over this frame's scene.
        let Some(tlas) = tlas else {
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return;
        };
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = crate::vulkan::descriptors::write_acceleration_structure(
            self.scatter_sets[frame],
            6,
            &mut accel_write,
        );
        // SAFETY: `accel_write` borrows `accel_structs`, both live for the
        // call; only slot `frame`'s set is touched and its fence has been
        // waited, and the set is not yet bound in `cmd`.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
        if let Some(timers) = timers.as_deref_mut() {
            timers.cmd_groundcover_scatter_start(device, cmd, frame);
        }
        self.record_interaction(device, cmd, frame);
        let counters = self.counter_buffer.as_ref().expect("created in new()");
        // SAFETY: `cmd` is recording; every handle below is live and owned by
        // this pipeline. The barriers order the clear against the dispatch and
        // the dispatch against both its readers (the indirect draw and the
        // vertex shader reading the blade buffer).
        unsafe {
            // Counters carry per-chunk append cursors and the histogram, both
            // of which are per-frame quantities. Clearing here rather than at
            // the end of the previous frame keeps the whole lifetime inside
            // one command buffer.
            device.cmd_fill_buffer(cmd, counters.buffer, 0, counters.size, 0);
            // #4293 — the seed fills below write ranges this zero fill already
            // wrote, and nothing orders the two: under the sync model this
            // crate follows (#4177/#4179/#4181) their relative order is
            // undefined, so sync validation reports each seed as a
            // WRITE_AFTER_WRITE hazard. If the zero fill landed last, both
            // `atomicMin` extrema would start at 0 and
            // `GroundCoverStats::d_ground_min` / `view_dist_min` would read 0 —
            // telemetry EXAL tuning reads directly. A TRANSFER → TRANSFER
            // barrier is the device edge that sequences them. Confirmed as the
            // source of the ten `vkCmdFillBuffer` WAW hazards on a
            // `BYRO_VALIDATION=1` Skyrim tundra capture, and cleared by this.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
            );
            // Zero is the identity for every counter here except the two
            // `atomicMin` extrema, which need the opposite. Two four-byte
            // fills rather than a staging upload: this is 8 bytes.
            device.cmd_fill_buffer(
                cmd,
                counters.buffer,
                (COUNTER_EXTREMA_BASE * 4) as vk::DeviceSize,
                4,
                EXTREMA_MIN_SEED,
            );
            device.cmd_fill_buffer(
                cmd,
                counters.buffer,
                ((COUNTER_EXTREMA_BASE + 2) * 4) as vk::DeviceSize,
                4,
                EXTREMA_MIN_SEED,
            );
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.scatter_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.scatter_pipeline_layout,
                0,
                &[self.scatter_sets[frame]],
                &[],
            );
            let push = ScatterPush {
                camera_pos: [
                    self.frame_push.camera_pixels[0],
                    self.frame_push.camera_pixels[1],
                    self.frame_push.camera_pixels[2],
                    self.frame_cover_reach,
                ],
                render_origin: [
                    self.frame_push.origin_time[0],
                    self.frame_push.origin_time[1],
                    self.frame_push.origin_time[2],
                    1.0,
                ],
                chunk_count: self.frame_chunk_count,
                blades_per_chunk: GROUNDCOVER_MAX_BLADES_PER_CHUNK,
                // The debug view is one point per blade; the blade tier is
                // `segments * 6`. The scatter writes this into the indirect
                // `vertexCount`, so it has to agree with whatever the vertex
                // shader will divide `gl_VertexIndex` by.
                verts_per_blade: if self.frame_debug_points {
                    1
                } else {
                    // A point grows GROUNDCOVER_BLADES_PER_POINT blades,
                    // and the vertex shader divides `gl_VertexIndex` by
                    // exactly this product to find the point.
                    VERTS_PER_BLADE_NEAR * GROUNDCOVER_BLADES_PER_POINT
                },
                species_count: self.frame_push.gust_and_timing[2] as u32,
            };
            device.cmd_push_constants(
                cmd,
                self.scatter_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                scatter_push_bytes(&push),
            );
            // §4: one workgroup per chunk, mirroring `cluster_cull.comp`.
            device.cmd_dispatch(cmd, self.frame_chunk_count, 1, 1);
            // #4181 / CONC-D2-01 — `TRANSFER` / `TRANSFER_READ` in the dst
            // scope is for the `cmd_copy_buffer` immediately below, not for
            // the draw.
            //
            // The scatter writes `counters.buffer` via atomics; this is the
            // single barrier publishing those writes, and it used to name
            // only the draw's consumers (`DRAW_INDIRECT | VERTEX_SHADER` /
            // `INDIRECT_COMMAND_READ | SHADER_READ`). The readback copy
            // reads the same buffer with no dependency on the compute write
            // at all — the two sibling edges in this file
            // (`TRANSFER→COMPUTE` above the dispatch, `COMPUTE→COMPUTE |
            // VERTEX` in `record_interaction`) are both correct by contrast.
            //
            // Rendering was never affected: the draw path reads through the
            // correctly-published half of this same barrier. What was
            // undefined is the readback `harvest` decodes into
            // `GroundCoverStats` — blade counts, density histogram, extrema
            // — which EXAL ground-cover tuning reads directly (#4054), so
            // the failure mode is silently wrong telemetry driving tuning
            // decisions rather than a visible artefact.
            //
            // Widening the existing edge rather than adding a second one:
            // same class of purely-additive change as #2403's skinned-vertex
            // publish mask.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::DRAW_INDIRECT
                    | vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::INDIRECT_COMMAND_READ
                    | vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::TRANSFER_READ,
            );
            // Snapshot the counters for the §11.3 histogram. Copied rather
            // than read in place so the scatter never touches host-visible
            // memory: it issues one atomic per candidate, and 3 M atomics a
            // frame across PCIe is not a diagnostic, it is a stall.
            device.cmd_copy_buffer(
                cmd,
                counters.buffer,
                self.counter_readback[frame].buffer,
                &[vk::BufferCopy::default().size(counters.size)],
            );
        }
        if let Some(timers) = timers {
            timers.cmd_groundcover_scatter_end(device, cmd, frame);
        }
        self.pending_chunks[frame] = self.frame_active_chunk_count;
        self.pending_chunk_slots[frame] = self.frame_chunk_count;
        self.pending_truncated[frame] = self.frame_chunks_truncated;
    }

    /// Record §12.4's field update. Runs from `record_scatter`, before the
    /// scatter's own dispatch, so ordering against the blade draw is the
    /// scatter's existing trailing barrier and there is no second edge to get
    /// wrong.
    ///
    /// It shares the scatter's "only when there is ground cover" gate on
    /// purpose. Nothing reads the field on a frame with no chunks, and the
    /// staleness that skipping introduces resolves itself: the origin the next
    /// live frame snaps to either matches (the camera did not move, and the
    /// trail is genuinely still there) or does not (every reprojection falls
    /// outside the previous field and reads zero).
    fn record_interaction(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        if self.interaction_pipeline == vk::Pipeline::null() {
            return;
        }
        // SAFETY: `cmd` is recording; the pipeline, layout and set are live and
        // owned here, and the caller has waited slot `frame`'s fence so the
        // host-visible header and disturber buffers this set points at are not
        // in flight. The trailing barrier orders the field's writes against
        // the blade vertex shader that reads them.
        unsafe {
            // The field is one persistent allocation shared by every
            // frame-in-flight, and this dispatch READS the half the *previous*
            // frame's dispatch wrote. A barrier's first synchronization scope
            // covers everything submitted earlier on the queue, so this one
            // edge sequences that read against that write.
            //
            // #4293 — this comment used to justify the edge by saying "the
            // per-slot fence only serialises frames MAX_FRAMES_IN_FLIGHT
            // apart, so consecutive frames can overlap on the queue". The
            // host does not do that: `sync_and_acquire_frame` waits on every
            // frame-in-flight fence, so the previous frame has finished before
            // this one records. The barrier is still required, for the reason
            // #4177/#4179 settled: a host-side fence wait is not a device
            // edge, and the queue's synchronization model — which is what sync
            // validation checks — only sees barriers.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.interaction_pipeline,
            );
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.interaction_pipeline_layout,
                0,
                &[self.interaction_sets[frame]],
                &[],
            );
            let groups = GROUNDCOVER_INTERACTION_TEXELS.div_ceil(GROUNDCOVER_INTERACTION_WORKGROUP);
            device.cmd_dispatch(cmd, groups, groups, 1);
            // The field's two halves alternate, so this frame's writes are the
            // next frame's reads as well as this frame's vertex-shader reads.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::AccessFlags::SHADER_READ,
            );
        }
    }

    /// Record the blade (or debug-point) draw. Must be INSIDE the main
    /// geometry render pass, after opaque geometry. Returns whether any
    /// indirect draw commands were recorded (their GPU instance count may be zero).
    pub fn record_draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        texture_set: vk::DescriptorSet,
        scene_set: vk::DescriptorSet,
    ) -> bool {
        if self.frame_chunk_count == 0 {
            return false;
        }
        let indirect = self.indirect_buffer.as_ref().expect("created in new()");
        let pipeline = if self.frame_debug_points {
            self.debug_pipeline
        } else {
            self.blade_pipeline
        };
        // #4307 — a failed `recreate_draw_pipelines` leaves these null rather
        // than dangling, and the ground cover stops drawing until the next
        // successful rebuild. The object itself stays alive so its arena,
        // field images and descriptor pool are still torn down by `destroy`;
        // dropping it here to signal the failure would leak every one of them.
        if pipeline == vk::Pipeline::null() {
            return false;
        }
        // SAFETY: `cmd` is recording inside the render pass this pipeline was
        // created against; the descriptor sets and the indirect buffer are
        // live, and the scatter's trailing barrier has made its writes visible
        // to `DRAW_INDIRECT` and `VERTEX_SHADER`.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.draw_pipeline_layout,
                0,
                &[texture_set, scene_set, self.draw_sets[frame]],
                &[],
            );
            let mut push = self.frame_push;
            // Stream 0 is the three-segment tier and stream 1 the one-segment
            // tier. They share every residency slab, while their separate
            // first-vertex strides prevent an LOD change from re-addressing a
            // neighbouring chunk's blade records.
            let streams = if self.frame_debug_points {
                1
            } else {
                GROUNDCOVER_INDIRECT_STREAMS
            };
            for tier in 0..streams {
                push.gust_and_timing[3] = self.frame_push.gust_and_timing[3] + tier as f32;
                device.cmd_push_constants(
                    cmd,
                    self.draw_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    blade_push_bytes(&push),
                );
                device.cmd_draw_indirect(
                    cmd,
                    indirect.buffer,
                    tier * GROUNDCOVER_MAX_CHUNKS as u64 * GC_DRAW_INDIRECT_STRIDE,
                    self.frame_chunk_count,
                    GC_DRAW_INDIRECT_STRIDE as u32,
                );
            }
        }
        true
    }

    /// The frame's chunk and cell records, the terrain vertex buffer and the
    /// camera, for the authored-model tier's placement (#4413) — the same
    /// chunks the blades scattered over. `None` when the scatter did not run
    /// this frame (an interior, or no TLAS to trace).
    pub fn model_scatter_inputs(
        &self,
        frame: usize,
        tlas: Option<vk::AccelerationStructureKHR>,
    ) -> Option<crate::vulkan::groundcover_models::ModelScatterInputs> {
        (self.frame_chunk_count > 0).then(|| crate::vulkan::groundcover_models::ModelScatterInputs {
            chunk_buffer: self.chunk_buffers[frame].buffer,
            cell_buffer: self.cell_buffers[frame].buffer,
            vertex_buffer: self.bound_vertex_buffer,
            chunk_count: self.frame_chunk_count,
            camera_pos: [
                self.frame_push.camera_pixels[0],
                self.frame_push.camera_pixels[1],
                self.frame_push.camera_pixels[2],
            ],
            render_origin: [
                self.frame_push.origin_time[0],
                self.frame_push.origin_time[1],
                self.frame_push.origin_time[2],
            ],
            tlas,
        })
    }

    pub fn stats(&self) -> GroundCoverStats {
        self.stats
    }

    /// # Safety
    ///
    /// No in-flight command buffer or descriptor set may still reference any
    /// object owned here — the caller must have idled the device.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for pipeline in [
            &mut self.scatter_pipeline,
            &mut self.interaction_pipeline,
            &mut self.blade_pipeline,
            &mut self.debug_pipeline,
        ] {
            if *pipeline != vk::Pipeline::null() {
                // SAFETY: caller's contract — nothing in flight uses it.
                unsafe { device.destroy_pipeline(*pipeline, None) };
                *pipeline = vk::Pipeline::null();
            }
        }
        for layout in [
            &mut self.scatter_pipeline_layout,
            &mut self.interaction_pipeline_layout,
            &mut self.draw_pipeline_layout,
        ] {
            if *layout != vk::PipelineLayout::null() {
                // SAFETY: every pipeline built against it is destroyed above.
                unsafe { device.destroy_pipeline_layout(*layout, None) };
                *layout = vk::PipelineLayout::null();
            }
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            // SAFETY: destroying the pool frees its sets; caller guarantees no
            // in-flight command buffer still binds them.
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
            self.descriptor_pool = vk::DescriptorPool::null();
            self.scatter_sets.clear();
            self.draw_sets.clear();
            self.interaction_sets.clear();
        }
        for layout in [
            &mut self.scatter_set_layout,
            &mut self.interaction_set_layout,
            &mut self.draw_set_layout,
        ] {
            if *layout != vk::DescriptorSetLayout::null() {
                // SAFETY: the pipeline layouts and sets referencing it are gone.
                unsafe { device.destroy_descriptor_set_layout(*layout, None) };
                *layout = vk::DescriptorSetLayout::null();
            }
        }
        for buffer in self
            .chunk_buffers
            .iter_mut()
            .chain(self.cell_buffers.iter_mut())
            .chain(self.species_buffers.iter_mut())
            .chain(self.species_table_buffers.iter_mut())
            .chain(self.counter_readback.iter_mut())
            .chain(self.field_state_buffers.iter_mut())
            .chain(self.disturber_buffers.iter_mut())
            .chain(self.blade_buffer.iter_mut())
            .chain(self.indirect_buffer.iter_mut())
            .chain(self.counter_buffer.iter_mut())
            .chain(self.field_buffer.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.chunk_buffers.clear();
        self.cell_buffers.clear();
        self.species_buffers.clear();
        self.species_table_buffers.clear();
        self.counter_readback.clear();
        self.field_state_buffers.clear();
        self.disturber_buffers.clear();
        self.blade_buffer = None;
        self.indirect_buffer = None;
        self.counter_buffer = None;
        self.field_buffer = None;
    }
}

/// Free-function core of [`GroundCoverPipeline::advance_field_state`], so the
/// snapping and clamping rules can be tested without a Vulkan device.
fn advance_field_state(
    field_previous: &mut Option<[f32; 2]>,
    field_write_half: &mut u32,
    camera_pos: [f32; 3],
    delta_seconds: f32,
    disturbers: usize,
) -> GpuGroundCoverFieldState {
    let texel = GROUNDCOVER_INTERACTION_TEXEL_UNITS;
    let half_extent = GROUNDCOVER_INTERACTION_UNITS * 0.5;
    let snap = |v: f32| ((v - half_extent) / texel).floor() * texel;
    let origin = [snap(camera_pos[0]), snap(camera_pos[2])];
    // Clamped, not trusted. A long hitch must not wipe a trail outright (the
    // decay is exponential, so a quarter-second step already removes ~11% of
    // it), and a negative or non-finite dt would *amplify* one — `exp(-k·dt)`
    // with dt < 0 grows without bound, and the field is fed back into itself
    // every frame, so one bad value would not decay away.
    let dt = if delta_seconds.is_finite() {
        delta_seconds.clamp(0.0, 0.25)
    } else {
        0.0
    };
    let write_half = *field_write_half;
    let previous = *field_previous;
    *field_write_half = 1 - write_half;
    *field_previous = Some(origin);
    GpuGroundCoverFieldState {
        current: [origin[0], origin[1], dt, write_half as f32],
        previous: [
            previous.map_or(0.0, |p| p[0]),
            previous.map_or(0.0, |p| p[1]),
            disturbers as f32,
            if previous.is_some() { 1.0 } else { 0.0 },
        ],
    }
}

fn buffer_barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
    dst_access: vk::AccessFlags,
) {
    let barrier = [vk::MemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)];
    // SAFETY: `cmd` is recording and `barrier` outlives the call. A global
    // memory barrier rather than per-buffer ones: the scatter's four written
    // buffers are all consumed at the same two stages, so naming them
    // individually would be four times the API traffic for the same edge.
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &barrier,
            &[],
            &[],
        );
    }
}

fn scatter_push_bytes(push: &ScatterPush) -> &[u8] {
    // SAFETY: `#[repr(C)]` over 4-byte scalars with no padding, so every byte
    // of the struct is initialised; the slice borrows `push`.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const ScatterPush).cast::<u8>(),
            std::mem::size_of::<ScatterPush>(),
        )
    }
}

fn blade_push_bytes(push: &BladePush) -> &[u8] {
    // SAFETY: as `scatter_push_bytes`.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const BladePush).cast::<u8>(),
            std::mem::size_of::<BladePush>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: #4181 / CONC-D2-01 — the scatter's publish barrier must
    /// cover the counter readback copy, not just the draw.
    ///
    /// `groundcover_scatter.comp` writes `counter_buffer` via atomics; the
    /// single barrier after the dispatch used to name only the draw's
    /// consumers (`DRAW_INDIRECT | VERTEX_SHADER` /
    /// `INDIRECT_COMMAND_READ | SHADER_READ`), and the `cmd_copy_buffer`
    /// immediately below it read the same buffer with no dependency on the
    /// compute write at all.
    ///
    /// Rendering was never affected — the draw reads through the
    /// correctly-published half of this same barrier. What was undefined is
    /// the readback `harvest` decodes into `GroundCoverStats`, which EXAL
    /// ground-cover tuning reads directly (#4054): silently wrong telemetry
    /// driving tuning decisions, which no rendering assertion would catch.
    #[test]
    fn scatter_publish_barrier_covers_the_counter_readback_copy() {
        // Scoped to the production portion so this test's own literals
        // cannot satisfy it.
        let src = crate::source_scan::production_text(include_str!("frame.rs"));

        let dispatch_at = src
            .find("device.cmd_dispatch(cmd, self.frame_chunk_count, 1, 1);")
            .expect("the scatter dispatch must still exist under this spelling");
        let copy_at = src[dispatch_at..]
            .find("device.cmd_copy_buffer(")
            .expect("the counter readback copy must still follow the scatter dispatch")
            + dispatch_at;
        let between = &src[dispatch_at..copy_at];

        assert!(
            between.contains("vk::PipelineStageFlags::TRANSFER"),
            "the barrier between the scatter dispatch and the counter readback must \
             name TRANSFER in its dst stage mask (#4181)"
        );
        assert!(
            between.contains("vk::AccessFlags::TRANSFER_READ"),
            "the barrier between the scatter dispatch and the counter readback must \
             name TRANSFER_READ in its dst access mask (#4181)"
        );
        // The draw's half must survive too — this is a widening, not a swap.
        assert!(
            between.contains("vk::PipelineStageFlags::DRAW_INDIRECT")
                && between.contains("vk::AccessFlags::INDIRECT_COMMAND_READ"),
            "widening the dst scope must not drop the draw's own edge"
        );
    }

    /// A negative or non-finite `dt` turns the decay into growth without
    /// bound (`exp(-k·dt)`, dt < 0), and a hitch must not wipe a trail.
    #[test]
    fn the_field_clamps_the_frame_delta() {
        for (input, expected) in [
            (-1.0f32, 0.0f32),
            (f32::NAN, 0.0),
            (10.0, 0.25),
            (0.016, 0.016),
        ] {
            let (mut prev, mut half) = (None, 0u32);
            let state = advance_field_state(&mut prev, &mut half, [0.0; 3], input, 0);
            assert_eq!(state.current[2], expected, "dt {input} clamped wrong");
        }
    }

    /// The origin must land on the texel grid, or the compute pass's
    /// reprojection needs a filtered fetch and the field smears into itself.
    #[test]
    fn the_field_origin_snaps_to_the_texel_grid() {
        let (mut prev, mut half) = (None, 0u32);
        // First update has no previous half to reproject from.
        let first = advance_field_state(&mut prev, &mut half, [1234.5, 0.0, -987.25], 0.016, 3);
        assert_eq!(first.previous[3], 0.0);
        assert_eq!(first.current[3], 0.0, "first frame writes half 0");
        for axis in [first.current[0], first.current[1]] {
            assert_eq!(
                axis % GROUNDCOVER_INTERACTION_TEXEL_UNITS,
                0.0,
                "origin {axis} is off the texel grid"
            );
        }
        // Second update flips the half and can reproject.
        let second = advance_field_state(&mut prev, &mut half, [1234.5, 0.0, -987.25], 0.016, 3);
        assert_eq!(second.current[3], 1.0);
        assert_eq!(second.previous[3], 1.0);
        assert_eq!(second.previous[0], first.current[0]);
        assert_eq!(second.previous[1], first.current[1]);
        assert_eq!(second.previous[2], 3.0, "disturber count rides previous.z");
    }

    /// #4293 — the counter clear's seed fills must be sequenced after the zero
    /// fill by a device edge.
    ///
    /// Three `vkCmdFillBuffer`s hit the one shared counter buffer: a
    /// whole-buffer zero, then two 4-byte `EXTREMA_MIN_SEED` writes into
    /// ranges the zero already covered. With nothing between them their order
    /// is undefined under the sync model this crate follows, and sync
    /// validation reports each seed as WRITE_AFTER_WRITE — the ten standing
    /// hazards a `BYRO_VALIDATION=1` Skyrim tundra capture showed, and which a
    /// TRANSFER → TRANSFER barrier took to zero. If the zero landed last, both
    /// `atomicMin` extrema would start at 0 and the `d_ground` / `view_dist`
    /// minima EXAL tuning reads would report 0.
    ///
    /// `cargo test` cannot run sync validation, so this pins the shape that
    /// validation accepted: a TRANSFER-to-TRANSFER barrier between the zero
    /// fill and the first seed fill.
    #[test]
    fn counter_seed_fills_are_sequenced_after_the_zero_fill() {
        let production = crate::source_scan::production_text(include_str!("frame.rs"));
        let zero = production
            .find("device.cmd_fill_buffer(cmd, counters.buffer, 0, counters.size, 0);")
            .expect("the whole-buffer zero fill");
        let after_zero = &production[zero..];
        let first_seed = after_zero
            .find("EXTREMA_MIN_SEED")
            .expect("the extrema seed fills follow the zero fill");
        let between = &after_zero[..first_seed];
        assert!(
            between.contains("buffer_barrier(")
                && between.contains(
                    "vk::PipelineStageFlags::TRANSFER,\n                vk::AccessFlags::TRANSFER_WRITE,\n                vk::PipelineStageFlags::TRANSFER,"
                ),
            "a TRANSFER -> TRANSFER barrier must separate the zero fill from the seed fills \
             (#4293); without it sync validation reports two WRITE_AFTER_WRITE hazards per \
             scatter frame"
        );
    }

    /// Tier changes are draw representations, not residency changes. Pin both
    /// streams to the one arena and make the shader publish a no-op command
    /// for both of an inactive ring slot.
    #[test]
    fn tiered_indirect_streams_preserve_fixed_blade_slabs() {
        let scatter = include_str!("../../../shaders/groundcover_scatter.comp");
        let blade = include_str!("../../../shaders/groundcover_blade.vert");
        let module = crate::source_scan::production_text(include_str!("frame.rs"));
        assert_eq!(GROUNDCOVER_INDIRECT_STREAMS, 3);
        assert!(scatter.contains("uint midIndex = chunkIdx + GROUNDCOVER_MAX_CHUNKS;"));
        assert!(scatter.contains("gcDraws[midIndex].firstVertex = sliceBase * midVertsPerPoint;"));
        assert!(scatter.contains("gcDraws[midIndex].instanceCount = 0u;"));
        assert!(scatter.contains("uint cardIndex = chunkIdx + 2u * GROUNDCOVER_MAX_CHUNKS;"));
        assert!(scatter.contains("gcDraws[cardIndex].firstVertex = (sliceBase / cardCluster)"));
        assert!(scatter.contains("uint midVertsPerPoint = GROUNDCOVER_BLADE_SEGMENTS_MID\n                * GROUNDCOVER_VERTS_PER_SEGMENT;"));
        assert!(blade.contains("GC_DEBUG_POINTS == 1u || GC_LOD_TIER == 1u || cardTier"));
        assert!(blade.contains("halfWidth * float(GROUNDCOVER_BLADES_PER_POINT)"));
        assert!(blade.contains("width * GROUNDCOVER_MAX_WIDTH_MULTIPLIER"));
        assert!(blade.contains("bool cardTier = GC_LOD_TIER == 2u;"));
        assert!(module.contains("tier * GROUNDCOVER_MAX_CHUNKS as u64 * GC_DRAW_INDIRECT_STRIDE"));
    }
}
