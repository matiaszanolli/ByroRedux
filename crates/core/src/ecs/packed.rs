//! Packed (sorted) storage backend.
//!
//! Entities and data are stored in parallel `Vec`s sorted by `EntityId`.
//! This gives cache-friendly, SIMD-ready iteration at the cost of O(log n)
//! insert and remove (binary search to find the slot, then shift).
//!
//! Best for components read every frame by many systems: Transform, Velocity,
//! mesh references, etc.

use super::storage::{Component, ComponentStorage, DynStorage, EntityId};
use std::any::Any;

pub struct PackedStorage<T> {
    /// Sorted by EntityId, parallel to `data`.
    entities: Vec<EntityId>,
    /// Component values, parallel to `entities`.
    data: Vec<T>,
}

impl<T> Default for PackedStorage<T> {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            data: Vec::new(),
        }
    }
}

impl<T: Component<Storage = Self>> ComponentStorage<T> for PackedStorage<T> {
    fn insert(&mut self, entity: EntityId, component: T) {
        match self.entities.binary_search(&entity) {
            Ok(idx) => {
                // Already present — overwrite.
                self.data[idx] = component;
            }
            Err(idx) => {
                // Insert at the sorted position.
                self.entities.insert(idx, entity);
                self.data.insert(idx, component);
            }
        }
    }

    fn remove(&mut self, entity: EntityId) -> Option<T> {
        match self.entities.binary_search(&entity) {
            Ok(idx) => {
                self.entities.remove(idx);
                Some(self.data.remove(idx))
            }
            Err(_) => None,
        }
    }

    fn get(&self, entity: EntityId) -> Option<&T> {
        let idx = self.entities.binary_search(&entity).ok()?;
        Some(&self.data[idx])
    }

    fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        let idx = self.entities.binary_search(&entity).ok()?;
        Some(&mut self.data[idx])
    }

    fn contains(&self, entity: EntityId) -> bool {
        self.entities.binary_search(&entity).is_ok()
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_> {
        Box::new(self.entities.iter().copied().zip(self.data.iter()))
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_> {
        Box::new(self.entities.iter().copied().zip(self.data.iter_mut()))
    }
}

impl<T: Component<Storage = Self>> DynStorage for PackedStorage<T> {
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

    struct Transform {
        x: f32,
        y: f32,
        z: f32,
    }
    impl Component for Transform {
        type Storage = PackedStorage<Self>;
    }

    #[test]
    fn insert_maintains_sort_order() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            10,
            Transform {
                x: 10.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            3,
            Transform {
                x: 3.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            7,
            Transform {
                x: 7.0,
                y: 0.0,
                z: 0.0,
            },
        );

        let entities: Vec<_> = s.iter().map(|(e, _)| e).collect();
        assert_eq!(entities, vec![3, 7, 10]);
    }

    #[test]
    fn overwrite() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            5,
            Transform {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            5,
            Transform {
                x: 99.0,
                y: 0.0,
                z: 0.0,
            },
        );
        assert_eq!(s.get(5).unwrap().x, 99.0);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn remove_middle() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            1,
            Transform {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            5,
            Transform {
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            9,
            Transform {
                x: 9.0,
                y: 0.0,
                z: 0.0,
            },
        );

        let removed = s.remove(5).unwrap();
        assert_eq!(removed.x, 5.0);
        assert_eq!(s.len(), 2);
        assert!(!s.contains(5));

        // Remaining are still sorted.
        let entities: Vec<_> = s.iter().map(|(e, _)| e).collect();
        assert_eq!(entities, vec![1, 9]);
    }

    #[test]
    fn remove_nonexistent() {
        let mut s = PackedStorage::<Transform>::default();
        assert!(s.remove(42).is_none());
    }

    #[test]
    fn iteration_is_sorted() {
        let mut s = PackedStorage::<Transform>::default();
        // Insert out of order.
        for id in [20, 5, 15, 1, 10] {
            s.insert(
                id,
                Transform {
                    x: id as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }

        let pairs: Vec<_> = s.iter().map(|(e, t)| (e, t.x as u32)).collect();
        assert_eq!(pairs, vec![(1, 1), (5, 5), (10, 10), (15, 15), (20, 20)]);
    }

    #[test]
    fn iter_mut_modify() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            0,
            Transform {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
        );
        s.insert(
            1,
            Transform {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
        );

        for (_, t) in s.iter_mut() {
            t.x *= -1.0;
        }

        assert_eq!(s.get(0).unwrap().x, -1.0);
        assert_eq!(s.get(1).unwrap().x, -4.0);
    }
}
