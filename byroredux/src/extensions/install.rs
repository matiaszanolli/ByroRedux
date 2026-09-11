//! Host lifecycle and host-service dispatch: construction, package
//! installation, and the console / Papyrus-provider / mod-event entry
//! points a guest reaches through the service table.
//!
//! Also owns `load_requested_extensions`, the CLI-driven profile load —
//! installation's only production caller.

use super::*;

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
            legacy_script_principals: BTreeSet::new(),
            legacy_random_state: Self::legacy_random_seed(),
            legacy_containers: BTreeMap::new(),
            legacy_mod_event_builders: BTreeMap::new(),
            retained_legacy_containers: Vec::new(),
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
            script_functions: Vec::new(),
            papyrus_providers: PapyrusProviderCatalog::engine_compatibility(),
            input_bindings: Vec::new(),
            ui_menu_snapshot: PapyrusUiMenuSnapshot::default(),
            player_entity: None,
        })
    }

    fn legacy_random_seed() -> u64 {
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let seed = (elapsed.as_nanos() as u64)
            ^ elapsed.as_secs().rotate_left(17)
            ^ u64::from(std::process::id());
        if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        }
    }

    pub(super) fn next_legacy_random_selector(&mut self) -> u64 {
        let mut state = self.legacy_random_state;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.legacy_random_state = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
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
        let mut staged_papyrus_providers = self.papyrus_providers.clone();
        if grants.contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY) {
            for function in &manifest.script_functions {
                staged_papyrus_providers
                    .insert(&manifest.id, function)
                    .map_err(|error| {
                        ExtensionHostError::PapyrusProviderAlias(format!("{error:?}"))
                    })?;
            }
        }
        let mut staged_state = self.state.clone();
        let mut staged_principal_storage = self.principal_storage.clone();
        let mut staged_retained_storage = self.retained_storage.clone();
        let mut staged_legacy_containers = self.legacy_containers.clone();
        let mut staged_retained_legacy_containers = self.retained_legacy_containers.clone();
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
            return Err(PrincipalStorageError::UndeclaredPrincipal(principal.clone()).into());
        }
        if let Some(index) = staged_retained_legacy_containers
            .iter()
            .position(|record| record.principal == principal)
        {
            let retained = staged_retained_legacy_containers.remove(index);
            retained.registry.validate()?;
            staged_legacy_containers.insert(principal.clone(), retained.registry);
        } else {
            staged_legacy_containers
                .entry(principal.clone())
                .or_default();
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
            instance.set_legacy_container_snapshot(
                staged_legacy_containers
                    .get(&principal)
                    .cloned()
                    .unwrap_or_default(),
            );
            instance.initialize()?;
            staged_legacy_containers.insert(
                principal.clone(),
                instance.legacy_container_snapshot().clone(),
            );
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
        self.legacy_containers = staged_legacy_containers;
        self.retained_legacy_containers = staged_retained_legacy_containers;
        self.papyrus_providers = staged_papyrus_providers;
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
        if grants.contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY) {
            self.script_functions
                .extend(
                    manifest
                        .script_functions
                        .iter()
                        .enumerate()
                        .map(|(index, function)| HostedScriptFunction {
                            name: function.qualified_name(&manifest.id),
                            extension: manifest.id.clone(),
                            component: function.component.clone(),
                            declaration_index: u32::try_from(index)
                                .expect("manifest script function count is bounded below u32::MAX"),
                            declaration: function.clone(),
                        }),
                );
        }
        self.diagnostics.extend(staged_diagnostics);
        Ok(())
    }

    pub(super) fn console_commands(&self) -> &[HostedConsoleCommand] {
        &self.console_commands
    }

    pub(super) fn papyrus_provider_catalog(&self) -> PapyrusProviderCatalog {
        self.papyrus_providers.clone()
    }

    pub(super) fn invoke_console_command(
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
        let principal = enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
            delivery_commit_context!(self, stats),
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

    /// Invoke an exact engine-native compatibility alias or a sandboxed
    /// principal-namespaced function through the shared typed boundary.
    pub(super) fn invoke_papyrus_provider(
        &mut self,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        self.invoke_owned_papyrus_provider(None, qualified_name, arguments)
    }

    pub(super) fn invoke_owned_papyrus_provider(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        if qualified_name.starts_with("byro.storage.compat.storage-util.") {
            return self.invoke_storage_util(principal, qualified_name, arguments);
        }
        if qualified_name.starts_with(PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX) {
            return self.invoke_legacy_container(principal, qualified_name, arguments);
        }
        if qualified_name.starts_with(PAPYRUS_MOD_EVENT_ROUTE_PREFIX) {
            return self.invoke_mod_event(principal, qualified_name, arguments);
        }
        let value = match (qualified_name, arguments) {
            (PAPYRUS_GAME_GET_PLAYER_ROUTE, []) => match self.player_entity {
                Some(entity) => ScriptValue::Entity(self.handles.handle_for(entity)?),
                None => ScriptValue::None,
            },
            (PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE, [ScriptValue::String(control)]) => {
                ScriptValue::Integer(i64::from(adapt_papyrus_input_get_mapped_key(
                    &self.input_bindings,
                    control,
                    0xff,
                )))
            }
            (
                PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE,
                [ScriptValue::String(control), ScriptValue::Integer(device_type)],
            ) => ScriptValue::Integer(i64::from(adapt_papyrus_input_get_mapped_key(
                &self.input_bindings,
                control,
                *device_type,
            ))),
            (PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE, [ScriptValue::Integer(keycode)]) => {
                ScriptValue::String(adapt_papyrus_input_get_mapped_control(
                    &self.input_bindings,
                    *keycode,
                ))
            }
            (PAPYRUS_UI_IS_MENU_OPEN_ROUTE, [ScriptValue::String(menu_name)]) => {
                ScriptValue::Boolean(adapt_papyrus_ui_is_menu_open(
                    &self.ui_menu_snapshot,
                    menu_name,
                ))
            }
            (PAPYRUS_GAME_GET_MOD_COUNT_ROUTE, []) => ScriptValue::Integer(i64::from(
                adapt_papyrus_game_get_mod_count(&self.content_catalog),
            )),
            (PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE, [ScriptValue::String(plugin)]) => {
                ScriptValue::Integer(i64::from(adapt_papyrus_game_get_mod_by_name(
                    &self.content_catalog,
                    plugin,
                )))
            }
            (
                PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE,
                [ScriptValue::Integer(form_id), ScriptValue::String(plugin)],
            ) => adapt_papyrus_game_get_form_from_file(&self.content_catalog, *form_id, plugin)
                .map_or(ScriptValue::None, ScriptValue::Form),
            (PAPYRUS_GAME_GET_MOD_NAME_ROUTE, [ScriptValue::Integer(index)]) => {
                ScriptValue::String(adapt_papyrus_game_get_mod_name(
                    &self.content_catalog,
                    *index,
                ))
            }
            (PAPYRUS_GAME_GET_MOD_DEPENDENCY_COUNT_ROUTE, [ScriptValue::Integer(index)]) => {
                ScriptValue::Integer(i64::from(adapt_papyrus_game_get_mod_dependency_count(
                    &self.content_catalog,
                    *index,
                )))
            }
            (PAPYRUS_GAME_IS_PLUGIN_INSTALLED_ROUTE, [ScriptValue::String(plugin)]) => {
                ScriptValue::Boolean(adapt_papyrus_game_is_plugin_installed(
                    &self.content_catalog,
                    plugin,
                ))
            }
            (PAPYRUS_GAME_GET_LIGHT_MOD_COUNT_ROUTE, []) => ScriptValue::Integer(i64::from(
                adapt_papyrus_game_get_light_mod_count(&self.content_catalog),
            )),
            (PAPYRUS_GAME_GET_LIGHT_MOD_BY_NAME_ROUTE, [ScriptValue::String(plugin)]) => {
                ScriptValue::Integer(i64::from(adapt_papyrus_game_get_light_mod_by_name(
                    &self.content_catalog,
                    plugin,
                )))
            }
            (PAPYRUS_GAME_GET_LIGHT_MOD_NAME_ROUTE, [ScriptValue::Integer(index)]) => {
                ScriptValue::String(adapt_papyrus_game_get_light_mod_name(
                    &self.content_catalog,
                    *index,
                ))
            }
            (PAPYRUS_GAME_GET_LIGHT_MOD_DEPENDENCY_COUNT_ROUTE, [ScriptValue::Integer(index)]) => {
                ScriptValue::Integer(i64::from(
                    adapt_papyrus_game_get_light_mod_dependency_count(
                        &self.content_catalog,
                        *index,
                    ),
                ))
            }
            (
                PAPYRUS_GAME_GET_NTH_LIGHT_MOD_DEPENDENCY_ROUTE,
                [ScriptValue::Integer(mod_index), ScriptValue::Integer(dependency_index)],
            ) => ScriptValue::Integer(i64::from(adapt_papyrus_game_get_nth_light_mod_dependency(
                &self.content_catalog,
                *mod_index,
                *dependency_index,
            ))),
            (route, _) if route == PAPYRUS_GAME_GET_PLAYER_ROUTE => {
                return Err(ExtensionHostError::ScriptFunctionUnavailable {
                    function: qualified_name.to_owned(),
                    reason: "engine player alias received invalid typed arguments".to_owned(),
                });
            }
            (route, _) if route.starts_with("byro.content.catalog.") => {
                return Err(ExtensionHostError::ScriptFunctionUnavailable {
                    function: qualified_name.to_owned(),
                    reason: "engine content-catalog alias received invalid typed arguments"
                        .to_owned(),
                });
            }
            (route, _) if route.starts_with("byro.ui.") => {
                return Err(ExtensionHostError::ScriptFunctionUnavailable {
                    function: qualified_name.to_owned(),
                    reason: "engine UI alias received invalid typed arguments".to_owned(),
                });
            }
            _ => return self.invoke_script_function(qualified_name, arguments),
        };
        Ok(value)
    }

    pub(super) fn enqueue_published_event(
        &mut self,
        principal: &PrincipalId,
        command: PublishEventCommand,
        function: &str,
    ) -> Result<(), ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: function.to_owned(),
            reason,
        };
        if !custom_event_publishable_by(&command.event, principal) {
            return Err(unavailable(
                "event channel is not publishable by this principal".to_owned(),
            ));
        }
        let event = CustomEvent {
            event: command.event,
            sender: principal.clone(),
            payload: command.payload,
        };
        if !event.is_valid() {
            return Err(unavailable("published event is invalid".to_owned()));
        }
        let next_count = self
            .pending_custom_events
            .len()
            .checked_add(1)
            .ok_or_else(|| unavailable("pending custom event count overflow".to_owned()))?;
        if next_count > MAX_PENDING_CUSTOM_EVENTS {
            return Err(unavailable(format!(
                "pending custom event limit of {MAX_PENDING_CUSTOM_EVENTS} exceeded"
            )));
        }
        let next_bytes = self
            .pending_custom_events
            .iter()
            .try_fold(event.payload.len(), |total, pending| {
                total.checked_add(pending.payload.len())
            })
            .ok_or_else(|| unavailable("pending custom event byte count overflow".to_owned()))?;
        if next_bytes > MAX_PENDING_CUSTOM_EVENT_BYTES {
            return Err(unavailable(format!(
                "pending custom event payload limit of {MAX_PENDING_CUSTOM_EVENT_BYTES} bytes exceeded"
            )));
        }
        self.pending_custom_events.push(event);
        Ok(())
    }

    fn invoke_mod_event(
        &mut self,
        principal: Option<&PrincipalId>,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let unavailable = |reason: String| ExtensionHostError::ScriptFunctionUnavailable {
            function: qualified_name.to_owned(),
            reason,
        };
        let principal = principal.ok_or_else(|| {
            unavailable("ModEvent call has no authenticated legacy-script principal".to_owned())
        })?;
        let operation = qualified_name
            .strip_prefix(PAPYRUS_MOD_EVENT_ROUTE_PREFIX)
            .ok_or_else(|| unavailable("invalid ModEvent engine route".to_owned()))?;
        // Papyrus handles are signed integers while SKSE stores them as
        // unsigned IDs. Any unrepresentable value is simply an invalid handle,
        // preserving the legacy false/no-op behavior.
        let handle = |value: i64| u32::try_from(value).unwrap_or(0);
        let integer = |value: i64| {
            i32::try_from(value).map_err(|_| {
                unavailable("ModEvent integer is outside the Papyrus i32 range".to_owned())
            })
        };
        let mut builders = self
            .legacy_mod_event_builders
            .get(principal)
            .cloned()
            .ok_or_else(|| {
                unavailable(
                    "ModEvent principal has no registered private builder registry".to_owned(),
                )
            })?;

        let result = match (operation, arguments) {
            ("mod-event-create", [ScriptValue::String(event_name)]) => {
                ScriptValue::Integer(i64::from(builders.create(event_name)))
            }
            ("mod-event-release", [ScriptValue::Integer(raw_handle)]) => {
                builders.release(handle(*raw_handle));
                ScriptValue::None
            }
            ("mod-event-send", [ScriptValue::Integer(raw_handle)]) => {
                let Some(command) = builders.send(handle(*raw_handle)) else {
                    self.legacy_mod_event_builders
                        .insert(principal.clone(), builders);
                    return Ok(ScriptValue::Boolean(false));
                };
                self.enqueue_published_event(principal, command, qualified_name)?;
                ScriptValue::Boolean(true)
            }
            (
                "mod-event-push-bool",
                [ScriptValue::Integer(raw_handle), ScriptValue::Boolean(value)],
            ) => {
                builders.push(handle(*raw_handle), LegacySkseModEventValue::Bool(*value));
                ScriptValue::None
            }
            (
                "mod-event-push-int",
                [ScriptValue::Integer(raw_handle), ScriptValue::Integer(value)],
            ) => {
                builders.push(
                    handle(*raw_handle),
                    LegacySkseModEventValue::Int(integer(*value)?),
                );
                ScriptValue::None
            }
            (
                "mod-event-push-float",
                [ScriptValue::Integer(raw_handle), ScriptValue::Float(value)],
            ) => {
                builders.push(handle(*raw_handle), LegacySkseModEventValue::float(*value));
                ScriptValue::None
            }
            (
                "mod-event-push-string",
                [ScriptValue::Integer(raw_handle), ScriptValue::String(value)],
            ) => {
                builders.push(
                    handle(*raw_handle),
                    LegacySkseModEventValue::String(value.clone()),
                );
                ScriptValue::None
            }
            ("mod-event-push-form", [ScriptValue::Integer(raw_handle), value]) => {
                let value = match value {
                    ScriptValue::Form(form) => Some(*form),
                    ScriptValue::None => None,
                    _ => {
                        return Err(unavailable(
                            "ModEvent Form argument has the wrong exact type".to_owned(),
                        ))
                    }
                };
                builders.push(handle(*raw_handle), LegacySkseModEventValue::Form(value));
                ScriptValue::None
            }
            _ => {
                return Err(unavailable(
                    "ModEvent compatibility call has invalid exact typed arguments".to_owned(),
                ))
            }
        };
        self.legacy_mod_event_builders
            .insert(principal.clone(), builders);
        Ok(result)
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

/// Load explicitly requested extension packages as one atomic profile.
///
/// `--extension <manifest.toml>` is repeatable. Capabilities remain denied
/// unless explicitly granted with
/// `--extension-grant <extension-id>=<capability-id>`; `=*` grants every
/// capability requested by that manifest. A failure leaves the default empty
/// host installed, so ordinary content remains usable.
pub(crate) fn load_requested_extensions(world: &World, args: &[String]) -> anyhow::Result<usize> {
    let manifest_paths = flag_values(args, "--extension");
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
    sync_extension_script_function_invoker(world);
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
