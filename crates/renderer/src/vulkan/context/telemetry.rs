//! The `fill_*` telemetry accessors — read-only projections of context
//! state into the debug-UI / ECS stats structs.
//!
//! #4217 — five methods, ~390 lines, none of which touch init, draw, resize
//! or teardown: they answer "what did the renderer just do", which is not a
//! lifecycle phase, so they had no home in the existing split. Moving them
//! also reunited the scratch-telemetry maintenance rule with
//! `fill_scratch_telemetry`; in `mod.rs` that doc block had drifted onto
//! `fill_upscaler_telemetry`, so the rule appeared to govern the wrong
//! function.

use super::super::scene_buffer;
use super::draw;
use super::{VulkanContext, SKIN_MAX_SLOTS};
use ash::vk;

impl VulkanContext {
    /// Refresh the world-visible upscaler telemetry. Only rewrites the
    /// string when the described state actually changed (context creation,
    /// resize, preset switch, or a latched dispatch failure), so the steady
    /// state costs one string compare per frame rather than an allocation.
    pub fn fill_upscaler_telemetry(&self, telemetry: &mut byroredux_core::ecs::UpscalerTelemetry) {
        // #2821 — carry the bracket's `_active` flag across with the value.
        // `0.0` from an absent timer pool, an upscaler that never dispatched,
        // and a genuinely sub-microsecond upscale are three different states;
        // `ctx.upscaler` used to print all three as `0.000`.
        let (upscale_ms, upscale_active) =
            self.gpu_timers.as_ref().map_or((0.0, false), |timers| {
                let snapshot = timers.last_snapshot();
                (snapshot.upscale_ms, snapshot.upscale_active)
            });
        telemetry.gpu_ms = upscale_ms;
        telemetry.gpu_ms_active = upscale_active;
        let Some(ref upscaler) = self.frame_upscaler else {
            telemetry.summary.clear();
            return;
        };
        let summary = upscaler.telemetry();
        if telemetry.summary != summary {
            telemetry.summary = summary;
        }
    }

