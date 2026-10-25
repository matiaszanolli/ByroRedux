//! Execution (back-end): the statement interpreter that runs the IR
//! against a live `World`.
//!
//! The module doc on the old single file claimed the whole thing
//! "never enters Wasm or touches the ECS while lowering" — true of the
//! front half, and this is the half it was not describing.

use super::*;
use crate::{QuestFormId, QuestScriptVariables};
use byroredux_plugin::esm::records::condition::ConditionStringId;

/// Where a reference's provider `OnInit` having run is kept, among its saved
/// script variables — the provider twin of the Papyrus VM's per-script
/// `::init::<script>` flag.
fn provider_initialized_key() -> ConditionStringId {
    ConditionStringId::from_text("::init::papyrus_provider")
}

/// Keep only the `OnInit` markers that are a reference's *first*: `OnInit`
/// "will run once, when the script is initialized" (CK), but the program is
/// re-attached every time the reference spawns — each stream-in, cell
/// re-entry and save load — so a non-idempotent body (a StorageUtil list add,
/// a counter) would otherwise replay into saved state each time (#6017). The
/// flag is keyed by the reference's FormID in the saved
/// [`QuestScriptVariables`], so it outlives the entity. An entity with no
/// FormID has nothing to key on and keeps its marker.
fn first_initializations(world: &World, initialized: BTreeSet<EntityId>) -> BTreeSet<EntityId> {
    if initialized.is_empty() {
        return initialized;
    }
    // Resolve the owners before the variables guard is taken, so no
    // component / pool lock is acquired underneath it.
    let owners: Vec<(EntityId, Option<u32>)> = initialized
        .into_iter()
        .map(|entity| (entity, crate::condition::entity_global_form_id(world, entity)))
        .collect();
    let Some(mut variables) = world.try_resource_mut::<QuestScriptVariables>() else {
        return owners.into_iter().map(|(entity, _)| entity).collect();
    };
    let key = provider_initialized_key();
    owners
        .into_iter()
        .filter(|(_, owner)| {
            let Some(owner) = owner else {
                return true;
            };
            let owner = QuestFormId(*owner);
            if variables.get(owner, key).is_some() {
                return false;
            }
            variables.set(owner, key, 1.0);
            true
        })
        .map(|(entity, _)| entity)
        .collect()
}

