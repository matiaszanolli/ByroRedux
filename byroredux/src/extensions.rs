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
use byroredux_sdk::actor_values::{
    ActorValueCommand, ActorValueOperation, ActorValueState, MAX_ACTOR_VALUES_PER_ENTITY,
};
use byroredux_sdk::animation::{AnimationEvent, AnimationSnapshot, PlayIdleCommand};
use byroredux_sdk::component::{
    ComponentSchema, ComponentStoreError, ComponentStoreLimits, ExtensionComponentStore,
    ExtensionStateSnapshot, ExtensionValue, PersistedComponentRow, RestoredComponentRow,
    EXTENSION_STATE_FORMAT_VERSION,
};
use byroredux_sdk::console::ConsoleCommandResult;
use byroredux_sdk::content::ContentCatalog;
use byroredux_sdk::event::{
    custom_event_owned_by, is_custom_event_id, ActivationEvent, CellLoadEvent, CustomEvent,
    EquipmentEvent, HitEvent, InputAction as SdkInputAction, InputActionEvent, InputPhase,
    SessionEvent, SessionPhase, UpdateEvent,
};
use byroredux_sdk::factions::{FactionMembership, FactionSnapshot, MAX_FACTIONS_PER_ENTITY};
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EntityRef, ExtensionId, FormRef, PrincipalId,
};
use byroredux_sdk::inventory::{
    InventoryEntry, InventorySnapshot, MAX_INVENTORY_ENTRIES_PER_ENTITY,
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
use byroredux_sdk::service::{
    ACTIVATE_EVENT, CELL_LOAD_EVENT, CONSOLE_REGISTER_CAPABILITY, EQUIPMENT_EVENT,
    EVENTS_SUBSCRIBE_CAPABILITY, HIT_EVENT, INPUT_ACTIONS_SUBSCRIBE_CAPABILITY, INPUT_ACTION_EVENT,
    SESSION_EVENT, SETTINGS_REGISTER_CAPABILITY, UPDATE_EVENT,
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
    #[error("extension {0} is already installed")]
    AlreadyInstalled(ExtensionId),
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
}

impl ExtensionHost {
    pub fn new(
        sandbox_config: SandboxConfig,
        state_limits: ComponentStoreLimits,
    ) -> Result<Self, ExtensionHostError> {
        if state_limits.max_commands_per_batch < sandbox_config.max_commands_per_entry {
            return Err(ExtensionHostError::IncompatibleCommandBudgets);
        }
        Ok(Self {
            runtime: SandboxRuntime::new(sandbox_config)?,
            state: ExtensionComponentStore::new(state_limits)?,
            principal_storage: PrincipalStorageStore::new(PrincipalStorageLimits::default())?,
            handles: EntityHandleRegistry::new(),
            components: Vec::new(),
            diagnostics: Vec::new(),
            retained_rows: Vec::new(),
            retained_storage: Vec::new(),
            pending_custom_events: Vec::new(),
            pending_setting_writes: Vec::new(),
            pending_actor_value_writes: Vec::new(),
            pending_package_evaluations: Vec::new(),
            pending_animation_commands: Vec::new(),
            pending_reputation_writes: Vec::new(),
            content_catalog: Arc::new(ContentCatalog::default()),
            faction_relationships: Arc::new(FactionRelationshipCatalog::default()),
            engine_settings: Arc::new(SettingsSnapshot::default()),
            console_commands: Vec::new(),
        })
    }

    /// Compile and initialize every component of one already-resolved package.
    ///
    /// The caller supplies effective grants explicitly; manifest requests are
    /// never promoted into authority here. Schema registration and component
    /// publication commit only after every artifact compiles, instantiates,
    /// and initializes successfully.
    pub fn install_package(
        &mut self,
        manifest: &ExtensionManifest,
        artifacts: &ExtensionArtifacts,
        grants: CapabilitySet,
    ) -> Result<(), ExtensionHostError> {
        if self
            .components
            .iter()
            .any(|component| component.extension == manifest.id)
        {
            return Err(ExtensionHostError::AlreadyInstalled(manifest.id.clone()));
        }

        let principal = PrincipalId::from(&manifest.id);
        let mut staged_state = self.state.clone();
        let mut staged_principal_storage = self.principal_storage.clone();
        let mut staged_retained_storage = self.retained_storage.clone();
        for declaration in &manifest.component_schemas {
            staged_state.register_schema(
                &principal,
                ComponentSchema {
                    id: declaration.id.clone(),
                    version: declaration.version,
                    fields: declaration.fields.clone(),
                },
            )?;
        }
        if let Some(version) = manifest.principal_storage_schema {
            staged_principal_storage.register_schema(principal.clone(), version)?;
            if let Some(index) = staged_retained_storage
                .iter()
                .position(|record| record.principal == principal)
            {
                let retained = staged_retained_storage.remove(index);
                let mut active = staged_principal_storage.persisted();
                active.push(retained);
                staged_principal_storage.replace_active(active)?;
            }
        } else if staged_retained_storage
            .iter()
            .any(|record| record.principal == principal)
        {
            return Err(PrincipalStorageError::UndeclaredPrincipal(principal).into());
        }

        let mut compiled = Vec::with_capacity(manifest.components.len());
        for declaration in &manifest.components {
            let bytes = artifacts.get(&declaration.id).ok_or_else(|| {
                ExtensionHostError::MissingArtifact {
                    extension: manifest.id.clone(),
                    component: declaration.id.clone(),
                }
            })?;
            compiled.push((
                declaration.id.clone(),
                self.runtime.compile(manifest, &declaration.id, bytes)?,
            ));
        }

        let receives_activate = manifest
            .subscriptions
            .iter()
            .any(|subscription| subscription.event.as_str() == ACTIVATE_EVENT);
        let receives_cell_load = manifest
            .subscriptions
            .iter()
            .any(|subscription| subscription.event.as_str() == CELL_LOAD_EVENT);
        let receives_hit = manifest
            .subscriptions
            .iter()
            .any(|subscription| subscription.event.as_str() == HIT_EVENT);
        let receives_equipment = manifest
            .subscriptions
            .iter()
            .any(|subscription| subscription.event.as_str() == EQUIPMENT_EVENT);
        let input_subscription = manifest
            .subscriptions
            .iter()
            .find(|subscription| subscription.event.as_str() == INPUT_ACTION_EVENT);
        let receives_input = input_subscription.is_some();
        let input_actions = input_subscription
            .into_iter()
            .flat_map(|subscription| &subscription.filters)
            .filter_map(|filter| SdkInputAction::parse(&filter.equals))
            .collect::<BTreeSet<_>>();
        let session_subscription = manifest
            .subscriptions
            .iter()
            .find(|subscription| subscription.event.as_str() == SESSION_EVENT);
        let receives_session = session_subscription.is_some();
        let session_phases = session_subscription
            .into_iter()
            .flat_map(|subscription| &subscription.filters)
            .filter_map(|filter| SessionPhase::parse(&filter.equals))
            .collect::<BTreeSet<_>>();
        let recurring_update = manifest
            .subscriptions
            .iter()
            .find(|subscription| subscription.event.as_str() == UPDATE_EVENT)
            .and_then(|subscription| subscription.interval_millis)
            .map(RecurringCadence::from_millis);
        let custom_subscriptions = manifest
            .subscriptions
            .iter()
            .filter(|subscription| is_custom_event_id(&subscription.event))
            .map(|subscription| subscription.event.clone())
            .collect::<BTreeSet<_>>();
        let mut staged_components = Vec::with_capacity(compiled.len());
        let mut staged_diagnostics = Vec::new();
        for (component_id, compiled) in compiled {
            let mut instance = self
                .runtime
                .instantiate(&compiled, manifest, grants.clone())?;
            instance.set_content_catalog_snapshot(Arc::clone(&self.content_catalog));
            instance.set_faction_relationships_snapshot(Arc::clone(&self.faction_relationships));
            instance.set_engine_settings_snapshot(Arc::clone(&self.engine_settings));
            instance.initialize()?;
            staged_diagnostics.extend(instance.take_logs().into_iter().map(|entry| {
                ExtensionDiagnostic::Log {
                    extension: manifest.id.clone(),
                    component: component_id.clone(),
                    entry,
                }
            }));
            staged_components.push(HostedComponent {
                extension: manifest.id.clone(),
                component: component_id,
                receives_activate,
                receives_cell_load,
                receives_equipment,
                receives_input,
                input_actions: input_actions.clone(),
                receives_session,
                session_phases: session_phases.clone(),
                custom_subscriptions: custom_subscriptions.clone(),
                receives_hit,
                recurring_update,
                instance,
            });
        }

        self.state = staged_state;
        self.principal_storage = staged_principal_storage;
        self.retained_storage = staged_retained_storage;
        self.components.extend(staged_components);
        if grants.contains(CONSOLE_REGISTER_CAPABILITY) {
            self.console_commands
                .extend(
                    manifest
                        .console_commands
                        .iter()
                        .enumerate()
                        .map(|(index, command)| HostedConsoleCommand {
                            name: command.qualified_name(&manifest.id),
                            description: command.description.clone(),
                            extension: manifest.id.clone(),
                            component: command.component.clone(),
                            declaration_index: u32::try_from(index)
                                .expect("manifest console command count is bounded below u32::MAX"),
                        }),
                );
        }
        self.diagnostics.extend(staged_diagnostics);
        Ok(())
    }

    fn console_commands(&self) -> &[HostedConsoleCommand] {
        &self.console_commands
    }

    fn invoke_console_command(
        &mut self,
        route: &HostedConsoleCommand,
        args: &str,
    ) -> ConsoleCommandResult {
        let Some(index) = self.components.iter().position(|hosted| {
            hosted.extension == route.extension && hosted.component == route.component
        }) else {
            return ConsoleCommandResult {
                success: false,
                lines: vec!["declared command component is unavailable".to_owned()],
            };
        };
        let hosted = &mut self.components[index];
        if hosted.instance.status() != &InstanceStatus::Active {
            return ConsoleCommandResult {
                success: false,
                lines: vec![format!("component is {}", hosted.instance.status())],
            };
        }
        let principal = hosted.instance.principal().id().clone();
        let storage_snapshot = self
            .principal_storage
            .values(&principal)
            .cloned()
            .unwrap_or_default();
        hosted
            .instance
            .set_principal_storage_snapshot(storage_snapshot);
        let result = hosted
            .instance
            .on_console_command(route.declaration_index, args);
        self.diagnostics
            .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                ExtensionDiagnostic::Log {
                    extension: hosted.extension.clone(),
                    component: hosted.component.clone(),
                    entry,
                }
            }));
        let (output, commands) = match result {
            Ok(result) => result,
            Err(error) => {
                self.diagnostics.push(ExtensionDiagnostic::Fault {
                    extension: hosted.extension.clone(),
                    component: hosted.component.clone(),
                    message: error.to_string(),
                });
                return ConsoleCommandResult {
                    success: false,
                    lines: vec![error.to_string()],
                };
            }
        };
        let mut stats = ExtensionDispatchStats::default();
        apply_delivery_result(
            hosted,
            Ok(commands),
            LifecyclePhase::ConsoleCommand,
            &principal,
            DeliveryCommitContext {
                state: &mut self.state,
                principal_storage: &mut self.principal_storage,
                pending_custom_events: &mut self.pending_custom_events,
                pending_setting_writes: &mut self.pending_setting_writes,
                pending_actor_value_writes: &mut self.pending_actor_value_writes,
                pending_package_evaluations: &mut self.pending_package_evaluations,
                pending_animation_commands: &mut self.pending_animation_commands,
                pending_reputation_writes: &mut self.pending_reputation_writes,
                diagnostics: &mut self.diagnostics,
                stats: &mut stats,
            },
        );
        if stats.faults == 0 {
            output
        } else {
            ConsoleCommandResult {
                success: false,
                lines: vec!["deferred command batch was rejected".to_owned()],
            }
        }
    }

    fn set_content_catalog(&mut self, catalog: Arc<ContentCatalog>) {
        if Arc::ptr_eq(&self.content_catalog, &catalog) || *self.content_catalog == *catalog {
            return;
        }
        self.content_catalog = Arc::clone(&catalog);
        for hosted in &mut self.components {
            hosted
                .instance
                .set_content_catalog_snapshot(Arc::clone(&catalog));
        }
    }

    fn set_faction_relationships(&mut self, relationships: Arc<FactionRelationshipCatalog>) {
        if Arc::ptr_eq(&self.faction_relationships, &relationships)
            || *self.faction_relationships == *relationships
        {
            return;
        }
        self.faction_relationships = Arc::clone(&relationships);
        for hosted in &mut self.components {
            hosted
                .instance
                .set_faction_relationships_snapshot(Arc::clone(&relationships));
        }
    }

    fn set_engine_settings(&mut self, settings: Arc<SettingsSnapshot>) {
        if *self.engine_settings == *settings {
            return;
        }
        self.engine_settings = Arc::clone(&settings);
        for hosted in &mut self.components {
            hosted
                .instance
                .set_engine_settings_snapshot(Arc::clone(&settings));
        }
    }

    fn set_spatial_snapshot(&mut self, snapshot: Arc<SpatialSnapshot>) {
        for hosted in &mut self.components {
            hosted.instance.set_spatial_snapshot(Arc::clone(&snapshot));
        }
    }

    fn catalog(&self) -> &byroredux_sdk::service::ServiceCatalog {
        self.runtime.catalog()
    }

    fn max_component_bytes(&self) -> usize {
        self.runtime.config().max_component_bytes
    }

    fn bind_entity(
        &mut self,
        entity: EntityId,
        form: Option<FormRef>,
    ) -> Result<EntityRef, ExtensionHostError> {
        let mut staged_handles = self.handles.clone();
        let handle = staged_handles.handle_for(entity)?;
        let Some(form) = form else {
            self.handles = staged_handles;
            return Ok(handle);
        };

        let mut rebound = Vec::new();
        let mut retained = Vec::with_capacity(self.retained_rows.len());
        for row in &self.retained_rows {
            if row.entity == form && self.state.schema(&row.principal, &row.schema).is_some() {
                rebound.push(RestoredComponentRow {
                    principal: row.principal.clone(),
                    schema: row.schema.clone(),
                    schema_version: row.schema_version,
                    entity: handle,
                    row: row.row.clone(),
                });
            } else {
                retained.push(row.clone());
            }
        }
        let mut staged_state = self.state.clone();
        staged_state.merge_rows(rebound)?;
        self.handles = staged_handles;
        self.state = staged_state;
        self.retained_rows = retained;
        Ok(handle)
    }

    fn bind_optional_entity(
        &mut self,
        entity: Option<EntityId>,
        form: Option<FormRef>,
    ) -> Result<Option<EntityRef>, ExtensionHostError> {
        entity
            .map(|entity| self.bind_entity(entity, form))
            .transpose()
    }

    /// Deliver already-snapshotted engine activations in deterministic order.
    #[cfg(test)]
    pub fn dispatch_activations(
        &mut self,
        activations: impl IntoIterator<Item = RawActivation>,
    ) -> ExtensionDispatchStats {
        self.dispatch_activations_with_projections(activations, &BTreeMap::new())
    }

    fn dispatch_activations_with_projections(
        &mut self,
        activations: impl IntoIterator<Item = RawActivation>,
        raw_projections: &BTreeMap<EntityId, RawEntityProjection>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for activation in activations {
            stats.events += 1;
            let subject = match self.bind_entity(activation.subject, activation.subject_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let activator = match activation.activator {
                Some(entity) => match self.bind_entity(entity, activation.activator_form) {
                    Ok(handle) => Some(handle),
                    Err(error) => {
                        self.record_host_fault(error.to_string());
                        stats.faults += 1;
                        continue;
                    }
                },
                None => None,
            };
            let mut entity_projections = vec![entity_projection(
                subject,
                activation.subject_form,
                raw_projections.get(&activation.subject),
            )];
            if let (Some(entity), Some(handle)) = (activation.activator, activator) {
                entity_projections.push(entity_projection(
                    handle,
                    activation.activator_form,
                    raw_projections.get(&entity),
                ));
            }

            for hosted in &mut self.components {
                if !hosted.receives_activate
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                hosted
                    .instance
                    .set_entity_projections(entity_projections.clone());
                let result = hosted
                    .instance
                    .on_activate(ActivationEvent { subject, activator });
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));

                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::Activate,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    /// Deliver already-snapshotted cell-load markers in deterministic order.
    #[cfg(test)]
    pub fn dispatch_cell_loads(
        &mut self,
        cell_loads: impl IntoIterator<Item = RawCellLoad>,
    ) -> ExtensionDispatchStats {
        self.dispatch_cell_loads_with_projections(cell_loads, &BTreeMap::new())
    }

    fn dispatch_cell_loads_with_projections(
        &mut self,
        cell_loads: impl IntoIterator<Item = RawCellLoad>,
        raw_projections: &BTreeMap<EntityId, RawEntityProjection>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for cell_load in cell_loads {
            stats.events += 1;
            let subject = match self.bind_entity(cell_load.subject, cell_load.subject_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let entity_projections = vec![entity_projection(
                subject,
                cell_load.subject_form,
                raw_projections.get(&cell_load.subject),
            )];

            for hosted in &mut self.components {
                if !hosted.receives_cell_load
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                hosted
                    .instance
                    .set_entity_projections(entity_projections.clone());
                let result = hosted.instance.on_cell_load(CellLoadEvent { subject });
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::CellLoad,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    /// Deliver already-snapshotted equipment changes in mutation order.
    #[cfg(test)]
    pub fn dispatch_equipment_changes(
        &mut self,
        changes: impl IntoIterator<Item = RawEquipmentChange>,
    ) -> ExtensionDispatchStats {
        self.dispatch_equipment_changes_with_projections(changes, &BTreeMap::new())
    }

    fn dispatch_equipment_changes_with_projections(
        &mut self,
        changes: impl IntoIterator<Item = RawEquipmentChange>,
        raw_projections: &BTreeMap<EntityId, RawEntityProjection>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for change in changes {
            stats.events += 1;
            let wearer = match self.bind_entity(change.wearer, change.wearer_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let entity_projections = vec![entity_projection(
                wearer,
                change.wearer_form,
                raw_projections.get(&change.wearer),
            )];

            for hosted in &mut self.components {
                if !hosted.receives_equipment
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                hosted
                    .instance
                    .set_entity_projections(entity_projections.clone());
                let result = hosted.instance.on_equipment_change(EquipmentEvent {
                    wearer,
                    item: change.item,
                    equipped: change.equipped,
                });
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::Equipment,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    #[cfg(test)]
    pub fn dispatch_input_actions(
        &mut self,
        events: impl IntoIterator<Item = InputActionEvent>,
    ) -> ExtensionDispatchStats {
        self.dispatch_input_actions_inner(events)
    }

    fn dispatch_input_actions_inner(
        &mut self,
        events: impl IntoIterator<Item = InputActionEvent>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for event in events {
            stats.events += 1;
            for hosted in &mut self.components {
                if !hosted.receives_input
                    || (!hosted.input_actions.is_empty()
                        && !hosted.input_actions.contains(&event.action))
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || !hosted
                        .instance
                        .grants()
                        .contains(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                let result = hosted.instance.on_input_action(event);
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::Input,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    #[cfg(test)]
    pub fn dispatch_session_events(
        &mut self,
        events: impl IntoIterator<Item = SessionEvent>,
    ) -> ExtensionDispatchStats {
        self.dispatch_session_events_inner(events)
    }

    fn dispatch_session_events_inner(
        &mut self,
        events: impl IntoIterator<Item = SessionEvent>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for event in events {
            stats.events += 1;
            for hosted in &mut self.components {
                if !hosted.receives_session
                    || (!hosted.session_phases.is_empty()
                        && !hosted.session_phases.contains(&event.phase))
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                let result = hosted.instance.on_session_event(event);
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::Session,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    /// Deliver custom events committed by earlier scheduler passes.
    ///
    /// The queue is taken before entering any guest, so events published from
    /// these callbacks remain pending and cannot cause nested guest execution.
    #[cfg(test)]
    pub fn dispatch_custom_events(&mut self) -> ExtensionDispatchStats {
        self.dispatch_pending_custom_events()
    }

    fn dispatch_pending_custom_events(&mut self) -> ExtensionDispatchStats {
        let events = std::mem::take(&mut self.pending_custom_events);
        let mut stats = ExtensionDispatchStats::default();
        for event in events {
            stats.events += 1;
            for hosted in &mut self.components {
                if !hosted.custom_subscriptions.contains(&event.event)
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                let result = hosted.instance.on_custom_event(event.clone());
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::CustomEvent,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    /// Deliver already-snapshotted combat hit markers in deterministic order.
    #[cfg(test)]
    pub fn dispatch_hits(
        &mut self,
        hits: impl IntoIterator<Item = RawHit>,
    ) -> ExtensionDispatchStats {
        self.dispatch_hits_with_projections(hits, &BTreeMap::new())
    }

    fn dispatch_hits_with_projections(
        &mut self,
        hits: impl IntoIterator<Item = RawHit>,
        raw_projections: &BTreeMap<EntityId, RawEntityProjection>,
    ) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        for hit in hits {
            stats.events += 1;
            if !hit.damage.is_finite() || hit.damage < 0.0 {
                self.record_host_fault(
                    ExtensionHostError::InvalidHitDamage(hit.damage).to_string(),
                );
                stats.faults += 1;
                continue;
            }
            let subject = match self.bind_entity(hit.subject, hit.subject_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let aggressor = match self.bind_optional_entity(hit.aggressor, hit.aggressor_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let source = match self.bind_optional_entity(hit.source, hit.source_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let projectile = match self.bind_optional_entity(hit.projectile, hit.projectile_form) {
                Ok(handle) => handle,
                Err(error) => {
                    self.record_host_fault(error.to_string());
                    stats.faults += 1;
                    continue;
                }
            };
            let entities = [
                Some((hit.subject, subject, hit.subject_form)),
                hit.aggressor
                    .zip(aggressor)
                    .map(|(entity, handle)| (entity, handle, hit.aggressor_form)),
                hit.source
                    .zip(source)
                    .map(|(entity, handle)| (entity, handle, hit.source_form)),
                hit.projectile
                    .zip(projectile)
                    .map(|(entity, handle)| (entity, handle, hit.projectile_form)),
            ];
            let entity_projections = entities
                .into_iter()
                .flatten()
                .map(|(entity, handle, form)| {
                    entity_projection(handle, form, raw_projections.get(&entity))
                })
                .collect::<Vec<_>>();

            for hosted in &mut self.components {
                if !hosted.receives_hit
                    || !hosted
                        .instance
                        .grants()
                        .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                    || hosted.instance.status() != &InstanceStatus::Active
                {
                    continue;
                }
                stats.deliveries += 1;
                let principal = hosted.instance.principal().id().clone();
                let storage_snapshot = self
                    .principal_storage
                    .values(&principal)
                    .cloned()
                    .unwrap_or_default();
                hosted
                    .instance
                    .set_principal_storage_snapshot(storage_snapshot);
                hosted
                    .instance
                    .set_entity_projections(entity_projections.clone());
                let result = hosted.instance.on_hit(HitEvent {
                    subject,
                    aggressor,
                    source,
                    projectile,
                    damage: hit.damage,
                    power_attack: hit.power_attack,
                    sneak_attack: hit.sneak_attack,
                    bash_attack: hit.bash_attack,
                    blocked: hit.blocked,
                });
                self.diagnostics
                    .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                        ExtensionDiagnostic::Log {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            entry,
                        }
                    }));
                apply_delivery_result(
                    hosted,
                    result,
                    LifecyclePhase::Hit,
                    &principal,
                    DeliveryCommitContext {
                        state: &mut self.state,
                        principal_storage: &mut self.principal_storage,
                        pending_custom_events: &mut self.pending_custom_events,
                        pending_setting_writes: &mut self.pending_setting_writes,
                        pending_actor_value_writes: &mut self.pending_actor_value_writes,
                        pending_package_evaluations: &mut self.pending_package_evaluations,
                        pending_animation_commands: &mut self.pending_animation_commands,
                        pending_reputation_writes: &mut self.pending_reputation_writes,
                        diagnostics: &mut self.diagnostics,
                        stats: &mut stats,
                    },
                );
            }
        }
        stats
    }

    /// Advance engine-owned recurring schedules and deliver callbacks that
    /// became due. Overshoot is retained while each component receives at
    /// most one callback per frame.
    #[cfg(test)]
    pub fn dispatch_updates(&mut self, dt: f32) -> ExtensionDispatchStats {
        self.dispatch_recurring_updates(dt)
    }

    fn dispatch_recurring_updates(&mut self, dt: f32) -> ExtensionDispatchStats {
        let mut stats = ExtensionDispatchStats::default();
        if !dt.is_finite() || dt < 0.0 {
            self.record_host_fault(format!(
                "recurring update delta must be finite and non-negative, got {dt}"
            ));
            stats.faults = 1;
            return stats;
        }
        for hosted in &mut self.components {
            if !hosted
                .instance
                .grants()
                .contains(EVENTS_SUBSCRIBE_CAPABILITY)
                || hosted.instance.status() != &InstanceStatus::Active
            {
                continue;
            }
            let Some(elapsed_seconds) = hosted
                .recurring_update
                .as_mut()
                .and_then(|cadence| cadence.advance(dt))
            else {
                continue;
            };
            stats.events += 1;
            stats.deliveries += 1;
            let principal = hosted.instance.principal().id().clone();
            let storage_snapshot = self
                .principal_storage
                .values(&principal)
                .cloned()
                .unwrap_or_default();
            hosted
                .instance
                .set_principal_storage_snapshot(storage_snapshot);
            let result = hosted.instance.on_update(UpdateEvent { elapsed_seconds });
            self.diagnostics
                .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                    ExtensionDiagnostic::Log {
                        extension: hosted.extension.clone(),
                        component: hosted.component.clone(),
                        entry,
                    }
                }));
            apply_delivery_result(
                hosted,
                result,
                LifecyclePhase::Update,
                &principal,
                DeliveryCommitContext {
                    state: &mut self.state,
                    principal_storage: &mut self.principal_storage,
                    pending_custom_events: &mut self.pending_custom_events,
                    pending_setting_writes: &mut self.pending_setting_writes,
                    pending_actor_value_writes: &mut self.pending_actor_value_writes,
                    pending_package_evaluations: &mut self.pending_package_evaluations,
                    pending_animation_commands: &mut self.pending_animation_commands,
                    pending_reputation_writes: &mut self.pending_reputation_writes,
                    diagnostics: &mut self.diagnostics,
                    stats: &mut stats,
                },
            );
        }
        stats
    }

    fn record_host_fault(&mut self, message: String) {
        self.diagnostics.push(ExtensionDiagnostic::Fault {
            extension: ExtensionId::new("byro.engine")
                .expect("the engine diagnostic principal is valid"),
            component: ComponentId::new("extension-host")
                .expect("the engine diagnostic component is valid"),
            message,
        });
    }

    #[cfg(test)]
    pub fn state(&self) -> &ExtensionComponentStore {
        &self.state
    }

    pub fn package_count(&self) -> usize {
        self.components
            .iter()
            .map(|component| &component.extension)
            .collect::<BTreeSet<_>>()
            .len()
    }

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    fn active_principals(&self) -> BTreeSet<PrincipalId> {
        self.components
            .iter()
            .map(|component| PrincipalId::from(&component.extension))
            .collect()
    }

    fn validate_saved_state(
        &self,
        saved: &ExtensionStateSnapshot,
    ) -> Result<(), ExtensionHostError> {
        if saved.format_version != EXTENSION_STATE_FORMAT_VERSION {
            return Err(ExtensionHostError::UnsupportedStateFormat {
                actual: saved.format_version,
                expected: EXTENSION_STATE_FORMAT_VERSION,
            });
        }
        if saved.rows.len() > MAX_PERSISTED_EXTENSION_ROWS {
            return Err(ExtensionHostError::PersistedRowBudgetExceeded {
                actual: saved.rows.len(),
                maximum: MAX_PERSISTED_EXTENSION_ROWS,
            });
        }

        let active = self.active_principals();
        let mut stable_keys = BTreeSet::new();
        let mut rows_per_principal = BTreeMap::<PrincipalId, usize>::new();
        let mut rebound = Vec::new();
        for (index, row) in saved.rows.iter().enumerate() {
            if !stable_keys.insert((row.principal.clone(), row.schema.clone(), row.entity)) {
                return Err(ExtensionHostError::DuplicatePersistedRow {
                    principal: row.principal.clone(),
                    schema: row.schema.clone(),
                    entity: row.entity,
                });
            }
            let principal_rows = rows_per_principal.entry(row.principal.clone()).or_default();
            *principal_rows += 1;
            if *principal_rows > self.state.limits().max_rows_per_principal {
                return Err(ComponentStoreError::RowBudgetExceeded {
                    principal: row.principal.clone(),
                    maximum: self.state.limits().max_rows_per_principal,
                }
                .into());
            }
            if row.row.iter().count() > self.state.limits().max_fields_per_schema {
                return Err(ComponentStoreError::TooManyFields {
                    schema: row.schema.clone(),
                    maximum: self.state.limits().max_fields_per_schema,
                }
                .into());
            }
            for (field, value) in row.row.iter() {
                let too_large = match value {
                    ExtensionValue::String(value) => (value.len()
                        > self.state.limits().max_string_bytes)
                        .then_some(self.state.limits().max_string_bytes),
                    ExtensionValue::Bytes(value) => (value.len()
                        > self.state.limits().max_blob_bytes)
                        .then_some(self.state.limits().max_blob_bytes),
                    _ => None,
                };
                if let Some(maximum) = too_large {
                    return Err(ComponentStoreError::ValueTooLarge {
                        field: field.clone(),
                        maximum,
                    }
                    .into());
                }
            }
            if !active.contains(&row.principal) {
                continue;
            }
            let object = u64::try_from(index)
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ExtensionHostError::HandleSpaceExhausted)?;
            let entity =
                EntityRef::new(1, object).ok_or(ExtensionHostError::HandleSpaceExhausted)?;
            rebound.push(RestoredComponentRow {
                principal: row.principal.clone(),
                schema: row.schema.clone(),
                schema_version: row.schema_version,
                entity,
                row: row.row.clone(),
            });
        }
        let mut staged = self.state.clone();
        staged.replace_rows(rebound)?;

        let mut storage_principals = BTreeSet::new();
        let mut active_storage = Vec::new();
        for record in &saved.principal_storage {
            if !storage_principals.insert(record.principal.clone()) {
                return Err(PrincipalStorageError::DuplicatePersistedPrincipal(
                    record.principal.clone(),
                )
                .into());
            }
            self.principal_storage.validate_record_bounds(record)?;
            if active.contains(&record.principal) {
                active_storage.push(record.clone());
            }
        }
        let mut staged_storage = self.principal_storage.clone();
        staged_storage.replace_active(active_storage)?;
        Ok(())
    }

    fn capture_saved_state(
        &self,
        forms_by_entity: &BTreeMap<EntityId, FormRef>,
    ) -> Result<ExtensionStateSnapshot, ExtensionHostError> {
        let mut rows = BTreeMap::new();
        for row in &self.retained_rows {
            rows.insert(
                (row.principal.clone(), row.schema.clone(), row.entity),
                row.clone(),
            );
        }
        let mut unpersistable = 0;
        for (principal, schema, handle, row) in self.state.rows() {
            let Some(entity) = self.handles.by_handle.get(&handle) else {
                unpersistable += 1;
                continue;
            };
            let Some(form) = forms_by_entity.get(entity) else {
                unpersistable += 1;
                continue;
            };
            let declaration = self
                .state
                .schema(principal, schema)
                .expect("materialized extension row always has a registered schema");
            let saved = PersistedComponentRow {
                principal: principal.clone(),
                schema: schema.clone(),
                schema_version: declaration.version,
                entity: *form,
                row: row.clone(),
            };
            rows.insert((principal.clone(), schema.clone(), *form), saved);
        }
        if unpersistable != 0 {
            return Err(ExtensionHostError::UnpersistableRows {
                count: unpersistable,
            });
        }
        let mut principal_storage = BTreeMap::new();
        for record in &self.retained_storage {
            principal_storage.insert(record.principal.clone(), record.clone());
        }
        for record in self.principal_storage.persisted() {
            principal_storage.insert(record.principal.clone(), record);
        }
        let saved = ExtensionStateSnapshot {
            format_version: EXTENSION_STATE_FORMAT_VERSION,
            rows: rows.into_values().collect(),
            principal_storage: principal_storage.into_values().collect(),
        };
        self.validate_saved_state(&saved)?;
        Ok(saved)
    }

    fn restore_saved_state(
        &mut self,
        saved: &ExtensionStateSnapshot,
        entities_by_form: &BTreeMap<FormRef, EntityId>,
    ) -> Result<u64, ExtensionHostError> {
        self.validate_saved_state(saved)?;
        let active = self.active_principals();
        let mut staged_handles = self.handles.clone();
        let generation = staged_handles.begin_world_generation()?;
        let mut rebound = Vec::new();
        let mut retained = Vec::new();
        for row in &saved.rows {
            if !active.contains(&row.principal)
                || self.state.schema(&row.principal, &row.schema).is_none()
            {
                retained.push(row.clone());
                continue;
            }
            let Some(entity) = entities_by_form.get(&row.entity) else {
                retained.push(row.clone());
                continue;
            };
            rebound.push(RestoredComponentRow {
                principal: row.principal.clone(),
                schema: row.schema.clone(),
                schema_version: row.schema_version,
                entity: staged_handles.handle_for(*entity)?,
                row: row.row.clone(),
            });
        }
        let mut staged_state = self.state.clone();
        staged_state.replace_rows(rebound)?;
        let mut active_storage = Vec::new();
        let mut retained_storage = Vec::new();
        for record in &saved.principal_storage {
            if active.contains(&record.principal)
                && self
                    .principal_storage
                    .schema_version(&record.principal)
                    .is_some()
            {
                active_storage.push(record.clone());
            } else {
                retained_storage.push(record.clone());
            }
        }
        let mut staged_storage = self.principal_storage.clone();
        staged_storage.replace_active(active_storage)?;
        self.handles = staged_handles;
        self.state = staged_state;
        self.principal_storage = staged_storage;
        self.retained_rows = retained;
        self.retained_storage = retained_storage;
        Ok(generation)
    }

    /// Invalidate every runtime entity handle at a world replacement boundary.
    #[cfg(test)]
    pub fn begin_world_generation(&mut self) -> Result<u64, ExtensionHostError> {
        let generation = self.handles.begin_world_generation()?;
        self.state.clear_rows();
        Ok(generation)
    }

    /// Stop active components in reverse publication order.
    pub fn shutdown_all(&mut self) {
        for hosted in self.components.iter_mut().rev() {
            if hosted.instance.status() != &InstanceStatus::Active {
                continue;
            }
            if let Err(error) = hosted.instance.shutdown() {
                self.diagnostics.push(ExtensionDiagnostic::Fault {
                    extension: hosted.extension.clone(),
                    component: hosted.component.clone(),
                    message: error.to_string(),
                });
            }
            self.diagnostics
                .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                    ExtensionDiagnostic::Log {
                        extension: hosted.extension.clone(),
                        component: hosted.component.clone(),
                        entry,
                    }
                }));
        }
    }

    pub fn take_diagnostics(&mut self) -> Vec<ExtensionDiagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    fn take_resolved_actor_value_writes(&mut self) -> Result<Vec<ResolvedActorValueWrite>, String> {
        let commands = std::mem::take(&mut self.pending_actor_value_writes);
        commands
            .into_iter()
            .map(|command| {
                let entity = self.handles.resolve(command.entity()).ok_or_else(|| {
                    format!(
                        "actor-value command targeted stale entity {:?}",
                        command.entity()
                    )
                })?;
                Ok(ResolvedActorValueWrite {
                    entity,
                    actor_value: command.actor_value(),
                    operation: command.operation(),
                    value: command.value(),
                })
            })
            .collect()
    }

    fn take_resolved_package_evaluations(&mut self) -> Result<Vec<EntityId>, String> {
        let commands = std::mem::take(&mut self.pending_package_evaluations);
        commands
            .into_iter()
            .map(|command| {
                self.handles.resolve(command.entity()).ok_or_else(|| {
                    format!(
                        "package reevaluation targeted stale entity {:?}",
                        command.entity()
                    )
                })
            })
            .collect()
    }

    fn take_resolved_animation_commands(&mut self) -> Result<Vec<ResolvedPlayIdle>, String> {
        let commands = std::mem::take(&mut self.pending_animation_commands);
        commands
            .into_iter()
            .map(|command| {
                let entity = self.handles.resolve(command.entity()).ok_or_else(|| {
                    format!(
                        "animation command targeted stale entity {:?}",
                        command.entity()
                    )
                })?;
                Ok(ResolvedPlayIdle {
                    entity,
                    idle: command.idle(),
                })
            })
            .collect()
    }

    fn take_resolved_reputation_writes(&mut self) -> Result<Vec<ResolvedReputationWrite>, String> {
        let commands = std::mem::take(&mut self.pending_reputation_writes);
        commands
            .into_iter()
            .map(|command| {
                let entity = self.handles.resolve(command.entity()).ok_or_else(|| {
                    format!(
                        "reputation command targeted stale entity {:?}",
                        command.entity()
                    )
                })?;
                Ok(ResolvedReputationWrite {
                    entity,
                    reputation: command.reputation(),
                    operation: command.operation(),
                    points: command.points(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedActorValueWrite {
    entity: EntityId,
    actor_value: FormRef,
    operation: ActorValueOperation,
    value: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPlayIdle {
    entity: EntityId,
    idle: FormRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedReputationWrite {
    entity: EntityId,
    reputation: FormRef,
    operation: ReputationOperation,
    points: u16,
}

struct DeliveryCommitContext<'a> {
    state: &'a mut ExtensionComponentStore,
    principal_storage: &'a mut PrincipalStorageStore,
    pending_custom_events: &'a mut Vec<CustomEvent>,
    pending_setting_writes: &'a mut Vec<byroredux_sdk::settings::SettingWriteCommand>,
    pending_actor_value_writes: &'a mut Vec<ActorValueCommand>,
    pending_package_evaluations: &'a mut Vec<EvaluatePackageCommand>,
    pending_animation_commands: &'a mut Vec<PlayIdleCommand>,
    pending_reputation_writes: &'a mut Vec<ReputationCommand>,
    diagnostics: &'a mut Vec<ExtensionDiagnostic>,
    stats: &'a mut ExtensionDispatchStats,
}

fn apply_delivery_result(
    hosted: &mut HostedComponent,
    result: Result<Vec<HostCommand>, SandboxError>,
    phase: LifecyclePhase,
    principal: &PrincipalId,
    context: DeliveryCommitContext<'_>,
) {
    let DeliveryCommitContext {
        state,
        principal_storage,
        pending_custom_events,
        pending_setting_writes,
        pending_actor_value_writes,
        pending_package_evaluations,
        pending_animation_commands,
        pending_reputation_writes,
        diagnostics,
        stats,
    } = context;
    let commands = match result {
        Ok(commands) => commands,
        Err(error) => {
            diagnostics.push(ExtensionDiagnostic::Fault {
                extension: hosted.extension.clone(),
                component: hosted.component.clone(),
                message: error.to_string(),
            });
            stats.faults += 1;
            return;
        }
    };
    let command_count = commands.len();
    let mut component_commands = Vec::new();
    let mut storage_commands = Vec::new();
    let mut published_events = Vec::new();
    let mut setting_writes = Vec::new();
    let mut actor_value_writes = Vec::new();
    let mut package_evaluations = Vec::new();
    let mut animation_commands = Vec::new();
    let mut reputation_writes = Vec::new();
    for command in commands {
        match command {
            HostCommand::ActorValue(command) => actor_value_writes.push(command),
            HostCommand::Component(command) => component_commands.push(command),
            HostCommand::EvaluatePackage(command) => package_evaluations.push(command),
            HostCommand::PlayIdle(command) => animation_commands.push(command),
            HostCommand::Reputation(command) => reputation_writes.push(command),
            HostCommand::PrincipalStorage(command) => storage_commands.push(command),
            HostCommand::PublishEvent(command) => published_events.push(command),
            HostCommand::Setting(command) => setting_writes.push(command),
        }
    }
    let mut staged_state = state.clone();
    let mut staged_storage = principal_storage.clone();
    let staged_events = published_events
        .into_iter()
        .map(|command| {
            if !custom_event_owned_by(&command.event, principal) {
                return Err(format!(
                    "principal {principal} does not own custom event {}",
                    command.event
                ));
            }
            let event = CustomEvent {
                event: command.event,
                sender: principal.clone(),
                payload: command.payload,
            };
            event
                .is_valid()
                .then_some(event)
                .ok_or_else(|| "custom event payload is invalid or exceeds its bound".to_owned())
        })
        .collect::<Result<Vec<_>, _>>();
    let apply_result = staged_events.and_then(|staged_events| {
        let owned_setting_prefix = format!("ext.{principal}.");
        if setting_writes
            .iter()
            .any(|command| !command.key.starts_with(&owned_setting_prefix))
        {
            return Err("setting write escaped its principal namespace".to_owned());
        }
        let next_setting_count = pending_setting_writes
            .len()
            .checked_add(setting_writes.len())
            .ok_or_else(|| "pending setting write count overflow".to_owned())?;
        if next_setting_count > MAX_PENDING_SETTING_WRITES {
            return Err(format!(
                "pending setting write limit of {MAX_PENDING_SETTING_WRITES} exceeded"
            ));
        }
        let next_actor_value_count = pending_actor_value_writes
            .len()
            .checked_add(actor_value_writes.len())
            .ok_or_else(|| "pending actor-value write count overflow".to_owned())?;
        if next_actor_value_count > MAX_PENDING_ACTOR_VALUE_WRITES {
            return Err(format!(
                "pending actor-value write limit of {MAX_PENDING_ACTOR_VALUE_WRITES} exceeded"
            ));
        }
        let next_package_evaluation_count = pending_package_evaluations
            .len()
            .checked_add(package_evaluations.len())
            .ok_or_else(|| "pending package reevaluation count overflow".to_owned())?;
        if next_package_evaluation_count > MAX_PENDING_PACKAGE_EVALUATIONS {
            return Err(format!(
                "pending package reevaluation limit of {MAX_PENDING_PACKAGE_EVALUATIONS} exceeded"
            ));
        }
        let next_animation_command_count = pending_animation_commands
            .len()
            .checked_add(animation_commands.len())
            .ok_or_else(|| "pending animation command count overflow".to_owned())?;
        if next_animation_command_count > MAX_PENDING_ANIMATION_COMMANDS {
            return Err(format!(
                "pending animation command limit of {MAX_PENDING_ANIMATION_COMMANDS} exceeded"
            ));
        }
        let next_reputation_write_count = pending_reputation_writes
            .len()
            .checked_add(reputation_writes.len())
            .ok_or_else(|| "pending reputation write count overflow".to_owned())?;
        if next_reputation_write_count > MAX_PENDING_REPUTATION_WRITES {
            return Err(format!(
                "pending reputation write limit of {MAX_PENDING_REPUTATION_WRITES} exceeded"
            ));
        }
        let next_event_count = pending_custom_events
            .len()
            .checked_add(staged_events.len())
            .ok_or_else(|| "pending custom event count overflow".to_owned())?;
        if next_event_count > MAX_PENDING_CUSTOM_EVENTS {
            return Err(format!(
                "pending custom event limit of {MAX_PENDING_CUSTOM_EVENTS} exceeded"
            ));
        }
        let next_payload_bytes = pending_custom_events
            .iter()
            .chain(&staged_events)
            .try_fold(0usize, |total, event| total.checked_add(event.payload.len()))
            .ok_or_else(|| "pending custom event byte count overflow".to_owned())?;
        if next_payload_bytes > MAX_PENDING_CUSTOM_EVENT_BYTES {
            return Err(format!(
                "pending custom event payload limit of {MAX_PENDING_CUSTOM_EVENT_BYTES} bytes exceeded"
            ));
        }
        staged_state
            .apply_batch(principal, &component_commands)
            .map_err(|error| error.to_string())
        .and_then(|()| {
            if storage_commands.is_empty() {
                Ok(())
            } else {
                staged_storage
                    .apply_batch(principal, &storage_commands)
                    .map_err(|error| error.to_string())
            }
        })?;
        Ok(staged_events)
    });
    match apply_result {
        Err(error) => {
            let message = format!("deferred command batch rejected: {error}");
            hosted
                .instance
                .reject_deferred_commands(phase, message.clone());
            diagnostics.push(ExtensionDiagnostic::Fault {
                extension: hosted.extension.clone(),
                component: hosted.component.clone(),
                message,
            });
            stats.faults += 1;
        }
        Ok(staged_events) => {
            *state = staged_state;
            *principal_storage = staged_storage;
            pending_custom_events.extend(staged_events);
            pending_setting_writes.extend(setting_writes);
            pending_actor_value_writes.extend(actor_value_writes);
            pending_package_evaluations.extend(package_evaluations);
            pending_animation_commands.extend(animation_commands);
            pending_reputation_writes.extend(reputation_writes);
            stats.commands_applied += command_count;
        }
    }
}

/// Raw ECS identity snapshot captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawActivation {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
    pub activator: Option<EntityId>,
    pub activator_form: Option<FormRef>,
}

/// Raw cell-load identity captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawCellLoad {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
}

/// Raw equipment change captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawEquipmentChange {
    pub wearer: EntityId,
    pub wearer_form: Option<FormRef>,
    pub item: FormRef,
    pub equipped: bool,
}

/// Raw combat event captured before entering untrusted code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RawHit {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
    pub aggressor: Option<EntityId>,
    pub aggressor_form: Option<FormRef>,
    pub source: Option<EntityId>,
    pub source_form: Option<FormRef>,
    pub projectile: Option<EntityId>,
    pub projectile_form: Option<FormRef>,
    pub damage: f32,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RawEntityProjection {
    name: Option<String>,
    world_transform: Option<WorldTransform>,
    actor_values: Option<Vec<(FormRef, ActorValueState)>>,
    inventory: Option<InventorySnapshot>,
    factions: Option<FactionSnapshot>,
    perks: Option<PerkSnapshot>,
    packages: Option<PackageSnapshot>,
    animation: Option<AnimationSnapshot>,
    reputation: Option<ReputationSnapshot>,
}

fn entity_projection(
    entity: EntityRef,
    form: Option<FormRef>,
    raw: Option<&RawEntityProjection>,
) -> EntityProjection {
    let projection = EntityProjection::new(
        entity,
        form,
        raw.and_then(|projection| projection.name.clone()),
        raw.and_then(|projection| projection.world_transform),
    )
    .expect("live projection capture enforces SDK bounds");
    let projection = match raw.and_then(|projection| projection.actor_values.as_ref()) {
        Some(actor_values) => projection
            .with_actor_values(actor_values.iter().copied())
            .expect("live actor-value capture enforces SDK bounds"),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.inventory.clone()) {
        Some(inventory) => projection.with_inventory(inventory),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.factions.clone()) {
        Some(factions) => projection.with_factions(factions),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.perks.clone()) {
        Some(perks) => projection.with_perks(perks),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.packages.clone()) {
        Some(packages) => projection.with_packages(packages),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.animation) {
        Some(animation) => projection.with_animation(animation),
        None => projection,
    };
    match raw.and_then(|projection| projection.reputation.clone()) {
        Some(reputation) => projection.with_reputation(reputation),
        None => projection,
    }
}

/// Cloneable ECS resource containing the non-ECS extension owner.
#[derive(Clone, Default)]
pub(crate) struct ExtensionHostSlot {
    host: Option<Arc<Mutex<ExtensionHost>>>,
    init_error: Option<Arc<str>>,
}

impl Resource for ExtensionHostSlot {}

impl ExtensionHostSlot {
    pub fn initialize_default() -> Self {
        match ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()) {
            Ok(host) => Self {
                host: Some(Arc::new(Mutex::new(host))),
                init_error: None,
            },
            Err(error) => Self {
                host: None,
                init_error: Some(Arc::from(error.to_string())),
            },
        }
    }

    #[cfg(test)]
    pub fn from_host(host: ExtensionHost) -> Self {
        Self {
            host: Some(Arc::new(Mutex::new(host))),
            init_error: None,
        }
    }

    fn replace_host(&mut self, host: ExtensionHost) {
        self.host = Some(Arc::new(Mutex::new(host)));
        self.init_error = None;
    }

    pub fn host(&self) -> Option<Arc<Mutex<ExtensionHost>>> {
        self.host.clone()
    }

    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }
}

struct ExtensionConsoleCommand {
    route: HostedConsoleCommand,
    host: std::sync::Weak<Mutex<ExtensionHost>>,
}

impl ConsoleCommand for ExtensionConsoleCommand {
    fn name(&self) -> &str {
        &self.route.name
    }

    fn description(&self) -> &str {
        &self.route.description
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(host) = self.host.upgrade() else {
            return CommandOutput::error("extension host is unavailable");
        };
        let (result, diagnostics) = {
            let mut host = host
                .lock()
                .expect("ExtensionHost mutex poisoned by a host panic");
            let result = host.invoke_console_command(&self.route, args);
            apply_pending_world_commands(world, &mut host);
            let diagnostics = host.take_diagnostics();
            (result, diagnostics)
        };
        emit_diagnostics(diagnostics);
        let mut lines = result.lines;
        if result.success {
            if lines.is_empty() {
                lines.push("OK".to_owned());
            }
        } else if let Some(first) = lines.first_mut() {
            *first = format!("Error: {first}");
        } else {
            lines.push("Error: extension command failed".to_owned());
        }
        CommandOutput::lines(lines)
    }
}

/// Publish granted manifest commands into the engine-owned console registry.
///
/// The bridge captures only a weak host reference and never reacquires the
/// registry while dispatch holds its read guard.
pub(crate) fn register_console_commands(world: &World, registry: &mut CommandRegistry) {
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let routes = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .console_commands()
        .to_vec();
    let existing = registry
        .list()
        .into_iter()
        .map(|(name, _)| name.to_owned())
        .collect::<BTreeSet<_>>();
    for route in routes {
        if existing.contains(&route.name) {
            log::error!(
                "extension console command {} collides with an engine command",
                route.name
            );
            continue;
        }
        registry.register(ExtensionConsoleCommand {
            route,
            host: Arc::downgrade(&host),
        });
    }
}

/// Bounded process-local queue of committed game-session transitions.
///
/// Save/load producers run after the scheduler has joined. They enqueue here
/// so guest code executes on the following Late stage, outside save registry,
/// renderer, and ECS resource guards.
#[derive(Default)]
pub(crate) struct SessionEventQueue {
    events: Vec<SessionEvent>,
}

impl Resource for SessionEventQueue {}

pub(crate) fn queue_session_event(world: &World, event: SessionEvent) -> Result<(), &'static str> {
    if !event.is_valid() {
        return Err("invalid session event payload");
    }
    let Some(mut queue) = world.try_resource_mut::<SessionEventQueue>() else {
        return Err("session event queue not installed");
    };
    if queue.events.len() >= MAX_PENDING_SESSION_EVENTS {
        return Err("session event queue is full");
    }
    queue.events.push(event);
    Ok(())
}

#[cfg(test)]
pub(crate) fn pending_session_events(world: &World) -> Vec<SessionEvent> {
    world
        .try_resource::<SessionEventQueue>()
        .map(|queue| queue.events.clone())
        .unwrap_or_default()
}

/// Drain the custom-event queue at the start of the extension dispatch pass.
///
/// Boot registers this before every producer-facing extension adapter. Any
/// event published later in the frame therefore waits for the next Late pass.
pub(crate) fn extension_custom_event_dispatch_system(world: &World, _dt: f32) {
    let spatial_snapshot = Arc::new(capture_spatial_snapshot(world));
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    host.set_spatial_snapshot(spatial_snapshot);
    let stats = host.dispatch_pending_custom_events();
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension custom-event dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Publish the active load-order snapshot before any guest callback runs.
///
/// The resolver owns the immutable catalog, so the common path is one Arc
/// clone plus a pointer comparison and never rebuilds plugin metadata.
pub(crate) fn extension_content_catalog_sync_system(world: &World, _dt: f32) {
    let (catalog, faction_relationships) = {
        let Some(resolver) =
            world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
        else {
            return;
        };
        (resolver.content_catalog(), resolver.faction_relationships())
    };
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    host.set_content_catalog(catalog);
    host.set_faction_relationships(faction_relationships);
}

/// Publish public engine configuration before sandbox callbacks run.
pub(crate) fn extension_engine_settings_sync_system(world: &World, _dt: f32) {
    let Some(settings) = engine_settings_snapshot(world) else {
        return;
    };
    let host = world
        .try_resource::<ExtensionHostSlot>()
        .and_then(|slot| slot.host());
    let Some(host) = host else {
        return;
    };
    host.lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .set_engine_settings(settings);
}

/// Commit sandbox setting writes after every callback for this frame has run.
pub(crate) fn extension_setting_write_apply_system(world: &World, _dt: f32) {
    let host = world
        .try_resource::<ExtensionHostSlot>()
        .and_then(|slot| slot.host());
    let Some(host) = host else {
        return;
    };
    let writes = {
        let mut host = host
            .lock()
            .expect("ExtensionHost mutex poisoned by a host panic");
        std::mem::take(&mut host.pending_setting_writes)
    };
    if writes.is_empty() {
        return;
    }

    let mut staged = world
        .resource::<byroredux_core::settings::SettingsRegistry>()
        .clone();
    for write in &writes {
        let value = match &write.value {
            SdkSettingValue::Boolean(value) => byroredux_core::settings::SettingValue::Bool(*value),
            SdkSettingValue::Number(value) => {
                byroredux_core::settings::SettingValue::Number(*value)
            }
            SdkSettingValue::Choice(value) => {
                byroredux_core::settings::SettingValue::Choice(value.clone())
            }
        };
        if let Err(error) = staged.set(&write.key, value) {
            log::error!("rejected validated extension setting batch: {error}");
            return;
        }
    }
    let snapshot = match settings_snapshot_from_registry(&staged) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            log::error!("rejected extension setting snapshot: {error}");
            return;
        }
    };
    *world.resource_mut::<byroredux_core::settings::SettingsRegistry>() = staged;
    if let Some(persistence) = world.try_resource::<crate::settings_io::SettingsPersistence>() {
        let settings = world.resource::<byroredux_core::settings::SettingsRegistry>();
        crate::settings_io::save(&settings, &persistence);
    }
    host.lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .set_engine_settings(snapshot);
}

fn engine_settings_snapshot(world: &World) -> Option<Arc<SettingsSnapshot>> {
    let registry = world.try_resource::<byroredux_core::settings::SettingsRegistry>()?;
    settings_snapshot_from_registry(&registry).ok()
}

fn settings_snapshot_from_registry(
    registry: &byroredux_core::settings::SettingsRegistry,
) -> Result<Arc<SettingsSnapshot>, byroredux_sdk::settings::SettingsSnapshotError> {
    let entries = registry.entries().map(|entry| {
        let value = match &entry.value {
            byroredux_core::settings::SettingValue::Bool(value) => SdkSettingValue::Boolean(*value),
            byroredux_core::settings::SettingValue::Number(value) => {
                SdkSettingValue::Number(*value)
            }
            byroredux_core::settings::SettingValue::Choice(value) => {
                SdkSettingValue::Choice(value.clone())
            }
        };
        (entry.id.clone(), value)
    });
    SettingsSnapshot::new(entries).map(Arc::new)
}

fn register_extension_setting(
    registry: &mut byroredux_core::settings::SettingsRegistry,
    extension: &ExtensionId,
    declaration: &SettingDeclaration,
) -> Result<(), byroredux_core::settings::SettingsError> {
    use byroredux_core::settings::{SettingChoice, SettingEntry};

    let id = declaration.qualified_name(extension);
    let mut entry = match (&declaration.default, &declaration.control) {
        (SdkSettingValue::Boolean(value), SettingControlDeclaration::Toggle) => {
            SettingEntry::toggle(
                id,
                extension.as_str(),
                &declaration.label,
                &declaration.description,
                *value,
            )
        }
        (
            SdkSettingValue::Number(value),
            SettingControlDeclaration::Slider {
                min,
                max,
                step,
                unit,
            },
        ) => SettingEntry::slider(
            id,
            extension.as_str(),
            &declaration.label,
            &declaration.description,
            *value,
            *min,
            *max,
            *step,
            unit,
        ),
        (SdkSettingValue::Choice(value), SettingControlDeclaration::Choice { options }) => {
            SettingEntry::choice(
                id,
                extension.as_str(),
                &declaration.label,
                &declaration.description,
                value,
                options
                    .iter()
                    .map(|option| SettingChoice::new(&option.value, &option.label))
                    .collect(),
            )
        }
        _ => unreachable!("validated setting declarations have matching types"),
    };
    entry.restart_required = declaration.restart_required;
    registry.register(entry)
}

/// Late-stage adapter from transient ECS markers to sandbox callbacks.
///
/// Every ECS guard is dropped before the host mutex is acquired and before any
/// guest code runs. The function is scheduled immediately before transient
/// event cleanup, after built-in activation consumers have observed the same
/// marker.
pub(crate) fn extension_activation_dispatch_system(world: &World, _dt: f32) {
    let missing_player = world
        .try_resource::<byroredux_scripting::papyrus_demo::PlayerEntity>()
        .is_none();
    let raw_activations: Vec<(EntityId, Option<EntityId>)> = {
        let Some(events) = world.query::<byroredux_scripting::ActivateEvent>() else {
            return;
        };
        events
            .iter()
            .map(|(subject, event)| {
                (
                    subject,
                    (!(missing_player && event.activator == 0)).then_some(event.activator),
                )
            })
            .collect()
    };
    if raw_activations.is_empty() {
        return;
    }
    let form_bindings = forms_by_entity(world);
    let disclosed_entities = raw_activations
        .iter()
        .flat_map(|(subject, activator)| std::iter::once(*subject).chain(*activator))
        .collect::<BTreeSet<_>>();
    let projections = capture_entity_projections(world, &disclosed_entities);
    let activations = raw_activations
        .into_iter()
        .map(|(subject, activator)| RawActivation {
            subject,
            subject_form: form_bindings.get(&subject).copied(),
            activator,
            activator_form: activator.and_then(|entity| form_bindings.get(&entity).copied()),
        });

    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_activations_with_projections(activations, &projections);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension activation dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage adapter from cell-load markers to sandbox callbacks.
///
/// Like activation delivery, all ECS guards are dropped before guest code
/// runs. The marker remains available to built-in systems until the shared
/// transient-event cleanup pass.
pub(crate) fn extension_cell_load_dispatch_system(world: &World, _dt: f32) {
    let raw_subjects = {
        let Some(events) = world.query::<byroredux_scripting::OnCellLoadEvent>() else {
            return;
        };
        events
            .iter()
            .map(|(subject, _)| subject)
            .collect::<Vec<_>>()
    };
    if raw_subjects.is_empty() {
        return;
    }
    let form_bindings = forms_by_entity(world);
    let disclosed_entities = raw_subjects.iter().copied().collect::<BTreeSet<_>>();
    let projections = capture_entity_projections(world, &disclosed_entities);
    let cell_loads = raw_subjects.into_iter().map(|subject| RawCellLoad {
        subject,
        subject_form: form_bindings.get(&subject).copied(),
    });

    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_cell_loads_with_projections(cell_loads, &projections);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension cell-load dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage adapter from ordered equipment mutation batches to sandbox callbacks.
pub(crate) fn extension_equipment_dispatch_system(world: &World, _dt: f32) {
    let raw_changes = {
        let Some(events) = world.query::<byroredux_scripting::EquipmentEventBatch>() else {
            return;
        };
        events
            .iter()
            .flat_map(|(wearer, batch)| batch.0.iter().copied().map(move |change| (wearer, change)))
            .collect::<Vec<_>>()
    };
    if raw_changes.is_empty() {
        return;
    }

    let form_bindings = forms_by_entity(world);
    let disclosed_entities = raw_changes
        .iter()
        .map(|(wearer, _)| *wearer)
        .collect::<BTreeSet<_>>();
    let projections = capture_entity_projections(world, &disclosed_entities);
    let changes = {
        let Some(resolver) =
            world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
        else {
            log::warn!("extension equipment dispatch skipped: load-order resolver is unavailable");
            return;
        };
        raw_changes
            .into_iter()
            .filter_map(|(wearer, change)| {
                let Some(item) = resolver.resolve(change.item_form_id).map(form_ref) else {
                    log::warn!(
                        "extension equipment dispatch skipped unresolved item form {:#010X}",
                        change.item_form_id
                    );
                    return None;
                };
                Some(RawEquipmentChange {
                    wearer,
                    wearer_form: form_bindings.get(&wearer).copied(),
                    item,
                    equipped: change.equipped,
                })
            })
            .collect::<Vec<_>>()
    };
    if changes.is_empty() {
        return;
    }

    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_equipment_changes_with_projections(changes, &projections);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension equipment dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage adapter from rebinding-independent gameplay action edges.
pub(crate) fn extension_input_dispatch_system(world: &World, _dt: f32) {
    let events = {
        let Some(state) = world.try_resource::<crate::interaction::ActionState>() else {
            return;
        };
        crate::interaction::InputAction::OBSERVABLE
            .into_iter()
            .filter_map(|action| {
                let phase = if state.was_pressed(action) {
                    InputPhase::Pressed
                } else if state.was_released(action) {
                    InputPhase::Released
                } else {
                    return None;
                };
                Some(InputActionEvent {
                    action: sdk_input_action(action),
                    phase,
                })
            })
            .collect::<Vec<_>>()
    };
    if events.is_empty() {
        return;
    }
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_input_actions_inner(events);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension input dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage delivery of committed save/load/new-game transitions.
pub(crate) fn extension_session_dispatch_system(world: &World, _dt: f32) {
    let events = {
        let Some(mut queue) = world.try_resource_mut::<SessionEventQueue>() else {
            return;
        };
        std::mem::take(&mut queue.events)
    };
    if events.is_empty() {
        return;
    }
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_session_events_inner(events);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension session dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage adapter from combat hit markers to sandbox callbacks.
pub(crate) fn extension_hit_dispatch_system(world: &World, _dt: f32) {
    let raw_hits = {
        let Some(events) = world.query::<byroredux_scripting::HitEvent>() else {
            return;
        };
        events
            .iter()
            .map(|(subject, event)| {
                (
                    subject,
                    event.aggressor,
                    event.source,
                    event.projectile,
                    event.damage,
                    event.power_attack,
                    event.sneak_attack,
                    event.bash_attack,
                    event.blocked,
                )
            })
            .collect::<Vec<_>>()
    };
    if raw_hits.is_empty() {
        return;
    }
    let form_bindings = forms_by_entity(world);
    let disclosed_entities = raw_hits
        .iter()
        .flat_map(|hit| {
            [
                Some(hit.0),
                Some(hit.1),
                Some(hit.2),
                (hit.3 != 0).then_some(hit.3),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<BTreeSet<_>>();
    let projections = capture_entity_projections(world, &disclosed_entities);
    let hits = raw_hits.into_iter().map(
        |(
            subject,
            aggressor,
            source,
            projectile,
            damage,
            power_attack,
            sneak_attack,
            bash_attack,
            blocked,
        )| {
            let projectile = (projectile != 0).then_some(projectile);
            RawHit {
                subject,
                subject_form: form_bindings.get(&subject).copied(),
                aggressor: Some(aggressor),
                aggressor_form: form_bindings.get(&aggressor).copied(),
                source: Some(source),
                source_form: form_bindings.get(&source).copied(),
                projectile,
                projectile_form: projectile.and_then(|entity| form_bindings.get(&entity).copied()),
                damage,
                power_attack,
                sneak_attack,
                bash_attack,
                blocked,
            }
        },
    );

    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_hits_with_projections(hits, &projections);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension hit dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

/// Late-stage owner of manifest-declared recurring extension callbacks.
pub(crate) fn extension_update_dispatch_system(world: &World, dt: f32) {
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let stats = host.dispatch_recurring_updates(dt);
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    emit_diagnostics(diagnostics);
    if stats.faults > 0 {
        log::warn!(
            "extension update dispatch: events={} deliveries={} commands={} faults={}",
            stats.events,
            stats.deliveries,
            stats.commands_applied,
            stats.faults
        );
    }
}

fn emit_diagnostics(diagnostics: Vec<ExtensionDiagnostic>) {
    for diagnostic in diagnostics {
        match diagnostic {
            ExtensionDiagnostic::Log {
                extension,
                component,
                entry,
            } => match entry.level {
                LogLevel::Debug => {
                    log::debug!("extension {extension}/{component}: {}", entry.message)
                }
                LogLevel::Info => {
                    log::info!("extension {extension}/{component}: {}", entry.message)
                }
                LogLevel::Warn => {
                    log::warn!("extension {extension}/{component}: {}", entry.message)
                }
                LogLevel::Error => {
                    log::error!("extension {extension}/{component}: {}", entry.message)
                }
            },
            ExtensionDiagnostic::Fault {
                extension,
                component,
                message,
            } => log::error!("extension {extension}/{component} fault: {message}"),
        }
    }
}

/// Load explicitly requested extension packages as one atomic profile.
///
/// `--extension <manifest.toml>` is repeatable. Capabilities remain denied
/// unless explicitly granted with
/// `--extension-grant <extension-id>=<capability-id>`; `=*` grants every
/// capability requested by that manifest. A failure leaves the default empty
/// host installed, so ordinary content remains usable.
pub(crate) fn load_requested_extensions(world: &World, args: &[String]) -> anyhow::Result<usize> {
    let manifest_paths = flag_values(args, "--extension");
    if manifest_paths.is_empty() {
        return Ok(0);
    }

    let mut staged_host =
        ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default())?;
    let mut sources = BTreeMap::<ExtensionId, PathBuf>::new();
    let mut manifests = Vec::with_capacity(manifest_paths.len());
    for path in manifest_paths {
        let manifest_path = std::fs::canonicalize(path)
            .map_err(|error| anyhow::anyhow!("extension manifest {path:?}: {error}"))?;
        let source = std::fs::read_to_string(&manifest_path).map_err(|error| {
            anyhow::anyhow!("could not read extension manifest {manifest_path:?}: {error}")
        })?;
        let manifest = byroredux_plugin::ResolvedExtensionSet::parse_manifest(&source)
            .map_err(|error| anyhow::anyhow!("extension manifest {manifest_path:?}: {error}"))?;
        let root = manifest_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("extension manifest has no package directory"))?
            .to_owned();
        if sources.insert(manifest.id.clone(), root).is_some() {
            anyhow::bail!("duplicate extension identity {}", manifest.id);
        }
        manifests.push(manifest);
    }

    let resolved =
        byroredux_plugin::ResolvedExtensionSet::resolve(manifests, staged_host.catalog())?;
    let by_id: BTreeMap<_, _> = resolved
        .manifests()
        .iter()
        .map(|manifest| (manifest.id.clone(), manifest))
        .collect();
    let mut grants = BTreeMap::<ExtensionId, CapabilitySet>::new();
    for spec in flag_values(args, "--extension-grant") {
        let (extension, capability) = spec.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "invalid --extension-grant {spec:?}; expected <extension-id>=<capability-id|*>"
            )
        })?;
        let extension = ExtensionId::new(extension)
            .map_err(|error| anyhow::anyhow!("invalid extension grant principal: {error}"))?;
        let manifest = by_id.get(&extension).ok_or_else(|| {
            anyhow::anyhow!("grant names extension {extension}, which is not requested")
        })?;
        let set = grants.entry(extension).or_default();
        if capability == "*" {
            for request in &manifest.capabilities {
                set.grant_id(request.id.clone());
            }
        } else {
            let capability = CapabilityId::new(capability)
                .map_err(|error| anyhow::anyhow!("invalid capability grant: {error}"))?;
            set.grant_id(capability);
        }
    }

    let mut staged_settings = world
        .try_resource::<byroredux_core::settings::SettingsRegistry>()
        .map(|registry| registry.clone());
    for manifest in resolved.manifests() {
        let granted = grants
            .get(&manifest.id)
            .is_some_and(|grants| grants.contains(SETTINGS_REGISTER_CAPABILITY));
        if !granted {
            continue;
        }
        let registry = staged_settings.as_mut().ok_or_else(|| {
            anyhow::anyhow!("extension settings requested before SettingsRegistry installation")
        })?;
        for declaration in &manifest.settings {
            register_extension_setting(registry, &manifest.id, declaration).map_err(|error| {
                anyhow::anyhow!("could not register setting for {}: {error}", manifest.id)
            })?;
        }
    }
    if let Some(registry) = staged_settings.as_mut() {
        if let Some(persistence) = world.try_resource::<crate::settings_io::SettingsPersistence>() {
            crate::settings_io::load(registry, &persistence);
        }
        let snapshot = settings_snapshot_from_registry(registry)
            .map_err(|error| anyhow::anyhow!("extension settings snapshot is invalid: {error}"))?;
        staged_host.set_engine_settings(snapshot);
    }

    for manifest in resolved.manifests() {
        let root = &sources[&manifest.id];
        let mut artifacts = ExtensionArtifacts::new();
        for component in &manifest.components {
            artifacts.insert(
                component.id.clone(),
                read_package_artifact(root, &component.path, staged_host.max_component_bytes())?,
            );
        }
        staged_host.install_package(
            manifest,
            &artifacts,
            grants.remove(&manifest.id).unwrap_or_default(),
        )?;
    }
    let package_count = staged_host.package_count();
    let component_count = staged_host.component_count();
    emit_diagnostics(staged_host.take_diagnostics());
    if let Some(staged_settings) = staged_settings {
        *world.resource_mut::<byroredux_core::settings::SettingsRegistry>() = staged_settings;
    }
    {
        let mut slot = world.resource_mut::<ExtensionHostSlot>();
        slot.replace_host(staged_host);
    }
    log::info!(
        "activated {package_count} executable extension packages ({component_count} components)"
    );
    Ok(package_count)
}

fn flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.iter()
        .enumerate()
        .filter_map(|(index, value)| (value == flag).then(|| args.get(index + 1)).flatten())
        .filter(|value| !value.starts_with("--"))
        .map(String::as_str)
        .collect()
}

fn read_package_artifact(root: &Path, relative: &str, maximum: usize) -> anyhow::Result<Vec<u8>> {
    let path = std::fs::canonicalize(root.join(relative)).map_err(|error| {
        anyhow::anyhow!("could not resolve extension artifact {relative:?}: {error}")
    })?;
    if !path.starts_with(root) {
        anyhow::bail!("extension artifact {relative:?} escapes its package directory");
    }
    let file = std::fs::File::open(&path)
        .map_err(|error| anyhow::anyhow!("could not open extension artifact {path:?}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| anyhow::anyhow!("could not read extension artifact {path:?}: {error}"))?;
    if bytes.len() > maximum {
        anyhow::bail!("extension artifact {path:?} exceeds the {maximum}-byte component limit");
    }
    Ok(bytes)
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

fn forms_by_entity(world: &World) -> BTreeMap<EntityId, FormRef> {
    match (
        world.query::<FormIdComponent>(),
        world.try_resource::<FormIdPool>(),
    ) {
        (Some(forms), Some(pool)) => forms
            .iter()
            .filter_map(|(entity, component)| {
                pool.resolve(component.0)
                    .copied()
                    .map(|pair| (entity, form_ref(pair)))
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn capture_spatial_snapshot(world: &World) -> SpatialSnapshot {
    let forms = forms_by_entity(world);
    let mut references = BTreeMap::<FormRef, SpatialReference>::new();
    let mut truncated = false;
    let global_transforms = world.query::<GlobalTransform>();
    let local_transforms = world.query::<Transform>();
    for (&entity, &form) in &forms {
        let position = global_transforms
            .as_ref()
            .and_then(|transforms| transforms.get(entity))
            .map(|transform| transform.translation.to_array())
            .or_else(|| {
                local_transforms
                    .as_ref()
                    .and_then(|transforms| transforms.get(entity))
                    .map(|transform| transform.translation.to_array())
            });
        let Some(position) = position else {
            continue;
        };
        let Ok(reference) = SpatialReference::new(form, position) else {
            truncated = true;
            continue;
        };
        if let std::collections::btree_map::Entry::Vacant(entry) = references.entry(form) {
            entry.insert(reference);
        } else {
            truncated = true;
        }
    }
    let mut references = references.into_values().collect::<Vec<_>>();
    if references.len() > MAX_SPATIAL_REFERENCES {
        references.truncate(MAX_SPATIAL_REFERENCES);
        truncated = true;
    }
    SpatialSnapshot::new(references, truncated)
        .expect("live spatial capture enforces SDK bounds and portable ordering")
}

fn capture_entity_projections(
    world: &World,
    entities: &BTreeSet<EntityId>,
) -> BTreeMap<EntityId, RawEntityProjection> {
    let mut projections = entities
        .iter()
        .copied()
        .map(|entity| (entity, RawEntityProjection::default()))
        .collect::<BTreeMap<_, _>>();

    if let (Some(names), Some(pool)) = (world.query::<Name>(), world.try_resource::<StringPool>()) {
        for (entity, projection) in &mut projections {
            projection.name = names
                .get(*entity)
                .and_then(|name| pool.resolve(name.0))
                .filter(|name| name.len() <= MAX_ENTITY_NAME_BYTES)
                .map(str::to_owned);
        }
    }

    if let Some(transforms) = world.query::<GlobalTransform>() {
        for (entity, projection) in &mut projections {
            projection.world_transform = transforms.get(*entity).and_then(|transform| {
                WorldTransform::new(
                    transform.translation.to_array(),
                    transform.rotation.to_array(),
                    transform.scale,
                )
                .ok()
            });
        }
    }
    if let Some(transforms) = world.query::<Transform>() {
        for (entity, projection) in &mut projections {
            if projection.world_transform.is_some() {
                continue;
            }
            projection.world_transform = transforms.get(*entity).and_then(|transform| {
                WorldTransform::new(
                    transform.translation.to_array(),
                    transform.rotation.to_array(),
                    transform.scale,
                )
                .ok()
            });
        }
    }
    if let (Some(actor_values), Some(resolver)) = (
        world.query::<ActorValues>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(values) = actor_values.get(*entity) else {
                continue;
            };
            let mut values = values
                .iter()
                .filter_map(|(form_id, value)| {
                    let actor_value = resolver.resolve(form_id).map(form_ref)?;
                    let value = ActorValueState::new(
                        value.base,
                        value.permanent_mod,
                        value.temporary_mod,
                        value.damage,
                    )
                    .ok()?;
                    Some((actor_value, value))
                })
                .collect::<Vec<_>>();
            values.sort_by_key(|(form, _)| *form);
            values.truncate(MAX_ACTOR_VALUES_PER_ENTITY);
            projection.actor_values = Some(values);
        }
    }
    let equipment_by_entity = world
        .query::<EquipmentSlots>()
        .map(|equipment| {
            projections
                .keys()
                .filter_map(|&entity| equipment.get(entity).cloned().map(|slots| (entity, slots)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    if let (Some(inventories), Some(resolver)) = (
        world.query::<Inventory>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        let item_catalog = world.try_resource::<crate::inventory::InventoryCatalog>();
        for (entity, projection) in &mut projections {
            let Some(inventory) = inventories.get(*entity) else {
                continue;
            };
            let equipment = equipment_by_entity.get(entity);
            let mut truncated = false;
            let mut summaries = BTreeMap::new();
            for (raw_index, stack) in inventory.items.iter().enumerate() {
                let Some(item) = resolver.resolve(stack.base_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if item.local() == 0 {
                    truncated = true;
                    continue;
                }
                let Ok(raw_index) = u32::try_from(raw_index) else {
                    truncated = true;
                    break;
                };
                let index = InventoryIndex(raw_index);
                let biped_slots = equipment.map_or(0, |slots| {
                    slots
                        .occupants
                        .iter()
                        .enumerate()
                        .fold(0_u32, |mask, (bit, occupant)| {
                            if *occupant == Some(index) {
                                mask | (1_u32 << bit)
                            } else {
                                mask
                            }
                        })
                });
                let weapon_equipped = equipment.is_some_and(|slots| slots.weapon == Some(index));
                let metadata = item_catalog
                    .as_ref()
                    .and_then(|catalog| catalog.sdk_metadata(stack.base_form_id));
                let summary = summaries
                    .entry(item)
                    .or_insert_with(|| (0_u64, 0_u32, false, metadata.clone()));
                summary.0 = summary
                    .0
                    .checked_add(u64::from(stack.count))
                    .unwrap_or_else(|| {
                        truncated = true;
                        u64::MAX
                    });
                summary.1 |= biped_slots;
                summary.2 |= weapon_equipped;
                if summary.3.is_none() {
                    summary.3 = metadata;
                }
            }
            let mut entries = summaries
                .into_iter()
                .map(|(item, (count, biped_slots, weapon_equipped, metadata))| {
                    InventoryEntry::new(item, count, biped_slots, weapon_equipped, metadata)
                        .expect("resolved inventory forms are non-null")
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_INVENTORY_ENTRIES_PER_ENTITY {
                entries.truncate(MAX_INVENTORY_ENTRIES_PER_ENTITY);
                truncated = true;
            }
            projection.inventory = Some(
                InventorySnapshot::new(entries, truncated)
                    .expect("live inventory capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(faction_ranks), Some(resolver)) = (
        world.query::<FactionRanks>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(ranks) = faction_ranks.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut memberships = BTreeMap::<FormRef, i8>::new();
            for &(global_faction, rank) in &ranks.0 {
                let Some(faction) = resolver.resolve(global_faction).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if faction.local() == 0 {
                    truncated = true;
                    continue;
                }
                memberships.entry(faction).or_insert(rank);
            }
            let mut memberships = memberships
                .into_iter()
                .map(|(faction, rank)| {
                    FactionMembership::new(faction, rank)
                        .expect("resolved faction forms are non-null")
                })
                .collect::<Vec<_>>();
            if memberships.len() > MAX_FACTIONS_PER_ENTITY {
                memberships.truncate(MAX_FACTIONS_PER_ENTITY);
                truncated = true;
            }
            projection.factions = Some(
                FactionSnapshot::new(memberships, truncated)
                    .expect("live faction capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(perks), Some(resolver)) = (
        world.query::<Perks>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(perks) = perks.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut entries = BTreeMap::<FormRef, u8>::new();
            for perk in &perks.entries {
                let Some(identity) = resolver.resolve(perk.perk_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if identity.local() == 0 || perk.rank == 0 {
                    truncated = true;
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = entries.entry(identity) {
                    entry.insert(perk.rank);
                } else {
                    truncated = true;
                }
            }
            let mut entries = entries
                .into_iter()
                .map(|(perk, rank)| {
                    PerkEntry::new(perk, rank).expect("resolved perk entries are valid")
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_PERKS_PER_ENTITY {
                entries.truncate(MAX_PERKS_PER_ENTITY);
                truncated = true;
            }
            projection.perks = Some(
                PerkSnapshot::new(entries, truncated)
                    .expect("live perk capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(reputations), Some(resolver)) = (
        world.query::<FactionReputation>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(reputation) = reputations.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut entries = BTreeMap::<FormRef, (u16, u16)>::new();
            for standing in &reputation.entries {
                let Some(identity) = resolver.resolve(standing.repu_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if identity.local() == 0 {
                    truncated = true;
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = entries.entry(identity) {
                    entry.insert((standing.fame, standing.infamy));
                } else {
                    truncated = true;
                }
            }
            let mut entries = entries
                .into_iter()
                .filter_map(|(reputation, (fame, infamy))| {
                    ReputationEntry::new(reputation, fame, infamy)
                        .map_err(|_| truncated = true)
                        .ok()
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_REPUTATIONS_PER_ENTITY {
                entries.truncate(MAX_REPUTATIONS_PER_ENTITY);
                truncated = true;
            }
            projection.reputation = Some(
                ReputationSnapshot::new(entries, truncated)
                    .expect("live reputation capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let Some(resolver) =
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
    {
        if let Some(states) = world.query::<byroredux_scripting::ActorCinematicState>() {
            for (entity, projection) in &mut projections {
                let Some(state) = states.get(*entity) else {
                    continue;
                };
                projection.animation = Some(AnimationSnapshot::new(
                    state
                        .requested_idle_form_id
                        .and_then(|form| resolver.resolve(form))
                        .map(form_ref)
                        .filter(|form| form.local() != 0),
                    state.idle_request_serial,
                    state.awaited_event.map(animation_event),
                    state.last_animation_event.map(animation_event),
                    state.animation_event_serial,
                ));
            }
        }
        let mut package_captures = projections
            .keys()
            .copied()
            .map(|entity| (entity, PackageCapture::default()))
            .collect::<BTreeMap<_, _>>();
        if let Some(ambient) = world.query::<AmbientPackageRuntime>() {
            for (entity, capture) in &mut package_captures {
                let Some(runtime) = ambient.get(*entity) else {
                    continue;
                };
                let active = runtime
                    .active_package_form_id
                    .and_then(|form| capture_package_form(form, &resolver, capture));
                let candidates =
                    capture_package_candidates(&runtime.package_candidates, &resolver, capture);
                push_package_selection(
                    capture,
                    PackageSelection::ambient(candidates, active)
                        .expect("captured ambient package selection is bounded"),
                );
            }
        }
        let mut scene_actions = world
            .query::<byroredux_scripting::ScenePackagePlayback>()
            .map(|playbacks| {
                playbacks
                    .iter()
                    .flat_map(|(_, playback)| playback.active_actions.iter().cloned())
                    .filter(|action| package_captures.contains_key(&action.actor))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        scene_actions.sort_by_key(|action| {
            (
                action.actor,
                action.scene_form_id,
                action.action_index,
                action.package_form_id,
                action.template_form_id,
            )
        });
        for action in scene_actions {
            let capture = package_captures
                .get_mut(&action.actor)
                .expect("scene action was filtered to a disclosed actor");
            if capture.selections.len() >= MAX_PACKAGE_SELECTIONS_PER_ENTITY {
                capture.truncated = true;
                continue;
            }
            let scene = capture_package_form(action.scene_form_id, &resolver, capture);
            let active = capture_package_form(action.package_form_id, &resolver, capture);
            let template = capture_package_form(action.template_form_id, &resolver, capture);
            let candidates =
                capture_package_candidates(&action.package_candidates, &resolver, capture);
            push_package_selection(
                capture,
                PackageSelection::scene_action(
                    scene,
                    action.action_index,
                    candidates,
                    active,
                    template,
                )
                .expect("captured scene package selection is bounded"),
            );
        }
        for (entity, capture) in package_captures {
            if capture.selections.is_empty() {
                continue;
            }
            projections
                .get_mut(&entity)
                .expect("package capture is scoped to projected entities")
                .packages = Some(
                PackageSnapshot::new(capture.selections, capture.truncated)
                    .expect("live package capture enforces SDK bounds"),
            );
        }
    }
    projections
}

#[derive(Default)]
struct PackageCapture {
    selections: Vec<PackageSelection>,
    references: usize,
    truncated: bool,
}

fn capture_package_form(
    global: u32,
    resolver: &crate::cell_loader::load_order::GlobalFormIdResolver,
    capture: &mut PackageCapture,
) -> Option<FormRef> {
    let Some(form) = resolver.resolve(global).map(form_ref) else {
        capture.truncated = true;
        return None;
    };
    if form.local() == 0 || capture.references >= MAX_PACKAGE_REFERENCES_PER_ENTITY {
        capture.truncated = true;
        return None;
    }
    capture.references += 1;
    Some(form)
}

fn capture_package_candidates(
    candidates: &[u32],
    resolver: &crate::cell_loader::load_order::GlobalFormIdResolver,
    capture: &mut PackageCapture,
) -> Vec<FormRef> {
    if candidates.len() > MAX_PACKAGE_CANDIDATES {
        capture.truncated = true;
    }
    candidates
        .iter()
        .take(MAX_PACKAGE_CANDIDATES)
        .filter_map(|&candidate| capture_package_form(candidate, resolver, capture))
        .collect()
}

fn push_package_selection(capture: &mut PackageCapture, selection: PackageSelection) {
    if capture.selections.len() >= MAX_PACKAGE_SELECTIONS_PER_ENTITY {
        capture.truncated = true;
    } else {
        capture.selections.push(selection);
    }
}

fn animation_event(event: byroredux_scripting::CinematicAnimationEvent) -> AnimationEvent {
    match event {
        byroredux_scripting::CinematicAnimationEvent::PlayImod => AnimationEvent::PlayImod,
        byroredux_scripting::CinematicAnimationEvent::IdleFurnitureExit => {
            AnimationEvent::IdleFurnitureExit
        }
        byroredux_scripting::CinematicAnimationEvent::ExitCartEnd => AnimationEvent::ExitCartEnd,
    }
}

fn apply_pending_actor_value_writes(world: &World, host: &mut ExtensionHost) {
    let commands = match host.take_resolved_actor_value_writes() {
        Ok(commands) => commands,
        Err(error) => {
            host.record_host_fault(format!("deferred actor-value batch rejected: {error}"));
            return;
        }
    };
    if commands.is_empty() {
        return;
    }
    let apply = (|| -> Result<(), String> {
        let resolver = world
            .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .ok_or_else(|| "active form resolver is unavailable".to_owned())?;
        let values = world
            .query::<ActorValues>()
            .ok_or_else(|| "ActorValues storage is unavailable".to_owned())?;
        let mut staged = BTreeMap::<EntityId, ActorValues>::new();
        for command in &commands {
            let actor_value = resolver
                .global_form_id(command.actor_value)
                .ok_or_else(|| "portable actor-value identity is not loaded".to_owned())?;
            let actor_values = if let Some(values) = staged.get_mut(&command.entity) {
                values
            } else {
                let values = values
                    .get(command.entity)
                    .cloned()
                    .ok_or_else(|| "actor-value target no longer carries ActorValues".to_owned())?;
                staged.entry(command.entity).or_insert(values)
            };
            match command.operation {
                ActorValueOperation::SetBase => actor_values.set_base(actor_value, command.value),
                ActorValueOperation::ModifyPermanent => {
                    actor_values.mod_permanent(actor_value, command.value)
                }
                ActorValueOperation::ModifyTemporary => {
                    actor_values.mod_temporary(actor_value, command.value)
                }
                ActorValueOperation::Damage => {
                    actor_values.apply_damage(actor_value, command.value)
                }
                ActorValueOperation::Restore => actor_values.restore(actor_value, command.value),
            }
            let state = actor_values
                .get(actor_value)
                .expect("actor-value mutation creates the target entry");
            if [
                state.base,
                state.permanent_mod,
                state.temporary_mod,
                state.damage,
                state.current(),
            ]
            .into_iter()
            .any(|value| !value.is_finite())
            {
                return Err("actor-value command batch produced a non-finite value".to_owned());
            }
        }
        drop(values);
        let mut live = world
            .query_mut::<ActorValues>()
            .ok_or_else(|| "ActorValues storage disappeared before commit".to_owned())?;
        for (entity, values) in staged {
            let target = live
                .get_mut(entity)
                .ok_or_else(|| "actor-value target disappeared before commit".to_owned())?;
            *target = values;
        }
        Ok(())
    })();
    if let Err(error) = apply {
        host.record_host_fault(format!("deferred actor-value batch rejected: {error}"));
    }
}

fn apply_pending_package_evaluations(world: &World, host: &mut ExtensionHost) {
    let entities = match host.take_resolved_package_evaluations() {
        Ok(entities) => entities,
        Err(error) => {
            host.record_host_fault(format!(
                "deferred package reevaluation batch rejected: {error}"
            ));
            return;
        }
    };
    if entities.is_empty() {
        return;
    }
    let Some(mut requests) = world.query_mut::<byroredux_scripting::EvaluatePackageRequest>()
    else {
        host.record_host_fault(
            "deferred package reevaluation batch rejected: EvaluatePackageRequest storage is unavailable"
                .to_owned(),
        );
        return;
    };
    for entity in entities {
        requests.insert(entity, byroredux_scripting::EvaluatePackageRequest);
    }
}

fn apply_pending_animation_commands(world: &World, host: &mut ExtensionHost) {
    let commands = match host.take_resolved_animation_commands() {
        Ok(commands) => commands,
        Err(error) => {
            host.record_host_fault(format!("deferred animation batch rejected: {error}"));
            return;
        }
    };
    if commands.is_empty() {
        return;
    }
    let apply = (|| -> Result<(), String> {
        let resolver = world
            .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .ok_or_else(|| "active form resolver is unavailable".to_owned())?;
        let mut resolved = Vec::with_capacity(commands.len());
        for command in commands {
            let idle = resolver
                .global_form_id(command.idle)
                .ok_or_else(|| "portable animation IDLE identity is not loaded".to_owned())?;
            resolved.push((command.entity, idle));
        }
        drop(resolver);
        let mut states = world
            .query_mut::<byroredux_scripting::ActorCinematicState>()
            .ok_or_else(|| "ActorCinematicState storage is unavailable".to_owned())?;
        for (entity, idle) in resolved {
            if let Some(state) = states.get_mut(entity) {
                state.request_idle(idle);
            } else {
                let mut state = byroredux_scripting::ActorCinematicState::default();
                state.request_idle(idle);
                states.insert(entity, state);
            }
        }
        Ok(())
    })();
    if let Err(error) = apply {
        host.record_host_fault(format!("deferred animation batch rejected: {error}"));
    }
}

fn apply_pending_reputation_writes(world: &World, host: &mut ExtensionHost) {
    let commands = match host.take_resolved_reputation_writes() {
        Ok(commands) => commands,
        Err(error) => {
            host.record_host_fault(format!("deferred reputation batch rejected: {error}"));
            return;
        }
    };
    if commands.is_empty() {
        return;
    }
    let apply = (|| -> Result<(), String> {
        let resolver = world
            .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .ok_or_else(|| "active form resolver is unavailable".to_owned())?;
        let live = world
            .query::<FactionReputation>()
            .ok_or_else(|| "FactionReputation storage is unavailable".to_owned())?;
        let mut staged = BTreeMap::<EntityId, FactionReputation>::new();
        for command in commands {
            let reputation = resolver
                .global_form_id(command.reputation)
                .ok_or_else(|| "portable REPU identity is not loaded".to_owned())?;
            let state = if let Some(state) = staged.get_mut(&command.entity) {
                state
            } else {
                let state = live
                    .get(command.entity)
                    .cloned()
                    .ok_or_else(|| "reputation target no longer carries state".to_owned())?;
                staged.entry(command.entity).or_insert(state)
            };
            match command.operation {
                ReputationOperation::AddFame => state.add_fame(reputation, command.points),
                ReputationOperation::AddInfamy => state.add_infamy(reputation, command.points),
                ReputationOperation::Reset => state.reset(reputation),
            }
        }
        drop(live);
        drop(resolver);
        let mut live = world
            .query_mut::<FactionReputation>()
            .ok_or_else(|| "FactionReputation storage disappeared before commit".to_owned())?;
        for (entity, state) in staged {
            let target = live
                .get_mut(entity)
                .ok_or_else(|| "reputation target disappeared before commit".to_owned())?;
            *target = state;
        }
        Ok(())
    })();
    if let Err(error) = apply {
        host.record_host_fault(format!("deferred reputation batch rejected: {error}"));
    }
}

fn apply_pending_world_commands(world: &World, host: &mut ExtensionHost) {
    apply_pending_actor_value_writes(world, host);
    apply_pending_package_evaluations(world, host);
    apply_pending_animation_commands(world, host);
    apply_pending_reputation_writes(world, host);
}

fn entities_by_form(world: &World) -> BTreeMap<FormRef, EntityId> {
    forms_by_entity(world)
        .into_iter()
        .map(|(entity, form)| (form, entity))
        .collect()
}

fn decode_saved_state(
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<ExtensionStateSnapshot> {
    snapshot
        .resources
        .get(EXTENSION_STATE_RESOURCE)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(|saved| saved.unwrap_or_default())
        .map_err(|error| anyhow::anyhow!("invalid {EXTENSION_STATE_RESOURCE} payload: {error}"))
}

/// Add engine-owned extension state to the checksummed ByroRedux snapshot.
///
/// This refuses a lossy save when a guest attached state to an entity that
/// lacks stable authored identity; transient SDK handles are never serialized.
pub(crate) fn capture_extension_state(
    world: &World,
    snapshot: &mut byroredux_save::Snapshot,
) -> anyhow::Result<usize> {
    let forms = forms_by_entity(world);
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return Ok(0);
        };
        slot.host()
    };
    let Some(host) = host else {
        return Ok(0);
    };
    let saved = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .capture_saved_state(&forms)?;
    let row_count = saved.rows.len();
    snapshot.resources.insert(
        EXTENSION_STATE_RESOURCE.to_owned(),
        serde_json::to_value(saved)
            .map_err(|error| anyhow::anyhow!("could not encode extension state: {error}"))?,
    );
    Ok(row_count)
}

/// Validate extension payloads before the live loader tears down the world.
pub(crate) fn preflight_extension_state(
    world: &World,
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<()> {
    let saved = decode_saved_state(snapshot)?;
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            if saved.rows.is_empty() && saved.principal_storage.is_empty() {
                return Ok(());
            }
            anyhow::bail!("save contains extension state but the extension host slot is absent");
        };
        slot.host()
    };
    let Some(host) = host else {
        if saved.rows.is_empty() && saved.principal_storage.is_empty() {
            return Ok(());
        }
        anyhow::bail!("save contains extension state but the extension host is unavailable");
    };
    host.lock()
        .expect("ExtensionHost mutex poisoned by a host panic")
        .validate_saved_state(&saved)?;
    Ok(())
}

/// Rebind saved form-backed rows after a successful world replacement.
pub(crate) fn restore_extension_state(
    world: &World,
    snapshot: &byroredux_save::Snapshot,
) -> anyhow::Result<usize> {
    let saved = decode_saved_state(snapshot)?;
    let entities = entities_by_form(world);
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            if saved.rows.is_empty() && saved.principal_storage.is_empty() {
                return Ok(0);
            }
            anyhow::bail!("save contains extension state but the extension host slot is absent");
        };
        slot.host()
    };
    let Some(host) = host else {
        if saved.rows.is_empty() && saved.principal_storage.is_empty() {
            return Ok(0);
        }
        anyhow::bail!("save contains extension state but the extension host is unavailable");
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    let generation = host.restore_saved_state(&saved, &entities)?;
    let retained = host.retained_rows.len();
    let retained_storage = host.retained_storage.len();
    log::debug!("extension entity-handle generation is now {generation}");
    if retained != 0 {
        log::info!("retained {retained} extension state row(s) for unavailable packages or forms");
    }
    if retained_storage != 0 {
        log::info!(
            "retained {retained_storage} principal storage record(s) for unavailable packages"
        );
    }
    Ok(saved.rows.len().saturating_sub(retained))
}

/// Stop all executable components without holding an ECS resource guard.
pub(crate) fn shutdown_extension_host(world: &World) {
    let host = {
        let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
            return;
        };
        slot.host()
    };
    let Some(host) = host else {
        return;
    };
    let diagnostics = {
        let mut host = host
            .lock()
            .expect("ExtensionHost mutex poisoned by a host panic");
        host.shutdown_all();
        host.take_diagnostics()
    };
    emit_diagnostics(diagnostics);
}

#[cfg(test)]
mod tests {
    use semver::{Version, VersionReq};

    use super::*;
    use byroredux_core::form_id::{LocalFormId, PluginId};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_sdk::component::{ComponentFieldDeclaration, ExtensionValue, ExtensionValueType};
    use byroredux_sdk::identity::{
        CapabilityId, ComponentFieldId, ComponentSchemaId, EventId, ServiceId, StorageKey,
    };
    use byroredux_sdk::manifest::{
        CapabilityRequest, ComponentSchemaDeclaration, EventFilter, EventSubscription,
        ExecutableComponent, EXTENSION_MANIFEST_VERSION,
    };
    use byroredux_sdk::service::{
        COMPONENTS_WRITE_OWN_CAPABILITY, EXTENSION_WORLD_SERVICE, STORAGE_READ_OWN_CAPABILITY,
        STORAGE_WRITE_OWN_CAPABILITY, WORLD_ENTITY_READ_CAPABILITY,
        WORLD_TRANSFORM_READ_CAPABILITY,
    };
    use byroredux_sdk::storage::{PrincipalStorageCommand, PrincipalStorageValue};

    const COMPONENT: &str = r#"
(component
  (import "byro:mod-host/state@0.1.0" (instance $state
    (type $entity-ref-shape (record
      (field "world-generation" u64)
      (field "object" u64)))
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
    (type $form-ref-shape (record
      (field "source-high" u64)
      (field "source-low" u64)
      (field "local" u32)))
    (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
    (type $hit-details-shape (record
      (field "damage" f32)
      (field "power-attack" bool)
      (field "sneak-attack" bool)
      (field "bash-attack" bool)
      (field "blocked" bool)))
    (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
    (export "queue-increment-own-i64" (func
      (param "entity" $entity-ref-in)
      (param "schema-index" u32)
      (param "field-index" u32)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
  (alias export $state "form-ref" (type $form-ref))
  (alias export $state "hit-details" (type $hit-details))
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
  (alias export $state "queue-increment-own-i64" (func $increment))
  (core func $increment-lower (canon lower (func $increment)))
  (core module $guest
    (import "host" "increment" (func $increment (param i64 i64 i32 i32 i64)))
    (func (export "initialize"))
    (func (export "shutdown"))
    (func (export "on-activate")
      (param $world i64) (param $object i64) (param i32 i64 i64)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-cell-load") (param $world i64) (param $object i64)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-hit")
      (param $world i64) (param $object i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-equipment-change")
      (param $world i64) (param $object i64)
      (param i64 i64 i32 i32)
      local.get $world
      local.get $object
      i32.const 0
      i32.const 0
      i64.const 1
      call $increment)
    (func (export "on-input-action") (param i32 i32))
    (func (export "on-session-event") (param i32 i32 i32))
    (func (export "on-custom-event") (param i32))
    (func (export "on-update") (param f32))
    (func (export "on-console-command") (param i32))
  )
  (core instance $guest-instance (instantiate $guest
    (with "host" (instance (export "increment" (func $increment-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
)
"#;

    const STORAGE_COMPONENT: &str = r#"
(component
  (import "byro:mod-host/state@0.1.0" (instance $state
    (type $entity-ref-shape (record
      (field "world-generation" u64)
      (field "object" u64)))
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
    (type $form-ref-shape (record
      (field "source-high" u64)
      (field "source-low" u64)
      (field "local" u32)))
    (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
    (type $hit-details-shape (record
      (field "damage" f32)
      (field "power-attack" bool)
      (field "sneak-attack" bool)
      (field "bash-attack" bool)
      (field "blocked" bool)))
    (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
  ))
  (import "byro:mod-host/storage@0.1.0" (instance $storage
    (export "queue-increment-i64" (func
      (param "key" string)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
  (alias export $state "form-ref" (type $form-ref))
  (alias export $state "hit-details" (type $hit-details))
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
  (alias export $storage "queue-increment-i64" (func $increment))
  (core module $libc
    (memory (export "memory") 1)
    (func (export "realloc") (param i32 i32 i32 i32) (result i32)
      unreachable)
  )
  (core instance $libc (instantiate $libc))
  (core func $increment-lower
    (canon lower (func $increment)
      (memory $libc "memory")
      (realloc (func $libc "realloc")))
  )
  (core module $guest
    (import "libc" "memory" (memory 1))
    (import "host" "increment" (func $increment (param i32 i32 i64)))
    (data (i32.const 0) "activation-count")
    (func (export "initialize"))
    (func (export "shutdown"))
    (func (export "on-activate") (param i64 i64 i32 i64 i64)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-cell-load") (param i64 i64))
    (func (export "on-hit")
      (param i64 i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32))
    (func (export "on-equipment-change")
      (param i64 i64 i64 i64 i32 i32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-input-action")
      (param $action i32) (param $phase i32)
      local.get $action
      i32.const 6
      i32.ne
      if
        unreachable
      end
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-session-event")
      (param $phase i32) (param $slot-tag i32) (param $slot i32)
      local.get $phase
      i32.const 2
      i32.ne
      local.get $slot-tag
      i32.const 1
      i32.ne
      i32.or
      local.get $slot
      i32.const 7
      i32.ne
      i32.or
      if
        unreachable
      end
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-custom-event") (param $subscription-index i32)
      local.get $subscription-index
      i32.eqz
      if
        i32.const 0
        i32.const 16
        i64.const 1
        call $increment
      else
        unreachable
      end)
    (func (export "on-update") (param f32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
    (func (export "on-console-command") (param i32)
      i32.const 0
      i32.const 16
      i64.const 1
      call $increment)
  )
  (core instance $guest-instance (instantiate $guest
    (with "libc" (instance $libc))
    (with "host" (instance (export "increment" (func $increment-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
)
"#;

    const PROJECTION_COMPONENT: &str = r#"
(component
  (type $entity-ref-shape (record
    (field "world-generation" u64)
    (field "object" u64)))
  (import "byro:mod-host/state@0.1.0" (instance $state
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
    (type $form-ref-shape (record
      (field "source-high" u64)
      (field "source-low" u64)
      (field "local" u32)))
    (export "form-ref" (type $form-ref-in (eq $form-ref-shape)))
    (type $hit-details-shape (record
      (field "damage" f32)
      (field "power-attack" bool)
      (field "sneak-attack" bool)
      (field "bash-attack" bool)
      (field "blocked" bool)))
    (export "hit-details" (type $hit-details-in (eq $hit-details-shape)))
    (type $input-action-shape (enum
      "move-forward" "move-backward" "strafe-left" "strafe-right"
      "jump" "sprint" "activate" "attack" "block" "inventory"
      "quicksave" "quickload" "pause"))
    (export "input-action" (type $input-action-in (eq $input-action-shape)))
    (type $input-phase-shape (enum "pressed" "released"))
    (export "input-phase" (type $input-phase-in (eq $input-phase-shape)))
    (type $session-phase-shape (enum "new-game" "save-complete" "load-complete"))
    (export "session-phase" (type $session-phase-in (eq $session-phase-shape)))
  ))
  (import "byro:mod-host/world-state@0.1.0" (instance $world
    (export "entity-ref" (type $entity-ref-world (eq $entity-ref-shape)))
    (export "contains-entity" (func
      (param "entity" $entity-ref-world)
      (result bool)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
  (alias export $state "form-ref" (type $form-ref))
  (alias export $state "hit-details" (type $hit-details))
  (alias export $state "input-action" (type $input-action))
  (alias export $state "input-phase" (type $input-phase))
  (alias export $state "session-phase" (type $session-phase))
  (alias export $world "contains-entity" (func $contains))
  (core func $contains-lower (canon lower (func $contains)))
  (core module $guest
    (import "host" "contains" (func $contains (param i64 i64) (result i32)))
    (func (export "initialize"))
    (func (export "shutdown"))
    (func (export "on-activate")
      (param $world i64) (param $object i64) (param i32 i64 i64)
      local.get $world
      local.get $object
      call $contains
      i32.eqz
      if
        unreachable
      end)
    (func (export "on-cell-load") (param i64 i64))
    (func (export "on-hit")
      (param i64 i64)
      (param i32 i64 i64) (param i32 i64 i64) (param i32 i64 i64)
      (param f32 i32 i32 i32 i32))
    (func (export "on-equipment-change")
      (param $world i64) (param $object i64)
      (param i64 i64 i32 i32)
      local.get $world
      local.get $object
      call $contains
      i32.eqz
      if
        unreachable
      end)
    (func (export "on-input-action") (param i32 i32))
    (func (export "on-session-event") (param i32 i32 i32))
    (func (export "on-custom-event") (param i32))
    (func (export "on-update") (param f32))
    (func (export "on-console-command") (param i32))
  )
  (core instance $guest-instance (instantiate $guest
    (with "host" (instance (export "contains" (func $contains-lower))))
  ))
  (func (export "initialize") (canon lift (core func $guest-instance "initialize")))
  (func (export "shutdown") (canon lift (core func $guest-instance "shutdown")))
  (func (export "on-activate")
    (param "subject" $entity-ref)
    (param "activator" (option $entity-ref))
    (canon lift (core func $guest-instance "on-activate")))
  (func (export "on-cell-load")
    (param "subject" $entity-ref)
    (canon lift (core func $guest-instance "on-cell-load")))
  (func (export "on-hit")
    (param "subject" $entity-ref)
    (param "aggressor" (option $entity-ref))
    (param "source" (option $entity-ref))
    (param "projectile" (option $entity-ref))
    (param "details" $hit-details)
    (canon lift (core func $guest-instance "on-hit")))
  (func (export "on-equipment-change")
    (param "wearer" $entity-ref)
    (param "item" $form-ref)
    (param "equipped" bool)
    (canon lift (core func $guest-instance "on-equipment-change")))
  (func (export "on-input-action")
    (param "action" $input-action)
    (param "phase" $input-phase)
    (canon lift (core func $guest-instance "on-input-action")))
  (func (export "on-session-event")
    (param "phase" $session-phase)
    (param "slot" (option u32))
    (canon lift (core func $guest-instance "on-session-event")))
  (func (export "on-custom-event")
    (param "subscription-index" u32)
    (canon lift (core func $guest-instance "on-custom-event")))
  (func (export "on-update")
    (param "elapsed-seconds" f32)
    (canon lift (core func $guest-instance "on-update")))
  (func (export "on-console-command")
    (param "command-index" u32)
    (canon lift (core func $guest-instance "on-console-command")))
)
"#;

    fn manifest(id: &str) -> ExtensionManifest {
        ExtensionManifest {
            manifest_version: EXTENSION_MANIFEST_VERSION,
            id: ExtensionId::new(id).unwrap(),
            name: id.to_owned(),
            version: Version::new(1, 0, 0),
            sdk: VersionReq::parse("^0.1").unwrap(),
            dependencies: Vec::new(),
            components: vec![ExecutableComponent {
                id: ComponentId::new("runtime").unwrap(),
                path: "runtime.wasm".to_owned(),
                world: ServiceId::new(EXTENSION_WORLD_SERVICE).unwrap(),
                world_version: VersionReq::parse("^0.1").unwrap(),
            }],
            capabilities: vec![
                CapabilityRequest {
                    id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
                    required: true,
                },
                CapabilityRequest {
                    id: CapabilityId::new(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap(),
                    required: true,
                },
            ],
            subscriptions: vec![EventSubscription {
                event: EventId::new(ACTIVATE_EVENT).unwrap(),
                filters: Vec::new(),
                interval_millis: None,
            }],
            component_schemas: vec![ComponentSchemaDeclaration {
                id: ComponentSchemaId::new("example.activation-count").unwrap(),
                version: 1,
                fields: vec![ComponentFieldDeclaration {
                    id: ComponentFieldId::new("count").unwrap(),
                    value_type: ExtensionValueType::I64,
                }],
            }],
            console_commands: Vec::new(),
            settings: Vec::new(),
            principal_storage_schema: None,
        }
    }

    fn grants() -> CapabilitySet {
        let mut grants = CapabilitySet::new();
        grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
        grants
    }

    fn cell_load_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(CELL_LOAD_EVENT).unwrap(),
            filters: Vec::new(),
            interval_millis: None,
        }];
        manifest
    }

    fn hit_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(HIT_EVENT).unwrap(),
            filters: Vec::new(),
            interval_millis: None,
        }];
        manifest
    }

    fn equipment_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(EQUIPMENT_EVENT).unwrap(),
            filters: Vec::new(),
            interval_millis: None,
        }];
        manifest
    }

    fn input_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = storage_manifest(id);
        manifest.capabilities.push(CapabilityRequest {
            id: CapabilityId::new(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY).unwrap(),
            required: true,
        });
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(INPUT_ACTION_EVENT).unwrap(),
            filters: vec![EventFilter {
                field: ServiceId::new(byroredux_sdk::service::INPUT_ACTION_FILTER_FIELD).unwrap(),
                equals: "activate".to_owned(),
            }],
            interval_millis: None,
        }];
        manifest
    }

    fn session_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = storage_manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(SESSION_EVENT).unwrap(),
            filters: vec![EventFilter {
                field: ServiceId::new(byroredux_sdk::service::SESSION_PHASE_FILTER_FIELD).unwrap(),
                equals: "load-complete".to_owned(),
            }],
            interval_millis: None,
        }];
        manifest
    }

    fn custom_event_manifest(id: &str, event: &str) -> ExtensionManifest {
        let mut manifest = storage_manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(event).unwrap(),
            filters: Vec::new(),
            interval_millis: None,
        }];
        manifest
    }

    fn storage_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = manifest(id);
        manifest.component_schemas.clear();
        manifest.principal_storage_schema = Some(1);
        manifest.capabilities = vec![
            CapabilityRequest {
                id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
                required: true,
            },
            CapabilityRequest {
                id: CapabilityId::new(STORAGE_READ_OWN_CAPABILITY).unwrap(),
                required: true,
            },
            CapabilityRequest {
                id: CapabilityId::new(STORAGE_WRITE_OWN_CAPABILITY).unwrap(),
                required: true,
            },
        ];
        manifest
    }

    fn console_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = storage_manifest(id);
        manifest.capabilities.push(CapabilityRequest {
            id: CapabilityId::new(CONSOLE_REGISTER_CAPABILITY).unwrap(),
            required: true,
        });
        manifest.console_commands = vec![byroredux_sdk::console::ConsoleCommandDeclaration {
            id: byroredux_sdk::identity::ConsoleCommandId::new("bump").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            description: "Increment the test counter".to_owned(),
        }];
        manifest
    }

    fn update_manifest(id: &str) -> ExtensionManifest {
        let mut manifest = storage_manifest(id);
        manifest.subscriptions = vec![EventSubscription {
            event: EventId::new(UPDATE_EVENT).unwrap(),
            filters: Vec::new(),
            interval_millis: Some(100),
        }];
        manifest
    }

    fn storage_grants() -> CapabilitySet {
        let mut grants = CapabilitySet::new();
        grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
        grants
    }

    fn console_grants() -> CapabilitySet {
        let mut grants = storage_grants();
        grants.grant(CONSOLE_REGISTER_CAPABILITY).unwrap();
        grants
    }

    fn install_storage_package(host: &mut ExtensionHost, id: &str) {
        let manifest = storage_manifest(id);
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        host.install_package(&manifest, &artifacts, storage_grants())
            .unwrap();
    }

    fn host_with_storage_package(id: &str) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        install_storage_package(&mut host, id);
        host
    }

    fn host_with_update_package(id: &str) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        host.install_package(&update_manifest(id), &artifacts, storage_grants())
            .unwrap();
        host
    }

    fn host_with_projection_package(id: &str) -> ExtensionHost {
        let mut manifest = manifest(id);
        manifest.component_schemas.clear();
        manifest.capabilities = vec![
            CapabilityRequest {
                id: CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY).unwrap(),
                required: true,
            },
            CapabilityRequest {
                id: CapabilityId::new(WORLD_ENTITY_READ_CAPABILITY).unwrap(),
                required: true,
            },
            CapabilityRequest {
                id: CapabilityId::new(WORLD_TRANSFORM_READ_CAPABILITY).unwrap(),
                required: true,
            },
        ];
        let mut grants = CapabilitySet::new();
        grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        grants.grant(WORLD_ENTITY_READ_CAPABILITY).unwrap();
        grants.grant(WORLD_TRANSFORM_READ_CAPABILITY).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(PROJECTION_COMPONENT).unwrap(),
        );
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        host.install_package(&manifest, &artifacts, grants).unwrap();
        host
    }

    fn host_with_package(id: &str) -> ExtensionHost {
        host_with_manifest(manifest(id))
    }

    fn host_with_cell_load_package(id: &str) -> ExtensionHost {
        host_with_manifest(cell_load_manifest(id))
    }

    fn host_with_hit_package(id: &str) -> ExtensionHost {
        host_with_manifest(hit_manifest(id))
    }

    fn host_with_equipment_package(id: &str) -> ExtensionHost {
        host_with_manifest(equipment_manifest(id))
    }

    fn host_with_input_package(id: &str) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        let mut grants = storage_grants();
        grants.grant(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY).unwrap();
        host.install_package(&input_manifest(id), &artifacts, grants)
            .unwrap();
        host
    }

    fn host_with_session_package(id: &str) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        host.install_package(&session_manifest(id), &artifacts, storage_grants())
            .unwrap();
        host
    }

    fn host_with_custom_event_package(id: &str, event: &str) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        host.install_package(
            &custom_event_manifest(id, event),
            &artifacts,
            storage_grants(),
        )
        .unwrap();
        host
    }

    fn host_with_manifest(manifest: ExtensionManifest) -> ExtensionHost {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(COMPONENT).unwrap(),
        );
        host.install_package(&manifest, &artifacts, grants())
            .unwrap();
        host
    }

    fn empty_snapshot() -> byroredux_save::Snapshot {
        byroredux_save::Snapshot {
            next_entity: 0,
            strings: Vec::new(),
            components: BTreeMap::new(),
            resources: BTreeMap::new(),
        }
    }

    fn form_pair_for_test() -> FormIdPair {
        FormIdPair {
            plugin: PluginId(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210),
            local: LocalFormId(0x123456),
        }
    }

    fn world_with_form_and_host(slot: ExtensionHostSlot, pair: FormIdPair) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<FormIdComponent>();
        let entity = world.spawn();
        let mut pool = FormIdPool::new();
        let form = pool.intern(pair);
        world.insert(entity, FormIdComponent(form));
        world.insert_resource(pool);
        world.insert_resource(slot);
        (world, entity)
    }

    #[test]
    fn live_host_delivers_activation_and_commits_owned_state() {
        let mut host = host_with_package("org.example.live");
        let stats = host.dispatch_activations([RawActivation {
            subject: 41,
            subject_form: None,
            activator: Some(1),
            activator_form: None,
        }]);
        assert_eq!(
            stats,
            ExtensionDispatchStats {
                events: 1,
                deliveries: 1,
                commands_applied: 1,
                faults: 0,
            }
        );

        let subject = host.handles.by_entity[&41];
        let owner = PrincipalId::new("org.example.live").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
        assert_eq!(host.package_count(), 1);
        assert_eq!(host.component_count(), 1);
    }

    #[test]
    fn live_host_delivers_cell_load_only_to_its_declared_subscriber() {
        let mut host = host_with_cell_load_package("org.example.live-cell-load");
        let stats = host.dispatch_cell_loads([RawCellLoad {
            subject: 41,
            subject_form: None,
        }]);
        assert_eq!(
            stats,
            ExtensionDispatchStats {
                events: 1,
                deliveries: 1,
                commands_applied: 1,
                faults: 0,
            }
        );

        let subject = host.handles.by_entity[&41];
        let owner = PrincipalId::new("org.example.live-cell-load").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
        assert_eq!(
            host.dispatch_activations([RawActivation {
                subject: 41,
                subject_form: None,
                activator: None,
                activator_form: None,
            }]),
            ExtensionDispatchStats {
                events: 1,
                ..ExtensionDispatchStats::default()
            }
        );
    }

    #[test]
    fn live_host_delivers_hit_payload_to_declared_subscriber() {
        let mut host = host_with_hit_package("org.example.live-hit");
        let stats = host.dispatch_hits([RawHit {
            subject: 41,
            subject_form: None,
            aggressor: Some(7),
            aggressor_form: None,
            source: Some(7),
            source_form: None,
            projectile: None,
            projectile_form: None,
            damage: 12.5,
            power_attack: true,
            sneak_attack: false,
            bash_attack: true,
            blocked: false,
        }]);
        assert_eq!(
            stats,
            ExtensionDispatchStats {
                events: 1,
                deliveries: 1,
                commands_applied: 1,
                faults: 0,
            }
        );
        let subject = host.handles.by_entity[&41];
        let owner = PrincipalId::new("org.example.live-hit").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
    }

    #[test]
    fn live_host_delivers_equipment_changes_only_to_declared_subscriber() {
        let mut host = host_with_equipment_package("org.example.live-equipment");
        let item = FormRef::new([0x5A; 16], 0x1234);
        let stats = host.dispatch_equipment_changes([RawEquipmentChange {
            wearer: 41,
            wearer_form: None,
            item,
            equipped: true,
        }]);
        assert_eq!(
            stats,
            ExtensionDispatchStats {
                events: 1,
                deliveries: 1,
                commands_applied: 1,
                faults: 0,
            }
        );
        let wearer = host.handles.by_entity[&41];
        let owner = PrincipalId::new("org.example.live-equipment").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, wearer)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );

        let mut unsubscribed = host_with_package("org.example.no-equipment");
        assert_eq!(
            unsubscribed.dispatch_equipment_changes([RawEquipmentChange {
                wearer: 41,
                wearer_form: None,
                item,
                equipped: false,
            }]),
            ExtensionDispatchStats {
                events: 1,
                ..ExtensionDispatchStats::default()
            }
        );
    }

    #[test]
    fn live_host_applies_normalized_input_action_filters_before_guest_delivery() {
        let principal = PrincipalId::new("org.example.live-input").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut host = host_with_input_package(principal.as_str());
        let stats = host.dispatch_input_actions([
            InputActionEvent {
                action: SdkInputAction::Inventory,
                phase: InputPhase::Pressed,
            },
            InputActionEvent {
                action: SdkInputAction::Activate,
                phase: InputPhase::Pressed,
            },
            InputActionEvent {
                action: SdkInputAction::Activate,
                phase: InputPhase::Released,
            },
        ]);
        assert_eq!(stats.events, 3);
        assert_eq!(stats.deliveries, 2);
        assert_eq!(stats.commands_applied, 2);
        assert_eq!(stats.faults, 0);
        assert_eq!(
            host.principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(2))
        );
    }

    #[test]
    fn live_host_applies_session_phase_filters_before_guest_delivery() {
        let principal = PrincipalId::new("org.example.live-session").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut host = host_with_session_package(principal.as_str());
        let stats = host.dispatch_session_events([
            SessionEvent {
                phase: SessionPhase::SaveComplete,
                slot: Some(7),
            },
            SessionEvent {
                phase: SessionPhase::LoadComplete,
                slot: Some(7),
            },
        ]);
        assert_eq!(stats.events, 2);
        assert_eq!(stats.deliveries, 1);
        assert_eq!(stats.commands_applied, 1);
        assert_eq!(stats.faults, 0);
        assert_eq!(
            host.principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );
    }

    #[test]
    fn custom_events_route_by_exact_channel_on_a_later_dispatch_pass() {
        let subscriber = PrincipalId::new("org.example.subscriber").unwrap();
        let sender = PrincipalId::new("org.example.publisher").unwrap();
        let channel = EventId::new("mod.org.example.publisher.event.ready").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut host = host_with_custom_event_package(subscriber.as_str(), channel.as_str());

        host.pending_custom_events.push(CustomEvent {
            event: channel,
            sender,
            payload: vec![1, 2, 3],
        });
        assert!(host
            .principal_storage
            .values(&subscriber)
            .and_then(|values| values.get(&key))
            .is_none());

        let stats = host.dispatch_custom_events();
        assert_eq!(
            stats,
            ExtensionDispatchStats {
                events: 1,
                deliveries: 1,
                commands_applied: 1,
                faults: 0,
            }
        );
        assert_eq!(
            host.principal_storage
                .values(&subscriber)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );

        host.pending_custom_events.push(CustomEvent {
            event: EventId::new("mod.org.example.other.event.ready").unwrap(),
            sender: PrincipalId::new("org.example.other").unwrap(),
            payload: Vec::new(),
        });
        assert_eq!(
            host.dispatch_custom_events(),
            ExtensionDispatchStats {
                events: 1,
                ..ExtensionDispatchStats::default()
            }
        );
    }

    #[test]
    fn custom_event_queue_overflow_rejects_the_entire_deferred_batch() {
        let mut host = host_with_package("org.example.atomic");
        let principal = PrincipalId::new("org.example.atomic").unwrap();
        let event = EventId::new("mod.org.example.atomic.event.ready").unwrap();
        host.pending_custom_events = (0..MAX_PENDING_CUSTOM_EVENTS)
            .map(|_| CustomEvent {
                event: event.clone(),
                sender: principal.clone(),
                payload: Vec::new(),
            })
            .collect();
        let entity = EntityRef::new(1, 99).unwrap();
        let commands = vec![
            HostCommand::Component(byroredux_sdk::component::ExtensionCommand::IncrementI64 {
                entity,
                schema: ComponentSchemaId::new("example.activation-count").unwrap(),
                field: ComponentFieldId::new("count").unwrap(),
                delta: 1,
            }),
            HostCommand::PublishEvent(
                byroredux_sdk::event::PublishEventCommand::new(event, Vec::new()).unwrap(),
            ),
        ];
        let mut stats = ExtensionDispatchStats::default();
        let hosted = &mut host.components[0];
        apply_delivery_result(
            hosted,
            Ok(commands),
            LifecyclePhase::Activate,
            &principal,
            DeliveryCommitContext {
                state: &mut host.state,
                principal_storage: &mut host.principal_storage,
                pending_custom_events: &mut host.pending_custom_events,
                pending_setting_writes: &mut host.pending_setting_writes,
                pending_actor_value_writes: &mut host.pending_actor_value_writes,
                pending_package_evaluations: &mut host.pending_package_evaluations,
                pending_animation_commands: &mut host.pending_animation_commands,
                pending_reputation_writes: &mut host.pending_reputation_writes,
                diagnostics: &mut host.diagnostics,
                stats: &mut stats,
            },
        );

        assert_eq!(host.pending_custom_events.len(), MAX_PENDING_CUSTOM_EVENTS);
        assert!(host
            .state
            .row(
                &principal,
                &ComponentSchemaId::new("example.activation-count").unwrap(),
                entity,
            )
            .is_none());
        assert_eq!(stats.commands_applied, 0);
        assert_eq!(stats.faults, 1);
        assert!(matches!(
            host.components[0].instance.status(),
            InstanceStatus::Quarantined(_)
        ));
    }

    #[test]
    fn invalid_hit_damage_is_rejected_before_guest_delivery() {
        let mut host = host_with_hit_package("org.example.invalid-hit");
        let stats = host.dispatch_hits([RawHit {
            subject: 41,
            subject_form: None,
            aggressor: Some(7),
            aggressor_form: None,
            source: None,
            source_form: None,
            projectile: None,
            projectile_form: None,
            damage: f32::NAN,
            power_attack: false,
            sneak_attack: false,
            bash_attack: false,
            blocked: false,
        }]);
        assert_eq!(stats.events, 1);
        assert_eq!(stats.deliveries, 0);
        assert_eq!(stats.commands_applied, 0);
        assert_eq!(stats.faults, 1);
        assert_eq!(
            host.components[0].instance.status(),
            &InstanceStatus::Active
        );
    }

    #[test]
    fn recurring_update_waits_full_interval_and_retains_overshoot() {
        let principal = PrincipalId::new("org.example.update").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut host = host_with_update_package(principal.as_str());

        assert_eq!(
            host.dispatch_updates(0.04),
            ExtensionDispatchStats::default()
        );
        assert_eq!(
            host.dispatch_updates(0.04),
            ExtensionDispatchStats::default()
        );
        assert_eq!(host.dispatch_updates(0.04).commands_applied, 1);
        assert_eq!(
            host.principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );

        assert_eq!(host.dispatch_updates(0.35).deliveries, 1);
        assert_eq!(host.dispatch_updates(0.0).deliveries, 1);
        assert_eq!(
            host.principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(3))
        );
    }

    #[test]
    fn invalid_update_delta_is_a_host_fault_without_guest_quarantine() {
        let mut host = host_with_update_package("org.example.invalid-update");
        let stats = host.dispatch_updates(f32::NAN);

        assert_eq!(stats.faults, 1);
        assert_eq!(stats.deliveries, 0);
        assert_eq!(
            host.components[0].instance.status(),
            &InstanceStatus::Active
        );
        assert!(matches!(
            host.take_diagnostics().as_slice(),
            [ExtensionDiagnostic::Fault { extension, .. }]
                if extension.as_str() == "byro.engine"
        ));
    }

    #[test]
    fn scheduler_update_adapter_advances_engine_owned_cadence() {
        let principal = PrincipalId::new("org.example.update-adapter").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let slot = ExtensionHostSlot::from_host(host_with_update_package(principal.as_str()));
        let host = slot.host().unwrap();
        let mut world = World::new();
        world.insert_resource(slot);

        extension_update_dispatch_system(&world, 0.05);
        extension_update_dispatch_system(&world, 0.05);

        assert_eq!(
            host.lock()
                .unwrap()
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );
    }

    #[test]
    fn principal_storage_is_live_private_and_save_persistent() {
        let principal = PrincipalId::new("org.example.storage-save").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut source = host_with_storage_package(principal.as_str());
        let activation = RawActivation {
            subject: 41,
            subject_form: None,
            activator: None,
            activator_form: None,
        };
        assert_eq!(
            source.dispatch_activations([activation]).commands_applied,
            1
        );
        assert_eq!(
            source.dispatch_activations([activation]).commands_applied,
            1
        );
        assert_eq!(
            source
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(2))
        );
        source
            .principal_storage
            .apply_batch(
                &principal,
                &[
                    PrincipalStorageCommand::ArrayPush {
                        key: StorageKey::new("history").unwrap(),
                        value: ExtensionValue::String("Helgen".to_owned()),
                    },
                    PrincipalStorageCommand::MapSet {
                        key: StorageKey::new("aliases").unwrap(),
                        entry: "player".to_owned(),
                        value: ExtensionValue::String("Dragonborn".to_owned()),
                    },
                    PrincipalStorageCommand::SetInsert {
                        key: StorageKey::new("visited").unwrap(),
                        value: ExtensionValue::U64(7),
                    },
                ],
            )
            .unwrap();

        let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();
        assert_eq!(saved.principal_storage.len(), 1);
        let mut restored = host_with_storage_package(principal.as_str());
        restored
            .restore_saved_state(&saved, &BTreeMap::new())
            .unwrap();
        assert_eq!(
            restored
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&StorageKey::new("history").unwrap())),
            Some(&PrincipalStorageValue::Array(vec![ExtensionValue::String(
                "Helgen".to_owned()
            )]))
        );
        assert_eq!(
            restored.dispatch_activations([activation]).commands_applied,
            1
        );
        assert_eq!(
            restored
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(3))
        );

        let mut unavailable =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        unavailable
            .restore_saved_state(&saved, &BTreeMap::new())
            .unwrap();
        assert_eq!(unavailable.retained_storage, saved.principal_storage);
        assert_eq!(
            unavailable
                .capture_saved_state(&BTreeMap::new())
                .unwrap()
                .principal_storage,
            saved.principal_storage
        );
        install_storage_package(&mut unavailable, principal.as_str());
        assert!(unavailable.retained_storage.is_empty());
        assert_eq!(
            unavailable
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(2))
        );
    }

    #[test]
    fn granted_manifest_console_command_registers_and_commits_deferred_state() {
        let id = "org.example.console";
        let manifest = console_manifest(id);
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        let mut extension_host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        extension_host
            .install_package(&manifest, &artifacts, console_grants())
            .unwrap();
        let slot = ExtensionHostSlot::from_host(extension_host);
        let host = slot.host().unwrap();
        let mut world = World::new();
        world.insert_resource(slot);
        let mut registry = CommandRegistry::new();
        register_console_commands(&world, &mut registry);
        assert_eq!(
            registry.list(),
            vec![("ext.org.example.console.bump", "Increment the test counter")]
        );
        world.insert_resource(registry);

        let output = world
            .resource::<CommandRegistry>()
            .execute(&world, "ext.org.example.console.bump ignored");
        assert_eq!(output.lines, vec!["OK"]);
        let principal = PrincipalId::new(id).unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        assert_eq!(
            host.lock()
                .unwrap()
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );
    }

    #[test]
    fn optional_denied_console_capability_publishes_no_engine_command() {
        let mut manifest = console_manifest("org.example.denied-console");
        manifest
            .capabilities
            .iter_mut()
            .find(|request| request.id.as_str() == CONSOLE_REGISTER_CAPABILITY)
            .unwrap()
            .required = false;
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(STORAGE_COMPONENT).unwrap(),
        );
        let mut extension_host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        extension_host
            .install_package(&manifest, &artifacts, storage_grants())
            .unwrap();
        let slot = ExtensionHostSlot::from_host(extension_host);
        let mut world = World::new();
        world.insert_resource(slot);
        let mut registry = CommandRegistry::new();
        register_console_commands(&world, &mut registry);
        assert!(registry.list().is_empty());
    }

    #[test]
    fn scheduler_adapter_snapshots_events_before_entering_the_host() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let subject_entity = world.spawn();
        let activator = world.spawn();
        world.insert(
            subject_entity,
            byroredux_scripting::ActivateEvent { activator },
        );
        let slot = ExtensionHostSlot::from_host(host_with_package("org.example.adapter"));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        extension_activation_dispatch_system(&world, 0.0);

        let host = host.lock().unwrap();
        let subject = host.handles.by_entity[&subject_entity];
        let owner = PrincipalId::new("org.example.adapter").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
        assert!(world.has::<byroredux_scripting::ActivateEvent>(subject_entity));
    }

    #[test]
    fn cell_load_adapter_delivers_before_shared_event_cleanup() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let subject_entity = world.spawn();
        world.insert(subject_entity, byroredux_scripting::OnCellLoadEvent);
        let slot =
            ExtensionHostSlot::from_host(host_with_cell_load_package("org.example.cell-load"));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        extension_cell_load_dispatch_system(&world, 0.0);

        let host = host.lock().unwrap();
        let subject = host.handles.by_entity[&subject_entity];
        let owner = PrincipalId::new("org.example.cell-load").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
        assert!(world.has::<byroredux_scripting::OnCellLoadEvent>(subject_entity));
    }

    #[test]
    fn hit_adapter_delivers_live_combat_marker_before_cleanup() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let subject = world.spawn();
        let aggressor = world.spawn();
        world.insert(
            subject,
            byroredux_scripting::HitEvent {
                aggressor,
                source: aggressor,
                projectile: 0,
                damage: 12.5,
                power_attack: true,
                sneak_attack: false,
                bash_attack: true,
                blocked: false,
            },
        );
        let slot = ExtensionHostSlot::from_host(host_with_hit_package("org.example.hit-adapter"));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        extension_hit_dispatch_system(&world, 0.0);

        let host = host.lock().unwrap();
        let subject_handle = host.handles.by_entity[&subject];
        let owner = PrincipalId::new("org.example.hit-adapter").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, subject_handle)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
        assert!(world.has::<byroredux_scripting::HitEvent>(subject));
    }

    #[test]
    fn equipment_adapter_resolves_items_and_preserves_batch_order_before_cleanup() {
        use crate::cell_loader::load_order::{GlobalFormIdResolver, LoadOrder};

        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let wearer = world.spawn();
        world.insert(
            wearer,
            byroredux_scripting::EquipmentEventBatch(vec![
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0x0112_3456,
                    equipped: false,
                },
                byroredux_scripting::EquipmentChange {
                    item_form_id: 0xFE00_5ABC,
                    equipped: true,
                },
            ]),
        );
        let order = LoadOrder::new(
            vec!["Base.esm".into(), "Gear.esp".into(), "Creation.esl".into()],
            vec![
                byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
                byroredux_plugin::esm::reader::GlobalSlot::Regular(1),
                byroredux_plugin::esm::reader::GlobalSlot::Light(5),
            ],
        );
        world.insert_resource(GlobalFormIdResolver::from_load_order(&order));
        let slot = ExtensionHostSlot::from_host(host_with_equipment_package(
            "org.example.equipment-adapter",
        ));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        extension_equipment_dispatch_system(&world, 0.0);

        let host = host.lock().unwrap();
        let wearer_handle = host.handles.by_entity[&wearer];
        let owner = PrincipalId::new("org.example.equipment-adapter").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, wearer_handle)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(2))
        );
        assert!(world.has::<byroredux_scripting::EquipmentEventBatch>(wearer));
    }

    #[test]
    fn input_adapter_observes_rebound_action_press_and_release_edges() {
        use winit::keyboard::KeyCode;

        let principal = PrincipalId::new("org.example.input-adapter").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut input = crate::components::InputState::default();
        input.keys_held.insert(KeyCode::KeyQ);
        let mut bindings = crate::interaction::ActionBindings::default();
        bindings.bind_key(KeyCode::KeyQ, crate::interaction::InputAction::Activate);
        let mut world = World::new();
        world.insert_resource(input);
        world.insert_resource(bindings);
        world.insert_resource(crate::interaction::ActionState::default());
        let slot = ExtensionHostSlot::from_host(host_with_input_package(principal.as_str()));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        crate::interaction::refresh_action_state(&world);
        extension_input_dispatch_system(&world, 0.0);
        world
            .resource_mut::<crate::components::InputState>()
            .keys_held
            .clear();
        crate::interaction::refresh_action_state(&world);
        extension_input_dispatch_system(&world, 0.0);

        assert_eq!(
            host.lock()
                .unwrap()
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(2))
        );
    }

    #[test]
    fn session_adapter_drains_bounded_committed_events_outside_the_queue_guard() {
        let principal = PrincipalId::new("org.example.session-adapter").unwrap();
        let key = StorageKey::new("activation-count").unwrap();
        let mut world = World::new();
        world.insert_resource(SessionEventQueue::default());
        let slot = ExtensionHostSlot::from_host(host_with_session_package(principal.as_str()));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        queue_session_event(
            &world,
            SessionEvent {
                phase: SessionPhase::SaveComplete,
                slot: Some(7),
            },
        )
        .unwrap();
        queue_session_event(
            &world,
            SessionEvent {
                phase: SessionPhase::LoadComplete,
                slot: Some(7),
            },
        )
        .unwrap();
        extension_session_dispatch_system(&world, 0.0);

        assert_eq!(world.resource::<SessionEventQueue>().events.len(), 0);
        assert_eq!(
            host.lock()
                .unwrap()
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&PrincipalStorageValue::I64(1))
        );
    }

    #[test]
    fn session_event_queue_rejects_invalid_payloads_and_overflow() {
        let mut world = World::new();
        world.insert_resource(SessionEventQueue::default());
        assert_eq!(
            queue_session_event(
                &world,
                SessionEvent {
                    phase: SessionPhase::NewGame,
                    slot: Some(1),
                }
            ),
            Err("invalid session event payload")
        );
        for _ in 0..MAX_PENDING_SESSION_EVENTS {
            queue_session_event(
                &world,
                SessionEvent {
                    phase: SessionPhase::NewGame,
                    slot: None,
                },
            )
            .unwrap();
        }
        assert_eq!(
            queue_session_event(
                &world,
                SessionEvent {
                    phase: SessionPhase::NewGame,
                    slot: None,
                }
            ),
            Err("session event queue is full")
        );
    }

    #[test]
    fn scheduler_adapter_exposes_bounded_name_and_world_transform_projections() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Name>();
        world.register::<GlobalTransform>();
        let subject = world.spawn();
        let activator = world.spawn();
        let mut strings = StringPool::new();
        let subject_name = strings.intern("subject door");
        world.insert_resource(strings);
        world.insert(subject, Name(subject_name));
        world.insert(
            subject,
            GlobalTransform::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY, 2.0),
        );
        world.insert(subject, byroredux_scripting::ActivateEvent { activator });
        let slot = ExtensionHostSlot::from_host(host_with_projection_package(
            "org.example.projection-adapter",
        ));
        let host = slot.host().unwrap();
        world.insert_resource(slot);

        let raw = capture_entity_projections(&world, &BTreeSet::from([subject, activator]));
        assert_eq!(raw[&subject].name.as_deref(), Some("subject door"));
        assert_eq!(
            raw[&subject].world_transform.unwrap().translation(),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(raw[&subject].world_transform.unwrap().scale(), 2.0);
        assert_eq!(raw[&activator], RawEntityProjection::default());

        extension_activation_dispatch_system(&world, 0.0);

        let host = host.lock().unwrap();
        assert_eq!(
            host.components[0].instance.status(),
            &InstanceStatus::Active
        );
        assert!(host.handles.by_entity.contains_key(&subject));
        assert!(host.handles.by_entity.contains_key(&activator));
    }

    #[test]
    fn actor_value_projection_and_deferred_apply_use_portable_avif_identity_atomically() {
        let mut world = World::new();
        let actor = world.spawn();
        let mut values = ActorValues::new();
        values.set_base(0x333, 100.0);
        world.insert(actor, values);
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );
        let actor_value = FormRef::new(
            byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
                .0
                .to_be_bytes(),
            0x333,
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let captured = projections[&actor]
            .actor_values
            .as_ref()
            .unwrap()
            .iter()
            .find(|(form, _)| *form == actor_value)
            .unwrap()
            .1;
        assert_eq!(captured.current(), 100.0);

        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let handle = host.bind_entity(actor, None).unwrap();
        host.pending_actor_value_writes.push(
            ActorValueCommand::new(
                handle,
                actor_value,
                ActorValueOperation::ModifyPermanent,
                5.0,
            )
            .unwrap(),
        );
        apply_pending_actor_value_writes(&world, &mut host);
        assert_eq!(
            world
                .query::<ActorValues>()
                .unwrap()
                .get(actor)
                .unwrap()
                .current(0x333),
            105.0
        );

        host.pending_actor_value_writes.push(
            ActorValueCommand::new(handle, actor_value, ActorValueOperation::SetBase, 200.0)
                .unwrap(),
        );
        host.pending_actor_value_writes.push(
            ActorValueCommand::new(
                handle,
                FormRef::new([99; 16], 1),
                ActorValueOperation::SetBase,
                1.0,
            )
            .unwrap(),
        );
        apply_pending_actor_value_writes(&world, &mut host);
        assert_eq!(
            world
                .query::<ActorValues>()
                .unwrap()
                .get(actor)
                .unwrap()
                .current(0x333),
            105.0,
            "an unresolved command rejects the whole actor-value batch"
        );
        assert!(host.take_diagnostics().iter().any(|diagnostic| matches!(
            diagnostic,
            ExtensionDiagnostic::Fault { message, .. }
                if message.contains("deferred actor-value batch rejected")
        )));
    }

    #[test]
    fn inventory_projection_aggregates_portable_forms_and_equipment_slots() {
        let mut world = World::new();
        let actor = world.spawn();
        let mut inventory = Inventory::new();
        let first = inventory.push(byroredux_core::ecs::components::ItemStack::new(0x1234, 2));
        inventory.push(byroredux_core::ecs::components::ItemStack::new(0x1234, 5));
        let weapon = inventory.push(byroredux_core::ecs::components::ItemStack::new(0x5678, 1));
        inventory.push(byroredux_core::ecs::components::ItemStack::new(
            0x0100_0001,
            4,
        ));
        world.insert(actor, inventory);
        let mut equipment = EquipmentSlots::new();
        equipment.equip(0b101, first);
        equipment.equip_weapon(weapon);
        world.insert(actor, equipment);
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].inventory.as_ref().unwrap();
        assert!(
            snapshot.truncated(),
            "unresolved forms are reported, not hidden"
        );
        assert_eq!(snapshot.entries().len(), 2);
        let armor = snapshot
            .entries()
            .iter()
            .find(|entry| entry.item().local() == 0x1234)
            .unwrap();
        assert_eq!(armor.count(), 7);
        assert_eq!(armor.biped_slots(), 0b101);
        assert!(!armor.weapon_equipped());
        let weapon = snapshot
            .entries()
            .iter()
            .find(|entry| entry.item().local() == 0x5678)
            .unwrap();
        assert_eq!(weapon.count(), 1);
        assert!(weapon.weapon_equipped());
    }

    #[test]
    fn faction_projection_preserves_first_rank_and_reports_unresolved_forms() {
        let mut world = World::new();
        let actor = world.spawn();
        world.insert(
            actor,
            FactionRanks::from_pairs([(0x44, -1), (0x44, 3), (0x0100_0045, 2)]),
        );
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].factions.as_ref().unwrap();
        assert!(snapshot.truncated());
        assert_eq!(snapshot.memberships().len(), 1);
        let faction = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x44);
        assert_eq!(snapshot.rank(faction), Some(-1));
    }

    #[test]
    fn perk_projection_preserves_first_rank_and_reports_invalid_entries() {
        let mut world = World::new();
        let actor = world.spawn();
        world.insert(
            actor,
            Perks {
                entries: vec![
                    byroredux_core::character::PerkRank {
                        perk_form_id: 0x44,
                        rank: 1,
                    },
                    byroredux_core::character::PerkRank {
                        perk_form_id: 0x44,
                        rank: 3,
                    },
                    byroredux_core::character::PerkRank {
                        perk_form_id: 0x45,
                        rank: 0,
                    },
                    byroredux_core::character::PerkRank {
                        perk_form_id: 0x0100_0046,
                        rank: 2,
                    },
                ],
            },
        );
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].perks.as_ref().unwrap();
        assert!(snapshot.truncated());
        assert_eq!(snapshot.entries().len(), 1);
        let perk = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x44);
        assert_eq!(snapshot.rank(perk), Some(1));
    }

    #[test]
    fn package_projection_unifies_ambient_and_scene_state_and_defers_reevaluation() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let actor = world.spawn();
        let scene_entity = world.spawn();
        world.insert(
            actor,
            AmbientPackageRuntime {
                package_candidates: vec![0x30, 0x31, 0x0100_0032],
                active_package_form_id: Some(0x31),
                actor_form_id: 0x20,
                last_evaluated_game_minute: Some(10),
            },
        );
        world.insert(
            scene_entity,
            byroredux_scripting::ScenePackagePlayback {
                active_actions: vec![byroredux_scripting::ActiveScenePackageAction {
                    scene_form_id: 0x40,
                    action_index: 2,
                    actor,
                    package_candidates: vec![0x50],
                    package_form_id: 0x50,
                    template_form_id: 0x51,
                    command: byroredux_scripting::ScenePackageCommand::AwaitExternal {
                        procedure_type: "Test".to_owned(),
                    },
                }],
            },
        );
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].packages.as_ref().unwrap();
        assert!(snapshot.truncated(), "the unresolved candidate is explicit");
        assert_eq!(snapshot.selections().len(), 2);
        assert_eq!(
            snapshot.selections()[0].source(),
            byroredux_sdk::packages::PackageSelectionSource::Ambient
        );
        assert_eq!(snapshot.selections()[0].candidates().len(), 2);
        assert_eq!(snapshot.selections()[0].active().unwrap().local(), 0x31);
        assert_eq!(
            snapshot.selections()[1].source(),
            byroredux_sdk::packages::PackageSelectionSource::Scene
        );
        assert_eq!(snapshot.selections()[1].action_index(), Some(2));
        assert_eq!(snapshot.selections()[1].scene().unwrap().local(), 0x40);
        assert_eq!(snapshot.selections()[1].template().unwrap().local(), 0x51);

        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let handle = host.bind_entity(actor, None).unwrap();
        host.pending_package_evaluations
            .push(EvaluatePackageCommand::new(handle));
        apply_pending_world_commands(&world, &mut host);
        assert!(world.has::<byroredux_scripting::EvaluatePackageRequest>(actor));
    }

    #[test]
    fn animation_projection_and_command_share_the_authored_idle_runtime() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let actor = world.spawn();
        world.insert(
            actor,
            byroredux_scripting::ActorCinematicState {
                requested_idle_form_id: Some(0x44),
                idle_request_serial: 3,
                awaited_event: Some(byroredux_scripting::CinematicAnimationEvent::ExitCartEnd),
                last_animation_event: Some(
                    byroredux_scripting::CinematicAnimationEvent::IdleFurnitureExit,
                ),
                animation_event_serial: 5,
                ..Default::default()
            },
        );
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].animation.unwrap();
        assert_eq!(snapshot.requested_idle().unwrap().local(), 0x44);
        assert_eq!(snapshot.request_generation(), 3);
        assert_eq!(snapshot.awaited_event(), Some(AnimationEvent::ExitCartEnd));
        assert_eq!(
            snapshot.last_event(),
            Some(AnimationEvent::IdleFurnitureExit)
        );
        assert_eq!(snapshot.event_generation(), 5);

        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let handle = host.bind_entity(actor, None).unwrap();
        let replacement = FormRef::new(PluginId::from_filename("Skyrim.esm").0.to_be_bytes(), 0x55);
        host.pending_animation_commands
            .push(PlayIdleCommand::new(handle, replacement));
        apply_pending_world_commands(&world, &mut host);
        let state = world
            .get::<byroredux_scripting::ActorCinematicState>(actor)
            .unwrap();
        assert_eq!(state.requested_idle_form_id, Some(0x55));
        assert_eq!(state.idle_request_serial, 4);
    }

    #[test]
    fn reputation_projection_and_writes_share_canonical_actor_state() {
        let mut world = World::new();
        world.register::<FactionReputation>();
        let actor = world.spawn();
        let mut reputation = FactionReputation::default();
        reputation.add_fame(0x44, 12);
        reputation.add_infamy(0x44, 4);
        world.insert(actor, reputation);
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["FalloutNV.esm".into()],
            vec![byroredux_plugin::esm::reader::GlobalSlot::Regular(0)],
        );
        world.insert_resource(
            crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order(&order),
        );

        let projections = capture_entity_projections(&world, &BTreeSet::from([actor]));
        let snapshot = projections[&actor].reputation.as_ref().unwrap();
        assert!(!snapshot.truncated());
        let repu = FormRef::new(
            PluginId::from_filename("FalloutNV.esm").0.to_be_bytes(),
            0x44,
        );
        assert_eq!(snapshot.get(repu).unwrap().fame(), 12);
        assert_eq!(snapshot.get(repu).unwrap().infamy(), 4);

        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let handle = host.bind_entity(actor, None).unwrap();
        host.pending_reputation_writes
            .push(ReputationCommand::new(handle, repu, ReputationOperation::AddInfamy, 3).unwrap());
        apply_pending_world_commands(&world, &mut host);
        let state = world.get::<FactionReputation>(actor).unwrap();
        assert_eq!(state.fame(0x44), 12);
        assert_eq!(state.infamy(0x44), 7);
        drop(state);

        host.pending_reputation_writes
            .push(ReputationCommand::new(handle, repu, ReputationOperation::AddFame, 9).unwrap());
        host.pending_reputation_writes.push(
            ReputationCommand::new(
                handle,
                FormRef::new([99; 16], 1),
                ReputationOperation::AddInfamy,
                1,
            )
            .unwrap(),
        );
        apply_pending_world_commands(&world, &mut host);
        let state = world.get::<FactionReputation>(actor).unwrap();
        assert_eq!(state.fame(0x44), 12, "an unresolved REPU rejects the batch");
        assert_eq!(state.infamy(0x44), 7);
        assert!(host.take_diagnostics().iter().any(|diagnostic| matches!(
            diagnostic,
            ExtensionDiagnostic::Fault { message, .. }
                if message.contains("deferred reputation batch rejected")
        )));
    }

    #[test]
    fn update_dispatch_flushes_world_commands_left_by_any_callback_phase() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let actor = world.spawn();
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let handle = host.bind_entity(actor, None).unwrap();
        host.pending_package_evaluations
            .push(EvaluatePackageCommand::new(handle));
        world.insert_resource(ExtensionHostSlot::from_host(host));

        extension_update_dispatch_system(&world, 0.016);

        assert!(world.has::<byroredux_scripting::EvaluatePackageRequest>(actor));
    }

    #[test]
    fn spatial_snapshot_captures_portable_authored_references_and_queries_by_distance() {
        let mut world = World::new();
        let near_entity = world.spawn();
        let far_entity = world.spawn();
        let plugin = PluginId::from_filename("Skyrim.esm");
        let near_pair = FormIdPair {
            plugin,
            local: LocalFormId(1),
        };
        let far_pair = FormIdPair {
            plugin,
            local: LocalFormId(2),
        };
        let mut pool = FormIdPool::new();
        let near_form = pool.intern(near_pair);
        let far_form = pool.intern(far_pair);
        world.insert(near_entity, FormIdComponent(near_form));
        world.insert(far_entity, FormIdComponent(far_form));
        world.insert(
            near_entity,
            GlobalTransform::new(Vec3::new(2.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(
            far_entity,
            Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
        );
        world.insert_resource(pool);

        let snapshot = capture_spatial_snapshot(&world);
        assert_eq!(snapshot.references().len(), 2);
        assert!(!snapshot.truncated());
        let result = snapshot.nearby([0.0; 3], 5.0, 1).unwrap();
        assert_eq!(result.hits().len(), 1);
        assert_eq!(result.hits()[0].reference().form(), form_ref(near_pair));
        assert_eq!(result.hits()[0].distance(), 2.0);
        assert!(result.truncated());
    }

    #[test]
    fn spatial_snapshot_marks_duplicate_portable_forms_truncated() {
        let mut world = World::new();
        let first = world.spawn();
        let duplicate = world.spawn();
        let pair = FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(1),
        };
        let mut pool = FormIdPool::new();
        let form = pool.intern(pair);
        world.insert(first, FormIdComponent(form));
        world.insert(duplicate, FormIdComponent(form));
        world.insert(first, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(duplicate, Transform::from_translation(Vec3::X));
        world.insert_resource(pool);

        let snapshot = capture_spatial_snapshot(&world);
        assert_eq!(snapshot.references().len(), 1);
        assert!(snapshot.truncated());
    }

    #[test]
    fn world_generation_invalidates_old_entity_handles() {
        let mut host = host_with_package("org.example.generation");
        host.dispatch_activations([RawActivation {
            subject: 7,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
        let old = host.handles.by_entity[&7];
        let owner = PrincipalId::new("org.example.generation").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert!(host.state().row(&owner, &schema, old).is_some());
        assert_eq!(host.handles.resolve(old), Some(7));
        assert_eq!(host.begin_world_generation().unwrap(), 2);
        assert_eq!(host.handles.resolve(old), None);
        assert!(host.state().row(&owner, &schema, old).is_none());
        let replacement = host.handles.handle_for(7).unwrap();
        assert_ne!(old, replacement);
        assert_eq!(replacement.world_generation(), 2);
    }

    #[test]
    fn content_catalog_sync_publishes_the_live_load_order_to_the_host() {
        let order = crate::cell_loader::load_order::LoadOrder::new(
            vec!["Skyrim.esm".into(), "Creation.esl".into()],
            vec![
                byroredux_plugin::esm::reader::GlobalSlot::Regular(0),
                byroredux_plugin::esm::reader::GlobalSlot::Light(3),
            ],
        );
        let faction = |form_id, relations| byroredux_plugin::esm::records::FactionRecord {
            form_id,
            editor_id: format!("Faction{form_id:08X}"),
            full_name: String::new(),
            flags: 0,
            relations,
            ranks: Vec::new(),
            reputation: None,
        };
        let factions = std::collections::HashMap::from([
            (
                0x0000_0100,
                faction(
                    0x0000_0100,
                    vec![byroredux_plugin::esm::records::FactionRelation {
                        other_faction: 0x0000_0200,
                        modifier: 25,
                        combat_reaction: 3,
                    }],
                ),
            ),
            (0x0000_0200, faction(0x0000_0200, Vec::new())),
        ]);
        let resolver = crate::cell_loader::load_order::GlobalFormIdResolver::from_load_order_with_records_and_factions(
            &order,
            &std::collections::HashMap::from([(0xFE00_3ABC, *b"STAT")]),
            &factions,
        );
        let slot = ExtensionHostSlot::initialize_default();
        let host = slot.host().unwrap();
        let mut world = World::new();
        world.insert_resource(resolver);
        world.insert_resource(slot);

        extension_content_catalog_sync_system(&world, 0.0);

        let host = host.lock().unwrap();
        assert_eq!(host.content_catalog.len(), 2);
        assert_eq!(host.content_catalog.plugin(0).unwrap().name(), "Skyrim.esm");
        assert_eq!(host.content_catalog.find("CREATION.ESL").unwrap().0, 1);
        let form = byroredux_sdk::identity::FormRef::new(
            byroredux_core::form_id::PluginId::from_filename("Creation.esl")
                .0
                .to_be_bytes(),
            0xabc,
        );
        assert_eq!(
            host.content_catalog.record(form).unwrap().record_type(),
            *b"STAT"
        );
        let source = FormRef::new(
            byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
                .0
                .to_be_bytes(),
            0x100,
        );
        let target = FormRef::new(
            byroredux_core::form_id::PluginId::from_filename("Skyrim.esm")
                .0
                .to_be_bytes(),
            0x200,
        );
        let relationship = host
            .faction_relationships
            .relationship(source, target)
            .unwrap();
        assert_eq!(relationship.modifier(), 25);
        assert_eq!(relationship.combat_reaction_raw(), 3);
    }

    #[test]
    fn engine_settings_sync_publishes_public_configuration_to_the_host() {
        let slot = ExtensionHostSlot::initialize_default();
        let host = slot.host().unwrap();
        let mut world = World::new();
        let mut settings = byroredux_core::settings::SettingsRegistry::default();
        settings
            .register(byroredux_core::settings::SettingEntry::slider(
                "gameplay.fov",
                "Gameplay",
                "FOV",
                "test",
                90.0,
                45.0,
                120.0,
                1.0,
                "degrees",
            ))
            .unwrap();
        settings
            .set(
                "gameplay.fov",
                byroredux_core::settings::SettingValue::Number(110.0),
            )
            .unwrap();
        world.insert_resource(settings);
        world.insert_resource(slot);

        extension_engine_settings_sync_system(&world, 0.0);

        let host = host.lock().unwrap();
        assert_eq!(
            host.engine_settings.get("gameplay.fov"),
            Some(&byroredux_sdk::settings::SettingValue::Number(110.0))
        );
    }

    #[test]
    fn deferred_extension_setting_writes_commit_through_native_registry() {
        let slot = ExtensionHostSlot::initialize_default();
        let host = slot.host().unwrap();
        let mut settings = byroredux_core::settings::SettingsRegistry::default();
        settings
            .register(byroredux_core::settings::SettingEntry::toggle(
                "ext.org.example.settings.enabled",
                "org.example.settings",
                "Enabled",
                "test",
                false,
            ))
            .unwrap();
        host.lock().unwrap().pending_setting_writes.push(
            byroredux_sdk::settings::SettingWriteCommand {
                key: "ext.org.example.settings.enabled".to_owned(),
                value: SdkSettingValue::Boolean(true),
            },
        );
        let mut world = World::new();
        world.insert_resource(settings);
        world.insert_resource(slot);

        extension_setting_write_apply_system(&world, 0.0);

        assert_eq!(
            world
                .resource::<byroredux_core::settings::SettingsRegistry>()
                .get("ext.org.example.settings.enabled")
                .unwrap()
                .value,
            byroredux_core::settings::SettingValue::Bool(true)
        );
        let host = host.lock().unwrap();
        assert!(host.pending_setting_writes.is_empty());
        assert_eq!(
            host.engine_settings.get("ext.org.example.settings.enabled"),
            Some(&SdkSettingValue::Boolean(true))
        );
    }

    #[test]
    fn orderly_shutdown_stops_active_components() {
        let mut host = host_with_package("org.example.shutdown");
        host.shutdown_all();

        assert_eq!(
            host.components[0].instance.status(),
            &InstanceStatus::Stopped
        );
        host.shutdown_all();
        assert_eq!(
            host.components[0].instance.status(),
            &InstanceStatus::Stopped
        );
    }

    #[test]
    fn extension_rows_round_trip_through_stable_form_identity() {
        let slot = ExtensionHostSlot::from_host(host_with_package("org.example.save"));
        let host = slot.host().unwrap();
        let (source, old_entity) = world_with_form_and_host(slot.clone(), form_pair_for_test());
        host.lock().unwrap().dispatch_activations([RawActivation {
            subject: old_entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
        let mut snapshot = empty_snapshot();
        assert_eq!(capture_extension_state(&source, &mut snapshot).unwrap(), 1);
        let encoded = byroredux_save::encode(&snapshot, 0xfeed_beef).unwrap();
        let snapshot = byroredux_save::decode(&encoded, 0xfeed_beef).unwrap();

        let (restored, new_entity) = world_with_form_and_host(slot, form_pair_for_test());
        assert_eq!(
            old_entity, new_entity,
            "fixture intentionally reuses the raw ECS id"
        );
        preflight_extension_state(&restored, &snapshot).unwrap();
        assert_eq!(restore_extension_state(&restored, &snapshot).unwrap(), 1);

        let host = host.lock().unwrap();
        let handle = host.handles.by_entity[&new_entity];
        assert_eq!(handle.world_generation(), 2);
        let owner = PrincipalId::new("org.example.save").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, handle)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(1))
        );
    }

    #[test]
    fn unavailable_extension_rows_are_preserved_verbatim() {
        let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.missing"));
        let source_host = source_slot.host().unwrap();
        let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
        source_host
            .lock()
            .unwrap()
            .dispatch_activations([RawActivation {
                subject: entity,
                subject_form: None,
                activator: None,
                activator_form: None,
            }]);
        let mut original = empty_snapshot();
        capture_extension_state(&source, &mut original).unwrap();

        let empty_slot = ExtensionHostSlot::initialize_default();
        let target_host = empty_slot.host().unwrap();
        let (target, _) = world_with_form_and_host(empty_slot, form_pair_for_test());
        preflight_extension_state(&target, &original).unwrap();
        assert_eq!(restore_extension_state(&target, &original).unwrap(), 0);
        assert_eq!(target_host.lock().unwrap().retained_rows.len(), 1);

        let mut resaved = empty_snapshot();
        capture_extension_state(&target, &mut resaved).unwrap();
        assert_eq!(
            original.resources[EXTENSION_STATE_RESOURCE],
            resaved.resources[EXTENSION_STATE_RESOURCE]
        );
    }

    #[test]
    fn retained_form_row_rebinds_before_the_next_activation() {
        let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.streamed"));
        let source_host = source_slot.host().unwrap();
        let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
        source_host
            .lock()
            .unwrap()
            .dispatch_activations([RawActivation {
                subject: entity,
                subject_form: None,
                activator: None,
                activator_form: None,
            }]);
        let mut snapshot = empty_snapshot();
        capture_extension_state(&source, &mut snapshot).unwrap();

        let target_slot = ExtensionHostSlot::from_host(host_with_package("org.example.streamed"));
        let target_host = target_slot.host().unwrap();
        let mut target = World::new();
        target.insert_resource(FormIdPool::new());
        target.insert_resource(target_slot);
        preflight_extension_state(&target, &snapshot).unwrap();
        assert_eq!(restore_extension_state(&target, &snapshot).unwrap(), 0);

        let stable = form_ref(form_pair_for_test());
        let stats = target_host
            .lock()
            .unwrap()
            .dispatch_activations([RawActivation {
                subject: 77,
                subject_form: Some(stable),
                activator: None,
                activator_form: None,
            }]);
        assert_eq!(stats.commands_applied, 1);
        let host = target_host.lock().unwrap();
        assert!(host.retained_rows.is_empty());
        let handle = host.handles.by_entity[&77];
        let owner = PrincipalId::new("org.example.streamed").unwrap();
        let schema = ComponentSchemaId::new("example.activation-count").unwrap();
        assert_eq!(
            host.state()
                .row(&owner, &schema, handle)
                .and_then(|row| row.get("count")),
            Some(&ExtensionValue::I64(2))
        );
    }

    #[test]
    fn schema_mismatch_is_rejected_before_world_replacement() {
        let source_slot = ExtensionHostSlot::from_host(host_with_package("org.example.schema"));
        let source_host = source_slot.host().unwrap();
        let (source, entity) = world_with_form_and_host(source_slot, form_pair_for_test());
        source_host
            .lock()
            .unwrap()
            .dispatch_activations([RawActivation {
                subject: entity,
                subject_form: None,
                activator: None,
                activator_form: None,
            }]);
        let mut snapshot = empty_snapshot();
        capture_extension_state(&source, &mut snapshot).unwrap();

        let mut changed = manifest("org.example.schema");
        changed.component_schemas[0].version = 2;
        let target_slot = ExtensionHostSlot::from_host(host_with_manifest(changed));
        let (target, _) = world_with_form_and_host(target_slot, form_pair_for_test());
        let error = preflight_extension_state(&target, &snapshot).unwrap_err();
        assert!(error.to_string().contains("saved state requires 1"));
    }

    #[test]
    fn unsupported_extension_state_format_fails_preflight() {
        let slot = ExtensionHostSlot::initialize_default();
        let (world, _) = world_with_form_and_host(slot, form_pair_for_test());
        let mut snapshot = empty_snapshot();
        snapshot.resources.insert(
            EXTENSION_STATE_RESOURCE.to_owned(),
            serde_json::json!({ "format_version": 99, "rows": [] }),
        );
        let error = preflight_extension_state(&world, &snapshot).unwrap_err();
        assert!(error.to_string().contains("format 99 is unsupported"));
    }

    #[test]
    fn transient_entity_rows_abort_instead_of_producing_a_lossy_save() {
        let slot = ExtensionHostSlot::from_host(host_with_package("org.example.transient"));
        let host = slot.host().unwrap();
        let mut world = World::new();
        let entity = world.spawn();
        world.insert_resource(slot);
        host.lock().unwrap().dispatch_activations([RawActivation {
            subject: entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
        let error = capture_extension_state(&world, &mut empty_snapshot()).unwrap_err();
        assert!(error.to_string().contains("refusing a lossy save"));
    }

    #[test]
    fn lossy_extension_snapshot_rejection_does_not_consume_quicksave_slot() {
        let directory = tempfile::tempdir().unwrap();
        let slot = ExtensionHostSlot::from_host(host_with_package("org.example.save-abort"));
        let host = slot.host().unwrap();
        let mut world = World::new();
        world.insert_resource(byroredux_core::string::StringPool::new());
        world.insert_resource(FormIdPool::new());
        world.insert_resource(crate::save_io::build_save_registry());
        world.insert_resource(crate::save_io::SaveState::new(
            directory.path().to_owned(),
            4,
        ));
        world.insert_resource(SessionEventQueue::default());
        let entity = world.spawn();
        world.insert_resource(slot);
        host.lock().unwrap().dispatch_activations([RawActivation {
            subject: entity,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);

        let output = crate::save_io::quicksave(&world);
        assert!(
            output.lines.join(" ").contains("refusing a lossy save"),
            "unexpected save output: {:?}",
            output.lines
        );
        assert_eq!(world.resource::<crate::save_io::SaveState>().ring.peek(), 0);
        assert!(byroredux_save::disk::list_slots(directory.path()).is_empty());
        assert!(pending_session_events(&world).is_empty());
    }

    #[test]
    fn denied_capability_prevents_package_publication() {
        let mut host =
            ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default()).unwrap();
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(COMPONENT).unwrap(),
        );
        assert!(host
            .install_package(
                &manifest("org.example.denied"),
                &artifacts,
                CapabilitySet::new()
            )
            .is_err());
        assert_eq!(host.package_count(), 0);
        assert_eq!(host.component_count(), 0);
    }

    #[test]
    fn one_trapping_package_does_not_block_an_unrelated_package() {
        let mut host = host_with_package("org.example.healthy");
        let trapping = COMPONENT.replacen(
            "      call $increment)",
            "      call $increment\n      unreachable)",
            1,
        );
        let mut artifacts = ExtensionArtifacts::new();
        artifacts.insert(
            ComponentId::new("runtime").unwrap(),
            wat::parse_str(&trapping).unwrap(),
        );
        host.install_package(&manifest("org.example.trapping"), &artifacts, grants())
            .unwrap();

        let stats = host.dispatch_activations([RawActivation {
            subject: 9,
            subject_form: None,
            activator: None,
            activator_form: None,
        }]);
        assert_eq!(stats.deliveries, 2);
        assert_eq!(stats.commands_applied, 1);
        assert_eq!(stats.faults, 1);
        let healthy = host
            .components
            .iter()
            .find(|component| component.extension.as_str() == "org.example.healthy")
            .unwrap();
        let trapping = host
            .components
            .iter()
            .find(|component| component.extension.as_str() == "org.example.trapping")
            .unwrap();
        assert_eq!(healthy.instance.status(), &InstanceStatus::Active);
        assert!(matches!(
            trapping.instance.status(),
            InstanceStatus::Quarantined(_)
        ));
    }

    #[test]
    fn cli_package_set_requires_explicit_grants_and_commits_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let manifest_path = directory.path().join("extension.toml");
        let component_path = directory.path().join("runtime.wasm");
        std::fs::write(&component_path, wat::parse_str(COMPONENT).unwrap()).unwrap();
        std::fs::write(
            &manifest_path,
            r#"
manifest_version = 1
id = "org.example.cli"
name = "CLI fixture"
version = "1.0.0"
sdk = "^0.1"

[[components]]
id = "runtime"
path = "runtime.wasm"
world = "byro.mod-host.extension"
world_version = "^0.1"

[[capabilities]]
id = "byro.events.subscribe"
required = true

[[capabilities]]
id = "byro.components.write-own"
required = true

[[capabilities]]
id = "byro.settings.register"
required = true

[[subscriptions]]
event = "byro.events.activate"

[[component_schemas]]
id = "example.activation-count"
version = 1

[[component_schemas.fields]]
id = "count"
value_type = "i64"

[[settings]]
id = "strength"
label = "Strength"
description = "Effect strength"
default = { Number = 1.0 }
control = { kind = "slider", min = 0.0, max = 2.0, step = 0.1, unit = "x" }
"#,
        )
        .unwrap();

        let mut world = World::new();
        world.insert_resource(ExtensionHostSlot::initialize_default());
        world.insert_resource(byroredux_core::settings::SettingsRegistry::default());
        let base_args = vec![
            "byroredux".to_owned(),
            "--extension".to_owned(),
            manifest_path.to_string_lossy().into_owned(),
        ];
        assert!(load_requested_extensions(&world, &base_args).is_err());
        {
            let slot = world.resource::<ExtensionHostSlot>();
            let host = slot.host().unwrap();
            assert_eq!(host.lock().unwrap().package_count(), 0);
        }
        assert!(!world
            .resource::<byroredux_core::settings::SettingsRegistry>()
            .contains("ext.org.example.cli.strength"));

        let mut granted_args = base_args;
        granted_args.extend([
            "--extension-grant".to_owned(),
            "org.example.cli=*".to_owned(),
        ]);
        assert_eq!(load_requested_extensions(&world, &granted_args).unwrap(), 1);
        let slot = world.resource::<ExtensionHostSlot>();
        let host = slot.host().unwrap();
        assert_eq!(host.lock().unwrap().package_count(), 1);
        let settings = world.resource::<byroredux_core::settings::SettingsRegistry>();
        let entry = settings.get("ext.org.example.cli.strength").unwrap();
        assert_eq!(
            entry.value,
            byroredux_core::settings::SettingValue::Number(1.0)
        );
        assert_eq!(entry.section, "org.example.cli");
    }
}
