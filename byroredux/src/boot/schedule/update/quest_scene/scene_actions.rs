//! Scene packages, two-state activators and scene dialogue.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// Scene packages, two-state activators and scene dialogue.
pub(super) fn register_scene_actions_systems(scheduler: &mut Scheduler) {
    // PACK actions resolve their Skyrim PKCU template/data inputs, move actors
    // toward authored invisible-marker coordinates for Travel-family leaves,
    // and queue Done completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_package_system);
    // Package Activate leaves emit the same canonical OnActivate marker as
    // player interaction — but they QUEUE it through
    // `PendingFragmentActivations` rather than inserting it here (#3936),
    // because `scene_package_system` above runs after
    // `rumble_on_activate_dispatch` and `quest_advance_dispatch`. The
    // early `fragment_activation_flush_system` delivers it to every
    // consumer next frame. This registration consumes the flushed
    // marker so GetVMScriptVariable phase gates observe the updated
    // two-state activator, before end-of-frame event cleanup.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::two_state_activator_system,
    );
    // Dialogue consumes the ActionStarted batch emitted immediately above,
    // exposes authored INFO subtitle/presentation state for the rest of the
    // frame, and queues completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_dialogue_system);
}
