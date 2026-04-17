//! Storage trait and Component trait.
//!
//! Every component declares its preferred storage backend via an associated
//! type. The two built-in backends are [`SparseSetStorage`](super::sparse_set::SparseSetStorage)
//! (default, O(1) insert/remove) and [`PackedStorage`](super::packed::PackedStorage)
//! (sorted by entity, cache-friendly iteration).

use std::any::Any;

pub type EntityId = u32;

/// Every component declares its own storage backend.
///
/// Default choice: `SparseSetStorage<Self>` for gameplay data.
/// Hot-path components that are read every frame should opt into
/// `PackedStorage<Self>` for cache-friendly iteration.
pub trait Component: 'static + Send + Sync + Sized {
    type Storage: ComponentStorage<Self> + DynStorage + Default + Send + Sync + 'static;
}

/// The storage contract. Both sparse-set and packed backends implement this.
pub trait ComponentStorage<T: Component> {
    fn insert(&mut self, entity: EntityId, component: T);
    fn remove(&mut self, entity: EntityId) -> Option<T>;
    fn get(&self, entity: EntityId) -> Option<&T>;
    fn get_mut(&mut self, entity: EntityId) -> Option<&mut T>;
    fn contains(&self, entity: EntityId) -> bool;
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate over all (entity, component) pairs.
    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_>;

    /// Iterate over all (entity, &mut component) pairs.
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_>;
}

/// Object-safe view over a storage of unknown element type.
///
/// `ComponentStorage<T>` is parameterized by `T` and returns `Option<T>`
/// from `remove`, which makes it non-object-safe. `DynStorage` exposes
/// just the operations `World` needs when walking its storages without
/// knowing each component type — specifically, removing all of one
/// entity's components in [`World::despawn`](super::world::World::despawn).
pub trait DynStorage: Send + Sync + 'static {
    /// Remove this entity's component from the storage, dropping the value.
    /// No-op if the entity has no component here.
    fn remove_entity_erased(&mut self, entity: EntityId);

    /// Upcast to `&dyn Any` so `World` can downcast back to the concrete
    /// `T::Storage` for typed access.
    fn as_any(&self) -> &dyn Any;

    /// Mutable counterpart to [`as_any`](Self::as_any).
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
