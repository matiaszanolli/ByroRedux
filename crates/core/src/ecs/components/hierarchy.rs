//! Hierarchy components for parent-child relationships.
//!
//! Sparse — only entities in a scene graph hierarchy need these.
//! Used by NIF import to preserve the NiNode tree for animation.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};

/// Points to this entity's parent in the scene hierarchy.
pub struct Parent(pub EntityId);

impl Component for Parent {
    type Storage = SparseSetStorage<Self>;
}

/// Lists this entity's children in the scene hierarchy.
/// Maintained alongside `Parent` — adding a Parent to a child should
/// also push the child into the parent's Children list.
pub struct Children(pub Vec<EntityId>);

impl Component for Children {
    type Storage = SparseSetStorage<Self>;
}
