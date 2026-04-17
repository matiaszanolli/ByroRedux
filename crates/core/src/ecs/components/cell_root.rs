//! Cell ownership marker.
//!
//! Every entity spawned during cell load gets a `CellRoot` component
//! pointing at the "root" entity for that cell. On cell unload, the
//! loader walks `CellRoot` storage, collects every entity whose root
//! matches, and passes the batch to
//! [`World::despawn`](crate::ecs::world::World::despawn).
//!
//! Without this, `World` has no way to enumerate a cell's entities —
//! meshes, BLASes, and textures uploaded during load stay for the
//! process lifetime. See #372.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

/// Marks an entity as owned by a cell. The wrapped `EntityId` points at
/// the cell's root entity — typically a placeholder entity created by
/// the cell loader that itself carries no components other than
/// `CellRoot(self)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellRoot(pub EntityId);

impl Component for CellRoot {
    type Storage = SparseSetStorage<Self>;
}
