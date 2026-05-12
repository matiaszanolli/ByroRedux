//! Debug / demo systems — spinning cube, stats logging.

use byroredux_core::ecs::{DebugStats, DeltaTime, TotalTime, Transform, World};
use byroredux_core::math::Quat;

use crate::components::Spinning;

/// Rotates only entities marked with the Spinning component.
pub(crate) fn spin_system(world: &World, dt: f32) {
    if let Some((sq, mut tq)) = world.query_2_mut::<Spinning, Transform>() {
        for (entity, _) in sq.iter() {
            if let Some(transform) = tq.get_mut(entity) {
                let rotation = Quat::from_rotation_y(dt * 1.0) * Quat::from_rotation_x(dt * 0.3);
                transform.rotation = rotation * transform.rotation;
            }
        }
    }
}

/// Logs engine stats once per second using DebugStats.
///
/// Writes at `log::info!` / target `engine::stats`, so `env_logger`'s
/// default filter (info) surfaces it in release builds without needing
/// `--debug`. Users who want a quieter console can set
/// `RUST_LOG=warn` or target-filter
/// `RUST_LOG=info,engine::stats=warn`. See #366.
pub(crate) fn log_stats_system(world: &World, _dt: f32) {
    let total = world.resource::<TotalTime>().0;
    let dt = world.resource::<DeltaTime>().0;
    let prev = total - dt;

    if prev < 0.0 || total.floor() != prev.floor() {
        let stats = world.resource::<DebugStats>();
        log::info!(
            target: "engine::stats",
            "fps={:.0} avg={:.0} dt={:.2}ms entities={} meshes={} textures={} draws={}",
            stats.fps, stats.avg_fps(), stats.frame_time_ms,
            stats.entity_count, stats.mesh_count, stats.texture_count, stats.draw_call_count,
        );
    }
}
