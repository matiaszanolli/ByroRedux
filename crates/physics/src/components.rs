//! ECS components carrying Rapier handles.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};

/// Handles into the `PhysicsWorld` Rapier sets for one simulated entity.
///
/// Inserted by `physics_sync_system` Phase 1 when it registers a new
/// body. Absence of this component is the signal that an entity with
/// `CollisionShape` + `RigidBodyData` still needs to be registered.
#[derive(Debug, Clone, Copy)]
pub struct RapierHandles {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
}

impl Component for RapierHandles {
    type Storage = SparseSetStorage<Self>;
}

/// Marker for the player-controlled body.
///
/// The physics sync system treats entities with this marker specially:
/// - rotations are locked (player stays upright)
/// - the fly camera system writes `linvel` instead of mutating `Transform`
/// - the body is built as a dynamic capsule even if no `CollisionShape`
///   was attached by a NIF
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerBody {
    /// Capsule half-height (Bethesda units, Y-up).
    pub half_height: f32,
    /// Capsule radius (Bethesda units).
    pub radius: f32,
}

impl PlayerBody {
    /// Default human-shaped capsule: ~144 BU tall, ~56 BU wide.
    pub const HUMAN: Self = Self {
        half_height: 72.0,
        radius: 28.0,
    };
}

impl Component for PlayerBody {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_body_human_dimensions() {
        let p = PlayerBody::HUMAN;
        assert!(p.half_height > 0.0);
        assert!(p.radius > 0.0);
    }
}
