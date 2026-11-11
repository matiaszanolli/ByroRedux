//! The player sitting on furniture: activating a placed furniture that has a sitting position
//! seats the player on it, and moving stands them up.
//!
//! What a seat means to scripts is the [`Seated`] component, the same one a sandboxing NPC
//! carries: `IsCurrentFurnitureRef` reads it. New Vegas's opening needs exactly that — the
//! psych test at Doc Mitchell's starts when `player.IsCurrentFurnitureRef DocMitchellCouchREF`
//! holds inside the couch's trigger. The player keeps their place and pose (no sit animation or
//! snap to the furniture's marker yet); the seat is the state the game logic tests.

use byroredux_core::ecs::components::{Furniture, FurnitureMarkerKind, Seated, SeatedAnimationRestore};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;

use crate::interaction::{ActionState, InputAction};

/// Whether `entity` is placed furniture with a sitting position.
pub(crate) fn is_seat(world: &World, entity: EntityId) -> bool {
    world.get::<Furniture>(entity).is_some_and(|furniture| furniture.markers.iter().any(|marker| marker.kind == FurnitureMarkerKind::Sit))
}

/// The seats the player can sit on now: none while seated already.
pub(crate) fn seat_entities(world: &World) -> Vec<EntityId> {
    let seated = crate::systems::PlayerEntity::get(world).is_some_and(|player| world.get::<Seated>(player).is_some());
    if seated {
        return Vec::new();
    }
    world
        .query::<Furniture>()
        .map(|furniture| {
            furniture
                .iter()
                .filter(|(_, furniture)| furniture.markers.iter().any(|marker| marker.kind == FurnitureMarkerKind::Sit))
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default()
}

/// The movement actions that stand a seated player up.
const STAND_UP_ACTIONS: [InputAction; 4] = [InputAction::MoveForward, InputAction::MoveBackward, InputAction::StrafeLeft, InputAction::StrafeRight];

/// Seat the player on the furniture they activated; stand them up when they move while movement
/// is theirs to use (a script that disables movement keeps them seated, as Doc's couch does).
pub(crate) fn player_seat_system(world: &World, _dt: f32) {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let activated: Vec<EntityId> = world
        .query::<byroredux_scripting::ActivateEvent>()
        .map(|events| events.iter().filter(|(_, event)| event.activator == player).map(|(target, _)| target).collect())
        .unwrap_or_default();
    let seat = activated.into_iter().find(|&target| is_seat(world, target));
    let seated = world.get::<Seated>(player).is_some();
    let Some(mut seats) = world.query_mut::<Seated>() else {
        return;
    };
    match seat {
        Some(furniture) if !seated => {
            seats.insert(player, Seated { furniture, animation_restore: SeatedAnimationRestore::default() });
            log::info!("player sits on furniture entity {furniture}");
        }
        _ if seated => {
            let movement = world.try_resource::<byroredux_scripting::PlayerControlState>().is_none_or(|controls| controls.movement_enabled);
            let moving = world.try_resource::<ActionState>().is_some_and(|actions| STAND_UP_ACTIONS.iter().any(|&action| actions.is_held(action)));
            if movement && moving {
                seats.remove(player);
                log::info!("player stands up");
            }
        }
        _ => {}
    }
}
