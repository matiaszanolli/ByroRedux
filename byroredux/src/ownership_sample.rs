//! Fill an [`OwnershipSnapshot`] from the live engine (EX-08 / #2374).
//!
//! The accounting rules live in `byroredux_core::ecs::resources::ownership`;
//! this module is only the collection side. It is split in two so the half
//! that needs no Vulkan device stays testable:
//!
//! - [`sample_ecs_owners`] — pure over `&World`. Covers every owner that lives
//!   in an ECS component row or resource, which after M44/M45 is most of them:
//!   physics bodies (`PhysicsWorld` resource), audio tracks (`AudioWorld`
//!   resource), script state, particles, water, and the cell-root index.
//! - [`sample_gpu_owners`] — needs `&VulkanContext` for the registries and the
//!   acceleration structures.
//!
//! Neither samples anything it has to allocate for; both are cheap enough to
//! run on the existing throttled telemetry cadence rather than every frame.

use byroredux_core::ecs::{
    MeshHandle, OwnershipSnapshot, ParticleEmitter, TextureHandle, Transform, World,
};
use byroredux_renderer::VulkanContext;

use crate::components::CellRootIndex;

/// Sample every owner class reachable from the ECS.
///
/// Absent resources read as zero rather than panicking: the loose-NIF demo
/// path opts out of physics, headless/CI has no audio device, and test
/// fixtures register neither. A soak run that never installs a subsystem sees
/// a flat zero for it, which is the correct "nothing to reclaim" answer.
pub(crate) fn sample_ecs_owners(world: &World, out: &mut OwnershipSnapshot) {
    use byroredux_core::ecs::components::{CellRoot, WaterPlane};

    out.entities_spawned = world.next_entity_id() as u64;
    out.transform_rows = world.count::<Transform>() as u64;
    out.cell_root_rows = world.count::<CellRoot>() as u64;
    out.particle_emitters = world.count::<ParticleEmitter>() as u64;
    out.water_planes = world.count::<WaterPlane>() as u64;
    out.script_variable_rows = world.count::<byroredux_scripting::ScriptVariables>() as u64;
    out.script_timer_rows = world.count::<byroredux_scripting::ScriptTimer>() as u64;

    // One entry per *resident cell*, not per owned entity — this is the map
    // `unload_cell_inner` drains, so a surplus is a cell root that outlived
    // its unload rather than a straggling child entity.
    out.cell_root_index_entries = world
        .try_resource::<CellRootIndex>()
        .map(|idx| idx.map.len() as u64)
        .unwrap_or(0);

    out.physics_bodies = world
        .try_resource::<byroredux_physics::PhysicsWorld>()
        .map(|pw| pw.body_count() as u64)
        .unwrap_or(0);

    if let Some(audio) = world.try_resource::<byroredux_audio::AudioWorld>() {
        out.audio_active_sounds = audio.active_sound_count() as u64;
        out.audio_pending_oneshots = audio.pending_oneshot_count() as u64;
    }

    // `SoundCache` is a registered `Resource` type that nothing installs yet;
    // `try_resource` keeps this at zero until it is wired, rather than the
    // sampler needing a follow-up edit at that point.
    out.sound_cache_entries = world
        .try_resource::<byroredux_audio::SoundCache>()
        .map(|c| c.len() as u64)
        .unwrap_or(0);
}

