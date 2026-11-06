//! Combat input, faction hostility, guard rush and NPC combat AI, in the order the hit contract needs them.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;

mod combat_input;
mod faction_hostility;
mod guard_rush;
mod npc_combat_ai;

/// Combat input, faction hostility, guard rush and NPC combat AI, in the order the hit contract needs them.
pub(super) fn register_combat(scheduler: &mut Scheduler) {
    combat_input::register_combat_input_systems(scheduler);
    faction_hostility::register_faction_hostility_systems(scheduler);
    guard_rush::register_guard_rush_systems(scheduler);
    npc_combat_ai::register_npc_combat_ai_systems(scheduler);
}
