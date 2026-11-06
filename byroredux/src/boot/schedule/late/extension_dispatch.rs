//! Equipment, input, session, hit and update delivery to the extension host, and its setting writes.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Equipment, input, session, hit and update delivery to the extension host, and its setting writes.
pub(super) fn register_extension_dispatch_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_equipment_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::EquipmentEventBatch>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_input_dispatch_system,
        Access::new()
            .reads_resource::<crate::interaction::ActionState>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_session_dispatch_system,
        Access::new()
            .writes_resource::<crate::extensions::SessionEventQueue>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_hit_dispatch_system,
        Access::new()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::Name>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::Transform>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads_resource::<byroredux_core::string::StringPool>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_update_dispatch_system,
        Access::new().writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_setting_write_apply_system,
        Access::new()
            .writes_resource::<byroredux_core::settings::SettingsRegistry>()
            .reads_resource::<crate::settings_io::SettingsPersistence>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
}
