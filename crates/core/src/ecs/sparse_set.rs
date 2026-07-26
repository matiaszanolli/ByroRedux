//! Sparse-set storage backend.
//!
//! O(1) insert, O(1) remove (swap-remove trick), O(1) lookup.
//! Iteration is dense but not sorted by EntityId.
//! Best for gameplay logic, AI states, status effects, inventory —
//! anything that mutates frequently.

use super::storage::{Component, ComponentStorage, DynStorage, EntityId};
use std::any::Any;

/// Sentinel written into [`SparseSetStorage::sparse`] for "this entity has no
/// component here".
///
/// #2148 — the slot type is `u32`, not `Option<u32>`. `u32` has no niche, so
/// `Option<u32>` costs 8 bytes per slot where a sentinel costs 4. That matters
/// because `sparse` is indexed by raw `EntityId` and entity IDs are
/// deliberately never reclaimed (#372), so every sparse storage that has ever
/// seen an insert near the high-water mark keeps a slot for every ID below it
/// — live or long dead. Halving the slot halves that floor across all 122
/// `SparseSetStorage` component declarations.
///
/// `u32::MAX` is safe as the empty marker: it encodes a *dense* index, and the
/// dense arrays are bounded by the live component count, which cannot reach
/// `u32::MAX` without exhausting `EntityId` first.
const EMPTY: u32 = u32::MAX;

pub struct SparseSetStorage<T> {
    /// entity → dense index. [`EMPTY`] means the entity has no component.
    sparse: Vec<u32>,
    /// dense index → entity (parallel to `data`).
    dense: Vec<EntityId>,
    /// dense index → component value (parallel to `dense`).
    data: Vec<T>,
    /// Monotonic counter bumped on every structural mutation (insert —
    /// including overwrite/reparent — and remove) when the component opts
    /// into [`Component::TRACK_CHANGES`]. Lets a consumer cheaply answer
    /// "did this storage change shape since I last looked?" without a per-
    /// entity dirty set. Used by transform propagation to detect hierarchy
    /// edits (`Parent` / `Children`) that don't move any `Transform` —
    /// e.g. a reparent that overwrites an existing `Parent` (entity count
    /// unchanged). Stays 0 for non-tracked components (zero overhead).
    structural_gen: u64,
}

impl<T> Default for SparseSetStorage<T> {
    fn default() -> Self {
        Self {
            sparse: Vec::new(),
            dense: Vec::new(),
            data: Vec::new(),
            structural_gen: 0,
        }
    }
}

impl<T: Component<Storage = Self>> SparseSetStorage<T> {
    /// Current structural-mutation generation (see [`Self::structural_gen`]).
    /// Compare against a previously-stored value to detect insert/remove
    /// activity since then. Always 0 for components that don't opt into
    /// [`Component::TRACK_CHANGES`].
    pub fn structural_generation(&self) -> u64 {
        self.structural_gen
    }
}

impl<T: Component<Storage = Self>> ComponentStorage<T> for SparseSetStorage<T> {
    fn insert(&mut self, entity: EntityId, component: T) {
        // Structural change (new entry OR overwrite/reparent) — bump the
        // generation for change-tracking consumers. const-false → no-op for
        // non-tracked components.
        if T::TRACK_CHANGES {
            self.structural_gen = self.structural_gen.wrapping_add(1);
        }
        let idx = entity as usize;

        // Grow sparse array if needed.
        if idx >= self.sparse.len() {
            self.sparse.resize(idx + 1, EMPTY);
        }

        if self.sparse[idx] != EMPTY {
            // Entity already has this component — overwrite in place.
            self.data[self.sparse[idx] as usize] = component;
        } else {
            // New entry: push to the end of the dense arrays.
            let dense_idx = self.dense.len() as u32;
            self.sparse[idx] = dense_idx;
            self.dense.push(entity);
            self.data.push(component);
        }
    }

    fn remove(&mut self, entity: EntityId) -> Option<T> {
        let idx = entity as usize;
        let slot = *self.sparse.get(idx)?;
        if slot == EMPTY {
            return None;
        }
        let dense_idx = slot as usize;

        // Structural change — bump generation (only once we know the entity
        // is actually present, matching the `?` early-returns above).
        if T::TRACK_CHANGES {
            self.structural_gen = self.structural_gen.wrapping_add(1);
        }

        // Clear the sparse slot for the removed entity.
        self.sparse[idx] = EMPTY;

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
        self.sparse[moved_entity] = dense_idx as u32;

        removed
    }

    fn get(&self, entity: EntityId) -> Option<&T> {
        let slot = *self.sparse.get(entity as usize)?;
        if slot == EMPTY {
            return None;
        }
        self.data.get(slot as usize)
    }

    fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        let slot = *self.sparse.get(entity as usize)?;
        if slot == EMPTY {
            return None;
        }
        self.data.get_mut(slot as usize)
    }

    fn contains(&self, entity: EntityId) -> bool {
        self.sparse
            .get(entity as usize)
            .is_some_and(|&slot| slot != EMPTY)
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

    fn clear_erased(&mut self) {
        self.sparse.clear();
        self.dense.clear();
        self.data.clear();
        // #2148 — `clear()` keeps capacity, so a save-load that replaces the
        // whole entity population would otherwise hand back nothing. This is
        // the one place a full release is unambiguously right: the storage is
        // known empty and the next population starts from scratch.
        self.sparse.shrink_to_fit();
        self.dense.shrink_to_fit();
        self.data.shrink_to_fit();
        self.structural_gen = self.structural_gen.wrapping_add(1);
    }

    fn shrink_sparse_tail(&mut self) {
        // Trailing EMPTY slots describe entities that are either dead or never
        // had this component; nothing indexes past the last live slot, so the
        // tail is pure overhead. A backwards scan is O(tail) and this only
        // runs at load boundaries.
        let live_end = self
            .sparse
            .iter()
            .rposition(|&slot| slot != EMPTY)
            .map_or(0, |i| i + 1);
        self.sparse.truncate(live_end);
        self.sparse.shrink_to_fit();
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

/// #2148 / ECS-2507-02 — the sparse index is sized by the monotonic
/// `EntityId` high-water mark, so it needs both a cheaper slot and a way to
/// give the tail back.
#[cfg(test)]
mod sparse_footprint_tests {
    use super::*;
    use crate::ecs::storage::{Component, ComponentStorage, DynStorage};

    struct Tag(u32);
    impl Component for Tag {
        type Storage = SparseSetStorage<Self>;
    }

    /// The whole point of the sentinel. `u32` has no niche, so `Option<u32>`
    /// pays 8 bytes for 4 bytes of payload — doubling a floor that is already
    /// proportional to every entity ever spawned.
    #[test]
    fn sparse_slot_is_four_bytes_not_eight() {
        assert_eq!(std::mem::size_of::<u32>(), 4);
        assert_eq!(
            std::mem::size_of::<Option<u32>>(),
            8,
            "if this ever becomes 4, Option<u32> gained a niche and the \
             sentinel could go back to being an Option",
        );
    }

    /// The sentinel must not be reachable as a real dense index. It can't be:
    /// dense length is bounded by the live component count, which is bounded
    /// by the entity count, which exhausts `EntityId` (also u32) first.
    #[test]
    fn empty_sentinel_is_outside_the_dense_index_range() {
        assert_eq!(EMPTY, u32::MAX);
        assert_eq!(
            std::mem::size_of::<EntityId>(),
            std::mem::size_of::<u32>(),
            "EntityId widening would let dense outgrow the sentinel",
        );
    }

    /// Insert high, remove, shrink: the tail must actually come back.
    #[test]
    fn shrink_sparse_tail_truncates_trailing_empty_slots() {
        let mut s = SparseSetStorage::<Tag>::default();
        s.insert(10_000, Tag(1));
        assert_eq!(s.sparse.len(), 10_001, "insert grows to the entity index");

        s.remove(10_000);
        assert_eq!(
            s.sparse.len(),
            10_001,
            "remove alone must not truncate — that is the leak this fixes",
        );

        s.shrink_sparse_tail();
        assert_eq!(s.sparse.len(), 0, "every slot was empty, so all of it goes");
    }

    /// A shrink must never disturb a live entry, including one that is the
    /// *only* live entry far below the high-water mark.
    #[test]
    fn shrink_preserves_live_entries_and_their_dense_indices() {
        let mut s = SparseSetStorage::<Tag>::default();
        s.insert(5, Tag(50));
        s.insert(9_000, Tag(90));
        s.remove(9_000);

        s.shrink_sparse_tail();

        assert_eq!(
            s.sparse.len(),
            6,
            "truncate to one past the last live slot, not to zero",
        );
        assert!(s.contains(5));
        assert_eq!(s.get(5).map(|t| t.0), Some(50));
        assert!(!s.contains(9_000), "the removed entity stays absent");
        assert_eq!(s.len(), 1, "dense arrays are untouched by the shrink");

        // And the storage must still be usable afterwards: an insert above
        // the truncated length has to re-grow correctly.
        s.insert(9_000, Tag(91));
        assert_eq!(s.get(9_000).map(|t| t.0), Some(91));
        assert_eq!(
            s.get(5).map(|t| t.0),
            Some(50),
            "re-growth must not clobber"
        );
    }

    /// The despawn-heavy cycle the issue describes: entities spawn at
    /// ever-higher IDs, get despawned, and the sparse index must not keep
    /// growing without bound once the shrink hook runs.
    #[test]
    fn load_unload_cycle_does_not_accumulate_sparse_slots() {
        let mut s = SparseSetStorage::<Tag>::default();
        let mut next: EntityId = 0;

        for _cycle in 0..8 {
            let base = next;
            for i in 0..100 {
                s.insert(base + i, Tag(i));
            }
            next = base + 100;
            for i in 0..100 {
                s.remove(base + i);
            }
            s.shrink_sparse_tail();
        }

        assert_eq!(s.len(), 0, "every entity was despawned");
        assert_eq!(
            s.sparse.len(),
            0,
            "sparse must not track the high-water mark ({next}) once the \
             population is empty and the shrink hook has run",
        );
    }

    /// `clear_erased` is the save-load path; it must return capacity, not just
    /// length, or a load leaves the previous population's footprint behind.
    #[test]
    fn clear_erased_releases_capacity() {
        let mut s = SparseSetStorage::<Tag>::default();
        for i in 0..1_000 {
            s.insert(i, Tag(i));
        }
        assert!(s.sparse.capacity() >= 1_000);

        s.clear_erased();

        assert_eq!(s.sparse.len(), 0);
        assert_eq!(
            s.sparse.capacity(),
            0,
            "clear() alone keeps capacity — a save-load would hand back nothing",
        );
        assert_eq!(s.dense.capacity(), 0);
        assert_eq!(s.data.capacity(), 0);
    }
}
