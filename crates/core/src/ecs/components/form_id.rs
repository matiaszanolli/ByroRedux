//! Stable record identity component.
//!
//! Attach [`FormIdComponent`] to any entity that represents a record
//! loaded from a plugin (legacy ESM/ESP/ESL or Redux-native).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::form_id::FormId;

/// Links an entity to its canonical record identity.
///
/// The [`FormId`] is a runtime handle — resolve it through
/// [`FormIdPool`](crate::form_id::FormIdPool) to get the stable
/// [`FormIdPair`](crate::form_id::FormIdPair).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormIdComponent(pub FormId);

impl Component for FormIdComponent {
    type Storage = SparseSetStorage<Self>;
}
