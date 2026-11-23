//! Alias and reference script events, parent activation relay and animation events.
//!
//! Split out of `script_runtime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage, World};

/// Alias and reference script events, parent activation relay and animation events.
pub(super) fn register_reference_scripts_systems(scheduler: &mut Scheduler) {
    fn check_alias_scripts_dispatch(world: &World, _dt: f32) {
        crate::asset_provider::check_running_quest_alias_scripts(world)
    }
    // M47.2 — reference scripts the Papyrus VM runs receive this frame's
    // cell-load, activation and trigger events (all produced above). Ahead
    // of quest fragment dispatch, so a stage they set runs its fragment
    // this frame.
    //
    // M47.2 — the alias scripts of quests that run go first: checked when a
    // quest first runs, started (OnInit / OnAliasInit) once its aliases are
    // filled, shut down when it stops. The events of the references the
    // aliases are filled with are delivered by the system after them, with the
    // reference scripts'.
    // A quest's own scripts receive `OnInit` once per save (CK *OnInit*: "on game startup"), ahead of
    // its alias scripts' start.
    fn check_quest_scripts_dispatch(world: &World, _dt: f32) {
        crate::asset_provider::check_quest_own_scripts(world)
    }
    scheduler.add_exclusive(Stage::Update, check_quest_scripts_dispatch);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::quest_script_init_system);
    // LMC-UI-002 — a Mod Configuration Menu script whose OnInit ran in an earlier session registers again.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::mcm::mcm_registration_system);
    // LMC-UI-002 slice 4b — MCM Helper's hotkeys: this frame's key presses and releases run them.
    scheduler.add_exclusive(Stage::Update, crate::mcm_hotkeys::mcm_hotkey_system);
    scheduler.add_exclusive(Stage::Update, check_alias_scripts_dispatch);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::alias_script_event_system);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::reference_script_event_system,
    );
    // M47.2 — activate parents: this frame's activations passed on to the children.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::activate_parent_relay_system);
    // M47.2 — the object graphs' raised events, delivered as `OnAnimationEvent`
    // to the scripts that registered for them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::animation_event_system);
    // M48 GUI-EVT-001 — the registry's menu openings and closings, delivered as
    // `OnMenuOpenCloseEvent` to the scripts that registered for them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::menu_open_close_event_system);
}
