//! Sparse-set storage backend.
//!
//! O(1) insert, O(1) remove (swap-remove trick), O(1) lookup.
//! Iteration is dense but not sorted by EntityId.
//! Best for gameplay logic, AI states, status effects, inventory —
//! anything that mutates frequently.

use super::storage::{Component, ComponentStorage, DynStorage, EntityId};
use std::any::Any;

pub struct SparseSetStorage<T> {
    /// entity → dense index. `None` means the entity has no component.
    sparse: Vec<Option<u32>>,
    /// dense index → entity (parallel to `data`).
    dense: Vec<EntityId>,
    /// dense index → component value (parallel to `dense`).
    data: Vec<T>,
}

impl<T> Default for SparseSetStorage<T> {
    fn default() -> Self {
        Self {
            sparse: Vec::new(),
            dense: Vec::new(),
            data: Vec::new(),
        }
    }
}

impl<T: Component<Storage = Self>> ComponentStorage<T> for SparseSetStorage<T> {
    fn insert(&mut self, entity: EntityId, component: T) {
        let idx = entity as usize;

        // Grow sparse array if needed.
        if idx >= self.sparse.len() {
            self.sparse.resize(idx + 1, None);
        }

        if let Some(dense_idx) = self.sparse[idx] {
            // Entity already has this component — overwrite in place.
            self.data[dense_idx as usize] = component;
        } else {
            // New entry: push to the end of the dense arrays.
            let dense_idx = self.dense.len() as u32;
            self.sparse[idx] = Some(dense_idx);
            self.dense.push(entity);
            self.data.push(component);
        }
    }

    fn remove(&mut self, entity: EntityId) -> Option<T> {
        let idx = entity as usize;
        let dense_idx = *self.sparse.get(idx)?.as_ref()? as usize;

        // Clear the sparse slot for the removed entity.
        self.sparse[idx] = None;

        let last = self.dense.len() - 1;

        if dense_idx == last {
            // Removing the last element — no swap needed.
            self.dense.pop();
            return self.data.pop();
        }

        // Swap-remove: move the last element into the gap.
        let moved_entity = self.dense[last] as usize;
        self.dense.swap(dense_idx, last);
        self.data.swap(dense_idx, last);

        self.dense.pop();
        let removed = self.data.pop();

        // Fix up the sparse pointer for the entity that was moved.
        self.sparse[moved_entity] = Some(dense_idx as u32);

        removed
    }

    fn get(&self, entity: EntityId) -> Option<&T> {
        let dense_idx = *self.sparse.get(entity as usize)?.as_ref()? as usize;
        self.data.get(dense_idx)
    }

    fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        let dense_idx = *self.sparse.get(entity as usize)?.as_ref()? as usize;
        self.data.get_mut(dense_idx)
    }

    fn contains(&self, entity: EntityId) -> bool {
        self.sparse
            .get(entity as usize)
            .is_some_and(|slot| slot.is_some())
    }

    fn len(&self) -> usize {
        self.dense.len()
    }

    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_> {
        Box::new(self.dense.iter().copied().zip(self.data.iter()))
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_> {
        Box::new(self.dense.iter().copied().zip(self.data.iter_mut()))
    }
}

impl<T: Component<Storage = Self>> DynStorage for SparseSetStorage<T> {
    fn remove_entity_erased(&mut self, entity: EntityId) {
        <Self as ComponentStorage<T>>::remove(self, entity);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Health(f32);
    impl Component for Health {
        type Storage = SparseSetStorage<Self>;
    }

    #[test]
    fn insert_and_get() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(5, Health(100.0));
        s.insert(10, Health(50.0));

        assert_eq!(s.get(5).unwrap().0, 100.0);
        assert_eq!(s.get(10).unwrap().0, 50.0);
        assert!(s.get(0).is_none());
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn overwrite() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(3, Health(100.0));
        s.insert(3, Health(75.0));
        assert_eq!(s.get(3).unwrap().0, 75.0);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn swap_remove() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(0, Health(10.0));
        s.insert(1, Health(20.0));
        s.insert(2, Health(30.0));

        // Remove the first — entity 2 should swap into slot 0.
        let removed = s.remove(0).unwrap();
        assert_eq!(removed.0, 10.0);
        assert!(!s.contains(0));
        assert_eq!(s.len(), 2);

        // Entity 1 and 2 should still be intact.
        assert_eq!(s.get(1).unwrap().0, 20.0);
        assert_eq!(s.get(2).unwrap().0, 30.0);
    }

    #[test]
    fn remove_last() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(0, Health(10.0));
        s.remove(0);
        assert!(s.is_empty());
    }

    #[test]
    fn remove_nonexistent() {
        let mut s = SparseSetStorage::<Health>::default();
        assert!(s.remove(999).is_none());
    }

    #[test]
    fn iter_all() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(3, Health(30.0));
        s.insert(1, Health(10.0));
        s.insert(7, Health(70.0));

        let mut pairs: Vec<_> = s.iter().map(|(e, h)| (e, h.0 as u32)).collect();
        pairs.sort_by_key(|&(e, _)| e);
        assert_eq!(pairs, vec![(1, 10), (3, 30), (7, 70)]);
    }

    #[test]
    fn iter_mut_modify() {
        let mut s = SparseSetStorage::<Health>::default();
        s.insert(0, Health(100.0));
        s.insert(1, Health(200.0));

        for (_entity, health) in s.iter_mut() {
            health.0 *= 2.0;
        }

        assert_eq!(s.get(0).unwrap().0, 200.0);
        assert_eq!(s.get(1).unwrap().0, 400.0);
    }
}
