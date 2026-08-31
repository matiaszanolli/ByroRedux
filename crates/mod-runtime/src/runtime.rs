use crate::bindings::byro::mod_host::{
    context, logging, state, storage as wit_storage, world_state,
};
use crate::bindings::Extension;
use crate::{CapabilitySet, Principal, Result, SandboxConfig, SandboxError};
use byroredux_sdk::component::{ExtensionCommand, ExtensionValue, ExtensionValueType};
use byroredux_sdk::event::{ActivationEvent, CellLoadEvent};
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EventId, ExtensionId, PrincipalId, ServiceId, StorageKey,
};
use byroredux_sdk::manifest::{ComponentSchemaDeclaration, ExtensionManifest};
use byroredux_sdk::projection::EntityProjection;
use byroredux_sdk::service::{
    current_sdk_version, CapabilityDescriptor, ServiceCatalog, ServiceDescriptor, ACTIVATE_EVENT,
    CELL_LOAD_EVENT, COMPONENTS_WRITE_OWN_CAPABILITY, COMPONENT_STATE_SERVICE, CONTEXT_SERVICE,
    EVENTS_SUBSCRIBE_CAPABILITY, EVENT_SERVICE, EXTENSION_WORLD_SERVICE, LOGGING_SERVICE,
    PRINCIPAL_STORAGE_SERVICE, STORAGE_READ_OWN_CAPABILITY, STORAGE_WRITE_OWN_CAPABILITY,
    WORLD_ENTITY_READ_CAPABILITY, WORLD_PROJECTION_SERVICE, WORLD_TRANSFORM_READ_CAPABILITY,
};
use byroredux_sdk::storage::{HostCommand, PrincipalStorageCommand};
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
    Shutdown,
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialize",
            Self::Activate => "on-activate",
            Self::CellLoad => "on-cell-load",
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
}

impl fmt::Display for FaultKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Guest => "guest fault",
            Self::LogBudgetExhausted => "log budget exhausted",
            Self::CommandBudgetExhausted => "command budget exhausted",
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

/// Engine-owned executable-mod runtime with no ambient WASI imports.
pub struct SandboxRuntime {
    engine: Engine,
    linker: Linker<HostState>,
    config: SandboxConfig,
    catalog: Arc<ServiceCatalog>,
}

impl SandboxRuntime {
    pub fn new(config: SandboxConfig) -> Result<Self> {
        config.validate()?;

        let mut engine_config = Config::new();
        engine_config.wasm_component_model(true);
        engine_config.consume_fuel(true);
        engine_config.max_wasm_stack(config.max_wasm_stack_bytes);

        let engine =
            Engine::new(&engine_config).map_err(|error| SandboxError::Engine(error.to_string()))?;
        let mut linker = Linker::new(&engine);
        Extension::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|error| SandboxError::Link(error.to_string()))?;

