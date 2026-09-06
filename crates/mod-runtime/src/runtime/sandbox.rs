//! Engine + linker construction: `SandboxRuntime` and mod compilation.

use super::*;

/// Every capability the catalog advertises, with the description a mod
/// manager shows the player when a mod requests it.
///
/// #3853: hoisted out of `SandboxRuntime::new` so the crate's advertised
/// trust surface can be read as a table instead of scrolled through.
const CAPABILITY_DESCRIPTORS: &[(&str, &str)] = &[
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
        ACTOR_VALUES_READ_CAPABILITY,
        "Read canonical actor values from callback-visible actors",
    ),
    (
        ACTOR_VALUES_WRITE_CAPABILITY,
        "Queue bounded canonical actor-value mutations",
    ),
    (
        ANIMATION_READ_CAPABILITY,
        "Read authored animation state from callback-visible actors",
    ),
    (
        ANIMATION_PLAY_CAPABILITY,
        "Request authored IDLE playback for callback-visible actors",
    ),
    (
        REPUTATION_READ_CAPABILITY,
        "Read canonical fame and infamy axes from callback-visible actors",
    ),
    (
        REPUTATION_WRITE_CAPABILITY,
        "Queue bounded canonical fame and infamy mutations",
    ),
    (
        INVENTORY_READ_CAPABILITY,
        "Read portable inventory and equipment summaries from callback-visible entities",
    ),
    (
        FACTIONS_READ_CAPABILITY,
        "Read portable faction membership ranks from callback-visible actors",
    ),
    (
        FACTION_RELATIONSHIPS_READ_CAPABILITY,
        "Read authored directional relationships between portable factions",
    ),
    (
        PERKS_READ_CAPABILITY,
        "Read portable ranked perks from callback-visible actors",
    ),
    (
        PACKAGES_READ_CAPABILITY,
        "Read live ambient and scene package selections from callback-visible actors",
    ),
    (
        PACKAGES_EVALUATE_CAPABILITY,
        "Request deferred package reevaluation for callback-visible actors",
    ),
    (
        WORLD_SPATIAL_READ_CAPABILITY,
        "Query bounded live authored references by finite world position and radius",
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
        SCRIPT_FUNCTIONS_REGISTER_CAPABILITY,
        "Publish and execute bounded typed principal-namespaced script functions",
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
];

