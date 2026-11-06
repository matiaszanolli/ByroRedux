//! Item-event handling and the end-of-Late drain of the one-frame event markers.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Item-event handling and the end-of-Late drain of the one-frame event markers.
pub(super) fn register_script_cleanup_systems(scheduler: &mut Scheduler) {
    // M47.3 — the scripts of items in inventories hear this frame's item and equipment
    // batches (OnAdd / OnDrop / OnEquip / OnUnequip) before the cleanup drains them.
    scheduler.add_exclusive(
        Stage::Late,
        byroredux_scripting::obscript_objects::obscript_item_system,
    );
    scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);
    // The markers are drained only after every consumer has run.
    scheduler.run_after(
        byroredux_core::ecs::system_name(&byroredux_scripting::event_cleanup_system),
        byroredux_core::ecs::system_name(&crate::extensions::extension_activation_dispatch_system),
    );
    scheduler.run_after(
        byroredux_core::ecs::system_name(&byroredux_scripting::event_cleanup_system),
        byroredux_core::ecs::system_name(&crate::extensions::extension_cell_load_dispatch_system),
    );
    scheduler.run_after(
        byroredux_core::ecs::system_name(&byroredux_scripting::event_cleanup_system),
        byroredux_core::ecs::system_name(
            &crate::npc_spawn::loot_appearance::equipment_appearance_system,
        ),
    );
}
