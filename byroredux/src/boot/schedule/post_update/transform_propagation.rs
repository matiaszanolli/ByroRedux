//! Transform propagation.
//!
//! Split out of `post_update.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage, Transform};
use crate::systems::make_transform_propagation_system;

/// Transform propagation.
pub(super) fn register_transform_propagation_systems(scheduler: &mut Scheduler) {
    scheduler.add_to_with_access(
        Stage::PostUpdate,
        make_transform_propagation_system(),
        Access::new()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_core::ecs::Children>()
            // WRITE (was read): the system drains the per-entity
            // Transform change-tracking dirty set, which needs &mut on
            // the storage. Local transforms are still only read.
            .writes::<Transform>()
            .writes::<byroredux_core::ecs::GlobalTransform>(),
    );
}