/// Execute provider handlers only after snapshotting programs and event
/// markers. No ECS query or resource guard survives the host callback.
pub fn papyrus_provider_system(world: &World, dt: f32) {
    let runtime = world
        .try_resource::<PapyrusProviderRuntime>()
        .and_then(|runtime| {
            runtime.callback().map(|callback| {
                (
                    runtime.catalog(),
                    callback,
                    runtime.entity_resolver(),
                    runtime.form_resolver(),
                    runtime.mod_event_publisher(),
                )
            })
        });
    let Some((catalog, callback, entity_resolver, form_resolver, mod_event_publisher)) = runtime
    else {
        return;
    };
    let pending = {
        let mut queue = world.resource_mut::<PapyrusProviderContinuationQueue>();
        std::mem::take(&mut queue.pending)
    };
    // #6018 — rows whose registrant is gone (cell unloaded, save reloaded)
    // can never match again; drop them before they are walked below.
    if world
        .try_resource::<PapyrusModEventRuntime>()
        .is_some_and(|runtime| !runtime.pending.is_empty())
    {
        prune_stale_mod_event_registrations(world);
    }
    // #5640 — the registration map is only read to deliver pending events,
    // so it is neither cloned wholesale nor walked per program entity: the
    // relevant registrations (those naming a pending event) are grouped by
    // owning entity once, under the lock, and only when something is
    // pending. A frame with no ModEvents costs nothing here, however many
    // registrations a mod has filled the map with.
    let (registrations_by_entity, pending_mod_events) = {
        let mut runtime = world.resource_mut::<PapyrusModEventRuntime>();
        let mut pending = std::mem::take(&mut runtime.pending);
        let mut by_entity: BTreeMap<EntityId, Vec<(PrincipalId, byroredux_sdk::identity::EventId, String)>> =
            BTreeMap::new();
        if !pending.is_empty() {
            // #6019 — deliver only as many events as the per-frame budget
            // covers; the rest wait, in order, ahead of anything queued later.
            let take = mod_event_batch_len(&pending, &runtime.registrations);
            runtime.pending = pending.split_off(take);
            let pending_events: BTreeSet<&byroredux_sdk::identity::EventId> =
                pending.iter().map(|event| &event.event).collect();
            // BTreeMap order (entity, principal, event) is kept within each
            // entity, so delivery order is unchanged.
            for ((entity, principal, event), callback_name) in &runtime.registrations {
                if pending_events.contains(event) {
                    by_entity.entry(*entity).or_default().push((
                        principal.clone(),
                        event.clone(),
                        callback_name.clone(),
                    ));
                }
            }
        }
        (by_entity, pending)
    };
    let pending_mod_events = pending_mod_events
        .into_iter()
        .filter_map(|event| {
            LegacySkseVariadicModEventPayload::decode(&event.payload)
                .map(|payload| (event, payload))
        })
        .collect::<Vec<_>>();
    let mut still_pending = Vec::new();
    let mut handlers = Vec::new();
    for mut continuation in pending {
        if !continuation.remaining_seconds.is_finite()
            || continuation.remaining_seconds < 0.0
            || continuation.remaining_seconds > MAX_PROVIDER_WAIT_SECONDS
        {
            log::warn!("Papyrus provider continuation dropped: invalid remaining wait");
            continue;
        }
        continuation.remaining_seconds -= dt.max(0.0);
        if continuation.remaining_seconds > 0.0 {
            still_pending.push(continuation);
        } else {
            handlers.push((
                continuation.statements,
                continuation.locals,
                Vec::new(),
                Vec::new(),
                continuation.principal,
                None,
            ));
        }
    }

    let initialized = world
        .query::<OnInitEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let loaded = world
        .query::<OnCellLoadEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    // #5994 — a native-only activation sends no OnActivate to any script.
    let activated: BTreeMap<EntityId, EntityId> =
        crate::papyrus_activations(world).into_iter().collect();
    let hits = world
        .query::<HitEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, *event))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let equipment_changes = world
        .query::<EquipmentEventBatch>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, batch)| {
                    (
                        entity,
                        batch
                            .0
                            .iter()
                            .map(|change| (change.item_form_id, change.equipped))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let trigger_entries = world
        .query::<OnTriggerEnterEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, event.triggerers.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let updated = world
        .query::<OnUpdateEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let initialized = first_initializations(world, initialized);
    let Some(programs) = world.query::<PapyrusProviderProgram>() else {
        return;
    };
    for (entity, program) in programs.iter() {
        let mut enqueue = |event, projected_entity, hit: Option<&HitEvent>, form| {
            for handler in program.handlers_for(event) {
                let projected = handler.projected_locals(projected_entity, hit, form);
                handlers.push((
                    handler.statements.clone(),
                    projected.values,
                    projected.entities,
                    projected.forms,
                    handler.principal.clone(),
                    Some(entity),
                ));
            }
        };
        if initialized.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnInit, None, None, None);
        }
        if loaded.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnLoad, None, None, None);
        }
        if let Some(activator) = activated.get(&entity) {
            enqueue(
                PapyrusProviderEvent::OnActivate,
                Some(*activator),
                None,
                None,
            );
        }
        if let Some(hit) = hits.get(&entity) {
            enqueue(
                PapyrusProviderEvent::OnHit,
                Some(hit.aggressor),
                Some(hit),
                None,
            );
        }
        if let Some(triggerers) = trigger_entries.get(&entity) {
            for triggerer in triggerers {
                enqueue(
                    PapyrusProviderEvent::OnTriggerEnter,
                    Some(*triggerer),
                    None,
                    None,
                );
            }
        }
        if let Some(changes) = equipment_changes.get(&entity) {
            for (form_id, equipped) in changes {
                let event = if *equipped {
                    PapyrusProviderEvent::OnObjectEquipped
                } else {
                    PapyrusProviderEvent::OnObjectUnequipped
                };
                enqueue(event, None, None, Some(*form_id));
            }
        }
        if updated.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnUpdate, None, None, None);
        }
        let entity_registrations = registrations_by_entity
            .get(&entity)
            .map_or(&[][..], Vec::as_slice);
        for (event, payload) in &pending_mod_events {
            for (principal, registered_event, callback_name) in entity_registrations {
                if registered_event != &event.event {
                    continue;
                }
                let Some(custom_handlers) = program.custom_handlers.get(callback_name) else {
                    continue;
                };
                for handler in custom_handlers {
                    if handler.principal.as_ref() != Some(principal) {
                        continue;
                    }
                    let Some(locals) = handler.projected_mod_event_locals(payload) else {
                        log::warn!(
                            "Papyrus ModEvent callback {callback_name} rejected a mismatched typed payload"
                        );
                        continue;
                    };
                    handlers.push((
                        handler.statements.clone(),
                        locals,
                        Vec::new(),
                        Vec::new(),
                        handler.principal.clone(),
                        Some(entity),
                    ));
                }
            }
        }
    }
    drop(programs);

    for (mut statements, mut locals, entity_locals, form_locals, principal, owner) in handlers {
        if statements_reference_local(&statements, PAPYRUS_SELF_LOCAL) {
            let Some(owner) = owner else {
                log::warn!("Papyrus provider handler aborted: self receiver has no owner");
                continue;
            };
            let Some(resolver) = entity_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: entity resolver is unavailable");
                continue;
            };
            match resolver(owner) {
                Ok(entity) => {
                    locals.insert(PAPYRUS_SELF_LOCAL.to_owned(), ScriptValue::Entity(entity));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    continue;
                }
            }
        }
        if statements_need_owner_sender(&statements) {
            let Some(owner_form_id) = owner.and_then(|owner| crate::condition::entity_global_form_id(world, owner)) else {
                log::warn!("Papyrus SendModEvent aborted: script owner has no stable FormID");
                continue;
            };
            let Some(resolver) = form_resolver.as_ref() else {
                log::warn!("Papyrus SendModEvent aborted: form resolver is unavailable");
                continue;
            };
            let owner_form = match resolver(owner_form_id) {
                Ok(form) => form,
                Err(error) => {
                    log::warn!("Papyrus SendModEvent aborted: {error}");
                    continue;
                }
            };
            resolve_mod_event_senders(&mut statements, Some(owner_form));
        }
        if let Err(error) = validate_provider_statements(&statements, catalog.as_ref(), 0) {
            log::warn!("Papyrus provider handler aborted before dispatch: {error}");
            continue;
        }
        let mut projection_failed = false;
        for (name, entity) in entity_locals {
            let Some(resolver) = entity_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: entity resolver is unavailable");
                projection_failed = true;
                break;
            };
            match resolver(entity) {
                Ok(entity) => {
                    locals.insert(name, ScriptValue::Entity(entity));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    projection_failed = true;
                    break;
                }
            }
        }
        if projection_failed {
            continue;
        }
        for (name, form_id) in form_locals {
            let Some(resolver) = form_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: form resolver is unavailable");
                projection_failed = true;
                break;
            };
            match resolver(form_id) {
                Ok(form) => {
                    locals.insert(name, ScriptValue::Form(form));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    projection_failed = true;
                    break;
                }
            }
        }
        if projection_failed {
            continue;
        }
        let mut registrations = Vec::new();
        match execute_statements(
            &statements,
            callback.as_ref(),
            mod_event_publisher.as_deref(),
            principal.as_ref(),
            &mut locals,
            &mut registrations,
        ) {
            Ok(Some((remaining_seconds, statements))) => {
                apply_mod_event_registrations(world, owner, principal.as_ref(), registrations);
                still_pending.push(PendingPapyrusProviderContinuation {
                    remaining_seconds,
                    statements,
                    locals,
                    principal,
                });
            }
            Ok(None) => {
                apply_mod_event_registrations(world, owner, principal.as_ref(), registrations)
            }
            Err(error) => {
                log::warn!("Papyrus provider handler aborted: {error}");
                // #6023 — the statements before the failing one already ran
                // (their provider calls had effect), and so did any
                // `RegisterForModEvent` among them; Papyrus keeps what ran
                // before a failed statement, so the collected registrations
                // are applied rather than discarded with the error.
                apply_mod_event_registrations(world, owner, principal.as_ref(), registrations);
            }
        }
    }
    if still_pending.len() > MAX_PROVIDER_CONTINUATIONS {
        log::warn!(
            "Papyrus provider continuation queue exceeded {MAX_PROVIDER_CONTINUATIONS}; dropping newest tails"
        );
        still_pending.truncate(MAX_PROVIDER_CONTINUATIONS);
    }
    world
        .resource_mut::<PapyrusProviderContinuationQueue>()
        .pending = still_pending;
}

