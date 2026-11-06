//! Object-graph steps, cinematic horse routes and vehicle attachment.
//!
//! Split out of `player_animation.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Object-graph steps, cinematic horse routes and vehicle attachment.
pub(super) fn register_cinematics_systems(scheduler: &mut Scheduler) {
    // Animated props' behavior graphs: clip-trigger events move their state machines on.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::object_graph_step_system);
    // Native cart tethers consume the horse ACHR's XLKR marker chain before
    // the attachment pass copies the resulting pose to carts and riders.
    scheduler.add_exclusive(Stage::Update, crate::systems::cinematic_horse_route_system);
    // Resolve horse -> tethered cart -> SetVehicle rider poses before the
    // PostUpdate transform pass propagates the chain through actor skeletons.
    scheduler.add_exclusive(Stage::Update, crate::systems::vehicle_attachment_system);
}
