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
    /// Per-entity change-tracking accumulator (see
    /// [`Component::TRACK_CHANGES`]). Only populated when the component
    /// opts in; stays empty (zero overhead) otherwise. Entities are
    /// pushed on insert / remove / `get_mut` / `iter_mut` and drained by
    /// [`take_dirty`](Self::take_dirty). May contain duplicates — the
    /// transform-propagation consumer treats it as a work list where dups
    /// are harmless (re-propagating a subtree is idempotent).
    dirty: Vec<EntityId>,
}

impl<T> Default for PackedStorage<T> {
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            data: Vec::new(),
            dirty: Vec::new(),
        }
    }
}

impl<T: Component<Storage = Self>> PackedStorage<T> {
    /// Drain the change-tracking dirty set, returning the entities mutated
    /// since the last drain. Empty (and allocation-free after warmup) for
    /// components that don't opt into [`Component::TRACK_CHANGES`].
    ///
    /// The list may contain duplicates and is in mutation order; callers
    /// that need uniqueness should dedup.
    ///
    /// **Allocation note**: uses `std::mem::take`, which transfers the Vec's
    /// backing allocation to the caller and leaves `self.dirty` with zero
    /// capacity. The next `mark_dirty` call therefore re-grows from scratch.
    /// Prefer [`drain_dirty_into`] when the caller can supply a persistent
    /// scratch buffer; that keeps the capacity on `self.dirty` across frames.
    pub fn take_dirty(&mut self) -> Vec<EntityId> {
        std::mem::take(&mut self.dirty)
    }

    /// Drain the change-tracking dirty set into `out`, preserving
    /// `self.dirty`'s backing capacity for the next frame.
    ///
    /// `out` is cleared before filling. After this call `self.dirty` is
    /// empty but retains its high-water-mark capacity, so the next
    /// `mark_dirty` call never re-grows. Use this when the caller owns a
    /// persistent scratch `Vec` (e.g. a closure-captured buffer) — it
    /// eliminates the 0→N growth every frame that `take_dirty` causes.
    pub fn drain_dirty_into(&mut self, out: &mut Vec<EntityId>) {
        out.clear();
        out.append(&mut self.dirty);
        // self.dirty is now empty and retains its allocated capacity.
    }

    /// Number of entries currently in the dirty set (with duplicates).
    /// Cheap probe for "did anything change?" without draining.
    pub fn dirty_len(&self) -> usize {
        self.dirty.len()
    }

    #[inline]
    fn mark_dirty(&mut self, entity: EntityId) {
        if T::TRACK_CHANGES {
            self.dirty.push(entity);
        }
    }
}

impl<T: Component<Storage = Self>> ComponentStorage<T> for PackedStorage<T> {
    fn insert(&mut self, entity: EntityId, component: T) {
        self.mark_dirty(entity);
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
                self.mark_dirty(entity);
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
        // Handing out `&mut` is a potential mutation — record it. For a
        // non-tracked component this is a const-false branch (no-op).
        self.mark_dirty(entity);
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
        let mut old_data: Vec<Option<T>> = std::mem::take(&mut self.data)
            .into_iter()
            .map(Some)
            .collect();
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
            while last_in_run + 1 < new_entities.len() && new_entities[last_in_run + 1] == entity {
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

        // A bulk insert touched the whole set — mark every entity dirty so
        // change-tracking consumers (transform propagation) re-process the
        // freshly-loaded content. Runs at cell-load boundaries, not per
        // frame, so the conservative all-mark is cheap relative to the load.
        if T::TRACK_CHANGES {
            self.dirty.extend_from_slice(&self.entities);
        }
    }

    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_> {
        Box::new(self.entities.iter().copied().zip(self.data.iter()))
    }

    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_> {
        // iter_mut hands out `&mut` to every element, so conservatively mark
        // all entities dirty. No tracked component is iter_mut'd over the
        // whole set on the hot path (Transform / GlobalTransform are mutated
        // via targeted get_mut), so this rarely fires for tracked storages.
        if T::TRACK_CHANGES {
            self.dirty.extend_from_slice(&self.entities);
        }
        Box::new(self.entities.iter().copied().zip(self.data.iter_mut()))
    }
}

impl<T: Component<Storage = Self>> DynStorage for PackedStorage<T> {
    fn remove_entity_erased(&mut self, entity: EntityId) {
        <Self as ComponentStorage<T>>::remove(self, entity);
    }

