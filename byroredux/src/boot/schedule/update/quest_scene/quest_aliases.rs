//! Quest alias refresh and readiness.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Quest alias refresh and readiness.
pub(super) fn register_quest_aliases_systems(scheduler: &mut Scheduler) {
    // Quest aliases are a general QUST facility, not a SCEN implementation
    // detail. Refresh after startup/cell candidate changes even when the load
    // order contains no scene records; the dirty fast path is allocation-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_refresh_system,
    );
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_readiness_stage_system,
    );
}
