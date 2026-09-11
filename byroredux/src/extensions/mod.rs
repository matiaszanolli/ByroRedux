//! Live executable-extension ownership and ECS event adaptation.
//!
//! The sandbox runtime deliberately knows nothing about [`World`]. This
//! module is the executable-owned bridge: it assigns opaque SDK handles,
//! delivers canonical events only after ECS guards are dropped, and applies
//! the returned principal-attributed command batch to engine-owned state.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use byroredux_core::character::{FactionReputation, Perks};
use byroredux_core::console::{CommandOutput, CommandRegistry, ConsoleCommand};
use byroredux_core::ecs::components::{
    ActorValues, EquipmentSlots, FactionRanks, FormIdComponent, GlobalTransform, Inventory,
    InventoryIndex, Name, Transform,
};
use byroredux_core::ecs::{EntityId, Resource, World};
use byroredux_core::form_id::{FormIdPair, FormIdPool};
use byroredux_core::string::StringPool;
use byroredux_mod_runtime::{
    CapabilitySet, InstanceStatus, LifecyclePhase, LogEntry, LogLevel, ModInstance, SandboxConfig,
    SandboxError, SandboxRuntime,
};
use byroredux_scripting::PapyrusProviderCatalog;
use byroredux_sdk::actor_values::{
    ActorValueCommand, ActorValueOperation, ActorValueState, MAX_ACTOR_VALUES_PER_ENTITY,
};
use byroredux_sdk::animation::{AnimationEvent, AnimationSnapshot, PlayIdleCommand};
use byroredux_sdk::compatibility::{
    adapt_papyrus_game_get_form_from_file, adapt_papyrus_game_get_light_mod_by_name,
    adapt_papyrus_game_get_light_mod_count, adapt_papyrus_game_get_light_mod_dependency_count,
    adapt_papyrus_game_get_light_mod_name, adapt_papyrus_game_get_mod_by_name,
    adapt_papyrus_game_get_mod_count, adapt_papyrus_game_get_mod_dependency_count,
    adapt_papyrus_game_get_mod_name, adapt_papyrus_game_get_nth_light_mod_dependency,
    adapt_papyrus_game_is_plugin_installed, adapt_papyrus_input_get_mapped_control,
    adapt_papyrus_input_get_mapped_key, adapt_papyrus_ui_is_menu_open,
    adapt_storage_util_global_form_filter, adapt_storage_util_global_list,
    adapt_storage_util_global_prefix, adapt_storage_util_global_scalar,
    parse_storage_util_list_route, parse_storage_util_prefix_route, PapyrusInputBinding,
    PapyrusUiMenuSnapshot, StorageUtilListCall, StorageUtilListKind, StorageUtilListOperation,
    StorageUtilListResult, StorageUtilListValue, StorageUtilPrefixKind, StorageUtilPrefixOperation,
    StorageUtilScalarCall, StorageUtilScalarResult, PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE,
    PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE, PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE,
    PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE, PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE,
    PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE, PAPYRUS_GAME_GET_MOD_COUNT_ROUTE,
    PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE, PAPYRUS_GAME_GET_MOD_NAME_ROUTE,
    PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE, PAPYRUS_GAME_GET_PLAYER_ROUTE,
    PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE, PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE,
    PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE, PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX,
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
    PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE, PAPYRUS_UI_IS_MENU_OPEN_ROUTE,
};
use byroredux_sdk::component::{
    ComponentSchema, ComponentStoreError, ComponentStoreLimits, ExtensionComponentStore,
    ExtensionStateSnapshot, ExtensionValue, PersistedComponentRow, RestoredComponentRow,
    EXTENSION_STATE_FORMAT_VERSION, MIN_EXTENSION_STATE_FORMAT_VERSION,
};
use byroredux_sdk::console::ConsoleCommandResult;
use byroredux_sdk::content::ContentCatalog;
use byroredux_sdk::event::{
    custom_event_publishable_by, is_custom_event_id, is_legacy_skse_mod_event_id, ActivationEvent,
    CellLoadEvent, CustomEvent, EquipmentEvent, HitEvent, InputAction as SdkInputAction,
    InputActionEvent, InputPhase, LegacyModEventSubscriptionCommand, LegacySkseModEventBuilders,
    LegacySkseModEventValue, PersistedLegacyModEventBuilders, PublishEventCommand, SessionEvent,
    SessionPhase, UpdateEvent,
};
use byroredux_sdk::factions::{FactionMembership, FactionSnapshot, MAX_FACTIONS_PER_ENTITY};
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EntityRef, ExtensionId, FormRef, PrincipalId,
};
use byroredux_sdk::inventory::{
    InventoryEntry, InventorySnapshot, MAX_INVENTORY_ENTRIES_PER_ENTITY,
};
use byroredux_sdk::legacy_containers::{
    LegacyContainerError, LegacyContainerRegistry, LegacyContainerValue, PersistedLegacyContainers,
};
use byroredux_sdk::manifest::ExtensionManifest;
use byroredux_sdk::packages::{
    EvaluatePackageCommand, PackageSelection, PackageSnapshot, MAX_PACKAGE_CANDIDATES,
    MAX_PACKAGE_REFERENCES_PER_ENTITY, MAX_PACKAGE_SELECTIONS_PER_ENTITY,
};
use byroredux_sdk::perks::{PerkEntry, PerkSnapshot, MAX_PERKS_PER_ENTITY};
use byroredux_sdk::projection::{EntityProjection, WorldTransform, MAX_ENTITY_NAME_BYTES};
use byroredux_sdk::relationships::FactionRelationshipCatalog;
use byroredux_sdk::reputation::{
    ReputationCommand, ReputationEntry, ReputationOperation, ReputationSnapshot,
    MAX_REPUTATIONS_PER_ENTITY,
};
use byroredux_sdk::script_function::{ScriptFunctionDeclaration, ScriptValue};
use byroredux_sdk::service::{
    ACTIVATE_EVENT, CELL_LOAD_EVENT, CONSOLE_REGISTER_CAPABILITY, EQUIPMENT_EVENT,
    EVENTS_SUBSCRIBE_CAPABILITY, HIT_EVENT, INPUT_ACTIONS_SUBSCRIBE_CAPABILITY, INPUT_ACTION_EVENT,
    SCRIPT_FUNCTIONS_REGISTER_CAPABILITY, SESSION_EVENT, SETTINGS_REGISTER_CAPABILITY,
    UPDATE_EVENT,
};
use byroredux_sdk::settings::{
    SettingControlDeclaration, SettingDeclaration, SettingValue as SdkSettingValue,
    SettingsSnapshot,
};
use byroredux_sdk::spatial::{SpatialReference, SpatialSnapshot, MAX_SPATIAL_REFERENCES};
use byroredux_sdk::storage::{
    HostCommand, PersistedPrincipalStorage, PrincipalStorageError, PrincipalStorageLimits,
    PrincipalStorageStore,
};
use thiserror::Error;