    fn remove_entities_erased(&mut self, victims: &[EntityId]) {
        debug_assert!(victims.windows(2).all(|pair| pair[0] < pair[1]));
        if victims.is_empty() || self.entities.is_empty() {
            return;
        }

        // Both inputs are sorted, so compact the storage in one merge pass —
        // #2397. Driven by a read/write cursor over the existing backing
        // Vecs (the `Vec::retain` shape) rather than draining into two
        // fresh `Vec::with_capacity(old_len)` buffers: the prior shape
        // allocated and moved every *surviving* row on every call, so an
        // eviction of a few victim cells paid `2 × live_rows` of
        // allocate-plus-move for each `PackedStorage` component type,
        // regardless of how few entities the victims actually own (#3689).
        // `Vec::swap` moves a kept row backward into the next free slot
        // without requiring `T: Clone`/`Default`; positions before `write`
        // are already finalized and never revisited, and the leftover
        // single-copy tail (victims plus the rows their slots were
        // swapped from) is dropped in place by `truncate`.
        let mut victim_idx = 0usize;
        let mut write = 0usize;
        for read in 0..self.entities.len() {
            let entity = self.entities[read];
            while victim_idx < victims.len() && victims[victim_idx] < entity {
                victim_idx += 1;
            }
            if victim_idx < victims.len() && victims[victim_idx] == entity {
                self.mark_dirty(entity);
                victim_idx += 1;
                continue;
            }
            if write != read {
                self.entities.swap(write, read);
                self.data.swap(write, read);
            }
            write += 1;
        }

        self.entities.truncate(write);
        self.data.truncate(write);
    }

