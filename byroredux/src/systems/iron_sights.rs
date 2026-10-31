//! Aiming down the sights — the Story Manager's `IRON` event (M43).
//!
//! Fallout 4 raises `IronSights` when the player aims a gun. The engine has no aim state, so this is the
//! smallest one, and it is **engine policy**: the player aims while holding the Attack/Block-secondary button
//! (the right mouse button, bound to [`InputAction::Block`]) with a weapon equipped that *uses ammo* — the
//! plain data difference between a gun or bow and a melee weapon, with no per-game animation-type enum
//! involved. The event fires on the frame the aim begins (the rising edge), with the player in `R1`; the
//! slots of the event are undocumented and the quests under it read none. There is no zoom, spread change or
//! camera change: aiming here is a state and an event, nothing the renderer sees. Idle on a title whose
//! Story Manager tree has no `IRON` root.

use crate::interaction::{ActionState, InputAction};
use crate::systems::PlayerMode;
use byroredux_core::ecs::components::EquippedWeapon;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_scripting::story_manager::{raise_story_event, SmTree, StoryEvent};
use std::collections::HashSet;

/// The `SMEN` mnemonic of the iron-sights event.
pub(crate) const IRON: [u8; 4] = *b"IRON";

/// Weapon bases that use ammo. `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub(crate) struct AimWeapons(HashSet<u32>);

impl Resource for AimWeapons {}

pub(crate) fn install_aim_weapons(world: &mut World, index: &EsmIndex) {
    let weapons = index
        .items
        .values()
        .filter(|item| matches!(item.kind, ItemKind::Weapon { ammo_form, .. } if ammo_form != 0))
        .map(|item| item.form_id)
        .collect();
    world.insert_resource(AimWeapons(weapons));
}

/// Whether the player is aiming. `NOT_SAVED_BY_DESIGN`: held input, re-read every frame.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct IronSightsState {
    pub(crate) aiming: bool,
}

impl Resource for IronSightsState {}

/// System: track the aim and raise `IRON` when it begins.
pub(crate) fn iron_sights_system(world: &World, _dt: f32) {
    let has_root = world
        .try_resource::<SmTree>()
        .is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&IRON));
    if !has_root {
        return;
    }
    let player = crate::systems::PlayerEntity::get(world);
    let holding = world.try_resource::<ActionState>().is_some_and(|actions| actions.is_held(InputAction::Block));
    let armed = player
        .and_then(|player| world.get::<EquippedWeapon>(player).map(|weapon| weapon.base_form_id))
        .is_some_and(|base| world.try_resource::<AimWeapons>().is_some_and(|guns| guns.0.contains(&base)));
    let aiming = holding
        && armed
        && world.try_resource::<PlayerMode>().is_some_and(|mode| *mode == PlayerMode::Character)
        && crate::systems::player_can_act(world);
    let began = match world.try_resource_mut::<IronSightsState>() {
        Some(mut state) => {
            let began = aiming && !state.aiming;
            state.aiming = aiming;
            began
        }
        None => false,
    };
    if let (true, Some(player)) = (began, player) {
        raise_story_event(
            world,
            player,
            StoryEvent {
                mnemonic: IRON,
                reference_1: player,
                reference_2: None,
                location_1: None,
                location_2: None,
                scripted: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::InventoryIndex;
    use byroredux_plugin::esm::records::{SmNodeKind, SmNodeRecord};
    use std::collections::HashMap;

    const RIFLE: u32 = 0x4F;
    const KNIFE: u32 = 0x50;

    fn equipped(base: u32) -> EquippedWeapon {
        EquippedWeapon {
            inventory_index: InventoryIndex(0),
            base_form_id: base,
            damage: 10.0,
            reach: 0.0,
            speed: 0.0,
        }
    }

    fn setup(root: bool) -> (World, byroredux_core::ecs::storage::EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<EquippedWeapon>();
        let records = if root {
            HashMap::from([(0x10, SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(IRON), ..Default::default() })])
        } else {
            HashMap::new()
        };
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(ActionState::default());
        world.insert_resource(IronSightsState::default());
        world.insert_resource(AimWeapons(HashSet::from([RIFLE])));
        (world, player)
    }

    fn count(world: &World) -> usize {
        world.query::<StoryEvent>().map(|e| e.iter().filter(|(_, e)| e.mnemonic == IRON).count()).unwrap_or(0)
    }

    /// Holding the aim button with a gun raises `IRON` once; a knife, or holding on, raises nothing more.
    #[test]
    fn aiming_a_gun_raises_iron_sights_on_the_rising_edge_only() {
        let (mut world, player) = setup(true);
        world.insert(player, equipped(RIFLE));
        iron_sights_system(&world, 0.016);
        assert_eq!(count(&world), 0, "button not held");
        world.resource_mut::<ActionState>().press_for_test(InputAction::Block);
        iron_sights_system(&world, 0.016);
        assert_eq!(count(&world), 1);
        world.query_mut::<StoryEvent>().unwrap().remove(player);
        iron_sights_system(&world, 0.016);
        assert_eq!(count(&world), 0, "still aiming: not a new aim");
    }

    #[test]
    fn a_melee_weapon_or_a_game_without_the_event_never_aims() {
        let (mut world, player) = setup(true);
        world.insert(player, equipped(KNIFE));
        world.resource_mut::<ActionState>().press_for_test(InputAction::Block);
        iron_sights_system(&world, 0.016);
        assert_eq!(count(&world), 0, "ammo-less weapon");
        let (mut world, player) = setup(false);
        world.insert(player, equipped(RIFLE));
        world.resource_mut::<ActionState>().press_for_test(InputAction::Block);
        iron_sights_system(&world, 0.016);
        assert_eq!(count(&world), 0, "no IRON root");
    }
}