/// Every service the catalog advertises, in registration order, paired with
/// the capability a principal must hold to reach it (`None` = ungated).
///
/// #3853: these were 23 near-identical 11-line `register_service` blocks
/// inside `SandboxRuntime::new`. The gating relation — the only part that
/// matters for review — was invisible behind the boilerplate; four services
/// are ungated and that fact was not previously legible at a glance.
const SERVICE_DESCRIPTORS: &[(&str, Option<&str>)] = &[
    (SETTINGS_SERVICE, Some(SETTINGS_READ_CAPABILITY)),
    (
        SCRIPT_FUNCTIONS_SERVICE,
        Some(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY),
    ),
    (CONSOLE_SERVICE, Some(CONSOLE_REGISTER_CAPABILITY)),
    (
        CONTENT_CATALOG_SERVICE,
        Some(CONTENT_CATALOG_READ_CAPABILITY),
    ),
    (ACTOR_VALUES_SERVICE, Some(ACTOR_VALUES_READ_CAPABILITY)),
    (ANIMATION_SERVICE, Some(ANIMATION_READ_CAPABILITY)),
    (REPUTATION_SERVICE, Some(REPUTATION_READ_CAPABILITY)),
    (INVENTORY_SERVICE, Some(INVENTORY_READ_CAPABILITY)),
    (FACTIONS_SERVICE, Some(FACTIONS_READ_CAPABILITY)),
    (
        FACTION_RELATIONSHIPS_SERVICE,
        Some(FACTION_RELATIONSHIPS_READ_CAPABILITY),
    ),
    (PERKS_SERVICE, Some(PERKS_READ_CAPABILITY)),
    (PACKAGES_SERVICE, Some(PACKAGES_READ_CAPABILITY)),
    (WORLD_SPATIAL_SERVICE, Some(WORLD_SPATIAL_READ_CAPABILITY)),
    (WORLD_PROJECTION_SERVICE, Some(WORLD_ENTITY_READ_CAPABILITY)),
    (CONTEXT_SERVICE, None),
    (INPUT_SERVICE, None),
    (UI_SERVICE, None),
    (PRINCIPAL_STORAGE_SERVICE, Some(STORAGE_READ_OWN_CAPABILITY)),
    (LEGACY_CONTAINERS_SERVICE, Some(STORAGE_READ_OWN_CAPABILITY)),
    (
        COMPONENT_STATE_SERVICE,
        Some(COMPONENTS_WRITE_OWN_CAPABILITY),
    ),
    (EVENT_SERVICE, Some(EVENTS_SUBSCRIBE_CAPABILITY)),
    (EXTENSION_WORLD_SERVICE, None),
    (LOGGING_SERVICE, Some(LOG_CAPABILITY)),
];

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
        for &(id, description) in CAPABILITY_DESCRIPTORS {
            catalog
                .register_capability(CapabilityDescriptor {
                    id: CapabilityId::new(id)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                    description: description.to_owned(),
                })
                .map_err(|error| SandboxError::Link(error.to_string()))?;
        }
        for &(service, required) in SERVICE_DESCRIPTORS {
            let required_capability = match required {
                Some(capability) => Some(
                    CapabilityId::new(capability)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                ),
                None => None,
            };
            catalog
                .register_service(ServiceDescriptor {
                    id: ServiceId::new(service)
                        .map_err(|error| SandboxError::Link(error.to_string()))?,
                    version: Version::new(0, 1, 0),
                    required_capability,
                })
                .map_err(|error| SandboxError::Link(error.to_string()))?;
        }

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
            spatial_snapshot: Arc::new(SpatialSnapshot::default()),
            content_catalog: Arc::new(ContentCatalog::default()),
            faction_relationships: Arc::new(FactionRelationshipCatalog::default()),
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
            legacy_mod_event_callbacks: BTreeMap::new(),
            legacy_mod_event_builders: LegacySkseModEventBuilders::new(),
            legacy_containers: LegacyContainerRegistry::new(),
            current_custom_event: None,
            current_legacy_callback: None,
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
            script_functions: manifest
                .script_functions
                .iter()
                .enumerate()
                .filter(|(_, function)| function.component == compiled.component_id)
                .map(|(index, function)| {
                    (
                        u32::try_from(index)
                            .expect("manifest script function count is bounded below u32::MAX"),
                        function.clone(),
                    )
                })
                .collect(),
            current_script_arguments: None,
            current_script_result: None,
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

#[cfg(test)]
mod descriptor_table_tests {
    use super::*;

    /// #3853 — the four ungated services are a deliberate, reviewed set.
    ///
    /// Before the table hoist these lived as `required_capability: None`
    /// inside 23 near-identical 11-line registration blocks, where an
    /// accidental `None` on a fifth service would have read exactly like
    /// the other 22 and gated nothing. `crates/mod-runtime` is named a
    /// trust boundary in `_audit-common.md`; this pins its ungated
    /// surface so widening it has to be deliberate.
    #[test]
    fn exactly_the_reviewed_services_are_ungated() {
        let ungated: Vec<&str> = SERVICE_DESCRIPTORS
            .iter()
            .filter(|(_, required)| required.is_none())
            .map(|(service, _)| *service)
            .collect();
        assert_eq!(
            ungated,
            vec![
                CONTEXT_SERVICE,
                INPUT_SERVICE,
                UI_SERVICE,
                EXTENSION_WORLD_SERVICE,
            ],
            "the set of services reachable without a capability changed"
        );
    }

    /// A service registered twice would have the second registration
    /// silently win (or error) depending on catalog policy; either way the
    /// table is the wrong place to learn about it.
    #[test]
    fn no_service_is_registered_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for (service, _) in SERVICE_DESCRIPTORS {
            assert!(seen.insert(*service), "{service} registered twice");
        }
        assert_eq!(seen.len(), SERVICE_DESCRIPTORS.len());
    }

    /// Every capability a service gates on must also be advertised as a
    /// capability, or a principal can never be granted it and the service
    /// is unreachable in practice while appearing available in the catalog.
    #[test]
    fn every_gating_capability_is_itself_advertised() {
        let advertised: std::collections::BTreeSet<&str> = CAPABILITY_DESCRIPTORS
            .iter()
            .map(|(id, _)| *id)
            .chain(std::iter::once(LOG_CAPABILITY))
            .collect();
        for (service, required) in SERVICE_DESCRIPTORS {
            if let Some(capability) = required {
                assert!(
                    advertised.contains(capability),
                    "{service} gates on {capability}, which no CapabilityDescriptor advertises"
                );
            }
        }
    }
}
