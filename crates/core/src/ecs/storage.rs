//! Storage trait and Component trait.
//!
//! Every component declares its preferred storage backend via an associated
//! type. The two built-in backends are [`SparseSetStorage`](super::sparse_set::SparseSetStorage)
//! (default, O(1) insert/remove) and [`PackedStorage`](super::packed::PackedStorage)
//! (sorted by entity, cache-friendly iteration).

pub type EntityId = u32;

/// Every component declares its own storage backend.
///
/// Default choice: `SparseSetStorage<Self>` for gameplay data.
/// Hot-path components that are read every frame should opt into
/// `PackedStorage<Self>` for cache-friendly iteration.
pub trait Component: 'static + Send + Sync + Sized {
    type Storage: ComponentStorage<Self> + Default + Send + Sync + 'static;
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
