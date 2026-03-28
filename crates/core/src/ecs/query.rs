//! Query types for safe, concurrent access to component storages.
//!
//! The key insight: `World::query` and `World::query_mut` both take `&self`.
//! The `RwLock` on each storage provides interior mutability, so multiple
//! queries can be held simultaneously as long as they follow normal
//! read/write locking rules (many readers OR one writer per storage).
//!
//! Multi-component queries acquire locks sorted by `TypeId` to prevent
//! deadlocks — regardless of declaration order in user code.

use super::storage::{Component, ComponentStorage, EntityId};
use std::any::Any;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

/// Immutable query over a single component type.
///
/// Holds a `RwLockReadGuard` — multiple `QueryRead`s can coexist, even
/// for the same component type.
pub struct QueryRead<'w, T: Component> {
    guard: RwLockReadGuard<'w, Box<dyn Any + Send + Sync>>,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> QueryRead<'w, T> {
    pub(crate) fn new(guard: RwLockReadGuard<'w, Box<dyn Any + Send + Sync>>) -> Self {
        Self {
            guard,
            _marker: PhantomData,
        }
    }

    /// Access the underlying typed storage.
    pub fn storage(&self) -> &T::Storage {
        self.guard
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
    guard: RwLockWriteGuard<'w, Box<dyn Any + Send + Sync>>,
    _marker: PhantomData<T>,
}

impl<'w, T: Component> QueryWrite<'w, T> {
    pub(crate) fn new(guard: RwLockWriteGuard<'w, Box<dyn Any + Send + Sync>>) -> Self {
        Self {
            guard,
            _marker: PhantomData,
        }
    }

    /// Access the underlying typed storage immutably.
    pub fn storage(&self) -> &T::Storage {
        self.guard
            .downcast_ref::<T::Storage>()
            .expect("storage type mismatch (bug in World)")
    }

    /// Access the underlying typed storage mutably.
    pub fn storage_mut(&mut self) -> &mut T::Storage {
        self.guard
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
