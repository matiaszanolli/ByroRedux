//! Small utility functions used across the application.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Children, World};

/// Add a child entity to a parent's Children component, creating it if needed.
pub(crate) fn add_child(world: &mut World, parent: EntityId, child: EntityId) {
    let has_children = world
        .query::<Children>()
        .map(|q| q.get(parent).is_some())
        .unwrap_or(false);

    if has_children {
        let mut cq = world.query_mut::<Children>().unwrap();
        cq.get_mut(parent).unwrap().0.push(child);
    } else {
        world.insert(parent, Children(vec![child]));
    }
}

pub(crate) fn world_resource_set<R: byroredux_core::ecs::Resource>(
    world: &World,
    f: impl FnOnce(&mut R),
) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}
