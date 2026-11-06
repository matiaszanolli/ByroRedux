//! `Stage::PostUpdate` registrations (#3855, split from `boot.rs`).

use byroredux_core::ecs::Scheduler;

use super::ai_locomotion_enabled;

mod transform_propagation;
mod particles;
mod ambient_ai;
mod combat_feedback;
mod world_bound;

/// `Stage::PostUpdate` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_post_update_systems(scheduler: &mut Scheduler) {
    transform_propagation::register_transform_propagation_systems(scheduler);
    particles::register_particles_systems(scheduler);
    ambient_ai::register_ambient_ai_systems(scheduler);
    combat_feedback::register_combat_feedback_systems(scheduler);
    world_bound::register_world_bound_systems(scheduler);
}