/// Sample the GPU-side owners.
///
/// `in_use_meshes` / `in_use_textures` are passed in rather than recomputed:
/// `about_to_wait` already walks the ECS for exactly these two counts to fill
/// [`DebugStats`](byroredux_core::ecs::DebugStats), and duplicating the walk
/// would double a per-frame cost that PERF-D1-NEW-01 deliberately throttled.
pub(crate) fn sample_gpu_owners(
    ctx: &VulkanContext,
    in_use_meshes: usize,
    in_use_textures: usize,
    out: &mut OwnershipSnapshot,
) {
    out.meshes_in_use = in_use_meshes as u64;
    out.textures_in_use = in_use_textures as u64;
    out.meshes_registry = ctx.mesh_registry.len() as u64;
    out.meshes_live_slots = ctx.mesh_registry.live_slot_count() as u64;
    out.textures_registry = ctx.texture_registry.len() as u64;
    out.texture_live_slots = ctx.texture_registry.live_slot_count() as u64;
    out.terrain_tiles = ctx.occupied_terrain_tile_count() as u64;

    if let Some(accel) = ctx.accel_manager.as_ref() {
        // Static + skinned are summed into one class: they have independent
        // lifecycles but the gate's question ("did every BLAS come back?") is
        // the same for both, and splitting them would let a static-for-skinned
        // swap net out to zero.
        out.blas_entries =
            (accel.live_static_blas_count() + accel.live_skinned_blas_count()) as u64;
        out.tlas_instances = accel.tlas_instances_scratch_telemetry().0 as u64;
    }
}

/// Collect a full snapshot. `renderer` is `None` in headless runs, which
/// leaves the GPU classes at zero.
pub(crate) fn sample_all(
    world: &World,
    renderer: Option<&VulkanContext>,
    in_use_meshes: usize,
    in_use_textures: usize,
) -> OwnershipSnapshot {
    let mut snapshot = OwnershipSnapshot::default();
    sample_ecs_owners(world, &mut snapshot);
    if let Some(ctx) = renderer {
        sample_gpu_owners(ctx, in_use_meshes, in_use_textures, &mut snapshot);
    }
    snapshot
}

/// Build the freshest snapshot obtainable from `&World` alone.
///
/// The console has no `VulkanContext` — commands take `&World` — so the GPU
/// classes come from the last [`OwnershipTelemetry`] sample while every ECS
/// class is re-read live. That split matters for the soak: `world.owners
/// cycle` fires the moment an unload settles, and the ECS half is exactly
/// where a same-instant reading changes the verdict. The GPU half is at worst
/// one telemetry cadence stale (~1 s), well inside the harness's settle wait.
pub(crate) fn fresh_snapshot(world: &World) -> OwnershipSnapshot {
    use byroredux_core::ecs::OwnershipTelemetry;
    let mut snapshot = world
        .try_resource::<OwnershipTelemetry>()
        .map(|t| t.current)
        .unwrap_or_default();
    sample_ecs_owners(world, &mut snapshot);
    // `meshes_in_use` / `textures_in_use` are counted by walking ECS rows, not
    // by asking the registries — they are the scene-scoped half of the #637
    // registry-vs-in-use leak pair and drop the instant a cell unload removes
    // the last holder. Re-derive them live too, so a `cycle` sample taken right
    // after unload reflects the drop rather than the pre-unload cadence tick.
    let (meshes, textures) = count_handles_in_use(world);
    snapshot.meshes_in_use = meshes as u64;
    snapshot.textures_in_use = textures as u64;
    snapshot
}

/// Recompute the in-use mesh/texture handle sets from the ECS.
///
/// `about_to_wait` keeps persistent scratch sets for this walk and only
/// refreshes them on a throttled cadence, so a caller outside the frame loop
/// (the `world.owners` console command, which must report *now* rather than up
/// to a second ago) needs its own pass. Returns `(meshes, textures)` as counts
/// of distinct non-zero handles, matching `DebugStats::meshes_in_use` /
/// `textures_in_use` exactly.
pub(crate) fn count_handles_in_use(world: &World) -> (usize, usize) {
    use std::collections::HashSet;
    let mut meshes: HashSet<u32> = HashSet::new();
    let mut textures: HashSet<u32> = HashSet::new();
    if let Some(q) = world.query::<MeshHandle>() {
        for (_, h) in q.iter() {
            if h.0 != 0 {
                meshes.insert(h.0);
            }
        }
    }
    if let Some(q) = world.query::<TextureHandle>() {
        for (_, h) in q.iter() {
            if h.0 != 0 {
                textures.insert(h.0);
            }
        }
    }
    (meshes.len(), textures.len())
}

#[cfg(test)]
#[path = "ownership_sample_tests.rs"]
mod tests;
