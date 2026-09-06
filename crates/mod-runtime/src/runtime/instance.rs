//! `ModInstance` — the per-mod guest handle and its lifecycle entry points.

use super::*;

/// A linked component isolated in its own Wasmtime store and principal state.
pub struct ModInstance {
    pub(crate) store: Store<HostState>,
    pub(crate) bindings: Extension,
    pub(crate) fuel_per_entry: u64,
    pub(crate) status: InstanceStatus,
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

    /// Apply a successfully committed legacy registration batch.
    pub fn apply_legacy_mod_event_subscription_commands(
        &mut self,
        commands: &[LegacyModEventSubscriptionCommand],
    ) {
        let data = self.store.data_mut();
        for command in commands {
            match command {
                LegacyModEventSubscriptionCommand::Subscribe { event, callback } => {
                    if !data.custom_subscriptions.contains(event) {
                        data.custom_subscriptions.push(event.clone());
                    }
                    data.legacy_mod_event_callbacks
                        .insert(event.clone(), callback.clone());
                }
                LegacyModEventSubscriptionCommand::Unsubscribe { event } => {
                    data.custom_subscriptions
                        .retain(|candidate| candidate != event);
                    data.legacy_mod_event_callbacks.remove(event);
                }
                LegacyModEventSubscriptionCommand::UnsubscribeAll => {
                    data.custom_subscriptions
                        .retain(|event| !is_legacy_skse_mod_event_id(event));
                    data.legacy_mod_event_callbacks.clear();
                }
            }
        }
    }

    /// Return the Papyrus callback registered for one compatibility channel.
    pub fn legacy_mod_event_callback(&self, event: &EventId) -> Option<&str> {
        self.store
            .data()
            .legacy_mod_event_callbacks
            .get(event)
            .map(String::as_str)
    }

    /// Deliver one custom event to an exact static or runtime subscriber.
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
            .expect("bounded subscription count is below u32::MAX");
        let callback = self
            .store
            .data()
            .legacy_mod_event_callbacks
            .get(&event.event)
            .cloned();
        self.store.data_mut().current_custom_event = Some(event);
        self.store.data_mut().current_legacy_callback = callback;
        let result = self.enter(LifecyclePhase::CustomEvent, true, |bindings, store| {
            bindings.call_on_custom_event(store, subscription_index)
        });
        self.store.data_mut().current_custom_event = None;
        self.store.data_mut().current_legacy_callback = None;
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

    /// Invoke one manifest-declared typed function. Host-supplied arguments
    /// are rejected before guest entry; guest result violations quarantine the
    /// component and discard every deferred command from the call.
    pub fn on_script_function(
        &mut self,
        function_index: u32,
        arguments: &[ScriptValue],
    ) -> Result<(ScriptValue, Vec<HostCommand>)> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::ScriptFunction,
                status: self.status.clone(),
            });
        }
        if !self
            .store
            .data()
            .grants
            .contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY)
        {
            return Err(SandboxError::InvalidScriptFunctionCall {
                function_index,
                message: format!(
                    "principal {} lacks capability {SCRIPT_FUNCTIONS_REGISTER_CAPABILITY}",
                    self.principal().id()
                ),
            });
        }
        let Some(declaration) = self
            .store
            .data()
            .script_functions
            .get(&function_index)
            .cloned()
        else {
            return Err(SandboxError::InvalidScriptFunctionCall {
                function_index,
                message: "function is not declared for this component".to_owned(),
            });
        };
        declaration.validate_arguments(arguments).map_err(|error| {
            SandboxError::InvalidScriptFunctionCall {
                function_index,
                message: error.to_string(),
            }
        })?;

        {
            let state = self.store.data_mut();
            state.current_script_arguments = Some(arguments.to_vec());
            state.current_script_result = None;
        }
        let result = self.enter(LifecyclePhase::ScriptFunction, true, |bindings, store| {
            bindings.call_on_script_function(store, function_index)
        });
        self.store.data_mut().current_script_arguments = None;
        result?;

        let Some(value) = self.store.data_mut().current_script_result.take() else {
            return self.quarantine(
                LifecyclePhase::ScriptFunction,
                FaultKind::Guest,
                "script function returned without setting a result".to_owned(),
            );
        };
        if let Err(error) = declaration.validate_result(&value) {
            return self.quarantine(
                LifecyclePhase::ScriptFunction,
                FaultKind::Guest,
                error.to_string(),
            );
        }
        let commands = std::mem::take(&mut self.store.data_mut().pending_commands);
        Ok((value, commands))
    }

    /// Replace the read-only principal storage snapshot visible to callbacks.
    pub fn set_principal_storage_snapshot(
        &mut self,
        values: BTreeMap<StorageKey, PrincipalStorageValue>,
    ) {
        self.store.data_mut().principal_storage = values;
    }

    /// Replace the principal-local JContainers compatibility object table.
    pub fn set_legacy_container_snapshot(&mut self, registry: LegacyContainerRegistry) {
        self.store.data_mut().legacy_containers = registry;
    }

    /// Snapshot the compatibility object table after a successful guest entry.
    pub fn legacy_container_snapshot(&self) -> &LegacyContainerRegistry {
        &self.store.data().legacy_containers
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

    /// Replace the bounded live authored-reference snapshot used by spatial queries.
    pub fn set_spatial_snapshot(&mut self, snapshot: Arc<SpatialSnapshot>) {
        self.store.data_mut().spatial_snapshot = snapshot;
    }

    /// Replace the immutable loaded-content catalog visible to host calls.
    pub fn set_content_catalog_snapshot(&mut self, catalog: Arc<ContentCatalog>) {
        self.store.data_mut().content_catalog = catalog;
    }

    /// Replace the immutable authored faction-relationship snapshot.
    pub fn set_faction_relationships_snapshot(
        &mut self,
        relationships: Arc<FactionRelationshipCatalog>,
    ) {
        self.store.data_mut().faction_relationships = relationships;
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
