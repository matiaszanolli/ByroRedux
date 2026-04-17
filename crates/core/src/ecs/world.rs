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
    resources: HashMap<TypeId, RwLock<Box<dyn Any + Send + Sync>>>,
    next_entity: EntityId,
}

impl World {
    pub fn new() -> Self {
        Self {
            storages: HashMap::new(),
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
        self.storages.entry(TypeId::of::<T>()).or_insert_with(|| {
            let storage: Box<dyn DynStorage> = Box::new(T::Storage::default());
            RwLock::new(storage)
        });
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
            lock.get_mut()
                .unwrap_or_else(|_| storage_lock_poisoned_erased("<unknown>"))
                .remove_entity_erased(entity);
            // `type_id` is present but not used for naming — keep it suppressed.
            let _ = type_id;
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
    pub fn has<T: Component>(&self, entity: EntityId) -> bool {
        self.query::<T>().is_some_and(|q| q.contains(entity))
    }

    /// Returns the number of entities that have component `T`.
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
    /// Panics if `A` and `B` are the same type (would deadlock).
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

        let scope_a = lock_tracker::TrackedRead::new(id_a, std::any::type_name::<A>());
        let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());

        // Always lock in TypeId order to prevent deadlocks.
        if id_a < id_b {
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
    /// Panics if `A` and `B` are the same type (would deadlock).
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

        let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
        let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());

        if id_a < id_b {
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
    /// Panics if `A` and `B` are the same type (would deadlock), or if either
    /// resource was never inserted.
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

        let scope_a = lock_tracker::TrackedWrite::new(id_a, std::any::type_name::<A>());
        let scope_b = lock_tracker::TrackedWrite::new(id_b, std::any::type_name::<B>());

        // Always lock in TypeId order to prevent deadlocks.
        if id_a < id_b {
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

    // ── Internal ────────────────────────────────────────────────────────

    /// Get or create the storage for a component type (requires &mut self).
    fn storage_write<T: Component>(&mut self) -> &mut T::Storage {
        self.storages
            .entry(TypeId::of::<T>())
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
mod tests {
    use super::*;
    use crate::ecs::packed::PackedStorage;
    use crate::ecs::sparse_set::SparseSetStorage;

    struct Health(f32);
    impl Component for Health {
        type Storage = SparseSetStorage<Self>;
    }

    struct Position {
        x: f32,
        y: f32,
    }
    impl Component for Position {
        type Storage = PackedStorage<Self>;
    }

    struct Velocity {
        dx: f32,
        dy: f32,
    }
    impl Component for Velocity {
        type Storage = PackedStorage<Self>;
    }

    // ── Basic World operations ──────────────────────────────────────────

    #[test]
    fn spawn_and_insert() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        world.insert(e, Position { x: 1.0, y: 2.0 });

        assert_eq!(world.get::<Health>(e).unwrap().0, 100.0);
        assert_eq!(world.get::<Position>(e).unwrap().x, 1.0);
    }

    #[test]
    fn despawn_removes_every_component() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();

        world.insert(a, Health(100.0));
        world.insert(a, Position { x: 1.0, y: 2.0 });
        world.insert(a, Velocity { dx: 3.0, dy: 4.0 });
        world.insert(b, Health(50.0));
        world.insert(b, Position { x: 5.0, y: 6.0 });

        world.despawn(a);

        assert!(world.get::<Health>(a).is_none(), "Health not removed");
        assert!(world.get::<Position>(a).is_none(), "Position not removed");
        assert!(world.query::<Velocity>().unwrap().get(a).is_none());

        // b is untouched.
        assert_eq!(world.get::<Health>(b).unwrap().0, 50.0);
        assert_eq!(world.get::<Position>(b).unwrap().x, 5.0);
    }

    #[test]
    fn despawn_nonexistent_entity_is_noop() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Health(100.0));

        // Entity id beyond next_entity — no-op, not a panic.
        world.despawn(12345);

        assert_eq!(world.get::<Health>(a).unwrap().0, 100.0);
    }

    #[test]
    fn despawn_empty_storages_is_noop() {
        let mut world = World::new();
        world.register::<Health>();
        let a = world.spawn();
        // Entity was spawned but never got any component.
        world.despawn(a);
        assert!(world.get::<Health>(a).is_none());
    }

    #[test]
    fn despawn_does_not_reclaim_entity_ids() {
        // next_entity must keep growing — reusing IDs without generation
        // tagging would cause silent component aliasing (see #36, #372).
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Health(1.0));
        let next_before = world.next_entity_id();

        world.despawn(a);
        let c = world.spawn();

        assert_eq!(c, next_before, "spawn should advance, not reclaim");
        assert_ne!(c, a, "reclaimed id would alias stale component data");
    }

