//! Custom-event, activation and cell-load delivery to the extension host.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Custom-event, activation and cell-load delivery to the extension host.
pub(super) fn register_extension_event_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_custom_event_dispatch_system,
        Access::new().writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    // SDK Phase 3 — deliver activation only after every built-in consumer has
    // run and immediately before the transient marker is drained. The adapter
    // snapshots `ActivateEvent` and the cloneable host slot, drops both ECS
    // guards, then enters untrusted code and atomically commits own-state
    // commands. `writes_resource` describes the logical mutation behind the
    // slot's Arc<Mutex<_>>, even though no ECS resource guard spans the call.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_activation_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_cell_load_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::OnCellLoadEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
}
