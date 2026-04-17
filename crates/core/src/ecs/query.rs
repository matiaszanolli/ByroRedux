//! Query types for safe, concurrent access to component storages.
//!
//! The key insight: `World::query` and `World::query_mut` both take `&self`.
//! The `RwLock` on each storage provides interior mutability, so multiple
//! queries can be held simultaneously as long as they follow normal
//! read/write locking rules (many readers OR one writer per storage).
//!
//! Multi-component queries acquire locks sorted by `TypeId` to prevent
//! deadlocks — regardless of declaration order in user code.

use super::lock_tracker;
use super::storage::{Component, ComponentStorage, DynStorage, EntityId};
use std::any::TypeId;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

/// Immutable query over a single component type.
///
/// Holds a `RwLockReadGuard` — multiple `QueryRead`s can coexist, even
/// for the same component type.
pub struct QueryRead<'w, T: Component> {
    guard: RwLockReadGuard<'w, Box<dyn DynStorage>>,
    type_id: TypeId,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> QueryRead<'w, T> {
    /// Create a new read query. Caller owns a `lock_tracker::TrackedRead`
    /// scope guard for `type_id`; this type's `Drop` impl untracks the read
    /// when the wrapper is dropped. The caller must have called
    /// `scope.defuse()` after successful lock acquisition so the scope hands
    /// ownership of the tracker entry to this wrapper. (See #137.)
    pub(crate) fn new(guard: RwLockReadGuard<'w, Box<dyn DynStorage>>, type_id: TypeId) -> Self {
        Self {
            guard,
            type_id,
            _marker: PhantomData,
        }
    }

    /// Access the underlying typed storage.
    pub fn storage(&self) -> &T::Storage {
        self.guard
            .as_any()
            .downcast_ref::<T::Storage>()
            .expect("storage type mismatch (bug in World)")
    }

    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.storage().get(entity)
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.storage().contains(entity)
    }

    pub fn len(&self) -> usize {
        self.storage().len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage().is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.storage().iter()
    }
}

/// Mutable query over a single component type.
///
/// Holds a `RwLockWriteGuard` — only one `QueryWrite` can exist for a
/// given component type at a time. Other `QueryRead`s for the same type
/// will block until this is dropped.
pub struct QueryWrite<'w, T: Component> {
    guard: RwLockWriteGuard<'w, Box<dyn DynStorage>>,
    type_id: TypeId,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> QueryWrite<'w, T> {
    /// Create a new write query. Caller owns a `lock_tracker::TrackedWrite`
    /// scope guard for `type_id`; this type's `Drop` impl untracks the write
    /// when the wrapper is dropped. The caller must have called
    /// `scope.defuse()` after successful lock acquisition so the scope hands
    /// ownership of the tracker entry to this wrapper. (See #137.)
    pub(crate) fn new(guard: RwLockWriteGuard<'w, Box<dyn DynStorage>>, type_id: TypeId) -> Self {
        Self {
            guard,
            type_id,
            _marker: PhantomData,
        }
    }

    /// Access the underlying typed storage immutably.
    pub fn storage(&self) -> &T::Storage {
        self.guard
            .as_any()
            .downcast_ref::<T::Storage>()
            .expect("storage type mismatch (bug in World)")
    }

    /// Access the underlying typed storage mutably.
    pub fn storage_mut(&mut self) -> &mut T::Storage {
        self.guard
            .as_any_mut()
            .downcast_mut::<T::Storage>()
            .expect("storage type mismatch (bug in World)")
    }

    pub fn get(&self, entity: EntityId) -> Option<&T> {
        self.storage().get(entity)
    }

    pub fn get_mut(&mut self, entity: EntityId) -> Option<&mut T> {
        self.storage_mut().get_mut(entity)
    }

    pub fn contains(&self, entity: EntityId) -> bool {
        self.storage().contains(entity)
    }

    pub fn len(&self) -> usize {
        self.storage().len()
    }

    pub fn is_empty(&self) -> bool {
        self.storage().is_empty()
    }

    pub fn insert(&mut self, entity: EntityId, component: T) {
        self.storage_mut().insert(entity, component);
    }

    pub fn remove(&mut self, entity: EntityId) -> Option<T> {
        self.storage_mut().remove(entity)
    }

    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &T)> {
        self.storage().iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut T)> {
        self.storage_mut().iter_mut()
    }
}

// ── Drop: untrack locks ─────────────────────────────────────────────────

impl<T: Component> Drop for QueryRead<'_, T> {
    fn drop(&mut self) {
        lock_tracker::untrack_read(self.type_id);
    }
}

impl<T: Component> Drop for QueryWrite<'_, T> {
    fn drop(&mut self) {
        lock_tracker::untrack_write(self.type_id);
    }
}

// ── Deref for ergonomic read access on QueryWrite ───────────────────────

impl<T: Component> Deref for QueryRead<'_, T> {
    type Target = T::Storage;
    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}

impl<T: Component> Deref for QueryWrite<'_, T> {
    type Target = T::Storage;
    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}

impl<T: Component> DerefMut for QueryWrite<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.storage_mut()
    }
}

// ── ComponentRef: guard-owning single-component reference ───────────────

/// An immutable reference to a single component, backed by a `RwLockReadGuard`.
///
/// Returned by [`World::get()`](super::world::World::get). Holds the read
/// lock for the component's storage, ensuring the reference remains valid
/// for the lifetime of this wrapper. Derefs to `&T`.
///
/// This replaces the previous unsound pattern where `World::get()` dropped
/// the guard and returned a raw pointer — see issue #35.
pub struct ComponentRef<'w, T: Component> {
    guard: RwLockReadGuard<'w, Box<dyn DynStorage>>,
    entity: EntityId,
    type_id: TypeId,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> ComponentRef<'w, T> {
    /// Create a new component reference. Caller owns a
    /// `lock_tracker::TrackedRead` scope guard for `type_id`.
    ///
    /// On `Some`: the caller must call `scope.defuse()` to hand ownership of
    /// the tracker entry to the returned `ComponentRef`; its `Drop` impl
    /// untracks the read.
    ///
    /// On `None`: the caller must keep its `TrackedRead` armed (do **not**
    /// defuse and do **not** call `untrack_read` manually); the scope's
    /// natural `Drop` will untrack. See `World::get` for the canonical
    /// pattern. (#137)
    pub(crate) fn new(
        guard: RwLockReadGuard<'w, Box<dyn DynStorage>>,
        entity: EntityId,
        type_id: TypeId,
    ) -> Option<Self> {
        // Verify the entity has the component before constructing.
        let storage = guard
            .as_any()
            .downcast_ref::<T::Storage>()
            .expect("storage type mismatch");
        if storage.contains(entity) {
            Some(Self {
                guard,
                entity,
                type_id,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
}

impl<T: Component> Drop for ComponentRef<'_, T> {
    fn drop(&mut self) {
        lock_tracker::untrack_read(self.type_id);
    }
}

impl<T: Component> Deref for ComponentRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // The entity's presence was verified in new(). The guard is held,
        // so the storage cannot be mutated. This unwrap is safe.
        self.guard
            .as_any()
            .downcast_ref::<T::Storage>()
            .expect("storage type mismatch")
            .get(self.entity)
            .expect("component removed (bug: guard should prevent this)")
    }
}
