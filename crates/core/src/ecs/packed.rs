//! Packed (sorted) storage backend.
//!
//! Entities and data are stored in parallel `Vec`s sorted by `EntityId`.
//! This gives cache-friendly, SIMD-ready iteration at the cost of
//! **O(n) insert and remove** — `binary_search` finds the slot in
//! O(log n), then `Vec::insert`/`Vec::remove` shift every element
//! after it. Fine for steady-state frame-to-frame mutation (Transform
//! is rarely inserted outside of cell load); expensive for bulk cell
//! load where thousands of entities get inserted in one burst.
//!
//! For bulk-load paths, call
//! [`ComponentStorage::insert_bulk`](super::storage::ComponentStorage::insert_bulk)
//! — the `PackedStorage` override appends every pair to the tail and
//! sorts once at the end (O(n log n) total vs O(n²) serial), so the
//! worst cell-load component picks up an ~N× speedup. See #467.
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

    /// Bulk insert — append everything to the tail, then re-sort both
    /// parallel Vecs by entity id in one pass. O(N + M log M + (N+M))
    /// where N is the pre-existing count and M is the new count.
    /// Serial `insert` is O((N+M) × M) because every shift runs to
    /// the tail; for M ≈ N the batched path is ~N× faster.
    ///
    /// Later entries for the same `EntityId` win — matches the
    /// overwrite semantics of single `insert`. The `sort_by_key` is
    /// stable so duplicates from the same input position stay in
    /// author order; the de-dup pass then keeps the *last* occurrence
    /// to match serial-insert behaviour (last-writer-wins). See #467.
    fn insert_bulk<I: IntoIterator<Item = (EntityId, T)>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        let (lo, _) = iter.size_hint();
        self.entities.reserve(lo);
        self.data.reserve(lo);

        let pre_existing = self.entities.len();
        for (entity, component) in iter {
            self.entities.push(entity);
            self.data.push(component);
        }

        // Fast bail if nothing was added.
        if self.entities.len() == pre_existing {
            return;
        }

        // Stable sort on indirection so we can reorder both Vecs
        // together. The data Vec's stable-sort-via-index approach
        // avoids moving T (which may be non-Copy) more than once.
        let len = self.entities.len();
        let mut indices: Vec<usize> = (0..len).collect();
        indices.sort_by_key(|&i| self.entities[i]);

        // Reorder `entities` and `data` into temporary Vecs. We can't
        // do this in-place without either swap-shuffling (hard for
        // non-Copy T with cycles) or an auxiliary bitset — the
        // allocate-reorder approach is simpler and the bulk path
        // only runs at cell-load boundaries, not per frame.
        let mut new_entities: Vec<EntityId> = Vec::with_capacity(len);
        let mut new_data: Vec<T> = Vec::with_capacity(len);
        // Drain into an `Option`-wrapped Vec so we can `take()` each
        // element out by its post-sort position without double-moving.
        let old_entities = std::mem::take(&mut self.entities);
        let mut old_data: Vec<Option<T>> =
            std::mem::take(&mut self.data).into_iter().map(Some).collect();
        for &idx in &indices {
            new_entities.push(old_entities[idx]);
            new_data.push(
                old_data[idx]
                    .take()
                    .expect("index visited twice in bulk insert sort"),
            );
        }

        // Dedup: on consecutive duplicate entity ids, keep the LAST
        // occurrence (matches single-insert overwrite semantics). A
        // forward scan shifting in-place gives O(N) dedup without a
        // second allocation.
        let mut write = 0usize;
        let mut read = 0usize;
        while read < new_entities.len() {
            // Find the end of the current run of this entity id.
            let entity = new_entities[read];
            let mut last_in_run = read;
            while last_in_run + 1 < new_entities.len()
                && new_entities[last_in_run + 1] == entity
            {
                last_in_run += 1;
            }
            // Move entry at `last_in_run` into slot `write`.
            if write != last_in_run {
                new_entities.swap(write, last_in_run);
                new_data.swap(write, last_in_run);
            }
            write += 1;
            read = last_in_run + 1;
        }
        new_entities.truncate(write);
        new_data.truncate(write);

        self.entities = new_entities;
        self.data = new_data;
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

    /// Regression for #467: `insert_bulk` must produce a state
    /// indistinguishable from looping `insert` on the same input
    /// order. The fast path sorts differently internally, so same-
    /// entity last-writer-wins is the invariant to pin.
    #[test]
    fn insert_bulk_matches_serial_insert_for_unique_ids() {
        // Serial reference.
        let mut serial = PackedStorage::<Transform>::default();
        let input: Vec<(EntityId, Transform)> = vec![
            (20, Transform { x: 20.0, y: 0.0, z: 0.0 }),
            (5,  Transform { x: 5.0,  y: 0.0, z: 0.0 }),
            (15, Transform { x: 15.0, y: 0.0, z: 0.0 }),
            (1,  Transform { x: 1.0,  y: 0.0, z: 0.0 }),
            (10, Transform { x: 10.0, y: 0.0, z: 0.0 }),
        ];
        for (e, t) in &input {
            serial.insert(
                *e,
                Transform { x: t.x, y: t.y, z: t.z },
            );
        }

        // Bulk.
        let mut bulk = PackedStorage::<Transform>::default();
        bulk.insert_bulk(input.into_iter());

        // Both must iterate in the same sorted order with the same
        // data. `iter` yields (id, &T); compare id + x (the only
        // varying field).
        let serial_pairs: Vec<(EntityId, u32)> =
            serial.iter().map(|(e, t)| (e, t.x as u32)).collect();
        let bulk_pairs: Vec<(EntityId, u32)> =
            bulk.iter().map(|(e, t)| (e, t.x as u32)).collect();
        assert_eq!(serial_pairs, bulk_pairs);
        assert_eq!(serial_pairs, vec![(1, 1), (5, 5), (10, 10), (15, 15), (20, 20)]);
    }

    /// Same-entity inputs: bulk path's last-writer-wins dedup must
    /// match serial's overwrite semantics.
    #[test]
    fn insert_bulk_duplicate_ids_last_writer_wins() {
        let mut bulk = PackedStorage::<Transform>::default();
        bulk.insert_bulk(vec![
            (5, Transform { x: 1.0, y: 0.0, z: 0.0 }),
            (3, Transform { x: 2.0, y: 0.0, z: 0.0 }),
            (5, Transform { x: 99.0, y: 0.0, z: 0.0 }), // wins for entity 5
            (3, Transform { x: 42.0, y: 0.0, z: 0.0 }), // wins for entity 3
        ]);

        assert_eq!(bulk.len(), 2);
        assert_eq!(bulk.get(3).unwrap().x, 42.0);
        assert_eq!(bulk.get(5).unwrap().x, 99.0);
    }

    /// Bulk on top of an existing non-empty storage must merge
    /// without dropping the pre-existing entries.
    #[test]
    fn insert_bulk_extends_non_empty_storage() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(10, Transform { x: 10.0, y: 0.0, z: 0.0 });
        s.insert(30, Transform { x: 30.0, y: 0.0, z: 0.0 });

        s.insert_bulk(vec![
            (5,  Transform { x: 5.0,  y: 0.0, z: 0.0 }),
            (20, Transform { x: 20.0, y: 0.0, z: 0.0 }),
            (10, Transform { x: 100.0, y: 0.0, z: 0.0 }), // overrides pre-existing
        ]);

        assert_eq!(s.len(), 4);
        let pairs: Vec<(EntityId, u32)> =
            s.iter().map(|(e, t)| (e, t.x as u32)).collect();
        assert_eq!(pairs, vec![(5, 5), (10, 100), (20, 20), (30, 30)]);
    }

    /// Empty bulk-insert must be a clean no-op.
    #[test]
    fn insert_bulk_empty_is_noop() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(1, Transform { x: 1.0, y: 0.0, z: 0.0 });
        s.insert_bulk(Vec::<(EntityId, Transform)>::new());
        assert_eq!(s.len(), 1);
        assert_eq!(s.get(1).unwrap().x, 1.0);
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
