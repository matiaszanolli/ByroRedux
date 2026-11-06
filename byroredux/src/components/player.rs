//! The player-character pointer resource (#6714: moved out of `systems::character` so
//! the render collector reads it from the lowest layer).

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};

/// Resource pointing at the player character entity, so other systems
/// (camera follow, audio listener attach, future quest-marker
/// distance computations) can find the player without walking the
/// `CharacterController` storage.
///
/// `None` means the engine isn't in player mode. Set when `scene` creates the
/// player rig. The body is intentionally not cell-owned, so it and this
/// process-local pointer survive live cell reloads.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PlayerEntity(pub Option<EntityId>);

impl Resource for PlayerEntity {}

impl PlayerEntity {
    /// The player character entity, or `None` when the resource is absent or
    /// the engine is not in player mode (#6362).
    ///
    /// The one read of this resource. It is NOT synchronised with the
    /// scripting crate's `PapyrusPlayerEntity` (`PapyrusPlayerEntity::get`), which
    /// differs in FlyCam, so a flow that crosses the crate boundary reads the
    /// pair knowingly rather than by hand-spelling either.
    pub(crate) fn get(world: &World) -> Option<EntityId> {
        world.try_resource::<PlayerEntity>().and_then(|player| player.0)
    }
}
