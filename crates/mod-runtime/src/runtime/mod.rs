use crate::bindings::byro::mod_host::{
    actor_values, animation, console, content_catalog, context, events, faction_relationships,
    factions, inventory, legacy_containers as wit_legacy_containers, logging, packages, perks,
    reputation, script_functions, state, storage as wit_storage, world_spatial, world_state,
};
use crate::bindings::Extension;
use crate::{CapabilitySet, Principal, Result, SandboxConfig, SandboxError};
use byroredux_sdk::actor_values::{ActorValueCommand, ActorValueOperation, ActorValueState};
use byroredux_sdk::animation::{AnimationEvent, AnimationSnapshot, PlayIdleCommand};
use byroredux_sdk::component::{ExtensionCommand, ExtensionValue, ExtensionValueType};
use byroredux_sdk::console::{
    ConsoleCommandResult, MAX_CONSOLE_ARGUMENT_BYTES, MAX_CONSOLE_OUTPUT_BYTES,
    MAX_CONSOLE_OUTPUT_LINES, MAX_CONSOLE_OUTPUT_LINE_BYTES,
};
use byroredux_sdk::content::{ContentCatalog, PluginKind, MAX_PLUGIN_NAME_BYTES};
use byroredux_sdk::event::{
    custom_event_publishable_by, is_custom_event_id, is_legacy_skse_mod_event_id, ActivationEvent,
    CellLoadEvent, CustomEvent, EquipmentEvent, HitEvent, InputAction, InputActionEvent,
    InputPhase, LegacyModEventSubscriptionCommand, LegacySkseModEventBuilders,
    LegacySkseModEventValue, PublishEventCommand, SessionEvent, SessionPhase, UpdateEvent,
};
use byroredux_sdk::factions::{FactionMembership, FactionSnapshot};
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EntityRef, EventId, ExtensionId, FormRef, PrincipalId, ServiceId,
    StorageKey,
};
use byroredux_sdk::inventory::{InventoryEntry, InventorySnapshot, ItemCategory, ItemMetadata};
use byroredux_sdk::legacy_containers::{LegacyContainerRegistry, LegacyContainerValue};
use byroredux_sdk::manifest::{ComponentSchemaDeclaration, ExtensionManifest};
use byroredux_sdk::packages::{
    EvaluatePackageCommand, PackageSelection, PackageSelectionSource, PackageSnapshot,
};
use byroredux_sdk::perks::{PerkEntry, PerkSnapshot};
use byroredux_sdk::projection::EntityProjection;
use byroredux_sdk::relationships::FactionRelationshipCatalog;
use byroredux_sdk::reputation::{
    ReputationCommand, ReputationEntry, ReputationOperation, ReputationSnapshot,
};
use byroredux_sdk::script_function::{
    ScriptFunctionDeclaration, ScriptValue, MAX_SCRIPT_STRING_BYTES,
};
use byroredux_sdk::service::{
    current_sdk_version, CapabilityDescriptor, ServiceCatalog, ServiceDescriptor, ACTIVATE_EVENT,
    ACTOR_VALUES_READ_CAPABILITY, ACTOR_VALUES_SERVICE, ACTOR_VALUES_WRITE_CAPABILITY,
    ANIMATION_PLAY_CAPABILITY, ANIMATION_READ_CAPABILITY, ANIMATION_SERVICE, CELL_LOAD_EVENT,
    COMPONENTS_WRITE_OWN_CAPABILITY, COMPONENT_STATE_SERVICE, CONSOLE_REGISTER_CAPABILITY,
    CONSOLE_SERVICE, CONTENT_CATALOG_READ_CAPABILITY, CONTENT_CATALOG_SERVICE, CONTEXT_SERVICE,
    EQUIPMENT_EVENT, EVENTS_PUBLISH_CAPABILITY, EVENTS_SUBSCRIBE_CAPABILITY, EVENT_SERVICE,
    EXTENSION_WORLD_SERVICE, FACTIONS_READ_CAPABILITY, FACTIONS_SERVICE,
    FACTION_RELATIONSHIPS_READ_CAPABILITY, FACTION_RELATIONSHIPS_SERVICE, HIT_EVENT,
    INPUT_ACTIONS_SUBSCRIBE_CAPABILITY, INPUT_ACTION_EVENT, INPUT_SERVICE,
    INVENTORY_READ_CAPABILITY, INVENTORY_SERVICE, LEGACY_CONTAINERS_SERVICE, LOGGING_SERVICE,
    PACKAGES_EVALUATE_CAPABILITY, PACKAGES_READ_CAPABILITY, PACKAGES_SERVICE,
    PERKS_READ_CAPABILITY, PERKS_SERVICE, PRINCIPAL_STORAGE_SERVICE, REPUTATION_READ_CAPABILITY,
    REPUTATION_SERVICE, REPUTATION_WRITE_CAPABILITY, SCRIPT_FUNCTIONS_REGISTER_CAPABILITY,
    SCRIPT_FUNCTIONS_SERVICE, SESSION_EVENT, SETTINGS_READ_CAPABILITY,
    SETTINGS_REGISTER_CAPABILITY, SETTINGS_SERVICE, SETTINGS_WRITE_OWN_CAPABILITY,
    STORAGE_READ_OWN_CAPABILITY, STORAGE_WRITE_OWN_CAPABILITY, UI_SERVICE, UPDATE_EVENT,
    WORLD_ENTITY_READ_CAPABILITY, WORLD_PROJECTION_SERVICE, WORLD_SPATIAL_READ_CAPABILITY,
    WORLD_SPATIAL_SERVICE, WORLD_TRANSFORM_READ_CAPABILITY,
};
use byroredux_sdk::settings::{
    SettingDeclaration, SettingValue, SettingWriteCommand, SettingsSnapshot, MAX_SETTING_KEY_BYTES,
};
use byroredux_sdk::spatial::{SpatialHit, SpatialQueryResult, SpatialSnapshot};
use byroredux_sdk::storage::{HostCommand, PrincipalStorageCommand, PrincipalStorageValue};
use semver::Version;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

