//! The scheduler-registered ECS systems, the resources they read, and
//! console-command registration.
//!
//! This is the only region that the engine schedule names directly;
//! everything else is reached through it or through `ExtensionHostSlot`.

use super::*;

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

    pub(super) fn replace_host(&mut self, host: ExtensionHost) {
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

pub(crate) fn sync_extension_script_function_invoker(world: &World) {
    let host = world
        .try_resource::<ExtensionHostSlot>()
        .and_then(|slot| slot.host());
    let catalog = host
        .as_ref()
        .and_then(|host| host.lock().ok().map(|host| host.papyrus_provider_catalog()))
        .unwrap_or_default();
    let callback = host.as_ref().map(|host| {
        let host = Arc::clone(host);
        Arc::new(move |name: &str, arguments: &[ScriptValue]| {
            host.lock()
                .map_err(|_| "extension host mutex was poisoned".to_owned())?
                .invoke_papyrus_provider(name, arguments)
                .map_err(|error| error.to_string())
        }) as Arc<byroredux_scripting::ExtensionScriptFunctionCallback>
    });
    let provider_callback = host.as_ref().map(|host| {
        let host = Arc::clone(host);
        Arc::new(
            move |principal: Option<&PrincipalId>, name: &str, arguments: &[ScriptValue]| {
                host.lock()
                    .map_err(|_| "extension host mutex was poisoned".to_owned())?
                    .invoke_owned_papyrus_provider(principal, name, arguments)
                    .map_err(|error| error.to_string())
            },
        ) as Arc<byroredux_scripting::PapyrusProviderCallback>
    });
    let mod_event_publisher = host.as_ref().map(|host| {
        let host = Arc::clone(host);
        Arc::new(
            move |principal: &PrincipalId, command: PublishEventCommand| {
                host.lock()
                    .map_err(|_| "extension host mutex was poisoned".to_owned())?
                    .enqueue_published_event(principal, command, "SendModEvent")
                    .map_err(|error| error.to_string())
            },
        ) as Arc<byroredux_scripting::PapyrusProviderModEventPublisher>
    });
    let entity_resolver = host.map(|host| {
        Arc::new(move |entity: EntityId| {
            host.lock()
                .map_err(|_| "extension host mutex was poisoned".to_owned())?
                .handles
                .handle_for(entity)
                .map_err(|error| error.to_string())
        }) as Arc<byroredux_scripting::PapyrusProviderEntityResolver>
    });
    byroredux_scripting::set_extension_script_function_invoker(world, callback);
    byroredux_scripting::set_papyrus_provider_runtime(world, Arc::new(catalog), provider_callback);
    byroredux_scripting::set_papyrus_provider_entity_resolver(world, entity_resolver);
    byroredux_scripting::set_papyrus_provider_mod_event_publisher(world, mod_event_publisher);
}

/// Register archive-backed legacy script packages as active engine principals.
/// This must run before save restoration so their private storage records are
/// rebound instead of retained as belonging to an unavailable package.
pub(crate) fn register_legacy_script_principals(
    world: &World,
    principals: impl IntoIterator<Item = PrincipalId>,
) -> anyhow::Result<()> {
    let host = world
        .try_resource::<ExtensionHostSlot>()
        .and_then(|slot| slot.host())
        .ok_or_else(|| anyhow::anyhow!("extension host is unavailable"))?;
    let mut host = host
        .lock()
        .map_err(|_| anyhow::anyhow!("extension host mutex was poisoned"))?;
    for principal in principals {
        host.register_legacy_script_principal(principal)?;
    }
    Ok(())
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
    pub(crate) events: Vec<SessionEvent>,
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
    let papyrus_mod_events = host
        .pending_custom_events
        .iter()
        .filter(|event| is_legacy_skse_mod_event_id(&event.event))
        .cloned()
        .collect::<Vec<_>>();
    let stats = host.dispatch_pending_custom_events();
    apply_pending_world_commands(world, &mut host);
    let diagnostics = host.take_diagnostics();
    drop(host);
    for event in papyrus_mod_events {
        byroredux_scripting::queue_papyrus_mod_event(world, event);
    }
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
    let (resolver, catalog, faction_relationships) = {
        let Some(resolver) =
            world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
        else {
            byroredux_scripting::set_papyrus_provider_form_resolver(world, None);
            return;
        };
        (
            resolver.clone(),
            resolver.content_catalog(),
            resolver.faction_relationships(),
        )
    };
    let form_resolver = Arc::new(move |form_id: u32| {
        resolver
            .resolve(form_id)
            .map(form_ref)
            .ok_or_else(|| format!("global FormID {form_id:#010x} is not in the active load order"))
    }) as Arc<byroredux_scripting::PapyrusProviderFormResolver>;
    byroredux_scripting::set_papyrus_provider_form_resolver(world, Some(form_resolver));
    // Built-in legacy scripts consume the same immutable snapshot as sandbox
    // guests. Publish it even when no ExtensionHost is configured so OBSE/
    // xNVSE-compatible engine behavior never depends on an external runtime.
    byroredux_scripting::set_legacy_obscript_content_catalog(world, Arc::clone(&catalog));
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

/// Publish the current normalized keyboard bindings to the Papyrus Input
/// compatibility adapter before any provider callback executes.
pub(crate) fn extension_input_bindings_sync_system(world: &World, _dt: f32) {
    let bindings = world
        .try_resource::<crate::interaction::ActionBindings>()
        .map_or_else(Vec::new, |bindings| bindings.papyrus_bindings());
    let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
        return;
    };
    let Some(host) = slot.host() else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    host.set_input_bindings(bindings);
}

/// Publish the process-lifetime engine player body to the provider host.
///
/// `Game.GetPlayer()` returns the same generational opaque handle as other
/// entity projections. Keeping this synchronization in the late stage means
/// fly-camera/test worlds correctly expose `None`, while a recreated player
/// body is never captured by a long-lived callback closure.
pub(crate) fn extension_player_entity_sync_system(world: &World, _dt: f32) {
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|resource| resource.0);
    let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
        return;
    };
    let Some(host) = slot.host() else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    host.set_player_entity(player);
}

/// Publish the active engine-owned menu snapshot before provider callbacks run.
pub(crate) fn extension_ui_menu_sync(world: &World, active_menu: Option<&str>, visible: bool) {
    let snapshot = PapyrusUiMenuSnapshot {
        active_menu: active_menu
            .filter(|menu| visible && !menu.is_empty())
            .map(str::to_owned),
        visible,
    };
    let Some(slot) = world.try_resource::<ExtensionHostSlot>() else {
        return;
    };
    let Some(host) = slot.host() else {
        return;
    };
    let mut host = host
        .lock()
        .expect("ExtensionHost mutex poisoned by a host panic");
    host.set_ui_menu_snapshot(snapshot);
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

pub(super) fn settings_snapshot_from_registry(
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

pub(super) fn register_extension_setting(
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
        .try_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
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

pub(super) fn emit_diagnostics(diagnostics: Vec<ExtensionDiagnostic>) {
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
