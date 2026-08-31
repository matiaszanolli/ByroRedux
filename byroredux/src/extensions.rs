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

use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform, Name, Transform};
use byroredux_core::ecs::{EntityId, Resource, World};
use byroredux_core::form_id::{FormIdPair, FormIdPool};
use byroredux_core::string::StringPool;
use byroredux_mod_runtime::{
    CapabilitySet, InstanceStatus, LogEntry, LogLevel, ModInstance, SandboxConfig, SandboxError,
    SandboxRuntime,
};
use byroredux_sdk::component::{
    ComponentSchema, ComponentStoreError, ComponentStoreLimits, ExtensionComponentStore,
    ExtensionStateSnapshot, ExtensionValue, PersistedComponentRow, RestoredComponentRow,
    EXTENSION_STATE_FORMAT_VERSION,
};
use byroredux_sdk::event::ActivationEvent;
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EntityRef, ExtensionId, FormRef, PrincipalId,
};
use byroredux_sdk::manifest::ExtensionManifest;
use byroredux_sdk::projection::{EntityProjection, WorldTransform, MAX_ENTITY_NAME_BYTES};
use byroredux_sdk::service::{ACTIVATE_EVENT, EVENTS_SUBSCRIBE_CAPABILITY};
use byroredux_sdk::storage::{
    HostCommand, PersistedPrincipalStorage, PrincipalStorageError, PrincipalStorageLimits,
    PrincipalStorageStore,
};
use thiserror::Error;

const EXTENSION_STATE_RESOURCE: &str = "ByroExtensionState";
const MAX_PERSISTED_EXTENSION_ROWS: usize = 262_144;

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

    #[cfg(test)]
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
    instance: ModInstance,
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
        let mut staged_components = Vec::with_capacity(compiled.len());
        let mut staged_diagnostics = Vec::new();
        for (component_id, compiled) in compiled {
            let mut instance = self
                .runtime
                .instantiate(&compiled, manifest, grants.clone())?;
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
                instance,
            });
        }

        self.state = staged_state;
        self.principal_storage = staged_principal_storage;
        self.retained_storage = staged_retained_storage;
        self.components.extend(staged_components);
        self.diagnostics.extend(staged_diagnostics);
        Ok(())
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

                match result {
                    Ok(commands) => {
                        let command_count = commands.len();
                        let mut component_commands = Vec::new();
                        let mut storage_commands = Vec::new();
                        for command in commands {
                            match command {
                                HostCommand::Component(command) => {
                                    component_commands.push(command);
                                }
                                HostCommand::PrincipalStorage(command) => {
                                    storage_commands.push(command);
                                }
                            }
                        }
                        let mut staged_state = self.state.clone();
                        let mut staged_storage = self.principal_storage.clone();
                        let apply_result = staged_state
                            .apply_batch(&principal, &component_commands)
                            .map_err(|error| error.to_string())
                            .and_then(|()| {
                                if storage_commands.is_empty() {
                                    Ok(())
                                } else {
                                    staged_storage
                                        .apply_batch(&principal, &storage_commands)
                                        .map_err(|error| error.to_string())
                                }
                            });
                        if let Err(error) = apply_result {
                            let message = format!("deferred command batch rejected: {error}");
                            hosted.instance.reject_deferred_commands(message.clone());
                            self.diagnostics.push(ExtensionDiagnostic::Fault {
                                extension: hosted.extension.clone(),
                                component: hosted.component.clone(),
                                message,
                            });
                            stats.faults += 1;
                        } else {
                            self.state = staged_state;
                            self.principal_storage = staged_storage;
                            stats.commands_applied += command_count;
                        }
                    }
                    Err(error) => {
                        self.diagnostics.push(ExtensionDiagnostic::Fault {
                            extension: hosted.extension.clone(),
                            component: hosted.component.clone(),
                            message: error.to_string(),
                        });
                        stats.faults += 1;
                    }
                }
            }
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
}

/// Raw ECS identity snapshot captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawActivation {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
    pub activator: Option<EntityId>,
    pub activator_form: Option<FormRef>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct RawEntityProjection {
    name: Option<String>,
    world_transform: Option<WorldTransform>,
}

