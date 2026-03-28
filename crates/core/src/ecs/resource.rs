//! Resources — global state not tied to any entity.
//!
//! Same RwLock guard pattern as QueryRead/QueryWrite. A developer
//! who knows QueryRead will immediately understand ResourceRead.

use std::any::Any;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::{RwLockReadGuard, RwLockWriteGuard};

/// Marker trait for types that can be stored as global resources.
pub trait Resource: 'static + Send + Sync {}

/// Immutable access to a resource. Holds a `RwLockReadGuard`.
/// Multiple `ResourceRead`s can coexist, even for the same type.
pub struct ResourceRead<'w, R: Resource> {
    guard: RwLockReadGuard<'w, Box<dyn Any + Send + Sync>>,
    _marker: PhantomData<R>,
}

impl<'w, R: Resource> ResourceRead<'w, R> {
    pub(crate) fn new(guard: RwLockReadGuard<'w, Box<dyn Any + Send + Sync>>) -> Self {
        Self {
            guard,
            _marker: PhantomData,
        }
    }
}

impl<R: Resource> Deref for ResourceRead<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.guard
            .downcast_ref::<R>()
            .expect("resource type mismatch (bug in World)")
    }
}

/// Mutable access to a resource. Holds a `RwLockWriteGuard`.
/// Only one `ResourceWrite` can exist per resource type at a time.
pub struct ResourceWrite<'w, R: Resource> {
    guard: RwLockWriteGuard<'w, Box<dyn Any + Send + Sync>>,
    _marker: PhantomData<R>,
}

impl<'w, R: Resource> ResourceWrite<'w, R> {
    pub(crate) fn new(guard: RwLockWriteGuard<'w, Box<dyn Any + Send + Sync>>) -> Self {
        Self {
            guard,
            _marker: PhantomData,
        }
    }
}

impl<R: Resource> Deref for ResourceWrite<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.guard
            .downcast_ref::<R>()
            .expect("resource type mismatch (bug in World)")
    }
}

impl<R: Resource> DerefMut for ResourceWrite<'_, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.guard
            .downcast_mut::<R>()
            .expect("resource type mismatch (bug in World)")
    }
}
