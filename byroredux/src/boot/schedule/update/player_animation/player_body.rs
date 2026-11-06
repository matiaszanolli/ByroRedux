//! Player body facing.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage, Transform};

/// Player body facing.
pub(super) fn register_player_body_systems(scheduler: &mut Scheduler) {
    // P3 player body — third-person facing. Writes the body root's Transform
    // rotation from `InputState.yaw`, the same look accumulator
    // `camera_follow_system` reads. #4995 — registered in **Update**, not
    // Late: its only input is written between frames (winit input, final
    // before the scheduler runs), so nothing forces it after Physics, and
    // its only consumer is PostUpdate transform propagation. In Late it
    // was composed a frame after the camera used the same yaw, so the body
    // visibly trailed the view on fast turns. Exclusive: the body-root
    // Transform write would pair WriteWrite with the animation batch in the
    // analyzer, and sequencing after that batch keeps the facing
    // authoritative over any animated root rotation. Pinned by
    // `player_body_facing_runs_in_update_before_propagation`.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::player_body::player_body_facing_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::player_body::PlayerBodyRootEntity>()
            .reads_resource::<crate::components::InputState>()
            .reads::<crate::player_body::PlayerBodyRoot>()
            .writes::<Transform>(),
    );
}