const LOG_CAPABILITY: &str = crate::LOG_CAPABILITY;

/// One host-attributed diagnostic record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEntry {
    pub principal: crate::PrincipalId,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Guest entry point currently being executed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    Initialize,
    Activate,
    CellLoad,
    Hit,
    Equipment,
    Input,
    Session,
    CustomEvent,
    Update,
    ConsoleCommand,
    ScriptFunction,
    Shutdown,
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialize",
            Self::Activate => "on-activate",
            Self::CellLoad => "on-cell-load",
            Self::Hit => "on-hit",
            Self::Equipment => "on-equipment-change",
            Self::Input => "on-input-action",
            Self::Session => "on-session-event",
            Self::CustomEvent => "on-custom-event",
            Self::Update => "on-update",
            Self::ConsoleCommand => "on-console-command",
            Self::ScriptFunction => "on-script-function",
            Self::Shutdown => "shutdown",
        })
    }
}

/// Why a component was quarantined.
///
/// #3050 — a log-budget overrun and a genuine guest fault both arrive at
/// `enter` as a failed call, and both used to produce an indistinguishable
/// `Quarantined`. They are not the same event: a fault means the guest
/// misbehaved, while a budget overrun means the host never drained what the
/// guest handed it (see [`ModInstance::take_logs`]). An operator triaging a
/// quarantined mod has to be able to tell those apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultKind {
    /// The guest trapped, ran out of fuel, or a host call rejected it.
    Guest,
    /// The instance's retained-log budget was exhausted. Not misbehaviour on
    /// the guest's part on its own — the budget only fills because nothing
    /// drained it.
    LogBudgetExhausted,
    /// The guest attempted to queue more mutations than one entry permits.
    CommandBudgetExhausted,
    /// The guest exceeded the bounded console output contract.
    ConsoleOutputBudgetExhausted,
}

impl fmt::Display for FaultKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Guest => "guest fault",
            Self::LogBudgetExhausted => "log budget exhausted",
            Self::CommandBudgetExhausted => "command budget exhausted",
            Self::ConsoleOutputBudgetExhausted => "console output budget exhausted",
        })
    }
}

/// Attributed fault retained after a component is quarantined.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultInfo {
    pub phase: LifecyclePhase,
    pub kind: FaultKind,
    pub message: String,
}

/// Explicit lifecycle state of one component instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstanceStatus {
    Ready,
    Active,
    Quarantined(FaultInfo),
    Stopped,
}

impl fmt::Display for InstanceStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Quarantined(_) => "quarantined",
            Self::Stopped => "stopped",
        })
    }
}

/// A component compiled for one runtime engine.
pub struct CompiledMod {
    component: Component,
    manifest: ExtensionManifest,
    extension: ExtensionId,
    extension_version: Version,
    component_id: ComponentId,
}

impl fmt::Debug for CompiledMod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompiledMod")
            .field("extension", &self.extension)
            .field("extension_version", &self.extension_version)
            .field("component_id", &self.component_id)
            .finish_non_exhaustive()
    }
}

mod capabilities;
mod convert;
mod host;
mod host_state;
mod instance;
mod sandbox;

// Sibling modules cannot see each other's private items, so the shared
// pieces of the old single-file `runtime.rs` are re-exported here at
// crate visibility (#3853). Nothing new is public outside the crate.
pub(crate) use convert::*;
pub(crate) use host_state::HostState;

pub use instance::ModInstance;
pub use sandbox::SandboxRuntime;
