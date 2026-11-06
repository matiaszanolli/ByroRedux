//! Ambient AI packages and the package, perk and terminal fragments.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Ambient AI packages and the package, perk and terminal fragments.
pub(super) fn register_package_fragments_systems(scheduler: &mut Scheduler) {
    // M42.9 / #2652 — ambient packages observe EvaluatePackageRequest before
    // the SCEN package system drains the transient marker. Time-driven checks
    // are bounded to one pass per in-game minute inside the system.
    scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);
    // M47.2 — the package transitions it reports run their PF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::package_fragment_system);
    // M47.2 — the perk entry runs the host reports run their PRKF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::perk_fragment_system);
    // M47.2 — the terminal menu picks the host reports run their TERM_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::terminal_fragment_system);
}