        let mut catalog = ServiceCatalog::new(current_sdk_version());
        catalog
            .register_capability(CapabilityDescriptor {
                id: CapabilityId::new(LOG_CAPABILITY)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                description: "Emit bounded principal-attributed diagnostics".to_owned(),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        for (id, description) in [
            (
                COMPONENTS_WRITE_OWN_CAPABILITY,
                "Queue bounded mutations to principal-owned component state",
            ),
            (
                EVENTS_SUBSCRIBE_CAPABILITY,
                "Receive declared canonical engine events",
            ),
            (
                STORAGE_READ_OWN_CAPABILITY,
                "Read bounded principal-scoped persistent storage",
            ),
            (
                STORAGE_WRITE_OWN_CAPABILITY,
                "Queue mutations to principal-scoped persistent storage",
            ),
            (
                WORLD_ENTITY_READ_CAPABILITY,
                "Read bounded facts about callback-visible live entities",
            ),
            (
                WORLD_TRANSFORM_READ_CAPABILITY,
                "Read world transforms from callback-visible entity projections",
            ),
        ] {
            catalog
                .register_capability(CapabilityDescriptor {
                    id: CapabilityId::new(id)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                    description: description.to_owned(),
                })
                .map_err(|error| SandboxError::Link(error.to_string()))?;
        }
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(WORLD_PROJECTION_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(WORLD_ENTITY_READ_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(CONTEXT_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: None,
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(PRINCIPAL_STORAGE_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(STORAGE_READ_OWN_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(COMPONENT_STATE_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(COMPONENTS_WRITE_OWN_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(EVENT_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(EVENTS_SUBSCRIBE_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(EXTENSION_WORLD_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: None,
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(LOGGING_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(LOG_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;

        Ok(Self {
            engine,
            linker,
            config,
            catalog: Arc::new(catalog),
        })
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Immutable discovery catalog used by package resolution and guests.
    pub fn catalog(&self) -> &ServiceCatalog {
        &self.catalog
    }

    /// Validate the extension contract before compiling untrusted bytes.
    pub fn compile(
        &self,
        manifest: &ExtensionManifest,
        component_id: &ComponentId,
        bytes: &[u8],
    ) -> Result<CompiledMod> {
        self.catalog.check_manifest(manifest)?;
        if !manifest
            .components
            .iter()
            .any(|component| component.id == *component_id)
        {
            return Err(SandboxError::UndeclaredComponent {
                extension: manifest.id.clone(),
                component: component_id.clone(),
            });
        }
        if bytes.len() > self.config.max_component_bytes {
            return Err(SandboxError::ComponentTooLarge {
                actual: bytes.len(),
                maximum: self.config.max_component_bytes,
            });
        }

        let component = Component::new(&self.engine, bytes)
            .map_err(|error| SandboxError::Compile(format!("{error:#}")))?;
        Ok(CompiledMod {
            component,
            manifest: manifest.clone(),
            extension: manifest.id.clone(),
            extension_version: manifest.version.clone(),
            component_id: component_id.clone(),
        })
    }

    pub fn instantiate(
        &self,
        compiled: &CompiledMod,
        manifest: &ExtensionManifest,
        grants: CapabilitySet,
    ) -> Result<ModInstance> {
        self.catalog.check_grants(manifest, &grants)?;
        if compiled.manifest != *manifest {
            return Err(SandboxError::ManifestMismatch {
                compiled: format!("{}@{}", compiled.extension, compiled.extension_version),
                requested: format!("{}@{}", manifest.id, manifest.version),
            });
        }
        let principal = Principal::new(PrincipalId::from(&manifest.id), manifest.name.clone())?;
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.config.max_memory_bytes)
            .table_elements(self.config.max_table_elements)
            .instances(self.config.max_instances)
            .tables(self.config.max_tables)
            .memories(self.config.max_memories)
            .trap_on_grow_failure(true)
            .build();
        let state = HostState {
            principal,
            grants,
            catalog: Arc::clone(&self.catalog),
            limits,
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: self.config.max_log_entries,
            max_log_message_bytes: self.config.max_log_message_bytes,
            max_log_bytes: self.config.max_log_bytes,
            log_budget_exhausted: false,
            schemas: manifest.component_schemas.clone(),
            principal_storage_schema: manifest.principal_storage_schema,
            principal_storage: BTreeMap::new(),
            entity_projections: BTreeMap::new(),
            subscribed_to_activate: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == ACTIVATE_EVENT),
            subscribed_to_cell_load: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == CELL_LOAD_EVENT),
            pending_commands: Vec::new(),
            max_commands_per_entry: self.config.max_commands_per_entry,
            accepting_commands: false,
            command_budget_exhausted: false,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.config.fuel_per_entry)
            .map_err(|error| SandboxError::Instantiate(error.to_string()))?;

        let bindings = Extension::instantiate(&mut store, &compiled.component, &self.linker)
            .map_err(|error| SandboxError::Instantiate(format!("{error:#}")))?;

        Ok(ModInstance {
            store,
            bindings,
            fuel_per_entry: self.config.fuel_per_entry,
            status: InstanceStatus::Ready,
        })
    }
}

/// A linked component isolated in its own Wasmtime store and principal state.
pub struct ModInstance {
    store: Store<HostState>,
    bindings: Extension,
    fuel_per_entry: u64,
    status: InstanceStatus,
}

impl ModInstance {
    pub fn principal(&self) -> &Principal {
        &self.store.data().principal
    }

    pub fn grants(&self) -> &CapabilitySet {
        &self.store.data().grants
    }

    /// Peek at the diagnostics the guest has produced and the host has not
    /// yet consumed. Does **not** free budget — see [`Self::take_logs`].
    pub fn logs(&self) -> &[LogEntry] {
        &self.store.data().logs
    }

    /// Remove and return every retained diagnostic, returning its budget to
    /// the guest.
    ///
    /// #3050 — `max_log_entries` / `max_log_bytes` used to be lifetime totals
    /// with no drain, so a well-behaved mod that logged at any steady rate was
    /// eventually quarantined for having run long enough rather than for
    /// misbehaving. They are a bound on what the host is *holding*, not on
    /// what the guest may ever say: draining hands the entries to the owner
    /// and the budget back to the guest. Mirrors the `take_errors` /
    /// `resource_errors` split in `crates/ui`, and is why [`Self::logs`] stays
    /// a non-consuming peek.
    ///
    /// A consumer that never calls this still gets the old behaviour, which is
    /// the correct backstop: undrained diagnostics cannot grow without bound.
    pub fn take_logs(&mut self) -> Vec<LogEntry> {
        let state = self.store.data_mut();
        state.log_bytes = 0;
        std::mem::take(&mut state.logs)
    }

    pub fn status(&self) -> &InstanceStatus {
        &self.status
    }

    pub fn fuel_remaining(&self) -> u64 {
        self.store.get_fuel().unwrap_or(0)
    }

    pub fn initialize(&mut self) -> Result<()> {
        if self.status != InstanceStatus::Ready {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Initialize,
                status: self.status.clone(),
            });
        }

        self.enter(LifecyclePhase::Initialize, false, |bindings, store| {
            bindings.call_initialize(store)
        })?;
        self.status = InstanceStatus::Active;
        Ok(())
    }

    /// Deliver one canonical activation and return its deferred state batch.
    ///
    /// No command is returned when the guest traps: `enter` clears the queue
    /// before quarantining the instance, so callers cannot accidentally apply
    /// a partial callback.
    pub fn on_activate(&mut self, event: ActivationEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Activate,
                status: self.status.clone(),
            });
        }
        let event_id = EventId::new(ACTIVATE_EVENT)
            .expect("the engine's canonical activation event id is valid");
        if !self.store.data().subscribed_to_activate {
            return Err(SandboxError::EventNotSubscribed(event_id));
        }
        if !self
            .store
            .data()
            .grants
            .contains(EVENTS_SUBSCRIBE_CAPABILITY)
        {
            return Err(SandboxError::EventDeliveryDenied(event_id));
        }

        let subject = state::EntityRef {
            world_generation: event.subject.world_generation(),
            object: event.subject.object(),
        };
        let activator = event.activator.map(|entity| state::EntityRef {
            world_generation: entity.world_generation(),
            object: entity.object(),
        });
        let result = self.enter(LifecyclePhase::Activate, true, |bindings, store| {
            bindings.call_on_activate(store, subject, activator)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Deliver one canonical entity-load event and return deferred commands.
    pub fn on_cell_load(&mut self, event: CellLoadEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::CellLoad,
                status: self.status.clone(),
            });
        }
        let event_id = EventId::new(CELL_LOAD_EVENT)
            .expect("the engine's canonical cell-load event id is valid");
        if !self.store.data().subscribed_to_cell_load {
            return Err(SandboxError::EventNotSubscribed(event_id));
        }
        if !self
            .store
            .data()
            .grants
            .contains(EVENTS_SUBSCRIBE_CAPABILITY)
        {
            return Err(SandboxError::EventDeliveryDenied(event_id));
        }
        let subject = state::EntityRef {
            world_generation: event.subject.world_generation(),
            object: event.subject.object(),
        };
        let result = self.enter(LifecyclePhase::CellLoad, true, |bindings, store| {
            bindings.call_on_cell_load(store, subject)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Replace the read-only principal storage snapshot visible to callbacks.
    pub fn set_principal_storage_snapshot(&mut self, values: BTreeMap<StorageKey, ExtensionValue>) {
        self.store.data_mut().principal_storage = values;
    }

    /// Replace the callback-local set of engine entity projections.
    pub fn set_entity_projections(
        &mut self,
        projections: impl IntoIterator<Item = EntityProjection>,
    ) {
        self.store.data_mut().entity_projections = projections
            .into_iter()
            .map(|projection| (projection.entity(), projection))
            .collect();
    }

    /// Quarantine an active instance after the host rejects its deferred
    /// command batch.
    ///
    /// Guest callbacks cannot mutate the live world directly. Consequently,
    /// validation can still fail after a callback returns—for example when a
    /// checked counter overflows or a principal exhausts its row budget. The
    /// engine owner calls this method before reporting that rejection so the
    /// component cannot repeatedly submit the same invalid batch.
    pub fn reject_deferred_commands(&mut self, message: impl Into<String>) -> SandboxError {
        let phase = LifecyclePhase::Activate;
        let message = message.into();
        self.store.data_mut().accepting_commands = false;
        self.store.data_mut().pending_commands.clear();
        self.status = InstanceStatus::Quarantined(FaultInfo {
            phase,
            kind: FaultKind::Guest,
            message: message.clone(),
        });
        SandboxError::GuestFault { phase, message }
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Shutdown,
                status: self.status.clone(),
            });
        }

        self.enter(LifecyclePhase::Shutdown, false, |bindings, store| {
            bindings.call_shutdown(store)
        })?;
        self.status = InstanceStatus::Stopped;
        Ok(())
    }

    fn enter(
        &mut self,
        phase: LifecyclePhase,
        accepting_commands: bool,
        call: impl FnOnce(&Extension, &mut Store<HostState>) -> wasmtime::Result<()>,
    ) -> Result<()> {
        {
            let state = self.store.data_mut();
            state.pending_commands.clear();
            state.accepting_commands = accepting_commands;
            state.command_budget_exhausted = false;
        }
        if let Err(error) = self.store.set_fuel(self.fuel_per_entry) {
            return self.quarantine(phase, FaultKind::Guest, error.to_string());
        }
        if let Err(error) = call(&self.bindings, &mut self.store) {
            // #3050 — the host sets this flag at the point it refuses a log
            // for budget, so the trap that propagates back here can be
            // attributed to the budget rather than to the guest. Read (and
            // cleared) here so a later, genuine fault is not mislabelled.
            let state = self.store.data_mut();
            state.accepting_commands = false;
            state.pending_commands.clear();
            let kind = if std::mem::take(&mut state.log_budget_exhausted) {
                FaultKind::LogBudgetExhausted
            } else if std::mem::take(&mut state.command_budget_exhausted) {
                FaultKind::CommandBudgetExhausted
            } else {
                FaultKind::Guest
            };
            return self.quarantine(phase, kind, format!("{error:#}"));
        }
        self.store.data_mut().accepting_commands = false;
        Ok(())
    }

    fn quarantine<T>(
        &mut self,
        phase: LifecyclePhase,
        kind: FaultKind,
        message: String,
    ) -> Result<T> {
        self.store.data_mut().accepting_commands = false;
        self.store.data_mut().pending_commands.clear();
        self.status = InstanceStatus::Quarantined(FaultInfo {
            phase,
            kind,
            message: message.clone(),
        });
        Err(SandboxError::GuestFault { phase, message })
    }
}

struct HostState {
    principal: Principal,
    grants: CapabilitySet,
    catalog: Arc<ServiceCatalog>,
    limits: StoreLimits,
    logs: Vec<LogEntry>,
    log_bytes: usize,
    max_log_entries: usize,
    max_log_message_bytes: usize,
    max_log_bytes: usize,
    /// Set when a log was refused for budget rather than for content (#3050),
    /// so `ModInstance::enter` can attribute the resulting trap. Cleared by
    /// the reader.
    log_budget_exhausted: bool,
    schemas: Vec<ComponentSchemaDeclaration>,
    subscribed_to_activate: bool,
    subscribed_to_cell_load: bool,
    principal_storage_schema: Option<u32>,
    principal_storage: BTreeMap<StorageKey, ExtensionValue>,
    entity_projections: BTreeMap<byroredux_sdk::identity::EntityRef, EntityProjection>,
    pending_commands: Vec<HostCommand>,
    max_commands_per_entry: usize,
    accepting_commands: bool,
    command_budget_exhausted: bool,
}

impl state::Host for HostState {
    fn queue_increment_own_i64(
        &mut self,
        entity: state::EntityRef,
        schema_index: u32,
        field_index: u32,
        delta: i64,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("state commands are only accepted during an event callback");
        }
        if !self.grants.contains(COMPONENTS_WRITE_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {COMPONENTS_WRITE_OWN_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        let schema = self
            .schemas
            .get(schema_index as usize)
            .ok_or_else(|| wasmtime::Error::msg(format!("unknown schema index {schema_index}")))?;
        let field = schema
            .fields
            .get(field_index as usize)
            .ok_or_else(|| wasmtime::Error::msg(format!("unknown field index {field_index}")))?;
        if field.value_type != ExtensionValueType::I64 {
            wasmtime::bail!("field {} in schema {} is not an i64", field.id, schema.id);
        }
        let entity =
            byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
                .ok_or_else(|| {
                    wasmtime::Error::msg("entity reference contains a reserved zero value")
                })?;
        self.pending_commands
            .push(HostCommand::Component(ExtensionCommand::IncrementI64 {
                entity,
                schema: schema.id.clone(),
                field: field.id.clone(),
                delta,
            }));
        Ok(())
    }
}

impl wit_storage::Host for HostState {
    fn schema_version(&mut self) -> wasmtime::Result<Option<u32>> {
        Ok(self.principal_storage_schema)
    }

    fn get(&mut self, key: String) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = StorageKey::new(key)
            .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))?;
        Ok(self.principal_storage.get(&key).map(|value| match value {
            ExtensionValue::Bool(value) => wit_storage::Value::Boolean(*value),
            ExtensionValue::I64(value) => wit_storage::Value::Signed(*value),
            ExtensionValue::U64(value) => wit_storage::Value::Unsigned(*value),
            ExtensionValue::String(value) => wit_storage::Value::Text(value.clone()),
            ExtensionValue::Bytes(value) => wit_storage::Value::Bytes(value.clone()),
        }))
    }

    fn queue_set(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = StorageKey::new(key)
            .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))?;
        let value = match value {
            wit_storage::Value::Boolean(value) => ExtensionValue::Bool(value),
            wit_storage::Value::Signed(value) => ExtensionValue::I64(value),
            wit_storage::Value::Unsigned(value) => ExtensionValue::U64(value),
            wit_storage::Value::Text(value) => ExtensionValue::String(value),
            wit_storage::Value::Bytes(value) => ExtensionValue::Bytes(value),
        };
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Set { key, value },
        ));
        Ok(())
    }

