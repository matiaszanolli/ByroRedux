//! World: the top-level container for all entities and component storages.
//!
//! Holds one `RwLock`-wrapped storage instance per component type in a
//! `TypeMap`. Storages are lazily initialised on first `insert()`.
//!
//! The `RwLock` enables query methods to take `&self` instead of `&mut self`,
//! so multiple queries can be held simultaneously across different component
//! types without fighting the borrow checker.

use super::lock_tracker;
use super::query::{ComponentRef, QueryRead, QueryWrite};
use super::resource::{Resource, ResourceRead, ResourceWrite};
use super::storage::{Component, ComponentStorage, DynStorage, EntityId};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::RwLock;

/// Panic with a type-aware message when a component storage lock is poisoned.
/// Helps trace the cascade back to the original panicking system.
#[cold]
#[inline(never)]
fn storage_lock_poisoned<T: Component>() -> ! {
    panic!(
        "Storage `{}` RwLock is poisoned — a system panicked while holding this lock. \
         Check the test or system that ran before this panic.",
        std::any::type_name::<T>()
    );
}

/// Panic (without a type name) when a component storage lock is poisoned in
/// a type-erased context (e.g. during [`World::despawn`]).
#[cold]
#[inline(never)]
fn storage_lock_poisoned_erased(type_name: &'static str) -> ! {
    panic!(
        "Storage `{}` RwLock is poisoned — a system panicked while holding this lock. \
         Check the test or system that ran before this panic.",
        type_name
    );
}

/// Panic with a type-aware message when a resource lock is poisoned.
#[cold]
#[inline(never)]
fn resource_lock_poisoned<R: Resource>() -> ! {
    panic!(
        "Resource `{}` RwLock is poisoned — a system panicked while holding this lock. \
         Check the test or system that ran before this panic.",
        std::any::type_name::<R>()
    );
}

pub struct World {
    storages: HashMap<TypeId, RwLock<Box<dyn DynStorage>>>,
    /// Maps each registered storage's `TypeId` to the source type name
    /// (`std::any::type_name::<T>()`). Used by type-erased panic paths
    /// like [`World::despawn`] so a poisoned-lock cascade still names
    /// the offending component. Populated at every storage-creation
    /// site ([`World::register`] and [`World::storage_write`]). #466.
    type_names: HashMap<TypeId, &'static str>,
    resources: HashMap<TypeId, RwLock<Box<dyn Any + Send + Sync>>>,
    next_entity: EntityId,
}

impl World {
    pub fn new() -> Self {
        Self {
            storages: HashMap::new(),
            type_names: HashMap::new(),
            resources: HashMap::new(),
            next_entity: 0,
        }
    }

    /// Allocate a new entity id.
    ///
    /// Panics if the `EntityId` counter would overflow. Previously used
    /// a bare `+= 1` which wraps silently at `u32::MAX` in release
    /// builds, causing entity ID aliasing (the new entity reuses a
    /// still-live ID) and silent component-data corruption. Hitting
    /// this limit requires allocating ~4 billion entities without ever
    /// reclaiming IDs — a symptom of a runaway spawn loop, not legit
    /// usage — so crashing is preferable to silent corruption.
    /// See issue #36.
    pub fn spawn(&mut self) -> EntityId {
        let id = self.next_entity;
        self.next_entity = self
            .next_entity
            .checked_add(1)
            .unwrap_or_else(|| panic!("World::spawn overflowed EntityId (u32::MAX reached)"));
        id
    }