fn entity_projection(
    entity: EntityRef,
    form: Option<FormRef>,
    raw: Option<&RawEntityProjection>,
) -> EntityProjection {
    EntityProjection::new(
        entity,
        form,
        raw.and_then(|projection| projection.name.clone()),
        raw.and_then(|projection| projection.world_transform),
    )
    .expect("live projection capture enforces SDK bounds")
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
    projections
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
        CapabilityRequest, ComponentSchemaDeclaration, EventSubscription, ExecutableComponent,
        EXTENSION_MANIFEST_VERSION,
    };
    use byroredux_sdk::service::{
        COMPONENTS_WRITE_OWN_CAPABILITY, EXTENSION_WORLD_SERVICE, STORAGE_READ_OWN_CAPABILITY,
        STORAGE_WRITE_OWN_CAPABILITY, WORLD_ENTITY_READ_CAPABILITY,
        WORLD_TRANSFORM_READ_CAPABILITY,
    };

    const COMPONENT: &str = r#"
(component
  (import "byro:mod-host/state@0.1.0" (instance $state
    (type $entity-ref-shape (record
      (field "world-generation" u64)
      (field "object" u64)))
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
    (export "queue-increment-own-i64" (func
      (param "entity" $entity-ref-in)
      (param "schema-index" u32)
      (param "field-index" u32)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
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
)
"#;

    const STORAGE_COMPONENT: &str = r#"
(component
  (import "byro:mod-host/state@0.1.0" (instance $state
    (type $entity-ref-shape (record
      (field "world-generation" u64)
      (field "object" u64)))
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
  ))
  (import "byro:mod-host/storage@0.1.0" (instance $storage
    (export "queue-increment-i64" (func
      (param "key" string)
      (param "delta" s64)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
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
)
"#;

    const PROJECTION_COMPONENT: &str = r#"
(component
  (type $entity-ref-shape (record
    (field "world-generation" u64)
    (field "object" u64)))
  (import "byro:mod-host/state@0.1.0" (instance $state
    (export "entity-ref" (type $entity-ref-in (eq $entity-ref-shape)))
  ))
  (import "byro:mod-host/world-state@0.1.0" (instance $world
    (export "entity-ref" (type $entity-ref-world (eq $entity-ref-shape)))
    (export "contains-entity" (func
      (param "entity" $entity-ref-world)
      (result bool)))
  ))
  (alias export $state "entity-ref" (type $entity-ref))
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
            }],
            component_schemas: vec![ComponentSchemaDeclaration {
                id: ComponentSchemaId::new("example.activation-count").unwrap(),
                version: 1,
                fields: vec![ComponentFieldDeclaration {
                    id: ComponentFieldId::new("count").unwrap(),
                    value_type: ExtensionValueType::I64,
                }],
            }],
            principal_storage_schema: None,
        }
    }

    fn grants() -> CapabilitySet {
        let mut grants = CapabilitySet::new();
        grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        grants.grant(COMPONENTS_WRITE_OWN_CAPABILITY).unwrap();
        grants
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

    fn storage_grants() -> CapabilitySet {
        let mut grants = CapabilitySet::new();
        grants.grant(EVENTS_SUBSCRIBE_CAPABILITY).unwrap();
        grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        grants.grant(STORAGE_WRITE_OWN_CAPABILITY).unwrap();
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
            Some(&ExtensionValue::I64(2))
        );

        let saved = source.capture_saved_state(&BTreeMap::new()).unwrap();
        assert_eq!(saved.principal_storage.len(), 1);
        let mut restored = host_with_storage_package(principal.as_str());
        restored
            .restore_saved_state(&saved, &BTreeMap::new())
            .unwrap();
        assert_eq!(
            restored.dispatch_activations([activation]).commands_applied,
            1
        );
        assert_eq!(
            restored
                .principal_storage
                .values(&principal)
                .and_then(|values| values.get(&key)),
            Some(&ExtensionValue::I64(3))
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
            Some(&ExtensionValue::I64(2))
        );
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

[[subscriptions]]
event = "byro.events.activate"

[[component_schemas]]
id = "example.activation-count"
version = 1

[[component_schemas.fields]]
id = "count"
value_type = "i64"
"#,
        )
        .unwrap();

        let mut world = World::new();
        world.insert_resource(ExtensionHostSlot::initialize_default());
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

        let mut granted_args = base_args;
        granted_args.extend([
            "--extension-grant".to_owned(),
            "org.example.cli=*".to_owned(),
        ]);
        assert_eq!(load_requested_extensions(&world, &granted_args).unwrap(), 1);
        let slot = world.resource::<ExtensionHostSlot>();
        let host = slot.host().unwrap();
        assert_eq!(host.lock().unwrap().package_count(), 1);
    }
}
