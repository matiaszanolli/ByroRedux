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
    // Track structural mutations so transform propagation can detect
    // reparents/attaches (which change the hierarchy without moving any
    // Transform) via the storage's structural generation.
    const TRACK_CHANGES: bool = true;
}

/// Lists this entity's children in the scene hierarchy.
/// Maintained alongside `Parent` — adding a Parent to a child should
/// also push the child into the parent's Children list.
pub struct Children(pub Vec<EntityId>);

impl Component for Children {
    type Storage = SparseSetStorage<Self>;
    // See `Parent` — child-list edits also signal a hierarchy change.
    const TRACK_CHANGES: bool = true;
}
