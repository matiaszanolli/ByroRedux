//! Name component for entity identification.
//!
//! Sparse — most entities (static geometry, particles) have no name.
//! Only actors, triggers, markers, and quest-relevant objects need one.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::string::FixedString;

/// An interned name attached to an entity.
///
/// Equality is integer comparison via [`FixedString`] — no string
/// comparisons in hot paths.
pub struct Name(pub FixedString);

impl Component for Name {
    type Storage = SparseSetStorage<Self>;
}