    #[test]
    fn different_storage_backends() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();

        world.insert(a, Health(50.0));
        world.insert(b, Health(75.0));
        world.insert(a, Position { x: 0.0, y: 0.0 });

        assert_eq!(world.count::<Health>(), 2);
        assert_eq!(world.count::<Position>(), 1);

        assert!(world.has::<Health>(a));
        assert!(world.has::<Health>(b));
        assert!(world.has::<Position>(a));
        assert!(!world.has::<Position>(b));
    }

    #[test]
    fn remove_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let removed = world.remove::<Health>(e).unwrap();
        assert_eq!(removed.0, 100.0);
        assert!(!world.has::<Health>(e));
    }

    #[test]
    fn mutate_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        world.get_mut::<Health>(e).unwrap().0 -= 25.0;
        assert_eq!(world.get::<Health>(e).unwrap().0, 75.0);
    }

    #[test]
    fn get_nonexistent() {
        let world = World::new();
        assert!(world.get::<Health>(0).is_none());
        assert!(world.get::<Position>(999).is_none());
    }

    #[test]
    fn lazy_storage_init() {
        let world = World::new();
        assert_eq!(world.count::<Health>(), 0);
        assert!(!world.has::<Health>(0));
    }

    // ── Single-component query ──────────────────────────────────────────

    #[test]
    fn query_read_single() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, Health(100.0));
        world.insert(b, Health(50.0));

        let q = world.query::<Health>().unwrap();
        assert_eq!(q.get(a).unwrap().0, 100.0);
        assert_eq!(q.get(b).unwrap().0, 50.0);
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn query_write_single() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        {
            let mut q = world.query_mut::<Health>().unwrap();
            q.get_mut(e).unwrap().0 -= 30.0;
        }

        assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
    }

    #[test]
    fn query_write_insert_remove() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, Health(100.0));

        {
            let mut q = world.query_mut::<Health>().unwrap();
            q.insert(b, Health(200.0));
            q.remove(a);
        }

        assert!(world.get::<Health>(a).is_none());
        assert_eq!(world.get::<Health>(b).unwrap().0, 200.0);
    }

    #[test]
    fn query_returns_none_for_unregistered() {
        let world = World::new();
        assert!(world.query::<Health>().is_none());
        assert!(world.query_mut::<Health>().is_none());
    }

    #[test]
    fn query_after_register() {
        let mut world = World::new();
        world.register::<Health>();

        let q = world.query::<Health>().unwrap();
        assert_eq!(q.len(), 0);
    }

    // ── Multiple concurrent queries ─────────────────────────────────────

    #[test]
    fn multiple_read_queries_coexist() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        world.insert(e, Position { x: 1.0, y: 2.0 });

        // Two reads at the same time — no deadlock, no borrow error.
        let q_health = world.query::<Health>().unwrap();
        let q_pos = world.query::<Position>().unwrap();

        assert_eq!(q_health.get(e).unwrap().0, 100.0);
        assert_eq!(q_pos.get(e).unwrap().x, 1.0);
    }

    #[test]
    fn query_2_mut_read_and_write() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 10.0, y: 20.0 });
        world.insert(e, Velocity { dx: 5.0, dy: 3.0 });

        {
            let (q_pos, mut q_vel) = world.query_2_mut::<Position, Velocity>().unwrap();

            let pos = q_pos.get(e).unwrap();
            let vel = q_vel.get_mut(e).unwrap();
            // Apply position offset to velocity.
            vel.dx += pos.x;
            vel.dy += pos.y;
        }

        assert_eq!(world.get::<Velocity>(e).unwrap().dx, 15.0);
        assert_eq!(world.get::<Velocity>(e).unwrap().dy, 23.0);
    }

    #[test]
    fn query_2_mut_mut_both_writable() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });
        world.insert(e, Velocity { dx: 10.0, dy: 20.0 });

        {
            let (mut q_pos, mut q_vel) = world.query_2_mut_mut::<Position, Velocity>().unwrap();

            let vel = q_vel.get(e).unwrap();
            let dx = vel.dx;
            let dy = vel.dy;

            let pos = q_pos.get_mut(e).unwrap();
            pos.x += dx;
            pos.y += dy;

            let vel = q_vel.get_mut(e).unwrap();
            vel.dx = 0.0;
            vel.dy = 0.0;
        }

        assert_eq!(world.get::<Position>(e).unwrap().x, 11.0);
        assert_eq!(world.get::<Position>(e).unwrap().y, 22.0);
        assert_eq!(world.get::<Velocity>(e).unwrap().dx, 0.0);
    }

    #[test]
    #[should_panic(expected = "must be different component types")]
    fn query_2_mut_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _ = world.query_2_mut::<Health, Health>();
    }

    #[test]
    #[should_panic(expected = "must be different component types")]
    fn query_2_mut_mut_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _ = world.query_2_mut_mut::<Health, Health>();
    }

    // ── Iteration ───────────────────────────────────────────────────────

    #[test]
    fn query_iter() {
        let mut world = World::new();
        for i in 0..5 {
            let e = world.spawn();
            world.insert(e, Health(i as f32 * 10.0));
        }

        let q = world.query::<Health>().unwrap();
        let sum: f32 = q.iter().map(|(_, h)| h.0).sum();
        assert_eq!(sum, 100.0); // 0 + 10 + 20 + 30 + 40
    }

    #[test]
    fn query_iter_mut() {
        let mut world = World::new();
        for i in 0..3 {
            let e = world.spawn();
            world.insert(e, Health(i as f32 * 10.0));
        }

        {
            let mut q = world.query_mut::<Health>().unwrap();
            for (_, health) in q.iter_mut() {
                health.0 *= 2.0;
            }
        }

        let q = world.query::<Health>().unwrap();
        let mut values: Vec<f32> = q.iter().map(|(_, h)| h.0).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(values, vec![0.0, 20.0, 40.0]);
    }

    // ── Intersection iteration (the real-world use case) ────────────────

    #[test]
    fn intersection_iteration() {
        let mut world = World::new();

        // Entity 0: has both Position + Velocity
        let e0 = world.spawn();
        world.insert(e0, Position { x: 0.0, y: 0.0 });
        world.insert(e0, Velocity { dx: 1.0, dy: 2.0 });

        // Entity 1: only Position
        let e1 = world.spawn();
        world.insert(e1, Position { x: 5.0, y: 5.0 });

        // Entity 2: has both
        let e2 = world.spawn();
        world.insert(e2, Position { x: 10.0, y: 10.0 });
        world.insert(e2, Velocity { dx: 3.0, dy: 4.0 });

        {
            let (q_vel, mut q_pos) = world.query_2_mut::<Velocity, Position>().unwrap();

            // Iterate the smaller set (velocity), look up in the larger.
            for (entity, vel) in q_vel.iter() {
                if let Some(pos) = q_pos.get_mut(entity) {
                    pos.x += vel.dx;
                    pos.y += vel.dy;
                }
            }
        }

        // e0 moved, e1 untouched, e2 moved.
        assert_eq!(world.get::<Position>(e0).unwrap().x, 1.0);
        assert_eq!(world.get::<Position>(e0).unwrap().y, 2.0);
        assert_eq!(world.get::<Position>(e1).unwrap().x, 5.0);
        assert_eq!(world.get::<Position>(e1).unwrap().y, 5.0);
        assert_eq!(world.get::<Position>(e2).unwrap().x, 13.0);
        assert_eq!(world.get::<Position>(e2).unwrap().y, 14.0);
    }

    // ── Resource tests ──────────────────────────────────────────────────

    struct DeltaTime(f32);
    impl Resource for DeltaTime {}

    struct GameConfig {
        gravity: f32,
        max_speed: f32,
    }
    impl Resource for GameConfig {}

    #[test]
    fn resource_insert_and_read() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(1.0 / 60.0));

        let dt = world.resource::<DeltaTime>();
        assert!((dt.0 - 1.0 / 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn resource_insert_and_mutate() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(1.0 / 60.0));

        {
            let mut dt = world.resource_mut::<DeltaTime>();
            dt.0 = 1.0 / 30.0;
        }

        let dt = world.resource::<DeltaTime>();
        assert!((dt.0 - 1.0 / 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn two_resource_types_coexist() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(0.016));
        world.insert_resource(GameConfig {
            gravity: -9.81,
            max_speed: 50.0,
        });

        // Both readable at the same time.
        let dt = world.resource::<DeltaTime>();
        let config = world.resource::<GameConfig>();
        assert!((dt.0 - 0.016).abs() < f32::EPSILON);
        assert!((config.gravity - -9.81).abs() < f32::EPSILON);
        assert!((config.max_speed - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    #[should_panic(expected = "Resource `")]
    fn missing_resource_panics_with_type_name() {
        let world = World::new();
        let _ = world.resource::<DeltaTime>();
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn missing_resource_mut_panics() {
        let world = World::new();
        let _ = world.resource_mut::<DeltaTime>();
    }

    #[test]
    fn remove_resource_returns_value() {
        let mut world = World::new();
        world.insert_resource(DeltaTime(0.016));

        let removed = world.remove_resource::<DeltaTime>().unwrap();
        assert!((removed.0 - 0.016).abs() < f32::EPSILON);

        // Gone now.
        assert!(world.try_resource::<DeltaTime>().is_none());
    }

    #[test]
    fn remove_nonexistent_resource_returns_none() {
        let mut world = World::new();
        assert!(world.remove_resource::<DeltaTime>().is_none());
    }

    #[test]
    fn resource_overwrite_returns_old() {
        let mut world = World::new();
        let first = world.insert_resource(DeltaTime(0.016));
        assert!(first.is_none()); // no previous value

        let second = world.insert_resource(DeltaTime(0.033));
        assert!(second.is_some());
        assert!((second.unwrap().0 - 0.016).abs() < f32::EPSILON);

        let dt = world.resource::<DeltaTime>();
        assert!((dt.0 - 0.033).abs() < f32::EPSILON);
    }

    #[test]
    fn resource_visible_to_system_via_scheduler() {
        use crate::ecs::scheduler::Scheduler;

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        world.insert_resource(DeltaTime(0.5));

        let mut scheduler = Scheduler::new();
        scheduler.add(|world: &World, _dt: f32| {
            let dt = world.resource::<DeltaTime>();
            let mut q = world.query_mut::<Health>().unwrap();
            for (_, health) in q.iter_mut() {
                // Drain 60 HP/sec.
                health.0 -= 60.0 * dt.0;
            }
        });

        scheduler.run(&world, 0.0);
        assert_eq!(world.get::<Health>(e).unwrap().0, 70.0);
    }

    #[test]
    fn try_resource_returns_none_when_missing() {
        let world = World::new();
        assert!(world.try_resource::<DeltaTime>().is_none());
        assert!(world.try_resource_mut::<DeltaTime>().is_none());
    }

    // ── Name + StringPool + find_by_name ────────────────────────────────

    // ── FormIdComponent + find_by_form_id ──────────────────────────────

    use crate::ecs::components::FormIdComponent;
    use crate::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

    #[test]
    fn form_id_component_attach_and_query() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x000014),
        };
        let fid = world.resource_mut::<FormIdPool>().intern(pair);

        let e = world.spawn();
        world.insert(e, FormIdComponent(fid));

        let got = world.get::<FormIdComponent>(e).unwrap();
        assert_eq!(got.0, fid);
    }

    #[test]
    fn find_by_form_id_hit() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x000014),
        };
        let fid = world.resource_mut::<FormIdPool>().intern(pair);

        let e = world.spawn();
        world.insert(e, FormIdComponent(fid));

        assert_eq!(world.find_by_form_id(fid), Some(e));
    }

    #[test]
    fn find_by_form_id_miss() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair_a = FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x000014),
        };
        let pair_b = FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x000015),
        };
        let fid_a = world.resource_mut::<FormIdPool>().intern(pair_a);
        let fid_b = world.resource_mut::<FormIdPool>().intern(pair_b);

        let e = world.spawn();
        world.insert(e, FormIdComponent(fid_a));

        assert!(world.find_by_form_id(fid_b).is_none());
    }

    #[test]
    fn find_by_form_id_no_components() {
        let world = World::new();
        let mut pool = FormIdPool::new();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x001),
        });
        assert!(world.find_by_form_id(fid).is_none());
    }

    #[test]
    fn form_id_pool_as_world_resource() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = FormIdPair {
            plugin: PluginId::from_filename("Oblivion.esm"),
            local: LocalFormId(0x100),
        };

        let fid = world.resource_mut::<FormIdPool>().intern(pair);
        let pool = world.resource::<FormIdPool>();
        assert_eq!(pool.resolve(fid).unwrap().local, LocalFormId(0x100));
        assert_eq!(pool.len(), 1);
        assert!(!pool.is_empty());
    }

    // ── Name + StringPool + find_by_name ────────────────────────────────

    use crate::ecs::components::Name;
    use crate::string::StringPool;

    #[test]
    fn name_component_attach_and_query() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());

        let sym = world.resource_mut::<StringPool>().intern("player");
        let e = world.spawn();
        world.insert(e, Name(sym));

        let name = world.get::<Name>(e).unwrap();
        assert_eq!(name.0, sym);

        let pool = world.resource::<StringPool>();
        assert_eq!(pool.resolve(name.0), Some("player"));
    }

    #[test]
    fn find_by_name_hit() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());

        let sym = world.resource_mut::<StringPool>().intern("hero");
        let e = world.spawn();
        world.insert(e, Name(sym));

        assert_eq!(world.find_by_name("hero"), Some(e));
    }

    #[test]
    fn find_by_name_miss() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());

        let sym = world.resource_mut::<StringPool>().intern("hero");
        let e = world.spawn();
        world.insert(e, Name(sym));

        assert!(world.find_by_name("villain").is_none());
    }

    #[test]
    fn find_by_name_no_pool() {
        let world = World::new();
        assert!(world.find_by_name("anything").is_none());
    }

    #[test]
    fn find_by_name_no_name_components() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.resource_mut::<StringPool>().intern("ghost");

        assert!(world.find_by_name("ghost").is_none());
    }

    #[test]
    fn string_pool_as_world_resource() {
        let mut world = World::new();
        world.insert_resource(StringPool::new());

        let sym = {
            let mut pool = world.resource_mut::<StringPool>();
            pool.intern("asset/texture.png")
        };

        let pool = world.resource::<StringPool>();
        assert_eq!(pool.resolve(sym), Some("asset/texture.png"));
    }

    // ── Regression: remove/get_mut must not create empty storage (#39) ──

    #[test]
    fn remove_nonexistent_does_not_create_storage() {
        let mut world = World::new();
        // Remove a component type that was never inserted.
        assert!(world.remove::<Health>(0).is_none());
        // query should still return None (no storage created).
        assert!(world.query::<Health>().is_none());
    }

    #[test]
    #[should_panic(expected = "was never spawned")]
    #[cfg(debug_assertions)]
    fn insert_unspawned_entity_panics_debug() {
        let mut world = World::new();
        // Entity 999 was never spawned — should panic in debug mode.
        world.insert(999, Health(100.0));
    }

    #[test]
    fn get_mut_nonexistent_does_not_create_storage() {
        let mut world = World::new();
        // get_mut on a type that was never inserted.
        assert!(world.get_mut::<Health>(0).is_none());
        // query should still return None.
        assert!(world.query::<Health>().is_none());
    }

    // ── resource_2_mut tests ────────────────────────────────────────

    struct ResA(f32);
    impl Resource for ResA {}
    struct ResB(f32);
    impl Resource for ResB {}

    #[test]
    fn resource_2_mut_both_writable() {
        let mut world = World::new();
        world.insert_resource(ResA(1.0));
        world.insert_resource(ResB(2.0));

        {
            let (mut a, mut b) = world.resource_2_mut::<ResA, ResB>();
            a.0 += 10.0;
            b.0 += 20.0;
        }

        let a = world.resource::<ResA>();
        let b = world.resource::<ResB>();
        assert!((a.0 - 11.0).abs() < 1e-6);
        assert!((b.0 - 22.0).abs() < 1e-6);
    }

    #[test]
    fn resource_2_mut_reverse_order_same_result() {
        let mut world = World::new();
        world.insert_resource(ResA(1.0));
        world.insert_resource(ResB(2.0));

        // Acquire in reverse generic order — lock ordering should still prevent deadlock.
        let (mut b, mut a) = world.resource_2_mut::<ResB, ResA>();
        b.0 = 99.0;
        a.0 = 88.0;
        drop(b);
        drop(a);

        assert!((world.resource::<ResA>().0 - 88.0).abs() < 1e-6);
        assert!((world.resource::<ResB>().0 - 99.0).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "must be different resource types")]
    fn resource_2_mut_same_type_panics() {
        let mut world = World::new();
        world.insert_resource(ResA(1.0));
        let _ = world.resource_2_mut::<ResA, ResA>();
    }

    // ── Deadlock detection (debug-only) ────────────────────────────────

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn query_read_then_write_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _read = world.query::<Health>().unwrap();
        let _write = world.query_mut::<Health>(); // deadlock → panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn query_write_then_read_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _write = world.query_mut::<Health>().unwrap();
        let _read = world.query::<Health>(); // deadlock → panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn query_write_then_write_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _w1 = world.query_mut::<Health>().unwrap();
        let _w2 = world.query_mut::<Health>(); // deadlock → panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn resource_read_then_write_same_type_panics() {
        let mut world = World::new();
        world.insert_resource(ResA(42.0));

        let _read = world.try_resource::<ResA>().unwrap();
        let _write = world.resource_mut::<ResA>(); // deadlock → panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn resource_write_then_read_same_type_panics() {
        let mut world = World::new();
        world.insert_resource(ResA(42.0));

        let _write = world.resource_mut::<ResA>();
        let _read = world.resource::<ResA>(); // deadlock → panic
    }

    #[test]
    fn query_read_then_write_different_types_ok() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        world.insert(e, Position { x: 1.0, y: 2.0 });

        let _read = world.query::<Health>().unwrap();
        let _write = world.query_mut::<Position>().unwrap();
        // No panic — different types.
    }

    #[test]
    fn sequential_query_after_drop_ok() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        {
            let _write = world.query_mut::<Health>().unwrap();
        }
        // Write dropped — read should succeed.
        let _read = world.query::<Health>().unwrap();
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn get_then_query_mut_same_type_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _ref = world.get::<Health>(e).unwrap();
        let _write = world.query_mut::<Health>(); // deadlock → panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn has_while_holding_write_panics() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));

        let _write = world.query_mut::<Health>().unwrap();
        let _ = world.has::<Health>(e); // deadlock → panic
    }

    // ── Poisoned-lock cascade reporting (issue #95) ─────────────────────

    #[test]
    fn poisoned_storage_lock_panics_with_type_name() {
        use std::sync::Arc;

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        let world = Arc::new(world);

        // Poison the Health storage from another thread.
        let w = Arc::clone(&world);
        let _ = std::thread::spawn(move || {
            let _q = w.query_mut::<Health>().unwrap();
            panic!("intentional panic to poison the lock");
        })
        .join();

        // Now any access to Health from this thread should surface a
        // type-aware panic — not the generic "lock poisoned".
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world.query::<Health>();
        }));
        let err = result.expect_err("expected poisoned-lock panic");
        let msg = err
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| err.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains("Health") && msg.contains("poisoned"),
            "panic message should name the component type and mention poisoning, got: {msg}"
        );
    }

    #[test]
    fn poisoned_resource_lock_panics_with_type_name() {
        use std::sync::Arc;

        let mut world = World::new();
        world.insert_resource(ResA(42.0));
        let world = Arc::new(world);

        let w = Arc::clone(&world);
        let _ = std::thread::spawn(move || {
            let _r = w.resource_mut::<ResA>();
            panic!("intentional panic to poison the lock");
        })
        .join();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world.try_resource::<ResA>();
        }));
        let err = result.expect_err("expected poisoned-lock panic");
        let msg = err
            .downcast_ref::<String>()
            .map(|s| s.as_str())
            .or_else(|| err.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            msg.contains("ResA") && msg.contains("poisoned"),
            "panic message should name the resource type and mention poisoning, got: {msg}"
        );
    }

    /// Regression test for issue #36: `spawn()` must panic when the
    /// `EntityId` counter would overflow, not wrap silently.
    #[test]
    #[should_panic(expected = "overflowed EntityId")]
    fn spawn_panics_on_entity_id_overflow() {
        let mut world = World::new();
        // Jam the counter to u32::MAX. The next spawn() returns MAX
        // and the increment inside should panic.
        world.next_entity = EntityId::MAX;
        let last = world.spawn();
        assert_eq!(last, EntityId::MAX);
        // This call must panic — u32::MAX + 1 overflows.
        let _ = world.spawn();
    }

    /// Regression test for issue #137: when the poisoned-lock panic
    /// path fires inside `query`/`resource`/etc., the RAII `TrackedRead`/
    /// `TrackedWrite` scope guard must untrack the pending row. Before
    /// the fix, `track_read` / `track_write` were called directly before
    /// `lock.read()/write()`, and the stale row leaked into the
    /// thread-local tracker — a subsequent `catch_unwind` recovery
    /// would then see a false "deadlock detected" panic on the same type.
    #[test]
    fn lock_tracker_is_clean_after_poisoned_panic() {
        use super::super::lock_tracker;
        use std::sync::Arc;

        // Sanity check: tracker is empty at the start of this test. In
        // debug builds the thread-local map is per-thread and each test
        // runs on a fresh worker thread, so this must hold.
        assert!(lock_tracker::is_clean(), "lock tracker must start clean");

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100.0));
        let world = Arc::new(world);

        // Poison the Health storage from another thread.
        let w = Arc::clone(&world);
        let _ = std::thread::spawn(move || {
            let _q = w.query_mut::<Health>().unwrap();
            panic!("intentional panic to poison the lock");
        })
        .join();

        // Each of the nine affected methods must leave the tracker
        // clean after its poisoned-lock panic unwinds. Iterating over
        // the representative set (single read/write, 2-read/write,
        // resource read/write, resource 2-write, try_read/try_write)
        // covers every `TrackedRead::new` / `TrackedWrite::new` call
        // site in `world.rs`.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world.query::<Health>();
        }));
        assert!(
            lock_tracker::is_clean(),
            "tracker row leaked after query<Health> poison-panic"
        );

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world.query_mut::<Health>();
        }));
        assert!(
            lock_tracker::is_clean(),
            "tracker row leaked after query_mut<Health> poison-panic"
        );

        // query_2_mut: the FIRST tracked scope must also untrack if
        // the second lock panics. Here the Health lock is poisoned —
        // the exact arm that panics depends on TypeId ordering, but
        // either way both scopes must untrack cleanly.
        let mut world2 = World::new();
        let e = world2.spawn();
        world2.insert(e, Health(100.0));
        world2.insert(e, Position { x: 0.0, y: 0.0 });
        let world2 = Arc::new(world2);
        let w = Arc::clone(&world2);
        let _ = std::thread::spawn(move || {
            let _q = w.query_mut::<Health>().unwrap();
            panic!("intentional panic to poison the lock");
        })
        .join();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world2.query_2_mut::<Position, Health>();
        }));
        assert!(
            lock_tracker::is_clean(),
            "tracker row leaked after query_2_mut poison-panic"
        );

        // Resource path mirrors the storage path.
        let mut world3 = World::new();
        world3.insert_resource(ResA(42.0));
        let world3 = Arc::new(world3);
        let w = Arc::clone(&world3);
        let _ = std::thread::spawn(move || {
            let _r = w.resource_mut::<ResA>();
            panic!("intentional panic to poison the resource lock");
        })
        .join();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = world3.try_resource::<ResA>();
        }));
        assert!(
            lock_tracker::is_clean(),
            "tracker row leaked after try_resource<ResA> poison-panic"
        );
    }
}