pub(crate) fn apply_mod_event_registrations(
    world: &World,
    owner: Option<EntityId>,
    principal: Option<&PrincipalId>,
    actions: Vec<PapyrusModEventRegistrationAction>,
) {
    if actions.is_empty() {
        return;
    }
    let (Some(owner), Some(principal)) = (owner, principal) else {
        log::warn!("Papyrus ModEvent registration ignored without an owned script instance");
        return;
    };
    // #6018 — a full table is mostly dead rows after enough streaming; free
    // them before the cap starts refusing live registrations. Done before the
    // resource guard below: the prune takes its own.
    if actions
        .iter()
        .any(|action| matches!(action, PapyrusModEventRegistrationAction::Register { .. }))
        && world
            .try_resource::<PapyrusModEventRuntime>()
            .is_some_and(|runtime| {
                runtime.registrations.len() >= MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS
            })
    {
        prune_stale_mod_event_registrations(world);
    }
    let mut runtime = world.resource_mut::<PapyrusModEventRuntime>();
    for action in actions {
        match action {
            PapyrusModEventRegistrationAction::Register {
                event_name,
                callback,
            } => {
                let Some(LegacyModEventSubscriptionCommand::Subscribe { event, .. }) =
                    LegacyModEventSubscriptionCommand::subscribe(&event_name, callback.clone())
                else {
                    continue;
                };
                let key = (owner, principal.clone(), event);
                if runtime.registrations.contains_key(&key)
                    || runtime.registrations.len() < MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS
                {
                    runtime.registrations.insert(key, callback);
                } else {
                    log::warn!(
                        "Papyrus ModEvent registration limit of {MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS} exceeded"
                    );
                }
            }
            PapyrusModEventRegistrationAction::Unregister { event_name } => {
                let Some(LegacyModEventSubscriptionCommand::Unsubscribe { event }) =
                    LegacyModEventSubscriptionCommand::unsubscribe(&event_name)
                else {
                    continue;
                };
                runtime
                    .registrations
                    .remove(&(owner, principal.clone(), event));
            }
            PapyrusModEventRegistrationAction::UnregisterAll => {
                runtime
                    .registrations
                    .retain(|(entity, owner_principal, _), _| {
                        *entity != owner || owner_principal != principal
                    });
            }
        }
    }
}

/// How many of the leading `pending` events fit the per-frame delivery budget
/// (#6019). An event costs one delivery per registration naming it; events no
/// one registered for are free. At least one event is always taken, so a
/// single event with more registrations than the budget still goes out whole
/// rather than wedging the queue.
pub(crate) fn mod_event_batch_len(
    pending: &[CustomEvent],
    registrations: &BTreeMap<(EntityId, PrincipalId, byroredux_sdk::identity::EventId), String>,
) -> usize {
    let mut cost: BTreeMap<&byroredux_sdk::identity::EventId, usize> =
        pending.iter().map(|event| (&event.event, 0)).collect();
    for (_, _, event) in registrations.keys() {
        if let Some(count) = cost.get_mut(event) {
            *count += 1;
        }
    }
    let mut spent = 0usize;
    let mut take = 0usize;
    for event in pending {
        let event_cost = cost.get(&event.event).copied().unwrap_or(0);
        if take > 0 && spent + event_cost > MAX_PAPYRUS_MOD_EVENT_DELIVERIES_PER_FRAME {
            break;
        }
        spent += event_cost;
        take += 1;
    }
    take
}

