//! Plugin system: manifests, records, conflict resolution.
//!
//! Redux-native plugins declare dependencies by UUID — no external mod
//! manager needed for load ordering. The [`DataStore`] resolves conflicts
//! via the dependency DAG: deeper dependents override their ancestors,
//! and independent conflicts are flagged for user review.
//!
//! Records are component bundles that spawn into the ECS [`World`] with
//! stable [`FormIdPair`] identity.
//!
//! ## Acknowledgement — xEdit / FNVEdit / TES5Edit / FO4Edit
//!
//! Bethesda does not publish header definitions for the legacy ESM
//! plugin format, BipedObject enums, or per-record sub-record layouts.
//! This crate's per-record parsers (and the [`equip`] module's biped-
//! slot bit constants) are written against the
//! [xEdit project](https://github.com/TES5Edit/TES5Edit) — the
//! canonical community-maintained ESM record reference, by
//! [ElminsterAU](https://github.com/ElminsterAU) and the xEdit team,
//! MPL-2.0 licensed. xEdit's
//! `Core/wbDefinitions{TES4,FNV,TES5,FO4,FO76,SF1}.pas` files
//! document every record shape across every targeted Bethesda title;
//! we cite the relevant `wbDefinitions*.pas:line` ranges in the
//! per-record modules whenever a non-obvious decode lands.

pub mod datastore;
pub mod equip;
pub mod esm;
pub mod legacy;
pub mod manifest;
pub mod record;
pub mod resolver;

pub use datastore::{Conflict, DataStore, ResolvedRecord};
pub use legacy::{LegacyFormId, LegacyLoadOrder};
pub use manifest::PluginManifest;
pub use record::{ErasedComponent, ErasedComponentData, Record, RecordType};
pub use resolver::{ConflictResolution, DependencyResolver};
