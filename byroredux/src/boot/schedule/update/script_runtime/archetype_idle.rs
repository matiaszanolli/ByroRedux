//! Archetype idles.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Archetype idles.
pub(super) fn register_archetype_idle_systems(scheduler: &mut Scheduler) {
    // Fallout 4 actors stand in the idle their animation archetype names; a script's
    // `ChangeAnimArchetype` (run above) swaps it.
    scheduler.add_exclusive(Stage::Update, crate::systems::archetype_idle::archetype_idle_system);
}
