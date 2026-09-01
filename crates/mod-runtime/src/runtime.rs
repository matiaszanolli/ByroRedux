use crate::bindings::byro::mod_host::{
    console, content_catalog, context, events, logging, state, storage as wit_storage, world_state,
};
use crate::bindings::Extension;
use crate::{CapabilitySet, Principal, Result, SandboxConfig, SandboxError};
use byroredux_sdk::component::{ExtensionCommand, ExtensionValue, ExtensionValueType};
use byroredux_sdk::console::{
    ConsoleCommandResult, MAX_CONSOLE_ARGUMENT_BYTES, MAX_CONSOLE_OUTPUT_BYTES,
    MAX_CONSOLE_OUTPUT_LINES, MAX_CONSOLE_OUTPUT_LINE_BYTES,
};
use byroredux_sdk::content::{ContentCatalog, PluginKind, MAX_PLUGIN_NAME_BYTES};
use byroredux_sdk::event::{
    custom_event_owned_by, is_custom_event_id, ActivationEvent, CellLoadEvent, CustomEvent,
    EquipmentEvent, HitEvent, InputAction, InputActionEvent, InputPhase, PublishEventCommand,
    SessionEvent, SessionPhase, UpdateEvent,
};
use byroredux_sdk::identity::{
    CapabilityId, ComponentId, EntityRef, EventId, ExtensionId, PrincipalId, ServiceId, StorageKey,
};
use byroredux_sdk::manifest::{ComponentSchemaDeclaration, ExtensionManifest};
use byroredux_sdk::projection::EntityProjection;
use byroredux_sdk::service::{
    current_sdk_version, CapabilityDescriptor, ServiceCatalog, ServiceDescriptor, ACTIVATE_EVENT,
    CELL_LOAD_EVENT, COMPONENTS_WRITE_OWN_CAPABILITY, COMPONENT_STATE_SERVICE,
    CONSOLE_REGISTER_CAPABILITY, CONSOLE_SERVICE, CONTENT_CATALOG_READ_CAPABILITY,
    CONTENT_CATALOG_SERVICE, CONTEXT_SERVICE, EQUIPMENT_EVENT, EVENTS_PUBLISH_CAPABILITY,
    EVENTS_SUBSCRIBE_CAPABILITY, EVENT_SERVICE, EXTENSION_WORLD_SERVICE, HIT_EVENT,
    INPUT_ACTIONS_SUBSCRIBE_CAPABILITY, INPUT_ACTION_EVENT, LOGGING_SERVICE,
    PRINCIPAL_STORAGE_SERVICE, SESSION_EVENT, SETTINGS_READ_CAPABILITY,
    SETTINGS_REGISTER_CAPABILITY, SETTINGS_SERVICE, SETTINGS_WRITE_OWN_CAPABILITY,
    STORAGE_READ_OWN_CAPABILITY, STORAGE_WRITE_OWN_CAPABILITY, UPDATE_EVENT,
    WORLD_ENTITY_READ_CAPABILITY, WORLD_PROJECTION_SERVICE, WORLD_TRANSFORM_READ_CAPABILITY,
};
use byroredux_sdk::settings::{
    SettingDeclaration, SettingValue, SettingWriteCommand, SettingsSnapshot, MAX_SETTING_KEY_BYTES,
};
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
                EVENTS_PUBLISH_CAPABILITY,
                "Publish bounded events in the authenticated principal namespace",
            ),
            (
                INPUT_ACTIONS_SUBSCRIBE_CAPABILITY,
                "Observe normalized player input actions after rebinding",
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
            (
                CONTENT_CATALOG_READ_CAPABILITY,
                "Inspect loaded game plugins and qualify portable authored forms",
            ),
            (
                CONSOLE_REGISTER_CAPABILITY,
                "Publish and execute bounded principal-namespaced console commands",
            ),
            (
                SETTINGS_READ_CAPABILITY,
                "Read stable typed public engine settings",
            ),
            (
                SETTINGS_REGISTER_CAPABILITY,
                "Register bounded principal-namespaced engine settings",
            ),
            (
                SETTINGS_WRITE_OWN_CAPABILITY,
                "Queue writes to principal-owned declared engine settings",
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
                id: ServiceId::new(SETTINGS_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(SETTINGS_READ_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(CONSOLE_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(CONSOLE_REGISTER_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
        catalog
            .register_service(ServiceDescriptor {
                id: ServiceId::new(CONTENT_CATALOG_SERVICE)
                    .map_err(|error| SandboxError::Link(error.to_string()))?,
                version: Version::new(0, 1, 0),
                required_capability: Some(
                    CapabilityId::new(CONTENT_CATALOG_READ_CAPABILITY)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
            })
            .map_err(|error| SandboxError::Link(error.to_string()))?;
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
            content_catalog: Arc::new(ContentCatalog::default()),
            engine_settings: Arc::new(SettingsSnapshot::default()),
            setting_declarations: manifest.settings.clone(),
            subscribed_to_activate: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == ACTIVATE_EVENT),
            subscribed_to_cell_load: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == CELL_LOAD_EVENT),
            subscribed_to_hit: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == HIT_EVENT),
            subscribed_to_equipment: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == EQUIPMENT_EVENT),
            subscribed_to_input: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == INPUT_ACTION_EVENT),
            subscribed_to_session: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == SESSION_EVENT),
            custom_subscriptions: manifest
                .subscriptions
                .iter()
                .filter(|subscription| is_custom_event_id(&subscription.event))
                .map(|subscription| subscription.event.clone())
                .collect(),
            current_custom_event: None,
            current_console_args: None,
            console_command_indices: manifest
                .console_commands
                .iter()
                .enumerate()
                .filter(|(_, command)| command.component == compiled.component_id)
                .map(|(index, _)| {
                    u32::try_from(index)
                        .expect("manifest console command count is bounded below u32::MAX")
                })
                .collect(),
            console_output: Vec::new(),
            console_output_bytes: 0,
            console_failed: false,
            console_output_budget_exhausted: false,
            subscribed_to_update: manifest
                .subscriptions
                .iter()
                .any(|subscription| subscription.event.as_str() == UPDATE_EVENT),
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

    /// Deliver one canonical combat hit and return deferred commands.
    pub fn on_hit(&mut self, event: HitEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Hit,
                status: self.status.clone(),
            });
        }
        let event_id =
            EventId::new(HIT_EVENT).expect("the engine's canonical hit event id is valid");
        if !self.store.data().subscribed_to_hit {
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
        if !event.damage.is_finite() || event.damage < 0.0 {
            return Err(SandboxError::InvalidEventPayload {
                event: event_id,
                message: format!(
                    "damage must be finite and non-negative, got {}",
                    event.damage
                ),
            });
        }
        let entity = |entity: EntityRef| state::EntityRef {
            world_generation: entity.world_generation(),
            object: entity.object(),
        };
        let result = self.enter(LifecyclePhase::Hit, true, |bindings, store| {
            bindings.call_on_hit(
                store,
                entity(event.subject),
                event.aggressor.map(entity),
                event.source.map(entity),
                event.projectile.map(entity),
                state::HitDetails {
                    damage: event.damage,
                    power_attack: event.power_attack,
                    sneak_attack: event.sneak_attack,
                    bash_attack: event.bash_attack,
                    blocked: event.blocked,
                },
            )
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Deliver one canonical equipment transition and return deferred commands.
    pub fn on_equipment_change(&mut self, event: EquipmentEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Equipment,
                status: self.status.clone(),
            });
        }
        let event_id = EventId::new(EQUIPMENT_EVENT)
            .expect("the engine's canonical equipment event id is valid");
        if !self.store.data().subscribed_to_equipment {
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
        let wearer = state::EntityRef {
            world_generation: event.wearer.world_generation(),
            object: event.wearer.object(),
        };
        let item = wit_form_ref(event.item);
        let result = self.enter(LifecyclePhase::Equipment, true, |bindings, store| {
            bindings.call_on_equipment_change(store, wearer, item, event.equipped)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    pub fn on_input_action(&mut self, event: InputActionEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Input,
                status: self.status.clone(),
            });
        }
        let event_id = EventId::new(INPUT_ACTION_EVENT)
            .expect("the engine's canonical input action event id is valid");
        if !self.store.data().subscribed_to_input {
            return Err(SandboxError::EventNotSubscribed(event_id));
        }
        if !self
            .store
            .data()
            .grants
            .contains(EVENTS_SUBSCRIBE_CAPABILITY)
            || !self
                .store
                .data()
                .grants
                .contains(INPUT_ACTIONS_SUBSCRIBE_CAPABILITY)
        {
            return Err(SandboxError::EventDeliveryDenied(event_id));
        }
        let action = match event.action {
            InputAction::MoveForward => state::InputAction::MoveForward,
            InputAction::MoveBackward => state::InputAction::MoveBackward,
            InputAction::StrafeLeft => state::InputAction::StrafeLeft,
            InputAction::StrafeRight => state::InputAction::StrafeRight,
            InputAction::Jump => state::InputAction::Jump,
            InputAction::Sprint => state::InputAction::Sprint,
            InputAction::Activate => state::InputAction::Activate,
            InputAction::Attack => state::InputAction::Attack,
            InputAction::Block => state::InputAction::Block,
            InputAction::Inventory => state::InputAction::Inventory,
            InputAction::Quicksave => state::InputAction::Quicksave,
            InputAction::Quickload => state::InputAction::Quickload,
            InputAction::Pause => state::InputAction::Pause,
        };
        let phase = match event.phase {
            InputPhase::Pressed => state::InputPhase::Pressed,
            InputPhase::Released => state::InputPhase::Released,
        };
        let result = self.enter(LifecyclePhase::Input, true, |bindings, store| {
            bindings.call_on_input_action(store, action, phase)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Deliver one committed game-session transition.
    pub fn on_session_event(&mut self, event: SessionEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Session,
                status: self.status.clone(),
            });
        }
        let event_id =
            EventId::new(SESSION_EVENT).expect("the engine's canonical session event id is valid");
        if !self.store.data().subscribed_to_session {
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
        if !event.is_valid() {
            return Err(SandboxError::InvalidEventPayload {
                event: event_id,
                message: format!(
                    "phase {} requires {} slot",
                    event.phase.as_str(),
                    if event.phase == SessionPhase::NewGame {
                        "no"
                    } else {
                        "one"
                    }
                ),
            });
        }
        let phase = match event.phase {
            SessionPhase::NewGame => state::SessionPhase::NewGame,
            SessionPhase::SaveComplete => state::SessionPhase::SaveComplete,
            SessionPhase::LoadComplete => state::SessionPhase::LoadComplete,
        };
        let result = self.enter(LifecyclePhase::Session, true, |bindings, store| {
            bindings.call_on_session_event(store, phase, event.slot)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Deliver one principal-owned custom event to an exact manifest subscriber.
    ///
    /// The callback receives the manifest subscription index. Its opaque bytes
    /// are readable only for the duration of this callback through `events`.
    pub fn on_custom_event(&mut self, event: CustomEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::CustomEvent,
                status: self.status.clone(),
            });
        }
        let Some(subscription_index) = self
            .store
            .data()
            .custom_subscriptions
            .iter()
            .position(|subscribed| subscribed == &event.event)
        else {
            return Err(SandboxError::EventNotSubscribed(event.event));
        };
        if !self
            .store
            .data()
            .grants
            .contains(EVENTS_SUBSCRIBE_CAPABILITY)
        {
            return Err(SandboxError::EventDeliveryDenied(event.event));
        }
        if !event.is_valid() {
            return Err(SandboxError::InvalidEventPayload {
                event: event.event,
                message: "custom event namespace or payload is invalid".to_owned(),
            });
        }
        let subscription_index = u32::try_from(subscription_index)
            .expect("manifest subscription count is bounded below u32::MAX");
        self.store.data_mut().current_custom_event = Some(event);
        let result = self.enter(LifecyclePhase::CustomEvent, true, |bindings, store| {
            bindings.call_on_custom_event(store, subscription_index)
        });
        self.store.data_mut().current_custom_event = None;
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Deliver one bounded recurring callback and return deferred commands.
    pub fn on_update(&mut self, event: UpdateEvent) -> Result<Vec<HostCommand>> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Update,
                status: self.status.clone(),
            });
        }
        let event_id =
            EventId::new(UPDATE_EVENT).expect("the engine's canonical update event id is valid");
        if !self.store.data().subscribed_to_update {
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
        if !event.elapsed_seconds.is_finite() || event.elapsed_seconds < 0.0 {
            return Err(SandboxError::InvalidEventPayload {
                event: event_id,
                message: format!(
                    "elapsed_seconds must be finite and non-negative, got {}",
                    event.elapsed_seconds
                ),
            });
        }
        let result = self.enter(LifecyclePhase::Update, true, |bindings, store| {
            bindings.call_on_update(store, event.elapsed_seconds)
        });
        self.store.data_mut().entity_projections.clear();
        result?;
        Ok(std::mem::take(&mut self.store.data_mut().pending_commands))
    }

    /// Invoke one manifest-declared console command by stable declaration index.
    pub fn on_console_command(
        &mut self,
        command_index: u32,
        args: &str,
    ) -> Result<(ConsoleCommandResult, Vec<HostCommand>)> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::ConsoleCommand,
                status: self.status.clone(),
            });
        }
        if !self
            .store
            .data()
            .grants
            .contains(CONSOLE_REGISTER_CAPABILITY)
        {
            return Err(SandboxError::GuestFault {
                phase: LifecyclePhase::ConsoleCommand,
                message: format!(
                    "principal {} lacks capability {CONSOLE_REGISTER_CAPABILITY}",
                    self.principal().id()
                ),
            });
        }
        if !self
            .store
            .data()
            .console_command_indices
            .contains(&command_index)
        {
            return Err(SandboxError::GuestFault {
                phase: LifecyclePhase::ConsoleCommand,
                message: format!(
                    "console command index {command_index} is not declared for this component"
                ),
            });
        }
        if args.len() > MAX_CONSOLE_ARGUMENT_BYTES {
            return Err(SandboxError::GuestFault {
                phase: LifecyclePhase::ConsoleCommand,
                message: format!(
                    "console arguments are {} bytes, exceeding {MAX_CONSOLE_ARGUMENT_BYTES}",
                    args.len()
                ),
            });
        }
        {
            let state = self.store.data_mut();
            state.current_console_args = Some(args.as_bytes().to_vec());
            state.console_output.clear();
            state.console_output_bytes = 0;
            state.console_failed = false;
            state.console_output_budget_exhausted = false;
        }
        let result = self.enter(LifecyclePhase::ConsoleCommand, true, |bindings, store| {
            bindings.call_on_console_command(store, command_index)
        });
        self.store.data_mut().current_console_args = None;
        result?;
        let state = self.store.data_mut();
        let output = ConsoleCommandResult {
            success: !state.console_failed,
            lines: std::mem::take(&mut state.console_output),
        };
        state.console_output_bytes = 0;
        let commands = std::mem::take(&mut state.pending_commands);
        Ok((output, commands))
    }

    /// Replace the read-only principal storage snapshot visible to callbacks.
    pub fn set_principal_storage_snapshot(
        &mut self,
        values: BTreeMap<StorageKey, PrincipalStorageValue>,
    ) {
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

    /// Replace the immutable loaded-content catalog visible to host calls.
    pub fn set_content_catalog_snapshot(&mut self, catalog: Arc<ContentCatalog>) {
        self.store.data_mut().content_catalog = catalog;
    }

    /// Replace the immutable public engine-settings snapshot.
    pub fn set_engine_settings_snapshot(&mut self, settings: Arc<SettingsSnapshot>) {
        self.store.data_mut().engine_settings = settings;
    }

    /// Quarantine an active instance after the host rejects its deferred
    /// command batch.
    ///
    /// Guest callbacks cannot mutate the live world directly. Consequently,
    /// validation can still fail after a callback returns—for example when a
    /// checked counter overflows or a principal exhausts its row budget. The
    /// engine owner calls this method before reporting that rejection so the
    /// component cannot repeatedly submit the same invalid batch.
    pub fn reject_deferred_commands(
        &mut self,
        phase: LifecyclePhase,
        message: impl Into<String>,
    ) -> SandboxError {
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
            } else if std::mem::take(&mut state.console_output_budget_exhausted) {
                FaultKind::ConsoleOutputBudgetExhausted
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
    subscribed_to_hit: bool,
    subscribed_to_equipment: bool,
    subscribed_to_input: bool,
    subscribed_to_session: bool,
    custom_subscriptions: Vec<EventId>,
    current_custom_event: Option<CustomEvent>,
    current_console_args: Option<Vec<u8>>,
    console_command_indices: std::collections::BTreeSet<u32>,
    console_output: Vec<String>,
    console_output_bytes: usize,
    console_failed: bool,
    console_output_budget_exhausted: bool,
    subscribed_to_update: bool,
    principal_storage_schema: Option<u32>,
    principal_storage: BTreeMap<StorageKey, PrincipalStorageValue>,
    entity_projections: BTreeMap<byroredux_sdk::identity::EntityRef, EntityProjection>,
    content_catalog: Arc<ContentCatalog>,
    engine_settings: Arc<SettingsSnapshot>,
    setting_declarations: Vec<SettingDeclaration>,
    pending_commands: Vec<HostCommand>,
    max_commands_per_entry: usize,
    accepting_commands: bool,
    command_budget_exhausted: bool,
}

impl events::Host for HostState {
    fn publish(&mut self, event: String, payload: Vec<u8>) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("custom events are only accepted during an event callback");
        }
        if !self.grants.contains(EVENTS_PUBLISH_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {EVENTS_PUBLISH_CAPABILITY}",
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
        let event = EventId::new(event)
            .map_err(|error| wasmtime::Error::msg(format!("invalid custom event id: {error}")))?;
        if !custom_event_owned_by(&event, self.principal.id()) {
            wasmtime::bail!(
                "principal {} does not own custom event {}",
                self.principal.id(),
                event
            );
        }
        let command = PublishEventCommand::new(event, payload).ok_or_else(|| {
            wasmtime::Error::msg("custom event id or payload exceeds the SDK contract")
        })?;
        self.pending_commands
            .push(HostCommand::PublishEvent(command));
        Ok(())
    }

    fn current_payload_len(&mut self) -> wasmtime::Result<u32> {
        let event = self.current_custom_event.as_ref().ok_or_else(|| {
            wasmtime::Error::msg("custom event payload is only visible during on-custom-event")
        })?;
        Ok(u32::try_from(event.payload.len())
            .expect("custom event payload is bounded below u32::MAX"))
    }

    fn current_payload_byte(&mut self, index: u32) -> wasmtime::Result<Option<u8>> {
        let event = self.current_custom_event.as_ref().ok_or_else(|| {
            wasmtime::Error::msg("custom event payload is only visible during on-custom-event")
        })?;
        Ok(event.payload.get(index as usize).copied())
    }
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
        let key = sdk_storage_key(key)?;
        Ok(self
            .principal_storage
            .get(&key)
            .and_then(PrincipalStorageValue::as_scalar)
            .map(wit_storage_value))
    }

    fn queue_set(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        let value = sdk_storage_value(value);
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Set { key, value },
        ));
        Ok(())
    }

    fn queue_delete(&mut self, key: String) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::Delete { key },
        ));
        Ok(())
    }

    fn queue_increment_i64(&mut self, key: String, delta: i64) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        let key = sdk_storage_key(key)?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::IncrementI64 { key, delta },
        ));
        Ok(())
    }

    fn get_collection_kind(
        &mut self,
        key: String,
    ) -> wasmtime::Result<Option<wit_storage::CollectionKind>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        Ok(match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(_)) => Some(wit_storage::CollectionKind::Array),
            Some(PrincipalStorageValue::Map(_)) => {
                Some(wit_storage::CollectionKind::AssociativeMap)
            }
            Some(PrincipalStorageValue::Set(_)) => Some(wit_storage::CollectionKind::Set),
            _ => None,
        })
    }

    fn collection_len(&mut self, key: String) -> wasmtime::Result<Option<u32>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        let length = match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(values)) => values.len(),
            Some(PrincipalStorageValue::Map(values)) => values.len(),
            Some(PrincipalStorageValue::Set(values)) => values.len(),
            _ => return Ok(None),
        };
        Ok(Some(u32::try_from(length).expect(
            "storage collection length is bounded below u32::MAX",
        )))
    }

    fn array_get(
        &mut self,
        key: String,
        index: u32,
    ) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Array(values)) => {
                Ok(values.get(index as usize).map(wit_storage_value_ref))
            }
            Some(_) => wasmtime::bail!("storage key {key} is not an array"),
            None => Ok(None),
        }
    }

    fn queue_array_push(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArrayPush {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_array_set(
        &mut self,
        key: String,
        index: u32,
        value: wit_storage::Value,
    ) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArraySet {
                key: sdk_storage_key(key)?,
                index,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_array_remove(&mut self, key: String, index: u32) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::ArrayRemove {
                key: sdk_storage_key(key)?,
                index,
            },
        ));
        Ok(())
    }

    fn map_get(
        &mut self,
        key: String,
        entry: String,
    ) -> wasmtime::Result<Option<wit_storage::Value>> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Map(values)) => {
                Ok(values.get(&entry).map(wit_storage_value_ref))
            }
            Some(_) => wasmtime::bail!("storage key {key} is not a map"),
            None => Ok(None),
        }
    }

    fn queue_map_set(
        &mut self,
        key: String,
        entry: String,
        value: wit_storage::Value,
    ) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::MapSet {
                key: sdk_storage_key(key)?,
                entry,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_map_delete(&mut self, key: String, entry: String) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::MapDelete {
                key: sdk_storage_key(key)?,
                entry,
            },
        ));
        Ok(())
    }

    fn set_contains(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<bool> {
        self.require_storage(STORAGE_READ_OWN_CAPABILITY)?;
        let key = sdk_storage_key(key)?;
        let value = sdk_storage_value(value);
        match self.principal_storage.get(&key) {
            Some(PrincipalStorageValue::Set(values)) => Ok(values.contains(&value)),
            Some(_) => wasmtime::bail!("storage key {key} is not a set"),
            None => Ok(false),
        }
    }

    fn queue_set_insert(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::SetInsert {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }

    fn queue_set_remove(&mut self, key: String, value: wit_storage::Value) -> wasmtime::Result<()> {
        self.require_storage_write()?;
        self.pending_commands.push(HostCommand::PrincipalStorage(
            PrincipalStorageCommand::SetRemove {
                key: sdk_storage_key(key)?,
                value: sdk_storage_value(value),
            },
        ));
        Ok(())
    }
}

fn sdk_storage_key(key: String) -> wasmtime::Result<StorageKey> {
    StorageKey::new(key)
        .map_err(|error| wasmtime::Error::msg(format!("invalid storage key: {error}")))
}

fn sdk_storage_value(value: wit_storage::Value) -> ExtensionValue {
    match value {
        wit_storage::Value::Boolean(value) => ExtensionValue::Bool(value),
        wit_storage::Value::Signed(value) => ExtensionValue::I64(value),
        wit_storage::Value::Unsigned(value) => ExtensionValue::U64(value),
        wit_storage::Value::Text(value) => ExtensionValue::String(value),
        wit_storage::Value::Bytes(value) => ExtensionValue::Bytes(value),
    }
}

fn wit_storage_value(value: ExtensionValue) -> wit_storage::Value {
    wit_storage_value_ref(&value)
}

fn wit_storage_value_ref(value: &ExtensionValue) -> wit_storage::Value {
    match value {
        ExtensionValue::Bool(value) => wit_storage::Value::Boolean(*value),
        ExtensionValue::I64(value) => wit_storage::Value::Signed(*value),
        ExtensionValue::U64(value) => wit_storage::Value::Unsigned(*value),
        ExtensionValue::String(value) => wit_storage::Value::Text(value.clone()),
        ExtensionValue::Bytes(value) => wit_storage::Value::Bytes(value.clone()),
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

impl content_catalog::Host for HostState {
    fn plugin_count(&mut self) -> wasmtime::Result<u32> {
        self.require_content_catalog_read()?;
        Ok(u32::try_from(self.content_catalog.len())
            .expect("content catalog is bounded below u32::MAX"))
    }

    fn plugin_at(&mut self, index: u32) -> wasmtime::Result<Option<content_catalog::PluginInfo>> {
        self.require_content_catalog_read()?;
        Ok(self.content_catalog.plugin(index).map(|plugin| {
            let source = plugin.source();
            content_catalog::PluginInfo {
                name: plugin.name().to_owned(),
                source_high: u64::from_be_bytes(
                    source[..8].try_into().expect("eight-byte source half"),
                ),
                source_low: u64::from_be_bytes(
                    source[8..].try_into().expect("eight-byte source half"),
                ),
                kind: match plugin.kind() {
                    PluginKind::Regular => content_catalog::PluginKind::Regular,
                    PluginKind::Light => content_catalog::PluginKind::Light,
                },
            }
        }))
    }

    fn find_plugin(&mut self, name: String) -> wasmtime::Result<Option<u32>> {
        self.require_content_catalog_read()?;
        validate_plugin_query(&name)?;
        Ok(self.content_catalog.find(&name).map(|(index, _)| index))
    }

    fn qualify_form(
        &mut self,
        plugin: String,
        local: u32,
    ) -> wasmtime::Result<Option<state::FormRef>> {
        self.require_content_catalog_read()?;
        validate_plugin_query(&plugin)?;
        Ok(self
            .content_catalog
            .qualify_form(&plugin, local)
            .map(wit_form_ref))
    }
}

impl console::Host for HostState {
    fn args_len(&mut self) -> wasmtime::Result<u32> {
        self.require_console_context()?;
        Ok(
            u32::try_from(self.current_console_args.as_ref().map_or(0, Vec::len))
                .expect("console arguments are bounded below u32::MAX"),
        )
    }

    fn args_byte(&mut self, index: u32) -> wasmtime::Result<Option<u8>> {
        self.require_console_context()?;
        Ok(self
            .current_console_args
            .as_ref()
            .and_then(|args| args.get(index as usize).copied()))
    }

    fn write_line(&mut self, message: String) -> wasmtime::Result<()> {
        self.require_console_context()?;
        let next_bytes = self
            .console_output_bytes
            .checked_add(message.len())
            .ok_or_else(|| wasmtime::Error::msg("console output byte count overflow"))?;
        if message.len() > MAX_CONSOLE_OUTPUT_LINE_BYTES
            || message.chars().any(char::is_control)
            || self.console_output.len() >= MAX_CONSOLE_OUTPUT_LINES
            || next_bytes > MAX_CONSOLE_OUTPUT_BYTES
        {
            self.console_output_budget_exhausted = true;
            wasmtime::bail!("console output exceeds its bounded line or byte contract");
        }
        self.console_output_bytes = next_bytes;
        self.console_output.push(message);
        Ok(())
    }

    fn set_failed(&mut self) -> wasmtime::Result<()> {
        self.require_console_context()?;
        self.console_failed = true;
        Ok(())
    }
}

fn validate_plugin_query(name: &str) -> wasmtime::Result<()> {
    if name.is_empty()
        || name.len() > MAX_PLUGIN_NAME_BYTES
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
    {
        wasmtime::bail!("invalid plugin basename query");
    }
    Ok(())
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
    let form = projection.form().map(wit_form_ref);
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

fn wit_form_ref(form: byroredux_sdk::identity::FormRef) -> state::FormRef {
    let source = form.source();
    state::FormRef {
        source_high: u64::from_be_bytes(source[..8].try_into().expect("eight-byte source half")),
        source_low: u64::from_be_bytes(source[8..].try_into().expect("eight-byte source half")),
        local: form.local(),
    }
}

impl HostState {
    fn require_console_context(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(CONSOLE_REGISTER_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {CONSOLE_REGISTER_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.current_console_args.is_none() {
            wasmtime::bail!("console host calls are only available during a console callback");
        }
        Ok(())
    }

    fn require_world_entity_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(WORLD_ENTITY_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {WORLD_ENTITY_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    fn require_content_catalog_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(CONTENT_CATALOG_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {CONTENT_CATALOG_READ_CAPABILITY}",
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

    fn engine_setting(&mut self, key: String) -> wasmtime::Result<Option<context::SettingValue>> {
        if !self.grants.contains(SETTINGS_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {SETTINGS_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        if key.is_empty() || key.len() > MAX_SETTING_KEY_BYTES || key.chars().any(char::is_control)
        {
            wasmtime::bail!("invalid engine setting key");
        }
        Ok(self.engine_settings.get(&key).map(|value| match value {
            SettingValue::Boolean(value) => context::SettingValue::Boolean(*value),
            SettingValue::Number(value) => context::SettingValue::Number(*value),
            SettingValue::Choice(value) => context::SettingValue::Choice(value.clone()),
        }))
    }

    fn queue_own_setting(
        &mut self,
        declaration_index: u32,
        value: context::SettingValue,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("setting writes are only accepted during an event callback");
        }
        if !self.grants.contains(SETTINGS_WRITE_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {SETTINGS_WRITE_OWN_CAPABILITY}",
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
        let declaration = self
            .setting_declarations
            .get(declaration_index as usize)
            .ok_or_else(|| wasmtime::Error::msg("unknown setting declaration index"))?;
        let value = match value {
            context::SettingValue::Boolean(value) => SettingValue::Boolean(value),
            context::SettingValue::Number(value) => SettingValue::Number(value),
            context::SettingValue::Choice(value) => SettingValue::Choice(value),
        };
        if !declaration.accepts(&value) {
            wasmtime::bail!("setting value does not satisfy its declared control");
        }
        self.pending_commands
            .push(HostCommand::Setting(SettingWriteCommand {
                key: declaration.qualified_name(
                    &ExtensionId::new(self.principal.id().as_str())
                        .expect("extension principals retain a valid extension identity"),
                ),
                value,
            }));
        Ok(())
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use byroredux_sdk::content::PluginInfo;
    use byroredux_sdk::identity::FormRef;
    use byroredux_sdk::projection::WorldTransform;
    use std::collections::{BTreeMap, BTreeSet};

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

    #[test]
    fn collection_snapshot_reads_preserve_kind_length_and_primitive_values() {
        let mut grants = CapabilitySet::new();
        grants.grant(STORAGE_READ_OWN_CAPABILITY).unwrap();
        let mut state = HostState {
            principal: Principal::new(
                PrincipalId::new("org.example.collections").unwrap(),
                "Collections".to_owned(),
            )
            .unwrap(),
            grants,
            catalog: Arc::new(ServiceCatalog::new(current_sdk_version())),
            limits: StoreLimitsBuilder::new().build(),
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: 1,
            max_log_message_bytes: 1,
            max_log_bytes: 1,
            log_budget_exhausted: false,
            schemas: Vec::new(),
            subscribed_to_activate: false,
            subscribed_to_cell_load: false,
            subscribed_to_hit: false,
            subscribed_to_equipment: false,
            subscribed_to_input: false,
            subscribed_to_session: false,
            custom_subscriptions: Vec::new(),
            current_custom_event: None,
            current_console_args: None,
            console_command_indices: BTreeSet::new(),
            console_output: Vec::new(),
            console_output_bytes: 0,
            console_failed: false,
            console_output_budget_exhausted: false,
            subscribed_to_update: false,
            principal_storage_schema: Some(1),
            principal_storage: BTreeMap::from([
                (
                    StorageKey::new("array").unwrap(),
                    PrincipalStorageValue::Array(vec![ExtensionValue::I64(7)]),
                ),
                (
                    StorageKey::new("map").unwrap(),
                    PrincipalStorageValue::Map(BTreeMap::from([(
                        "entry".to_owned(),
                        ExtensionValue::String("value".to_owned()),
                    )])),
                ),
                (
                    StorageKey::new("set").unwrap(),
                    PrincipalStorageValue::Set(BTreeSet::from([ExtensionValue::U64(9)])),
                ),
            ]),
            entity_projections: BTreeMap::new(),
            content_catalog: Arc::new(ContentCatalog::default()),
            engine_settings: Arc::new(SettingsSnapshot::default()),
            setting_declarations: Vec::new(),
            pending_commands: Vec::new(),
            max_commands_per_entry: 1,
            accepting_commands: false,
            command_budget_exhausted: false,
        };

        assert!(matches!(
            <HostState as wit_storage::Host>::get_collection_kind(&mut state, "array".to_owned())
                .unwrap(),
            Some(wit_storage::CollectionKind::Array)
        ));
        assert_eq!(
            <HostState as wit_storage::Host>::collection_len(&mut state, "map".to_owned()).unwrap(),
            Some(1)
        );
        assert!(matches!(
            <HostState as wit_storage::Host>::array_get(&mut state, "array".to_owned(), 0).unwrap(),
            Some(wit_storage::Value::Signed(7))
        ));
        assert!(matches!(
            <HostState as wit_storage::Host>::map_get(
                &mut state,
                "map".to_owned(),
                "entry".to_owned()
            )
            .unwrap(),
            Some(wit_storage::Value::Text(value)) if value == "value"
        ));
        assert!(<HostState as wit_storage::Host>::set_contains(
            &mut state,
            "set".to_owned(),
            wit_storage::Value::Unsigned(9),
        )
        .unwrap());
    }

    fn content_host_state(granted: bool) -> HostState {
        let mut grants = CapabilitySet::new();
        if granted {
            grants.grant(CONTENT_CATALOG_READ_CAPABILITY).unwrap();
        }
        HostState {
            principal: Principal::new(
                PrincipalId::new("org.example.content").unwrap(),
                "Content".to_owned(),
            )
            .unwrap(),
            grants,
            catalog: Arc::new(ServiceCatalog::new(current_sdk_version())),
            limits: StoreLimitsBuilder::new().build(),
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: 1,
            max_log_message_bytes: 1,
            max_log_bytes: 1,
            log_budget_exhausted: false,
            schemas: Vec::new(),
            subscribed_to_activate: false,
            subscribed_to_cell_load: false,
            subscribed_to_hit: false,
            subscribed_to_equipment: false,
            subscribed_to_input: false,
            subscribed_to_session: false,
            custom_subscriptions: Vec::new(),
            current_custom_event: None,
            current_console_args: None,
            console_command_indices: BTreeSet::new(),
            console_output: Vec::new(),
            console_output_bytes: 0,
            console_failed: false,
            console_output_budget_exhausted: false,
            subscribed_to_update: false,
            principal_storage_schema: None,
            principal_storage: BTreeMap::new(),
            entity_projections: BTreeMap::new(),
            content_catalog: Arc::new(
                ContentCatalog::new(vec![
                    PluginInfo::new("Skyrim.esm", 1_u128.to_be_bytes(), PluginKind::Regular)
                        .unwrap(),
                    PluginInfo::new("Creation.esl", 2_u128.to_be_bytes(), PluginKind::Light)
                        .unwrap(),
                ])
                .unwrap(),
            ),
            engine_settings: Arc::new(
                SettingsSnapshot::new([
                    ("render.vsync".to_owned(), SettingValue::Boolean(false)),
                    ("gameplay.fov".to_owned(), SettingValue::Number(120.0)),
                    (
                        "render.upscaler".to_owned(),
                        SettingValue::Choice("taa".to_owned()),
                    ),
                ])
                .unwrap(),
            ),
            setting_declarations: Vec::new(),
            pending_commands: Vec::new(),
            max_commands_per_entry: 1,
            accepting_commands: false,
            command_budget_exhausted: false,
        }
    }

    #[test]
    fn content_catalog_host_reads_are_portable_case_insensitive_and_capability_gated() {
        let mut state = content_host_state(true);
        assert_eq!(
            <HostState as content_catalog::Host>::plugin_count(&mut state).unwrap(),
            2
        );
        let plugin = <HostState as content_catalog::Host>::plugin_at(&mut state, 1)
            .unwrap()
            .unwrap();
        assert_eq!(plugin.name, "Creation.esl");
        assert!(matches!(plugin.kind, content_catalog::PluginKind::Light));
        assert_eq!(plugin.source_high, 0);
        assert_eq!(plugin.source_low, 2);
        assert_eq!(
            <HostState as content_catalog::Host>::find_plugin(
                &mut state,
                "CREATION.ESL".to_owned(),
            )
            .unwrap(),
            Some(1)
        );
        let form = <HostState as content_catalog::Host>::qualify_form(
            &mut state,
            "creation.esl".to_owned(),
            0xabc,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (form.source_high, form.source_low, form.local),
            (0, 2, 0xabc)
        );
        assert!(<HostState as content_catalog::Host>::qualify_form(
            &mut state,
            "Creation.esl".to_owned(),
            0x1000,
        )
        .unwrap()
        .is_none());

        let mut denied = content_host_state(false);
        let error = <HostState as content_catalog::Host>::plugin_count(&mut denied).unwrap_err();
        assert!(error.to_string().contains(CONTENT_CATALOG_READ_CAPABILITY));
        assert!(<HostState as content_catalog::Host>::find_plugin(
            &mut state,
            "../escape.esm".to_owned(),
        )
        .is_err());
    }

    #[test]
    fn engine_settings_are_typed_bounded_and_capability_gated() {
        let mut state = content_host_state(true);
        state.grants.grant(SETTINGS_READ_CAPABILITY).unwrap();
        assert!(matches!(
            <HostState as context::Host>::engine_setting(&mut state, "gameplay.fov".to_owned(),)
                .unwrap(),
            Some(context::SettingValue::Number(120.0))
        ));
        assert!(
            <HostState as context::Host>::engine_setting(&mut state, "unknown".to_owned())
                .unwrap()
                .is_none()
        );

        let mut denied = content_host_state(false);
        assert!(<HostState as context::Host>::engine_setting(
            &mut denied,
            "render.vsync".to_owned(),
        )
        .is_err());
        assert!(
            <HostState as context::Host>::engine_setting(&mut state, "bad\nkey".to_owned(),)
                .is_err()
        );
    }

    #[test]
    fn own_setting_writes_are_deferred_typed_and_capability_gated() {
        let mut state = content_host_state(false);
        state.grants.grant(SETTINGS_WRITE_OWN_CAPABILITY).unwrap();
        state.accepting_commands = true;
        state.setting_declarations = vec![SettingDeclaration {
            id: byroredux_sdk::identity::SettingId::new("strength").unwrap(),
            label: "Strength".to_owned(),
            description: "Effect strength".to_owned(),
            default: SettingValue::Number(1.0),
            control: byroredux_sdk::settings::SettingControlDeclaration::Slider {
                min: 0.0,
                max: 2.0,
                step: 0.1,
                unit: "x".to_owned(),
            },
            restart_required: false,
        }];

        <HostState as context::Host>::queue_own_setting(
            &mut state,
            0,
            context::SettingValue::Number(1.5),
        )
        .unwrap();
        assert!(matches!(
            state.pending_commands.as_slice(),
            [HostCommand::Setting(SettingWriteCommand { key, value: SettingValue::Number(1.5) })]
                if key == "ext.org.example.content.strength"
        ));
        assert!(<HostState as context::Host>::queue_own_setting(
            &mut state,
            0,
            context::SettingValue::Number(3.0),
        )
        .is_err());

        let mut denied = content_host_state(false);
        denied.accepting_commands = true;
        denied.setting_declarations = state.setting_declarations;
        assert!(<HostState as context::Host>::queue_own_setting(
            &mut denied,
            0,
            context::SettingValue::Number(1.0),
        )
        .is_err());
    }
}