use crate::components::AmbientPackageRuntime;

/// Build a [`DeliveryCommitContext`] from a host and a stats sink (#3863).
///
/// Fourteen dispatch sites used to spell out all eleven `&mut self.<field>`
/// borrows by hand — about 154 lines of pure plumbing, and a twelfth pending-
/// command queue (the file already has six, and the SDK surface is still
/// growing) meant editing every one of them.
///
/// A macro rather than a constructor method because the borrows must stay
/// *field-disjoint*: `fn commit_context(&mut self, ..)` would take one `&mut
/// self` covering the whole host, which conflicts with the `&mut
/// HostedComponent` every call site already holds out of `self.components`.
/// Expanding in place keeps the eleven borrows separate, exactly as the
/// hand-written literal did. (The alternative — hoisting the nine owned
/// `pending_*`/diagnostics fields into a `DeliveryState` sub-struct so a
/// constructor can borrow just that — is the shape this should eventually
/// take, but it rewrites several hundred field paths across the workspace's
/// largest file for the same benefit this gets in one place.)
///
/// Takes the host as an expression so it works for both spellings in use:
/// `self` inside `ExtensionHost` methods, and a `host` binding in the free
/// functions.
macro_rules! delivery_commit_context {
    ($host:expr, $stats:expr) => {
        // Path-qualified: this macro expands in `dispatch` and `install`
        // as well as here, and must not depend on each one's imports.
        crate::extensions::commands::DeliveryCommitContext {
            state: &mut $host.state,
            principal_storage: &mut $host.principal_storage,
            legacy_containers: &mut $host.legacy_containers,
            pending_custom_events: &mut $host.pending_custom_events,
            pending_setting_writes: &mut $host.pending_setting_writes,
            pending_actor_value_writes: &mut $host.pending_actor_value_writes,
            pending_package_evaluations: &mut $host.pending_package_evaluations,
            pending_animation_commands: &mut $host.pending_animation_commands,
            pending_reputation_writes: &mut $host.pending_reputation_writes,
            diagnostics: &mut $host.diagnostics,
            stats: &mut $stats,
        }
    };
}

