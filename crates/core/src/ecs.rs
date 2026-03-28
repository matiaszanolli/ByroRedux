//! Minimal Entity-Component-System foundation.
//!
//! This is a starting point — a sparse-set ECS will replace this once
//! we need real query performance.

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// Opaque entity handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity(pub u64);

/// Stores all component data for the world.
pub struct World {
    next_id: u64,
    /// component_type -> (entity -> component_data)
    storage: HashMap<TypeId, HashMap<u64, Box<dyn Any>>>,
}

impl World {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            storage: HashMap::new(),
        }
    }

    pub fn spawn(&mut self) -> Entity {
        let id = self.next_id;
        self.next_id += 1;
        Entity(id)
    }

    pub fn insert<T: 'static>(&mut self, entity: Entity, component: T) {
        self.storage
            .entry(TypeId::of::<T>())
            .or_default()
            .insert(entity.0, Box::new(component));
    }

    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.storage
            .get(&TypeId::of::<T>())?
            .get(&entity.0)?
            .downcast_ref()
    }

    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        self.storage
            .get_mut(&TypeId::of::<T>())?
            .get_mut(&entity.0)?
            .downcast_mut()
    }

    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Option<T> {
        self.storage
            .get_mut(&TypeId::of::<T>())?
            .remove(&entity.0)
            .and_then(|b| b.downcast().ok())
            .map(|b| *b)
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

    struct Position {
        x: f32,
        y: f32,
    }

    #[test]
    fn spawn_and_query() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position { x: 1.0, y: 2.0 });

        let pos = world.get::<Position>(e).unwrap();
        assert_eq!(pos.x, 1.0);
        assert_eq!(pos.y, 2.0);
    }
}