/// Drop ModEvent registrations whose registrant entity no longer carries a
/// Papyrus provider program (#6018). Entity ids are never reclaimed, so such
/// a row (a despawned / cell-unloaded reference, or a registrant from before a
/// save load) can never match again — yet it counts against
/// [`MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS`] and is walked on every delivery.
/// Returns how many rows were dropped.
pub(crate) fn prune_stale_mod_event_registrations(world: &World) -> usize {
    // Snapshot the registrants, test liveness with no registration guard held
    // (the program query is a different lock), then retain under a fresh one.
    let registrants: BTreeSet<EntityId> = match world.try_resource::<PapyrusModEventRuntime>() {
        Some(runtime) => runtime
            .registrations
            .keys()
            .map(|(entity, _, _)| *entity)
            .collect(),
        None => return 0,
    };
    if registrants.is_empty() {
        return 0;
    }
    let live: BTreeSet<EntityId> = match world.query::<PapyrusProviderProgram>() {
        Some(programs) => registrants
            .into_iter()
            .filter(|entity| programs.get(*entity).is_some())
            .collect(),
        None => BTreeSet::new(),
    };
    let Some(mut runtime) = world.try_resource_mut::<PapyrusModEventRuntime>() else {
        return 0;
    };
    let before = runtime.registrations.len();
    runtime
        .registrations
        .retain(|(entity, _, _), _| live.contains(entity));
    before - runtime.registrations.len()
}