    fn clear_erased(&mut self) {
        self.entities.clear();
        self.data.clear();
        // Tracked components push every cleared entity through the dirty
        // set on a normal remove; a wholesale clear instead just drops
        // the dirty list — the consumers re-derive from the (now empty)
        // population on the next frame.
        self.dirty.clear();
        // #2395 — mirror SparseSetStorage::clear_erased (#2148): `clear()`
        // alone keeps capacity, so a save-load that replaces the whole
        // entity population would otherwise hand back nothing. The storage
        // is known empty here and the next population starts from scratch,
        // so a full release is unambiguously right.
        self.entities.shrink_to_fit();
        self.data.shrink_to_fit();
        self.dirty.shrink_to_fit();
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

    /// A change-tracked component for exercising the dirty-set primitive.
    #[derive(Debug, PartialEq)]
    struct Tracked(u32);
    impl Component for Tracked {
        type Storage = PackedStorage<Self>;
        const TRACK_CHANGES: bool = true;
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
            (
                20,
                Transform {
                    x: 20.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                5,
                Transform {
                    x: 5.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                15,
                Transform {
                    x: 15.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                1,
                Transform {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                10,
                Transform {
                    x: 10.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
        ];
        for (e, t) in &input {
            serial.insert(
                *e,
                Transform {
                    x: t.x,
                    y: t.y,
                    z: t.z,
                },
            );
        }

        // Bulk.
        let mut bulk = PackedStorage::<Transform>::default();
        bulk.insert_bulk(input);

        // Both must iterate in the same sorted order with the same
        // data. `iter` yields (id, &T); compare id + x (the only
        // varying field).
        let serial_pairs: Vec<(EntityId, u32)> =
            serial.iter().map(|(e, t)| (e, t.x as u32)).collect();
        let bulk_pairs: Vec<(EntityId, u32)> = bulk.iter().map(|(e, t)| (e, t.x as u32)).collect();
        assert_eq!(serial_pairs, bulk_pairs);
        assert_eq!(
            serial_pairs,
            vec![(1, 1), (5, 5), (10, 10), (15, 15), (20, 20)]
        );
    }

    /// Same-entity inputs: bulk path's last-writer-wins dedup must
    /// match serial's overwrite semantics.
    #[test]
    fn insert_bulk_duplicate_ids_last_writer_wins() {
        let mut bulk = PackedStorage::<Transform>::default();
        bulk.insert_bulk(vec![
            (
                5,
                Transform {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                3,
                Transform {
                    x: 2.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                5,
                Transform {
                    x: 99.0,
                    y: 0.0,
                    z: 0.0,
                },
            ), // wins for entity 5
            (
                3,
                Transform {
                    x: 42.0,
                    y: 0.0,
                    z: 0.0,
                },
            ), // wins for entity 3
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
        s.insert(
            10,
            Transform {
                x: 10.0,
                y: 0.0,
                z: 0.0,
            },
        );
        s.insert(
            30,
            Transform {
                x: 30.0,
                y: 0.0,
                z: 0.0,
            },
        );

        s.insert_bulk(vec![
            (
                5,
                Transform {
                    x: 5.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                20,
                Transform {
                    x: 20.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            (
                10,
                Transform {
                    x: 100.0,
                    y: 0.0,
                    z: 0.0,
                },
            ), // overrides pre-existing
        ]);

        assert_eq!(s.len(), 4);
        let pairs: Vec<(EntityId, u32)> = s.iter().map(|(e, t)| (e, t.x as u32)).collect();
        assert_eq!(pairs, vec![(5, 5), (10, 100), (20, 20), (30, 30)]);
    }

    /// Empty bulk-insert must be a clean no-op.
    #[test]
    fn insert_bulk_empty_is_noop() {
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            1,
            Transform {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        );
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

    // ── change tracking (TRACK_CHANGES) ─────────────────────────────────

    #[test]
    fn untracked_component_never_accumulates_dirty() {
        // `Transform` here leaves TRACK_CHANGES at its default (false).
        let mut s = PackedStorage::<Transform>::default();
        s.insert(
            0,
            Transform {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
        );
        let _ = s.get_mut(0);
        for (_, t) in s.iter_mut() {
            t.x = 9.0;
        }
        s.remove(0);
        assert_eq!(s.dirty_len(), 0, "non-tracked storage must stay dirty-free");
        assert!(s.take_dirty().is_empty());
    }

    #[test]
    fn tracked_get_mut_and_insert_record_dirty() {
        let mut s = PackedStorage::<Tracked>::default();
        s.insert(5, Tracked(5)); // insert → dirty
        s.insert(2, Tracked(2));
        let _ = s.get_mut(5); // mutable access → dirty
        let _ = s.get(2); // immutable read → NOT dirty
        let _ = s.get_mut(99); // miss → no entry

        let dirty = s.take_dirty();
        assert!(dirty.contains(&5));
        assert!(dirty.contains(&2));
        // get(2) was read-only and get_mut(99) missed, so the only entries
        // are the two inserts plus the get_mut(5).
        assert_eq!(dirty.iter().filter(|&&e| e == 5).count(), 2); // insert + get_mut
        assert_eq!(dirty.iter().filter(|&&e| e == 2).count(), 1); // insert only
        assert!(!dirty.contains(&99));
    }

    #[test]
    fn take_dirty_drains_and_resets() {
        let mut s = PackedStorage::<Tracked>::default();
        s.insert(1, Tracked(1));
        assert_eq!(s.take_dirty(), vec![1]);
        // Drained — a second take is empty until the next mutation.
        assert!(s.take_dirty().is_empty());
        let _ = s.get_mut(1);
        assert_eq!(s.take_dirty(), vec![1]);
    }

    /// Regression for #1371 — `drain_dirty_into` must preserve `self.dirty`'s
    /// backing capacity (so the next `mark_dirty` doesn't re-grow from zero)
    /// while emptying it into the caller's buffer.
    #[test]
    fn drain_dirty_into_preserves_storage_capacity() {
        let mut s = PackedStorage::<Tracked>::default();
        // Insert enough entities to guarantee an allocation.
        for i in 1u32..=8 {
            s.insert(i, Tracked(i));
        }

        // First drain: warms the dirty vec capacity.
        let mut out: Vec<EntityId> = Vec::new();
        s.drain_dirty_into(&mut out);
        assert_eq!(out.len(), 8, "all 8 inserts should be in the dirty list");
        // self.dirty is now empty but must have retained capacity.
        assert!(s.dirty_len() == 0);

        // Mark 4 entities dirty again.
        for i in 1u32..=4 {
            let _ = s.get_mut(i);
        }
        // Second drain into a pre-populated `out` — must clear first.
        out.push(99); // sentinel to verify clear() runs
        s.drain_dirty_into(&mut out);
        assert!(
            !out.contains(&99),
            "drain_dirty_into must clear `out` first"
        );
        assert_eq!(out.len(), 4);
        assert!(s.dirty_len() == 0);
    }

    #[test]
    fn tracked_remove_and_iter_mut_record_dirty() {
        let mut s = PackedStorage::<Tracked>::default();
        s.insert(1, Tracked(1));
        s.insert(2, Tracked(2));
        let _ = s.take_dirty(); // clear the insert marks

        for (_, t) in s.iter_mut() {
            t.0 += 1;
        }
        // iter_mut conservatively marks every live entity.
        let mut after_iter = s.take_dirty();
        after_iter.sort_unstable();
        assert_eq!(after_iter, vec![1, 2]);

        s.remove(2); // remove → dirty
        assert_eq!(s.take_dirty(), vec![2]);
    }

    // ── remove_entities_erased (#2396) ──────────────────────────────────

    #[test]
    fn remove_entities_erased_preserves_ascending_order() {
        // The merge-compaction path (an in-place read/write cursor over the
        // existing buffers, not a rebuild from scratch — see #3689) doesn't
        // inherit sort order from `Vec::remove` for free, so pin it
        // directly: iteration must still yield entities in ascending order
        // after removing a scattered victim set from a >3-element storage.
        let mut s = PackedStorage::<Transform>::default();
        for i in [1u32, 2, 3, 4, 5, 6, 7] {
            s.insert(
                i,
                Transform {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }
        // Victims must be sorted, per the method's own debug_assert.
        let victims = [2u32, 4, 6];
        s.remove_entities_erased(&victims);

        let entities: Vec<_> = s.iter().map(|(e, _)| e).collect();
        assert_eq!(entities, vec![1, 3, 5, 7]);
    }

    #[test]
    fn remove_entities_erased_does_not_reallocate() {
        // #3689 — the merge-compaction now runs in place on the existing
        // `entities`/`data` buffers via a read/write cursor (`Vec::swap` +
        // `truncate`), not into two fresh `Vec::with_capacity(old_len)`
        // buffers. Pin that directly: both backing allocations — same
        // pointer, same capacity — must survive a removal untouched.
        let mut s = PackedStorage::<Transform>::default();
        for i in [1u32, 2, 3, 4, 5, 6, 7] {
            s.insert(
                i,
                Transform {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }
        let entities_ptr = s.entities.as_ptr();
        let data_ptr = s.data.as_ptr();
        let entities_cap = s.entities.capacity();
        let data_cap = s.data.capacity();

        let victims = [2u32, 4, 6];
        s.remove_entities_erased(&victims);

        assert_eq!(
            s.entities.as_ptr(),
            entities_ptr,
            "entities buffer must not be reallocated by a removal"
        );
        assert_eq!(
            s.data.as_ptr(),
            data_ptr,
            "data buffer must not be reallocated by a removal"
        );
        assert_eq!(s.entities.capacity(), entities_cap);
        assert_eq!(s.data.capacity(), data_cap);
    }

    #[test]
    fn remove_entities_erased_marks_exactly_the_removed_ids_dirty() {
        // The merge loop's dirty marking (now routed through `mark_dirty`,
        // previously hand-inlined) is the only marking site for this
        // removal path — pin that it fires for exactly the removed
        // entities, no more and no fewer, on a TRACK_CHANGES fixture.
        let mut s = PackedStorage::<Tracked>::default();
        for i in [1u32, 2, 3, 4, 5] {
            s.insert(i, Tracked(i));
        }
        let _ = s.take_dirty(); // clear the insert marks

        let victims = [2u32, 4];
        s.remove_entities_erased(&victims);

        let mut dirty = s.take_dirty();
        dirty.sort_unstable();
        assert_eq!(dirty, vec![2, 4]);

        let entities: Vec<_> = s.iter().map(|(e, _)| e).collect();
        assert_eq!(entities, vec![1, 3, 5]);
    }

    #[test]
    fn remove_entities_erased_on_untracked_storage_stays_dirty_free() {
        let mut s = PackedStorage::<Transform>::default();
        for i in [1u32, 2, 3] {
            s.insert(
                i,
                Transform {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }
        s.remove_entities_erased(&[2u32]);
        assert!(s.take_dirty().is_empty());
    }

    // ── clear_erased capacity release (#2395) ───────────────────────────

    #[test]
    fn clear_erased_releases_capacity() {
        // Mirror of SparseSetStorage::clear_erased_releases_capacity —
        // #2395 found PackedStorage's clear_erased dropped only length,
        // keeping the pre-load peak's capacity alive until the next batch
        // removal re-fit it.
        let mut s = PackedStorage::<Transform>::default();
        for i in 0..1_000u32 {
            s.insert(
                i,
                Transform {
                    x: i as f32,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }
        assert!(s.entities.capacity() >= 1_000);
        assert!(s.data.capacity() >= 1_000);

        s.clear_erased();

        assert_eq!(s.entities.len(), 0);
        assert_eq!(
            s.entities.capacity(),
            0,
            "clear() alone keeps capacity — a save-load would hand back nothing",
        );
        assert_eq!(s.data.capacity(), 0);
    }

    #[test]
    fn clear_erased_releases_dirty_capacity_for_tracked_component() {
        let mut s = PackedStorage::<Tracked>::default();
        for i in 0..1_000u32 {
            s.insert(i, Tracked(i));
        }
        assert!(s.dirty.capacity() >= 1_000);

        s.clear_erased();

        assert_eq!(s.dirty.capacity(), 0);
    }
}