mod capture;
mod commands;
mod dispatch;
mod install;
mod legacy_compat;
mod persist;
mod systems;
#[cfg(test)]
mod tests;

// #3843 — `extensions` was one file until the split, so the rest of the
// binary names these at `crate::extensions::*`. Re-exporting here keeps
// that the module's public face: which region a system happens to live in
// is an internal arrangement, and callers should not have to track it.
pub(in crate::extensions) use capture::{
    capture_entity_projections, capture_spatial_snapshot, entities_by_form, entity_projection,
    forms_by_entity, RawEntityProjection,
};
pub(crate) use capture::{RawActivation, RawCellLoad, RawEquipmentChange, RawHit};
pub(crate) use commands::animation_event;
pub(in crate::extensions) use commands::{
    apply_delivery_result, apply_pending_world_commands, enter_guest,
};
pub(crate) use install::load_requested_extensions;
pub(crate) use persist::{
    capture_extension_state, preflight_extension_state, restore_extension_state,
    shutdown_extension_host,
};
#[cfg(test)]
pub(crate) use systems::pending_session_events;
pub(in crate::extensions) use systems::{
    emit_diagnostics, register_extension_setting, settings_snapshot_from_registry,
};
pub(crate) use systems::{
    extension_activation_dispatch_system, extension_cell_load_dispatch_system,
    extension_content_catalog_sync_system, extension_custom_event_dispatch_system,
    extension_engine_settings_sync_system, extension_equipment_dispatch_system,
    extension_hit_dispatch_system, extension_input_bindings_sync_system,
    extension_input_dispatch_system, extension_player_entity_sync_system,
    extension_session_dispatch_system, extension_setting_write_apply_system,
    extension_ui_menu_sync, extension_update_dispatch_system, queue_session_event,
    register_console_commands, register_legacy_script_principals,
    sync_extension_script_function_invoker, ExtensionHostSlot, SessionEventQueue,
};

const EXTENSION_STATE_RESOURCE: &str = "ByroExtensionState";
const MAX_PERSISTED_EXTENSION_ROWS: usize = 262_144;
const MAX_PENDING_SESSION_EVENTS: usize = 64;
const MAX_PENDING_CUSTOM_EVENTS: usize = 256;
const MAX_PENDING_CUSTOM_EVENT_BYTES: usize = 1024 * 1024;
const MAX_PENDING_SETTING_WRITES: usize = 256;
const MAX_PENDING_ACTOR_VALUE_WRITES: usize = 256;
const MAX_PENDING_PACKAGE_EVALUATIONS: usize = 256;
const MAX_PENDING_ANIMATION_COMMANDS: usize = 256;
const MAX_PENDING_REPUTATION_WRITES: usize = 256;

/// Package-relative component bytes supplied to [`ExtensionHost::install_package`].
pub(crate) type ExtensionArtifacts = BTreeMap<ComponentId, Vec<u8>>;