pub(crate) fn validate_provider_statements(
    statements: &[PapyrusProviderStatement],
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider continuation nesting exceeds the runtime bound".to_owned());
    }
    for statement in statements {
        match statement {
            PapyrusProviderStatement::Declare { .. }
            | PapyrusProviderStatement::UnregisterAllModEvents => {}
            PapyrusProviderStatement::AssignValue {
                value, value_type, ..
            } => {
                if !matches!(
                    value_type,
                    ScriptValueType::Boolean
                        | ScriptValueType::Integer
                        | ScriptValueType::Float
                        | ScriptValueType::String
                ) {
                    return Err("saved provider expression has a non-scalar result".to_owned());
                }
                validate_provider_value(value, catalog)?;
            }
            PapyrusProviderStatement::RegisterModEvent {
                event_name,
                callback,
            } => {
                LegacyModEventSubscriptionCommand::subscribe(event_name, callback.clone())
                    .ok_or_else(|| "saved ModEvent registration is invalid".to_owned())?;
            }
            PapyrusProviderStatement::UnregisterModEvent { event_name } => {
                LegacyModEventSubscriptionCommand::unsubscribe(event_name)
                    .ok_or_else(|| "saved ModEvent unregistration is invalid".to_owned())?;
            }
            PapyrusProviderStatement::SendModEvent {
                event_name,
                string_arg,
                number_arg,
                sender: _,
            } => {
                validate_mod_event_send_argument(event_name, ScriptValueType::String)?;
                validate_mod_event_send_argument(string_arg, ScriptValueType::String)?;
                validate_mod_event_send_argument(number_arg, ScriptValueType::Float)?;
            }
            PapyrusProviderStatement::AssignCall { call, .. }
            | PapyrusProviderStatement::ArrayWritebackCall { call, .. }
            | PapyrusProviderStatement::Call(call) => validate_provider_call(call, catalog, depth)?,
            PapyrusProviderStatement::Wait { seconds } => {
                // #6025 — the lowering bound (#5657) is re-checked here: a
                // continuation saved before it, or hand-edited, could carry
                // an effectively-infinite wait that holds a queue slot for
                // the rest of the session.
                if !seconds.is_finite() || *seconds < 0.0 || *seconds > MAX_PROVIDER_WAIT_SECONDS {
                    return Err("saved provider continuation contains an invalid wait".to_owned());
                }
            }
            PapyrusProviderStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                validate_provider_condition(condition, catalog, depth + 1)?;
                validate_provider_statements(then_branch, catalog, depth + 1)?;
                validate_provider_statements(else_branch, catalog, depth + 1)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_mod_event_send_argument(
    argument: &PapyrusProviderArgument,
    expected: ScriptValueType,
) -> Result<(), String> {
    match argument {
        PapyrusProviderArgument::Literal(value) if value.matches(expected, false) => Ok(()),
        PapyrusProviderArgument::Local { name, value_type }
            if !name.is_empty()
                && *name == name.to_ascii_lowercase()
                && *value_type == expected =>
        {
            Ok(())
        }
        _ => Err("saved SendModEvent argument is invalid".to_owned()),
    }
}

/// `depth` counts value / receiver nesting from the handler body, so a
/// `a.f().f().f()…` receiver chain is bounded by
/// [`MAX_PROVIDER_HANDLER_NESTING`] here rather than only by the front-ends'
/// expression-depth caps (#6025).
pub(crate) fn validate_provider_call(
    call: &PapyrusProviderInvocation,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider call nesting exceeds the runtime bound".to_owned());
    }
    let alias = call
        .route
        .declaration
        .papyrus
        .as_ref()
        .ok_or_else(|| "saved provider route has no Papyrus alias".to_owned())?;
    let live = catalog
        .resolve(&alias.provider, &alias.function)
        .ok_or_else(|| "saved provider route is no longer published".to_owned())?;
    let saved = call.route.declaration();
    let current = live.declaration();
    if live.qualified_name() != call.route.qualified_name()
        || saved.id != current.id
        || saved.component != current.component
        || saved.parameters != current.parameters
        || saved.result != current.result
        || saved.papyrus != current.papyrus
        || call.result != current.result
        || call
            .result_object_type
            .as_ref()
            .is_some_and(|saved| Some(saved) != provider_result_object_type(live).as_ref())
    {
        return Err("saved provider route does not match the live catalog".to_owned());
    }
    let parameter_offset = if let Some(receiver) = &call.receiver {
        let parameter = current
            .parameters
            .first()
            .filter(|parameter| {
                parameter.value_type == ScriptValueType::Entity && !parameter.optional
            })
            .ok_or_else(|| "saved provider route has no required Entity receiver".to_owned())?;
        match &**receiver {
            PapyrusProviderArgument::Local { name, value_type }
                if !name.is_empty()
                    && *name == name.to_ascii_lowercase()
                    && *value_type == parameter.value_type => {}
            PapyrusProviderArgument::Value { value, value_type }
                if *value_type == parameter.value_type =>
            {
                validate_provider_value_at_depth(value, catalog, depth + 1)?;
            }
            _ => return Err("saved provider receiver is invalid".to_owned()),
        }
        1
    } else {
        0
    };
    call.arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            let parameter = current
                .parameters
                .get(index + parameter_offset)
                .ok_or_else(|| "saved provider call has too many arguments".to_owned())?;
            match argument {
                PapyrusProviderArgument::Literal(value) => {
                    if !value.matches(parameter.value_type, parameter.optional) {
                        return Err("saved provider literal argument changed type".to_owned());
                    }
                    Ok(())
                }
                PapyrusProviderArgument::Local { name, value_type } => {
                    if name.is_empty()
                        || *name != name.to_ascii_lowercase()
                        || *value_type != parameter.value_type
                    {
                        return Err("saved provider local argument is invalid".to_owned());
                    }
                    Ok(())
                }
                PapyrusProviderArgument::Value { value, value_type } => {
                    if *value_type != parameter.value_type {
                        return Err("saved provider computed argument changed type".to_owned());
                    }
                    validate_provider_value_at_depth(value, catalog, depth + 1)
                        .map_err(|_| "saved provider computed argument is invalid".to_owned())
                }
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    if current
        .parameters
        .iter()
        .skip(parameter_offset + call.arguments.len())
        .any(|parameter| !parameter.optional)
    {
        return Err("saved provider call omits a required argument".to_owned());
    }
    validate_storage_util_arguments(call.route.qualified_name(), &call.arguments)
        .map_err(|_| "saved StorageUtil call has an invalid exact signature".to_owned())?;
    validate_legacy_container_arity(call.route.qualified_name(), call.arguments.len())
        .map_err(|_| "saved JContainers call has an invalid exact signature".to_owned())?;
    validate_mod_event_arity(call.route.qualified_name(), call.arguments.len())
        .map_err(|_| "saved ModEvent call has an invalid exact signature".to_owned())?;
    Ok(())
}

pub(crate) fn materialize_provider_arguments(
    call: &PapyrusProviderInvocation,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
    depth: usize,
) -> Result<Vec<ScriptValue>, String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("provider argument nesting exceeds the runtime bound".to_owned());
    }
    let parameter_offset = if call.receiver.is_some() { 1 } else { 0 };
    let mut arguments = Vec::with_capacity(call.arguments.len() + parameter_offset);
    if let Some(receiver) = &call.receiver {
        let parameter = call
            .route
            .declaration()
            .parameters
            .first()
            .filter(|parameter| {
                parameter.value_type == ScriptValueType::Entity && !parameter.optional
            })
            .ok_or_else(|| "provider receiver declaration is invalid".to_owned())?;
        let value = match receiver.as_ref() {
            PapyrusProviderArgument::Local { name, value_type } => {
                if name.is_empty()
                    || *name != name.to_ascii_lowercase()
                    || *value_type != parameter.value_type
                {
                    return Err("provider receiver local changed type".to_owned());
                }
                locals
                    .get(name)
                    .cloned()
                    .ok_or_else(|| "translated provider receiver was not initialized".to_owned())?
            }
            PapyrusProviderArgument::Value { value, value_type } => {
                if *value_type != parameter.value_type {
                    return Err("provider receiver expression changed type".to_owned());
                }
                evaluate_provider_value(value, callback, principal, locals, depth + 1)?
            }
            PapyrusProviderArgument::Literal(_) => {
                return Err("provider receiver must be a local or expression".to_owned());
            }
        };
        if !value.matches(parameter.value_type, parameter.optional) {
            return Err("translated provider receiver changed type at execution".to_owned());
        }
        arguments.push(value);
    }
    for (index, argument) in call.arguments.iter().enumerate() {
        let parameter = call
            .route
            .declaration()
            .parameters
            .get(index + parameter_offset)
            .ok_or_else(|| "provider call has too many arguments".to_owned())?;
        let value = match argument {
            PapyrusProviderArgument::Literal(value) => value.clone(),
            PapyrusProviderArgument::Local { name, value_type } => {
                if *value_type != parameter.value_type {
                    return Err("provider local argument declaration changed type".to_owned());
                }
                locals
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("translated local {name} was not initialized"))?
            }
            PapyrusProviderArgument::Value { value, value_type } => {
                if *value_type != parameter.value_type {
                    return Err("provider computed argument declaration changed type".to_owned());
                }
                evaluate_provider_value(value, callback, principal, locals, depth + 1)?
            }
        };
        if !value.matches(parameter.value_type, parameter.optional) {
            return Err(format!(
                "translated argument {} changed type at execution",
                parameter.id.as_str()
            ));
        }
        arguments.push(value);
    }
    call.route
        .declaration()
        .validate_arguments(&arguments)
        .map_err(|error| format!("provider arguments are invalid at execution: {error:?}"))?;
    Ok(arguments)
}