    fn queue_delete(&mut self, key: String) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = StorageKey::new(key)
            .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Delete { key },
        ));
        Ok(())
    }

    fn queue_increment_i64(&mut self, key: String, delta: i64) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = StorageKey::new(key)
            .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::IncrementI64 { key, delta },
        ));
        Ok(())
    }
}

impl world_state::Host for HostState {
    fn contains_entity(&mut self, entity: state::EntityRef) -> wasmtime::Result<bool> {
        self.require_world_entity_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self.entity_projections.contains_key(&entity))
    }

    fn get_entity(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<world_state::EntityProjection>> {
        self.require_world_entity_read()?;
        let entity = sdk_entity_ref(entity)?;
        let Some(projection) = self.entity_projections.get(&entity) else {
            return Ok(None);
        };
        Ok(Some(wit_entity_projection(
            projection,
            self.grants.contains(WORLD_TRANSFORM_READ_CAPABILITY),
        )))
    }
}

fn sdk_entity_ref(
    entity: state::EntityRef,
) -> wasmtime::Result<byroredux_sdk::identity::EntityRef> {
    byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
        .ok_or_else(|| wasmtime::Error::msg("entity reference contains a reserved zero value"))
}

fn wit_entity_projection(
    projection: &EntityProjection,
    include_transform: bool,
) -> world_state::EntityProjection {
    let entity = projection.entity();
    let form = projection.form().map(|form| {
        let source = form.source();
        world_state::FormRef {
            source_high: u64::from_be_bytes(
                source[..8].try_into().expect("eight-byte source half"),
            ),
            source_low: u64::from_be_bytes(source[8..].try_into().expect("eight-byte source half")),
            local: form.local(),
        }
    });
    let world_transform = include_transform
        .then(|| projection.world_transform())
        .flatten()
        .map(|transform| {
            let [x, y, z] = transform.translation();
            let [qx, qy, qz, qw] = transform.rotation();
            world_state::WorldTransform {
                translation: world_state::Vec3 { x, y, z },
                rotation: world_state::Quat {
                    x: qx,
                    y: qy,
                    z: qz,
                    w: qw,
                },
                scale: transform.scale(),
            }
        });
    world_state::EntityProjection {
        entity: state::EntityRef {
            world_generation: entity.world_generation(),
            object: entity.object(),
        },
        form,
        name: projection.name().map(str::to_owned),
        world_transform,
    }
}

