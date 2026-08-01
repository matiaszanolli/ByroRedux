//! Sparse actor lifecycle markers.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor as dead.
///
/// Absence means alive. Keeping death as a sparse marker makes it cheap for
/// the common live-actor case and gives combat, scripts, resurrection, and
/// condition evaluation one shared source of truth.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Dead;

impl Component for Dead {
    type Storage = SparseSetStorage<Self>;
}
