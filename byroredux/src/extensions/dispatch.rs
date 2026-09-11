//! Canonical event delivery: the typed script-function entry, the
//! engine-snapshot setters a frame installs before dispatching, and the
//! eight `dispatch_*` families with their projection/inner twins.
//!
//! Every path here hands the guest a view assembled *before* entry and
//! commits its returned command batch *after* every ECS guard is dropped
//! — see `commands::enter_guest` for the prologue that enforces it.

use super::*;

impl ExtensionHost {
    /// Invoke a principal-namespaced typed function and publish its result
    /// only after every deferred side effect commits atomically.
    pub(crate) fn invoke_script_function(
        &mut self,
        qualified_name: &str,
        arguments: &[ScriptValue],
    ) -> Result<ScriptValue, ExtensionHostError> {
        let route = self
            .script_functions
            .iter()
            .find(|route| route.name == qualified_name)
            .cloned()
            .ok_or_else(|| ExtensionHostError::UnknownScriptFunction(qualified_name.to_owned()))?;
        route
            .declaration
            .validate_arguments(arguments)
            .map_err(|error| ExtensionHostError::ScriptFunctionUnavailable {
                function: qualified_name.to_owned(),
                reason: error.to_string(),
            })?;
        let index = self
            .components
            .iter()
            .position(|hosted| {
                hosted.extension == route.extension && hosted.component == route.component
            })
            .ok_or_else(|| ExtensionHostError::ScriptFunctionUnavailable {
                function: qualified_name.to_owned(),
                reason: "declared component is unavailable".to_owned(),
            })?;
        let hosted = &mut self.components[index];
        if hosted.instance.status() != &InstanceStatus::Active {
            return Err(ExtensionHostError::ScriptFunctionUnavailable {
                function: qualified_name.to_owned(),
                reason: format!("component is {}", hosted.instance.status()),
            });
        }
        let principal = enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
        let result = hosted
            .instance
            .on_script_function(route.declaration_index, arguments);
        self.diagnostics
            .extend(hosted.instance.take_logs().into_iter().map(|entry| {
                ExtensionDiagnostic::Log {
                    extension: hosted.extension.clone(),
                    component: hosted.component.clone(),
                    entry,
                }
            }));
        let (value, commands) = result.map_err(|error| {
            self.diagnostics.push(ExtensionDiagnostic::Fault {
                extension: hosted.extension.clone(),
                component: hosted.component.clone(),
                message: error.to_string(),
            });
            ExtensionHostError::Sandbox(error)
        })?;
        let mut stats = ExtensionDispatchStats::default();
        apply_delivery_result(
            hosted,
            Ok(commands),
            LifecyclePhase::ScriptFunction,
            &principal,
            delivery_commit_context!(self, stats),
        );
        if stats.faults == 0 {
            Ok(value)
        } else {
            Err(ExtensionHostError::ScriptFunctionCommandBatchRejected(
                qualified_name.to_owned(),
            ))
        }
    }

    pub(super) fn set_content_catalog(&mut self, catalog: Arc<ContentCatalog>) {
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

    pub(super) fn set_input_bindings(&mut self, bindings: Vec<PapyrusInputBinding>) {
        if self.input_bindings != bindings {
            self.input_bindings = bindings;
        }
    }

    pub(super) fn set_ui_menu_snapshot(&mut self, snapshot: PapyrusUiMenuSnapshot) {
        if self.ui_menu_snapshot != snapshot {
            self.ui_menu_snapshot = snapshot;
        }
    }

    pub(super) fn set_player_entity(&mut self, entity: Option<EntityId>) {
        self.player_entity = entity;
    }

    pub(super) fn set_faction_relationships(
        &mut self,
        relationships: Arc<FactionRelationshipCatalog>,
    ) {
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

    pub(super) fn set_engine_settings(&mut self, settings: Arc<SettingsSnapshot>) {
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

    pub(super) fn set_spatial_snapshot(&mut self, snapshot: Arc<SpatialSnapshot>) {
        for hosted in &mut self.components {
            hosted.instance.set_spatial_snapshot(Arc::clone(&snapshot));
        }
    }

    pub(super) fn catalog(&self) -> &byroredux_sdk::service::ServiceCatalog {
        self.runtime.catalog()
    }

    pub(super) fn max_component_bytes(&self) -> usize {
        self.runtime.config().max_component_bytes
    }

    pub(super) fn bind_entity(
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

    pub(super) fn dispatch_activations_with_projections(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_cell_loads_with_projections(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_equipment_changes_with_projections(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_input_actions_inner(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_session_events_inner(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_pending_custom_events(&mut self) -> ExtensionDispatchStats {
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_hits_with_projections(
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
                let principal =
                    enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                    delivery_commit_context!(self, stats),
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

    pub(super) fn dispatch_recurring_updates(&mut self, dt: f32) -> ExtensionDispatchStats {
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
            let principal = enter_guest(hosted, &self.principal_storage, &self.legacy_containers);
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
                delivery_commit_context!(self, stats),
            );
        }
        stats
    }

    pub(super) fn record_host_fault(&mut self, message: String) {
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

    pub(super) fn active_principals(&self) -> BTreeSet<PrincipalId> {
        self.components
            .iter()
            .map(|component| PrincipalId::from(&component.extension))
            .chain(self.legacy_script_principals.iter().cloned())
            .collect()
    }

    pub(super) fn register_legacy_script_principal(
        &mut self,
        principal: PrincipalId,
    ) -> Result<(), ExtensionHostError> {
        if self.legacy_script_principals.contains(&principal) {
            return Ok(());
        }
        self.principal_storage
            .register_schema(principal.clone(), 1)?;
        self.legacy_containers.entry(principal.clone()).or_default();
        self.legacy_mod_event_builders
            .entry(principal.clone())
            .or_default();
        self.legacy_script_principals.insert(principal);
        Ok(())
    }
}
