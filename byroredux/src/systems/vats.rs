//! VATS mode machine (#6350) — the enter/exit transitions on the VATS
//! key edge, and (Phase 3c) the targeting scan + queue execution that
//! ride the same resource.
//!
//! The system runs in `Stage::Early` after `player_controller_system`
//! (so this frame's `ActionState` edges are refreshed) and — like every
//! Early system — it runs even while the world is frozen: dt is a
//! parameter, not a precondition, the same property the loading cover's
//! debug drain already relies on.

use crate::components::{VatsMode, VatsState};
use crate::interaction::ActionState;
use crate::interaction::InputAction::Vats;
use crate::systems::{player_can_act, PlayerMode};
use byroredux_core::character::VatsModel;
use byroredux_core::ecs::World;

/// Enter/exit VATS on the bound key's press edge.
///
/// Entry gates (each its own degraded "no", never a failure):
/// * a live [`VatsModel`] — non-Fallout loads never enter;
/// * `player_can_act` — a dead player, or none at all (fly-cam, tests);
/// * the scripting layer's `Control::Vats` domain enabled
///   (`InputEnableLayer`'s first reader — absent resource = enabled).
///
/// Exit clears the scan and queue; un-executed AP reservations are the
/// caller's to refund (Phase 3c's confirm path owns the spend).
pub(crate) fn vats_mode_system(world: &World, _dt: f32) {
    let pressed = world
        .try_resource::<ActionState>()
        .is_some_and(|actions| actions.was_pressed(Vats));
    if !pressed {
        return;
    }
    // Read-only mode check first, then drop the guard before the gate
    // checks: `player_can_act` itself reads `VatsState` (the VATS
    // suppression gate), and holding this write lock across it is the
    // same-thread deadlock the lock tracker exists to catch.
    let entering = world
        .try_resource::<VatsState>()
        .is_some_and(|vats| vats.mode == VatsMode::None);
    if !entering {
        if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
            vats.exit_all();
        }
        return;
    }
    // Entry gates.
    if world.try_resource::<VatsModel>().is_none() {
        return;
    }
    if !player_can_act(world) {
        return;
    }
    if world
        .try_resource::<PlayerMode>()
        .is_none_or(|mode| *mode == PlayerMode::FlyCam)
    {
        return;
    }
    if !control_vats_enabled(world) {
        return;
    }
    if let Some(mut vats) = world.try_resource_mut::<VatsState>() {
        vats.mode = VatsMode::TargetSelect;
    }
}

/// The `Control::Vats` input-enable domain — the first reader of the
/// domain the scripting crate has tracked since it was written
/// (`input_layers.rs`: "VATS … answer the Is…Enabled queries; nothing
/// reads them yet"). Absent layer resource or absent layer stack
/// defaults to enabled.
fn control_vats_enabled(world: &World) -> bool {
    world
        .try_resource::<byroredux_scripting::input_layers::InputLayers>()
        .is_none_or(|layers| layers.enabled(byroredux_scripting::input_layers::Control::Vats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::VatsMode;
    use crate::components::InputState;
    use crate::interaction::{refresh_action_state, ActionBindings};
    use crate::systems::PlayerEntity;
    use byroredux_core::ecs::components::Dead;
    use byroredux_scripting::input_layers::{Control, InputLayers};
    use winit::keyboard::KeyCode;

    fn armed_world() -> World {
        let mut world = World::new();
        world.insert_resource(InputState::default());
        world.insert_resource(ActionBindings::default());
        world.insert_resource(ActionState::default());
        world.insert_resource(VatsState::default());
        world.insert_resource(VatsModel::FALLOUT3);
        world.insert_resource(PlayerMode::Character);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        world
    }

    fn press_vats(world: &World) {
        world
            .resource_mut::<InputState>()
            .keys_held
            .insert(KeyCode::KeyV);
        refresh_action_state(world);
    }

    fn release_all(world: &World) {
        world.resource_mut::<InputState>().keys_held.clear();
        refresh_action_state(world);
    }

    #[test]
    fn vats_key_enters_and_exits_target_select() {
        let world = armed_world();
        assert_eq!(
            world.resource::<VatsState>().mode,
            VatsMode::None,
            "idle state"
        );
        assert_eq!(world.resource::<VatsState>().world_time_scale(), 1.0);

        press_vats(&world);
        vats_mode_system(&world, 0.0);
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::TargetSelect);
        // The published world time scale: FO3/FNV freeze solid.
        assert_eq!(vats.world_time_scale(), 0.0);
        // Normal player input is suppressed while targeting.
        assert!(!crate::systems::player_can_act(&world));
        drop(vats);

        // Release, press again: exit restores the running world.
        release_all(&world);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        let vats = world.resource::<VatsState>();
        assert_eq!(vats.mode, VatsMode::None);
        assert_eq!(vats.world_time_scale(), 1.0);
        assert!(crate::systems::player_can_act(&world));
    }

    #[test]
    fn entry_refuses_without_a_model_a_live_player_or_on_foot() {
        // No VatsModel (a non-Fallout load): never enters.
        let mut world = armed_world();
        world.remove_resource::<VatsModel>();
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Dead player: no.
        let mut world = armed_world();
        let player = world.resource::<PlayerEntity>().0.unwrap();
        world.insert(player, Dead);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Fly-cam: no.
        let mut world = armed_world();
        world.remove_resource::<PlayerMode>();
        world.insert_resource(PlayerMode::FlyCam);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);
    }

    /// #6350 — the first reader of the `Control::Vats` input-enable
    /// domain: a Papyrus layer that disables VATS blocks entry, and an
    /// absent layer resource defaults to enabled.
    #[test]
    fn entry_honours_the_vats_control_domain() {
        let mut world = armed_world();
        let mut layers = InputLayers::default();
        let layer = layers.create();
        layers.set_enabled(layer, Control::Vats, false);
        world.insert_resource(layers);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::None);

        // Re-enable through the layer: entry works again.
        let mut layers = world.resource_mut::<InputLayers>();
        layers.set_enabled(layer, Control::Vats, true);
        drop(layers);
        release_all(&world);
        press_vats(&world);
        vats_mode_system(&world, 0.0);
        assert_eq!(world.resource::<VatsState>().mode, VatsMode::TargetSelect);
    }
}