/// Errors that prevent a package or live event batch from being accepted.
#[derive(Debug, Error)]
pub(crate) enum ExtensionHostError {
    #[error("sandbox runtime rejected the extension: {0}")]
    Sandbox(#[from] SandboxError),
    #[error("extension state rejected the package or command batch: {0}")]
    State(#[from] ComponentStoreError),
    #[error("principal storage rejected the package, command batch, or saved state: {0}")]
    PrincipalStorage(#[from] PrincipalStorageError),
    #[error("legacy container state is invalid: {0}")]
    LegacyContainers(#[from] LegacyContainerError),
    #[error("extension {0} is already installed")]
    AlreadyInstalled(ExtensionId),
    #[error("Papyrus provider alias could not be published: {0}")]
    PapyrusProviderAlias(String),
    #[error("extension {extension} is missing component artifact {component}")]
    MissingArtifact {
        extension: ExtensionId,
        component: ComponentId,
    },
    #[error("entity handle space is exhausted")]
    HandleSpaceExhausted,
    #[error("world generation space is exhausted")]
    GenerationExhausted,
    #[error("hit damage must be finite and non-negative, got {0}")]
    InvalidHitDamage(f32),
    #[error("component-store command budget is smaller than the sandbox entry budget")]
    IncompatibleCommandBudgets,
    #[error("script function {0} is not registered")]
    UnknownScriptFunction(String),
    #[error("script function {function} is unavailable: {reason}")]
    ScriptFunctionUnavailable { function: String, reason: String },
    #[error("deferred command batch from script function {0} was rejected")]
    ScriptFunctionCommandBatchRejected(String),
    #[error("extension-state format {actual} is unsupported; this engine supports {expected}")]
    UnsupportedStateFormat { actual: u32, expected: u32 },
    #[error("extension state contains {actual} rows, exceeding the limit of {maximum}")]
    PersistedRowBudgetExceeded { actual: usize, maximum: usize },
    #[error("saved extension state repeats {principal}/{schema} on form {entity:?}")]
    DuplicatePersistedRow {
        principal: PrincipalId,
        schema: byroredux_sdk::identity::ComponentSchemaId,
        entity: FormRef,
    },
    #[error("saved extension state repeats legacy containers for principal {0}")]
    DuplicateLegacyContainerPrincipal(PrincipalId),
    #[error("saved extension state repeats ModEvent builders for principal {0}")]
    DuplicateLegacyModEventBuilderPrincipal(PrincipalId),
    #[error("saved extension state contains invalid ModEvent builders for principal {0}")]
    InvalidLegacyModEventBuilders(PrincipalId),
    #[error(
        "{count} extension row(s) target transient entities without stable form identity; refusing a lossy save"
    )]
    UnpersistableRows { count: usize },
}

/// Engine-owned diagnostic emitted by one hosted component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ExtensionDiagnostic {
    Log {
        extension: ExtensionId,
        component: ComponentId,
        entry: LogEntry,
    },
    Fault {
        extension: ExtensionId,
        component: ComponentId,
        message: String,
    },
}

/// Summary of one ECS-marker dispatch pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExtensionDispatchStats {
    pub events: usize,
    pub deliveries: usize,
    pub commands_applied: usize,
    pub faults: usize,
}

#[derive(Clone, Debug)]
struct EntityHandleRegistry {
    world_generation: u64,
    next_object: u64,
    by_entity: BTreeMap<EntityId, EntityRef>,
    by_handle: BTreeMap<EntityRef, EntityId>,
}

impl EntityHandleRegistry {
    fn new() -> Self {
        Self {
            world_generation: 1,
            next_object: 1,
            by_entity: BTreeMap::new(),
            by_handle: BTreeMap::new(),
        }
    }

    fn handle_for(&mut self, entity: EntityId) -> Result<EntityRef, ExtensionHostError> {
        if let Some(handle) = self.by_entity.get(&entity) {
            return Ok(*handle);
        }
        let handle = EntityRef::new(self.world_generation, self.next_object)
            .ok_or(ExtensionHostError::HandleSpaceExhausted)?;
        self.next_object = self
            .next_object
            .checked_add(1)
            .ok_or(ExtensionHostError::HandleSpaceExhausted)?;
        self.by_entity.insert(entity, handle);
        self.by_handle.insert(handle, entity);
        Ok(handle)
    }

    fn resolve(&self, handle: EntityRef) -> Option<EntityId> {
        (handle.world_generation() == self.world_generation)
            .then(|| self.by_handle.get(&handle).copied())
            .flatten()
    }

    fn begin_world_generation(&mut self) -> Result<u64, ExtensionHostError> {
        self.world_generation = self
            .world_generation
            .checked_add(1)
            .ok_or(ExtensionHostError::GenerationExhausted)?;
        self.next_object = 1;
        self.by_entity.clear();
        self.by_handle.clear();
        Ok(self.world_generation)
    }
}

struct HostedComponent {
    extension: ExtensionId,
    component: ComponentId,
    receives_activate: bool,
    receives_cell_load: bool,
    receives_equipment: bool,
    receives_input: bool,
    input_actions: BTreeSet<SdkInputAction>,
    receives_session: bool,
    session_phases: BTreeSet<SessionPhase>,
    custom_subscriptions: BTreeSet<byroredux_sdk::identity::EventId>,
    receives_hit: bool,
    recurring_update: Option<RecurringCadence>,
    instance: ModInstance,
}

