//! Manifest-published Papyrus provider functions: lowering and execution.
//!
//! The pipeline has two halves of roughly equal size, split into modules on
//! the IR they share (#3852):
//!
//! - **Front end** (`catalog`, `lower_call`, `lower_program`) resolves
//!   a legal `Provider.Function(...)` or reserved `self.Function(...)`
//!   spelling to the principal-qualified SDK route, validates typed
//!   arguments, and lowers the Papyrus AST to the IR in `ir`. This half is
//!   host-neutral: it never enters Wasm and never touches the ECS.
//! - **Back end** (`execute`) is a statement interpreter that runs that IR
//!   against a live `World`, so it very much does touch the ECS.
//!
//! The old single-file module doc claimed host-neutrality for the module as
//! a whole. That described the front end only — the interpreter was already
//! there, 1160 lines further down.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};
use byroredux_papyrus::ast::{
    AssignOp, BinaryOp, CallArg, Event, Expr, Script, ScriptItem, StateItem, Stmt, Type, UnaryOp,
};
use byroredux_sdk::{
    compatibility::{
        adapt_legacy_send_mod_event, classify_static_call, papyrus_game_content_declarations,
        papyrus_input_declarations, papyrus_legacy_container_declarations,
        papyrus_mod_event_declarations, papyrus_storage_util_declarations, papyrus_ui_declarations,
        parse_storage_util_list_route, parse_storage_util_prefix_route, StorageUtilListOperation,
        PAPYRUS_GAME_GET_PLAYER_ROUTE, PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX,
        PAPYRUS_MOD_EVENT_ROUTE_PREFIX, PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE,
    },
    event::{
        CustomEvent, LegacyModEventSubscriptionCommand, LegacySkseModEventValue,
        LegacySkseVariadicModEventPayload, PublishEventCommand,
    },
    identity::{EntityRef, ExtensionId, FormRef, PrincipalId},
    script_function::{
        ScriptFunctionDeclaration, ScriptFunctionError, ScriptResultDeclaration, ScriptValue,
        ScriptValueType, MAX_SCRIPT_ARRAY_ELEMENTS,
    },
};

use crate::events::{
    ActivateEvent, EquipmentEventBatch, HitEvent, OnCellLoadEvent, OnInitEvent, OnTriggerEnterEvent,
};
use crate::recurring_update::OnUpdateEvent;

const MAX_PROVIDER_HANDLER_NESTING: usize = 32;
const MAX_PROVIDER_CONTINUATIONS: usize = 4_096;
const MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS: usize = 4_096;
const MAX_PENDING_PAPYRUS_MOD_EVENTS: usize = 256;
const PAPYRUS_SELF_PROVIDER: &str = "Self";
const PAPYRUS_SELF_LOCAL: &str = "self";

/// Host callback shared by Papyrus handlers after all ECS guards are dropped.
pub type PapyrusProviderCallback =
    dyn Fn(Option<&PrincipalId>, &str, &[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync;

/// Executable-owned conversion from a raw ECS identity to the same opaque,
/// generational handle used by sandbox callbacks.
pub type PapyrusProviderEntityResolver =
    dyn Fn(EntityId) -> Result<EntityRef, String> + Send + Sync;

/// Executable-owned conversion from a remapped global FormID to portable SDK
/// identity. Unlike entity handles, resolved forms are safe to persist.
pub type PapyrusProviderFormResolver = dyn Fn(u32) -> Result<FormRef, String> + Send + Sync;

/// Executable-owned bridge into the shared custom-event queue. The command is
/// already shaped as the engine SDK event contract; the callback only adds the
/// authenticated legacy-script principal and enforces host queue limits.
pub type PapyrusProviderModEventPublisher =
    dyn Fn(&PrincipalId, PublishEventCommand) -> Result<(), String> + Send + Sync;

mod catalog;
mod execute;
mod ir;
mod lower_call;
mod lower_program;
mod runtime;

// The old single file was one flat `pub mod papyrus_provider`, so every
// public item stays reachable at its original path (#3852). Sibling
// modules cannot see each other's private items, so items shared across
// the split are raised to `pub(crate)` — that widens nothing outside the
// crate.
pub use catalog::*;
pub use execute::*;
pub use ir::*;
pub use lower_call::*;
pub use lower_program::*;
pub use runtime::*;

#[cfg(test)]
mod tests;