    /// Pre-register a storage for a component type without inserting data.
    ///
    /// Call this during setup if you need `query()`/`query_mut()` to
    /// succeed for a type before any entity has that component.
    /// Otherwise, storage is created lazily on first `insert()`.
    pub fn register<T: Component>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.storages.entry(type_id).or_insert_with(|| {
            let storage: Box<dyn DynStorage> = Box::new(T::Storage::default());
            RwLock::new(storage)
        });
        // Record the type name so type-erased panic paths can surface
        // it (#466). Idempotent: re-registration leaves the entry alone.
        self.type_names
            .entry(type_id)
            .or_insert_with(std::any::type_name::<T>);
    }

    /// Despawn an entity, removing all of its components from every
    /// registered storage.
    ///
    /// Entity IDs are NOT reclaimed — `next_entity` keeps growing. Reuse
    /// without generational tagging would cause silent component-data
    /// corruption on any dangling reference (`Parent(entity)`, script
    /// targets, etc.). See #372 and the companion note on #36.
    ///
    /// No-op if `entity` was never spawned or has already been despawned.
    pub fn despawn(&mut self, entity: EntityId) {
        if entity >= self.next_entity {
            return;
        }
        for (type_id, lock) in self.storages.iter_mut() {
            // Resolve the source type name through the side-table
            // populated at every storage-creation site (#466). Fallback
            // is unreachable in practice — every storage in `self.storages`
            // arrives via `register` or `storage_write`, both of which
            // also populate `type_names` — but kept for defense.
            let type_name = self.type_names.get(type_id).copied().unwrap_or("<unknown>");
            lock.get_mut()
                .unwrap_or_else(|_| storage_lock_poisoned_erased(type_name))
                .remove_entity_erased(entity);
        }
    }

    /// Attach a component to an entity. Overwrites if already present.
    /// Creates the storage for this component type if it doesn't exist yet.
    ///
    /// # Panics (debug only)
    /// Panics if `entity` was never returned by `spawn()`.
    pub fn insert<T: Component>(&mut self, entity: EntityId, component: T) {
        debug_assert!(
            entity < self.next_entity,
            "insert(): entity {} was never spawned (next_entity_id = {})",
            entity,
            self.next_entity,
        );
        self.storage_write::<T>().insert(entity, component);
    }

    /// Insert many `(entity, component)` pairs of the same type in one
    /// storage lookup. Equivalent to calling [`insert`](Self::insert) in
    /// a loop but amortizes the per-call HashMap lookup + `downcast_mut`
    /// across the batch.
    ///
    /// Prefer this when a loader / import path has a natural "collect
    /// all Transforms then all GlobalTransforms then all MeshHandles"
    /// shape. The existing scatter-shot "per-entity 3-5 different types"
    /// cell-loader pattern does NOT benefit without restructuring the
    /// outer loop; see #512 for the migration plan.
    ///
    /// # Panics (debug only)
    /// Panics on the first item whose `entity` was never returned by
    /// `spawn()` (mirrors [`insert`](Self::insert)). The partial state
    /// at that point is undefined — don't catch and reuse the `World`.
    ///
    /// # Example
    /// ```
    /// use byroredux_core::ecs::{Component, World};
    /// use byroredux_core::ecs::sparse_set::SparseSetStorage;
    ///
    /// #[derive(Debug, Clone, Copy, PartialEq)]
    /// struct Health(f32);
    /// impl Component for Health {
    ///     type Storage = SparseSetStorage<Self>;
    /// }
    ///
    /// let mut world = World::new();
    /// let entities: Vec<_> = (0..100).map(|_| world.spawn()).collect();
    /// world.insert_batch(entities.iter().map(|&e| (e, Health(100.0))));
    ///
    /// let q = world.query::<Health>().unwrap();
    /// assert_eq!(q.iter().count(), 100);
    /// ```
    pub fn insert_batch<T, I>(&mut self, items: I)
    where
        T: Component,
        I: IntoIterator<Item = (EntityId, T)>,
    {
        // Capture `next_entity` BEFORE borrowing storages — `storage_write`
        // returns a `&mut T::Storage` that holds the storages borrow for
        // the whole batch, so we can't read `self.next_entity` inside the
        // loop.
        let next_entity = self.next_entity;
        let storage = self.storage_write::<T>();
        // #467 — dispatch to `insert_bulk` so `PackedStorage` picks up
        // its append + single-sort fast path (was O(N × M) per-insert
        // shift; now O(N + M log M)). The debug_assert still fires
        // through an adapter iterator so bulk callers can't sneak in
        // unspawned entity ids.
        storage.insert_bulk(items.into_iter().map(move |(entity, component)| {
            debug_assert!(
                entity < next_entity,
                "insert_batch(): entity {} was never spawned (next_entity_id = {})",
                entity,
                next_entity,
            );
            (entity, component)
        }));
    }

    /// Remove a component from an entity.
    /// Returns `None` if the entity doesn't have this component or if
    /// no storage exists for this type (avoids creating empty storage).
    pub fn remove<T: Component>(&mut self, entity: EntityId) -> Option<T> {
        let storage = self
            .storages
            .get_mut(&TypeId::of::<T>())?
            .get_mut()
            .unwrap_or_else(|_| storage_lock_poisoned::<T>())
            .as_any_mut()
            .downcast_mut::<T::Storage>()?;
        storage.remove(entity)
    }

    /// Get an immutable reference to an entity's component.
    ///
    /// Returns a [`ComponentRef`](super::query::ComponentRef) that holds the
    /// read lock and derefs to `&T`. The lock is held for the lifetime of
    /// the returned wrapper, preventing mutation through `query_mut()`.
    ///
    /// For holding references across multiple component types, use
    /// [`query`](Self::query) / [`query_mut`](Self::query_mut) instead.
    ///
    /// # Panics (debug only)
    /// The `lock_tracker` panics if a conflicting lock on `T`
    /// (specifically an already-held `QueryWrite<T>` on the same
    /// thread) would cause a deadlock. Drop the offending guard
    /// before calling.
    pub fn get<T: Component>(&self, entity: EntityId) -> Option<ComponentRef<'_, T>> {
        let type_id = TypeId::of::<T>();
        let lock = self.storages.get(&type_id)?;
        // RAII scope guard: if lock.read() panics (poisoned), TrackedRead's
        // Drop untracks automatically — no stale tracker state. (#311)
        let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<T>());
        let guard = lock.read().unwrap_or_else(|_| storage_lock_poisoned::<T>());
        match ComponentRef::new(guard, entity, type_id) {
            Some(cr) => {
                // ComponentRef::Drop will untrack — hand off ownership.
                scope.defuse();
                Some(cr)
            }
            None => {
                // scope drops here → TrackedRead::Drop untracks.
                None
            }
        }
    }

    /// Get a mutable reference to an entity's component.
    /// Returns `None` if no storage exists for this type (avoids creating empty storage).
    pub fn get_mut<T: Component>(&mut self, entity: EntityId) -> Option<&mut T> {
        let storage = self
            .storages
            .get_mut(&TypeId::of::<T>())?
            .get_mut()
            .unwrap_or_else(|_| storage_lock_poisoned::<T>())
            .as_any_mut()
            .downcast_mut::<T::Storage>()?;
        storage.get_mut(entity)
    }

    /// Check if an entity has a specific component.
    ///
    /// # Panics (debug only)
    /// Inherits the `lock_tracker` panic contract from `query<T>`:
    /// debug builds panic if a conflicting lock on `T` is already
    /// held on the same thread.
    pub fn has<T: Component>(&self, entity: EntityId) -> bool {
        self.query::<T>().is_some_and(|q| q.contains(entity))
    }

    /// Returns the number of entities that have component `T`.
    ///
    /// # Panics (debug only)
    /// Inherits the `lock_tracker` panic contract from `query<T>`.
    pub fn count<T: Component>(&self) -> usize {
        self.query::<T>().map_or(0, |q| q.len())
    }

    /// Returns the next entity id that will be assigned (monotonic high-water mark).
    ///
    /// This is NOT a count of live entities — it's the next ID that
    /// `spawn()` will return. Entity IDs are never reused.
    pub fn next_entity_id(&self) -> EntityId {
        self.next_entity
    }

    /// Find the first entity with the given name.
    ///
    /// Resolves `name` through the [`StringPool`](crate::string::StringPool)
    /// resource, then scans [`Name`](super::components::Name) components
    /// for a matching symbol. Returns `None` if the string was never
    /// interned or no entity has that name.
    pub fn find_by_name(&self, name: &str) -> Option<EntityId> {
        use super::components::Name;
        use crate::string::StringPool;

        let pool = self.try_resource::<StringPool>()?;
        let sym = pool.get(name)?;
        drop(pool);

        let names = self.query::<Name>()?;
        let result = names.iter().find(|(_, n)| n.0 == sym).map(|(id, _)| id);
        result
    }

    /// Find the first entity with the given [`FormId`](crate::form_id::FormId).
    ///
    /// Scans [`FormIdComponent`](super::components::FormIdComponent) storage
    /// for a matching handle. Returns `None` if no entity has that form ID.
    pub fn find_by_form_id(&self, id: crate::form_id::FormId) -> Option<EntityId> {
        use super::components::FormIdComponent;

        let q = self.query::<FormIdComponent>()?;
        let result = q.iter().find(|(_, fid)| fid.0 == id).map(|(eid, _)| eid);
        result
    }

    // ── Query API (takes &self — RwLock provides interior mutability) ───

    /// Acquire a read-only query for a single component type.
    ///
    /// Returns `None` if no entity has ever had this component
    /// (storage was never created). Use `register::<T>()` during
    /// setup if you need guaranteed access.
    ///
    /// # Panics (debug only)
    /// The `lock_tracker` (enabled in debug builds) panics if a
    /// conflicting lock on `T` — specifically, any already-held
    /// `QueryWrite<T>` on the same thread — would cause a deadlock.
    /// Drop the offending guard before calling. Release builds do
    /// not enforce the check (production hot paths get a zero-cost
    /// no-op).
    pub fn query<T: Component>(&self) -> Option<QueryRead<'_, T>> {
        let type_id = TypeId::of::<T>();
        let lock = self.storages.get(&type_id)?;
        let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<T>());
        let guard = lock.read().unwrap_or_else(|_| storage_lock_poisoned::<T>());
        scope.defuse();
        Some(QueryRead::new(guard, type_id))
    }

    /// Acquire a mutable query for a single component type.
    ///
    /// Returns `None` if no entity has ever had this component.
    /// Only one `QueryWrite` can exist per component type at a time.
    ///
    /// # Panics (debug only)
    /// The `lock_tracker` panics if a conflicting lock on `T`
    /// (a read or write guard) is already held on the same thread.
    /// Drop the offending guard before calling.
    pub fn query_mut<T: Component>(&self) -> Option<QueryWrite<'_, T>> {
        let type_id = TypeId::of::<T>();
        let lock = self.storages.get(&type_id)?;
        let scope = lock_tracker::TrackedWrite::new(type_id, std::any::type_name::<T>());
        let guard = lock
            .write()
            .unwrap_or_else(|_| storage_lock_poisoned::<T>());
        scope.defuse();
        Some(QueryWrite::new(guard, type_id))
    }

    /// Acquire a read query and a write query for two different component
    /// types simultaneously.
    ///
    /// Locks are acquired in `TypeId` order to prevent deadlocks.
    ///
    /// Returns `None` if either storage doesn't exist.
    ///
    /// # Panics
    /// Always panics if `A` and `B` are the same type (would deadlock).
    ///
    /// In debug builds, the `lock_tracker` additionally panics if a
    /// conflicting lock on `A` or `B` is already held on the same
    /// thread, or if the ordered lock graph detects a cross-thread
    /// ABBA risk (#313). Drop any offending guard before calling.
    pub fn query_2_mut<A: Component, B: Component>(
        &self,
    ) -> Option<(QueryRead<'_, A>, QueryWrite<'_, B>)> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "query_2_mut: A and B must be different component types"
        );

        let id_a = TypeId::of::<A>();
        let id_b = TypeId::of::<B>();

        let lock_a = self.storages.get(&id_a)?;
        let lock_b = self.storages.get(&id_b)?;

        // Set up tracker scopes *in TypeId-ascending order* — same order
        // the real locks are acquired below. Pre-#313 the scopes were
        // set up in generic-parameter order (A then B) regardless of
        // TypeId, which looked like ABBA to the global lock-order graph
        // whenever the caller spelled `<B, A>` where TypeId(A) < TypeId(B).
        if id_a < id_b {
            let scope_a = lock_tracker::TrackedRead::new(id_a, std::any::type_name::<A>());
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let guard_a = lock_a
                .read()
                .unwrap_or_else(|_| storage_lock_poisoned::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<B>());
            scope_a.defuse();
            scope_b.defuse();
            Some((
                QueryRead::new(guard_a, id_a),
                QueryWrite::new(guard_b, id_b),
            ))
        } else {
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let scope_a = lock_tracker::TrackedRead::new(id_a, std::any::type_name::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<B>());
            let guard_a = lock_a
                .read()
                .unwrap_or_else(|_| storage_lock_poisoned::<A>());
            scope_a.defuse();
            scope_b.defuse();
            Some((
                QueryRead::new(guard_a, id_a),
                QueryWrite::new(guard_b, id_b),
            ))
        }
    }

    /// Acquire write queries for two different component types simultaneously.
    ///
    /// Locks are acquired in `TypeId` order to prevent deadlocks.
    ///
    /// Returns `None` if either storage doesn't exist.
    ///
    /// # Panics
    /// Always panics if `A` and `B` are the same type (would deadlock).
    ///
    /// In debug builds, the `lock_tracker` additionally panics if a
    /// conflicting lock on `A` or `B` is already held on the same
    /// thread, or if the ordered lock graph detects a cross-thread
    /// ABBA risk (#313). Drop any offending guard before calling.
    pub fn query_2_mut_mut<A: Component, B: Component>(
        &self,
    ) -> Option<(QueryWrite<'_, A>, QueryWrite<'_, B>)> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "query_2_mut_mut: A and B must be different component types"
        );

        let id_a = TypeId::of::<A>();
        let id_b = TypeId::of::<B>();

        let lock_a = self.storages.get(&id_a)?;
        let lock_b = self.storages.get(&id_b)?;

        // TypeId-sorted tracker setup — see `query_2_mut` for rationale (#313).
        if id_a < id_b {
            let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let guard_a = lock_a
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<B>());
            scope_a.defuse();
            scope_b.defuse();
            Some((
                QueryWrite::new(guard_a, id_a),
                QueryWrite::new(guard_b, id_b),
            ))
        } else {
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<B>());
            let guard_a = lock_a
                .write()
                .unwrap_or_else(|_| storage_lock_poisoned::<A>());
            scope_a.defuse();
            scope_b.defuse();
            Some((
                QueryWrite::new(guard_a, id_a),
                QueryWrite::new(guard_b, id_b),
            ))
        }
    }

    // ── Resource API ─────────────────────────────────────────────────────

    /// Insert a global resource. Returns the previous value if one existed.
    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> Option<R> {
        let old = self
            .resources
            .insert(TypeId::of::<R>(), RwLock::new(Box::new(resource)));
        old.and_then(|lock| {
            lock.into_inner()
                .ok()
                .and_then(|boxed| boxed.downcast::<R>().ok())
                .map(|b| *b)
        })
    }

    /// Remove a global resource, returning it if it existed.
    pub fn remove_resource<R: Resource>(&mut self) -> Option<R> {
        let lock = self.resources.remove(&TypeId::of::<R>())?;
        let boxed = lock
            .into_inner()
            .unwrap_or_else(|_| resource_lock_poisoned::<R>());
        Some(*boxed.downcast::<R>().expect("resource type mismatch"))
    }

    /// Read-only access to a resource (takes `&self`).
    ///
    /// # Panics
    /// Panics if the resource was never inserted. The panic message
    /// includes the type name for easy debugging.
    ///
    /// In debug builds the `lock_tracker` additionally panics if a
    /// conflicting lock on `R` is already held on the same thread.
    /// Use `try_resource` if you need a graceful miss.
    pub fn resource<R: Resource>(&self) -> ResourceRead<'_, R> {
        let type_id = TypeId::of::<R>();
        let lock = self.resources.get(&type_id).unwrap_or_else(|| {
            panic!(
                "Resource `{}` not found — call world.insert_resource() first",
                std::any::type_name::<R>()
            )
        });
        let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<R>());
        let guard = lock
            .read()
            .unwrap_or_else(|_| resource_lock_poisoned::<R>());
        scope.defuse();
        ResourceRead::new(guard, type_id)
    }

    /// Mutable access to a resource (takes `&self`).
    ///
    /// # Panics
    /// Panics if the resource was never inserted. The panic message
    /// includes the type name for easy debugging.
    ///
    /// In debug builds the `lock_tracker` additionally panics if a
    /// conflicting lock on `R` (read or write) is already held on the
    /// same thread. Use `try_resource_mut` if you need a graceful miss.
    pub fn resource_mut<R: Resource>(&self) -> ResourceWrite<'_, R> {
        let type_id = TypeId::of::<R>();
        let lock = self.resources.get(&type_id).unwrap_or_else(|| {
            panic!(
                "Resource `{}` not found — call world.insert_resource() first",
                std::any::type_name::<R>()
            )
        });
        let scope = lock_tracker::TrackedWrite::new(type_id, std::any::type_name::<R>());
        let guard = lock
            .write()
            .unwrap_or_else(|_| resource_lock_poisoned::<R>());
        scope.defuse();
        ResourceWrite::new(guard, type_id)
    }

    /// Mutable access to two different resources with TypeId-sorted lock ordering.
    ///
    /// Prevents deadlocks when two systems each need two resources in different order.
    /// Same pattern as `query_2_mut` for component storages.
    ///
    /// # Panics
    /// Always panics if `A` and `B` are the same type (would deadlock),
    /// or if either resource was never inserted.
    ///
    /// In debug builds the `lock_tracker` additionally panics if a
    /// conflicting lock on `A` or `B` is already held on the same
    /// thread, or if the ordered lock graph detects a cross-thread
    /// ABBA risk (#313).
    pub fn resource_2_mut<A: Resource, B: Resource>(
        &self,
    ) -> (ResourceWrite<'_, A>, ResourceWrite<'_, B>) {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "resource_2_mut: A and B must be different resource types"
        );

        let id_a = TypeId::of::<A>();
        let id_b = TypeId::of::<B>();

        let lock_a = self.resources.get(&id_a).unwrap_or_else(|| {
            panic!(
                "Resource `{}` not found — call world.insert_resource() first",
                std::any::type_name::<A>()
            )
        });
        let lock_b = self.resources.get(&id_b).unwrap_or_else(|| {
            panic!(
                "Resource `{}` not found — call world.insert_resource() first",
                std::any::type_name::<B>()
            )
        });

        // TypeId-sorted tracker setup — see `query_2_mut` for rationale (#313).
        if id_a < id_b {
            let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let guard_a = lock_a
                .write()
                .unwrap_or_else(|_| resource_lock_poisoned::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| resource_lock_poisoned::<B>());
            scope_a.defuse();
            scope_b.defuse();
            (
                ResourceWrite::new(guard_a, id_a),
                ResourceWrite::new(guard_b, id_b),
            )
        } else {
            let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());
            let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
            let guard_b = lock_b
                .write()
                .unwrap_or_else(|_| resource_lock_poisoned::<B>());
            let guard_a = lock_a
                .write()
                .unwrap_or_else(|_| resource_lock_poisoned::<A>());
            scope_a.defuse();
            scope_b.defuse();
            (
                ResourceWrite::new(guard_a, id_a),
                ResourceWrite::new(guard_b, id_b),
            )
        }
    }

    /// Try to read a resource, returning `None` if it doesn't exist.
    ///
    /// # Panics (debug only)
    /// The `lock_tracker` panics if a conflicting lock on `R` is
    /// already held on the same thread. The `try_` prefix is about
    /// existence, not re-entrancy — drop the offending guard before
    /// calling.
    pub fn try_resource<R: Resource>(&self) -> Option<ResourceRead<'_, R>> {
        let type_id = TypeId::of::<R>();
        let lock = self.resources.get(&type_id)?;
        let scope = lock_tracker::TrackedRead::new(type_id, std::any::type_name::<R>());
        let guard = lock
            .read()
            .unwrap_or_else(|_| resource_lock_poisoned::<R>());
        scope.defuse();
        Some(ResourceRead::new(guard, type_id))
    }

    /// Try to write a resource, returning `None` if it doesn't exist.
    ///
    /// # Panics (debug only)
    /// The `lock_tracker` panics if a conflicting lock on `R` (read
    /// or write) is already held on the same thread.
    pub fn try_resource_mut<R: Resource>(&self) -> Option<ResourceWrite<'_, R>> {
        let type_id = TypeId::of::<R>();
        let lock = self.resources.get(&type_id)?;
        let scope = lock_tracker::TrackedWrite::new(type_id, std::any::type_name::<R>());
        let guard = lock
            .write()
            .unwrap_or_else(|_| resource_lock_poisoned::<R>());
        scope.defuse();
        Some(ResourceWrite::new(guard, type_id))
    }

    /// Try to mutably access two different resources with TypeId-sorted
    /// lock ordering, returning `None` if either is missing.
    ///
    /// Sibling of [`resource_2_mut`](Self::resource_2_mut) for callers
    /// that need graceful-miss semantics without losing the
    /// TypeId-sorted acquisition guarantee. Sequential
    /// `try_resource_mut` calls would drop the ordering and reintroduce
    /// the ABBA deadlock risk guarded by #313.
    ///
    /// Both existence checks complete BEFORE any lock is acquired — a
    /// missing resource returns `None` without touching either lock.
    ///
    /// # Panics
    /// Always panics if `A` and `B` are the same type (would deadlock).
    ///
    /// In debug builds the `lock_tracker` additionally panics if a
    /// conflicting lock on `A` or `B` is already held on the same
    /// thread, or if the ordered lock graph detects a cross-thread
    /// ABBA risk (#313).
    pub fn try_resource_2_mut<A: Resource, B: Resource>(
        &self,
    ) -> Option<(ResourceWrite<'_, A>, ResourceWrite<'_, B>)> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "try_resource_2_mut: A and B must be different resource types"
        );

        // Existence check before acquiring any lock — see #465.
        if !self.resources.contains_key(&TypeId::of::<A>())
            || !self.resources.contains_key(&TypeId::of::<B>())
        {
            return None;
        }

        Some(self.resource_2_mut::<A, B>())
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Get or create the storage for a component type (requires &mut self).
    fn storage_write<T: Component>(&mut self) -> &mut T::Storage {
        let type_id = TypeId::of::<T>();
        // Record the type name on first lazy creation so type-erased
        // panic paths can surface it (#466). Done outside the entry
        // closure so the borrow on `self.storages` doesn't conflict.
        self.type_names
            .entry(type_id)
            .or_insert_with(std::any::type_name::<T>);
        self.storages
            .entry(type_id)
            .or_insert_with(|| {
                let storage: Box<dyn DynStorage> = Box::new(T::Storage::default());
                RwLock::new(storage)
            })
            .get_mut()
            .unwrap_or_else(|_| storage_lock_poisoned::<T>())
            .as_any_mut()
            .downcast_mut::<T::Storage>()
            .expect("storage type mismatch (bug in World)")
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
