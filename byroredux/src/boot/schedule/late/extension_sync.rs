//! Engine state published into the extension host before any callback runs.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Engine state published into the extension host before any callback runs.
pub(super) fn register_extension_sync_systems(scheduler: &mut Scheduler) {
    // Custom events committed on the previous frame are drained before any
    // callback in this frame can publish another one. This is the scheduler
    // boundary that prevents nested/reentrant guest execution.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_engine_settings_sync_system,
        Access::new()
            .reads_resource::<byroredux_core::settings::SettingsRegistry>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_content_catalog_sync_system,
        Access::new()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .writes_resource::<byroredux_scripting::LegacyObscriptContentCatalog>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_input_bindings_sync_system,
        Access::new()
            .reads_resource::<crate::interaction::ActionBindings>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::extensions::extension_player_entity_sync_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .writes_resource::<crate::extensions::ExtensionHostSlot>(),
    );
}