pub(crate) fn validate_provider_condition(
    condition: &PapyrusProviderCondition,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider condition nesting exceeds the runtime bound".to_owned());
    }
    match condition {
        PapyrusProviderCondition::Literal(_) | PapyrusProviderCondition::Local(_) => Ok(()),
        PapyrusProviderCondition::Call(call) => validate_provider_call(call, catalog, depth),
        PapyrusProviderCondition::Not(condition) => {
            validate_provider_condition(condition, catalog, depth + 1)
        }
        PapyrusProviderCondition::And(left, right) | PapyrusProviderCondition::Or(left, right) => {
            validate_provider_condition(left, catalog, depth + 1)?;
            validate_provider_condition(right, catalog, depth + 1)
        }
        PapyrusProviderCondition::Compare { left, right, .. } => {
            validate_provider_value_at_depth(left, catalog, depth + 1)?;
            validate_provider_value_at_depth(right, catalog, depth + 1)
        }
    }
}

pub(crate) fn validate_provider_value(
    value: &PapyrusProviderValue,
    catalog: &PapyrusProviderCatalog,
) -> Result<(), String> {
    validate_provider_value_at_depth(value, catalog, 0)
}

pub(crate) fn validate_provider_value_at_depth(
    value: &PapyrusProviderValue,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider value nesting exceeds the runtime bound".to_owned());
    }
    match value {
        PapyrusProviderValue::Call(call) => validate_provider_call(call, catalog, depth),
        PapyrusProviderValue::Binary { left, right, .. } => {
            validate_provider_value_at_depth(left, catalog, depth + 1)?;
            validate_provider_value_at_depth(right, catalog, depth + 1)
        }
        PapyrusProviderValue::Literal(_) | PapyrusProviderValue::Local(_) => Ok(()),
    }
}

