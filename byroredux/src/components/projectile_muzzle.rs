//! Where a launcher's projectiles leave it: the NIF's `ProjectileNode`.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::math::Vec3;

/// The root-local position (renderer Y-up) of the placed model's
/// `ProjectileNode`.
///
/// Skyrim's dart ports author it 16 units ahead of their origin, which is
/// outside the wall the port is mounted in; `Weapon.Fire` from a reference
/// that carries one starts its projectile there. Stamped from the NIF at
/// spawn, never saved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProjectileMuzzle(pub(crate) Vec3);

impl Component for ProjectileMuzzle {
    type Storage = SparseSetStorage<Self>;
}
