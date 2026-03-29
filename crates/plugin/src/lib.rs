//! Plugin system: manifests, records, conflict resolution.
//!
//! Redux-native plugins declare dependencies by UUID — no external mod
//! manager needed for load ordering. The [`DataStore`] resolves conflicts
//! via the dependency DAG: deeper dependents override their ancestors,
//! and independent conflicts are flagged for user review.
//!
//! Records are component bundles that spawn into the ECS [`World`] with
//! stable [`FormIdPair`] identity.

pub mod datastore;
pub mod legacy;
pub mod manifest;
pub mod record;
pub mod resolver;

pub use datastore::{Conflict, DataStore, ResolvedRecord};
pub use legacy::{LegacyFormId, LegacyLoadOrder};
pub use manifest::PluginManifest;
pub use record::{ErasedComponent, ErasedComponentData, Record, RecordType};
pub use resolver::{ConflictResolution, DependencyResolver};