impl HostState {
    fn require_world_entity_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(WORLD_ENTITY_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {WORLD_ENTITY_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    fn require_storage(&self, capability: &str) -> wasmtime::Result<()> {
        if self.principal_storage_schema.is_none() {
            wasmtime::bail!(
                "principal {} did not declare persistent storage",
                self.principal.id()
            );
        }
        if !self.grants.contains(capability) {
            wasmtime::bail!(
                "principal {} lacks capability {capability}",
                self.principal.id()
            );
        }
        Ok(())
    }

    fn require_storage_write(&mut self) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("storage commands are only accepted during an event callback");
        }
        self.require_storage(STORAGE_WRITE_OWN_CAPABILITY)?;
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        Ok(())
    }
}

impl logging::Host for HostState {
    fn log(&mut self, level: logging::Level, message: String) -> wasmtime::Result<()> {
        if !self.grants.contains(LOG_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {LOG_CAPABILITY}",
                self.principal.id()
            );
        }
        if message.len() > self.max_log_message_bytes {
            wasmtime::bail!(
                "log message is {} bytes, exceeding the {}-byte limit",
                message.len(),
                self.max_log_message_bytes
            );
        }
        // #3050 — these two are budget, not misbehaviour: they bound what the
        // host is holding undrained, and `ModInstance::take_logs` returns
        // both. Flag the distinction for `enter` before bailing. The
        // per-message size check above is deliberately NOT flagged: an
        // oversized single message is the guest breaking a per-call contract,
        // which no amount of draining changes.
        if self.logs.len() >= self.max_log_entries {
            self.log_budget_exhausted = true;
            wasmtime::bail!(
                "log entry limit of {} exceeded ({} undrained); \
                 drain with ModInstance::take_logs",
                self.max_log_entries,
                self.logs.len()
            );
        }
        let next_log_bytes = self
            .log_bytes
            .checked_add(message.len())
            .ok_or_else(|| wasmtime::Error::msg("log byte count overflow"))?;
        if next_log_bytes > self.max_log_bytes {
            self.log_budget_exhausted = true;
            wasmtime::bail!(
                "log byte limit of {} exceeded ({} undrained); \
                 drain with ModInstance::take_logs",
                self.max_log_bytes,
                self.log_bytes
            );
        }