    /// Snapshot every persistent CPU-side scratch `Vec` owned by the
    /// renderer (R6). The rows land on the [`ScratchTelemetry`]
    /// resource via [`crate::vulkan::context::VulkanContext`] each
    /// frame and are surfaced by the `ctx.scratch` console command.
    ///
    /// **Maintenance**: every persistent `Vec` scratch declared in this
    /// crate must show up here. Adding a new scratch field on
    /// `VulkanContext` (or its sub-managers) without a row added below
    /// reintroduces the pre-R6 blind spot where scratches grow with
    /// zero observability.
    ///
    /// Reuses the caller's `Vec` to avoid a per-frame allocation in
    /// the telemetry path itself. Capacity stabilises at the number of
    /// declared scratches after the first frame.
    pub fn fill_scratch_telemetry(&self, rows: &mut Vec<byroredux_core::ecs::ScratchRow>) {
        use byroredux_core::ecs::ScratchRow;
        use std::mem::size_of;

        rows.clear();
        rows.push(ScratchRow {
            name: "gpu_instances_scratch",
            len: self.gpu_instances_scratch.len(),
            capacity: self.gpu_instances_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuInstance>(),
        });
        rows.push(ScratchRow {
            name: "frame_lights_scratch",
            len: self.frame_lights_scratch.len(),
            capacity: self.frame_lights_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuLight>(),
        });
        rows.push(ScratchRow {
            name: "previous_models_scratch",
            len: self.previous_models_scratch.len(),
            capacity: self.previous_models_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuPreviousModel>(),
        });
        rows.push(ScratchRow {
            name: "batches_scratch",
            len: self.batches_scratch.len(),
            capacity: self.batches_scratch.capacity(),
            elem_size_bytes: size_of::<draw::DrawBatch>(),
        });
        // #2486 / D5-01 — the two rigid-motion history maps are members of
        // the same per-frame scratch cluster (`clear` + `reserve` + shrink),
        // so they belong in the same report. Like the `skin_dispatch_seen`
        // HashSet row above, `capacity × elem_size` under-counts a hash
        // table's real footprint (no control bytes / load-factor slack);
        // it is a proportional signal, not an allocator-accurate figure.
        for (name, map) in [
            ("previous_rigid_models", &self.history.previous_rigid_models),
            (
                "current_rigid_models_scratch",
                &self.current_rigid_models_scratch,
            ),
        ] {
            rows.push(ScratchRow {
                name,
                len: map.len(),
                capacity: map.capacity(),
                elem_size_bytes: size_of::<(u32, [f32; 16])>(),
            });
        }
        rows.push(ScratchRow {
            name: "indirect_draws_scratch",
            len: self.indirect_draws_scratch.len(),
            capacity: self.indirect_draws_scratch.capacity(),
            elem_size_bytes: size_of::<vk::DrawIndexedIndirectCommand>(),
        });
        rows.push(ScratchRow {
            name: "terrain_tile_scratch",
            len: self.terrain_tile_scratch.len(),
            capacity: self.terrain_tile_scratch.capacity(),
            elem_size_bytes: size_of::<scene_buffer::GpuTerrainTile>(),
        });
        // #1133 — skin-path scratches. The HashSet's heap footprint
        // isn't directly measurable through the public API; report
        // its `len` against `capacity` for what we can see.
        rows.push(ScratchRow {
            name: "skin_dispatch_seen_scratch",
            len: self.skin_dispatch_seen_scratch.len(),
            capacity: self.skin_dispatch_seen_scratch.capacity(),
            elem_size_bytes: size_of::<byroredux_core::ecs::storage::EntityId>(),
        });
        rows.push(ScratchRow {
            name: "skin_dispatches_scratch",
            len: self.skin_dispatches_scratch.len(),
            capacity: self.skin_dispatches_scratch.capacity(),
            elem_size_bytes: size_of::<(
                byroredux_core::ecs::storage::EntityId,
                super::super::skin_compute::SkinPushConstants,
                vk::Buffer,
                u32,
                u32,
            )>(),
        });
        rows.push(ScratchRow {
            name: "skin_first_sight_builds_scratch",
            len: self.skin_first_sight_builds_scratch.len(),
            capacity: self.skin_first_sight_builds_scratch.capacity(),
            elem_size_bytes: size_of::<(
                byroredux_core::ecs::storage::EntityId,
                vk::Buffer,
                u32,
                vk::Buffer,
                u32,
            )>(),
        });
        rows.push(ScratchRow {
            name: "skin_built_this_frame_scratch",
            len: self.skin_built_this_frame_scratch.len(),
            capacity: self.skin_built_this_frame_scratch.capacity(),
            elem_size_bytes: size_of::<byroredux_core::ecs::storage::EntityId>(),
        });
        if let Some(accel) = &self.accel_manager {
            let (len, capacity) = accel.tlas_instances_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
            // #3693 — the two sibling `AccelerationManager` scratches next
            // to `tlas_instances_scratch` that never got a row.
            let (len, capacity) = accel.tlas_addresses_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_addresses_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<u64>(),
            });
            let (len, capacity) = accel.tlas_missing_samples_scratch_telemetry();
            rows.push(ScratchRow {
                name: "tlas_missing_samples_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<String>(),
            });
        } else {
            rows.push(ScratchRow {
                name: "tlas_instances_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<vk::AccelerationStructureInstanceKHR>(),
            });
            rows.push(ScratchRow {
                name: "tlas_addresses_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<u64>(),
            });
            rows.push(ScratchRow {
                name: "tlas_missing_samples_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<String>(),
            });
        }
        // #3061 / dim_2 converted this to `FxHashSet` for its hasher; #3693
        // is the telemetry half that was never added alongside it. Same
        // len/capacity-only caveat as the other hash-container rows above.
        rows.push(ScratchRow {
            name: "blend_seen_scratch",
            len: self.blend_seen_scratch.len(),
            capacity: self.blend_seen_scratch.capacity(),
            elem_size_bytes: size_of::<(u8, u8, bool, bool)>(),
        });
        if let Some(water) = &self.water {
            let (len, capacity) = water.param_scratch_telemetry();
            rows.push(ScratchRow {
                name: "water_param_scratch",
                len,
                capacity,
                elem_size_bytes: size_of::<super::super::water::GpuWaterParams>(),
            });
        } else {
            rows.push(ScratchRow {
                name: "water_param_scratch",
                len: 0,
                capacity: 0,
                elem_size_bytes: size_of::<super::super::water::GpuWaterParams>(),
            });
        }
    }

    /// Snapshot the skinned-BLAS coverage counters from the last
    /// `draw_frame` invocation. Filled into the
    /// [`byroredux_core::ecs::SkinCoverageStats`] resource each frame by
    /// the engine binary, alongside `fill_scratch_telemetry`, and
    /// surfaced by the `skin.coverage` console command.
    ///
    /// The `failed_entity_ids` snapshot caps at 16 IDs to keep the
    /// resource cheap to copy; the full count is in `slots_failed`. IDs
    /// are sampled in HashSet iteration order (non-deterministic) — fine
    /// for diagnostic spot-checks via `byro-dbg`, not a stable
    /// regression key.
    pub fn fill_skin_coverage_stats(&self, stats: &mut byroredux_core::ecs::SkinCoverageStats) {
        let f = self.last_skin_coverage_frame;
        stats.dispatches_total = f.dispatches_total;
        stats.dispatches_skipped = f.dispatches_skipped;
        stats.first_sight_attempted = f.first_sight_attempted;
        stats.first_sight_succeeded = f.first_sight_succeeded;
        stats.refits_attempted = f.refits_attempted;
        stats.refits_succeeded = f.refits_succeeded;
        // #2803 — host-side chain cost, same-frame (not fence-lagged
        // like the GPU brackets below).
        stats.cpu_skin_chain_ms = f.cpu_skin_chain_ns as f32 / 1.0e6;
        // #1194 — GPU timer snapshot. Zeros when timer unavailable
        // (driver lacks timestamp support) or first pipelined cycle
        // hasn't completed.
        if let Some(ref timers) = self.gpu_timers {
            let snap = timers.last_snapshot();
            stats.gpu_skin_dispatch_ms = snap.skin_dispatch_ms;
            stats.gpu_skin_palette_ms = snap.skin_palette_ms;
            stats.gpu_skin_blas_refit_ms = snap.skin_blas_refit_ms;
            stats.gpu_taa_ms = snap.taa_ms;
            stats.gpu_main_render_ms = snap.main_render_ms;
            stats.gpu_tlas_build_ms = snap.tlas_build_ms;
            stats.gpu_cluster_cull_ms = snap.cluster_cull_ms;
            stats.gpu_svgf_ms = snap.svgf_ms;
            stats.gpu_composite_ms = snap.composite_ms;
            stats.gpu_ssao_ms = snap.ssao_ms;
            stats.gpu_bloom_ms = snap.bloom_ms;
            stats.gpu_caustic_splat_ms = snap.caustic_splat_ms;
            stats.gpu_volumetrics_ms = snap.volumetrics_ms;
            stats.gpu_upscale_ms = snap.upscale_ms;
            stats.gpu_presentation_ms = snap.presentation_ms;
            stats.gpu_depth_history_copy_ms = snap.depth_history_copy_ms;
            stats.gpu_sky_cube_ms = snap.sky_cube_ms;
            // #2513 / REN-D20-NEW-03 — copy the "did this bracket actually
            // run" flags too, closing the gap #2278 opened at the producer
            // but nothing downstream ever read.
            stats.gpu_skin_dispatch_active = snap.skin_dispatch_active;
            stats.gpu_skin_palette_active = snap.skin_palette_active;
            stats.gpu_skin_blas_refit_active = snap.skin_blas_refit_active;
            stats.gpu_taa_active = snap.taa_active;
            stats.gpu_main_render_active = snap.main_render_active;
            stats.gpu_tlas_build_active = snap.tlas_build_active;
            stats.gpu_cluster_cull_active = snap.cluster_cull_active;
            stats.gpu_svgf_active = snap.svgf_active;
            stats.gpu_composite_active = snap.composite_active;
            stats.gpu_ssao_active = snap.ssao_active;
            stats.gpu_bloom_active = snap.bloom_active;
            stats.gpu_caustic_splat_active = snap.caustic_splat_active;
            stats.gpu_volumetrics_active = snap.volumetrics_active;
            stats.gpu_upscale_active = snap.upscale_active;
            stats.gpu_presentation_active = snap.presentation_active;
            stats.gpu_depth_history_copy_active = snap.depth_history_copy_active;
            stats.gpu_sky_cube_active = snap.sky_cube_active;
        } else {
            stats.gpu_skin_dispatch_ms = 0.0;
            stats.gpu_skin_palette_ms = 0.0;
            stats.gpu_skin_blas_refit_ms = 0.0;
            stats.gpu_taa_ms = 0.0;
            stats.gpu_main_render_ms = 0.0;
            stats.gpu_tlas_build_ms = 0.0;
            stats.gpu_cluster_cull_ms = 0.0;
            stats.gpu_svgf_ms = 0.0;
            stats.gpu_composite_ms = 0.0;
            stats.gpu_ssao_ms = 0.0;
            stats.gpu_bloom_ms = 0.0;
            stats.gpu_caustic_splat_ms = 0.0;
            stats.gpu_volumetrics_ms = 0.0;
            stats.gpu_upscale_ms = 0.0;
            stats.gpu_presentation_ms = 0.0;
            stats.gpu_depth_history_copy_ms = 0.0;
            stats.gpu_sky_cube_ms = 0.0;
            // No `GpuPerFrameTimers` at all (driver lacks timestamp
            // support) — every bracket is inactive, not just zero.
            stats.gpu_skin_dispatch_active = false;
            stats.gpu_skin_palette_active = false;
            stats.gpu_skin_blas_refit_active = false;
            stats.gpu_taa_active = false;
            stats.gpu_main_render_active = false;
            stats.gpu_tlas_build_active = false;
            stats.gpu_cluster_cull_active = false;
            stats.gpu_svgf_active = false;
            stats.gpu_composite_active = false;
            stats.gpu_ssao_active = false;
            stats.gpu_bloom_active = false;
            stats.gpu_caustic_splat_active = false;
            stats.gpu_volumetrics_active = false;
            stats.gpu_upscale_active = false;
            stats.gpu_presentation_active = false;
            stats.gpu_depth_history_copy_active = false;
            stats.gpu_sky_cube_active = false;
        }
        stats.slots_active = self.skin_slots.len() as u32;
        stats.slot_pool_capacity = if self.skin_compute.is_some() {
            SKIN_MAX_SLOTS
        } else {
            0
        };
        stats.slots_failed = self.failed_skin_slots.len() as u32;
        // #4049 — cumulative count, not a per-frame gauge: the upload can
        // fail on frame N and succeed on frame N+1 without the pool being
        // reallocated, unlike `failed_skin_slots`'s "cleared on eviction"
        // shape, so there is no natural point to zero this at.
        stats.bind_inverse_upload_failures = self.bind_inverse_upload_failure_count;
        let (morph_slots, morph_bytes) = self.morph_memory_usage();
        stats.morph_slots = morph_slots;
        stats.morph_bytes = morph_bytes;
        stats.failed_entity_ids.clear();
        for &eid in self.failed_skin_slots.iter().take(16) {
            stats.failed_entity_ids.push(eid);
        }
    }

    /// Join the last frame's RT publication, TLAS membership, and clustered
    /// light capacity counters into the engine's durable integrity resource.
    pub fn fill_rt_integrity_stats(&self, stats: &mut byroredux_core::ecs::RtIntegrityStats) {
        let tlas = self
            .accel_manager
            .as_ref()
            .map(super::super::acceleration::AccelerationManager::integrity_snapshot)
            .unwrap_or_default();
        let cluster = self
            .cluster_cull
            .as_ref()
            .map(super::super::compute::ClusterCullPipeline::latest_telemetry)
            .unwrap_or_default();
        let (lights_submitted, lights_uploaded) = self.scene_buffers.light_upload_counts();

        stats.frame = tlas.frame;
        stats.sampled = self.frame_counter > 0;
        stats.rt_supported = self.device_caps.ray_query_supported;
        stats.rt_flag = self.rt_flag_last_frame;
        stats.tlas_build_succeeded = self.tlas_build_succeeded_last_frame;
        stats.tlas_eligible = tlas.eligible;
        stats.tlas_emitted = tlas.emitted;
        stats.missing_skinned_blas = tlas.missing_skinned_blas;
        stats.missing_rigid_blas = tlas.missing_rigid_blas;
        stats.missing_ssbo_instance = tlas.missing_ssbo_instance;
        stats.lights_submitted = lights_submitted;
        stats.lights_uploaded = lights_uploaded;
        stats.lights_dropped = lights_submitted.saturating_sub(lights_uploaded);
        stats.cluster_sampled = cluster.sampled;
        stats.cluster_overflowed = cluster.overflowed_clusters;
        stats.cluster_dropped = cluster.dropped_lights;
        stats.cluster_max_lights = cluster.max_lights;

        // #3999 — BLAS residency + the deferred-destroy backlog. The
        // acceleration manager already computed all five; each sat behind a
        // `pub` accessor with no caller, so the exact overshoot
        // `REN-2026-09-06-D1-01` describes was undiagnosable in the field.
        // Read through the accessors rather than the fields so this is a
        // genuine consumer of the documented surface, not a second path
        // around it.
        if let Some(accel) = self.accel_manager.as_ref() {
            stats.blas_total_bytes = accel.total_blas_bytes();
            stats.blas_static_bytes = accel.static_blas_bytes();
            stats.blas_pending_destroy_bytes = accel.pending_destroy_static_bytes();
            stats.blas_pending_destroy_count = accel.pending_destroy_blas_count() as u32;
            stats.scratch_pending_destroy_count = accel.pending_destroy_scratch_count() as u32;
        }
        // #4117 — the texture-side deferred-destroy backlog, read the same way.
        stats.texture_pending_destroy_count = self.texture_registry.pending_destroy_count() as u32;
    }

    /// #3305 — publish the shadow-mask census from the last TLAS gather.
    ///
    /// Kept separate from [`Self::fill_rt_integrity_stats`] rather than
    /// folded into it: the two answer different questions (did every
    /// eligible draw reach the TLAS, versus which visibility bucket did the
    /// ones that arrived land in), and a missing-BLAS investigation and a
    /// mask investigation want to read them independently.
    pub fn fill_shadow_mask_census(&self, census: &mut byroredux_core::ecs::ShadowMaskCensus) {
        let Some(accel) = self.accel_manager.as_ref() else {
            return;
        };
        let snapshot = accel.shadow_mask_snapshot();
        census.frame = snapshot.frame;
        census.sampled = self.frame_counter > 0;
        census.architecture = snapshot.architecture;
        census.static_prop = snapshot.static_prop;
        census.dynamic_actor = snapshot.dynamic_actor;
        census.foliage = snapshot.foliage;
        census.effect = snapshot.effect;
        census.glass = snapshot.glass;
        census.actor_layer_total = snapshot.actor_layer_total;
        census.actor_diverted_glass = snapshot.actor_diverted_glass;
        census.actor_diverted_alpha_blend = snapshot.actor_diverted_alpha_blend;
        census.actor_diverted_effect_shader = snapshot.actor_diverted_effect_shader;
        census.actor_diverted_fire_refraction = snapshot.actor_diverted_fire_refraction;
    }
}
