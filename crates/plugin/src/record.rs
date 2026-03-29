//! Records: component bundles loaded from plugins.
//!
//! A [`Record`] is an entity template — a collection of components that
//! can be spawned into the [`World`]. Each record carries a stable
//! [`FormIdPair`] identity and a [`RecordType`] tag.
//!
//! Component data is type-erased via [`ErasedComponentData`] so that
//! records can hold arbitrary component types without the record type
//! itself being generic.

use gamebyro_core::ecs::components::FormIdComponent;
use gamebyro_core::ecs::storage::{Component, EntityId};
use gamebyro_core::ecs::world::World;
use gamebyro_core::form_id::{FormIdPair, FormIdPool};
use std::any::TypeId;
use std::collections::HashMap;

// ── Type-erased component data ──────────────────────────────────────────

/// Object-safe trait for inserting a component into a [`World`] without
/// knowing its concrete type at the call site.
pub trait ErasedComponentData: Send + Sync {
    /// Insert this component onto `entity` in `world`.
    fn insert_into(&self, world: &mut World, entity: EntityId);

    /// The concrete component type's [`TypeId`].
    fn type_id(&self) -> TypeId;
}

/// Concrete wrapper that implements [`ErasedComponentData`] for any
/// component type. One wrapper per `T` — this is the bridge between
/// the generic component world and the type-erased record storage.
pub struct ErasedComponent<T: Component + Clone> {
    pub data: T,
}

impl<T: Component + Clone + Send + Sync + 'static> ErasedComponentData for ErasedComponent<T> {
    fn insert_into(&self, world: &mut World, entity: EntityId) {
        world.insert(entity, self.data.clone());
    }

    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }
}

// ── Record ──────────────────────────────────────────────────────────────

/// An entity template loaded from a plugin.
///
/// Holds a stable identity ([`FormIdPair`]), a type tag, and a bag of
/// type-erased components. Call [`spawn`](Self::spawn) to materialise
/// the record as a live entity in the [`World`].
pub struct Record {
    pub form_id: FormIdPair,
    pub record_type: RecordType,
    pub components: HashMap<TypeId, Box<dyn ErasedComponentData>>,
}

impl Record {
    pub fn new(form_id: FormIdPair, record_type: RecordType) -> Self {
        Self {
            form_id,
            record_type,
            components: HashMap::new(),
        }
    }

    /// Add a component to this record template.
    ///
    /// If a component of the same type was already present, it is replaced.
    pub fn add_component<T: Component + Clone + Send + Sync + 'static>(&mut self, data: T) {
        self.components.insert(
            TypeId::of::<T>(),
            Box::new(ErasedComponent { data }),
        );
    }

    /// Spawn this record as a live entity in the [`World`].
    ///
    /// 1. Allocates a new entity via `world.spawn()`
    /// 2. Interns the record's [`FormIdPair`] into the [`FormIdPool`]
    ///    resource and attaches a [`FormIdComponent`] so the entity is
    ///    findable via [`World::find_by_form_id`]
    /// 3. Inserts all component data from the template
    ///
    /// # Panics
    /// Panics if the [`FormIdPool`] resource has not been inserted into
    /// the world.
    pub fn spawn(&self, world: &mut World) -> EntityId {
        let entity = world.spawn();

        // Intern the stable identity and attach it as a component.
        let form_id = {
            let mut pool = world.resource_mut::<FormIdPool>();
            pool.intern(self.form_id)
        };
        world.insert(entity, FormIdComponent(form_id));

        // Insert all template components.
        for component in self.components.values() {
            component.insert_into(world, entity);
        }

        entity
    }
}

// ── RecordType ──────────────────────────────────────────────────────────

/// High-level category tag for a record.
///
/// Used for filtering and display — not for dispatch. The actual data
/// lives in the component bundle.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordType {
    Weapon,
    Armor,
    Npc,
    Cell,
    WorldSpace,
    Static,
    Misc,
    Unknown(String),
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gamebyro_core::ecs::components::Transform;
    use gamebyro_core::ecs::sparse_set::SparseSetStorage;
    use gamebyro_core::form_id::{LocalFormId, PluginId};
    use gamebyro_core::math::Vec3;

    fn test_pair() -> FormIdPair {
        FormIdPair {
            plugin: PluginId::from_filename("TestPlugin.esm"),
            local: LocalFormId(0x100),
        }
    }

    // A simple test component
    #[derive(Debug, Clone, PartialEq)]
    struct Damage(pub f32);
    impl Component for Damage {
        type Storage = SparseSetStorage<Self>;
    }

    #[test]
    fn record_add_component_and_spawn() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let mut record = Record::new(test_pair(), RecordType::Weapon);
        record.add_component(Transform::from_translation(Vec3::new(1.0, 2.0, 3.0)));
        record.add_component(Damage(50.0));

        let entity = record.spawn(&mut world);

        // Transform present
        let t = world.get::<Transform>(entity).unwrap();
        assert_eq!(t.translation, Vec3::new(1.0, 2.0, 3.0));

        // Damage present
        let d = world.get::<Damage>(entity).unwrap();
        assert_eq!(d.0, 50.0);
    }

    #[test]
    fn spawn_inserts_form_id_component() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = test_pair();
        let record = Record::new(pair, RecordType::Static);
        let entity = record.spawn(&mut world);

        // FormIdComponent is present
        let fid_comp = world.get::<FormIdComponent>(entity).unwrap();

        // Resolve back through the pool — must match original pair
        let pool = world.resource::<FormIdPool>();
        let resolved = pool.resolve(fid_comp.0).unwrap();
        assert_eq!(*resolved, pair);
    }

    #[test]
    fn spawned_entity_findable_by_form_id() {
        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let pair = test_pair();
        let record = Record::new(pair, RecordType::Npc);
        let entity = record.spawn(&mut world);

        // Look up the runtime FormId from the pool
        let pool = world.resource::<FormIdPool>();
        let fid = pool.get(&pair).unwrap();
        drop(pool);

        assert_eq!(world.find_by_form_id(fid), Some(entity));
    }

    #[test]
    fn add_component_replaces_same_type() {
        let mut record = Record::new(test_pair(), RecordType::Weapon);
        record.add_component(Damage(10.0));
        record.add_component(Damage(99.0));

        assert_eq!(record.components.len(), 1);

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let entity = record.spawn(&mut world);

        let d = world.get::<Damage>(entity).unwrap();
        assert_eq!(d.0, 99.0);
    }
}
