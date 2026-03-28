//! Entity-Component-System with pluggable storage backends.
//!
//! Components declare their preferred storage via `Component::Storage`.
//! Two built-in backends:
//! - [`SparseSetStorage`] — O(1) insert/remove, dense iteration (default)
//! - [`PackedStorage`] — sorted by entity, cache-friendly iteration (opt-in)

pub mod packed;
pub mod query;
pub mod sparse_set;
pub mod storage;
pub mod world;

pub use packed::PackedStorage;
pub use query::{QueryRead, QueryWrite};
pub use sparse_set::SparseSetStorage;
pub use storage::{Component, ComponentStorage, EntityId};
pub use world::World;