#[derive(Clone, Debug)]
struct HostedConsoleCommand {
    name: String,
    description: String,
    extension: ExtensionId,
    component: ComponentId,
    declaration_index: u32,
}

#[derive(Clone, Debug)]
struct HostedScriptFunction {
    name: String,
    extension: ExtensionId,
    component: ComponentId,
    declaration_index: u32,
    declaration: ScriptFunctionDeclaration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RecurringCadence {
    interval_seconds: f32,
    seconds_until_next: f32,
    elapsed_seconds: f32,
}

impl RecurringCadence {
    fn from_millis(interval_millis: u32) -> Self {
        let interval_seconds = interval_millis as f32 / 1_000.0;
        Self {
            interval_seconds,
            seconds_until_next: interval_seconds,
            elapsed_seconds: 0.0,
        }
    }

    fn advance(&mut self, dt: f32) -> Option<f32> {
        self.seconds_until_next -= dt;
        self.elapsed_seconds += dt;
        if self.seconds_until_next > 0.0 {
            return None;
        }
        self.seconds_until_next += self.interval_seconds;
        Some(std::mem::take(&mut self.elapsed_seconds))
    }
}

/// Main-thread owner of executable packages and their dynamic component state.
pub(crate) struct ExtensionHost {
    runtime: SandboxRuntime,
    state: ExtensionComponentStore,
    principal_storage: PrincipalStorageStore,
    handles: EntityHandleRegistry,
    components: Vec<HostedComponent>,
    diagnostics: Vec<ExtensionDiagnostic>,
    retained_rows: Vec<PersistedComponentRow>,
    retained_storage: Vec<PersistedPrincipalStorage>,
    legacy_script_principals: BTreeSet<PrincipalId>,
    legacy_random_state: u64,
    legacy_containers: BTreeMap<PrincipalId, LegacyContainerRegistry>,
    legacy_mod_event_builders: BTreeMap<PrincipalId, LegacySkseModEventBuilders>,
    retained_legacy_containers: Vec<PersistedLegacyContainers>,
    pending_custom_events: Vec<CustomEvent>,
    pending_setting_writes: Vec<byroredux_sdk::settings::SettingWriteCommand>,
    pending_actor_value_writes: Vec<ActorValueCommand>,
    pending_package_evaluations: Vec<EvaluatePackageCommand>,
    pending_animation_commands: Vec<PlayIdleCommand>,
    pending_reputation_writes: Vec<ReputationCommand>,
    content_catalog: Arc<ContentCatalog>,
    faction_relationships: Arc<FactionRelationshipCatalog>,
    engine_settings: Arc<SettingsSnapshot>,
    console_commands: Vec<HostedConsoleCommand>,
    script_functions: Vec<HostedScriptFunction>,
    papyrus_providers: PapyrusProviderCatalog,
    input_bindings: Vec<PapyrusInputBinding>,
    ui_menu_snapshot: PapyrusUiMenuSnapshot,
    player_entity: Option<EntityId>,
}

fn form_ref(pair: FormIdPair) -> FormRef {
    FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0)
}

fn sdk_input_action(action: crate::interaction::InputAction) -> SdkInputAction {
    match action {
        crate::interaction::InputAction::MoveForward => SdkInputAction::MoveForward,
        crate::interaction::InputAction::MoveBackward => SdkInputAction::MoveBackward,
        crate::interaction::InputAction::StrafeLeft => SdkInputAction::StrafeLeft,
        crate::interaction::InputAction::StrafeRight => SdkInputAction::StrafeRight,
        crate::interaction::InputAction::Jump => SdkInputAction::Jump,
        crate::interaction::InputAction::Sprint => SdkInputAction::Sprint,
        crate::interaction::InputAction::Activate => SdkInputAction::Activate,
        crate::interaction::InputAction::Attack => SdkInputAction::Attack,
        crate::interaction::InputAction::Block => SdkInputAction::Block,
        crate::interaction::InputAction::Inventory => SdkInputAction::Inventory,
        crate::interaction::InputAction::Quicksave => SdkInputAction::Quicksave,
        crate::interaction::InputAction::Quickload => SdkInputAction::Quickload,
        crate::interaction::InputAction::Pause => SdkInputAction::Pause,
    }
}

