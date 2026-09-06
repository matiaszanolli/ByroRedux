//! Source-API compatibility catalog for extender-era Papyrus calls.
//!
//! This catalog does not pretend that recognizing a legacy call makes its
//! binary ABI available. It identifies the engine semantic service a source
//! adapter must target, or records why the operation is intentionally absent.

use crate::component::ExtensionValue;
use crate::content::{ContentCatalog, PluginKind};
use crate::event::{legacy_skse_mod_event_id, LegacySkseModEventPayload, PublishEventCommand};
use crate::identity::{
    ComponentId, FormRef, IdentityError, ScriptFunctionId, ScriptParameterId, StorageKey,
};
use crate::script_function::{
    PapyrusFunctionAlias, ScriptFunctionDeclaration, ScriptParameterDeclaration,
    ScriptResultDeclaration, ScriptValueType, MAX_SCRIPT_ARRAY_ELEMENTS,
};
use crate::service::{
    CONTENT_CATALOG_SERVICE, CONTEXT_SERVICE, EVENT_SERVICE, INPUT_SERVICE,
    LEGACY_CONTAINERS_SERVICE, PRINCIPAL_STORAGE_SERVICE, UI_SERVICE, WORLD_PROJECTION_SERVICE,
};
use crate::storage::{PrincipalStorageCommand, PrincipalStorageValue};
use std::collections::{BTreeMap, BTreeSet};

mod declarations;
mod game_content;
mod input_ui;
mod legacy_containers;
mod mod_events;
mod routes;
mod source_alias;
mod storage_util;

// The old single file was one flat `pub mod compatibility`, so every
// public item stays reachable at its original path (#3851) —
// `byroredux/src/extensions.rs`'s 50-symbol `use` block is unchanged.
pub use declarations::*;
pub use game_content::*;
pub use input_ui::*;
pub use legacy_containers::*;
pub use mod_events::*;
pub use routes::*;
pub use source_alias::*;
pub use storage_util::*;

/// Extender ecosystem that introduced a recognized Papyrus provider/call.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExtenderFamily {
    Skse,
    F4se,
    Xnvse,
    Obse,
    PapyrusUtil,
    JContainers,
    Shared,
}

/// Current source-compatibility disposition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CompatibilityDisposition {
    /// A source-level adapter is installed and delegates to the named service.
    Native,
    /// An engine semantic service exists, but the legacy call still needs an
    /// adapter or explicit source migration.
    Mapped,
    /// The operation has no safe/equivalent engine contract.
    Unsupported,
}

/// Classification returned for one recognized extender-era call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityMatch {
    pub family: ExtenderFamily,
    pub disposition: CompatibilityDisposition,
    pub service: Option<&'static str>,
    pub guidance: &'static str,
}

/// Exact semantic target for a legacy source call that can be ported onto an
/// existing engine service without reproducing extender internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAlias {
    pub provider: &'static str,
    pub function: &'static str,
    pub service: &'static str,
    pub operation: &'static str,
    pub value_kind: &'static str,
    pub constraint: &'static str,
}

#[cfg(test)]
mod tests;
