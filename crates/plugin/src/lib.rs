//! Plugin system: manifests, records, conflict resolution.
//!
//! **Status (#5609): the Redux-native pipeline below has no production
//! caller** — nothing constructs a `DataStore`, `PluginManifest` or
//! `DependencyResolver` outside this crate's own tests. The live load
//! path is `parse_esm_with_load_order` + `FormIdRemap`/`GlobalSlot`
//! (`byroredux/src/cell_loader/load_order.rs`), which this layer was
//! meant to grow into (ROADMAP's Owed list tracks it). The types are
//! kept as the design surface for that loader.
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
pub mod consumables;
pub mod equip;
pub mod esm;
pub mod extension;
pub mod manifest;
pub mod record;
pub mod resolver;

pub use datastore::{Conflict, DataStore, ResolvedRecord};
pub use extension::{ExtensionResolutionError, ResolvedExtensionSet};
pub use manifest::PluginManifest;
pub use record::{ErasedComponent, ErasedComponentData, Record, RecordType};
pub use resolver::{ConflictResolution, DependencyResolver};
