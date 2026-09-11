//! Guest entry and command write-back.
//!
//! `enter_guest` is the one prologue that loads a guest's snapshots;
//! `apply_delivery_result` is the one epilogue that resolves a returned
//! command batch against engine state. The `Resolved*` types between them
//! exist so resolution happens while ECS guards are held and application
//! happens after they are dropped.

use super::*;

impl ExtensionHost {
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

pub(super) struct DeliveryCommitContext<'a> {
    pub(super) state: &'a mut ExtensionComponentStore,
    pub(super) principal_storage: &'a mut PrincipalStorageStore,
    pub(super) legacy_containers: &'a mut BTreeMap<PrincipalId, LegacyContainerRegistry>,
    pub(super) pending_custom_events: &'a mut Vec<CustomEvent>,
    pub(super) pending_setting_writes: &'a mut Vec<byroredux_sdk::settings::SettingWriteCommand>,
    pub(super) pending_actor_value_writes: &'a mut Vec<ActorValueCommand>,
    pub(super) pending_package_evaluations: &'a mut Vec<EvaluatePackageCommand>,
    pub(super) pending_animation_commands: &'a mut Vec<PlayIdleCommand>,
    pub(super) pending_reputation_writes: &'a mut Vec<ReputationCommand>,
    pub(super) diagnostics: &'a mut Vec<ExtensionDiagnostic>,
    pub(super) stats: &'a mut ExtensionDispatchStats,
}

/// Load a guest's persisted snapshots into its instance and return its
/// principal — the prologue every sandboxed-guest entry point shares (#3863).
///
/// Ten dispatch paths repeated these five statements verbatim: clone the
/// principal id, read the principal-storage values, push them into the
/// instance, read the legacy-container registry, push that in too. The commit
/// half was already factored (`DeliveryCommitContext` /
/// [`apply_delivery_result`]); the entry half never was.
///
/// A free function taking the two stores by reference, not a `&mut self`
/// method: every call site is already holding a `&mut HostedComponent` out of
/// `self.components`, so a method would present a second whole-`self` borrow
/// and conflict. Narrow parameters keep the borrows disjoint.
pub(super) fn enter_guest(
    hosted: &mut HostedComponent,
    principal_storage: &PrincipalStorageStore,
    legacy_containers: &BTreeMap<PrincipalId, LegacyContainerRegistry>,
) -> PrincipalId {
    let principal = hosted.instance.principal().id().clone();
    hosted.instance.set_principal_storage_snapshot(
        principal_storage
            .values(&principal)
            .cloned()
            .unwrap_or_default(),
    );
    hosted.instance.set_legacy_container_snapshot(
        legacy_containers
            .get(&principal)
            .cloned()
            .unwrap_or_default(),
    );
    principal
}