/// Every production file in this directory, concatenated, for the
/// source-shape guards below.
///
/// #3843 split `extensions.rs` into this directory. The guards count
/// occurrences of a literal across the whole host, so reading only
/// `mod.rs` would have quietly reduced every count to zero and turned two
/// real invariants into assertions that pass by not looking — the hazard
/// that made the split worth doing carefully rather than quickly.
///
/// `tests.rs` is deliberately absent: the guards run over *production*
/// text, and `production_text` strips `#[cfg(test)] mod` blocks by brace
/// matching. A test module that is a whole file has no such block for it
/// to strip, so including it would fold test code into the counts.
/// `covers_every_production_file_in_the_directory` below pins the list.
#[cfg(test)]
const SOURCES: &str = concat!(
    include_str!("capture.rs"),
    include_str!("commands.rs"),
    include_str!("dispatch.rs"),
    include_str!("install.rs"),
    include_str!("legacy_compat.rs"),
    include_str!("persist.rs"),
    include_str!("systems.rs"),
    // LAST, and that is load-bearing. `production_text` strips
    // `#[cfg(test)] mod` blocks by counting braces, and it is not
    // string-aware — a `{` inside a literal in the stripped block leaves the
    // count unbalanced, so the strip runs past the block's real end. Inside
    // one file that over-run hits EOF harmlessly; with another file
    // concatenated after it, it silently eats that file instead. This module
    // is the one carrying a test block, so it goes at the end where an
    // over-run has nothing left to consume. Caught by the guards themselves
    // during #3843 — with mod.rs first they reported 1/0/0 against a true
    // 2/1/2, which is the "passes by not looking" failure they exist to
    // prevent, landing one assertion away from silent.
    include_str!("mod.rs"),
);

/// Source-shape guards for the guest-entry/commit plumbing (#3863).
#[cfg(test)]
mod delivery_plumbing_shape_tests {
    use super::SOURCES as EXTENSIONS_RS;
    /// This file's own text, for the concat-coverage scan — kept separate
    /// from `SOURCES` so the scan reads the list, not the whole host.
    const SOURCES_LIST: &str = include_str!("mod.rs");

