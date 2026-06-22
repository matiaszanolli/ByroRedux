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
///
/// For save/load the symbol is serialised as its raw `u32` (via
/// [`fixed_string_serde`](crate::string::fixed_string_serde)); the
/// snapshot restores the owning `StringPool` first, so the symbol
/// resolves to the same string on load.
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct Name(
    #[cfg_attr(feature = "inspect", serde(with = "crate::string::fixed_string_serde"))]
    pub  FixedString,
);

impl Component for Name {
    type Storage = SparseSetStorage<Self>;
}