pub(crate) fn execute_statements(
    statements: &[PapyrusProviderStatement],
    callback: &PapyrusProviderCallback,
    mod_event_publisher: Option<&PapyrusProviderModEventPublisher>,
    principal: Option<&PrincipalId>,
    locals: &mut BTreeMap<String, ScriptValue>,
    registrations: &mut Vec<PapyrusModEventRegistrationAction>,
) -> Result<Option<(f32, Vec<PapyrusProviderStatement>)>, String> {
    // Explicit cursor stack (#5635, the #3935 shape `apply_effects` already
    // uses): `pending` holds the slices still to run, innermost on top. An
    // executed `If` pushes the rest of the current slice and then the
    // chosen branch — borrowed, never cloned — so N sequential `If`s cost
    // O(1) stack and no tail copies. It used to rebuild `branch ++ tail`
    // and recurse per executed `If`: N `If`s meant N stack frames and
    // O(N^2) live statement clones, and `MAX_PROVIDER_HANDLER_NESTING`
    // only bounds nesting, not the sequential count.
    let mut pending: Vec<&[PapyrusProviderStatement]> = vec![statements];
    while let Some(slice) = pending.pop() {
        let Some((statement, rest)) = slice.split_first() else {
            continue;
        };
        if !rest.is_empty() {
            pending.push(rest);
        }
        match statement {
            PapyrusProviderStatement::Declare { name, value } => {
                locals.insert(name.clone(), value.clone());
            }
            PapyrusProviderStatement::AssignCall { name, call } => {
                let arguments =
                    materialize_provider_arguments(call, callback, principal, locals, 0)?;
                let value = callback(principal, call.route.qualified_name(), &arguments)?;
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::AssignValue {
                name,
                value,
                value_type,
            } => {
                let value = evaluate_provider_value(value, callback, principal, locals, 0)?;
                if !value.matches(*value_type, false) {
                    return Err(format!(
                        "provider expression assigned an invalid {value_type:?} value"
                    ));
                }
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::ArrayWritebackCall { name, call } => {
                let arguments =
                    materialize_provider_arguments(call, callback, principal, locals, 0)?;
                let value = callback(principal, call.route.qualified_name(), &arguments)?;
                let expected = call
                    .route
                    .declaration()
                    .parameters
                    .get(2)
                    .map(|parameter| parameter.value_type)
                    .ok_or_else(|| "StorageUtil ListSlice array parameter is missing".to_owned())?;
                if !value.matches(expected, false) {
                    return Err(
                        "StorageUtil ListSlice callback returned an invalid array type".to_owned(),
                    );
                }
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::Call(call) => {
                let arguments =
                    materialize_provider_arguments(call, callback, principal, locals, 0)?;
                callback(principal, call.route.qualified_name(), &arguments)?;
            }
            PapyrusProviderStatement::RegisterModEvent {
                event_name,
                callback,
            } => registrations.push(PapyrusModEventRegistrationAction::Register {
                event_name: event_name.clone(),
                callback: callback.clone(),
            }),
            PapyrusProviderStatement::UnregisterModEvent { event_name } => {
                registrations.push(PapyrusModEventRegistrationAction::Unregister {
                    event_name: event_name.clone(),
                });
            }
            PapyrusProviderStatement::UnregisterAllModEvents => {
                registrations.push(PapyrusModEventRegistrationAction::UnregisterAll);
            }
            PapyrusProviderStatement::SendModEvent {
                event_name,
                string_arg,
                number_arg,
                sender,
            } => {
                let Some(principal) = principal else {
                    return Err(
                        "SendModEvent has no authenticated legacy-script principal".to_owned()
                    );
                };
                let Some(publisher) = mod_event_publisher else {
                    return Err("SendModEvent publisher is unavailable".to_owned());
                };
                let ScriptValue::String(event_name) =
                    materialize_mod_event_argument(event_name, ScriptValueType::String, locals)?
                else {
                    unreachable!("validated SendModEvent event name type")
                };
                let ScriptValue::String(string_arg) =
                    materialize_mod_event_argument(string_arg, ScriptValueType::String, locals)?
                else {
                    unreachable!("validated SendModEvent string argument type")
                };
                let ScriptValue::Float(number_arg) =
                    materialize_mod_event_argument(number_arg, ScriptValueType::Float, locals)?
                else {
                    unreachable!("validated SendModEvent number argument type")
                };
                let PapyrusModEventSender::Resolved(sender) = sender else {
                    return Err("SendModEvent sender was not resolved before execution".to_owned());
                };
                let command =
                    adapt_legacy_send_mod_event(&event_name, string_arg, number_arg, *sender)
                        .map_err(|error| {
                            format!("SendModEvent arguments are invalid: {error:?}")
                        })?;
                publisher(principal, command)?;
            }
            PapyrusProviderStatement::Wait { seconds } => {
                // The one place an owned program is built: everything still
                // pending, in execution order (top of the stack first).
                let mut tail = Vec::with_capacity(pending.iter().map(|slice| slice.len()).sum());
                for slice in pending.iter().rev() {
                    tail.extend_from_slice(slice);
                }
                return Ok(Some((*seconds, tail)));
            }
            PapyrusProviderStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let selected = if evaluate_condition(condition, callback, principal, locals)? {
                    then_branch
                } else {
                    else_branch
                };
                if !selected.is_empty() {
                    pending.push(selected);
                }
            }
        }
    }
    Ok(None)
}

pub(crate) fn materialize_mod_event_argument(
    argument: &PapyrusProviderArgument,
    expected: ScriptValueType,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<ScriptValue, String> {
    let value = match argument {
        PapyrusProviderArgument::Literal(value) => value.clone(),
        PapyrusProviderArgument::Local { name, value_type } => {
            if *value_type != expected {
                return Err("SendModEvent local declaration changed type".to_owned());
            }
            locals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("translated local {name} was not initialized"))?
        }
        PapyrusProviderArgument::Value { .. } => {
            return Err("SendModEvent computed arguments are unsupported".to_owned());
        }
    };
    value
        .matches(expected, false)
        .then_some(value)
        .ok_or_else(|| "SendModEvent argument changed type at execution".to_owned())
}

pub(crate) fn evaluate_condition(
    condition: &PapyrusProviderCondition,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<bool, String> {
    match condition {
        PapyrusProviderCondition::Not(condition) => {
            return Ok(!evaluate_condition(condition, callback, principal, locals)?);
        }
        PapyrusProviderCondition::And(left, right) => {
            return Ok(evaluate_condition(left, callback, principal, locals)?
                && evaluate_condition(right, callback, principal, locals)?);
        }
        PapyrusProviderCondition::Or(left, right) => {
            return Ok(evaluate_condition(left, callback, principal, locals)?
                || evaluate_condition(right, callback, principal, locals)?);
        }
        PapyrusProviderCondition::Compare {
            left,
            operator,
            right,
        } => {
            let left = evaluate_condition_value(left, callback, principal, locals)?;
            let right = evaluate_condition_value(right, callback, principal, locals)?;
            return compare_condition_values(&left, *operator, &right);
        }
        _ => {}
    }
    let value = match condition {
        PapyrusProviderCondition::Literal(value) => return Ok(*value),
        PapyrusProviderCondition::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized"))?,
        PapyrusProviderCondition::Call(call) => {
            let arguments = materialize_provider_arguments(call, callback, principal, locals, 0)?;
            callback(principal, call.route.qualified_name(), &arguments)?
        }
        PapyrusProviderCondition::Not(_)
        | PapyrusProviderCondition::And(_, _)
        | PapyrusProviderCondition::Or(_, _)
        | PapyrusProviderCondition::Compare { .. } => unreachable!("handled above"),
    };
    match value {
        ScriptValue::Boolean(value) => Ok(value),
        _ => Err("provider returned a non-boolean condition result".to_owned()),
    }
}

pub(crate) fn evaluate_condition_value(
    value: &PapyrusProviderValue,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<ScriptValue, String> {
    evaluate_provider_value(value, callback, principal, locals, 0)
}

pub(crate) fn evaluate_provider_value(
    value: &PapyrusProviderValue,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
    depth: usize,
) -> Result<ScriptValue, String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("provider expression nesting exceeds the runtime bound".to_owned());
    }
    match value {
        PapyrusProviderValue::Literal(value) => Ok(value.clone()),
        PapyrusProviderValue::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized")),
        PapyrusProviderValue::Call(call) => {
            let arguments =
                materialize_provider_arguments(call, callback, principal, locals, depth)?;
            callback(principal, call.route.qualified_name(), &arguments)
        }
        PapyrusProviderValue::Binary {
            left,
            operator,
            right,
        } => {
            let left = evaluate_provider_value(left, callback, principal, locals, depth + 1)?;
            let right = evaluate_provider_value(right, callback, principal, locals, depth + 1)?;
            apply_provider_arithmetic(left, *operator, right)
        }
    }
}

pub(crate) fn apply_provider_arithmetic(
    left: ScriptValue,
    operator: PapyrusProviderArithmetic,
    right: ScriptValue,
) -> Result<ScriptValue, String> {
    match (left, operator, right) {
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Add,
            ScriptValue::Integer(right),
        ) => left
            .checked_add(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer addition overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Sub,
            ScriptValue::Integer(right),
        ) => left
            .checked_sub(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer subtraction overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Mul,
            ScriptValue::Integer(right),
        ) => left
            .checked_mul(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer multiplication overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Div,
            ScriptValue::Integer(right),
        ) => {
            if right == 0 {
                return Err("provider integer division by zero".to_owned());
            }
            left.checked_div(right)
                .map(ScriptValue::Integer)
                .ok_or_else(|| "provider integer division overflowed".to_owned())
        }
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Mod,
            ScriptValue::Integer(right),
        ) => {
            if right == 0 {
                return Err("provider integer remainder by zero".to_owned());
            }
            left.checked_rem(right)
                .map(ScriptValue::Integer)
                .ok_or_else(|| "provider integer remainder overflowed".to_owned())
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Add, ScriptValue::Float(right)) => {
            finite_float_result(left + right, "addition")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Sub, ScriptValue::Float(right)) => {
            finite_float_result(left - right, "subtraction")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Mul, ScriptValue::Float(right)) => {
            finite_float_result(left * right, "multiplication")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Div, ScriptValue::Float(right)) => {
            if right == 0.0 {
                return Err("provider float division by zero".to_owned());
            }
            finite_float_result(left / right, "division")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Mod, ScriptValue::Float(right)) => {
            if right == 0.0 {
                return Err("provider float remainder by zero".to_owned());
            }
            finite_float_result(left % right, "remainder")
        }
        (
            ScriptValue::String(left),
            PapyrusProviderArithmetic::StrCat,
            ScriptValue::String(right),
        ) => {
            let value = format!("{left}{right}");
            let result = ScriptValue::String(value);
            result
                .matches(ScriptValueType::String, false)
                .then_some(result)
                .ok_or_else(|| "provider string concatenation exceeded the script limit".to_owned())
        }
        _ => Err("provider expression operands changed type at execution".to_owned()),
    }
}