    /// Production text only: `#[cfg(test)] mod <name> { .. }` blocks removed
    /// by brace matching.
    ///
    /// Truncating at the first `#[cfg(test)]` instead would silently drop the
    /// production code that follows an inner test module — this file has one
    /// at ~line 3900 with thousands of production lines after it — and the
    /// scans below would then pass by not looking (the #4069 lesson). A
    /// block-less `mod foo;` is skipped explicitly for the same reason: brace
    /// matching from there would swallow the next item whole.
    fn production_text(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let mut rest = src;
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            const ATTR: &str = "#[cfg(test)]";
            // The `mod` must follow the attribute IMMEDIATELY (whitespace and
            // an optional visibility only). Searching the whole remainder
            // would let an item-level `#[cfg(test)]` bind to some distant
            // `mod` and strip every production line between them — which is
            // how this helper first reported "no hand-written literals": by
            // deleting the code it was supposed to scan.
            let body = &after[ATTR.len()..];
            let lead = body.len() - body.trim_start().len();
            let trimmed = body.trim_start();
            let vis = ["pub(crate) ", "pub(super) ", "pub "]
                .into_iter()
                .find(|v| trimmed.starts_with(v))
                .map_or(0, str::len);
            let Some(mod_at) = trimmed[vis..]
                .starts_with("mod ")
                .then_some(ATTR.len() + lead + vis)
            else {
                // Item-level attribute — step past it and keep the text.
                out.push_str(&after[..ATTR.len()]);
                rest = &after[ATTR.len()..];
                continue;
            };
            let tail = &after[mod_at..];
            let brace = tail.find('{');
            let semi = tail.find(';');
            match (brace, semi) {
                // `mod foo;` — no body here to strip.
                (Some(b), Some(sc)) if sc < b => {
                    out.push_str(&after[..mod_at + sc + 1]);
                    rest = &after[mod_at + sc + 1..];
                }
                (None, Some(sc)) => {
                    out.push_str(&after[..mod_at + sc + 1]);
                    rest = &after[mod_at + sc + 1..];
                }
                (Some(b), _) => {
                    let mut depth = 0usize;
                    let bytes = tail.as_bytes();
                    let mut i = b;
                    while i < bytes.len() {
                        match bytes[i] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    // `i` lands on the closing brace, or on `bytes.len()`
                    // for an unbalanced tail; clamp either way.
                    let resume = (mod_at + i + 1).min(after.len());
                    rest = &after[resume..];
                }
                (None, None) => {
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// The eleven-field commit context must be built in exactly one place.
    ///
    /// Before #3863 it was spelled out at fourteen call sites — about 154
    /// lines of borrow plumbing — so a twelfth pending-command queue meant
    /// fourteen edits, and a site that missed one became a silent divergence
    /// the moment that field gained a default. The macro is the single edit
    /// point; this keeps it that way.
    ///
    /// Needles are composed at runtime so this test's own source cannot be
    /// what the scan finds.
    #[test]
    fn the_commit_context_is_constructed_only_by_the_macro() {
        let src = production_text(EXTENSIONS_RS);
        let literal = format!("{}{}", "DeliveryCommitContext ", "{");
        let occurrences = src.match_indices(&literal).count();
        assert_eq!(
            occurrences, 2,
            "expected exactly two `{literal}` in production — the macro body              and `apply_delivery_result`'s destructuring — found {occurrences}.              A hand-written literal is back; use `delivery_commit_context!` so              a new pending-command queue stays a one-line change."
        );
    }

    /// Guest-entry snapshot loading is centralised in `enter_guest`.
    ///
    /// The prologue (clone principal, load storage snapshot, load legacy
    /// containers) was repeated verbatim at ten dispatch paths. A site that
    /// loads one snapshot but not the other hands the guest a half-populated
    /// view, invisible until a guest reads the missing half.
    ///
    /// The legacy-container setter legitimately has one extra production
    /// caller: instantiation seeds a newly created instance from the
    /// *staged* registry built during load, before any principal storage
    /// exists to pair with. That asymmetry is real and is pinned here rather
    /// than papered over — the audit's "perfectly symmetric 10/10" counted
    /// only the dispatch prologue.
    #[test]
    fn guest_entry_snapshots_are_loaded_only_by_enter_guest() {
        let src = production_text(EXTENSIONS_RS);
        let storage = format!("{}{}", "set_principal_storage_", "snapshot(");
        let legacy = format!("{}{}", "set_legacy_container_", "snapshot(");
        assert_eq!(
            src.match_indices(&storage).count(),
            1,
            "`{storage}` must be called only from `enter_guest`"
        );
        assert_eq!(
            src.match_indices(&legacy).count(),
            2,
            "`{legacy}` must be called only from `enter_guest` and the              instantiation seed in `load`"
        );
        assert!(
            src.contains("fn enter_guest("),
            "enter_guest must exist — the dispatch prologue has no other home"
        );
    }

    /// The concat list above must name every production file here.
    ///
    /// A new region added to the split is invisible to the guards until it
    /// joins `SOURCES`, and the failure is silent in the dangerous
    /// direction: counts go *down*, and an equality assertion on a count
    /// that should be 2 fails loudly — but one that should be 0 would not.
    #[test]
    fn covers_every_production_file_in_the_directory() {
        // Assembled at run time so `SOURCES`'s own literal list, which is
        // part of this file and therefore part of `SOURCES`, is not what
        // the scan matches on.
        let open = format!("{}{}", "include_", "str!(\"");
        let listed: std::collections::BTreeSet<String> = SOURCES_LIST
            .match_indices(open.as_str())
            .map(|(at, _)| {
                let rest = &SOURCES_LIST[at + open.len()..];
                rest[..rest.find('"').expect("unterminated include_str!")].to_string()
            })
            .collect();

        let mut on_disk: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(env!("CARGO_MANIFEST_DIR"))
            .map(|_| std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src/extensions")))
            .expect("manifest dir readable")
            .expect("the extensions directory must exist")
        {
            let name = entry.expect("readable entry").file_name();
            let name = name.to_string_lossy().to_string();
            if name.ends_with(".rs") && name != "tests.rs" {
                on_disk.insert(name);
            }
        }

        assert_eq!(
            listed, on_disk,
            "`SOURCES` and the files in src/extensions/ disagree. Add the new module to \
             the concat list (production) or name it tests.rs (test-only) — until then \
             the source-shape guards below do not see it (#3843).",
        );
    }
}