        self.log_bytes = next_log_bytes;
        self.logs.push(LogEntry {
            principal: self.principal.id().clone(),
            level: match level {
                logging::Level::Debug => LogLevel::Debug,
                logging::Level::Info => LogLevel::Info,
                logging::Level::Warn => LogLevel::Warn,
                logging::Level::Error => LogLevel::Error,
            },
            message,
        });
        Ok(())
    }
}

impl context::Host for HostState {
    fn principal_id(&mut self) -> wasmtime::Result<String> {
        Ok(self.principal.id().as_str().to_owned())
    }

    fn has_capability(&mut self, capability: String) -> wasmtime::Result<bool> {
        Ok(self.grants.contains(&capability))
    }

    fn sdk_version(&mut self) -> wasmtime::Result<String> {
        Ok(self.catalog.sdk_version().to_string())
    }

    fn service_version(&mut self, service: String) -> wasmtime::Result<Option<String>> {
        Ok(self
            .catalog
            .service_version(&service)
            .map(ToString::to_string))
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use byroredux_sdk::identity::FormRef;
    use byroredux_sdk::projection::WorldTransform;

    #[test]
    fn wit_projection_preserves_portable_fields_and_redacts_transform_without_grant() {
        let entity = byroredux_sdk::identity::EntityRef::new(2, 9).unwrap();
        let form = FormRef::new([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], 42);
        let transform = WorldTransform::new([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], 2.0).unwrap();
        let projection =
            EntityProjection::new(entity, Some(form), Some("Door".to_owned()), Some(transform))
                .unwrap();

        let visible = wit_entity_projection(&projection, true);
        assert_eq!(visible.entity.world_generation, 2);
        assert_eq!(visible.entity.object, 9);
        assert_eq!(
            visible.form.as_ref().unwrap().source_high,
            0x0001_0203_0405_0607
        );
        assert_eq!(
            visible.form.as_ref().unwrap().source_low,
            0x0809_0a0b_0c0d_0e0f
        );
        assert_eq!(visible.form.as_ref().unwrap().local, 42);
        assert_eq!(visible.name.as_deref(), Some("Door"));
        assert_eq!(visible.world_transform.as_ref().unwrap().translation.x, 1.0);
        assert_eq!(visible.world_transform.as_ref().unwrap().scale, 2.0);

        assert!(wit_entity_projection(&projection, false)
            .world_transform
            .is_none());
    }
}