pub(crate) fn finite_float_result(value: f32, operation: &str) -> Result<ScriptValue, String> {
    value
        .is_finite()
        .then_some(ScriptValue::Float(value))
        .ok_or_else(|| format!("provider float {operation} produced a non-finite result"))
}

pub(crate) fn compare_condition_values(
    left: &ScriptValue,
    operator: PapyrusProviderComparison,
    right: &ScriptValue,
) -> Result<bool, String> {
    match (left, right) {
        (ScriptValue::Boolean(left), ScriptValue::Boolean(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered boolean provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Integer(left), ScriptValue::Integer(right)) => {
            Ok(compare_ordered(*left, operator, *right))
        }
        (ScriptValue::Float(left), ScriptValue::Float(right)) => {
            Ok(compare_ordered(*left, operator, *right))
        }
        (ScriptValue::String(left), ScriptValue::String(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered string provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Entity(left), ScriptValue::Entity(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered entity provider comparison reached execution".to_owned()),
        },
        (ScriptValue::None, ScriptValue::None) => match operator {
            PapyrusProviderComparison::Equal => Ok(true),
            PapyrusProviderComparison::NotEqual => Ok(false),
            _ => Err("ordered null entity provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Entity(_), ScriptValue::None)
        | (ScriptValue::None, ScriptValue::Entity(_)) => match operator {
            PapyrusProviderComparison::Equal => Ok(false),
            PapyrusProviderComparison::NotEqual => Ok(true),
            _ => Err("ordered nullable entity provider comparison reached execution".to_owned()),
        },
        _ => Err("provider comparison operands changed type at execution".to_owned()),
    }
}

pub(crate) fn compare_ordered<T: PartialOrd + PartialEq>(
    left: T,
    operator: PapyrusProviderComparison,
    right: T,
) -> bool {
    match operator {
        PapyrusProviderComparison::Equal => left == right,
        PapyrusProviderComparison::NotEqual => left != right,
        PapyrusProviderComparison::Less => left < right,
        PapyrusProviderComparison::LessOrEqual => left <= right,
        PapyrusProviderComparison::Greater => left > right,
        PapyrusProviderComparison::GreaterOrEqual => left >= right,
    }
}
