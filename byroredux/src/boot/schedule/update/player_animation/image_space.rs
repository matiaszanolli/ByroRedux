//! Image-space modifiers.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Image-space modifiers.
pub(super) fn register_image_space_systems(scheduler: &mut Scheduler) {
    // Sample freshly delivered ImageSpaceModifier.Apply callbacks after the
    // animation event system so MQ101's blur begins in this rendered frame.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::image_space_modifier_system,
    );
}