pub(super) fn apply_delivery_result(
    hosted: &mut HostedComponent,
    result: Result<Vec<HostCommand>, SandboxError>,
    phase: LifecyclePhase,
    principal: &PrincipalId,
    context: DeliveryCommitContext<'_>,
) {
    let DeliveryCommitContext {
        state,
        principal_storage,
        legacy_containers,
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
    let mut legacy_event_subscriptions = Vec::new();
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
            HostCommand::LegacyModEventSubscription(command) => {
                legacy_event_subscriptions.push(command)
            }
            HostCommand::PlayIdle(command) => animation_commands.push(command),
            HostCommand::Reputation(command) => reputation_writes.push(command),
            HostCommand::PrincipalStorage(command) => storage_commands.push(command),
            HostCommand::PublishEvent(command) => published_events.push(command),
            HostCommand::Setting(command) => setting_writes.push(command),
        }
    }
    let mut staged_state = state.clone();
    let mut staged_storage = principal_storage.clone();
    let staged_legacy_containers = hosted.instance.legacy_container_snapshot().clone();
    let mut staged_custom_subscriptions = hosted.custom_subscriptions.clone();
    let staged_subscriptions = staged_legacy_containers
        .validate()
        .map_err(|error| error.to_string())
        .and_then(|()| {
            legacy_event_subscriptions.iter().try_for_each(|command| {
                if !command.is_valid() {
                    return Err("legacy mod-event subscription command is invalid".to_owned());
                }
                match command {
                    LegacyModEventSubscriptionCommand::Subscribe { event, .. } => {
                        staged_custom_subscriptions.insert(event.clone());
                    }
                    LegacyModEventSubscriptionCommand::Unsubscribe { event } => {
                        staged_custom_subscriptions.remove(event);
                    }
                    LegacyModEventSubscriptionCommand::UnsubscribeAll => {
                        staged_custom_subscriptions
                            .retain(|event| !is_legacy_skse_mod_event_id(event));
                    }
                }
                Ok(())
            })
        });
    let staged_events = published_events
        .into_iter()
        .map(|command| {
            if !custom_event_publishable_by(&command.event, principal) {
                return Err(format!(
                    "principal {principal} may not publish custom event {}",
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
    let apply_result = staged_subscriptions.and(staged_events).and_then(|staged_events| {
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
            legacy_containers.insert(principal.clone(), staged_legacy_containers);
            hosted.custom_subscriptions = staged_custom_subscriptions;
            hosted
                .instance
                .apply_legacy_mod_event_subscription_commands(&legacy_event_subscriptions);
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

pub(crate) fn animation_event(
    event: byroredux_scripting::CinematicAnimationEvent,
) -> AnimationEvent {
    match event {
        byroredux_scripting::CinematicAnimationEvent::PlayImod => AnimationEvent::PlayImod,
        byroredux_scripting::CinematicAnimationEvent::IdleFurnitureExit => {
            AnimationEvent::IdleFurnitureExit
        }
        byroredux_scripting::CinematicAnimationEvent::ExitCartEnd => AnimationEvent::ExitCartEnd,
    }
}

pub(super) fn apply_pending_actor_value_writes(world: &World, host: &mut ExtensionHost) {
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
        // #3819 — `ActorValues` before `GlobalFormIdResolver`, not the
        // reverse. This was the one site acquiring the pair in
        // `GlobalFormIdResolver → ActorValues` order while
        // `capture_entity_projections` (above) acquires
        // `ActorValues → GlobalFormIdResolver`, closing a cross-thread
        // ABBA cycle the `BYRO_LOCK_ORDER_CHECK=1` graph correctly
        // flagged. `resolver` is dropped immediately after the read loop
        // below (it is unused past that point) so it also doesn't
        // overlap the later `query_mut::<ActorValues>()` write pass.
        let values = world
            .query::<ActorValues>()
            .ok_or_else(|| "ActorValues storage is unavailable".to_owned())?;
        let resolver = world
            .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .ok_or_else(|| "active form resolver is unavailable".to_owned())?;
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
        drop(resolver);
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
        // #3819 — `FactionReputation` before `GlobalFormIdResolver`, not
        // the reverse. Same fix as `apply_pending_actor_value_writes`
        // above: this was the one site acquiring the pair in
        // `GlobalFormIdResolver → FactionReputation` order while
        // `capture_entity_projections` acquires
        // `FactionReputation → GlobalFormIdResolver`, closing a
        // cross-thread ABBA cycle.
        let live = world
            .query::<FactionReputation>()
            .ok_or_else(|| "FactionReputation storage is unavailable".to_owned())?;
        let resolver = world
            .try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .ok_or_else(|| "active form resolver is unavailable".to_owned())?;
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

pub(super) fn apply_pending_world_commands(world: &World, host: &mut ExtensionHost) {
    apply_pending_actor_value_writes(world, host);
    apply_pending_package_evaluations(world, host);
    apply_pending_animation_commands(world, host);
    apply_pending_reputation_writes(world, host);
}
